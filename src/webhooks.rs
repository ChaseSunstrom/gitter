//! Repository webhooks. An event becomes one row per interested hook in
//! `webhook_deliveries`; that table is both the queue (so retries survive a
//! restart) and the delivery log. A background loop POSTs the rows that are
//! due, signed with the hook's secret, through `net::post`.

use std::time::Duration;

use hmac::{Hmac, Mac};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use crate::AppState;
use crate::db::now;
use crate::git::{Commit, LogQuery};
use crate::repo::Repo;
use crate::smart_http::RefUpdate;

/// Events a hook can ask for; "*" means all of them. Every hook also gets `ping`.
pub const EVENTS: &[&str] = &["push", "issues", "issue_comment", "pull_request", "ci_run"];
/// The wait before each retry of a failed delivery; after the last one it's given up.
pub const BACKOFF: [i64; 5] = [60, 300, 1800, 7200, 21600];
/// Finished deliveries kept per hook: its log.
pub const KEEP: i64 = 50;
/// Deliveries waiting per hook. Past this new events are dropped, so a dead
/// receiver can't grow the table without bound.
pub const MAX_WAITING: i64 = 100;
/// A delivery taken by a server that stopped mid-send is tried again after this.
const LEASE: i64 = 120;
/// Deliveries sent at once.
const PARALLEL: usize = 8;
/// Bytes of each reply kept for the log.
const KEEP_REPLY: usize = 2048;
/// A push moving more refs than this sends events for the first ones only.
const MAX_REFS: usize = 20;

pub fn wants(events: &str, event: &str) -> bool {
    events == "*" || events.split(',').any(|e| e == event)
}

/// `sha256=<hex HMAC-SHA256 of body>`, the X-Gitter-Signature-256 header.
pub fn sign(secret: &[u8], body: &[u8]) -> String {
    let mut m = Hmac::<sha2::Sha256>::new_from_slice(secret).expect("HMAC takes keys of any length");
    m.update(body);
    format!("sha256={}", hex::encode(m.finalize().into_bytes()))
}

/// A random (version 4) UUID.
fn uuid() -> String {
    let mut b: [u8; 16] = rand::random();
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h = hex::encode(b);
    format!("{}-{}-{}-{}-{}", &h[..8], &h[8..12], &h[12..16], &h[16..20], &h[20..])
}

// ---------------------------------------------------------------- queueing events

/// Queue `event` for repository `repo_id`'s active hooks that want it (or for
/// hook `only`, whatever it wants). `build` makes the payload from the
/// `repository` object and runs only when some hook will get it; `repository`
/// and `sender` are then added. Runs inside the caller's connection (or
/// transaction). Returns how many deliveries were queued.
pub fn enqueue(
    c: &Connection,
    base: &str,
    repo_id: i64,
    sender: Option<i64>,
    event: &str,
    only: Option<i64>,
    build: impl FnOnce(&Connection, &Value) -> rusqlite::Result<Option<Value>>,
) -> rusqlite::Result<usize> {
    let hooks: Vec<i64> = {
        let mut q = c.prepare_cached("SELECT id, events FROM webhooks WHERE repo_id = ?1 AND active = 1")?;
        let rows = q.query_map([repo_id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
        let rows: Vec<(i64, String)> = rows.collect::<Result<_, _>>()?;
        rows.into_iter().filter(|(id, ev)| only.map_or_else(|| wants(ev, event), |o| o == *id)).map(|(id, _)| id).collect()
    };
    if hooks.is_empty() {
        return Ok(0);
    }
    let repo = repo_json(c, base, repo_id)?;
    let Some(mut payload) = build(c, &repo)? else { return Ok(0) };
    payload["repository"] = repo;
    payload["sender"] = match sender {
        Some(id) => c.query_row("SELECT username FROM users WHERE id = ?1", [id], |r| Ok(json!({ "id": id, "username": r.get::<_, String>(0)? }))).optional()?.unwrap_or(Value::Null),
        None => Value::Null,
    };
    let (body, t, mut n) = (payload.to_string(), now(), 0);
    for h in hooks {
        let queued = c.execute(
            "INSERT INTO webhook_deliveries(hook_id, guid, event, payload, next_at, created_at)
             SELECT ?1, ?2, ?3, ?4, ?5, ?5 WHERE (SELECT count(*) FROM webhook_deliveries WHERE hook_id = ?1 AND next_at IS NOT NULL) < ?6",
            params![h, uuid(), event, body, t, MAX_WAITING],
        )?;
        if queued == 0 {
            // Logged where the hook's admins look, without the payload (it's what would pile up).
            c.execute(
                "INSERT INTO webhook_deliveries(hook_id, guid, event, payload, status, error, next_at, created_at)
                 VALUES (?1, ?2, ?3, '', 'failed', ?4, NULL, ?5)",
                params![h, uuid(), event, format!("Dropped: {MAX_WAITING} deliveries were already waiting for this receiver"), t],
            )?;
        }
        n += queued;
    }
    Ok(n)
}

/// Run `f` (which queues deliveries) on the database thread and wake the
/// sender. Problems are logged, never handed to the request that caused them.
async fn queued(st: &AppState, f: impl FnOnce(&Connection, &str) -> rusqlite::Result<usize> + Send + 'static) -> usize {
    let base = st.cfg.url.clone();
    match st.db.call(move |c| Ok(f(c, &base)?)).await {
        Ok(n) => {
            if n > 0 {
                st.hook_wake.notify_one();
            }
            n
        }
        Err(e) => {
            eprintln!("webhooks: {}", e.msg);
            0
        }
    }
}

fn repo_json(c: &Connection, base: &str, id: i64) -> rusqlite::Result<Value> {
    c.query_row(
        "SELECT u.username, r.name, r.description, r.is_private, r.default_branch FROM repos r JOIN users u ON u.id = r.owner_id WHERE r.id = ?1",
        [id],
        |r| {
            let (owner, name): (String, String) = (r.get(0)?, r.get(1)?);
            let url = format!("{base}/{owner}/{name}");
            Ok(json!({
                "id": id, "full_name": format!("{owner}/{name}"), "owner": owner, "name": name,
                "description": r.get::<_, String>(2)?, "private": r.get::<_, bool>(3)?, "default_branch": r.get::<_, String>(4)?,
                "clone_url": format!("{url}.git"), "url": url,
            }))
        },
    )
}

/// Issue or pull request `number` as payloads show it, and whether it's a pull request.
fn issue_json(c: &Connection, repo: &Value, repo_id: i64, number: i64) -> rusqlite::Result<Option<(bool, Value)>> {
    let row = c
        .query_row(
            "SELECT i.id, i.title, i.body, i.state, i.is_pr, u.username, i.created_at, i.updated_at, i.closed_at, p.head_branch, p.base_branch, p.merged_sha
             FROM issues i JOIN users u ON u.id = i.author_id LEFT JOIN pulls p ON p.issue_id = i.id WHERE i.repo_id = ?1 AND i.number = ?2",
            [repo_id, number],
            |r| {
                let (is_pr, state): (bool, String) = (r.get(4)?, r.get(3)?);
                let url = format!("{}/{}/{number}", repo["url"].as_str().unwrap_or_default(), if is_pr { "pull" } else { "issues" });
                let mut v = json!({
                    "number": number, "title": r.get::<_, String>(1)?, "body": clip(r.get::<_, String>(2)?), "state": state,
                    "user": { "username": r.get::<_, String>(5)? }, "labels": [],
                    "created_at": r.get::<_, i64>(6)?, "updated_at": r.get::<_, i64>(7)?, "closed_at": r.get::<_, Option<i64>>(8)?, "url": url,
                });
                if is_pr {
                    v["head"] = json!(r.get::<_, Option<String>>(9)?);
                    v["base"] = json!(r.get::<_, Option<String>>(10)?);
                    v["merged"] = json!(state == "merged");
                    v["merge_commit_sha"] = json!(r.get::<_, Option<String>>(11)?);
                }
                Ok((r.get::<_, i64>(0)?, is_pr, v))
            },
        )
        .optional()?;
    let Some((id, is_pr, mut v)) = row else { return Ok(None) };
    let mut q = c.prepare_cached("SELECT l.name FROM issue_labels il JOIN labels l ON l.id = il.label_id WHERE il.issue_id = ?1 ORDER BY l.name")?;
    v["labels"] = json!(q.query_map([id], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?);
    Ok(Some((is_pr, v)))
}

/// An issue (`issues`) or pull request (`pull_request`) was opened, edited,
/// closed, reopened or merged.
pub async fn issue(st: &AppState, repo_id: i64, sender: i64, number: i64, is_pr: bool, action: &'static str) {
    let event = if is_pr { "pull_request" } else { "issues" };
    queued(st, move |c, base| {
        enqueue(c, base, repo_id, Some(sender), event, None, |c, repo| {
            Ok(issue_json(c, repo, repo_id, number)?.map(|(pr, v)| {
                let mut p = json!({ "action": action });
                p[if pr { "pull_request" } else { "issue" }] = v;
                p
            }))
        })
    })
    .await;
}

/// Queue `issue_comment` for comment `id` (created, edited or deleted: call it
/// before the comment is deleted, in the same transaction).
pub fn comment_now(c: &Connection, base: &str, repo_id: i64, sender: i64, id: i64, action: &str) -> rusqlite::Result<usize> {
    enqueue(c, base, repo_id, Some(sender), "issue_comment", None, |c, repo| {
        let found = c
            .query_row(
                "SELECT i.number, c.body, u.username, c.created_at, c.updated_at FROM comments c JOIN issues i ON i.id = c.issue_id
                 JOIN users u ON u.id = c.author_id WHERE c.id = ?1 AND i.repo_id = ?2",
                [id, repo_id],
                |r| {
                    let comment = json!({ "id": id, "body": clip(r.get::<_, String>(1)?), "user": { "username": r.get::<_, String>(2)? },
                        "created_at": r.get::<_, i64>(3)?, "updated_at": r.get::<_, i64>(4)? });
                    Ok((r.get::<_, i64>(0)?, comment))
                },
            )
            .optional()?;
        let Some((number, comment)) = found else { return Ok(None) };
        let Some((is_pr, mut issue)) = issue_json(c, repo, repo_id, number)? else { return Ok(None) };
        issue["is_pull_request"] = json!(is_pr);
        Ok(Some(json!({ "action": action, "issue": issue, "comment": comment })))
    })
}

pub async fn comment(st: &AppState, repo_id: i64, sender: i64, id: i64, action: &'static str) {
    queued(st, move |c, base| comment_now(c, base, repo_id, sender, id, action)).await;
}

/// Send hook `hook` a `ping`. False when nothing was queued (it's off, or too much is waiting).
pub async fn ping(st: &AppState, repo_id: i64, hook: i64, sender: i64) -> bool {
    let n = queued(st, move |c, base| {
        enqueue(c, base, repo_id, Some(sender), "ping", Some(hook), |c, _| {
            let events: String = c.query_row("SELECT events FROM webhooks WHERE id = ?1", [hook], |r| r.get(0))?;
            Ok(Some(json!({ "hook": { "id": hook, "events": events.split(',').collect::<Vec<_>>() } })))
        })
    })
    .await;
    n > 0
}

/// Characters of a commit message a payload carries (git allows any length).
const MAX_MESSAGE: usize = 8192;
/// Issue, PR and comment bodies in payloads are cut here (each payload is stored once per hook).
const MAX_BODY: usize = 16384;

fn clip(s: String) -> String {
    if s.len() <= MAX_BODY { s } else { s.chars().take(MAX_BODY).collect() }
}

fn commit_json(repo_url: &str, c: &Commit) -> Value {
    let full = if c.body.is_empty() { c.subject.clone() } else { format!("{}\n\n{}", c.subject, c.body) };
    let message: String = full.chars().take(MAX_MESSAGE).collect();
    json!({ "id": c.sha, "message": message, "timestamp": c.author_time,
        "author": { "name": c.author_name, "email": c.author_email }, "url": format!("{repo_url}/commit/{}", c.sha) })
}

/// After a push, a web commit or a merge: one `push` event per moved ref, with
/// up to 20 of the commits it added, newest first (a new branch lists its head only).
pub async fn push(st: &AppState, repo: &Repo, sender: i64, updates: &[RefUpdate]) {
    let id = repo.id;
    let wanted = st
        .db
        .call(move |c| {
            let mut q = c.prepare_cached("SELECT events FROM webhooks WHERE repo_id = ?1 AND active = 1")?;
            let all: Vec<String> = q.query_map([id], |r| r.get(0))?.collect::<Result<_, _>>()?;
            Ok(all.iter().any(|e| wants(e, "push")))
        })
        .await
        .unwrap_or(false);
    if !wanted {
        return;
    }
    let url = format!("{}/{}", st.cfg.url, repo.full_name());
    for u in updates.iter().take(MAX_REFS) {
        let commits = match (&u.old, &u.new) {
            (Some(old), Some(new)) => {
                let range = format!("{old}..{new}");
                let q = LogQuery { range: &range, path: None, skip: 0, limit: 20, grep: None, author: None, first_parent: false };
                st.git.log(&repo.path, &q).await.unwrap_or_default()
            }
            (None, Some(new)) => st.git.commit_info(&repo.path, new).await.ok().flatten().into_iter().collect(),
            _ => vec![],
        };
        let commits: Vec<Value> = commits.iter().map(|c| commit_json(&url, c)).collect();
        let p = json!({ "ref": u.refname, "before": u.old, "after": u.new, "created": u.old.is_none(), "deleted": u.new.is_none(),
            "head_commit": commits.first().cloned(), "commits": commits });
        queued(st, move |c, base| enqueue(c, base, id, Some(sender), "push", None, |_, _| Ok(Some(p)))).await;
    }
}

/// Queue a `ci_run` event for each CI run that finished since the last look.
/// Runs finish in several places (a runner's report, cancel, lost jobs, broken
/// configs); looking for them here catches every one, even across a restart.
fn ci_runs(c: &Connection, base: &str) -> rusqlite::Result<usize> {
    let runs: Vec<(i64, i64, Option<i64>)> = {
        let mut q = c.prepare_cached("SELECT id, repo_id, actor_id FROM ci_runs WHERE finished_at IS NOT NULL AND finished_at IS NOT hooked_at ORDER BY id LIMIT 100")?;
        q.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<Result<_, _>>()?
    };
    let mut n = 0;
    for (id, repo_id, actor) in runs {
        // Marked first, so a run whose event can't be built isn't tried forever.
        c.execute("UPDATE ci_runs SET hooked_at = finished_at WHERE id = ?1", [id])?;
        let built = enqueue(c, base, repo_id, actor, "ci_run", None, |c, repo| {
            c.query_row(
                "SELECT number, event, ref, sha, pr_number, title, status, error, created_at, started_at, finished_at FROM ci_runs WHERE id = ?1",
                [id],
                |r| {
                    let number: i64 = r.get(0)?;
                    Ok(json!({ "action": "completed", "run": {
                        "number": number, "event": r.get::<_, String>(1)?, "ref": r.get::<_, String>(2)?, "sha": r.get::<_, String>(3)?,
                        "pr_number": r.get::<_, Option<i64>>(4)?, "title": r.get::<_, String>(5)?, "status": r.get::<_, String>(6)?,
                        "error": r.get::<_, String>(7)?, "created_at": r.get::<_, i64>(8)?, "started_at": r.get::<_, Option<i64>>(9)?,
                        "finished_at": r.get::<_, Option<i64>>(10)?, "url": format!("{}/ci/{number}", repo["url"].as_str().unwrap_or_default()),
                    }}))
                },
            )
            .optional()
        });
        match built {
            Ok(k) => n += k,
            Err(e) => eprintln!("webhooks: ci run {id}: {e}"),
        }
    }
    Ok(n)
}

// ---------------------------------------------------------------- sending

struct Due {
    id: i64,
    guid: String,
    event: String,
    payload: String,
    url: String,
    secret: Vec<u8>,
    allow_private: bool,
}

/// Take up to `limit` due deliveries, leased: a server that stops mid-send tries them again later.
fn claim(c: &mut Connection, limit: usize) -> rusqlite::Result<Vec<Due>> {
    let tx = c.transaction()?;
    let t = now();
    tx.execute(
        "UPDATE webhook_deliveries SET next_at = NULL, status = 'failed', error = 'The webhook was turned off'
         WHERE next_at <= ?1 AND hook_id IN (SELECT id FROM webhooks WHERE active = 0)",
        [t],
    )?;
    // A suspended owner's or a moderator-hidden repository's hooks don't send.
    tx.execute(
        "UPDATE webhook_deliveries SET next_at = NULL, status = 'failed', error = 'Not sent: the repository is suspended or hidden by a moderator'
         WHERE next_at <= ?1 AND hook_id IN (SELECT h.id FROM webhooks h JOIN repos r ON r.id = h.repo_id JOIN users u ON u.id = r.owner_id
             WHERE u.suspended = 1 OR r.hidden = 1 OR r.deleted_at IS NOT NULL)",
        [t],
    )?;
    let due: Vec<Due> = {
        let mut q = tx.prepare_cached(
            // The oldest due delivery of each hook, and none of a hook that's sending one now,
            // so one slow receiver only ever holds one worker.
            "SELECT d.id, d.guid, d.event, d.payload, h.url, h.secret, h.allow_private FROM webhook_deliveries d
             JOIN webhooks h ON h.id = d.hook_id
             WHERE d.id IN (SELECT min(id) FROM webhook_deliveries WHERE next_at <= ?1 GROUP BY hook_id)
               AND NOT EXISTS (SELECT 1 FROM webhook_deliveries x WHERE x.hook_id = d.hook_id AND x.status = 'sending' AND x.next_at > ?1)
             ORDER BY d.next_at, d.id LIMIT ?2",
        )?;
        q.query_map(params![t, limit as i64], |r| {
            Ok(Due { id: r.get(0)?, guid: r.get(1)?, event: r.get(2)?, payload: r.get(3)?, url: r.get(4)?, secret: r.get(5)?, allow_private: r.get(6)? })
        })?
        .collect::<Result<_, _>>()?
    };
    for d in &due {
        tx.execute("UPDATE webhook_deliveries SET next_at = ?2, attempts = attempts + 1, status = 'sending' WHERE id = ?1", params![d.id, t + LEASE])?;
    }
    tx.commit()?;
    Ok(due)
}

async fn attempt(st: &AppState, d: Due) {
    let mut headers = vec![
        ("Content-Type", "application/json".to_string()),
        ("X-Gitter-Event", d.event.clone()),
        ("X-Gitter-Delivery", d.guid.clone()),
    ];
    let started = std::time::Instant::now();
    let result = match crate::security::open(&st.key, &d.secret) {
        None => Err("The secret can't be decrypted (was GITTER_DATA/secret.key replaced?). Set it again in this webhook's settings.".to_string()),
        Some(secret) => {
            headers.push(("X-Gitter-Signature-256", sign(&secret, d.payload.as_bytes())));
            let (url, body, h, allow) = (d.url, d.payload, headers.clone(), d.allow_private);
            tokio::task::spawn_blocking(move || crate::net::post(&url, &h, body.as_bytes(), allow)).await.unwrap_or_else(|e| Err(e.to_string()))
        }
    };
    let ms = started.elapsed().as_millis() as i64;
    headers.push(("User-Agent", crate::net::USER_AGENT.to_string()));
    let sent = Value::Object(headers.into_iter().map(|(k, v)| (k.to_string(), Value::String(v))).collect()).to_string();
    let id = d.id;
    if let Err(e) = st.db.call(move |c| Ok(finish(c, id, &sent, result, ms)?)).await {
        eprintln!("webhooks: delivery {id}: {}", e.msg);
    }
}

/// Record an attempt: delivered on a 2xx, else retried after the next BACKOFF
/// wait, or given up after the last. Then trim the hook's log to KEEP.
fn finish(c: &Connection, id: i64, sent: &str, result: Result<crate::net::Reply, String>, ms: i64) -> rusqlite::Result<()> {
    let row = c.query_row("SELECT hook_id, attempts FROM webhook_deliveries WHERE id = ?1", [id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))).optional()?;
    // Gone: its hook was deleted meanwhile.
    let Some((hook, attempts)) = row else { return Ok(()) };
    let (code, body, error) = match result {
        Ok(r) => (Some(r.status), String::from_utf8_lossy(&r.body[..r.body.len().min(KEEP_REPLY)]).into_owned(), String::new()),
        Err(e) => (None, String::new(), e),
    };
    let t = now();
    let (status, next) = match code {
        Some(200..=299) => ("ok", None),
        _ => match usize::try_from(attempts - 1).ok().and_then(|i| BACKOFF.get(i)) {
            Some(wait) => ("pending", Some(t + wait)),
            None => ("failed", None),
        },
    };
    c.execute(
        "UPDATE webhook_deliveries SET status = ?2, next_at = ?3, request_headers = ?4, response_code = ?5, response_body = ?6,
         error = ?7, duration_ms = ?8, tried_at = ?9 WHERE id = ?1",
        params![id, status, next, sent, code, body, error, ms, t],
    )?;
    c.execute(
        "DELETE FROM webhook_deliveries WHERE hook_id = ?1 AND next_at IS NULL
         AND id NOT IN (SELECT id FROM webhook_deliveries WHERE hook_id = ?1 AND next_at IS NULL ORDER BY id DESC LIMIT ?2)",
        params![hook, KEEP],
    )?;
    Ok(())
}

/// Announce finished CI runs, then send every due delivery, PARALLEL at a
/// time and each receiver's in order, waiting for them (tests and tools; the
/// server uses `run`). Returns how many were sent.
pub async fn tick(st: &AppState) -> usize {
    let base = st.cfg.url.clone();
    if let Err(e) = st.db.call(move |c| Ok(ci_runs(c, &base)?)).await {
        eprintln!("webhooks: ci runs: {}", e.msg);
    }
    // Rounds until nothing is due: a round takes one delivery per receiver.
    let mut n = 0;
    loop {
        let due = match st.db.call(|c| Ok(claim(c, PARALLEL)?)).await {
            Ok(d) => d,
            Err(e) => {
                eprintln!("webhooks: {}", e.msg);
                return n;
            }
        };
        if due.is_empty() {
            return n;
        }
        n += due.len();
        futures_util::future::join_all(due.into_iter().map(|d| attempt(st, d))).await;
    }
}

/// The delivery loop, for as long as the server runs: up to PARALLEL
/// deliveries in flight, each started as soon as a worker is free.
pub async fn run(st: AppState) {
    let workers = std::sync::Arc::new(tokio::sync::Semaphore::new(PARALLEL));
    loop {
        let base = st.cfg.url.clone();
        if let Err(e) = st.db.call(move |c| Ok(ci_runs(c, &base)?)).await {
            eprintln!("webhooks: ci runs: {}", e.msg);
        }
        let room = workers.available_permits();
        let due = if room == 0 { vec![] } else { st.db.call(move |c| Ok(claim(c, room)?)).await.unwrap_or_else(|e| {
            eprintln!("webhooks: {}", e.msg);
            vec![]
        }) };
        let started = due.len();
        for d in due {
            let Ok(permit) = workers.clone().try_acquire_owned() else { break };
            let st = st.clone();
            tokio::spawn(async move {
                attempt(&st, d).await;
                drop(permit);
                st.hook_wake.notify_one();
            });
        }
        if started == 0 || started == room {
            tokio::select! {
                _ = st.hook_wake.notified() => {}
                _ = tokio::time::sleep(Duration::from_secs(5)) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signatures_and_ids() {
        // HMAC-SHA256 test case 2 of RFC 4231.
        assert_eq!(sign(b"Jefe", b"what do ya want for nothing?"), "sha256=5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843");
        let id = uuid();
        assert_eq!((id.len(), &id[14..15], id.matches('-').count()), (36, "4", 4));
        assert_ne!(uuid(), id);
        assert!(wants("*", "push") && wants("issues,push", "push") && !wants("issues", "push") && !wants("push", "pus"));
    }
}
