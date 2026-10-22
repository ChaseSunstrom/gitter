//! Two-factor setup, the session list, re-authentication for dangerous
//! actions, and the security log.

use axum::Json;
use axum::extract::{Path, Query, State};
use rusqlite::types::Value;
use serde::Deserialize;
use serde_json::json;

use super::admin::Admin;
use super::{ClientIp, J, like, ok};
use crate::AppState;
use crate::auth::{Authed, CurrentSession, Sudo, password_login};
use crate::db::now;
use crate::error::AppError;
use crate::security::{self, Event, record};

/// Everything the Security settings tab shows.
pub async fn overview(
    State(st): State<AppState>,
    Authed(u): Authed,
    CurrentSession(cur): CurrentSession,
) -> J<serde_json::Value> {
    let current = cur.map(|s| s.hash).unwrap_or_default();
    let out = st
        .db
        .call(move |c| {
            let (on, pending): (bool, bool) = c.query_row(
                "SELECT totp_secret IS NOT NULL, totp_pending IS NOT NULL FROM users WHERE id = ?1",
                [u.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            let left: i64 = c.query_row(
                "SELECT count(*) FROM recovery_codes WHERE user_id = ?1 AND used_at IS NULL",
                [u.id],
                |r| r.get(0),
            )?;
            let mut s = c.prepare(
                "SELECT token_hash, created_at, last_seen, ip, agent FROM sessions WHERE user_id = ?1 AND expires_at > ?2 ORDER BY last_seen DESC",
            )?;
            let sessions: Vec<serde_json::Value> = s
                .query_map(rusqlite::params![u.id, now()], |r| {
                    let h: String = r.get(0)?;
                    Ok(json!({
                        "id": &h[..16], "current": h == current, "created_at": r.get::<_, i64>(1)?,
                        "last_seen": r.get::<_, i64>(2)?, "ip": r.get::<_, String>(3)?, "agent": r.get::<_, String>(4)?,
                    }))
                })?
                .collect::<Result<_, _>>()?;
            Ok(json!({ "two_factor": on, "setup_pending": pending, "recovery_left": left, "sessions": sessions }))
        })
        .await?;
    Ok(Json(out))
}

/// Start turning on 2FA: a new secret to scan. Nothing changes until `enable`.
pub async fn setup(State(st): State<AppState>, Sudo(u): Sudo) -> J<serde_json::Value> {
    if crate::auth::has_2fa(&st.db, u.id).await? {
        return Err(AppError::conflict("Two-factor authentication is already on"));
    }
    let secret = security::new_totp_secret();
    let (s2, uid) = (security::seal_text(&st.key, &secret), u.id);
    st.db
        .call(move |c| Ok(c.execute("UPDATE users SET totp_pending = ?1 WHERE id = ?2", rusqlite::params![s2, uid])?))
        .await?;
    let issuer = st.settings.get().instance_name;
    let uri = security::otpauth_uri(&issuer, &u.username, &secret);
    Ok(Json(json!({ "secret": secret, "uri": uri, "qr": security::qr_svg(&uri) })))
}

#[derive(Deserialize)]
pub struct CodeReq {
    code: String,
}

/// Finish turning on 2FA with a code from the app; returns the recovery codes (shown once).
pub async fn enable(
    State(st): State<AppState>,
    Sudo(u): Sudo,
    ip: ClientIp,
    Json(req): Json<CodeReq>,
) -> J<serde_json::Value> {
    let codes = security::new_recovery_codes();
    let hashes: Vec<String> = codes.iter().map(|c| c.1.clone()).collect();
    let (uid, key) = (u.id, st.key.clone());
    st.db
        .call(move |c| {
            let tx = c.transaction()?;
            let pending: Option<String> =
                tx.query_row("SELECT totp_pending FROM users WHERE id = ?1", [uid], |r| r.get(0))?;
            let Some(secret) = pending.and_then(|p| security::open_text(&key, &p)) else {
                return Err(AppError::bad("Start the setup again"));
            };
            let Some(step) = security::check_totp(&secret, &req.code, 0) else {
                return Err(AppError::bad("That code didn't match. Check the time on your phone and try the next code."));
            };
            tx.execute(
                "UPDATE users SET totp_secret = totp_pending, totp_pending = NULL, totp_last_step = ?1 WHERE id = ?2",
                rusqlite::params![step, uid],
            )?;
            replace_codes(&tx, uid, &hashes)?;
            tx.commit()?;
            Ok(())
        })
        .await?;
    record(&st.db, Event { actor: Some(&u), subject: Some(u.id), action: "2fa_enabled", detail: String::new(), ip: &ip.0 }).await;
    Ok(Json(json!({ "recovery_codes": codes.into_iter().map(|c| c.0).collect::<Vec<_>>() })))
}

fn replace_codes(tx: &rusqlite::Transaction, uid: i64, hashes: &[String]) -> rusqlite::Result<()> {
    tx.execute("DELETE FROM recovery_codes WHERE user_id = ?1", [uid])?;
    for h in hashes {
        tx.execute("INSERT INTO recovery_codes(user_id, code_hash) VALUES (?1, ?2)", rusqlite::params![uid, h])?;
    }
    Ok(())
}

pub async fn disable(State(st): State<AppState>, Sudo(u): Sudo, ip: ClientIp) -> J<serde_json::Value> {
    let uid = u.id;
    st.db.call(move |c| Ok(clear_2fa(c, uid)?)).await?;
    record(&st.db, Event { actor: Some(&u), subject: Some(u.id), action: "2fa_disabled", detail: String::new(), ip: &ip.0 }).await;
    ok()
}

pub fn clear_2fa(c: &rusqlite::Connection, uid: i64) -> rusqlite::Result<()> {
    c.execute(
        "UPDATE users SET totp_secret = NULL, totp_pending = NULL, totp_last_step = 0 WHERE id = ?1",
        [uid],
    )?;
    c.execute("DELETE FROM recovery_codes WHERE user_id = ?1", [uid])?;
    Ok(())
}

pub async fn new_codes(State(st): State<AppState>, Sudo(u): Sudo, ip: ClientIp) -> J<serde_json::Value> {
    if !crate::auth::has_2fa(&st.db, u.id).await? {
        return Err(AppError::bad("Turn on two-factor authentication first"));
    }
    let codes = security::new_recovery_codes();
    let hashes: Vec<String> = codes.iter().map(|c| c.1.clone()).collect();
    let uid = u.id;
    st.db
        .call(move |c| {
            let tx = c.transaction()?;
            replace_codes(&tx, uid, &hashes)?;
            tx.commit()?;
            Ok(())
        })
        .await?;
    record(&st.db, Event { actor: Some(&u), subject: Some(u.id), action: "recovery_codes_replaced", detail: String::new(), ip: &ip.0 }).await;
    Ok(Json(json!({ "recovery_codes": codes.into_iter().map(|c| c.0).collect::<Vec<_>>() })))
}

/// Sign out one session (by the id the list shows).
pub async fn revoke(
    State(st): State<AppState>,
    Sudo(u): Sudo,
    ip: ClientIp,
    Path(id): Path<String>,
) -> J<serde_json::Value> {
    if id.len() != 16 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(AppError::not_found());
    }
    let uid = u.id;
    let gone: Option<String> = st
        .db
        .call(move |c| {
            let pattern = format!("{id}%");
            let agent: Option<String> = rusqlite::OptionalExtension::optional(c.query_row(
                "SELECT agent FROM sessions WHERE user_id = ?1 AND token_hash LIKE ?2",
                rusqlite::params![uid, pattern],
                |r| r.get(0),
            ))?;
            c.execute("DELETE FROM sessions WHERE user_id = ?1 AND token_hash LIKE ?2", rusqlite::params![uid, pattern])?;
            Ok(agent)
        })
        .await?;
    let Some(agent) = gone else { return Err(AppError::not_found()) };
    record(&st.db, Event { actor: Some(&u), subject: Some(u.id), action: "session_revoked", detail: short_agent(&agent), ip: &ip.0 }).await;
    ok()
}

pub async fn revoke_others(
    State(st): State<AppState>,
    Sudo(u): Sudo,
    CurrentSession(cur): CurrentSession,
    ip: ClientIp,
) -> J<serde_json::Value> {
    let (uid, keep) = (u.id, cur.map(|s| s.hash).unwrap_or_default());
    let n = st
        .db
        .call(move |c| Ok(c.execute("DELETE FROM sessions WHERE user_id = ?1 AND token_hash != ?2", rusqlite::params![uid, keep])?))
        .await?;
    record(&st.db, Event { actor: Some(&u), subject: Some(u.id), action: "session_revoked", detail: format!("all other sessions ({n})"), ip: &ip.0 }).await;
    Ok(Json(json!({ "revoked": n })))
}

/// "Firefox on Linux"-style summary of a user agent for the log.
pub fn short_agent(ua: &str) -> String {
    let browser = ["Firefox", "Edg", "Chrome", "Safari", "git", "curl"]
        .into_iter()
        .find(|b| ua.contains(b))
        .map(|b| if b == "Edg" { "Edge" } else { b })
        .unwrap_or("Unknown browser");
    let os = ["Windows", "Mac OS", "Android", "iPhone", "Linux"]
        .into_iter()
        .find(|o| ua.contains(o))
        .map(|o| if o == "Mac OS" { "macOS" } else { o });
    match os {
        Some(o) => format!("{browser} on {o}"),
        None => browser.to_string(),
    }
}

#[derive(Deserialize)]
pub struct ReauthReq {
    #[serde(default)]
    password: String,
    #[serde(default)]
    code: String,
    /// From confirming through a sign-in provider: stands in for the password.
    #[serde(default)]
    challenge: String,
}

/// Confirm the password (and 2FA code) to unlock dangerous actions for a few minutes.
pub async fn reauth(
    State(st): State<AppState>,
    Authed(u): Authed,
    CurrentSession(cur): CurrentSession,
    ip: ClientIp,
    Json(req): Json<ReauthReq>,
) -> J<serde_json::Value> {
    let Some(sess) = cur else {
        return Err(AppError::bad("Access tokens don't need this"));
    };
    // Counted before checking, so parallel guesses share the budget.
    let key = format!("reauth:{}", u.id);
    if !st.limiter.allow(&key, 10, 600) {
        return Err(AppError::new(axum::http::StatusCode::TOO_MANY_REQUESTS, "Too many attempts. Wait a few minutes."));
    }
    if !req.challenge.is_empty() {
        if st.challenges.attempt(&req.challenge) != Some(u.id) {
            return Err(AppError::new(axum::http::StatusCode::UNAUTHORIZED, "That confirmation expired. Start again."));
        }
    } else if password_login(&st.db, &u.username, &req.password).await?.is_none() {
        return Err(AppError::new(axum::http::StatusCode::UNAUTHORIZED, "That password isn't right"));
    }
    if crate::auth::has_2fa(&st.db, u.id).await? {
        if req.code.trim().is_empty() {
            return Err(AppError::new(axum::http::StatusCode::UNAUTHORIZED, "Enter the code from your authenticator app").with_code("need_code"));
        }
        if security::use_code(&st.db, st.key.clone(), u.id, &req.code).await?.is_none() {
            record(&st.db, Event { actor: Some(&u), subject: Some(u.id), action: "2fa_failed", detail: "wrong code when confirming".into(), ip: &ip.0 }).await;
            return Err(AppError::new(axum::http::StatusCode::UNAUTHORIZED, "That code didn't work").with_code("need_code"));
        }
    }
    st.limiter.reset(&key);
    st.challenges.finish(&req.challenge);
    let h = sess.hash;
    st.db
        .call(move |c| Ok(c.execute("UPDATE sessions SET verified_at = ?1 WHERE token_hash = ?2", rusqlite::params![now(), h])?))
        .await?;
    Ok(Json(json!({ "ok": true, "valid_for": crate::auth::SUDO_SECS })))
}

#[derive(Deserialize)]
pub struct LogQ {
    page: Option<usize>,
    q: Option<String>,
}

/// Your own security events: what you did, and what happened to your account.
pub async fn my_log(State(st): State<AppState>, Authed(u): Authed, Query(q): Query<LogQ>) -> J<serde_json::Value> {
    let page = super::page(q.page);
    let (events, more) = st
        .db
        .call(move |c| {
            let (mut rows, more) = security::list(c, "(a.subject_id = ?1 OR a.actor_id = ?1)", vec![Value::Integer(u.id)], page)?;
            // Someone else's address (an admin acting on your account) isn't yours to see.
            for r in rows.iter_mut().filter(|r| r.actor_id != Some(u.id)) {
                r.ip.clear();
            }
            Ok((rows, more))
        })
        .await?;
    Ok(Json(json!({ "events": events, "has_more": more, "page": page })))
}

/// The whole instance's security log, for admins, with an integrity check.
pub async fn admin_log(State(st): State<AppState>, _a: Admin, Query(q): Query<LogQ>) -> J<serde_json::Value> {
    let page = super::page(q.page);
    let out = st
        .db
        .call(move |c| {
            let (cond, params) = match q.q.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                Some(s) => (
                    "(a.actor LIKE ?1 ESCAPE '\\' OR a.action LIKE ?1 ESCAPE '\\' OR a.detail LIKE ?1 ESCAPE '\\' OR a.ip LIKE ?1 ESCAPE '\\')",
                    vec![Value::Text(like(s))],
                ),
                None => ("1=1", vec![]),
            };
            let (events, more) = security::list(c, cond, params, page)?;
            let intact = match security::verify(c)? {
                Ok(h) => json!({ "ok": true, "events": h.count, "head_id": h.last_id, "head": &h.hash.get(..12).unwrap_or("") }),
                Err(id) => json!({ "ok": false, "altered_at": id }),
            };
            Ok(json!({ "events": events, "has_more": more, "page": page, "integrity": intact }))
        })
        .await?;
    Ok(Json(out))
}
