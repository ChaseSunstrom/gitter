//! Repository webhooks in the API: list, add (which sends a ping), change,
//! remove, ping, the delivery log and redelivery. For people who can write
//! to the repository; adding, changing and removing a hook need a fresh
//! sign-in, as CI secrets do, since a hook sends the repository's activity out.

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
use crate::repo::Repo;
use crate::security::{Event, record};
use crate::webhooks::{self, EVENTS};

/// Hooks per repository.
const MAX_HOOKS: i64 = 20;

#[derive(Deserialize)]
pub struct HookReq {
    url: Option<String>,
    /// Missing or empty keeps the current one.
    secret: Option<String>,
    events: Option<Vec<String>>,
    active: Option<bool>,
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

/// "*" or the chosen events, comma-separated in EVENTS order.
fn check_events(events: &[String]) -> R<String> {
    if events.iter().any(|e| e == "*") {
        return Ok("*".into());
    }
    if events.iter().any(|e| !EVENTS.contains(&e.as_str())) {
        return Err(AppError::bad(format!("Events are {} (or * for all)", EVENTS.join(", "))));
    }
    let picked: Vec<&str> = EVENTS.iter().copied().filter(|e| events.iter().any(|x| x == e)).collect();
    if picked.is_empty() {
        return Err(AppError::bad("Pick at least one event"));
    }
    Ok(picked.join(","))
}

fn sealed_secret(st: &AppState, s: &str) -> R<Vec<u8>> {
    if s.len() > 1024 {
        return Err(AppError::bad("Keep the secret under 1,024 characters"));
    }
    Ok(crate::security::seal(&st.key, s.as_bytes()))
}

fn admin_only() -> AppError {
    AppError::new(axum::http::StatusCode::FORBIDDEN, "Only site admins can let a webhook reach private addresses")
}

/// What the audit log keeps: the host, not the whole URL (it may carry a token).
fn detail(url: &str, r: &Repo, private: bool) -> String {
    let host = url.parse::<ureq::http::Uri>().ok().and_then(|u| u.host().map(String::from)).unwrap_or_default();
    format!("{host} in {}{}", r.full_name(), if private { " (private addresses allowed)" } else { "" })
}

const HOOK_COLS: &str = "h.id, h.url, h.events, h.active, h.allow_private, h.created_at, h.updated_at,
    (SELECT json_object('status', d.status, 'response_code', d.response_code, 'error', d.error, 'at', d.tried_at)
     FROM webhook_deliveries d WHERE d.hook_id = h.id AND d.attempts > 0 ORDER BY d.id DESC LIMIT 1)";

fn hook_json(r: &rusqlite::Row) -> rusqlite::Result<Value> {
    let events: String = r.get(2)?;
    Ok(json!({
        "id": r.get::<_, i64>(0)?, "url": r.get::<_, String>(1)?, "events": events.split(',').collect::<Vec<_>>(),
        "active": r.get::<_, bool>(3)?, "allow_private": r.get::<_, bool>(4)?, "created_at": r.get::<_, i64>(5)?, "updated_at": r.get::<_, i64>(6)?,
        "last": r.get::<_, Option<String>>(7)?.and_then(|s| serde_json::from_str::<Value>(&s).ok()),
    }))
}

async fn load(st: &AppState, r: &Repo, id: i64) -> R<Value> {
    let rid = r.id;
    st.db
        .call(move |c| Ok(c.query_row(&format!("SELECT {HOOK_COLS} FROM webhooks h WHERE h.id = ?1 AND h.repo_id = ?2"), [id, rid], hook_json).optional()?))
        .await?
        .ok_or_else(AppError::not_found)
}

pub async fn list(State(st): State<AppState>, Authed(u): Authed, Path((o, n)): Path<(String, String)>) -> J<Value> {
    let r = Repo::administrable(&st, Some(&u), &o, &n).await?;
    let rid = r.id;
    let hooks: Vec<Value> = st
        .db
        .call(move |c| {
            let mut q = c.prepare(&format!("SELECT {HOOK_COLS} FROM webhooks h WHERE h.repo_id = ?1 ORDER BY h.id"))?;
            Ok(q.query_map([rid], hook_json)?.collect::<Result<_, _>>()?)
        })
        .await?;
    Ok(Json(json!({ "hooks": hooks, "events": EVENTS, "can_allow_private": u.is_admin })))
}

pub async fn create(
    State(st): State<AppState>,
    Sudo(u): Sudo,
    ip: ClientIp,
    Path((o, n)): Path<(String, String)>,
    Json(req): Json<HookReq>,
) -> J<Value> {
    let r = Repo::administrable(&st, Some(&u), &o, &n).await?;
    let url = check_url(req.url.as_deref().unwrap_or_default())?;
    let secret = req.secret.unwrap_or_default();
    if secret.is_empty() {
        return Err(AppError::bad("Add a secret: your receiver uses it to check that deliveries come from here"));
    }
    let sealed = sealed_secret(&st, &secret)?;
    let events = check_events(&req.events.unwrap_or_else(|| vec!["push".into()]))?;
    let private = req.allow_private.unwrap_or(false);
    if private && !u.is_admin {
        return Err(admin_only());
    }
    let (rid, url2, active) = (r.id, url.clone(), req.active.unwrap_or(true));
    let id = st
        .db
        .call(move |c| {
            let tx = c.transaction()?;
            let count: i64 = tx.query_row("SELECT count(*) FROM webhooks WHERE repo_id = ?1", [rid], |r| r.get(0))?;
            if count >= MAX_HOOKS {
                return Err(AppError::bad(format!("A repository can have up to {MAX_HOOKS} webhooks")));
            }
            tx.execute(
                "INSERT INTO webhooks(repo_id, url, secret, events, active, allow_private, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
                params![rid, url2, sealed, events, active, private, now()],
            )?;
            let id = tx.last_insert_rowid();
            tx.commit()?;
            Ok(id)
        })
        .await?;
    record(&st.db, Event { actor: Some(&u), subject: Some(r.owner_id), action: "webhook_created", detail: detail(&url, &r, private), ip: &ip.0 }).await;
    webhooks::ping(&st, r.id, id, u.id).await;
    Ok(Json(json!({ "id": id })))
}

pub async fn update(
    State(st): State<AppState>,
    Sudo(u): Sudo,
    ip: ClientIp,
    Path((o, n, id)): Path<(String, String, i64)>,
    Json(req): Json<HookReq>,
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
    let secret = match req.secret.as_deref() {
        Some(s) if !s.is_empty() => Some(sealed_secret(&st, s)?),
        _ => None,
    };
    let events = req.events.as_deref().map(check_events).transpose()?;
    let (rid, url2, active) = (r.id, url.clone(), req.active);
    st.db
        .call(move |c| {
            Ok(c.execute(
                "UPDATE webhooks SET url = coalesce(?3, url), secret = coalesce(?4, secret), events = coalesce(?5, events),
                 active = coalesce(?6, active), allow_private = ?7, updated_at = ?8 WHERE id = ?1 AND repo_id = ?2",
                params![id, rid, url2, secret, events, active, private, now()],
            )?)
        })
        .await?;
    let shown = url.as_deref().or(cur["url"].as_str()).unwrap_or_default();
    record(&st.db, Event { actor: Some(&u), subject: Some(r.owner_id), action: "webhook_changed", detail: detail(shown, &r, private), ip: &ip.0 }).await;
    ok()
}

pub async fn remove(State(st): State<AppState>, Sudo(u): Sudo, ip: ClientIp, Path((o, n, id)): Path<(String, String, i64)>) -> J<Value> {
    let r = Repo::administrable(&st, Some(&u), &o, &n).await?;
    let cur = load(&st, &r, id).await?;
    let rid = r.id;
    st.db.call(move |c| Ok(c.execute("DELETE FROM webhooks WHERE id = ?1 AND repo_id = ?2", [id, rid])?)).await?;
    let d = detail(cur["url"].as_str().unwrap_or_default(), &r, false);
    record(&st.db, Event { actor: Some(&u), subject: Some(r.owner_id), action: "webhook_deleted", detail: d, ip: &ip.0 }).await;
    ok()
}

pub async fn ping(State(st): State<AppState>, Authed(u): Authed, Path((o, n, id)): Path<(String, String, i64)>) -> J<Value> {
    let r = Repo::administrable(&st, Some(&u), &o, &n).await?;
    if load(&st, &r, id).await?["active"] != true {
        return Err(AppError::conflict("Turn the webhook on first"));
    }
    if !webhooks::ping(&st, r.id, id, u.id).await {
        return Err(AppError::conflict("Too many deliveries are waiting for this webhook. Wait for them to go out."));
    }
    ok()
}

const DELIVERY_COLS: &str =
    "d.id, d.guid, d.event, d.redelivery, d.status, d.attempts, d.next_at, d.response_code, d.error, d.duration_ms, d.created_at, d.tried_at";

fn delivery_json(r: &rusqlite::Row) -> rusqlite::Result<Value> {
    Ok(json!({
        "id": r.get::<_, i64>(0)?, "guid": r.get::<_, String>(1)?, "event": r.get::<_, String>(2)?, "redelivery": r.get::<_, bool>(3)?,
        "status": r.get::<_, String>(4)?, "attempts": r.get::<_, i64>(5)?, "next_at": r.get::<_, Option<i64>>(6)?,
        "response_code": r.get::<_, Option<i64>>(7)?, "error": r.get::<_, String>(8)?, "duration_ms": r.get::<_, Option<i64>>(9)?,
        "created_at": r.get::<_, i64>(10)?, "tried_at": r.get::<_, Option<i64>>(11)?,
    }))
}

/// A hook and its newest deliveries.
pub async fn show(State(st): State<AppState>, Authed(u): Authed, Path((o, n, id)): Path<(String, String, i64)>) -> J<Value> {
    let r = Repo::administrable(&st, Some(&u), &o, &n).await?;
    let hook = load(&st, &r, id).await?;
    let deliveries: Vec<Value> = st
        .db
        .call(move |c| {
            let mut q = c.prepare(&format!("SELECT {DELIVERY_COLS} FROM webhook_deliveries d WHERE d.hook_id = ?1 ORDER BY d.id DESC LIMIT ?2"))?;
            Ok(q.query_map(params![id, webhooks::KEEP], delivery_json)?.collect::<Result<_, _>>()?)
        })
        .await?;
    Ok(Json(json!({ "hook": hook, "deliveries": deliveries, "events": EVENTS, "can_allow_private": u.is_admin })))
}

/// One delivery with what was sent and what came back.
pub async fn delivery(State(st): State<AppState>, Authed(u): Authed, Path((o, n, id, did)): Path<(String, String, i64, i64)>) -> J<Value> {
    let r = Repo::administrable(&st, Some(&u), &o, &n).await?;
    let rid = r.id;
    st.db
        .call(move |c| {
            Ok(c.query_row(
                &format!(
                    "SELECT {DELIVERY_COLS}, d.payload, d.request_headers, d.response_body FROM webhook_deliveries d
                     JOIN webhooks h ON h.id = d.hook_id WHERE d.id = ?1 AND d.hook_id = ?2 AND h.repo_id = ?3"
                ),
                [did, id, rid],
                |r| {
                    let mut v = delivery_json(r)?;
                    v["payload"] = json!(r.get::<_, String>(12)?);
                    v["request_headers"] = serde_json::from_str(&r.get::<_, String>(13)?).unwrap_or(Value::Null);
                    v["response_body"] = json!(r.get::<_, String>(14)?);
                    Ok(v)
                },
            )
            .optional()?)
        })
        .await?
        .map(Json)
        .ok_or_else(AppError::not_found)
}

/// Send a delivery's payload again (same event and delivery id, signed with the current secret).
pub async fn redeliver(State(st): State<AppState>, Authed(u): Authed, Path((o, n, id, did)): Path<(String, String, i64, i64)>) -> J<Value> {
    let r = Repo::administrable(&st, Some(&u), &o, &n).await?;
    load(&st, &r, id).await?;
    let new_id = st
        .db
        .call(move |c| {
            let waiting: i64 = c.query_row("SELECT count(*) FROM webhook_deliveries WHERE hook_id = ?1 AND next_at IS NOT NULL", [id], |r| r.get(0))?;
            if waiting >= webhooks::MAX_WAITING {
                return Err(AppError::conflict("Too many deliveries are waiting for this webhook. Wait for them to go out."));
            }
            let t = now();
            let n = c.execute(
                "INSERT INTO webhook_deliveries(hook_id, guid, event, payload, redelivery, next_at, created_at)
                 SELECT hook_id, guid, event, payload, 1, ?3, ?3 FROM webhook_deliveries WHERE id = ?1 AND hook_id = ?2",
                params![did, id, t],
            )?;
            if n == 0 {
                return Err(AppError::not_found());
            }
            Ok(c.last_insert_rowid())
        })
        .await?;
    st.hook_wake.notify_one();
    Ok(Json(json!({ "id": new_id })))
}
