//! Operator commands: `gitter backup`, `gitter restore`, `gitter doctor`, and
//! the consistency check that runs every time the server boots.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension};
use sha2::Digest;

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

/// A folder of bare git repositories under GITTER_DATA, one `<id>.git` per row of `table`.
struct Store {
    dir: &'static str,
    table: &'static str,
    what: &'static str,
    /// Every row has a folder (wikis are only made on the first edit).
    required: bool,
    /// Only the rows in (true) or out of (false) the trash; None for all of them.
    trashed: Option<bool>,
}

const STORES: &[Store] = &[
    Store { dir: "repos", table: "repos", what: "repositories", required: true, trashed: Some(false) },
    Store { dir: "trash", table: "repos", what: "deleted repositories", required: true, trashed: Some(true) },
    Store { dir: "wikis", table: "repos", what: "wikis", required: false, trashed: None },
    Store { dir: "snippets", table: "snippets", what: "snippets", required: true, trashed: None },
];

/// Folders of plain files the database lists one by one: Git LFS objects and release files.
const DATA_DIRS: [&str; 2] = ["lfs", "releases"];

/// Folder names (`<id>.git`) the database knows for a store. A table that
/// doesn't exist yet (a database from before it was added) knows none, and
/// nothing is in the trash before the trash existed.
fn folders(db: &Connection, s: &Store) -> Result<Vec<String>> {
    let exists = db
        .query_row("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1", [s.table], |_| Ok(()))
        .optional()?;
    if exists.is_none() {
        return Ok(vec![]);
    }
    let has_trash = db
        .query_row("SELECT 1 FROM pragma_table_info(?1) WHERE name = 'deleted_at'", [s.table], |_| Ok(()))
        .optional()?
        .is_some();
    let cond = match (s.trashed, has_trash) {
        (Some(true), false) => return Ok(vec![]),
        (Some(true), true) => "deleted_at IS NOT NULL",
        (Some(false), true) => "deleted_at IS NULL",
        _ => "1 = 1",
    };
    let mut q = db.prepare(&format!("SELECT CAST(id AS TEXT) FROM {} WHERE {cond} ORDER BY id", s.table))?;
    let ids = q.query_map([], |r| r.get::<_, String>(0))?.collect::<Result<Vec<String>, _>>()?;
    // Ids are numbers or hex; anything else (a hand-edited row) must never become a path.
    Ok(ids
        .into_iter()
        .filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric()))
        .map(|id| format!("{id}.git"))
        .collect())
}

/// Git LFS object ids the database refers to (none before the table exists).
fn lfs_oids(db: &Connection) -> Result<Vec<String>> {
    let table = db
        .query_row("SELECT 1 FROM sqlite_master WHERE name = 'lfs_objects'", [], |_| Ok(()))
        .optional()?;
    if table.is_none() {
        return Ok(vec![]);
    }
    let mut s = db.prepare("SELECT DISTINCT oid FROM lfs_objects ORDER BY oid")?;
    let oids = s
        .query_map([], |r| r.get(0))?
        .collect::<Result<Vec<String>, _>>()?;
    Ok(oids.into_iter().filter(|o| crate::lfs::valid_oid(o)).collect())
}

/// Release files the database refers to, as (`releases/<release id>/<asset id>`, sha256);
/// none before the table exists.
fn release_files(db: &Connection) -> Result<Vec<(String, String)>> {
    let table = db
        .query_row("SELECT 1 FROM sqlite_master WHERE name = 'release_assets'", [], |_| Ok(()))
        .optional()?;
    if table.is_none() {
        return Ok(vec![]);
    }
    let mut s = db.prepare("SELECT release_id, id, sha256 FROM release_assets ORDER BY id")?;
    let files = s.query_map([], |r| Ok((format!("releases/{}/{}", r.get::<_, i64>(0)?, r.get::<_, i64>(1)?), r.get(2)?)))?;
    Ok(files.collect::<Result<_, _>>()?)
}

fn sha256_file(p: &Path) -> Result<String> {
    let mut h = sha2::Sha256::new();
    std::io::copy(&mut std::fs::File::open(p)?, &mut h)?;
    Ok(hex::encode(h.finalize()))
}

/// Write `<out>` (`-` for stdout): a .tar.gz holding a consistent copy of the
/// database, every repository and Git LFS file it lists and a manifest. Safe to run while the server is up.
pub fn backup(cfg: &Config, out: &Path) -> Result<()> {
    let db_path = cfg.data_dir.join("gitter.db");
    if !db_path.exists() {
        bail!(
            "no database at {} (is GITTER_DATA right?)",
            db_path.display()
        );
    }
    let to_stdout = out == Path::new("-");
    let out = if to_stdout { out.to_path_buf() } else { absolute(out)? };
    // Created before the work starts, so a refusal to overwrite never deletes an older backup.
    let file: Box<dyn std::io::Write> = if to_stdout {
        Box::new(std::io::stdout().lock())
    } else {
        Box::new(owner_only_file(&out)?)
    };
    // The snapshot holds password and session hashes: keep it next to the data, owner-only.
    let tmp = tempdir_in(&cfg.data_dir)?;
    let res = (|| -> Result<(usize, i64, usize, usize)> {
        let snap = tmp.join("gitter.db");
        let conn = Connection::open(&db_path)?;
        conn.busy_timeout(std::time::Duration::from_secs(10))?;
        // VACUUM INTO writes a transactionally consistent snapshot without stopping writers.
        conn.execute("VACUUM INTO ?1", [snap.to_string_lossy()])?;
        let snap_conn = Connection::open(&snap)?;
        let stores: Vec<Vec<String>> = STORES.iter().map(|s| folders(&snap_conn, s)).collect::<Result<_>>()?;
        let oids = lfs_oids(&snap_conn)?;
        let assets = release_files(&snap_conn)?;
        drop(snap_conn);
        let users: i64 = conn.query_row("SELECT count(*) FROM users", [], |r| r.get(0))?;
        drop(conn);
        let count = |dir: &str| STORES.iter().zip(&stores).find(|(s, _)| s.dir == dir).map_or(0, |(_, n)| n.len());
        let manifest = serde_json::json!({
            "format": FORMAT, "gitter": env!("CARGO_PKG_VERSION"), "created_at": crate::db::now(),
            "repos": count("repos"), "trash": count("trash"), "snippets": count("snippets"), "users": users, "lfs_objects": oids.len(),
            "release_files": assets.len(),
        });
        std::fs::write(tmp.join("manifest.json"), serde_json::to_vec_pretty(&manifest)?)?;
        let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(file, flate2::Compression::default()));
        tar.follow_symlinks(false);
        tar.append_path_with_name(tmp.join("manifest.json"), "manifest.json")?;
        tar.append_path_with_name(&snap, "gitter.db")?;
        // The key CI secrets are encrypted with: without it a restore can't read them.
        let key = cfg.data_dir.join("secret.key");
        if key.is_file() {
            tar.append_path_with_name(&key, "secret.key")?;
        }
        // Only the folders in the snapshot, so one created meanwhile can't show up as an orphan.
        for (s, names) in STORES.iter().zip(&stores) {
            tar.append_dir(s.dir, &cfg.data_dir)?;
            // A repository moved into or out of the trash since the snapshot is in the other
            // folder now; it's archived where the snapshot's row expects it.
            let sibling = match s.dir {
                "repos" => Some("trash"),
                "trash" => Some("repos"),
                _ => None,
            };
            for name in names {
                let here = cfg.data_dir.join(s.dir).join(name);
                let src = if here.is_dir() {
                    here
                } else if let Some(there) = sibling.map(|o| cfg.data_dir.join(o).join(name)).filter(|p| p.is_dir()) {
                    there
                } else {
                    if s.required {
                        eprintln!("warning: {}/{name} is missing (deleted during the backup?)", s.dir);
                    }
                    continue;
                };
                append_repo(&mut tar, &src, &format!("{}/{name}", s.dir))?;
            }
        }
        // A row is written only once its file is in place, and files are never rewritten
        // with other content, so every object the snapshot lists is on disk.
        tar.append_dir("lfs", &cfg.data_dir)?;
        for oid in &oids {
            let src = crate::lfs::object_path(&cfg.data_dir, oid);
            if !src.is_file() {
                eprintln!("warning: Git LFS object {oid} is missing");
                continue;
            }
            let name = crate::lfs::object_path(Path::new(""), oid);
            tar.append_path_with_name(&src, name)?;
        }
        // Like LFS files: a release file is in place before its row commits, and never rewritten.
        tar.append_dir("releases", &cfg.data_dir)?;
        for (name, _) in &assets {
            let src = cfg.data_dir.join(name);
            if !src.is_file() {
                eprintln!("warning: release file {name} is missing (deleted during the backup?)");
                continue;
            }
            // Tolerates the file vanishing between the check and the copy.
            append_tree(&mut tar, &src, name)?;
        }
        tar.into_inner()?.finish()?.flush()?;
        Ok((stores[0].len(), users, oids.len(), assets.len()))
    })();
    let _ = std::fs::remove_dir_all(&tmp);
    if res.is_err() && !to_stdout {
        let _ = std::fs::remove_file(&out);
    }
    let (repos, users, lfs, files) = res?;
    let size = if to_stdout { String::new() } else {
        format!(" ({:.1} MB)", std::fs::metadata(&out)?.len() as f64 / 1e6)
    };
    eprintln!(
        "Backed up {users} accounts, {repos} repositories, {lfs} Git LFS files and {files} release files to {}{size}.",
        if to_stdout { "stdout".into() } else { out.display().to_string() },
    );
    Ok(())
}

/// Add one bare repository. Refs go first: objects copied after them are a
/// superset of what those refs need, even while a push is landing.
// ponytail: a `git gc` racing the backup can still delete a pack mid-copy; restore's fsck catches it.
fn append_repo<W: std::io::Write>(tar: &mut tar::Builder<W>, src: &Path, name: &str) -> Result<()> {
    tar.append_dir(name, src)?;
    let first = ["HEAD", "packed-refs", "config", "refs"];
    for f in first {
        append_tree(tar, &src.join(f), &format!("{name}/{f}"))?;
    }
    for e in std::fs::read_dir(src)?.flatten() {
        let n = e.file_name().to_string_lossy().into_owned();
        if !first.contains(&n.as_str()) && !n.starts_with("gitter-index-") {
            append_tree(tar, &e.path(), &format!("{name}/{n}"))?;
        }
    }
    Ok(())
}

/// Add a file or directory tree, skipping files that vanish while we walk (temp files, repacks).
fn append_tree<W: std::io::Write>(tar: &mut tar::Builder<W>, path: &Path, name: &str) -> Result<()> {
    let gone = |e: &std::io::Error| e.kind() == std::io::ErrorKind::NotFound;
    let meta = match std::fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if gone(&e) => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    if meta.is_dir() {
        tar.append_dir(name, path)?;
        let rd = match std::fs::read_dir(path) {
            Ok(rd) => rd,
            Err(e) if gone(&e) => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        for e in rd.flatten() {
            append_tree(tar, &e.path(), &format!("{name}/{}", e.file_name().to_string_lossy()))?;
        }
    } else if meta.is_file() {
        match std::fs::File::open(path) {
            Ok(mut f) => tar.append_file(name, &mut f)?,
            Err(e) if gone(&e) => {}
            Err(e) => return Err(e.into()),
        }
    }
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
            || name == "secret.key"
            || name == "lfs"
            || name.starts_with("lfs/")
            || name == "releases"
            || name.starts_with("releases/")
            || STORES.iter().any(|s| name == s.dir || name.starts_with(&format!("{}/", s.dir)));
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

/// Unpack a checked archive into `stage` and make sure its database opens cleanly.
fn unpack(archive: &Path, stage: &Path) -> Result<serde_json::Value> {
    let manifest = inspect_archive(archive)?;
    let f = std::fs::File::open(archive)?;
    let mut ar = tar::Archive::new(flate2::read::GzDecoder::new(f));
    ar.set_preserve_permissions(true);
    for e in ar.entries()? {
        e?.unpack_in(stage)?;
    }
    // Backups from before wikis, snippets, LFS and releases have no such folders.
    for d in STORES.iter().map(|s| s.dir).chain(DATA_DIRS) {
        std::fs::create_dir_all(stage.join(d))?;
    }
    let conn = Connection::open(stage.join("gitter.db"))?;
    let ok: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
    if ok != "ok" {
        bail!("the database in the backup is damaged: {ok}");
    }
    Ok(manifest)
}

/// `-` means stdin: spool it to an owner-only file under `dir` (the archive is read twice).
fn archive_path(archive: &Path, dir: &Path) -> Result<(PathBuf, Option<PathBuf>)> {
    if archive != Path::new("-") {
        let p = absolute(archive)?
            .canonicalize()
            .with_context(|| format!("can't find {}", archive.display()))?;
        return Ok((p, None));
    }
    let tmp = tempdir_in(dir)?;
    let p = tmp.join("stdin.tar.gz");
    let mut f = owner_only_file(&p)?;
    std::io::copy(&mut std::io::stdin().lock(), &mut f).context("couldn't read the backup from stdin")?;
    Ok((p, Some(tmp)))
}

/// Check a backup end to end (archive, database, every repository's objects) without restoring it.
pub fn verify(cfg: &Config, archive: &Path) -> Result<()> {
    let scratch = std::env::temp_dir();
    let (archive, spool) = archive_path(archive, &scratch)?;
    let stage = tempdir_in(&scratch)?;
    let res = (|| -> Result<()> {
        let manifest = unpack(&archive, &stage)?;
        let problems = check(&Config { data_dir: stage.clone(), ..cfg.clone() }, true)?;
        if problems > 0 {
            bail!("the backup has {problems} problem(s) (see above)");
        }
        println!("The backup is good: {} accounts and {} repositories, made by gitter {}.",
            manifest["users"], manifest["repos"], manifest["gitter"].as_str().unwrap_or("?"));
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&stage);
    if let Some(t) = spool {
        let _ = std::fs::remove_dir_all(t);
    }
    res
}

/// Restore a backup (`-` for stdin) into GITTER_DATA. Nothing existing is
/// touched until the archive has been fully checked and unpacked into a staging folder.
pub fn restore(cfg: &Config, archive: &Path, force: bool) -> Result<()> {
    std::fs::create_dir_all(&cfg.data_dir)?;
    let data = cfg.data_dir.canonicalize()?;
    let has = |d: &str| std::fs::read_dir(data.join(d)).is_ok_and(|mut rd| rd.next().is_some());
    let occupied = data.join("gitter.db").exists() || DATA_DIRS.iter().any(|d| has(d)) || STORES.iter().any(|s| has(s.dir));
    if occupied && !force {
        bail!(
            "{} already has data. Restore into an empty directory, or pass --force to move the current data aside first.",
            data.display()
        );
    }
    let (archive, spool) = archive_path(archive, &data)?;
    let res = restore_from(cfg, &data, &archive, occupied);
    if let Some(t) = spool {
        let _ = std::fs::remove_dir_all(t);
    }
    res
}

fn restore_from(cfg: &Config, data: &Path, archive: &Path, occupied: bool) -> Result<()> {
    let stage = tempdir_in(data)?;
    let manifest = match unpack(archive, &stage) {
        Ok(m) => m,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&stage);
            return Err(e);
        }
    };
    if occupied {
        // Move the current contents into a subfolder (the data dir itself may be a
        // mount point, e.g. a Docker volume, so it can't be renamed).
        let aside = data.join(format!("before-restore-{}", crate::db::now()));
        std::fs::create_dir_all(&aside)?;
        for e in std::fs::read_dir(data)?.flatten() {
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
        eprintln!("Moved the existing data to {}.", aside.display());
    }
    for name in ["gitter.db"].into_iter().chain(DATA_DIRS).chain(STORES.iter().map(|s| s.dir)) {
        std::fs::rename(stage.join(name), data.join(name))
            .with_context(|| format!("couldn't move the restored {name} into place"))?;
    }
    let key = stage.join("secret.key");
    if key.is_file() {
        #[cfg(unix)]
        std::fs::set_permissions(&key, std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
        std::fs::rename(&key, data.join("secret.key")).context("couldn't move the restored secret.key into place")?;
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
    // Optional, so never a problem: without one, commits signed that way show "Can't verify".
    for (tool, arg, kind) in [("gpg", "--version", "GPG"), ("ssh-keygen", "-?", "SSH")] {
        match Command::new(tool).arg(arg).output() {
            Ok(_) => println!("✓ {tool} is installed: {kind} commit signatures are checked"),
            Err(_) => println!("! {tool} isn't installed: {kind}-signed commits show \"Can't verify\""),
        }
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
    match conn
        .query_row("SELECT 1 FROM sqlite_master WHERE name = 'audit'", [], |_| Ok(()))
        .map(|_| crate::security::verify(&conn))
    {
        Ok(Ok(Ok(h))) => report(
            true,
            format!("security log intact ({} events; latest #{} sealed {})", h.count, h.last_id, h.hash.get(..12).unwrap_or("-")),
        ),
        Ok(Ok(Err(id))) => report(false, format!("security log was altered at or before event #{id}")),
        _ => {}
    }
    for s in STORES {
        let names = folders(&conn, s)?;
        let root = data.join(s.dir);
        let (mut present, mut missing, mut broken) = (0, 0, Vec::new());
        for name in &names {
            let p = root.join(name);
            if !p.is_dir() {
                missing += s.required as usize;
                continue;
            }
            present += 1;
            let gd = format!("--git-dir={}", p.display());
            let mut args = vec![gd.as_str(), "rev-parse", "--git-dir"];
            if fsck {
                args = vec![gd.as_str(), "fsck", "--connectivity-only", "--no-dangling", "--no-progress"];
            }
            if run(git(&args), "git").is_err() {
                broken.push(name.trim_end_matches(".git").to_string());
            }
        }
        if s.required {
            let gone = if missing > 0 { format!(" ({missing} missing)") } else { String::new() };
            report(missing == 0, format!("{present} {} on disk for {} in the database{gone}", s.what, names.len()));
        }
        report(
            broken.is_empty(),
            if broken.is_empty() {
                format!("{present} {} {}", s.what, if fsck { "pass git fsck" } else { "open cleanly" })
            } else {
                format!("damaged {} (ids): {broken:?}", s.what)
            },
        );
        let known: HashSet<&str> = names.iter().map(String::as_str).collect();
        let orphans = std::fs::read_dir(&root)
            .map(|rd| {
                rd.flatten()
                    .filter(|e| e.file_name().to_str().is_some_and(|n| n.ends_with(".git") && !known.contains(n)))
                    .count()
            })
            .unwrap_or(0);
        report(orphans == 0, format!("{orphans} folders in {}/ with no database entry", s.dir));
    }
    let oids = lfs_oids(&conn)?;
    if !oids.is_empty() {
        let (mut missing, mut damaged) = (0, 0);
        for oid in &oids {
            let p = crate::lfs::object_path(data, oid);
            if !p.is_file() {
                missing += 1;
            } else if fsck && sha256_file(&p).ok().as_ref() != Some(oid) {
                damaged += 1;
            }
        }
        report(
            missing == 0,
            format!("{} of {} Git LFS files on disk", oids.len() - missing, oids.len()),
        );
        if fsck {
            report(damaged == 0, format!("{damaged} Git LFS files don't match their hash"));
        }
    }
    let assets = release_files(&conn)?;
    if !assets.is_empty() {
        let (mut missing, mut damaged) = (0, 0);
        for (name, sha) in &assets {
            let p = data.join(name);
            if !p.is_file() {
                missing += 1;
            } else if fsck && sha256_file(&p).ok().as_ref() != Some(sha) {
                damaged += 1;
            }
        }
        report(missing == 0, format!("{} of {} release files on disk", assets.len() - missing, assets.len()));
        if fsck {
            report(damaged == 0, format!("{damaged} release files don't match their hash"));
        }
    }
    Ok(problems)
}

/// At boot: move repository, wiki and snippet folders the database doesn't
/// know about into `orphans/` (so a new one can never reuse stale data) and
/// warn about missing ones.
pub fn reconcile(cfg: &Config) -> Result<()> {
    // Git LFS and release uploads cut off by the restart.
    for d in DATA_DIRS {
        let _ = std::fs::remove_dir_all(cfg.data_dir.join(d).join("tmp"));
    }
    let db_path = cfg.data_dir.join("gitter.db");
    if !db_path.exists() {
        return Ok(());
    }
    let conn = Connection::open(&db_path)?;
    // A crash between moving a folder into or out of the trash and recording it
    // leaves the folder on the other side: put it back where the database says.
    for s in STORES.iter().filter(|s| s.trashed.is_some()) {
        let Some(other) = STORES.iter().find(|o| o.table == s.table && o.trashed.is_some() && o.trashed != s.trashed) else { continue };
        for name in folders(&conn, s)? {
            let (want, stray) = (cfg.data_dir.join(s.dir).join(&name), cfg.data_dir.join(other.dir).join(&name));
            if !want.is_dir() && stray.is_dir() {
                std::fs::create_dir_all(cfg.data_dir.join(s.dir))?;
                std::fs::rename(&stray, &want)?;
                eprintln!("moved {}/{name} back to {}/", other.dir, s.dir);
            }
        }
    }
    for s in STORES {
        let names = folders(&conn, s)?;
        let root = cfg.data_dir.join(s.dir);
        for name in names.iter().filter(|n| s.required && !root.join(n).is_dir()) {
            eprintln!("warning: {}/{name} is in the database but its folder is missing; run `gitter doctor`", s.dir);
        }
        let Ok(rd) = std::fs::read_dir(&root) else { continue };
        let known: HashSet<&str> = names.iter().map(String::as_str).collect();
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.ends_with(".git") && !known.contains(name.as_str()) {
                let dest = cfg.data_dir.join("orphans");
                std::fs::create_dir_all(&dest)?;
                // Repos keep their old orphan names; the others say where they came from.
                let prefix = if s.dir == "repos" { String::new() } else { format!("{}-", s.dir) };
                let to = dest.join(format!("{prefix}{name}.{}", crate::db::now()));
                std::fs::rename(e.path(), &to)?;
                eprintln!("moved unknown folder {}/{name} to {}", s.dir, to.display());
            }
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
        let cfg = cfg_for(dir.path());
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

    #[test]
    fn reconcile_keeps_known_wikis_and_snippets() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        crate::db::Db::open(&d.join("gitter.db")).unwrap();
        let c = Connection::open(d.join("gitter.db")).unwrap();
        c.execute_batch(
            "INSERT INTO users(id, username, pass_hash, created_at) VALUES (1, 'a', 'x', 0);
             INSERT INTO repos(id, owner_id, name, created_at, updated_at) VALUES (7, 1, 'r', 0, 0);
             INSERT INTO snippets(id, owner_id, created_at, updated_at) VALUES ('ab', 1, 0, 0);",
        )
        .unwrap();
        for p in ["repos/7.git", "wikis/7.git", "wikis/8.git", "snippets/ab.git", "snippets/cd.git"] {
            std::fs::create_dir_all(d.join(p)).unwrap();
        }
        reconcile(&cfg_for(d)).unwrap();
        for (p, kept) in [("repos/7.git", true), ("wikis/7.git", true), ("snippets/ab.git", true), ("wikis/8.git", false), ("snippets/cd.git", false)] {
            assert_eq!(d.join(p).exists(), kept, "{p}");
        }
        let mut moved: Vec<String> = std::fs::read_dir(d.join("orphans")).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        moved.sort();
        assert!(moved[0].starts_with("snippets-cd.git.") && moved[1].starts_with("wikis-8.git."), "{moved:?}");
    }

    #[test]
    fn reconcile_puts_half_moved_trash_folders_back() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        crate::db::Db::open(&d.join("gitter.db")).unwrap();
        Connection::open(d.join("gitter.db"))
            .unwrap()
            .execute_batch(
                "INSERT INTO users(id, username, pass_hash, created_at) VALUES (1, 'a', 'x', 0);
                 INSERT INTO repos(id, owner_id, name, created_at, updated_at) VALUES (1, 1, 'live', 0, 0);
                 INSERT INTO repos(id, owner_id, name, deleted_name, deleted_at, created_at, updated_at) VALUES (2, 1, '~2', 'gone', 5, 0, 0);",
            )
            .unwrap();
        // A crash left the live repo's folder in the trash and the trashed one's still in repos/.
        for p in ["trash/1.git", "repos/2.git"] {
            std::fs::create_dir_all(d.join(p)).unwrap();
        }
        reconcile(&cfg_for(d)).unwrap();
        assert!(d.join("repos/1.git").is_dir() && d.join("trash/2.git").is_dir());
        assert!(!d.join("orphans").exists(), "nothing known is moved aside");
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
    fn backups_carry_lfs_files_and_verify_checks_their_hashes() {
        use sha2::Digest;
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data");
        std::fs::create_dir_all(data.join("repos")).unwrap();
        crate::db::Db::open(&data.join("gitter.db")).unwrap();
        run(git(&["init", "-q", "--bare", &data.join("repos/1.git").to_string_lossy()]), "git init").unwrap();
        let body = b"pretend this is a large video".to_vec();
        let oid = hex::encode(sha2::Sha256::digest(&body));
        Connection::open(data.join("gitter.db"))
            .unwrap()
            .execute_batch(&format!(
                "INSERT INTO users(id, username, pass_hash, created_at) VALUES (1, 'a', 'x', 0);
                 INSERT INTO repos(id, owner_id, name, created_at, updated_at) VALUES (1, 1, 'r', 0, 0);
                 INSERT INTO lfs_objects(repo_id, oid, size, created_at) VALUES (1, '{oid}', {}, 0);",
                body.len()
            ))
            .unwrap();
        let obj = crate::lfs::object_path(&data, &oid);
        std::fs::create_dir_all(obj.parent().unwrap()).unwrap();
        std::fs::write(&obj, &body).unwrap();
        std::fs::create_dir_all(data.join("lfs/tmp")).unwrap();
        std::fs::write(data.join("lfs/tmp/half-upload"), b"x").unwrap();

        let good = root.path().join("good.tar.gz");
        backup(&cfg_for(&data), &good).unwrap();
        verify(&cfg_for(&data), &good).unwrap();
        let fresh = root.path().join("fresh");
        restore(&cfg_for(&fresh), &good, false).unwrap();
        assert_eq!(std::fs::read(crate::lfs::object_path(&fresh, &oid)).unwrap(), body);
        assert!(!fresh.join("lfs/tmp").exists(), "unfinished uploads aren't backed up");

        std::fs::write(&obj, b"bit rot").unwrap();
        let bad = root.path().join("bad.tar.gz");
        backup(&cfg_for(&data), &bad).unwrap();
        assert!(verify(&cfg_for(&data), &bad).is_err(), "a damaged LFS file fails verify");
        std::fs::remove_file(&obj).unwrap();
        assert_eq!(check(&cfg_for(&data), false).unwrap(), 1, "a missing LFS file is a problem");
    }

    #[test]
    fn backups_carry_release_assets_and_verify_checks_them() {
        use sha2::Digest;
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data");
        std::fs::create_dir_all(data.join("repos")).unwrap();
        crate::db::Db::open(&data.join("gitter.db")).unwrap();
        run(git(&["init", "-q", "--bare", &data.join("repos/1.git").to_string_lossy()]), "git init").unwrap();
        let body = b"a release binary".to_vec();
        Connection::open(data.join("gitter.db"))
            .unwrap()
            .execute_batch(&format!(
                "INSERT INTO users(id, username, pass_hash, created_at) VALUES (1, 'a', 'x', 0);
                 INSERT INTO repos(id, owner_id, name, created_at, updated_at) VALUES (1, 1, 'r', 0, 0);
                 INSERT INTO releases(id, repo_id, tag, created_at) VALUES (5, 1, 'v1', 0);
                 INSERT INTO release_assets(id, release_id, name, size, sha256, created_at) VALUES (9, 5, 'tool', {}, '{}', 0);",
                body.len(),
                hex::encode(sha2::Sha256::digest(&body))
            ))
            .unwrap();
        let file = data.join("releases/5/9");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, &body).unwrap();
        std::fs::create_dir_all(data.join("releases/tmp")).unwrap();
        std::fs::write(data.join("releases/tmp/half-upload"), b"x").unwrap();

        let good = root.path().join("good.tar.gz");
        backup(&cfg_for(&data), &good).unwrap();
        verify(&cfg_for(&data), &good).unwrap();
        let fresh = root.path().join("fresh");
        restore(&cfg_for(&fresh), &good, false).unwrap();
        assert_eq!(std::fs::read(fresh.join("releases/5/9")).unwrap(), body);
        assert!(!fresh.join("releases/tmp").exists(), "unfinished uploads aren't backed up");

        std::fs::write(&file, b"bit rot").unwrap();
        let bad = root.path().join("bad.tar.gz");
        backup(&cfg_for(&data), &bad).unwrap();
        assert!(verify(&cfg_for(&data), &bad).is_err(), "a damaged release file fails verify");
        std::fs::remove_file(&file).unwrap();
        assert_eq!(check(&cfg_for(&data), false).unwrap(), 1, "a missing release file is a problem");
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
