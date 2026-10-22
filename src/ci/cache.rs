//! Job caches: workspace folders kept between runs under a key.
//!
//! The runner expands the key template (`deps-{{ hash Cargo.lock }}`) in the
//! fresh checkout, asks for the newest archive with that key (or, failing that,
//! one whose key starts with a `restore-keys` prefix), unpacks it into the
//! workspace before the steps, and after a successful job saves the paths under
//! the key unless it restored that exact key. Archives are .tar.gz files under
//! GITTER_DATA/ci/cache, sent through the runner API like artifacts.
//!
//! Caches never cross into a more trusted run (cache poisoning): a run reads
//! caches of its own branch or the default branch, and only those written by
//! runs at least as trusted as itself (an admin's > a person's > an agent's).
//! So the default branch never sees a branch's cache, and an admin's run never
//! sees one an agent's or a collaborator's run wrote.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};

use crate::AppState;
use crate::db::now;
use crate::error::{AppError, R};

/// One archive is at most this big.
pub const ENTRY_CAP: u64 = 1 << 30;
/// A repository's caches together; past it the least recently used go.
// ponytail: one fixed cap for every repository; a site setting if admins need to tune it.
pub const REPO_CAP: u64 = 2 << 30;
/// Caches nobody restored for this long are deleted.
pub const CACHE_DAYS: i64 = 7;

/// Trust levels: who started the run that wrote (or reads) a cache.
pub const AGENT: i64 = 0;
pub const USER: i64 = 1;
pub const ADMIN: i64 = 2;

const MAX_KEY: usize = 200;

pub fn path(data: &Path, id: i64) -> PathBuf {
    data.join("ci/cache").join(format!("{id}.tar.gz"))
}

/// (text before, the words inside) per `{{ … }}`, and the text after the last.
type Parts<'a> = (Vec<(&'a str, Vec<&'a str>)>, &'a str);

/// The `{{ … }}` parts of a template.
fn parts(t: &str) -> Result<Parts<'_>, String> {
    let (mut out, mut rest) = (Vec::new(), t);
    while let Some(i) = rest.find("{{") {
        let close = rest[i..].find("}}").ok_or("a {{ isn't closed with }}")?;
        out.push((&rest[..i], rest[i + 2..i + close].split_whitespace().collect()));
        rest = &rest[i + close + 2..];
    }
    Ok((out, rest))
}

/// Check a key template: text, `{{ hash GLOB… }}`, `{{ os }}`, `{{ arch }}`
/// (and `${{ matrix.x }}`, filled in when the file is read).
pub fn check_key(t: &str) -> Result<(), String> {
    if t.trim().is_empty() || t.len() > MAX_KEY {
        return Err(format!("give a key of 1 to {MAX_KEY} characters"));
    }
    for (before, words) in parts(t)?.0 {
        match words.as_slice() {
            ["os" | "arch"] => {}
            ["hash", globs @ ..] if (1..=10).contains(&globs.len()) => {}
            [m] if before.ends_with('$') && m.starts_with("matrix.") => {}
            _ => return Err(format!("unknown {{{{ {} }}}} (use hash FILES, os or arch)", words.join(" "))),
        }
    }
    Ok(())
}

/// Regular files under `ws` (not `.git`, not through symlinks) whose
/// workspace-relative path passes `keep`, sorted. At most `max` are looked at.
pub fn walk(ws: &Path, keep: impl Fn(&str) -> bool, max: usize) -> std::io::Result<Vec<(PathBuf, String)>> {
    let (mut files, mut stack, mut seen) = (Vec::new(), vec![ws.to_path_buf()], 0);
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d)?.flatten() {
            seen += 1;
            if seen > max {
                return Err(std::io::Error::other(format!("more than {max} files in the workspace")));
            }
            let (p, ft) = (e.path(), e.file_type()?);
            if ft.is_dir() && e.file_name() != ".git" {
                stack.push(p);
            } else if ft.is_file()
                && let Ok(rel) = p.strip_prefix(ws)
            {
                let rel = rel.to_string_lossy().into_owned();
                if keep(&rel) {
                    files.push((p, rel));
                }
            }
        }
    }
    files.sort_by(|a, b| a.1.cmp(&b.1));
    Ok(files)
}

/// Fill in a key template in workspace `ws`, leaving only `A-Z a-z 0-9 . _ -`
/// (anything else becomes `-`). `hash` is the SHA-256 of the matching files'
/// names and contents ("" files hash too, so a key always comes out).
pub fn expand_key(t: &str, ws: &Path) -> String {
    let mut out = String::new();
    match parts(t) {
        Ok((ps, rest)) => {
            for (before, words) in ps {
                out.push_str(before);
                match words.as_slice() {
                    ["os"] => out.push_str(std::env::consts::OS),
                    ["arch"] => out.push_str(std::env::consts::ARCH),
                    ["hash", globs @ ..] => {
                        let globs: Vec<String> = globs.iter().map(|g| g.to_string()).collect();
                        let mut h = Sha256::new();
                        for (p, rel) in walk(ws, |r| globs.iter().any(|g| super::config::glob(g, r)), 200_000).unwrap_or_default() {
                            h.update(rel.as_bytes());
                            h.update([0]);
                            if let Ok(mut f) = std::fs::File::open(&p) {
                                let _ = std::io::copy(&mut f, &mut h);
                            }
                            h.update([0]);
                        }
                        out.push_str(&hex::encode(h.finalize()));
                    }
                    w => out.push_str(&w.join("-")),
                }
            }
            out.push_str(rest);
        }
        Err(_) => out.push_str(t),
    }
    out.chars().map(|c| if c.is_ascii_alphanumeric() || "._-".contains(c) { c } else { '-' }).take(MAX_KEY).collect()
}

/// Which caches a job may read and where it writes.
pub struct Scope {
    pub repo_id: i64,
    pub refname: String,
    pub default_ref: String,
    pub trust: i64,
}

/// The scope of job `job`, from its run (never from what a runner says).
pub fn scope(c: &Connection, job: i64) -> rusqlite::Result<Scope> {
    c.query_row(
        "SELECT r.repo_id, r.ref, 'refs/heads/' || p.default_branch, r.agent IS NOT NULL,
                COALESCE((SELECT is_admin FROM users WHERE id = r.actor_id), 0)
         FROM ci_jobs j JOIN ci_runs r ON r.id = j.run_id JOIN repos p ON p.id = r.repo_id WHERE j.id = ?1",
        [job],
        |r| {
            let trust = if r.get::<_, bool>(3)? { AGENT } else if r.get::<_, bool>(4)? { ADMIN } else { USER };
            Ok(Scope { repo_id: r.get(0)?, refname: r.get(1)?, default_ref: r.get(2)?, trust })
        },
    )
}

/// The cache to restore: the newest with `key` on the run's branch, else with
/// a `restore` prefix there, then the same on the default branch. (id, key)
pub fn lookup(c: &Connection, s: &Scope, key: &str, restore: &[String]) -> rusqlite::Result<Option<(i64, String)>> {
    let mut refs = vec![s.refname.as_str()];
    if s.default_ref != s.refname {
        refs.push(&s.default_ref);
    }
    let mut exact = c.prepare_cached(
        "SELECT id, key FROM ci_caches WHERE repo_id = ?1 AND ref = ?2 AND trust >= ?3 AND key = ?4 ORDER BY created_at DESC, id DESC LIMIT 1",
    )?;
    let mut prefix = c.prepare_cached(
        "SELECT id, key FROM ci_caches WHERE repo_id = ?1 AND ref = ?2 AND trust >= ?3 AND substr(key, 1, length(?4)) = ?4
         ORDER BY created_at DESC, id DESC LIMIT 1",
    )?;
    for r in refs {
        let row = |q: &mut rusqlite::CachedStatement, k: &str| q.query_row(params![s.repo_id, r, s.trust, k], |x| Ok((x.get(0)?, x.get(1)?))).optional();
        if let Some(hit) = row(&mut exact, key)? {
            return Ok(Some(hit));
        }
        for p in restore.iter().filter(|p| !p.is_empty()) {
            if let Some(hit) = row(&mut prefix, p)? {
                return Ok(Some(hit));
            }
        }
    }
    Ok(None)
}

/// Bring a repository's caches under `cap`, least recently used first. Returns
/// the ids removed (their files are the caller's to delete).
pub fn evict(c: &Connection, repo: i64, cap: u64) -> rusqlite::Result<Vec<i64>> {
    let rows: Vec<(i64, i64)> = c
        .prepare("SELECT id, size FROM ci_caches WHERE repo_id = ?1 ORDER BY used_at DESC, id DESC")?
        .query_map([repo], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let mut total = 0u64;
    let mut gone: Vec<i64> = rows
        .into_iter()
        .filter(|(_, size)| {
            total = total.saturating_add((*size).max(0) as u64);
            total > cap
        })
        .map(|r| r.0)
        .collect();
    gone.reverse();
    for id in &gone {
        c.execute("DELETE FROM ci_caches WHERE id = ?1", [id])?;
    }
    Ok(gone)
}

/// The archive to restore for job `job`, if any: (its key, its file).
pub async fn find(st: &AppState, job: i64, key: String, restore: Vec<String>) -> R<Option<(String, PathBuf)>> {
    let data = st.cfg.data_dir.clone();
    let hit = st
        .db
        .call(move |c| {
            let s = scope(c, job)?;
            let hit = lookup(c, &s, &key, &restore)?;
            if let Some((id, _)) = &hit {
                // A row whose file is gone (a crash before it was placed, a restored backup)
                // goes too, or it would hold that key forever.
                if !path(&data, *id).exists() {
                    c.execute("DELETE FROM ci_caches WHERE id = ?1", [id])?;
                    return Ok(None);
                }
                c.execute("UPDATE ci_caches SET used_at = ?2 WHERE id = ?1", params![id, now()])?;
            }
            Ok(hit)
        })
        .await?;
    Ok(hit.map(|(id, k)| (k, path(&st.cfg.data_dir, id))))
}

/// Keep `file` as job `job`'s cache under `key` (moving or copying it), unless
/// its scope has that key already. Then evict over REPO_CAP. True if kept.
pub async fn store(st: &AppState, job: i64, key: String, file: &Path) -> R<bool> {
    let size = tokio::fs::metadata(file).await?.len();
    if size > ENTRY_CAP {
        return Err(AppError::bad("A cache is limited to 1 GB"));
    }
    if expand_key(&key, Path::new("")) != key || key.is_empty() {
        return Err(AppError::bad("That isn't a cache key"));
    }
    let id = st
        .db
        .call(move |c| {
            let s = scope(c, job)?;
            let t = now();
            let n = c.execute(
                "INSERT OR IGNORE INTO ci_caches(repo_id, ref, trust, key, size, created_at, used_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
                params![s.repo_id, s.refname, s.trust, key, size as i64, t],
            )?;
            Ok((n == 1).then(|| c.last_insert_rowid()))
        })
        .await?;
    let Some(id) = id else { return Ok(false) };
    let dest = path(&st.cfg.data_dir, id);
    let placed = async {
        if let Some(d) = dest.parent() {
            tokio::fs::create_dir_all(d).await?;
        }
        if tokio::fs::rename(file, &dest).await.is_err() {
            tokio::fs::copy(file, &dest).await?;
        }
        Ok::<_, std::io::Error>(())
    }
    .await;
    let gone = st
        .db
        .call(move |c| {
            if placed.is_err() {
                c.execute("DELETE FROM ci_caches WHERE id = ?1", [id])?;
                return Ok(vec![]);
            }
            let repo: i64 = c.query_row("SELECT repo_id FROM ci_caches WHERE id = ?1", [id], |r| r.get(0))?;
            Ok(evict(c, repo, REPO_CAP)?)
        })
        .await?;
    for g in gone {
        let _ = tokio::fs::remove_file(path(&st.cfg.data_dir, g)).await;
    }
    Ok(true)
}

/// Hourly: drop caches unused for CACHE_DAYS, and files with no row (their
/// repository was deleted, or they were evicted mid-write).
pub async fn sweep(st: &AppState) -> R<usize> {
    let dir = st.cfg.data_dir.join("ci/cache");
    // Files first, rows second: a file written after the listing isn't judged.
    let files: Vec<(PathBuf, Option<i64>)> = match std::fs::read_dir(&dir) {
        Ok(rd) => rd.flatten().map(|e| (e.path(), e.file_name().to_string_lossy().strip_suffix(".tar.gz").and_then(|n| n.parse().ok()))).collect(),
        Err(_) => vec![],
    };
    let cutoff = now() - CACHE_DAYS * 86400;
    let known: std::collections::HashSet<i64> = st
        .db
        .call(move |c| {
            c.execute("DELETE FROM ci_caches WHERE used_at < ?1", [cutoff])?;
            let mut q = c.prepare("SELECT id FROM ci_caches")?;
            Ok(q.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?)
        })
        .await?;
    let mut gone = 0;
    for (p, id) in files {
        if !id.is_some_and(|id| known.contains(&id)) && tokio::fs::remove_file(&p).await.is_ok() {
            gone += 1;
        }
    }
    Ok(gone)
}
