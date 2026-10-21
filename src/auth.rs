use std::collections::HashMap;
use std::sync::Mutex;

use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use axum::extract::FromRequestParts;
use axum::http::{HeaderMap, header, request::Parts};
use base64::Engine;
use rand::RngCore;
use rusqlite::OptionalExtension;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::AppState;
use crate::db::{Db, now};
use crate::error::{AppError, R};

pub const SESSION_COOKIE: &str = "gitter_session";
pub const SESSION_DAYS: i64 = 30;
pub const TOKEN_PREFIX: &str = "gtr_";

#[derive(Clone, Debug, Serialize)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub display_name: String,
    pub email: String,
    pub is_admin: bool,
}

impl User {
    pub fn from_row(r: &rusqlite::Row) -> rusqlite::Result<User> {
        Ok(User {
            id: r.get("id")?,
            username: r.get("username")?,
            display_name: r.get("display_name")?,
            email: r.get("email")?,
            is_admin: r.get::<_, i64>("is_admin")? != 0,
        })
    }
    /// Name and email used for commits made through the web.
    pub fn signature(&self) -> (String, String) {
        let name = if self.display_name.trim().is_empty() {
            self.username.clone()
        } else {
            self.display_name.clone()
        };
        // The account email stays private: web commits use a stable no-reply address.
        (
            name,
            format!("{}+{}@users.noreply.gitter", self.id, self.username),
        )
    }
}

pub const USER_COLS: &str = "id, username, display_name, email, is_admin";

pub async fn hash_password(pw: String) -> R<String> {
    tokio::task::spawn_blocking(move || {
        let salt = SaltString::generate(&mut rand::rngs::OsRng);
        Argon2::default()
            .hash_password(pw.as_bytes(), &salt)
            .map(|h| h.to_string())
            .map_err(AppError::internal)
    })
    .await
    .map_err(AppError::internal)?
}

pub async fn verify_password(pw: String, hash: String) -> bool {
    tokio::task::spawn_blocking(move || {
        PasswordHash::new(&hash)
            .map(|h| Argon2::default().verify_password(pw.as_bytes(), &h).is_ok())
            .unwrap_or(false)
    })
    .await
    .unwrap_or(false)
}

pub fn sha256_hex(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}

/// A fresh random secret and the hash we store for it.
pub fn new_secret(prefix: &str) -> (String, String) {
    let mut b = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut b);
    let raw = format!(
        "{prefix}{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
    );
    let hash = sha256_hex(&raw);
    (raw, hash)
}

pub fn session_cookie(token: &str, secure: bool) -> String {
    format!(
        "{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}{}",
        SESSION_DAYS * 86400,
        if secure { "; Secure" } else { "" }
    )
}

pub fn clear_cookie(secure: bool) -> String {
    format!(
        "{SESSION_COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0{}",
        if secure { "; Secure" } else { "" }
    )
}

pub fn cookie_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v)
}

pub async fn create_session(db: &Db, user_id: i64) -> R<String> {
    let (raw, hash) = new_secret("");
    db.call(move |c| {
        let t = now();
        c.execute(
            "INSERT INTO sessions(token_hash, user_id, created_at, expires_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![hash, user_id, t, t + SESSION_DAYS * 86400],
        )?;
        c.execute("DELETE FROM sessions WHERE expires_at < ?1", [t])?;
        Ok(())
    })
    .await?;
    Ok(raw)
}

async fn user_by_session(db: &Db, raw: &str) -> R<Option<User>> {
    let hash = sha256_hex(raw);
    db.call(move |c| {
        Ok(c.query_row(
            &format!(
                "SELECT {} FROM users u JOIN sessions s ON s.user_id = u.id WHERE s.token_hash = ?1 AND s.expires_at > ?2",
                USER_COLS.split(", ").map(|c| format!("u.{c}")).collect::<Vec<_>>().join(", ")
            ),
            rusqlite::params![hash, now()],
            User::from_row,
        )
        .optional()?)
    })
    .await
}

pub async fn user_by_token(db: &Db, raw: &str) -> R<Option<User>> {
    if !raw.starts_with(TOKEN_PREFIX) {
        return Ok(None);
    }
    let hash = sha256_hex(raw);
    db.call(move |c| {
        let u = c
            .query_row(
                "SELECT u.id, u.username, u.display_name, u.email, u.is_admin FROM users u
                 JOIN tokens t ON t.user_id = u.id WHERE t.token_hash = ?1",
                [&hash],
                User::from_row,
            )
            .optional()?;
        if u.is_some() {
            c.execute(
                "UPDATE tokens SET last_used = ?1 WHERE token_hash = ?2",
                rusqlite::params![now(), hash],
            )?;
        }
        Ok(u)
    })
    .await
}

/// Username + password, or username + access token (used by git over HTTP).
pub async fn user_by_basic(st: &AppState, headers: &HeaderMap) -> R<Option<User>> {
    let Some(v) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    else {
        return Ok(None);
    };
    let Some(b64) = v
        .strip_prefix("Basic ")
        .or_else(|| v.strip_prefix("basic "))
    else {
        return Ok(None);
    };
    let Ok(dec) = base64::engine::general_purpose::STANDARD.decode(b64.trim()) else {
        return Ok(None);
    };
    let Ok(dec) = String::from_utf8(dec) else {
        return Ok(None);
    };
    let Some((name, secret)) = dec.split_once(':') else {
        return Ok(None);
    };
    if let Some(u) = user_by_token(&st.db, secret).await? {
        // Token works with any username (like GitHub), but if a name was given it must match.
        if name.is_empty()
            || name.eq_ignore_ascii_case(&u.username)
            || name == "x-access-token"
            || name == "token"
        {
            return Ok(Some(u));
        }
        return Ok(None);
    }
    let key = format!("basic:{}", name.to_lowercase());
    if st.limiter.blocked(&key, 10) {
        return Err(AppError::new(
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            "Too many failed attempts. Wait a minute.",
        ));
    }
    let u = password_login(&st.db, name, secret).await?;
    if u.is_none() {
        st.limiter.fail(&key, 60);
    }
    Ok(u)
}

pub async fn password_login(db: &Db, name: &str, pw: &str) -> R<Option<User>> {
    let n = name.to_string();
    let row = db
        .call(move |c| {
            Ok(c.query_row(
                "SELECT id, username, display_name, email, is_admin, pass_hash FROM users WHERE username = ?1 OR (email = ?1 AND email != '')",
                [&n],
                |r| Ok((User::from_row(r)?, r.get::<_, String>("pass_hash")?)),
            )
            .optional()?)
        })
        .await?;
    match row {
        Some((u, hash)) => Ok(verify_password(pw.to_string(), hash).await.then_some(u)),
        None => {
            // Spend about the same time as a real check so names can't be probed by timing.
            let _ = verify_password(pw.to_string(), DUMMY_HASH.to_string()).await;
            Ok(None)
        }
    }
}

const DUMMY_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHRzb21lc2FsdA$Zb2gO1xLtJk3dY5Yx3o0iYQ6O1gS1b8r2Vh0m3Yw9iE";

/// The signed-in user, if any (session cookie or `Authorization: Bearer <token>`).
pub struct Viewer(pub Option<User>);

/// Like `Viewer` but rejects anonymous requests.
pub struct Authed(pub User);

impl FromRequestParts<AppState> for Viewer {
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, st: &AppState) -> R<Self> {
        if let Some(u) = parts.extensions.get::<CachedViewer>() {
            return Ok(Viewer(u.0.clone()));
        }
        let mut user = None;
        if let Some(v) = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            && let Some(t) = v
                .strip_prefix("Bearer ")
                .or_else(|| v.strip_prefix("token "))
        {
            user = user_by_token(&st.db, t.trim()).await?;
            if user.is_none() {
                return Err(AppError::unauthorized());
            }
        }
        if user.is_none()
            && let Some(raw) = cookie_value(&parts.headers, SESSION_COOKIE)
        {
            user = user_by_session(&st.db, raw).await?;
        }
        parts.extensions.insert(CachedViewer(user.clone()));
        Ok(Viewer(user))
    }
}

#[derive(Clone)]
struct CachedViewer(Option<User>);

impl FromRequestParts<AppState> for Authed {
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, st: &AppState) -> R<Self> {
        match Viewer::from_request_parts(parts, st).await?.0 {
            Some(u) => Ok(Authed(u)),
            None => Err(AppError::unauthorized()),
        }
    }
}

/// Fixed-window rate limiter keyed by string (IP, username, ...). Each entry
/// remembers its own window, and the map is bounded.
#[derive(Default)]
pub struct Limiter(Mutex<HashMap<String, Window>>);

struct Window {
    count: u32,
    start: i64,
    len: i64,
}

const LIMITER_CAP: usize = 100_000;

impl Limiter {
    /// Count one event and say whether it is within `max` per `window_secs`.
    pub fn allow(&self, key: &str, max: u32, window_secs: i64) -> bool {
        self.bump(key, window_secs) <= max
    }

    /// Whether `key` is already over `max`, without counting this call.
    pub fn blocked(&self, key: &str, max: u32) -> bool {
        let t = now();
        let m = self.0.lock().unwrap_or_else(|p| p.into_inner());
        m.get(key)
            .is_some_and(|w| t - w.start < w.len && w.count >= max)
    }

    /// Count one failure (used with `blocked`, so successes are free).
    pub fn fail(&self, key: &str, window_secs: i64) {
        self.bump(key, window_secs);
    }

    fn bump(&self, key: &str, window_secs: i64) -> u32 {
        let t = now();
        let mut m = self.0.lock().unwrap_or_else(|p| p.into_inner());
        if m.len() >= LIMITER_CAP {
            m.retain(|_, w| t - w.start < w.len);
            if m.len() >= LIMITER_CAP {
                m.clear();
            }
        }
        let w = m.entry(key.to_string()).or_insert(Window {
            count: 0,
            start: t,
            len: window_secs,
        });
        if t - w.start >= w.len {
            *w = Window {
                count: 0,
                start: t,
                len: window_secs,
            };
        }
        w.count += 1;
        w.count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn password_roundtrip() {
        let h = hash_password("hunter22".into()).await.unwrap();
        assert!(verify_password("hunter22".into(), h.clone()).await);
        assert!(!verify_password("nope".into(), h).await);
        assert!(!verify_password("x".into(), DUMMY_HASH.into()).await);
    }

    #[test]
    fn secrets_and_cookies() {
        let (raw, hash) = new_secret(TOKEN_PREFIX);
        assert!(raw.starts_with(TOKEN_PREFIX));
        assert_eq!(hash, sha256_hex(&raw));
        let mut h = HeaderMap::new();
        h.insert(
            header::COOKIE,
            "a=1; gitter_session=xyz; b=2".parse().unwrap(),
        );
        assert_eq!(cookie_value(&h, SESSION_COOKIE), Some("xyz"));
    }

    #[test]
    fn limiter() {
        let l = Limiter::default();
        assert!(l.allow("k", 2, 60));
        assert!(l.allow("k", 2, 60));
        assert!(!l.allow("k", 2, 60));
        assert!(l.allow("other", 2, 60));
        assert!(!l.blocked("f", 2));
        l.fail("f", 60);
        l.fail("f", 60);
        assert!(l.blocked("f", 2));
    }
}
