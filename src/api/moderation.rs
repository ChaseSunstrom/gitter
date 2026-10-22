//! Reports from users and the admin moderation queue. Resolving a report can
//! dismiss it, hide the content (only its owner and admins still see it), or
//! suspend the account behind it.

use axum::Json;
use axum::extract::{Path, Query, State};
use rusqlite::OptionalExtension;
use serde::Deserialize;
use serde_json::json;

use super::admin::Admin;
use super::{ClientIp, J};
use crate::AppState;
use crate::auth::Authed;
use crate::db::now;
use crate::error::{AppError, R};
use crate::repo::Repo;
use crate::security::{Event, record};

/// The report's target, whichever kind it is.
const TARGET: &str = "COALESCE(comment_id, issue_id, repo_id, user_id)";

fn column(kind: &str) -> &'static str {
    match kind {
        "repo" => "repo_id",
        "issue" => "issue_id",
        "comment" => "comment_id",
        _ => "user_id",
    }
}

pub const REASONS: &[&str] = &["spam", "abuse", "illegal", "malware", "impersonation", "other"];

#[derive(Deserialize)]
pub struct ReportReq {
    /// "repo", "issue", "comment" or "user".
    kind: String,
    #[serde(default)]
    owner: String,
    #[serde(default)]
    repo: String,
    /// Issue or PR number (kind "issue").
    number: Option<i64>,
    /// Comment id (kind "comment").
    comment: Option<i64>,
    /// Username (kind "user").
    #[serde(default)]
    user: String,
    reason: String,
    #[serde(default)]
    details: String,
}

/// Report something you can see. One open report per person per thing.
pub async fn report(
    State(st): State<AppState>,
    Authed(u): Authed,
    Json(req): Json<ReportReq>,
) -> J<serde_json::Value> {
    if !REASONS.contains(&req.reason.as_str()) {
        return Err(AppError::bad("Pick a reason"));
    }
    if req.details.chars().count() > 2000 {
        return Err(AppError::bad("Keep the details under 2,000 characters"));
    }
    if let Err(wait) = st.limiter.check(&format!("report:{}", u.id), 20, 3600) {
        return Err(AppError::slow_down("You've sent a lot of reports. Try again later.", wait));
    }
    let target = match req.kind.as_str() {
        "user" => {
            let name = req.user.clone();
            st.db
                .call(move |c| Ok(c.query_row("SELECT id FROM users WHERE username = ?1", [name], |r| r.get::<_, i64>(0)).optional()?))
                .await?
        }
        kind @ ("repo" | "issue" | "comment") => {
            // Only what the reporter can see, so reports can't probe private repos.
            let r = Repo::readable(&st, Some(&u), &req.owner, &req.repo).await?;
            let rid = r.id;
            match kind {
                "repo" => Some(r.id),
                "issue" => {
                    let n = req.number.unwrap_or(0);
                    st.db
                        .call(move |c| Ok(c.query_row("SELECT id FROM issues WHERE repo_id = ?1 AND number = ?2 AND hidden = 0", [rid, n], |r| r.get::<_, i64>(0)).optional()?))
                        .await?
                }
                _ => {
                    let id = req.comment.unwrap_or(0);
                    st.db
                        .call(move |c| {
                            Ok(c.query_row(
                                "SELECT c.id FROM comments c JOIN issues i ON i.id = c.issue_id WHERE c.id = ?1 AND i.repo_id = ?2 AND c.hidden = 0 AND i.hidden = 0",
                                [id, rid],
                                |r| r.get::<_, i64>(0),
                            )
                            .optional()?)
                        })
                        .await?
                }
            }
        }
        _ => return Err(AppError::bad("Unknown kind of report")),
    };
    let target = target.ok_or_else(AppError::not_found)?;
    let (uid, kind, reason, details) = (u.id, req.kind.clone(), req.reason.clone(), req.details.trim().to_string());
    st.db
        .call(move |c| {
            let dup = c
                .query_row(
                    &format!("SELECT 1 FROM reports WHERE reporter_id = ?1 AND kind = ?2 AND {TARGET} = ?3 AND status = 'open'"),
                    rusqlite::params![uid, kind, target],
                    |_| Ok(()),
                )
                .optional()?;
            if dup.is_some() {
                return Err(AppError::conflict("You've already reported this. A moderator will look at it."));
            }
            c.execute(
                &format!("INSERT INTO reports(reporter_id, kind, {}, reason, details, created_at) VALUES (?1,?2,?3,?4,?5,?6)", column(&kind)),
                rusqlite::params![uid, kind, target, reason, details, now()],
            )?;
            Ok(())
        })
        .await?;
    super::ok()
}

#[derive(Deserialize)]
pub struct QueueQ {
    status: Option<String>,
}

/// What a report points at, for the queue: a label, a link, its author, whether it's hidden.
fn describe(c: &rusqlite::Connection, kind: &str, id: i64) -> rusqlite::Result<serde_json::Value> {
    let row = match kind {
        "repo" => c
            .query_row(
                "SELECT u.username || '/' || r.name, '/' || u.username || '/' || r.name, u.username, r.hidden, r.description FROM repos r JOIN users u ON u.id = r.owner_id WHERE r.id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()?,
        "issue" => c
            .query_row(
                "SELECT ou.username || '/' || r.name || '#' || i.number || ' ' || i.title,
                        '/' || ou.username || '/' || r.name || (CASE WHEN i.is_pr = 1 THEN '/pull/' ELSE '/issues/' END) || i.number,
                        au.username, i.hidden, substr(i.body, 1, 300)
                 FROM issues i JOIN repos r ON r.id = i.repo_id JOIN users ou ON ou.id = r.owner_id JOIN users au ON au.id = i.author_id WHERE i.id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()?,
        "comment" => c
            .query_row(
                "SELECT 'Comment on ' || ou.username || '/' || r.name || '#' || i.number,
                        '/' || ou.username || '/' || r.name || (CASE WHEN i.is_pr = 1 THEN '/pull/' ELSE '/issues/' END) || i.number || '#c' || cm.id,
                        au.username, cm.hidden, substr(cm.body, 1, 300)
                 FROM comments cm JOIN issues i ON i.id = cm.issue_id JOIN repos r ON r.id = i.repo_id
                 JOIN users ou ON ou.id = r.owner_id JOIN users au ON au.id = cm.author_id WHERE cm.id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()?,
        _ => c
            .query_row(
                "SELECT username, '/' || username, username, suspended, bio FROM users WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()?,
    };
    Ok(match row {
        Some((label, url, author, hidden, excerpt)) => {
            let (label, url, author, hidden, excerpt): (String, String, String, i64, String) = (label, url, author, hidden, excerpt);
            json!({ "label": label, "url": url, "author": author, "hidden": hidden != 0, "excerpt": excerpt })
        }
        None => json!({ "label": "(deleted)", "url": null, "author": null, "hidden": false, "excerpt": "" }),
    })
}

/// SQL condition: report `rp` isn't about a repository in the trash (or something in one).
/// Those wait out of the queue: they're purged with it, or come back if it's restored.
const NOT_TRASHED: &str = "NOT EXISTS (SELECT 1 FROM repos x WHERE x.deleted_at IS NOT NULL AND x.id IN (rp.repo_id,
    (SELECT repo_id FROM issues WHERE id = rp.issue_id),
    (SELECT i.repo_id FROM comments cm JOIN issues i ON i.id = cm.issue_id WHERE cm.id = rp.comment_id)))";

pub async fn queue(State(st): State<AppState>, _a: Admin, Query(q): Query<QueueQ>) -> J<serde_json::Value> {
    let open = q.status.as_deref() != Some("resolved");
    let out = st
        .db
        .call(move |c| {
            let mut s = c.prepare(&format!(
                "SELECT rp.id, rp.kind, COALESCE(rp.comment_id, rp.issue_id, rp.repo_id, rp.user_id), rp.reason, rp.details, rp.created_at, u.username, rp.resolution, ru.username, rp.resolved_at
                 FROM reports rp LEFT JOIN users u ON u.id = rp.reporter_id LEFT JOIN users ru ON ru.id = rp.resolved_by
                 WHERE rp.status {} AND {NOT_TRASHED} ORDER BY rp.id DESC LIMIT 200",
                if open { "= 'open'" } else { "= 'resolved'" }
            ))?;
            let rows: Vec<(i64, String, i64, String, String, i64, Option<String>, String, Option<String>, Option<i64>)> = s
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?)))?
                .collect::<Result<_, _>>()?;
            let mut out = Vec::new();
            for (id, kind, target, reason, details, at, by, resolution, resolver, resolved_at) in rows {
                out.push(json!({
                    "id": id, "kind": kind, "reason": reason, "details": details, "created_at": at, "reporter": by,
                    "resolution": resolution, "resolved_by": resolver, "resolved_at": resolved_at,
                    "target": describe(c, &kind, target)?,
                }));
            }
            let open_count: i64 = c.query_row(&format!("SELECT count(*) FROM reports rp WHERE rp.status = 'open' AND {NOT_TRASHED}"), [], |r| r.get(0))?;
            Ok(json!({ "reports": out, "open": open_count }))
        })
        .await?;
    Ok(Json(out))
}

#[derive(Deserialize)]
pub struct ResolveReq {
    /// "dismiss", "hide", "unhide" or "suspend".
    action: String,
    #[serde(default)]
    note: String,
}

/// Act on a report. Hiding or suspending closes every open report on the same thing.
pub async fn resolve(
    State(st): State<AppState>,
    Admin(me): Admin,
    ip: ClientIp,
    Path(id): Path<i64>,
    Json(req): Json<ResolveReq>,
) -> J<serde_json::Value> {
    if !matches!(req.action.as_str(), "dismiss" | "hide" | "unhide" | "suspend") {
        return Err(AppError::bad("Choose dismiss, hide, unhide or suspend"));
    }
    let (action, note, mid) = (req.action.clone(), req.note.trim().chars().take(500).collect::<String>(), me.id);
    let (kind, label, subject) = st
        .db
        .call(move |c| -> R<(String, String, Option<i64>)> {
            let tx = c.transaction()?;
            let (kind, target): (String, i64) =
                tx.query_row(&format!("SELECT kind, {TARGET} FROM reports WHERE id = ?1"), [id], |r| Ok((r.get(0)?, r.get(1)?)))?;
            let label = describe(&tx, &kind, target)?["label"].as_str().unwrap_or("").to_string();
            // The account behind the content: the one to suspend, and whose log records it.
            let author: Option<i64> = match kind.as_str() {
                "user" => Some(target),
                "repo" => tx.query_row("SELECT owner_id FROM repos WHERE id = ?1", [target], |r| r.get(0)).optional()?,
                "issue" => tx.query_row("SELECT author_id FROM issues WHERE id = ?1", [target], |r| r.get(0)).optional()?,
                _ => tx.query_row("SELECT author_id FROM comments WHERE id = ?1", [target], |r| r.get(0)).optional()?,
            };
            let table = match kind.as_str() {
                "repo" => Some("repos"),
                "issue" => Some("issues"),
                "comment" => Some("comments"),
                _ => None,
            };
            match action.as_str() {
                "hide" | "unhide" => {
                    let t = table.ok_or_else(|| AppError::bad("Accounts are suspended, not hidden"))?;
                    tx.execute(&format!("UPDATE {t} SET hidden = ?1 WHERE id = ?2"), rusqlite::params![(action == "hide") as i64, target])?;
                }
                "suspend" => {
                    let a = author.ok_or_else(AppError::not_found)?;
                    let admin: bool = tx.query_row("SELECT is_admin FROM users WHERE id = ?1", [a], |r| r.get(0))?;
                    if admin {
                        return Err(AppError::bad("That account is an admin. Remove its admin rights first."));
                    }
                    tx.execute("UPDATE users SET suspended = 1 WHERE id = ?1", [a])?;
                    tx.execute("DELETE FROM sessions WHERE user_id = ?1", [a])?;
                }
                _ => {}
            }
            let resolution = if note.is_empty() { action.clone() } else { format!("{action}: {note}") };
            let close_all = matches!(action.as_str(), "hide" | "suspend");
            tx.execute(
                &format!(
                    "UPDATE reports SET status = 'resolved', resolution = ?1, resolved_by = ?2, resolved_at = ?3 WHERE {}",
                    if close_all { format!("(kind = ?4 AND {TARGET} = ?5 AND status = 'open') OR id = ?6") } else { "id = ?6 AND ?4 = ?4 AND ?5 = ?5".to_string() }
                ),
                rusqlite::params![resolution, mid, now(), kind, target, id],
            )?;
            tx.commit()?;
            Ok((kind, label, author))
        })
        .await?;
    let action = format!("moderation_{}", req.action);
    record(&st.db, Event { actor: Some(&me), subject, action: &action, detail: format!("{kind}: {label}"), ip: &ip.0 }).await;
    super::ok()
}
