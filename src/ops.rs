//! Operator commands: `gitter backup`, `gitter restore`, `gitter doctor`, and
//! the consistency check that runs every time the server boots.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use rusqlite::Connection;

use crate::Config;

const FORMAT: u32 = 1;

fn git(args: &[&str]) -> Command {
    let mut c = Command::new("git");
    c.env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args(args);
    c
}

fn run(mut c: Command, what: &str) -> Result<String> {
    let out = c.output().with_context(|| format!("couldn't run {what}"))?;
    if !out.status.success() {
        bail!(
            "{what} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Repository ids the database knows about.
fn repo_ids(db: &Connection) -> Result<Vec<i64>> {
    let mut s = db.prepare("SELECT id FROM repos ORDER BY id")?;
    let ids = s
        .query_map([], |r| r.get(0))?
        .collect::<Result<Vec<i64>, _>>()?;
    Ok(ids)
}

/// Write `<out>`: a .tar.gz holding a consistent copy of the database, every
/// repository and a manifest. Safe to run while the server is up.
pub fn backup(cfg: &Config, out: &Path) -> Result<()> {
    let db_path = cfg.data_dir.join("gitter.db");
    if !db_path.exists() {
        bail!(
            "no database at {} (is GITTER_DATA right?)",
            db_path.display()
        );
    }
    // The snapshot holds password and session hashes: keep it next to the data, owner-only.
    let tmp = tempdir_in(&cfg.data_dir)?;
    let snap = tmp.join("gitter.db");
    let conn = Connection::open(&db_path)?;
    conn.busy_timeout(std::time::Duration::from_secs(10))?;
    // VACUUM INTO writes a transactionally consistent snapshot without stopping writers.
    conn.execute("VACUUM INTO ?1", [snap.to_string_lossy()])?;
    let ids = repo_ids(&conn)?;
    let users: i64 = conn.query_row("SELECT count(*) FROM users", [], |r| r.get(0))?;
    drop(conn);
    let manifest = serde_json::json!({
        "format": FORMAT, "gitter": env!("CARGO_PKG_VERSION"), "created_at": crate::db::now(),
        "repos": ids.len(), "users": users,
    });
    std::fs::write(
        tmp.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    let out = absolute(out)?;
    // Created before the work starts, so a refusal to overwrite never deletes an older backup.
    let file = match owner_only_file(&out) {
        Ok(f) => f,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err(e);
        }
    };
    let res = (|| -> Result<()> {
        let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(
            file,
            flate2::Compression::default(),
        ));
        tar.follow_symlinks(false);
        tar.append_path_with_name(tmp.join("manifest.json"), "manifest.json")?;
        tar.append_path_with_name(&snap, "gitter.db")?;
        tar.append_dir_all("repos", cfg.data_dir.join("repos"))?;
        tar.into_inner()?.finish()?;
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&tmp);
    if res.is_err() {
        let _ = std::fs::remove_file(&out);
    }
    res?;
    let size = std::fs::metadata(&out)?.len();
    println!(
        "Backed up {users} accounts and {} repositories to {} ({:.1} MB).",
        ids.len(),
        out.display(),
        size as f64 / 1e6
    );
    Ok(())
}

/// Check every entry of a backup and read its manifest, without writing anything.
fn inspect_archive(archive: &Path) -> Result<serde_json::Value> {
    let f = std::fs::File::open(archive)
        .with_context(|| format!("couldn't open {}", archive.display()))?;
    let mut ar = tar::Archive::new(flate2::read::GzDecoder::new(f));
    let mut manifest = None;
    let mut has_db = false;
    for e in ar.entries()? {
        let mut e = e?;
        let path = e.path()?.into_owned();
        let name = path.to_string_lossy().into_owned();
        let known = name == "manifest.json"
            || name == "gitter.db"
            || name == "repos"
            || name.starts_with("repos/");
        // Only plain relative paths, no `..`, no absolute paths, no links that could point outside.
        let safe = path
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)));
        let kind = e.header().entry_type();
        if !known || !safe || kind.is_symlink() || kind.is_hard_link() {
            bail!(
                "{} doesn't look like a Gitter backup (unexpected entry {name:?})",
                archive.display()
            );
        }
        if name == "manifest.json" {
            let mut buf = Vec::new();
            std::io::Read::read_to_end(&mut e, &mut buf)?;
            manifest = Some(
                serde_json::from_slice::<serde_json::Value>(&buf)
                    .context("the backup manifest is unreadable")?,
            );
        }
        has_db |= name == "gitter.db";
    }
    let manifest = manifest.context("the backup has no manifest")?;
    if manifest["format"].as_u64() != Some(FORMAT as u64) {
        bail!("unsupported backup format {}", manifest["format"]);
    }
    if !has_db {
        bail!("the backup has no database");
    }
    Ok(manifest)
}

/// Restore a backup into GITTER_DATA. Nothing existing is touched until the
/// archive has been fully checked and unpacked into a staging folder.
pub fn restore(cfg: &Config, archive: &Path, force: bool) -> Result<()> {
    let archive = absolute(archive)?
        .canonicalize()
        .with_context(|| format!("can't find {}", archive.display()))?;
    std::fs::create_dir_all(&cfg.data_dir)?;
    let data = cfg.data_dir.canonicalize()?;
    let occupied = data.join("gitter.db").exists();
    if occupied && !force {
        bail!(
            "{} already has data. Restore into an empty directory, or pass --force to move the current data aside first.",
            data.display()
        );
    }
    let manifest = inspect_archive(&archive)?;

    let stage = tempdir_in(&data)?;
    let staged = (|| -> Result<()> {
        let f = std::fs::File::open(&archive)?;
        let mut ar = tar::Archive::new(flate2::read::GzDecoder::new(f));
        ar.set_preserve_permissions(true);
        for e in ar.entries()? {
            e?.unpack_in(&stage)?;
        }
        std::fs::create_dir_all(stage.join("repos"))?;
        let conn = Connection::open(stage.join("gitter.db"))?;
        let ok: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
        if ok != "ok" {
            bail!("the database in the backup is damaged: {ok}");
        }
        Ok(())
    })();
    if let Err(e) = staged {
        let _ = std::fs::remove_dir_all(&stage);
        return Err(e);
    }

    if occupied {
        // Move the current contents into a subfolder (the data dir itself may be a
        // mount point, e.g. a Docker volume, so it can't be renamed).
        let aside = data.join(format!("before-restore-{}", crate::db::now()));
        std::fs::create_dir_all(&aside)?;
        for e in std::fs::read_dir(&data)?.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            let keep = p == archive
                || p == stage
                || p == aside
                || name.starts_with("before-restore-")
                || name.starts_with(".backup-tmp-");
            if !keep {
                std::fs::rename(&p, aside.join(&name))
                    .with_context(|| format!("couldn't move {} aside", p.display()))?;
            }
        }
        println!("Moved the existing data to {}.", aside.display());
    }
    for name in ["gitter.db", "repos"] {
        std::fs::rename(stage.join(name), data.join(name))
            .with_context(|| format!("couldn't move the restored {name} into place"))?;
    }
    std::fs::remove_dir_all(&stage)?;
    std::fs::create_dir_all(data.join("home"))?;
    let problems = check(cfg, true)?;
    if problems > 0 {
        bail!("restored, but {problems} problem(s) were found (see above)");
    }
    println!(
        "Restored {} accounts and {} repositories from {}.",
        manifest["users"],
        manifest["repos"],
        archive.display()
    );
    Ok(())
}

/// Print a health report. Returns the number of problems.
pub fn check(cfg: &Config, fsck: bool) -> Result<usize> {
    let mut problems = 0;
    let mut report = |ok: bool, msg: String| {
        println!("{} {msg}", if ok { "✓" } else { "✗" });
        if !ok {
            problems += 1;
        }
    };
    match run(git(&["--version"]), "git") {
        Ok(v) => {
            let ver = v.trim().trim_start_matches("git version ").to_string();
            let mut nums = ver
                .split(|c: char| !c.is_ascii_digit())
                .filter_map(|n| n.parse::<u32>().ok());
            let (maj, min) = (nums.next().unwrap_or(0), nums.next().unwrap_or(0));
            report(
                (maj, min) >= (2, 38),
                format!("git {ver} (needs 2.38 or newer)"),
            );
        }
        Err(e) => report(false, format!("git is not installed: {e}")),
    }
    let data = &cfg.data_dir;
    let probe = data.join(".write-test");
    let writable = std::fs::write(&probe, b"ok").is_ok();
    let _ = std::fs::remove_file(&probe);
    report(
        writable,
        format!("data directory {} is writable", data.display()),
    );
    let db_path = data.join("gitter.db");
    if !db_path.exists() {
        report(
            false,
            format!(
                "no database at {} yet (the server creates it on first start)",
                db_path.display()
            ),
        );
        return Ok(problems);
    }
    let conn = Connection::open(&db_path)?;
    let integrity: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
    report(
        integrity == "ok",
        format!("database integrity: {integrity}"),
    );
    let ids = repo_ids(&conn)?;
    let known: HashSet<String> = ids.iter().map(|i| format!("{i}.git")).collect();
    let repos = data.join("repos");
    let mut missing = 0;
    let mut broken = Vec::new();
    for id in &ids {
        let p = repos.join(format!("{id}.git"));
        if !p.is_dir() {
            missing += 1;
            continue;
        }
        let gd = format!("--git-dir={}", p.display());
        let mut args = vec![gd.as_str(), "rev-parse", "--git-dir"];
        if fsck {
            args = vec![
                gd.as_str(),
                "fsck",
                "--connectivity-only",
                "--no-dangling",
                "--no-progress",
            ];
        }
        if run(git(&args), "git").is_err() {
            broken.push(*id);
        }
    }
    report(
        missing == 0,
        format!(
            "{} repositories on disk for {} in the database{}",
            ids.len() - missing,
            ids.len(),
            if missing > 0 {
                format!(" ({missing} missing)")
            } else {
                String::new()
            }
        ),
    );
    report(
        broken.is_empty(),
        if broken.is_empty() {
            format!(
                "repositories {}",
                if fsck {
                    "pass git fsck"
                } else {
                    "open cleanly"
                }
            )
        } else {
            format!("damaged repositories (ids): {broken:?}")
        },
    );
    let orphans = std::fs::read_dir(&repos)
        .map(|rd| {
            rd.flatten()
                .filter(|e| {
                    e.file_name()
                        .to_str()
                        .is_some_and(|n| n.ends_with(".git") && !known.contains(n))
                })
                .count()
        })
        .unwrap_or(0);
    report(
        orphans == 0,
        format!("{orphans} repository folders with no database entry"),
    );
    Ok(problems)
}

/// At boot: move repository folders the database doesn't know about into
/// `orphans/` (so a new repo can never reuse stale data) and warn about missing ones.
pub fn reconcile(cfg: &Config) -> Result<()> {
    let db_path = cfg.data_dir.join("gitter.db");
    if !db_path.exists() {
        return Ok(());
    }
    let conn = Connection::open(&db_path)?;
    let ids = repo_ids(&conn)?;
    let known: HashSet<String> = ids.iter().map(|i| format!("{i}.git")).collect();
    let repos = cfg.data_dir.join("repos");
    for id in &ids {
        if !repos.join(format!("{id}.git")).is_dir() {
            eprintln!(
                "warning: repository {id} is in the database but its folder is missing; run `gitter doctor`"
            );
        }
    }
    let Ok(rd) = std::fs::read_dir(&repos) else {
        return Ok(());
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.ends_with(".git") && !known.contains(&name) {
            let dest = cfg.data_dir.join("orphans");
            std::fs::create_dir_all(&dest)?;
            let to = dest.join(format!("{name}.{}", crate::db::now()));
            std::fs::rename(e.path(), &to)?;
            eprintln!("moved unknown repository folder {name} to {}", to.display());
        }
    }
    Ok(())
}

fn absolute(p: &Path) -> Result<PathBuf> {
    Ok(if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir()?.join(p)
    })
}

fn tempdir_in(base: &Path) -> Result<PathBuf> {
    let mut b = [0u8; 8];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut b);
    let p = base.join(format!(".backup-tmp-{}", hex::encode(b)));
    let mut d = std::fs::DirBuilder::new();
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut d, 0o700);
    d.create(&p)
        .with_context(|| format!("couldn't create {}", p.display()))?;
    Ok(p)
}

/// A new file only its owner can read; refuses to overwrite an existing one.
fn owner_only_file(path: &Path) -> Result<std::fs::File> {
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
    o.open(path).with_context(|| {
        format!(
            "couldn't create {} (does it already exist?)",
            path.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconcile_moves_unknown_repo_folders_aside() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = Config {
            data_dir: dir.path().to_path_buf(),
            addr: String::new(),
            url: String::new(),
            secure_cookies: false,
            trust_proxy: false,
            signup: crate::SignupMode::Open,
            max_push_bytes: 1,
            max_repos_per_user: 1,
        };
        crate::db::Db::open(&dir.path().join("gitter.db")).unwrap();
        std::fs::create_dir_all(dir.path().join("repos/99.git")).unwrap();
        reconcile(&cfg).unwrap();
        assert!(!dir.path().join("repos/99.git").exists());
        assert_eq!(
            std::fs::read_dir(dir.path().join("orphans"))
                .unwrap()
                .count(),
            1
        );
    }

    #[cfg(unix)]
    #[test]
    fn backups_are_owner_only_and_never_overwrite() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("b.tar.gz");
        owner_only_file(&out).unwrap();
        assert_eq!(
            std::fs::metadata(&out).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(owner_only_file(&out).is_err());
        let t = tempdir_in(dir.path()).unwrap();
        assert_eq!(
            std::fs::metadata(&t).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }

    fn cfg_for(dir: &Path) -> Config {
        Config {
            data_dir: dir.to_path_buf(),
            addr: String::new(),
            url: String::new(),
            secure_cookies: false,
            trust_proxy: false,
            signup: crate::SignupMode::Open,
            max_push_bytes: 1,
            max_repos_per_user: 1,
        }
    }

    /// Write a gzip tar with raw header names (bypassing the tar crate's own path checks).
    fn evil_archive(path: &Path, entries: &[(&str, tar::EntryType, &str)]) {
        let f = std::fs::File::create(path).unwrap();
        let mut b = tar::Builder::new(flate2::write::GzEncoder::new(
            f,
            flate2::Compression::fast(),
        ));
        for (name, kind, link) in entries {
            let mut h = tar::Header::new_gnu();
            h.as_gnu_mut().unwrap().name[..name.len()].copy_from_slice(name.as_bytes());
            h.set_entry_type(*kind);
            if !link.is_empty() {
                h.set_link_name(link).unwrap();
            }
            let data = b"{\"format\":1}";
            h.set_size(if kind.is_file() { data.len() as u64 } else { 0 });
            h.set_mode(0o644);
            h.set_cksum();
            b.append(&h, if kind.is_file() { &data[..] } else { &[][..] })
                .unwrap();
        }
        b.into_inner().unwrap().finish().unwrap();
    }

    #[test]
    fn restore_refuses_archives_that_escape_the_data_dir() {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data");
        let cases: &[&[(&str, tar::EntryType, &str)]] = &[
            &[
                ("manifest.json", tar::EntryType::Regular, ""),
                ("gitter.db", tar::EntryType::Regular, ""),
                ("repos/../../evil", tar::EntryType::Regular, ""),
            ],
            &[
                ("manifest.json", tar::EntryType::Regular, ""),
                ("/tmp/evil", tar::EntryType::Regular, ""),
            ],
            &[
                ("manifest.json", tar::EntryType::Regular, ""),
                ("repos/link", tar::EntryType::Symlink, "/etc"),
            ],
            &[
                ("manifest.json", tar::EntryType::Regular, ""),
                ("repos/hard", tar::EntryType::Link, "/etc/passwd"),
            ],
            &[
                ("manifest.json", tar::EntryType::Regular, ""),
                ("notes.txt", tar::EntryType::Regular, ""),
            ],
        ];
        for (i, entries) in cases.iter().enumerate() {
            let a = root.path().join(format!("evil{i}.tar.gz"));
            evil_archive(&a, entries);
            assert!(
                restore(&cfg_for(&data), &a, false).is_err(),
                "case {i} should be refused"
            );
            assert!(!root.path().join("evil").exists());
            assert!(
                !data.join("gitter.db").exists(),
                "case {i} must not write anything"
            );
        }
    }

    #[test]
    fn failed_force_restore_leaves_current_data_alone() {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data");
        crate::db::Db::open(&{
            std::fs::create_dir_all(&data).unwrap();
            data.join("gitter.db")
        })
        .unwrap();
        let bad = root.path().join("bad.tar.gz");
        evil_archive(&bad, &[("manifest.json", tar::EntryType::Regular, "")]); // no database
        assert!(restore(&cfg_for(&data), &bad, true).is_err());
        assert!(
            data.join("gitter.db").exists(),
            "live data must stay in place"
        );
        assert!(std::fs::read_dir(&data).unwrap().flatten().all(|e| {
            !e.file_name()
                .to_string_lossy()
                .starts_with("before-restore-")
        }));
    }

    #[test]
    fn backup_never_replaces_an_existing_file() {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data");
        std::fs::create_dir_all(data.join("repos")).unwrap();
        crate::db::Db::open(&data.join("gitter.db")).unwrap();
        let out = root.path().join("b.tar.gz");
        std::fs::write(&out, b"older backup").unwrap();
        assert!(backup(&cfg_for(&data), &out).is_err());
        assert_eq!(std::fs::read(&out).unwrap(), b"older backup");
    }
}
