//! Branch rules, commit statuses, auto-merge and CI badges over the API.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use rusqlite::{OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{ClientIp, J, ok};
use crate::AppState;
use crate::auth::{Authed, Viewer};
use crate::db::now;
use crate::error::{AppError, R};
use crate::protect::{self, Rule};
use crate::repo::Repo;
use crate::security::{Event, record};
use crate::validate;

// ---------------------------------------------------------------- branch rules

/// The repository's branch rules (anyone who can read it sees what a merge needs).
pub async fn rules(State(st): State<AppState>, Viewer(v): Viewer, Path((o, n)): Path<(String, String)>) -> J<Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let id = r.id;
    let rules = st.db.call(move |c| Ok(protect::load(c, id)?)).await?;
    Ok(Json(json!({ "rules": rules, "can_admin": r.can_admin })))
}

async fn audit(st: &AppState, u: &crate::auth::User, ip: &str, action: &str, detail: String) {
    record(&st.db, Event { actor: Some(u), subject: Some(u.id), action, detail, ip }).await;
}

pub async fn create_rule(State(st): State<AppState>, Authed(u): Authed, ClientIp(ip): ClientIp, Path((o, n)): Path<(String, String)>, Json(rule): Json<Rule>) -> J<Value> {
    let r = Repo::administrable(&st, Some(&u), &o, &n).await?;
    rule.validate()?;
    let (id, rl) = (r.id, rule.clone());
    let made = st
        .db
        .call(move |c| {
            let count: i64 = c.query_row("SELECT COUNT(*) FROM branch_rules WHERE repo_id = ?1", [id], |x| x.get(0))?;
            if count >= protect::MAX_RULES {
                return Ok(Err(AppError::bad(format!("A repository has at most {} branch rules", protect::MAX_RULES))));
            }
            let n = c.execute(
                "INSERT OR IGNORE INTO branch_rules (repo_id, pattern, require_pr, approvals, checks, allow_force_push, allow_deletion, enforce_admins, merge_queue, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?10, ?9)",
                params![id, rl.pattern, rl.require_pr, rl.approvals, serde_json::to_string(&rl.checks).unwrap_or_default(), rl.allow_force_push, rl.allow_deletion, rl.enforce_admins, now(), rl.merge_queue],
            )?;
            if n == 0 {
                return Ok(Err(AppError::conflict("There's already a rule for that pattern")));
            }
            Ok(Ok(c.last_insert_rowid()))
        })
        .await??;
    audit(&st, &u, &ip, "branch_rule", format!("{} added a rule for {} in {}", u.username, rule.pattern, r.full_name())).await;
    Ok(Json(json!({ "id": made })))
}

pub async fn update_rule(
    State(st): State<AppState>,
    Authed(u): Authed,
    ClientIp(ip): ClientIp,
    Path((o, n, rid)): Path<(String, String, i64)>,
    Json(rule): Json<Rule>,
) -> J<Value> {
    let r = Repo::administrable(&st, Some(&u), &o, &n).await?;
    rule.validate()?;
    let (id, rl) = (r.id, rule.clone());
    let changed = st
        .db
        .call(move |c| {
            Ok(c.execute(
                "UPDATE OR IGNORE branch_rules SET pattern = ?3, require_pr = ?4, approvals = ?5, checks = ?6, allow_force_push = ?7, allow_deletion = ?8, enforce_admins = ?9, merge_queue = ?10
                 WHERE id = ?1 AND repo_id = ?2",
                params![rid, id, rl.pattern, rl.require_pr, rl.approvals, serde_json::to_string(&rl.checks).unwrap_or_default(), rl.allow_force_push, rl.allow_deletion, rl.enforce_admins, rl.merge_queue],
            )?)
        })
        .await?;
    if changed == 0 {
        return Err(AppError::conflict("No such rule, or another rule has that pattern"));
    }
    audit(&st, &u, &ip, "branch_rule", format!("{} changed the rule for {} in {}", u.username, rule.pattern, r.full_name())).await;
    ok()
}

pub async fn delete_rule(State(st): State<AppState>, Authed(u): Authed, ClientIp(ip): ClientIp, Path((o, n, rid)): Path<(String, String, i64)>) -> J<Value> {
    let r = Repo::administrable(&st, Some(&u), &o, &n).await?;
    let id = r.id;
    let gone: Option<String> = st
        .db
        .call(move |c| {
            let p = c.query_row("SELECT pattern FROM branch_rules WHERE id = ?1 AND repo_id = ?2", params![rid, id], |x| x.get(0)).optional()?;
            c.execute("DELETE FROM branch_rules WHERE id = ?1 AND repo_id = ?2", params![rid, id])?;
            Ok(p)
        })
        .await?;
    let Some(pattern) = gone else { return Err(AppError::not_found()) };
    audit(&st, &u, &ip, "branch_rule", format!("{} removed the rule for {pattern} in {}", u.username, r.full_name())).await;
    ok()
}

// ---------------------------------------------------------------- commit statuses

#[derive(Deserialize)]
pub struct StatusReq {
    state: String,
    #[serde(default)]
    context: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    target_url: String,
}

/// Statuses one commit may have (one per context).
const MAX_CONTEXTS: i64 = 100;

/// An outside system (CI elsewhere, a deploy, a linter) reports on a commit:
/// one status per context, replaced by the next one with the same context.
pub async fn post_status(State(st): State<AppState>, Authed(u): Authed, Path((o, n, sha)): Path<(String, String, String)>, Json(req): Json<StatusReq>) -> J<Value> {
    let r = Repo::writable(&st, Some(&u), &o, &n).await?;
    if !validate::sha(&sha) || sha.len() < 40 {
        return Err(AppError::bad("Give the commit's full sha"));
    }
    if st.git.resolve(&r.path, &format!("{sha}^{{commit}}")).await?.as_deref() != Some(sha.as_str()) {
        return Err(AppError::not_found());
    }
    if !matches!(req.state.as_str(), "pending" | "success" | "failure" | "error") {
        return Err(AppError::bad("state is pending, success, failure or error"));
    }
    let context = if req.context.trim().is_empty() { "default".to_string() } else { req.context.trim().to_string() };
    if context.chars().count() > 100 || context.chars().any(char::is_control) {
        return Err(AppError::bad("The context is up to 100 characters"));
    }
    let description: String = req.description.trim().chars().filter(|c| !c.is_control()).take(140).collect();
    let url = req.target_url.trim().to_string();
    if !url.is_empty() && (url.len() > 2000 || !(url.starts_with("https://") || url.starts_with("http://"))) {
        return Err(AppError::bad("target_url is an http(s) link of up to 2000 characters"));
    }
    let (id, uid, s2, st8, ctx, d2, u2) = (r.id, u.id, sha.clone(), req.state.clone(), context.clone(), description.clone(), url.clone());
    st.db
        .call(move |c| {
            let known: bool = c.query_row("SELECT EXISTS (SELECT 1 FROM commit_statuses WHERE repo_id = ?1 AND sha = ?2 AND context = ?3)", params![id, s2, ctx], |x| x.get(0))?;
            let count: i64 = c.query_row("SELECT COUNT(*) FROM commit_statuses WHERE repo_id = ?1 AND sha = ?2", params![id, s2], |x| x.get(0))?;
            if !known && count >= MAX_CONTEXTS {
                return Ok(Err(AppError::bad(format!("A commit has at most {MAX_CONTEXTS} status contexts"))));
            }
            let t = now();
            c.execute(
                "INSERT INTO commit_statuses (repo_id, sha, context, state, description, target_url, creator_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
                 ON CONFLICT (repo_id, sha, context) DO UPDATE SET state = ?4, description = ?5, target_url = ?6, creator_id = ?7, updated_at = ?8",
                params![id, s2, ctx, st8, d2, u2, uid, t],
            )?;
            Ok(Ok(()))
        })
        .await??;
    let (st2, rid) = (st.clone(), r.id);
    tokio::spawn(async move {
        protect::auto_merge(&st2, rid, Some(sha)).await;
        crate::mergequeue::advance(&st2, rid).await;
    });
    Ok(Json(json!({ "context": context, "state": req.state, "description": description, "target_url": url })))
}

/// Everything that ran on a commit (CI jobs and statuses) and one state for them all.
pub async fn checks(State(st): State<AppState>, Viewer(v): Viewer, Path((o, n, sha)): Path<(String, String, String)>) -> J<Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    if !validate::sha(&sha) {
        return Err(AppError::bad("That isn't a commit sha"));
    }
    let (id, url) = (r.id, format!("/{}/{}", r.owner, r.name));
    let list = st.db.call(move |c| Ok(protect::checks(c, id, &sha, &url)?)).await?;
    Ok(Json(json!({ "state": protect::combined(&list), "checks": list })))
}

// ---------------------------------------------------------------- auto-merge

#[derive(Deserialize)]
pub struct AutoReq {
    #[serde(default = "merge_method")]
    method: String,
}

fn merge_method() -> String {
    "merge".into()
}

/// Merge this pull request by itself once its checks pass and its reviews are in.
pub async fn set_auto_merge(State(st): State<AppState>, Authed(u): Authed, Path((o, n, num)): Path<(String, String, i64)>, Json(req): Json<AutoReq>) -> J<Value> {
    let r = Repo::writable(&st, Some(&u), &o, &n).await?;
    if !matches!(req.method.as_str(), "merge" | "squash" | "ff") {
        return Err(AppError::bad("Unknown merge method"));
    }
    // Armed for the head as it is now: a newer push by someone else disarms it.
    let (id, uid, m) = (r.id, u.id, req.method.clone());
    let head: Option<(String, bool)> = st
        .db
        .call(move |c| {
            let Some((head, base)) = c
                .query_row(
                    "SELECT p.head_branch, p.base_branch FROM pulls p JOIN issues i ON i.id = p.issue_id WHERE i.repo_id = ?1 AND i.number = ?2 AND i.is_pr = 1 AND i.state = 'open'",
                    params![id, num],
                    |x| Ok((x.get::<_, String>(0)?, x.get::<_, String>(1)?)),
                )
                .optional()?
            else {
                return Ok(None);
            };
            let queued = protect::effective(&protect::load(c, id)?, &base).is_some_and(|x| x.merge_queue);
            Ok(Some((head, queued)))
        })
        .await?;
    let Some((branch, queued)) = head else { return Err(AppError::conflict("That pull request isn't open")) };
    if queued {
        return Err(AppError::conflict("This branch uses a merge queue: add the pull request to the queue instead"));
    }
    let sha = st.git.resolve(&r.path, &format!("refs/heads/{branch}")).await?.ok_or_else(|| AppError::conflict("The source branch is gone"))?;
    let n = st
        .db
        .call(move |c| {
            Ok(c.execute(
                "UPDATE pulls SET auto_merge = ?3, auto_merge_by = ?4, auto_merge_sha = ?5 WHERE issue_id = (SELECT id FROM issues WHERE repo_id = ?1 AND number = ?2 AND is_pr = 1 AND state = 'open')",
                params![id, num, m, uid, sha],
            )?)
        })
        .await?;
    if n == 0 {
        return Err(AppError::conflict("That pull request isn't open"));
    }
    // Maybe nothing is in the way already.
    protect::auto_merge(&st, r.id, None).await;
    ok()
}

pub async fn clear_auto_merge(State(st): State<AppState>, Authed(u): Authed, Path((o, n, num)): Path<(String, String, i64)>) -> J<Value> {
    let r = Repo::writable(&st, Some(&u), &o, &n).await?;
    let id = r.id;
    st.db
        .call(move |c| {
            Ok(c.execute(
                "UPDATE pulls SET auto_merge = NULL, auto_merge_by = NULL, auto_merge_sha = NULL WHERE issue_id = (SELECT id FROM issues WHERE repo_id = ?1 AND number = ?2)",
                params![id, num],
            )?)
        })
        .await?;
    ok()
}

// ---------------------------------------------------------------- badges

#[derive(Deserialize)]
pub struct BadgeQ {
    branch: Option<String>,
}

/// An SVG badge with the latest run's result on a branch (the default one
/// unless named), for READMEs. "unknown" when there's nothing to show,
/// including for repositories the viewer can't see.
pub async fn badge(State(st): State<AppState>, Viewer(v): Viewer, Path((o, n)): Path<(String, String)>, Query(q): Query<BadgeQ>) -> Response {
    let status = badge_status(&st, v.as_ref(), &o, &n, q.branch).await.unwrap_or(None);
    let (text, color) = match status.as_deref() {
        Some("success") => ("passing", "#2da44e"),
        Some("failure") | Some("error") => ("failing", "#cf222e"),
        Some("cancelled") => ("cancelled", "#6e7781"),
        Some("skipped") => ("skipped", "#6e7781"),
        Some(_) => ("running", "#bf8700"),
        None => ("unknown", "#8c959f"),
    };
    let svg = badge_svg("ci", text, color);
    let headers = [
        (header::CONTENT_TYPE, HeaderValue::from_static("image/svg+xml")),
        (header::CACHE_CONTROL, HeaderValue::from_static("no-cache, max-age=0")),
    ];
    (headers, svg).into_response()
}

async fn badge_status(st: &AppState, v: Option<&crate::auth::User>, o: &str, n: &str, branch: Option<String>) -> R<Option<String>> {
    let r = Repo::readable(st, v, o, n).await?;
    let branch = branch.unwrap_or_else(|| r.default_branch.clone());
    if !validate::ref_name(&branch) {
        return Ok(None);
    }
    let (id, refname) = (r.id, format!("refs/heads/{branch}"));
    st.db
        .call(move |c| {
            Ok(c.query_row("SELECT status FROM ci_runs WHERE repo_id = ?1 AND ref = ?2 AND event != 'pull_request' ORDER BY id DESC LIMIT 1", params![id, refname], |x| x.get(0))
                .optional()?)
        })
        .await
}

/// A flat two-part badge, sized by a rough per-character width.
fn badge_svg(label: &str, text: &str, color: &str) -> String {
    let w = |s: &str| 10 + 7 * s.chars().count();
    let (lw, rw) = (w(label), w(text));
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{t}" height="20" role="img" aria-label="{label}: {text}"><title>{label}: {text}</title><rect width="{lw}" height="20" fill="#555"/><rect x="{lw}" width="{rw}" height="20" fill="{color}"/><g fill="#fff" text-anchor="middle" font-family="Verdana,DejaVu Sans,sans-serif" font-size="11"><text x="{lx}" y="14">{label}</text><text x="{rx}" y="14">{text}</text></g></svg>"##,
        t = lw + rw,
        lx = lw / 2,
        rx = lw + rw / 2,
    )
}
