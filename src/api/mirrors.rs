//! Repository mirrors in the API (Settings → Mirrors): list, add, change,
//! remove and "sync now". For the repository's admins; adding, changing and
//! removing need a fresh sign-in, as webhooks do, since a mirror sends the
//! code out or replaces it. Passwords go in and never come back out.

use axum::Json;
use axum::extract::{Path, State};
use rusqlite::{OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{ClientIp, J, ok};
use crate::AppState;
use crate::auth::{Authed, Sudo};
use crate::db::now;
use crate::error::{AppError, R};
use crate::mirrors::{MAX_INTERVAL, MIN_INTERVAL};
use crate::repo::Repo;
use crate::security::{Event, record};

/// Mirrors per repository.
const MAX_MIRRORS: i64 = 10;

#[derive(Deserialize)]
pub struct MirrorReq {
    /// "pull" or "push"; set when adding only.
    direction: Option<String>,
    url: Option<String>,
    username: Option<String>,
    /// Missing or empty keeps the current one (an empty username drops both).
    password: Option<String>,
    interval_minutes: Option<i64>,
    allow_private: Option<bool>,
}

fn check_url(url: &str) -> R<String> {
    let url = url.trim();
    if url.len() > 2048 {
        return Err(AppError::bad("Keep the URL under 2,048 characters"));
    }
    crate::net::check_url(url).map_err(AppError::bad)?;
    Ok(url.to_string())
}

fn check_username(u: &str) -> R<String> {
    let u = u.trim();
    if u.chars().count() > 200 || u.contains(':') || u.chars().any(char::is_control) {
        return Err(AppError::bad("A user name is up to 200 characters, without ':'"));
    }
    Ok(u.to_string())
}

fn sealed(st: &AppState, password: &str) -> R<Vec<u8>> {
    if password.len() > 1024 || password.chars().any(char::is_control) {
        return Err(AppError::bad("A password or token is up to 1,024 characters on one line"));
    }
    Ok(crate::security::seal(&st.key, password.as_bytes()))
}

fn check_interval(minutes: i64) -> R<i64> {
    let secs = minutes.saturating_mul(60);
    if !(MIN_INTERVAL..=MAX_INTERVAL).contains(&secs) {
        return Err(AppError::bad("Sync every 10 minutes to every 7 days"));
    }
    Ok(secs)
}

fn admin_only() -> AppError {
    AppError::new(axum::http::StatusCode::FORBIDDEN, "Only site admins can let a mirror reach private addresses")
}

/// What the audit log keeps: the host, not the whole URL.
fn detail(url: &str, r: &Repo, direction: &str) -> String {
    let host = url.parse::<ureq::http::Uri>().ok().and_then(|u| u.host().map(String::from)).unwrap_or_default();
    format!("{direction} mirror of {} with {host}", r.full_name())
}

const COLS: &str = "id, direction, url, username, secret IS NOT NULL, allow_private, interval_secs, next_at, running_until, failures,
    last_ok_at, last_tried_at, last_error, created_at";

fn mirror_json(r: &rusqlite::Row) -> rusqlite::Result<Value> {
    let running: Option<i64> = r.get(8)?;
    Ok(json!({
        "id": r.get::<_, i64>(0)?, "direction": r.get::<_, String>(1)?, "url": r.get::<_, String>(2)?, "username": r.get::<_, String>(3)?,
        "has_password": r.get::<_, bool>(4)?, "allow_private": r.get::<_, bool>(5)?, "interval_minutes": r.get::<_, i64>(6)? / 60,
        "next_at": r.get::<_, Option<i64>>(7)?, "running": running.is_some_and(|t| t > now()), "failures": r.get::<_, i64>(9)?,
        "last_ok_at": r.get::<_, Option<i64>>(10)?, "last_tried_at": r.get::<_, Option<i64>>(11)?, "last_error": r.get::<_, String>(12)?,
        "created_at": r.get::<_, i64>(13)?,
    }))
}

async fn load(st: &AppState, r: &Repo, id: i64) -> R<Value> {
    let rid = r.id;
    st.db
        .call(move |c| Ok(c.query_row(&format!("SELECT {COLS} FROM mirrors WHERE id = ?1 AND repo_id = ?2"), [id, rid], mirror_json).optional()?))
        .await?
        .ok_or_else(AppError::not_found)
}

pub async fn list(State(st): State<AppState>, Authed(u): Authed, Path((o, n)): Path<(String, String)>) -> J<Value> {
    let r = Repo::administrable(&st, Some(&u), &o, &n).await?;
    let rid = r.id;
    let mirrors: Vec<Value> = st
        .db
        .call(move |c| {
            let mut q = c.prepare(&format!("SELECT {COLS} FROM mirrors WHERE repo_id = ?1 ORDER BY direction, id"))?;
            Ok(q.query_map([rid], mirror_json)?.collect::<Result<_, _>>()?)
        })
        .await?;
    // Only an empty repository can become a pull mirror: syncing replaces every branch and tag.
    let empty = st.git.refs(&r.path).await?.is_empty();
    Ok(Json(json!({ "mirrors": mirrors, "can_pull": empty && !r.mirror, "can_allow_private": u.is_admin, "min_interval_minutes": MIN_INTERVAL / 60 })))
}

pub async fn create(
    State(st): State<AppState>,
    Sudo(u): Sudo,
    ip: ClientIp,
    Path((o, n)): Path<(String, String)>,
    Json(req): Json<MirrorReq>,
) -> J<Value> {
    let r = Repo::administrable(&st, Some(&u), &o, &n).await?;
    let direction = req.direction.unwrap_or_default();
    if !matches!(direction.as_str(), "pull" | "push") {
        return Err(AppError::bad("A mirror pulls from its remote or pushes to it"));
    }
    let url = check_url(req.url.as_deref().unwrap_or_default())?;
    let username = check_username(req.username.as_deref().unwrap_or_default())?;
    let secret = req.password.as_deref().filter(|p| !p.is_empty()).map(|p| sealed(&st, p)).transpose()?;
    let interval = check_interval(req.interval_minutes.unwrap_or(480))?;
    let private = req.allow_private.unwrap_or(false);
    if private && !u.is_admin {
        return Err(admin_only());
    }
    let pull = direction == "pull";
    if pull && !st.git.refs(&r.path).await?.is_empty() {
        return Err(AppError::conflict("Only an empty repository can become a pull mirror: syncing replaces all its branches and tags. Make a new repository for it."));
    }
    let (rid, url2, dir2) = (r.id, url.clone(), direction.clone());
    let id = st
        .db
        .call(move |c| {
            let tx = c.transaction()?;
            let count: i64 = tx.query_row("SELECT count(*) FROM mirrors WHERE repo_id = ?1", [rid], |r| r.get(0))?;
            if count >= MAX_MIRRORS {
                return Err(AppError::bad(format!("A repository can have up to {MAX_MIRRORS} mirrors")));
            }
            if pull && tx.query_row("SELECT 1 FROM mirrors WHERE repo_id = ?1 AND direction = 'pull'", [rid], |_| Ok(())).optional()?.is_some() {
                return Err(AppError::conflict("This repository already mirrors a remote"));
            }
            let t = now();
            tx.execute(
                "INSERT INTO mirrors(repo_id, direction, url, username, secret, allow_private, interval_secs, next_at, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
                params![rid, dir2, url2, username, secret, private, interval, t],
            )?;
            let id = tx.last_insert_rowid();
            tx.commit()?;
            Ok(id)
        })
        .await?;
    record(&st.db, Event { actor: Some(&u), subject: Some(r.owner_id), action: "mirror_created", detail: detail(&url, &r, &direction), ip: &ip.0 }).await;
    Ok(Json(json!({ "id": id })))
}

pub async fn update(
    State(st): State<AppState>,
    Sudo(u): Sudo,
    ip: ClientIp,
    Path((o, n, id)): Path<(String, String, i64)>,
    Json(req): Json<MirrorReq>,
) -> J<Value> {
    let r = Repo::administrable(&st, Some(&u), &o, &n).await?;
    let cur = load(&st, &r, id).await?;
    let url = req.url.as_deref().map(check_url).transpose()?;
    let moved = url.as_deref().is_some_and(|x| Some(x) != cur["url"].as_str());
    let was_private = cur["allow_private"].as_bool().unwrap_or(false);
    let mut private = req.allow_private.unwrap_or(was_private);
    // Only an admin turns this on, and it holds for the URL the admin saw.
    if !u.is_admin && private && (moved || !was_private) {
        if req.allow_private == Some(true) {
            return Err(admin_only());
        }
        private = false;
    }
    let username = req.username.as_deref().map(check_username).transpose()?;
    let secret = req.password.as_deref().filter(|p| !p.is_empty()).map(|p| sealed(&st, p)).transpose()?;
    // A new remote doesn't get the old one's password.
    let drop_secret = secret.is_none() && (moved || username.as_deref() == Some(""));
    let interval = req.interval_minutes.map(check_interval).transpose()?;
    let (rid, url2) = (r.id, url.clone());
    st.db
        .call(move |c| {
            Ok(c.execute(
                "UPDATE mirrors SET url = coalesce(?3, url), username = coalesce(?4, username),
                 secret = CASE WHEN ?6 THEN NULL ELSE coalesce(?5, secret) END, allow_private = ?7,
                 interval_secs = coalesce(?8, interval_secs), failures = 0, next_at = ?9 WHERE id = ?1 AND repo_id = ?2",
                params![id, rid, url2, username, secret, drop_secret, private, interval, now()],
            )?)
        })
        .await?;
    let shown = url.as_deref().or(cur["url"].as_str()).unwrap_or_default();
    let d = detail(shown, &r, cur["direction"].as_str().unwrap_or_default());
    record(&st.db, Event { actor: Some(&u), subject: Some(r.owner_id), action: "mirror_changed", detail: d, ip: &ip.0 }).await;
    ok()
}

pub async fn remove(State(st): State<AppState>, Sudo(u): Sudo, ip: ClientIp, Path((o, n, id)): Path<(String, String, i64)>) -> J<Value> {
    let r = Repo::administrable(&st, Some(&u), &o, &n).await?;
    let cur = load(&st, &r, id).await?;
    let rid = r.id;
    st.db.call(move |c| Ok(c.execute("DELETE FROM mirrors WHERE id = ?1 AND repo_id = ?2", [id, rid])?)).await?;
    let d = detail(cur["url"].as_str().unwrap_or_default(), &r, cur["direction"].as_str().unwrap_or_default());
    record(&st.db, Event { actor: Some(&u), subject: Some(r.owner_id), action: "mirror_deleted", detail: d, ip: &ip.0 }).await;
    ok()
}

/// "Sync now": due at once (after the sync in progress, if one is).
pub async fn sync(State(st): State<AppState>, Authed(u): Authed, Path((o, n, id)): Path<(String, String, i64)>) -> J<Value> {
    let r = Repo::administrable(&st, Some(&u), &o, &n).await?;
    let rid = r.id;
    let n = st.db.call(move |c| Ok(c.execute("UPDATE mirrors SET next_at = ?3 WHERE id = ?1 AND repo_id = ?2", params![id, rid, now()])?)).await?;
    if n == 0 {
        return Err(AppError::not_found());
    }
    ok()
}
