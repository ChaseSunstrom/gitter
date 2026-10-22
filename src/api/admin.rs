//! Health check and the admin dashboard API (instance stats, accounts, settings).

use std::path::Path as FsPath;

use axum::Json;
use axum::extract::{FromRequestParts, Path, Query, State};
use axum::http::request::Parts;
use rusqlite::types::Value;
use serde::Deserialize;
use serde_json::json;

use super::{ClientIp, J, like, ok};
use crate::AppState;
use crate::auth::{Authed, Sudo, User};
use crate::security::{Event, record};
use crate::error::{AppError, R};
use crate::settings::Settings;

/// A signed-in administrator.
pub struct Admin(pub User);

impl FromRequestParts<AppState> for Admin {
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, st: &AppState) -> R<Self> {
        let Authed(u) = Authed::from_request_parts(parts, st).await?;
        if !u.is_admin {
            return Err(AppError::forbidden());
        }
        Ok(Admin(u))
    }
}

/// Liveness + readiness for load balancers and the compose healthcheck.
pub async fn healthz(State(st): State<AppState>) -> R<Json<serde_json::Value>> {
    st.db
        .call(|c| Ok(c.query_row("SELECT 1", [], |r| r.get::<_, i64>(0))?))
        .await?;
    let repos_ok = tokio::fs::metadata(&st.git.root)
        .await
        .map(|m| m.is_dir())
        .unwrap_or(false);
    if !repos_ok {
        return Err(AppError::new(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "Repository storage is missing",
        ));
    }
    Ok(Json(
        json!({ "ok": true, "version": env!("CARGO_PKG_VERSION") }),
    ))
}

/// Total bytes under `dir` (follows no symlinks).
pub fn dir_size(dir: &FsPath) -> u64 {
    let mut total = 0;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            match e.file_type() {
                Ok(t) if t.is_dir() => stack.push(e.path()),
                Ok(t) if t.is_file() => total += e.metadata().map(|m| m.len()).unwrap_or(0),
                _ => {}
            }
        }
    }
    total
}

/// Free bytes on the filesystem holding `dir`, via `df` (portable across glibc and musl).
pub async fn free_space(dir: &FsPath) -> Option<u64> {
    let out = tokio::process::Command::new("df")
        .arg("-Pk")
        .arg(dir)
        .output()
        .await
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().nth(1)?;
    line.split_whitespace()
        .nth(3)?
        .parse::<u64>()
        .ok()
        .map(|k| k * 1024)
}

pub async fn git_version() -> String {
    tokio::process::Command::new("git")
        .arg("--version")
        .output()
        .await
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .trim()
                .trim_start_matches("git version ")
                .to_string()
        })
        .unwrap_or_else(|_| "not found".into())
}

pub async fn overview(State(st): State<AppState>, _a: Admin) -> J<serde_json::Value> {
    let counts = st
        .db
        .call(|c| {
            let n = |sql: &str| -> rusqlite::Result<i64> { c.query_row(sql, [], |r| r.get(0)) };
            let day = crate::db::now() - 86400;
            let mut s = c.prepare("SELECT username, created_at FROM users ORDER BY id DESC LIMIT 6")?;
            let recent: Vec<serde_json::Value> = s
                .query_map([], |r| Ok(json!({ "username": r.get::<_, String>(0)?, "created_at": r.get::<_, i64>(1)? })))?
                .collect::<Result<_, _>>()?;
            Ok(json!({
                "users": n("SELECT count(*) FROM users")?,
                "suspended": n("SELECT count(*) FROM users WHERE suspended = 1")?,
                "repos": n("SELECT count(*) FROM repos WHERE deleted_at IS NULL")?,
                "private_repos": n("SELECT count(*) FROM repos WHERE is_private = 1 AND deleted_at IS NULL")?,
                "open_issues": n("SELECT count(*) FROM issues WHERE is_pr = 0 AND state = 'open'")?,
                "open_pulls": n("SELECT count(*) FROM issues WHERE is_pr = 1 AND state = 'open'")?,
                "events_today": c.query_row("SELECT count(*) FROM events WHERE created_at > ?1", [day], |r| r.get::<_, i64>(0))?,
                "recent_users": recent,
            }))
        })
        .await?;
    let data = st.cfg.data_dir.clone();
    let repos_dir = st.git.root.clone();
    let (repo_bytes, db_bytes) = tokio::task::spawn_blocking(move || {
        let db = ["gitter.db", "gitter.db-wal"]
            .iter()
            .map(|f| {
                std::fs::metadata(data.join(f))
                    .map(|m| m.len())
                    .unwrap_or(0)
            })
            .sum::<u64>();
        (dir_size(&repos_dir), db)
    })
    .await
    .map_err(AppError::internal)?;
    let mut out = counts;
    out["version"] = json!(env!("CARGO_PKG_VERSION"));
    out["git_version"] = json!(git_version().await);
    out["uptime"] = json!(crate::db::now() - st.started);
    out["repo_bytes"] = json!(repo_bytes);
    out["db_bytes"] = json!(db_bytes);
    out["free_bytes"] = json!(free_space(&st.cfg.data_dir).await);
    out["data_dir"] = json!(st.cfg.data_dir.display().to_string());
    out["url"] = json!(st.cfg.url);
    Ok(Json(out))
}

#[derive(Deserialize)]
pub struct UsersQ {
    q: Option<String>,
    page: Option<usize>,
}

pub async fn users(
    State(st): State<AppState>,
    _a: Admin,
    Query(q): Query<UsersQ>,
) -> J<serde_json::Value> {
    let page = super::page(q.page);
    let out = st
        .db
        .call(move |c| {
            let mut cond = String::from("1=1");
            let mut params: Vec<Value> = vec![];
            if let Some(s) = q.q.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                cond.push_str(" AND (u.username LIKE ?1 ESCAPE '\\' OR u.email LIKE ?1 ESCAPE '\\' OR u.display_name LIKE ?1 ESCAPE '\\')");
                params.push(Value::Text(like(s)));
            }
            let per = 50;
            let mut s = c.prepare(&format!(
                "SELECT u.username, u.display_name, u.email, u.is_admin, u.suspended, u.created_at,
                    (SELECT count(*) FROM repos r WHERE r.owner_id = u.id AND r.deleted_at IS NULL),
                    (SELECT max(e.created_at) FROM events e WHERE e.user_id = u.id),
                    u.totp_secret IS NOT NULL
                 FROM users u WHERE {cond} ORDER BY u.id DESC LIMIT {} OFFSET {}",
                per + 1,
                (page - 1) * per
            ))?;
            let mut rows: Vec<serde_json::Value> = s
                .query_map(rusqlite::params_from_iter(params), |r| {
                    Ok(json!({
                        "username": r.get::<_, String>(0)?, "display_name": r.get::<_, String>(1)?, "email": r.get::<_, String>(2)?,
                        "is_admin": r.get::<_, i64>(3)? != 0, "suspended": r.get::<_, i64>(4)? != 0, "created_at": r.get::<_, i64>(5)?,
                        "repos": r.get::<_, i64>(6)?, "last_active": r.get::<_, Option<i64>>(7)?, "two_factor": r.get::<_, bool>(8)?,
                    }))
                })?
                .collect::<Result<_, _>>()?;
            let more = rows.len() > per;
            rows.truncate(per);
            Ok(json!({ "users": rows, "has_more": more, "page": page }))
        })
        .await?;
    Ok(Json(out))
}

#[derive(Deserialize)]
pub struct UserPatch {
    is_admin: Option<bool>,
    suspended: Option<bool>,
    /// Turn off someone's two-factor auth (they lost their phone and codes).
    #[serde(default)]
    reset_2fa: bool,
}

pub async fn update_user(
    State(st): State<AppState>,
    Admin(me): Admin,
    _s: Sudo,
    ip: ClientIp,
    Path(name): Path<String>,
    Json(p): Json<UserPatch>,
) -> J<serde_json::Value> {
    if name.eq_ignore_ascii_case(&me.username) {
        return Err(AppError::bad(
            "You can't change your own admin or suspension status",
        ));
    }
    let me2 = me.clone();
    st.db
        .call(move |c| {
            let tx = c.transaction()?;
            let id: i64 =
                tx.query_row("SELECT id FROM users WHERE username = ?1", [&name], |r| {
                    r.get(0)
                })?;
            let log = |action: &str, detail: String| {
                crate::security::append(&tx, &Event { actor: Some(&me2), subject: Some(id), action, detail, ip: &ip.0 })
            };
            if let Some(a) = p.is_admin {
                log(if a { "admin_granted" } else { "admin_revoked" }, name.clone())?;
            }
            if let Some(s) = p.suspended {
                log(if s { "user_suspended" } else { "user_unsuspended" }, name.clone())?;
            }
            if p.reset_2fa {
                crate::api::security::clear_2fa(&tx, id)?;
                log("2fa_reset", format!("{name}'s two-factor turned off by an admin"))?;
            }
            if p.is_admin == Some(false) || p.suspended == Some(true) {
                ensure_another_admin(&tx, id)?;
            }
            if let Some(a) = p.is_admin {
                tx.execute(
                    "UPDATE users SET is_admin = ?1 WHERE id = ?2",
                    rusqlite::params![a as i64, id],
                )?;
            }
            if let Some(s) = p.suspended {
                tx.execute(
                    "UPDATE users SET suspended = ?1 WHERE id = ?2",
                    rusqlite::params![s as i64, id],
                )?;
                if s {
                    tx.execute("DELETE FROM sessions WHERE user_id = ?1", [id])?;
                }
            }
            tx.commit()?;
            Ok(())
        })
        .await?;
    ok()
}

#[derive(Deserialize)]
pub struct DeleteUser {
    confirm: String,
}

pub async fn delete_user(
    State(st): State<AppState>,
    Admin(me): Admin,
    _s: Sudo,
    ip: ClientIp,
    Path(name): Path<String>,
    Json(d): Json<DeleteUser>,
) -> J<serde_json::Value> {
    if name.eq_ignore_ascii_case(&me.username) {
        return Err(AppError::bad("You can't delete your own account here"));
    }
    if !d.confirm.eq_ignore_ascii_case(&name) {
        return Err(AppError::bad(format!("Type {name} to confirm")));
    }
    super::lifecycle::delete_account(&st, &me, name, &ip.0).await?;
    ok()
}

/// Refuse a change that would leave the instance without an active admin.
/// Runs inside the change's transaction, so two admins can't demote each other at once.
pub(super) fn ensure_another_admin(tx: &rusqlite::Transaction, losing: i64) -> R<()> {
    let others: i64 = tx.query_row(
        "SELECT count(*) FROM users WHERE is_admin = 1 AND suspended = 0 AND id != ?1",
        [losing],
        |r| r.get(0),
    )?;
    let is_admin: bool = tx.query_row(
        "SELECT is_admin = 1 AND suspended = 0 FROM users WHERE id = ?1",
        [losing],
        |r| r.get(0),
    )?;
    if is_admin && others == 0 {
        return Err(AppError::bad(
            "That would leave no active admin. Make someone else an admin first.",
        ));
    }
    Ok(())
}

pub async fn get_settings(State(st): State<AppState>, _a: Admin) -> J<Settings> {
    Ok(Json(st.settings.get()))
}

pub async fn put_settings(
    State(st): State<AppState>,
    Admin(me): Admin,
    _s: Sudo,
    ip: ClientIp,
    Json(s): Json<Settings>,
) -> J<Settings> {
    let mut s = s;
    s.instance_name = s.instance_name.trim().to_string();
    s.announcement = s.announcement.trim().to_string();
    let before = st.settings.get();
    let changed = changed_fields(&before, &s);
    st.settings.save(&st.db, s).await?;
    if !changed.is_empty() {
        record(&st.db, Event { actor: Some(&me), subject: None, action: "settings_changed", detail: changed, ip: &ip.0 }).await;
    }
    Ok(Json(st.settings.get()))
}

/// "signup: open → closed, max_push_mb: 500 → 100" for the audit log.
fn changed_fields(a: &Settings, b: &Settings) -> String {
    let (a, b) = (serde_json::to_value(a).unwrap_or_default(), serde_json::to_value(b).unwrap_or_default());
    let mut out = vec![];
    if let (Some(a), Some(b)) = (a.as_object(), b.as_object()) {
        for (k, v) in b {
            if a.get(k) != Some(v) {
                out.push(format!("{k}: {} → {v}", a.get(k).cloned().unwrap_or_default()));
            }
        }
    }
    out.join(", ")
}

// ---------- invites (for invite-only sign-up) ----------

pub async fn invites(State(st): State<AppState>, _a: Admin) -> J<serde_json::Value> {
    let rows = st
        .db
        .call(|c| {
            let mut s = c.prepare(
                "SELECT i.id, i.note, i.max_uses, i.uses, i.expires_at, i.created_at, u.username FROM invites i
                 LEFT JOIN users u ON u.id = i.created_by ORDER BY i.id DESC LIMIT 200",
            )?;
            let t = crate::db::now();
            let v: Vec<serde_json::Value> = s
                .query_map([], |r| {
                    let (max, uses, exp): (i64, i64, Option<i64>) = (r.get(2)?, r.get(3)?, r.get(4)?);
                    Ok(json!({
                        "id": r.get::<_, i64>(0)?, "note": r.get::<_, String>(1)?, "max_uses": max, "uses": uses,
                        "expires_at": exp, "created_at": r.get::<_, i64>(5)?, "created_by": r.get::<_, Option<String>>(6)?,
                        "live": uses < max && exp.is_none_or(|e| e > t),
                    }))
                })?
                .collect::<Result<_, _>>()?;
            Ok(v)
        })
        .await?;
    Ok(Json(json!({ "invites": rows })))
}

#[derive(Deserialize)]
pub struct InviteReq {
    #[serde(default)]
    note: String,
    #[serde(default = "one")]
    max_uses: i64,
    expires_days: Option<i64>,
}

fn one() -> i64 {
    1
}

/// A sign-up link: /signup?invite=<code>. The code is shown once and stored hashed.
pub async fn create_invite(State(st): State<AppState>, Admin(me): Admin, ip: ClientIp, Json(req): Json<InviteReq>) -> J<serde_json::Value> {
    if !(1..=1000).contains(&req.max_uses) {
        return Err(AppError::bad("An invite can be used 1 to 1,000 times"));
    }
    if req.expires_days.is_some_and(|d| !(1..=365).contains(&d)) {
        return Err(AppError::bad("Invites expire after 1 to 365 days, or never"));
    }
    let note: String = req.note.trim().chars().take(120).collect();
    let code = crate::auth::new_secret("").0[..20].to_string();
    let (h, n2, uid, max) = (crate::auth::sha256_hex(&code), note.clone(), me.id, req.max_uses);
    let expires = req.expires_days.map(|d| crate::db::now() + d * 86400);
    st.db
        .call(move |c| {
            c.execute(
                "INSERT INTO invites(code_hash, created_by, note, max_uses, expires_at, created_at) VALUES (?1,?2,?3,?4,?5,?6)",
                rusqlite::params![h, uid, n2, max, expires, crate::db::now()],
            )?;
            Ok(())
        })
        .await?;
    record(&st.db, Event { actor: Some(&me), subject: None, action: "invite_created", detail: format!("{note} ({max} uses)"), ip: &ip.0 }).await;
    let url = format!("{}/signup?invite={code}", st.cfg.url.trim_end_matches('/'));
    Ok(Json(json!({ "code": code, "url": url })))
}

pub async fn delete_invite(State(st): State<AppState>, _a: Admin, Path(id): Path<i64>) -> J<serde_json::Value> {
    st.db.call(move |c| Ok(c.execute("DELETE FROM invites WHERE id = ?1", [id])?)).await?;
    ok()
}
