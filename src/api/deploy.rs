//! Deployments and environments: where a repository's code runs (production, staging, a pull
//! request's preview), what was deployed there and by whom or which CI job, and how it went.
//! An environment can keep other branches out and has secrets of its own.

use axum::Json;
use axum::extract::{Path, Query, State};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{ClientIp, J, ok};
use crate::AppState;
use crate::auth::{Authed, Sudo, Viewer};
use crate::db::now;
use crate::error::{AppError, R};
use crate::repo::Repo;
use crate::security::{Event, record};

/// How many environments a repository can have (pull request previews count).
pub const MAX_ENVIRONMENTS: i64 = 100;

/// What a deployment can be: waiting or running, how it ended, or replaced by a later one.
pub const STATUSES: &[&str] = &["queued", "in_progress", "success", "failure", "inactive"];

/// An environment's name: `production`, `staging`, `preview/pr-7`.
pub fn env_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && !s.starts_with(['/', '.', '-'])
        && !s.ends_with('/')
        && !s.contains("//")
        && !s.contains("..")
        && s.chars().all(|c| c.is_ascii_alphanumeric() || "._-/".contains(c))
}

/// Whether `refname` (a full ref) may deploy to an environment whose rule is `rule`: branch
/// patterns as in branch rules, or full refs like `refs/tags/v*`, comma-separated; '' lets anything.
pub fn allowed(rule: &str, refname: &str) -> bool {
    let mut pats = rule.split(',').map(str::trim).filter(|p| !p.is_empty()).peekable();
    pats.peek().is_none()
        || pats.any(|p| match p.starts_with("refs/") {
            true => crate::protect::covers(p, refname),
            false => refname.strip_prefix("refs/heads/").is_some_and(|b| crate::protect::covers(p, b)),
        })
}

/// A link shown on the page: http(s) only, so it can never be a script.
fn link(s: &str) -> R<String> {
    let s = s.trim();
    if s.is_empty() || crate::validate::web_link(s) {
        return Ok(s.to_string());
    }
    Err(AppError::bad("A URL starts with https:// (or http://)"))
}

pub struct Env {
    pub id: i64,
    pub url: String,
    pub branches: String,
}

/// Environment `name` of repository `repo`, made (open to every branch) if it's new.
pub fn ensure(c: &Connection, repo: i64, name: &str) -> rusqlite::Result<Env> {
    c.execute("INSERT OR IGNORE INTO environments(repo_id, name, created_at) VALUES (?1, ?2, ?3)", params![repo, name, now()])?;
    c.query_row("SELECT id, url, branches FROM environments WHERE repo_id = ?1 AND name = ?2", params![repo, name], |r| {
        Ok(Env { id: r.get(0)?, url: r.get(1)?, branches: r.get(2)? })
    })
}

/// A new deployment, the newest of its environment.
#[allow(clippy::too_many_arguments)]
pub fn insert(c: &Connection, repo: i64, env: i64, refname: &str, sha: &str, creator: Option<i64>, job: Option<i64>, pr: Option<i64>, status: &str, url: &str, description: &str) -> rusqlite::Result<i64> {
    let t = now();
    c.execute(
        "INSERT INTO deployments(repo_id, environment_id, ref, sha, creator_id, job_id, pr_number, status, url, description, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)",
        params![repo, env, refname, sha, creator, job, pr, status, url, description, t],
    )?;
    Ok(c.last_insert_rowid())
}

/// Settle deployment `id`. A success retires the environment's earlier ones (or this one, when a
/// later one already succeeded). The ids of retired deployments that had a preview come back.
pub fn set_status(c: &Connection, id: i64, status: &str, url: Option<&str>) -> rusqlite::Result<Vec<i64>> {
    c.execute("UPDATE deployments SET status = ?2, url = coalesce(?3, url), updated_at = ?4 WHERE id = ?1", params![id, status, url, now()])?;
    if status != "success" {
        return Ok(vec![]);
    }
    let env: i64 = c.query_row("SELECT environment_id FROM deployments WHERE id = ?1", [id], |r| r.get(0))?;
    c.execute(
        "UPDATE deployments SET status = 'inactive', updated_at = ?3 WHERE environment_id = ?1 AND id < ?2 AND status = 'success'",
        params![env, id, now()],
    )?;
    c.execute(
        "UPDATE deployments SET status = 'inactive' WHERE id = ?1 AND EXISTS (SELECT 1 FROM deployments WHERE environment_id = ?2 AND id > ?1 AND status = 'success')",
        params![id, env],
    )?;
    retire_previews(c, "environment_id = ?1 AND status = 'inactive'", env)
}

/// Clear the preview of each deployment matching `filter` (with ?1 bound to `arg`): their ids.
pub fn retire_previews(c: &Connection, filter: &str, arg: i64) -> rusqlite::Result<Vec<i64>> {
    let mut q = c.prepare(&format!("SELECT id FROM deployments WHERE preview IS NOT NULL AND {filter}"))?;
    let ids: Vec<i64> = q.query_map([arg], |r| r.get(0))?.collect::<Result<_, _>>()?;
    for id in &ids {
        c.execute("UPDATE deployments SET preview = NULL WHERE id = ?1", [id])?;
    }
    Ok(ids)
}

/// A deployment as the API shows it. A CI job's deployment that's still open after its job
/// ended some other way (cancelled, timed out) shows how the job ended.
const COLS: &str = "d.id, e.name, d.ref, d.sha, u.username, d.job_id, d.pr_number,
    CASE WHEN d.status IN ('queued', 'in_progress') AND j.id IS NOT NULL AND j.status NOT IN ('waiting', 'queued', 'running')
         THEN CASE j.status WHEN 'success' THEN 'success' ELSE 'failure' END ELSE d.status END,
    d.url, d.description, d.preview, d.created_at, d.updated_at, cr.number
    FROM deployments d JOIN environments e ON e.id = d.environment_id LEFT JOIN users u ON u.id = d.creator_id
    LEFT JOIN ci_jobs j ON j.id = d.job_id LEFT JOIN ci_runs cr ON cr.id = j.run_id";

fn row_json(r: &rusqlite::Row) -> rusqlite::Result<Value> {
    let preview: Option<String> = r.get(10)?;
    Ok(json!({
        "id": r.get::<_, i64>(0)?, "environment": r.get::<_, String>(1)?, "ref": r.get::<_, String>(2)?, "sha": r.get::<_, String>(3)?,
        "creator": r.get::<_, Option<String>>(4)?, "job_id": r.get::<_, Option<i64>>(5)?, "pr_number": r.get::<_, Option<i64>>(6)?,
        "status": r.get::<_, String>(7)?, "url": r.get::<_, String>(8)?, "description": r.get::<_, String>(9)?,
        "preview": preview.map(|t| format!("/preview/{t}/")), "created_at": r.get::<_, i64>(11)?, "updated_at": r.get::<_, i64>(12)?,
        "run_number": r.get::<_, Option<i64>>(13)?,
    }))
}

// ---------------------------------------------------------------- CI jobs

/// A job deploying to an environment starts: its branch rule is checked, the deployment recorded
/// (in progress), and the environment's secrets handed back (none for a run an agent started).
/// A job with only `preview:` deploys to its pull request's preview environment (a run that isn't
/// a pull request's has nothing to preview). Err: why the job may not deploy there.
pub async fn job_started(st: &AppState, l: &crate::ci::queue::Lease, by_agent: bool) -> R<Result<Vec<(String, String)>, String>> {
    let (repo, job, run, refname, sha) = (l.repo_id, l.job_id, l.run_id, l.refname.clone(), l.sha.clone());
    let (env, preview, event) = (l.job.environment.clone(), l.job.preview.is_some(), l.event.clone());
    let sealed = st
        .db
        .call(move |c| {
            let (creator, pr): (Option<i64>, Option<i64>) = c.query_row("SELECT actor_id, pr_number FROM ci_runs WHERE id = ?1", [run], |r| Ok((r.get(0)?, r.get(1)?)))?;
            let open = |n: i64| -> rusqlite::Result<bool> {
                Ok(c.query_row("SELECT count(*) FROM issues WHERE repo_id = ?1 AND number = ?2 AND is_pr = 1 AND state = 'open'", params![repo, n], |r| r.get(0))?)
            };
            let (name, url) = match (env, pr) {
                (Some(e), _) => (e.name, e.url),
                // A pull request's own runs (not the merge queue's test commits), while it's open.
                (None, Some(n)) if preview && event == "pull_request" && open(n)? => (format!("preview/pr-{n}"), String::new()),
                (None, _) => return Ok(Ok(vec![])),
            };
            let known: bool = c.query_row("SELECT count(*) FROM environments WHERE repo_id = ?1 AND name = ?2", params![repo, name], |r| r.get(0))?;
            let count: i64 = c.query_row("SELECT count(*) FROM environments WHERE repo_id = ?1", [repo], |r| r.get(0))?;
            if !known && count >= MAX_ENVIRONMENTS {
                return Ok(Err(format!("This repository has {MAX_ENVIRONMENTS} environments, so {name} can't be made: delete some on the Deployments page")));
            }
            let e = ensure(c, repo, &name)?;
            if !allowed(&e.branches, &refname) {
                return Ok(Err(format!("{name} takes deployments from {} only, so this job didn't run (it was for {refname})", e.branches)));
            }
            let short = refname.strip_prefix("refs/heads/").or_else(|| refname.strip_prefix("refs/tags/")).unwrap_or(&refname);
            // An earlier attempt of this job that never ended.
            c.execute("DELETE FROM deployments WHERE job_id = ?1 AND status IN ('queued', 'in_progress')", [job])?;
            insert(c, repo, e.id, short, &sha, creator, Some(job), pr, "in_progress", if url.is_empty() { &e.url } else { &url }, "")?;
            let mut q = c.prepare("SELECT name, value FROM environment_secrets WHERE environment_id = ?1 ORDER BY name")?;
            Ok(Ok(q.query_map([e.id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?)))?.collect::<Result<Vec<_>, _>>()?))
        })
        .await?;
    let sealed = match sealed {
        Ok(s) if !by_agent => s,
        Ok(_) => vec![],
        Err(why) => return Ok(Err(why)),
    };
    Ok(Ok(sealed.into_iter().filter_map(|(n, v)| Some((n, String::from_utf8(crate::security::open(&st.key, &v)?).ok()?))).collect()))
}

/// A job ended: its deployment (if it has one) takes the result, and a successful job's preview
/// folder is published from its artifacts.
pub async fn job_finished(st: &AppState, job: i64, success: bool) {
    if let Err(e) = settle(st, job, success).await {
        eprintln!("deployments: job {job}: {}", e.msg);
    }
}

/// (its pull request is closed or merged, it's in that pull request's preview environment)
fn pull_state(c: &Connection, id: i64) -> rusqlite::Result<(bool, bool)> {
    c.query_row(
        "SELECT coalesce((SELECT i.state != 'open' FROM issues i WHERE i.repo_id = d.repo_id AND i.number = d.pr_number AND i.is_pr = 1), 0),
                e.name = 'preview/pr-' || d.pr_number
         FROM deployments d JOIN environments e ON e.id = d.environment_id WHERE d.id = ?1",
        [id],
        |r| Ok((r.get(0)?, r.get::<_, Option<bool>>(1)?.unwrap_or(false))),
    )
}

async fn settle(st: &AppState, job: i64, success: bool) -> R<()> {
    let open: Option<(i64, String, bool)> = st
        .db
        .call(move |c| {
            let row: Option<(i64, String)> = c
                .query_row("SELECT d.id, j.spec FROM deployments d JOIN ci_jobs j ON j.id = d.job_id WHERE d.job_id = ?1 AND d.status = 'in_progress'", [job], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()?;
            Ok(match row {
                Some((id, spec)) => Some((id, spec, pull_state(c, id)?.0)),
                None => None,
            })
        })
        .await?;
    let Some((id, spec, closed)) = open else { return Ok(()) };
    // A pull request that's closed (or merged) by now gets no preview.
    let folder = serde_json::from_str::<crate::ci::config::Job>(&spec).ok().and_then(|j| j.preview).filter(|_| !closed);
    let (mut status, mut why, mut token, mut bytes) = (if success { "success" } else { "failure" }, String::new(), None, 0);
    if let (true, Some(folder)) = (success, folder) {
        match super::preview::publish(st, job, id, &folder).await {
            Ok((t, b)) => (token, bytes) = (Some(t), b),
            Err(e) => (status, why) = ("failure", e),
        }
    }
    let published = token.is_some();
    let gone = st
        .db
        .call(move |c| {
            // Closed while this was being published: then it's not shown after all, and a
            // preview environment's deployment is done.
            let (closed, preview_env) = pull_state(c, id)?;
            let token = token.filter(|_| !closed);
            if token.is_some() || !why.is_empty() {
                c.execute("UPDATE deployments SET preview = ?2, description = ?3, preview_bytes = ?4 WHERE id = ?1", params![id, token, why, bytes])?;
            }
            let mut gone = set_status(c, id, if closed && preview_env { "inactive" } else { status }, None)?;
            if published && token.is_none() {
                gone.push(id);
            }
            Ok(gone)
        })
        .await?;
    super::preview::remove(st, gone).await;
    Ok(())
}

/// A pull request closed or merged: its previews go away, and its preview environment's
/// deployments are done (what it deployed to staging or production stays as it is).
pub async fn pull_closed(st: &AppState, repo: i64, number: i64) {
    let gone = st
        .db
        .call(move |c| {
            let gone = retire_previews(c, &format!("repo_id = ?1 AND pr_number = {number}"), repo)?;
            c.execute(
                "UPDATE deployments SET status = 'inactive', updated_at = ?3 WHERE repo_id = ?1 AND pr_number = ?2 AND status IN ('success', 'queued')
                 AND environment_id IN (SELECT id FROM environments WHERE repo_id = ?1 AND name = 'preview/pr-' || ?2)",
                params![repo, number, now()],
            )?;
            Ok(gone)
        })
        .await;
    if let Ok(gone) = gone {
        super::preview::remove(st, gone).await;
    }
}

// ---------------------------------------------------------------- environments

pub async fn environments(State(st): State<AppState>, Viewer(v): Viewer, Path((o, n)): Path<(String, String)>) -> J<Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let id = r.id;
    let envs: Vec<Value> = st
        .db
        .call(move |c| {
            let mut q = c.prepare(
                "SELECT e.id, e.name, e.url, e.branches, (SELECT max(id) FROM deployments WHERE environment_id = e.id) AS last
                 FROM environments e WHERE e.repo_id = ?1 ORDER BY last IS NULL, last DESC, e.name",
            )?;
            let rows: Vec<(i64, String, String, String, Option<i64>)> = q.query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?.collect::<Result<_, _>>()?;
            let mut out = vec![];
            for (eid, name, url, branches, last) in rows {
                let latest = match last {
                    Some(d) => c.query_row(&format!("SELECT {COLS} WHERE d.id = ?1"), [d], row_json).optional()?,
                    None => None,
                };
                out.push(json!({ "id": eid, "name": name, "url": url, "branches": branches, "latest": latest }));
            }
            Ok(out)
        })
        .await?;
    Ok(Json(json!({ "environments": envs, "can_admin": r.can_admin, "can_write": r.can_write })))
}

#[derive(Deserialize)]
pub struct EnvReq {
    name: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    branches: String,
}

/// Make an environment, or change its URL and branch rule (repository admins, recently signed in:
/// the rule decides who gets its secrets).
pub async fn put_environment(State(st): State<AppState>, Sudo(u): Sudo, ip: ClientIp, Path((o, n)): Path<(String, String)>, Json(req): Json<EnvReq>) -> J<Value> {
    let r = Repo::administrable(&st, Some(&u), &o, &n).await?;
    let name = req.name.trim().to_string();
    if !env_name(&name) {
        return Err(AppError::bad("Environment names use letters, numbers, '.', '-', '_' and '/' (up to 64)"));
    }
    let url = link(&req.url)?;
    let branches = req.branches.trim().to_string();
    if branches.len() > 500 || branches.split(',').map(str::trim).any(|p| !p.is_empty() && !crate::validate::ref_name(p.trim_start_matches("refs/").replace('*', "x").as_str())) {
        return Err(AppError::bad("Branches: patterns like main or release/*, or refs/tags/v*, separated by commas"));
    }
    let rid = r.id;
    let count: i64 = st.db.call(move |c| Ok(c.query_row("SELECT count(*) FROM environments WHERE repo_id = ?1", [rid], |x| x.get(0))?)).await?;
    let id = st
        .db
        .call(move |c| {
            let exists: Option<i64> = c.query_row("SELECT id FROM environments WHERE repo_id = ?1 AND name = ?2", params![rid, name], |x| x.get(0)).optional()?;
            if exists.is_none() && count >= MAX_ENVIRONMENTS {
                return Err(AppError::bad(format!("A repository can have up to {MAX_ENVIRONMENTS} environments")));
            }
            let e = ensure(c, rid, &name)?;
            c.execute("UPDATE environments SET url = ?2, branches = ?3 WHERE id = ?1", params![e.id, url, branches])?;
            Ok(e.id)
        })
        .await?;
    let rule = if req.branches.trim().is_empty() { "any branch".to_string() } else { req.branches.trim().to_string() };
    record(&st.db, Event { actor: Some(&u), subject: Some(r.owner_id), action: "environment_set", detail: format!("{} in {} (deploys from {rule})", req.name.trim(), r.full_name()), ip: &ip.0 }).await;
    Ok(Json(json!({ "id": id })))
}

async fn environment(st: &AppState, r: &Repo, id: i64) -> R<i64> {
    let rid = r.id;
    st.db
        .call(move |c| Ok(c.query_row("SELECT id FROM environments WHERE id = ?1 AND repo_id = ?2", [id, rid], |x| x.get(0)).optional()?))
        .await?
        .ok_or_else(AppError::not_found)
}

pub async fn delete_environment(State(st): State<AppState>, Sudo(u): Sudo, ip: ClientIp, Path((o, n, id)): Path<(String, String, i64)>) -> J<Value> {
    let r = Repo::administrable(&st, Some(&u), &o, &n).await?;
    let id = environment(&st, &r, id).await?;
    record(&st.db, Event { actor: Some(&u), subject: Some(r.owner_id), action: "environment_deleted", detail: format!("environment {id} in {}", r.full_name()), ip: &ip.0 }).await;
    let gone = st
        .db
        .call(move |c| {
            let gone = retire_previews(c, "environment_id = ?1", id)?;
            c.execute("DELETE FROM environments WHERE id = ?1", [id])?;
            Ok(gone)
        })
        .await?;
    super::preview::remove(&st, gone).await;
    ok()
}

pub async fn env_secrets(State(st): State<AppState>, Authed(u): Authed, Path((o, n, id)): Path<(String, String, i64)>) -> J<Value> {
    let r = Repo::administrable(&st, Some(&u), &o, &n).await?;
    let id = environment(&st, &r, id).await?;
    let list: Vec<Value> = st
        .db
        .call(move |c| {
            let mut q = c.prepare("SELECT name, updated_at FROM environment_secrets WHERE environment_id = ?1 ORDER BY name")?;
            Ok(q.query_map([id], |r| Ok(json!({ "name": r.get::<_, String>(0)?, "updated_at": r.get::<_, i64>(1)? })))?.collect::<Result<_, _>>()?)
        })
        .await?;
    Ok(Json(json!({ "secrets": list })))
}

#[derive(Deserialize)]
pub struct SecretReq {
    value: String,
}

/// Set one of an environment's secrets: only jobs deploying there get it. Encrypted at rest, never shown again.
pub async fn set_env_secret(
    State(st): State<AppState>,
    Sudo(u): Sudo,
    ip: ClientIp,
    Path((o, n, id, name)): Path<(String, String, i64, String)>,
    Json(req): Json<SecretReq>,
) -> J<Value> {
    let r = Repo::administrable(&st, Some(&u), &o, &n).await?;
    let id = environment(&st, &r, id).await?;
    if !super::ci::secret_name(&name) {
        return Err(AppError::bad("Secret names use A–Z, 0–9 and _, start with a letter or _, and don't start with GITTER"));
    }
    if req.value.is_empty() || req.value.len() > 64 * 1024 {
        return Err(AppError::bad("A secret is 1 byte to 64 KB"));
    }
    let sealed = crate::security::seal(&st.key, req.value.as_bytes());
    let n2 = name.clone();
    st.db
        .call(move |c| {
            let count: i64 = c.query_row("SELECT count(*) FROM environment_secrets WHERE environment_id = ?1 AND name != ?2", params![id, n2], |x| x.get(0))?;
            if count >= 100 {
                return Err(AppError::bad("An environment can have up to 100 secrets"));
            }
            c.execute(
                "INSERT INTO environment_secrets(environment_id, name, value, updated_at) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(environment_id, name) DO UPDATE SET value = ?3, updated_at = ?4",
                params![id, n2, sealed, now()],
            )?;
            Ok(())
        })
        .await?;
    record(&st.db, Event { actor: Some(&u), subject: Some(r.owner_id), action: "ci_secret_set", detail: format!("{name} in {} (environment {id})", r.full_name()), ip: &ip.0 }).await;
    ok()
}

pub async fn delete_env_secret(State(st): State<AppState>, Sudo(u): Sudo, ip: ClientIp, Path((o, n, id, name)): Path<(String, String, i64, String)>) -> J<Value> {
    let r = Repo::administrable(&st, Some(&u), &o, &n).await?;
    let id = environment(&st, &r, id).await?;
    let n2 = name.clone();
    let gone = st.db.call(move |c| Ok(c.execute("DELETE FROM environment_secrets WHERE environment_id = ?1 AND name = ?2", params![id, n2])?)).await?;
    if gone == 0 {
        return Err(AppError::not_found());
    }
    record(&st.db, Event { actor: Some(&u), subject: Some(r.owner_id), action: "ci_secret_deleted", detail: format!("{name} in {} (environment {id})", r.full_name()), ip: &ip.0 }).await;
    ok()
}

// ---------------------------------------------------------------- deployments

#[derive(Deserialize)]
pub struct ListQ {
    environment: Option<String>,
    pr: Option<i64>,
}

pub async fn list(State(st): State<AppState>, Viewer(v): Viewer, Path((o, n)): Path<(String, String)>, Query(q): Query<ListQ>) -> J<Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let id = r.id;
    let list: Vec<Value> = st
        .db
        .call(move |c| {
            let mut s = c.prepare(&format!(
                "SELECT {COLS} WHERE d.repo_id = ?1 AND (?2 IS NULL OR e.name = ?2) AND (?3 IS NULL OR d.pr_number = ?3) ORDER BY d.id DESC LIMIT 100"
            ))?;
            Ok(s.query_map(params![id, q.environment, q.pr], row_json)?.collect::<Result<_, _>>()?)
        })
        .await?;
    Ok(Json(json!({ "deployments": list })))
}

#[derive(Deserialize)]
pub struct NewReq {
    environment: String,
    #[serde(rename = "ref")]
    refname: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    status: Option<String>,
}

/// The full ref and sha `name` (a branch, a tag or a full ref) points at.
async fn resolve(st: &AppState, r: &Repo, name: &str) -> R<(String, String)> {
    let candidates = if name.starts_with("refs/") { vec![name.to_string()] } else { vec![format!("refs/heads/{name}"), format!("refs/tags/{name}")] };
    for full in candidates {
        if crate::validate::ref_name(full.trim_start_matches("refs/")) && let Some(sha) = st.git.resolve(&r.path, &format!("{full}^{{commit}}")).await? {
            return Ok((full, sha));
        }
    }
    Err(AppError::bad(format!("There's no branch or tag {name}")))
}

/// Record a deployment made outside CI (a deploy tool, by hand). People with write access.
pub async fn create(State(st): State<AppState>, Authed(u): Authed, Path((o, n)): Path<(String, String)>, Json(req): Json<NewReq>) -> J<Value> {
    let r = Repo::writable(&st, Some(&u), &o, &n).await?;
    let name = req.environment.trim().to_string();
    if !env_name(&name) {
        return Err(AppError::bad("Environment names use letters, numbers, '.', '-', '_' and '/' (up to 64)"));
    }
    let status = req.status.unwrap_or_else(|| "queued".into());
    if !STATUSES.contains(&status.as_str()) || status == "inactive" {
        return Err(AppError::bad("A new deployment is queued, in_progress, success or failure"));
    }
    let (url, description) = (link(&req.url)?, req.description.chars().take(500).collect::<String>());
    let (full, sha) = resolve(&st, &r, req.refname.trim()).await?;
    let short = full.strip_prefix("refs/heads/").or_else(|| full.strip_prefix("refs/tags/")).unwrap_or(&full).to_string();
    let (rid, uid) = (r.id, u.id);
    let (id, gone) = st
        .db
        .call(move |c| {
            let count: i64 = c.query_row("SELECT count(*) FROM environments WHERE repo_id = ?1", [rid], |x| x.get(0))?;
            let known: bool = c.query_row("SELECT count(*) FROM environments WHERE repo_id = ?1 AND name = ?2", params![rid, name], |x| x.get(0))?;
            if !known && count >= MAX_ENVIRONMENTS {
                return Err(AppError::bad(format!("A repository can have up to {MAX_ENVIRONMENTS} environments")));
            }
            let e = ensure(c, rid, &name)?;
            if !allowed(&e.branches, &full) {
                return Err(AppError::new(axum::http::StatusCode::FORBIDDEN, format!("{name} takes deployments from {} only", e.branches)));
            }
            let id = insert(c, rid, e.id, &short, &sha, Some(uid), None, None, "queued", &url, &description)?;
            let gone = if status == "queued" { vec![] } else { set_status(c, id, &status, None)? };
            Ok((id, gone))
        })
        .await?;
    super::preview::remove(&st, gone).await;
    Ok(Json(json!({ "id": id })))
}

#[derive(Deserialize)]
pub struct StatusReq {
    status: Option<String>,
    url: Option<String>,
    description: Option<String>,
}

/// Move a deployment on (people with write access). A CI job's deployment follows its job.
pub async fn update(State(st): State<AppState>, Authed(u): Authed, Path((o, n, id)): Path<(String, String, i64)>, Json(req): Json<StatusReq>) -> J<Value> {
    let r = Repo::writable(&st, Some(&u), &o, &n).await?;
    let url = req.url.as_deref().map(link).transpose()?;
    if let Some(s) = &req.status
        && !STATUSES.contains(&s.as_str())
    {
        return Err(AppError::bad(format!("A deployment's status is one of {}", STATUSES.join(", "))));
    }
    let rid = r.id;
    let gone = st
        .db
        .call(move |c| {
            let job: Option<Option<i64>> = c.query_row("SELECT job_id FROM deployments WHERE id = ?1 AND repo_id = ?2", params![id, rid], |x| x.get(0)).optional()?;
            match job {
                None => return Err(AppError::not_found()),
                Some(Some(_)) => return Err(AppError::conflict("This deployment follows its CI job")),
                Some(None) => {}
            }
            if let Some(d) = &req.description {
                c.execute("UPDATE deployments SET description = ?2 WHERE id = ?1", params![id, d.chars().take(500).collect::<String>()])?;
            }
            let status: String = match &req.status {
                Some(s) => s.clone(),
                None => c.query_row("SELECT status FROM deployments WHERE id = ?1", [id], |x| x.get(0))?,
            };
            Ok(set_status(c, id, &status, url.as_deref())?)
        })
        .await?;
    super::preview::remove(&st, gone).await;
    ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_rules_match_branches_and_full_refs() {
        for (rule, refname, ok) in [
            ("", "refs/heads/anything", true),
            ("", "refs/tags/v1", true),
            ("main", "refs/heads/main", true),
            ("main", "refs/heads/fix/main", false),
            ("main", "refs/tags/main", false),
            ("main, release/*", "refs/heads/release/2", true),
            ("release/*", "refs/heads/release/2/x", false),
            ("release/**", "refs/heads/release/2/x", true),
            ("refs/tags/v*", "refs/tags/v1.2", true),
            ("refs/tags/v*", "refs/heads/v1.2", false),
            ("main", "refs/pull/1/head", false),
        ] {
            assert_eq!(allowed(rule, refname), ok, "{rule:?} {refname}");
        }
        for (name, ok) in [("production", true), ("preview/pr-7", true), ("../x", false), ("/x", false), ("x/", false), ("a//b", false), ("-x", false), ("a b", false), ("", false)] {
            assert_eq!(env_name(name), ok, "{name:?}");
        }
    }
}
