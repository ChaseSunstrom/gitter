//! CI in the API: runs and their jobs, logs, artifacts, cancel and re-run,
//! secrets, commit statuses, and (for admins) runners.

use std::convert::Infallible;
use std::time::Duration;

use axum::Json;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderValue, header};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::Response;
use rusqlite::OptionalExtension;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use super::admin::Admin;
use super::{ClientIp, J, ok};
use crate::AppState;
use crate::auth::{Authed, Sudo, Viewer};
use crate::ci::{jobs, queue, trigger};
use crate::error::{AppError, R};
use crate::repo::Repo;
use crate::security::{Event, record};
use crate::validate;

const RUN_COLS: &str = "r.id, r.number, r.event, r.ref, r.sha, r.pr_number, r.title, r.status, r.error, r.created_at, r.started_at, r.finished_at, u.username, r.agent, r.approval";

fn run_json(r: &rusqlite::Row) -> rusqlite::Result<Value> {
    Ok(json!({
        "id": r.get::<_, i64>(0)?, "number": r.get::<_, i64>(1)?, "event": r.get::<_, String>(2)?, "ref": r.get::<_, String>(3)?,
        "sha": r.get::<_, String>(4)?, "pr_number": r.get::<_, Option<i64>>(5)?, "title": r.get::<_, String>(6)?,
        "status": r.get::<_, String>(7)?, "error": r.get::<_, String>(8)?, "created_at": r.get::<_, i64>(9)?,
        "started_at": r.get::<_, Option<i64>>(10)?, "finished_at": r.get::<_, Option<i64>>(11)?, "actor": r.get::<_, Option<String>>(12)?,
        "agent": r.get::<_, Option<String>>(13)?, "approval": r.get::<_, Option<String>>(14)?,
    }))
}

#[derive(Deserialize)]
pub struct RunsQ {
    branch: Option<String>,
    sha: Option<String>,
    page: Option<i64>,
}

pub async fn runs(State(st): State<AppState>, Viewer(v): Viewer, Path((o, n)): Path<(String, String)>, Query(q): Query<RunsQ>) -> J<Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let page = q.page.unwrap_or(1).clamp(1, 1000);
    let refname = q.branch.filter(|b| validate::ref_name(b)).map(|b| format!("refs/heads/{b}"));
    let sha = q.sha.filter(|s| validate::sha(s));
    let id = r.id;
    let (list, total) = st
        .db
        .call(move |c| {
            let filter = "r.repo_id = ?1 AND (?2 IS NULL OR r.ref = ?2) AND (?3 IS NULL OR r.sha = ?3)";
            let mut s = c.prepare(&format!(
                "SELECT {RUN_COLS} FROM ci_runs r LEFT JOIN users u ON u.id = r.actor_id WHERE {filter} ORDER BY r.id DESC LIMIT 30 OFFSET ?4"
            ))?;
            let list: Vec<Value> = s.query_map(rusqlite::params![id, refname, sha, (page - 1) * 30], run_json)?.collect::<Result<_, _>>()?;
            let total: i64 = c.query_row(&format!("SELECT count(*) FROM ci_runs r WHERE {filter}"), rusqlite::params![id, refname, sha], |r| r.get(0))?;
            Ok((list, total))
        })
        .await?;
    let enabled = trigger::enabled(&st, r.owner_id).await;
    // Only writers see the Run button, so only they make this read the pipeline file.
    let can_run = enabled && r.can_write && can_start_manually(&st, &r).await;
    Ok(Json(json!({ "runs": list, "total": total, "can_run": can_run, "enabled": enabled })))
}

/// Whether the default branch's pipeline allows the Run button.
async fn can_start_manually(st: &AppState, r: &Repo) -> bool {
    let Ok(Some(sha)) = st.git.resolve(&r.path, &format!("refs/heads/{}", r.default_branch)).await else { return false };
    matches!(trigger::pipeline_at(st, &r.path, &sha, &r.default_branch).await, Ok(Some(Ok(p))) if p.triggers.manual)
}

async fn load_run(st: &AppState, r: &Repo, number: i64) -> R<Value> {
    let id = r.id;
    st.db
        .call(move |c| {
            Ok(c.query_row(
                &format!("SELECT {RUN_COLS} FROM ci_runs r LEFT JOIN users u ON u.id = r.actor_id WHERE r.repo_id = ?1 AND r.number = ?2"),
                [id, number],
                run_json,
            )
            .optional()?)
        })
        .await?
        .ok_or_else(AppError::not_found)
}

pub async fn run(State(st): State<AppState>, Viewer(v): Viewer, Path((o, n, number)): Path<(String, String, i64)>) -> J<Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let mut run = load_run(&st, &r, number).await?;
    let run_id = run["id"].as_i64().unwrap_or(0);
    let jobs: Vec<Value> = st
        .db
        .call(move |c| {
            let mut s = c.prepare(
                "SELECT j.id, j.name, j.key, j.status, j.exit_code, j.steps, j.attempt, j.started_at, j.finished_at, j.spec, a.size, ru.name, j.reason
                 FROM ci_jobs j LEFT JOIN ci_artifacts a ON a.job_id = j.id LEFT JOIN ci_runners ru ON ru.id = j.runner_id
                 WHERE j.run_id = ?1 ORDER BY j.id",
            )?;
            Ok(s.query_map([run_id], |r| {
                let spec: Value = serde_json::from_str(&r.get::<_, String>(9)?).unwrap_or(Value::Null);
                Ok(json!({
                    "id": r.get::<_, i64>(0)?, "name": r.get::<_, String>(1)?, "key": r.get::<_, String>(2)?,
                    "status": r.get::<_, String>(3)?, "exit_code": r.get::<_, Option<i32>>(4)?,
                    "steps": serde_json::from_str::<Value>(&r.get::<_, String>(5)?).unwrap_or(json!([])),
                    "attempt": r.get::<_, i64>(6)?, "started_at": r.get::<_, Option<i64>>(7)?, "finished_at": r.get::<_, Option<i64>>(8)?,
                    "needs": spec["needs"], "image": spec["image"], "artifact_size": r.get::<_, Option<i64>>(10)?,
                    "runner": r.get::<_, Option<String>>(11)?, "reason": r.get::<_, String>(12)?,
                }))
            })?
            .collect::<Result<_, _>>()?)
        })
        .await?;
    run["jobs"] = json!(jobs);
    run["can_write"] = json!(r.can_write);
    Ok(Json(run))
}

#[derive(Deserialize)]
pub struct StartReq {
    branch: Option<String>,
}

/// The Run button: a manual run of a branch (the default branch unless named).
pub async fn start(State(st): State<AppState>, Authed(u): Authed, Path((o, n)): Path<(String, String)>, Json(req): Json<StartReq>) -> J<Value> {
    let r = Repo::writable(&st, Some(&u), &o, &n).await?;
    if !trigger::enabled(&st, r.owner_id).await {
        return Err(AppError::bad("CI isn't turned on for this repository's owner on this site"));
    }
    let branch = req.branch.unwrap_or_else(|| r.default_branch.clone());
    if !validate::ref_name(&branch) {
        return Err(AppError::bad("That isn't a valid branch name"));
    }
    let refname = format!("refs/heads/{branch}");
    let sha = st.git.resolve(&r.path, &refname).await?.ok_or_else(|| AppError::bad("No such branch"))?;
    let pipeline = trigger::pipeline_at(&st, &r.path, &sha, &r.default_branch)
        .await?
        .ok_or_else(|| AppError::bad("This branch has no .gitter/ci.yml, .gitter/ci.sh, GitHub workflows or .gitlab-ci.yml"))?;
    if let Ok(p) = &pipeline
        && !p.triggers.manual
    {
        return Err(AppError::bad("This pipeline doesn't allow manual runs (add `manual` under `on:`, or workflow_dispatch to a GitHub workflow)"));
    }
    let pipeline = trigger::pick(pipeline, "manual", &branch, &None).ok_or_else(|| AppError::bad("No job here runs manually"))?;
    let s = trigger::Start { event: "manual", refname, sha, pr_number: None, actor_id: Some(u.id), files: None };
    let number = trigger::start(&st, &r, pipeline, s).await?;
    Ok(Json(json!({ "number": number })))
}

/// The id of run `number` of this repository.
async fn run_id(st: &AppState, r: &Repo, number: i64) -> R<i64> {
    let repo = r.id;
    st.db
        .call(move |c| Ok(c.query_row("SELECT id FROM ci_runs WHERE repo_id = ?1 AND number = ?2", [repo, number], |r| r.get(0)).optional()?))
        .await?
        .ok_or_else(AppError::not_found)
}

pub async fn cancel(State(st): State<AppState>, Authed(u): Authed, Path((o, n, number)): Path<(String, String, i64)>) -> J<Value> {
    let r = Repo::writable(&st, Some(&u), &o, &n).await?;
    let id = run_id(&st, &r, number).await?;
    // Ending a run moves on what waits for it (the merge queue, auto-merge), once.
    if st.db.call(move |c| Ok(queue::cancel(c, id)?)).await?
        && let Err(e) = crate::ci::jobs::run_ended(&st, id).await
    {
        eprintln!("ci: after cancelling run {id}: {}", e.msg);
    }
    ok()
}

/// Let a run that waits for approval (one an agent's changes started) go to
/// the runners. It still gets no secrets and never runs on the server itself.
pub async fn approve(State(st): State<AppState>, Authed(u): Authed, Path((o, n, number)): Path<(String, String, i64)>) -> J<Value> {
    // A person reads the changes: never an agent (tokens for agents only reach /mcp anyway).
    if u.agent().is_some() {
        return Err(AppError::new(axum::http::StatusCode::FORBIDDEN, "A person has to approve this run"));
    }
    let r = Repo::writable(&st, Some(&u), &o, &n).await?;
    let id = run_id(&st, &r, number).await?;
    let uid = u.id;
    let changed = st
        .db
        .call(move |c| Ok(c.execute("UPDATE ci_runs SET approval = 'approved', approved_by = ?2 WHERE id = ?1 AND approval = 'needed'", rusqlite::params![id, uid])?))
        .await?;
    if changed == 0 {
        return Err(AppError::conflict("This run isn't waiting for approval"));
    }
    st.ci_wake.notify_waiters();
    ok()
}

#[derive(Deserialize)]
pub struct RerunReq {
    #[serde(default)]
    all: bool,
}

pub async fn rerun(State(st): State<AppState>, Authed(u): Authed, Path((o, n, number)): Path<(String, String, i64)>, Json(req): Json<RerunReq>) -> J<Value> {
    let r = Repo::writable(&st, Some(&u), &o, &n).await?;
    if !trigger::enabled(&st, r.owner_id).await {
        return Err(AppError::bad("CI isn't turned on for this repository's owner on this site"));
    }
    let id = run_id(&st, &r, number).await?;
    if !st.db.call(move |c| Ok(queue::rerun(c, id, req.all)?)).await? {
        return Err(AppError::conflict("This run can't be re-run now: it's still going (cancel it or wait), or its pipeline file was broken (push a fix)"));
    }
    st.ci_wake.notify_waiters();
    ok()
}

/// A job of this repository (by id), checked against the URL's repository.
async fn job_of(st: &AppState, repo: i64, id: i64) -> R<String> {
    st.db
        .call(move |c| {
            Ok(c.query_row(
                "SELECT j.status FROM ci_jobs j JOIN ci_runs r ON r.id = j.run_id WHERE j.id = ?1 AND r.repo_id = ?2",
                [id, repo],
                |r| r.get(0),
            )
            .optional()?)
        })
        .await?
        .ok_or_else(AppError::not_found)
}

#[derive(Deserialize)]
pub struct LogQ {
    offset: Option<u64>,
}

/// Read a job's log from `offset` (at most 4 MB at a time).
async fn read_log(st: &AppState, id: i64, offset: u64) -> R<Vec<u8>> {
    let Ok(mut f) = tokio::fs::File::open(jobs::log_path(&st.cfg.data_dir, id)).await else { return Ok(vec![]) };
    f.seek(std::io::SeekFrom::Start(offset)).await?;
    let mut out = Vec::new();
    f.take(4 << 20).read_to_end(&mut out).await?;
    Ok(out)
}

/// The log as plain text (`?offset=` for the part after what you have).
pub async fn log(State(st): State<AppState>, Viewer(v): Viewer, Path((o, n, id)): Path<(String, String, i64)>, Query(q): Query<LogQ>) -> R<Response> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let status = job_of(&st, r.id, id).await?;
    let data = read_log(&st, id, q.offset.unwrap_or(0)).await?;
    let mut resp = Response::new(Body::from(data));
    let h = resp.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/plain; charset=utf-8"));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert("x-content-type-options", HeaderValue::from_static("nosniff"));
    if let Ok(v) = HeaderValue::from_str(&status) {
        h.insert("x-job-status", v);
    }
    Ok(resp)
}

/// The log as it's written: `data` events carry new output (as JSON strings),
/// a `status` event the job's state, and the stream ends when the job does.
pub async fn log_stream(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    headers: axum::http::HeaderMap,
    Path((o, n, id)): Path<(String, String, i64)>,
    Query(q): Query<LogQ>,
) -> R<Sse<impl futures_util::Stream<Item = Result<SseEvent, Infallible>>>> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    job_of(&st, r.id, id).await?;
    let repo = r.id;
    // Event ids are byte offsets, so a browser that reconnects carries on where it was.
    let resume = headers.get("last-event-id").and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok());
    let state = (st, resume.unwrap_or(0).max(q.offset.unwrap_or(0)), String::new(), false);
    let stream = futures_util::stream::unfold(state, move |(st, offset, last, done)| async move {
        if done {
            return None;
        }
        // ponytail: polls the file every 500 ms; a per-job broadcast if many people watch at once.
        for _ in 0..3600 {
            if crate::ci::runner::SHUTDOWN.load(std::sync::atomic::Ordering::SeqCst) {
                return None;
            }
            let status = job_of(&st, repo, id).await.unwrap_or_else(|_| "error".into());
            let mut data = read_log(&st, id, offset).await.unwrap_or_default();
            // Whole lines only while the job runs, so a chunk never ends inside a UTF-8 character.
            if !queue::terminal(&status) {
                data.truncate(data.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1));
            }
            if !data.is_empty() {
                let text = String::from_utf8_lossy(&data).into_owned();
                let ev = SseEvent::default().event("data").id((offset + data.len() as u64).to_string()).json_data(&text).unwrap_or_default();
                return Some((Ok(ev), (st, offset + data.len() as u64, last, false)));
            }
            if status != last {
                let finished = queue::terminal(&status);
                let ev = SseEvent::default().event("status").data(status.clone());
                return Some((Ok(ev), (st, offset, status, finished)));
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        None
    });
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

pub async fn artifacts(State(st): State<AppState>, Viewer(v): Viewer, Path((o, n, id)): Path<(String, String, i64)>) -> R<Response> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    job_of(&st, r.id, id).await?;
    let f = tokio::fs::File::open(jobs::artifact_path(&st.cfg.data_dir, id)).await.map_err(|_| AppError::not_found())?;
    let len = f.metadata().await?.len();
    let mut resp = Response::new(Body::from_stream(tokio_util::io::ReaderStream::new(f)));
    let h = resp.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/gzip"));
    h.insert(header::CONTENT_LENGTH, HeaderValue::from(len));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    if let Ok(v) = HeaderValue::from_str(&format!("attachment; filename=\"{}-{}-job{id}-artifacts.tar.gz\"", r.owner, r.name)) {
        h.insert(header::CONTENT_DISPOSITION, v);
    }
    Ok(resp)
}

#[derive(Deserialize)]
pub struct StatusQ {
    /// Comma-separated commit shas (up to 100).
    shas: String,
}

/// The latest run's status for each commit: `{ sha: { status, number } }`.
pub async fn statuses(State(st): State<AppState>, Viewer(v): Viewer, Path((o, n)): Path<(String, String)>, Query(q): Query<StatusQ>) -> J<Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let shas: Vec<String> = q.shas.split(',').filter(|s| validate::sha(s)).take(100).map(String::from).collect();
    let id = r.id;
    let out = st
        .db
        .call(move |c| {
            let mut s = c.prepare_cached("SELECT number, status FROM ci_runs WHERE repo_id = ?1 AND sha = ?2 ORDER BY id DESC LIMIT 1")?;
            let mut m = serde_json::Map::new();
            for sha in shas {
                if let Some((number, status)) = s.query_row(rusqlite::params![id, sha], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))).optional()? {
                    m.insert(sha, json!({ "number": number, "status": status }));
                }
            }
            Ok(m)
        })
        .await?;
    Ok(Json(Value::Object(out)))
}

// ---------------------------------------------------------------- secrets

pub(crate) fn secret_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes().enumerate().all(|(i, b)| b == b'_' || b.is_ascii_uppercase() || (i > 0 && b.is_ascii_digit()))
        && !s.starts_with("GITTER")
}

pub async fn secrets(State(st): State<AppState>, Authed(u): Authed, Path((o, n)): Path<(String, String)>) -> J<Value> {
    let r = Repo::administrable(&st, Some(&u), &o, &n).await?;
    let id = r.id;
    let list: Vec<Value> = st
        .db
        .call(move |c| {
            let mut s = c.prepare("SELECT name, updated_at FROM ci_secrets WHERE repo_id = ?1 ORDER BY name")?;
            Ok(s.query_map([id], |r| Ok(json!({ "name": r.get::<_, String>(0)?, "updated_at": r.get::<_, i64>(1)? })))?.collect::<Result<_, _>>()?)
        })
        .await?;
    Ok(Json(json!({ "secrets": list })))
}

#[derive(Deserialize)]
pub struct SecretReq {
    value: String,
}

/// Set a secret. Its value is encrypted at rest and never shown again.
pub async fn set_secret(
    State(st): State<AppState>,
    Sudo(u): Sudo,
    ip: ClientIp,
    Path((o, n, name)): Path<(String, String, String)>,
    Json(req): Json<SecretReq>,
) -> J<Value> {
    let r = Repo::administrable(&st, Some(&u), &o, &n).await?;
    if !secret_name(&name) {
        return Err(AppError::bad("Secret names use A–Z, 0–9 and _, start with a letter or _, and don't start with GITTER"));
    }
    if req.value.is_empty() || req.value.len() > 64 * 1024 {
        return Err(AppError::bad("A secret is 1 byte to 64 KB"));
    }
    let sealed = crate::security::seal(&st.key, req.value.as_bytes());
    let (id, n2) = (r.id, name.clone());
    let count: i64 = st
        .db
        .call(move |c| {
            c.execute(
                "INSERT INTO ci_secrets(repo_id, name, value, updated_at) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(repo_id, name) DO UPDATE SET value = ?3, updated_at = ?4",
                rusqlite::params![id, n2, sealed, crate::db::now()],
            )?;
            Ok(c.query_row("SELECT count(*) FROM ci_secrets WHERE repo_id = ?1", [id], |r| r.get(0))?)
        })
        .await?;
    if count > 100 {
        let (id, n2) = (r.id, name.clone());
        st.db.call(move |c| Ok(c.execute("DELETE FROM ci_secrets WHERE repo_id = ?1 AND name = ?2", rusqlite::params![id, n2])?)).await?;
        return Err(AppError::bad("A repository can have up to 100 secrets"));
    }
    record(&st.db, Event { actor: Some(&u), subject: Some(r.owner_id), action: "ci_secret_set", detail: format!("{} in {}", name, r.full_name()), ip: &ip.0 }).await;
    ok()
}

pub async fn delete_secret(State(st): State<AppState>, Sudo(u): Sudo, ip: ClientIp, Path((o, n, name)): Path<(String, String, String)>) -> J<Value> {
    let r = Repo::administrable(&st, Some(&u), &o, &n).await?;
    let (id, n2) = (r.id, name.clone());
    let gone = st.db.call(move |c| Ok(c.execute("DELETE FROM ci_secrets WHERE repo_id = ?1 AND name = ?2", rusqlite::params![id, n2])?)).await?;
    if gone == 0 {
        return Err(AppError::not_found());
    }
    record(&st.db, Event { actor: Some(&u), subject: Some(r.owner_id), action: "ci_secret_deleted", detail: format!("{} in {}", name, r.full_name()), ip: &ip.0 }).await;
    ok()
}

// ---------------------------------------------------------------- runners (admins)

pub async fn runners(State(st): State<AppState>, Admin(_): Admin) -> J<Value> {
    let list: Vec<Value> = st
        .db
        .call(|c| {
            let mut s = c.prepare(
                "SELECT r.id, r.name, r.labels, r.builtin, r.last_seen, r.created_at,
                    (SELECT count(*) FROM ci_jobs j WHERE j.runner_id = r.id AND j.status = 'running')
                 FROM ci_runners r ORDER BY r.builtin DESC, r.id",
            )?;
            Ok(s.query_map([], |r| {
                Ok(json!({
                    "id": r.get::<_, i64>(0)?, "name": r.get::<_, String>(1)?,
                    "labels": serde_json::from_str::<Value>(&r.get::<_, String>(2)?).unwrap_or(json!([])),
                    "builtin": r.get::<_, bool>(3)?, "last_seen": r.get::<_, Option<i64>>(4)?, "created_at": r.get::<_, i64>(5)?,
                    "running": r.get::<_, i64>(6)?,
                }))
            })?
            .collect::<Result<_, _>>()?)
        })
        .await?;
    let queued: i64 = st.db.call(|c| Ok(c.query_row("SELECT count(*) FROM ci_jobs WHERE status = 'queued'", [], |r| r.get(0))?)).await?;
    Ok(Json(json!({ "runners": list, "queued": queued })))
}

#[derive(Deserialize)]
pub struct RunnerReq {
    name: String,
}

/// Register a runner; its token is shown once.
pub async fn create_runner(State(st): State<AppState>, Admin(u): Admin, _s: Sudo, ip: ClientIp, Json(req): Json<RunnerReq>) -> J<Value> {
    let name: String = req.name.trim().chars().take(60).collect();
    if name.is_empty() {
        return Err(AppError::bad("Give the runner a name"));
    }
    let (token, hash) = crate::auth::new_secret(crate::ci::http::RUNNER_TOKEN_PREFIX);
    let n2 = name.clone();
    let id = st
        .db
        .call(move |c| {
            c.execute("INSERT INTO ci_runners(name, token_hash, created_at) VALUES (?1, ?2, ?3)", rusqlite::params![n2, hash, crate::db::now()])?;
            Ok(c.last_insert_rowid())
        })
        .await?;
    record(&st.db, Event { actor: Some(&u), subject: None, action: "ci_runner_created", detail: name, ip: &ip.0 }).await;
    Ok(Json(json!({ "id": id, "token": token })))
}

pub async fn delete_runner(State(st): State<AppState>, Admin(u): Admin, _s: Sudo, ip: ClientIp, Path(id): Path<i64>) -> J<Value> {
    let found: Option<(String, queue::Lost)> = st
        .db
        .call(move |c| {
            let Some(name) = c.query_row("SELECT name FROM ci_runners WHERE id = ?1 AND builtin = 0", [id], |r| r.get(0)).optional()? else {
                return Ok(None);
            };
            // Its running jobs go back to the queue at once.
            let lost = queue::release_runner(c, id)?;
            c.execute("DELETE FROM ci_runners WHERE id = ?1", [id])?;
            Ok(Some((name, lost)))
        })
        .await?;
    let (name, lost) = found.ok_or_else(AppError::not_found)?;
    // The runner is gone either way: a follow-up's error is logged, not the request's.
    if let Err(e) = crate::ci::jobs::after_lost(&st, lost).await {
        eprintln!("ci: runner {id} removed: follow-ups: {}", e.msg);
    }
    st.ci_wake.notify_waiters();
    record(&st.db, Event { actor: Some(&u), subject: None, action: "ci_runner_deleted", detail: name, ip: &ip.0 }).await;
    ok()
}
