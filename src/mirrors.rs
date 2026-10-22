//! Repository mirrors. A pull mirror fetches its remote's branches and tags on
//! a schedule (and on "sync now"), and the repository is read-only here while
//! it has one. Push mirrors push the repository's branches and tags to their
//! remote after every change. Each `mirrors` row is its own queue entry
//! (`next_at`), so a burst of pushes becomes one mirror push.
//!
//! git does the talking, with the password or token only in its environment
//! (an `Authorization` header from GIT_CONFIG_*), never in its arguments, the
//! URL or the errors kept. The remote's host is looked up and checked here
//! (`net::pin`) and git connects to exactly those addresses; no redirects, http(s) only.
//!
//! A pull mirror's sync is not a push: no CI runs, webhooks or push events. It
//! refreshes what pushes refresh (languages, size, the commit-graph) and
//! triggers the repository's own push mirrors.

use std::path::Path;
use std::time::Duration;

use base64::Engine;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use crate::AppState;
use crate::db::now;
use crate::error::{AppError, R};
use crate::repo::Repo;

/// Pull mirrors sync at most this often...
pub const MIN_INTERVAL: i64 = 600;
/// ...and at least this often.
pub const MAX_INTERVAL: i64 = 7 * 86400;
/// The longest wait after failures.
pub const MAX_BACKOFF: i64 = 86400;
/// A fetch or push that takes longer is stopped.
const TIMEOUT: Duration = Duration::from_secs(1800);
/// A sync taken by a server that stopped mid-way is tried again after this (past TIMEOUT
/// and the commit-graph refresh after a fetch).
const LEASE: i64 = 3600;
/// Syncs running at once.
const PARALLEL: usize = 4;
/// Characters of git's complaint kept as the last error.
const KEEP_ERROR: usize = 1000;

pub const READ_ONLY: &str = "This repository is a mirror: it changes only when it syncs from its remote. Remove the mirror in Settings → Mirrors to change it here.";

/// Refuse a change to a pull mirror's branches or tags (git pushes, web
/// commits, merges, reverts, suggestions, new branches and tags, deletions).
pub fn refuse_writes(repo: &Repo) -> R<()> {
    if repo.mirror {
        return Err(AppError::new(axum::http::StatusCode::FORBIDDEN, READ_ONLY));
    }
    Ok(())
}

/// The wait before the next try after `failures` failures in a row: doubling
/// from `base` (10 minutes for pull mirrors, 1 minute for push mirrors), at most a day.
pub fn backoff(base: i64, failures: i64) -> i64 {
    (base << (failures - 1).clamp(0, 20)).min(MAX_BACKOFF)
}

pub struct Remote {
    pub url: String,
    pub username: String,
    pub password: String,
    /// Set by a site admin: loopback and private addresses may be reached.
    pub allow_private: bool,
}

/// `git` for repository `repo`, set up to talk to `remote`: http(s) only, no
/// redirects, the host pinned to the addresses checked, a stall limit, and the
/// credentials in the environment only. Callers add the subcommand.
pub async fn git(st: &AppState, repo: &Path, remote: &Remote) -> Result<tokio::process::Command, String> {
    let pinned = if remote.allow_private {
        crate::net::check_url(&remote.url)?;
        None
    } else {
        let url = remote.url.clone();
        tokio::task::spawn_blocking(move || crate::net::pin(&url)).await.map_err(|e| e.to_string())??
    };
    let mut cmd = st.git.command(Some(repo));
    for kv in [
        "protocol.allow=never",
        "protocol.http.allow=always",
        "protocol.https.allow=always",
        "protocol.file.allow=never",
        "http.followRedirects=false",
        // Under 1 KB/s for a minute is a stall.
        "http.lowSpeedLimit=1024",
        "http.lowSpeedTime=60",
    ] {
        cmd.arg("-c").arg(kv);
    }
    if let Some(p) = pinned {
        cmd.arg("-c").arg(format!("http.curloptResolve={p}"));
    }
    if let Some(h) = auth_header(remote) {
        cmd.env("GIT_CONFIG_COUNT", "1").env("GIT_CONFIG_KEY_0", "http.extraHeader").env("GIT_CONFIG_VALUE_0", h);
    }
    Ok(cmd)
}

fn basic(remote: &Remote) -> String {
    base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", remote.username, remote.password))
}

fn auth_header(remote: &Remote) -> Option<String> {
    (!remote.username.is_empty() || !remote.password.is_empty()).then(|| format!("Authorization: Basic {}", basic(remote)))
}

/// git's complaint as kept and shown: the credentials blanked out (git
/// shouldn't print them, but a remote could echo them back), the end of it only.
fn clean(err: &str, remote: &Remote) -> String {
    let mut e = err.replace('\r', "\n");
    for s in [remote.password.clone(), basic(remote)] {
        if !s.is_empty() {
            e = e.replace(&s, "•••");
        }
    }
    let e = e.trim();
    let n = e.chars().count();
    let e: String = if n > KEEP_ERROR { e.chars().skip(n - KEEP_ERROR).collect() } else { e.to_string() };
    if e.is_empty() { "git failed without saying why".into() } else { e }
}

/// Run a prepared git command to its end, within TIMEOUT. `cap`: a fetch into
/// `repo` is stopped once the packs it's writing pass this many bytes.
async fn exec(st: &AppState, mut cmd: tokio::process::Command, cap: Option<(&Path, u64)>) -> Result<(), String> {
    let _permit = st.git.permit_stream().await.map_err(|e| e.msg)?;
    // Its own process group, so stopping it stops the helpers it started too.
    cmd.process_group(0).stdout(std::process::Stdio::null());
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let mut stderr = child.stderr.take().ok_or("no stderr")?;
    let errs = tokio::spawn(async move {
        let mut v = Vec::new();
        let _ = tokio::io::AsyncReadExt::read_to_end(&mut stderr, &mut v).await;
        v
    });
    let packs = cap.map(|(repo, max)| (repo.join("objects/pack"), max));
    let start = packs.as_ref().map_or(0, |(d, _)| pack_bytes(d));
    let deadline = tokio::time::Instant::now() + TIMEOUT;
    let stop = loop {
        tokio::select! {
            status = child.wait() => {
                let status = status.map_err(|e| e.to_string())?;
                let err = errs.await.unwrap_or_default();
                return if status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&err).into_owned()) };
            }
            _ = tokio::time::sleep(Duration::from_secs(2)) => {
                if tokio::time::Instant::now() > deadline {
                    break format!("Stopped after {} minutes", TIMEOUT.as_secs() / 60);
                }
                if let Some((d, max)) = &packs && pack_bytes(d).saturating_sub(start) > *max {
                    break "Stopped: the remote sent more than the storage left on this account".to_string();
                }
            }
        }
    };
    if let Some(pid) = child.id() {
        let _ = tokio::process::Command::new("kill").args(["-KILL", &format!("-{pid}")]).status().await;
    }
    let _ = child.kill().await;
    // What a stopped fetch leaves behind.
    if let Some((d, _)) = &packs
        && let Ok(rd) = std::fs::read_dir(d)
    {
        for e in rd.flatten() {
            if e.file_name().to_string_lossy().starts_with("tmp_") {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    Err(stop)
}

/// Bytes in a repository's pack folder (where a fetch writes what it receives).
fn pack_bytes(dir: &Path) -> u64 {
    std::fs::read_dir(dir).map(|rd| rd.flatten().filter_map(|e| e.metadata().ok()).map(|m| m.len()).sum()).unwrap_or(0)
}

// ---------------------------------------------------------------- the queue

struct Job {
    id: i64,
    repo_id: i64,
    pull: bool,
    url: String,
    username: String,
    secret: Option<Vec<u8>>,
    allow_private: bool,
    failures: i64,
}

/// Take up to `limit` due mirrors, leased. Those of deleted or moderator-hidden
/// repositories and suspended owners wait (and resume once that's over).
fn claim(c: &mut Connection, limit: usize) -> rusqlite::Result<Vec<Job>> {
    let tx = c.transaction()?;
    let t = now();
    let jobs: Vec<Job> = {
        let mut q = tx.prepare_cached(
            "SELECT m.id, m.repo_id, m.direction = 'pull', m.url, m.username, m.secret, m.allow_private, m.failures
             FROM mirrors m JOIN repos r ON r.id = m.repo_id JOIN users u ON u.id = r.owner_id
             WHERE coalesce(m.running_until, 0) < ?1 AND (m.next_at <= ?1 OR m.running_until IS NOT NULL)
               AND r.deleted_at IS NULL AND r.hidden = 0 AND u.suspended = 0
             ORDER BY coalesce(m.next_at, 0), m.id LIMIT ?2",
        )?;
        q.query_map(params![t, limit as i64], |r| {
            Ok(Job {
                id: r.get(0)?,
                repo_id: r.get(1)?,
                pull: r.get(2)?,
                url: r.get(3)?,
                username: r.get(4)?,
                secret: r.get(5)?,
                allow_private: r.get(6)?,
                failures: r.get(7)?,
            })
        })?
        .collect::<Result<_, _>>()?
    };
    for j in &jobs {
        tx.execute("UPDATE mirrors SET running_until = ?2, next_at = NULL, last_tried_at = ?3 WHERE id = ?1", params![j.id, t + LEASE, t])?;
    }
    tx.commit()?;
    Ok(jobs)
}

/// Record how a sync went. A pull mirror is next due an interval later (or
/// sooner, if "sync now" was pressed meanwhile); a push mirror when the next
/// change comes (one that came meanwhile is already waiting). Failures wait longer each time.
fn finish(c: &Connection, j: &Job, result: Result<(), String>) -> rusqlite::Result<()> {
    let t = now();
    match result {
        Ok(()) => {
            c.execute(
                "UPDATE mirrors SET running_until = NULL, failures = 0, last_ok_at = ?2, last_error = '',
                 next_at = CASE direction WHEN 'pull' THEN coalesce(next_at, ?2 + interval_secs) ELSE next_at END WHERE id = ?1",
                params![j.id, t],
            )?;
            // A one-time import is done: the repository is an ordinary one from here on (its password goes too).
            c.execute("DELETE FROM mirrors WHERE id = ?1 AND once = 1", [j.id])?
        }
        Err(e) => {
            let wait = backoff(if j.pull { 600 } else { 60 }, j.failures + 1);
            c.execute(
                "UPDATE mirrors SET running_until = NULL, failures = failures + 1, last_error = ?2, next_at = ?3 WHERE id = ?1",
                params![j.id, e, t + wait],
            )?
        }
    };
    Ok(())
}

async fn sync(st: &AppState, j: Job) {
    let password = match &j.secret {
        None => Some(String::new()),
        Some(s) => crate::security::open(&st.key, s).and_then(|p| String::from_utf8(p).ok()),
    };
    let result = match password {
        None => Err("The password can't be decrypted (was GITTER_DATA/secret.key replaced?). Set it again in this mirror's settings.".to_string()),
        Some(password) => {
            let remote = Remote { url: j.url.clone(), username: j.username.clone(), password, allow_private: j.allow_private };
            let r = if j.pull { pull(st, j.repo_id, &remote).await } else { push(st, j.repo_id, &remote).await };
            r.map_err(|e| clean(&e, &remote))
        }
    };
    let id = j.id;
    if let Err(e) = st.db.call(move |c| Ok(finish(c, &j, result)?)).await {
        eprintln!("mirrors: {id}: {}", e.msg);
    }
}

async fn load_repo(st: &AppState, id: i64) -> Result<Repo, String> {
    let sql = format!("{} WHERE r.id = ?1", crate::repo::REPO_SELECT);
    let repo = st.db.call(move |c| Ok(c.query_row(&sql, [id], crate::repo::row_to_repo).optional()?)).await.map_err(|e| e.msg)?;
    Ok(Repo::with_access(st, repo.ok_or("The repository is gone")?, None))
}

/// Fetch the remote's branches and tags over ours (those gone there go here too).
async fn pull(st: &AppState, repo_id: i64, remote: &Remote) -> Result<(), String> {
    let repo = load_repo(st, repo_id).await?;
    // git has no fetch-size limit to pass, so exec watches what the fetch writes.
    let left = crate::repo::storage_left(st, repo.owner_id).await.map_err(|e| e.msg)?;
    if left == Some(0) {
        return Err("This account is out of storage, so nothing more is fetched".into());
    }
    let before = crate::smart_http::ref_map(st, &repo).await.map_err(|e| e.msg)?;
    let mut cmd = git(st, &repo.path, remote).await?;
    // Objects from elsewhere are checked like pushed ones.
    cmd.args(["-c", "fetch.fsckObjects=true", "fetch", "--quiet", "--prune", "--force", "--no-write-fetch-head", "--no-auto-gc", &remote.url, "+refs/heads/*:refs/heads/*", "+refs/tags/*:refs/tags/*"]);
    exec(st, cmd, left.map(|l| (repo.path.as_path(), l))).await?;
    let after = crate::smart_http::ref_map(st, &repo).await.map_err(|e| e.msg)?;
    if after == before {
        return Ok(());
    }
    if let Err(e) = crate::smart_http::fix_head(st, &repo, &after).await {
        eprintln!("mirrors: HEAD of {}: {}", repo.full_name(), e.msg);
    }
    let gone: Vec<String> = before.keys().filter(|r| !after.contains_key(*r)).filter_map(|r| r.strip_prefix("refs/heads/")).map(String::from).collect();
    if !gone.is_empty() {
        crate::hooks::forget_branches(st, repo_id, gone).await;
    }
    crate::hooks::after_import(st, repo_id).await;
    changed(st, repo_id).await;
    Ok(())
}

/// Push our branches and tags over the remote's (those gone here go there too).
async fn push(st: &AppState, repo_id: i64, remote: &Remote) -> Result<(), String> {
    let path = st.git.repo_path(repo_id);
    let mut cmd = git(st, &path, remote).await?;
    cmd.args(["push", "--quiet", "--force", "--prune", &remote.url, "+refs/heads/*:refs/heads/*", "+refs/tags/*:refs/tags/*"]);
    exec(st, cmd, None).await
}

/// Repository `repo_id` changed: its push mirrors are due (unless one is
/// failing, and waits out its backoff). Called by `hooks::on_push` and pull syncs.
pub async fn changed(st: &AppState, repo_id: i64) {
    let r = st
        .db
        .call(move |c| {
            Ok(c.execute(
                "UPDATE mirrors SET next_at = ?2 WHERE repo_id = ?1 AND direction = 'push' AND (next_at IS NULL OR (failures = 0 AND next_at > ?2))",
                params![repo_id, now()],
            )?)
        })
        .await;
    if let Err(e) = r {
        eprintln!("mirrors: {}", e.msg);
    }
}

/// Sync every due mirror once, waiting for them (tests and tools; the server
/// uses `run`). Returns how many were synced.
pub async fn tick(st: &AppState) -> usize {
    let jobs = match st.db.call(|c| Ok(claim(c, 100)?)).await {
        Ok(j) => j,
        Err(e) => {
            eprintln!("mirrors: {}", e.msg);
            return 0;
        }
    };
    let n = jobs.len();
    futures_util::future::join_all(jobs.into_iter().map(|j| sync(st, j))).await;
    n
}

/// The sync loop, for as long as the server runs: due mirrors are looked for
/// every few seconds, PARALLEL at a time.
// ponytail: polls every 5 s (a push mirror starts within 5 s); add a Notify on AppState if that's too slow.
pub async fn run(st: AppState) {
    let workers = std::sync::Arc::new(tokio::sync::Semaphore::new(PARALLEL));
    loop {
        let room = workers.available_permits();
        if room > 0 {
            match st.db.call(move |c| Ok(claim(c, room)?)).await {
                Ok(jobs) => {
                    for j in jobs {
                        let Ok(permit) = workers.clone().try_acquire_owned() else { break };
                        let st = st.clone();
                        tokio::spawn(async move {
                            sync(&st, j).await;
                            drop(permit);
                        });
                    }
                }
                Err(e) => eprintln!("mirrors: {}", e.msg),
            }
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

// ---------------------------------------------------------------- shown elsewhere

/// A pull mirror's remote and last sync, for the badge on its pages. Only the
/// repository's admins (`full`) see the whole URL; others see it without a query
/// string (tokens hide there), and nothing of a private address.
pub async fn badge(st: &AppState, repo_id: i64, full: bool) -> R<Value> {
    st.db
        .call(move |c| {
            Ok(c.query_row("SELECT url, last_ok_at, allow_private, once, last_error FROM mirrors WHERE repo_id = ?1 AND direction = 'pull'", [repo_id], |r| {
                let (url, private, once, error): (String, bool, bool, String) = (r.get(0)?, r.get(2)?, r.get(3)?, r.get(4)?);
                let shown = if full {
                    url
                } else if private {
                    "a private server".to_string()
                } else {
                    url.split(['?', '#']).next().unwrap_or_default().to_string()
                };
                // Why the last sync failed, for those who can fix it.
                let error = if full { error } else { String::new() };
                Ok(json!({ "url": shown, "last_ok_at": r.get::<_, Option<i64>>(1)?, "once": once, "error": error }))
            })
            .optional()?
            .unwrap_or(Value::Null))
        })
        .await
}

/// Mirrors failing across the site, worst first (the admin overview).
pub fn failing(c: &Connection) -> rusqlite::Result<Vec<Value>> {
    let mut q = c.prepare(
        "SELECT u.username, r.name, m.direction, m.url, m.failures, m.last_error, m.last_tried_at, m.last_ok_at
         FROM mirrors m JOIN repos r ON r.id = m.repo_id JOIN users u ON u.id = r.owner_id
         WHERE m.failures > 0 AND r.deleted_at IS NULL ORDER BY m.failures DESC, m.id LIMIT 50",
    )?;
    q.query_map([], |r| {
        Ok(json!({
            "owner": r.get::<_, String>(0)?, "name": r.get::<_, String>(1)?, "direction": r.get::<_, String>(2)?, "url": r.get::<_, String>(3)?,
            "failures": r.get::<_, i64>(4)?, "last_error": r.get::<_, String>(5)?, "last_tried_at": r.get::<_, Option<i64>>(6)?,
            "last_ok_at": r.get::<_, Option<i64>>(7)?,
        }))
    })?
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_up_to_a_day() {
        assert_eq!([1, 2, 3].map(|f| backoff(600, f)), [600, 1200, 2400]);
        assert_eq!(backoff(60, 1), 60);
        assert_eq!(backoff(600, 9), MAX_BACKOFF);
        assert_eq!(backoff(600, 1000), MAX_BACKOFF, "no overflow");
    }

    #[test]
    fn errors_never_keep_credentials() {
        let r = Remote { url: "https://example.com/x.git".into(), username: "me".into(), password: "hunter2-token".into(), allow_private: false };
        let e = clean(&format!("remote: bad hunter2-token\r{}\n", basic(&r)), &r);
        assert!(!e.contains("hunter2") && !e.contains(&basic(&r)), "{e}");
        assert_eq!(clean(&"x".repeat(5000), &r).len(), KEEP_ERROR);
        assert!(auth_header(&Remote { username: String::new(), password: String::new(), ..r }).is_none());
        let short = Remote { url: "https://example.com/x.git".into(), username: "me".into(), password: "pw1".into(), allow_private: false };
        assert!(!clean("remote: wrong pw1", &short).contains("pw1"), "short passwords are blanked too");
    }

    #[tokio::test]
    async fn a_fetch_past_the_storage_left_is_stopped_and_its_leftovers_removed() {
        let dir = std::env::temp_dir().join(format!("gitter-cap-{}", std::process::id()));
        let pack = dir.join("objects/pack");
        std::fs::create_dir_all(&pack).unwrap();
        let cfg = crate::Config {
            data_dir: dir.clone(),
            addr: String::new(),
            url: "http://localhost".into(),
            secure_cookies: false,
            trust_proxy: false,
            signup: crate::SignupMode::Open,
            max_push_bytes: 1,
            max_repos_per_user: 1,
        };
        let st = AppState::new(cfg, crate::db::Db::memory().unwrap());
        // Stands in for a fetch that keeps receiving.
        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-c").arg(format!("head -c 3000000 /dev/zero > {}/tmp_pack_x; exec tail -f /dev/null", pack.display())).stderr(std::process::Stdio::piped());
        let t = std::time::Instant::now();
        let e = exec(&st, cmd, Some((dir.as_path(), 1_000_000))).await.unwrap_err();
        assert!(e.contains("more than the storage left") && t.elapsed() < Duration::from_secs(15), "{e}");
        assert_eq!(pack_bytes(&pack), 0, "the partial pack is gone");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
