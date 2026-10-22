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
    /// Set when the request came with a limited access token.
    #[serde(skip)]
    pub limits: Option<Limits>,
}

/// What an access token may do: `read` (clone, view), `write` (also push and
/// change issues, PRs and files), or `all` (everything the account can).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Read,
    Write,
    All,
}

impl Scope {
    pub fn parse(s: &str) -> Option<Scope> {
        Some(match s {
            "read" => Scope::Read,
            "write" => Scope::Write,
            "all" => Scope::All,
            _ => return None,
        })
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Scope::Read => "read",
            Scope::Write => "write",
            Scope::All => "all",
        }
    }
}

/// A token's limits: its scope and, for deploy tokens, the one repository it's for.
#[derive(Clone, Debug)]
pub struct Limits {
    pub scope: Scope,
    pub repo_id: Option<i64>,
    /// Set for agent tokens: they work only through /mcp (see `api::mcp`).
    pub agent: Option<std::sync::Arc<Agent>>,
}

/// An agent token: the repositories and MCP tools it was given.
#[derive(Debug)]
pub struct Agent {
    pub token_id: i64,
    pub name: String,
    pub repos: Vec<i64>,
    pub tools: Vec<String>,
}

impl Limits {
    pub fn unlimited(&self) -> bool {
        self.scope == Scope::All && self.repo_id.is_none() && self.agent.is_none()
    }
    /// Whether this token may act in repository `id` at all.
    pub fn covers(&self, id: i64) -> bool {
        self.repo_id.is_none_or(|r| r == id) && self.agent.as_ref().is_none_or(|a| a.repos.contains(&id))
    }
}

tokio::task_local! {
    /// The agent token's name while `api::mcp` runs one of its tool calls.
    pub static ACTING_AGENT: String;
}

/// The agent acting right now, if any (for records made without the `User` at hand).
pub fn acting_agent() -> Option<String> {
    ACTING_AGENT.try_with(|a| a.clone()).ok()
}

/// Run the next extractions of a request as `u`, as if it had signed in (for
/// requests the server makes on a user's behalf, like MCP tool calls).
pub fn act_as(ext: &mut axum::http::Extensions, u: User) {
    ext.insert(CachedViewer(Some(u)));
}

pub const AGENT_ONLY_MCP: &str = "Agent tokens work only through /mcp";

impl User {
    pub fn from_row(r: &rusqlite::Row) -> rusqlite::Result<User> {
        Ok(User {
            id: r.get("id")?,
            username: r.get("username")?,
            display_name: r.get("display_name")?,
            email: r.get("email")?,
            is_admin: r.get::<_, i64>("is_admin")? != 0,
            limits: None,
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
    /// The agent token this request came with, if any.
    pub fn agent(&self) -> Option<&Agent> {
        self.limits.as_ref()?.agent.as_deref()
    }
}

pub const USER_COLS: &str = "id, username, display_name, email, is_admin";

/// An account that isn't suspended, by id (signed in with no token limits).
pub async fn user_by_id(db: &Db, id: i64) -> R<Option<User>> {
    db.call(move |c| Ok(c.query_row(&format!("SELECT {USER_COLS} FROM users WHERE id = ?1 AND suspended = 0"), [id], User::from_row).optional()?)).await
}

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

/// Compare secrets without leaking where they differ through timing.
pub fn constant_eq(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
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

/// Where a sign-in came from, shown in the session list.
pub fn device(headers: &HeaderMap) -> String {
    let ua = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    ua.chars().take(200).collect()
}

pub async fn create_session(db: &Db, user_id: i64, ip: &str, agent: &str) -> R<String> {
    let (raw, hash) = new_secret("");
    let (ip, agent) = (ip.to_string(), agent.to_string());
    db.call(move |c| {
        let t = now();
        c.execute(
            "INSERT INTO sessions(token_hash, user_id, created_at, expires_at, ip, agent, last_seen, verified_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?3, ?3)",
            rusqlite::params![hash, user_id, t, t + SESSION_DAYS * 86400, ip, agent],
        )?;
        c.execute("DELETE FROM sessions WHERE expires_at < ?1", [t])?;
        Ok(())
    })
    .await?;
    Ok(raw)
}

/// The signed-in browser session behind a request (absent for token auth).
#[derive(Clone, Debug)]
pub struct Session {
    pub hash: String,
    pub verified_at: i64,
}

/// How long a password confirmation covers dangerous actions.
pub const SUDO_SECS: i64 = 600;

async fn user_by_session(db: &Db, raw: &str) -> R<Option<(User, Session)>> {
    let hash = sha256_hex(raw);
    db.call(move |c| {
        let t = now();
        let row = c
            .query_row(
                &format!(
                    "SELECT {}, s.verified_at, s.last_seen FROM users u JOIN sessions s ON s.user_id = u.id WHERE s.token_hash = ?1 AND s.expires_at > ?2 AND u.suspended = 0",
                    USER_COLS.split(", ").map(|c| format!("u.{c}")).collect::<Vec<_>>().join(", ")
                ),
                rusqlite::params![hash, t],
                |r| Ok((User::from_row(r)?, r.get::<_, i64>("verified_at")?, r.get::<_, i64>("last_seen")?)),
            )
            .optional()?;
        let Some((u, verified_at, last_seen)) = row else { return Ok(None) };
        // At most one write a minute per session, for "last active" in the session list.
        if t - last_seen > 60 {
            c.execute("UPDATE sessions SET last_seen = ?1 WHERE token_hash = ?2", rusqlite::params![t, hash])?;
        }
        Ok(Some((u, Session { hash, verified_at })))
    })
    .await
}

pub async fn has_2fa(db: &Db, user_id: i64) -> R<bool> {
    db.call(move |c| {
        Ok(c.query_row("SELECT totp_secret IS NOT NULL FROM users WHERE id = ?1", [user_id], |r| r.get(0))?)
    })
    .await
}

pub async fn user_by_token(db: &Db, raw: &str) -> R<Option<User>> {
    if raw.starts_with(crate::ci::queue::JOB_TOKEN_PREFIX) {
        return job_user(db, raw).await;
    }
    if !raw.starts_with(TOKEN_PREFIX) {
        return Ok(None);
    }
    let hash = sha256_hex(raw);
    db.call(move |c| {
        let row = c
            .query_row(
                "SELECT u.id, u.username, u.display_name, u.email, u.is_admin, t.scope, t.repo_id, t.id AS tid, t.kind, t.name AS tname, t.tools FROM users u
                 JOIN tokens t ON t.user_id = u.id
                 WHERE t.token_hash = ?1 AND u.suspended = 0 AND (t.expires_at IS NULL OR t.expires_at > ?2)",
                rusqlite::params![hash, now()],
                |r| {
                    let mut u = User::from_row(r)?;
                    let scope = Scope::parse(&r.get::<_, String>("scope")?).unwrap_or(Scope::Read);
                    u.limits = Some(Limits { scope, repo_id: r.get("repo_id")?, agent: None });
                    let agent = (r.get::<_, String>("kind")? == "agent").then(|| -> rusqlite::Result<_> {
                        Ok((r.get::<_, i64>("tid")?, r.get::<_, String>("tname")?, r.get::<_, String>("tools")?))
                    });
                    Ok((u, agent.transpose()?))
                },
            )
            .optional()?;
        let u = match row {
            Some((mut u, Some((token_id, name, tools)))) => {
                let mut q = c.prepare("SELECT repo_id FROM token_repos WHERE token_id = ?1")?;
                let repos = q.query_map([token_id], |r| r.get(0))?.collect::<Result<_, _>>()?;
                let tools = tools.split(',').filter(|s| !s.is_empty()).map(String::from).collect();
                // Agents never administer, whatever the row says, and a site admin's agent is no admin.
                u.is_admin = false;
                u.limits = Some(Limits { scope: Scope::Write, repo_id: None, agent: Some(std::sync::Arc::new(Agent { token_id, name, repos, tools })) });
                Some(u)
            }
            Some((mut u, None)) => {
                u.limits = u.limits.filter(|l| !l.unlimited());
                Some(u)
            }
            None => None,
        };
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

/// A running CI job's token reads its own repository, like a read-only deploy
/// token, until the attempt ends.
async fn job_user(db: &Db, raw: &str) -> R<Option<User>> {
    let hash = sha256_hex(raw);
    db.call(move |c| {
        Ok(c.query_row(
            "SELECT u.id, u.username, u.display_name, u.email, u.is_admin, r.repo_id FROM ci_jobs j
             JOIN ci_runs r ON r.id = j.run_id JOIN repos p ON p.id = r.repo_id JOIN users u ON u.id = p.owner_id
             WHERE j.token_hash = ?1 AND j.status = 'running' AND u.suspended = 0 AND p.deleted_at IS NULL",
            [hash],
            |r| {
                let mut u = User::from_row(r)?;
                u.limits = Some(Limits { scope: Scope::Read, repo_id: Some(r.get("repo_id")?), agent: None });
                Ok(u)
            },
        )
        .optional()?)
    })
    .await
}

/// The password (or token) of a Basic Authorization header.
pub fn basic_secret(headers: &HeaderMap) -> Option<String> {
    let v = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let b64 = v.strip_prefix("Basic ").or_else(|| v.strip_prefix("basic "))?;
    let dec = String::from_utf8(base64::engine::general_purpose::STANDARD.decode(b64.trim()).ok()?).ok()?;
    dec.split_once(':').map(|(_, s)| s.to_string())
}

/// Username + password, or username + access token (used by git over HTTP, Git
/// LFS and the registry). Failed passwords are limited per name and per address
/// (`ip`), so rotating names can't turn each request into a password hash.
pub async fn user_by_basic(st: &AppState, headers: &HeaderMap, ip: &str) -> R<Option<User>> {
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
        if u.agent().is_some() {
            return Err(AppError::new(axum::http::StatusCode::FORBIDDEN, AGENT_ONLY_MCP));
        }
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
    let by_ip = format!("basic-ip:{}", ip_bucket(ip));
    if st.limiter.blocked(&key, 10) || st.limiter.blocked(&by_ip, 30) {
        return Err(AppError::new(
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            "Too many failed attempts. Wait a minute.",
        ));
    }
    let u = password_login(&st.db, name, secret).await?;
    if u.is_none() {
        st.limiter.fail(&key, 60);
        st.limiter.fail(&by_ip, 60);
    }
    if let Some(u) = &u
        && has_2fa(&st.db, u.id).await?
    {
        // Like GitHub: with two-factor on, git needs an access token, not the password.
        return Err(AppError::new(
            axum::http::StatusCode::UNAUTHORIZED,
            "This account uses two-factor authentication: use an access token instead of your password",
        ));
    }
    Ok(u)
}

pub async fn password_login(db: &Db, name: &str, pw: &str) -> R<Option<User>> {
    let n = name.to_string();
    let row = db
        .call(move |c| {
            Ok(c.query_row(
                "SELECT id, username, display_name, email, is_admin, pass_hash FROM users WHERE (username = ?1 OR (email = ?1 AND email != '')) AND suspended = 0 AND is_org = 0",
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
        let mut session = None;
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
            // MCP tool calls come in through `act_as`, never with the token itself.
            if user.as_ref().is_some_and(|u| u.agent().is_some()) {
                return Err(AppError::new(axum::http::StatusCode::FORBIDDEN, AGENT_ONLY_MCP));
            }
        }
        if user.is_none()
            && let Some(raw) = cookie_value(&parts.headers, SESSION_COOKIE)
            && let Some((u, s)) = user_by_session(&st.db, raw).await?
        {
            user = Some(u);
            session = Some(s);
        }
        parts.extensions.insert(CachedViewer(user.clone()));
        if let Some(s) = session {
            parts.extensions.insert(s);
        }
        Ok(Viewer(user))
    }
}

#[derive(Clone)]
struct CachedViewer(Option<User>);

/// Whether this request may do something dangerous right now: only a browser
/// session whose password (and 2FA) was confirmed in the last `SUDO_SECS`.
/// Access tokens never may, so a leaked token can't turn off 2FA or delete repos.
pub struct Fresh(pub bool);

impl FromRequestParts<AppState> for Fresh {
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, st: &AppState) -> R<Self> {
        let Viewer(v) = Viewer::from_request_parts(parts, st).await?;
        if v.is_none() {
            return Ok(Fresh(false));
        }
        Ok(Fresh(parts.extensions.get::<Session>().is_some_and(|s| now() - s.verified_at < SUDO_SECS)))
    }
}

/// A signed-in user who confirmed their password recently (see `Fresh`).
pub struct Sudo(pub User);

impl FromRequestParts<AppState> for Sudo {
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, st: &AppState) -> R<Self> {
        let Authed(u) = Authed::from_request_parts(parts, st).await?;
        if parts.extensions.get::<Session>().is_none() {
            return Err(AppError::new(
                axum::http::StatusCode::FORBIDDEN,
                "Access tokens can't do this. Sign in on the web instead.",
            ));
        }
        if !Fresh::from_request_parts(parts, st).await?.0 {
            return Err(AppError::reauth());
        }
        Ok(Sudo(u))
    }
}

/// The current browser session, if the request has one.
pub struct CurrentSession(pub Option<Session>);

impl FromRequestParts<AppState> for CurrentSession {
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, st: &AppState) -> R<Self> {
        Viewer::from_request_parts(parts, st).await?;
        Ok(CurrentSession(parts.extensions.get::<Session>().cloned()))
    }
}

impl FromRequestParts<AppState> for Authed {
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, st: &AppState) -> R<Self> {
        match Viewer::from_request_parts(parts, st).await?.0 {
            Some(u) => Ok(Authed(u)),
            None => Err(AppError::unauthorized()),
        }
    }
}

/// The part of an address that identifies one client for rate limits: the
/// whole IPv4 address, or the /64 of an IPv6 one (a home or VPS usually gets a
/// whole /64, so rotating within it mustn't reset the limits).
pub fn ip_bucket(ip: &str) -> String {
    match ip.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V6(v6)) if v6.to_ipv4_mapped().is_none() => {
            let s = v6.segments();
            format!("{:x}:{:x}:{:x}:{:x}::/64", s[0], s[1], s[2], s[3])
        }
        Ok(std::net::IpAddr::V6(v6)) => v6.to_ipv4_mapped().map(|v| v.to_string()).unwrap_or_default(),
        _ => ip.to_string(),
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

    /// Count one event against `max` per `window_secs`; Err(seconds to wait) when over.
    pub fn check(&self, key: &str, max: u32, window_secs: i64) -> Result<(), i64> {
        if self.allow(key, max, window_secs) {
            return Ok(());
        }
        let t = now();
        let m = self.0.lock().unwrap_or_else(|p| p.into_inner());
        Err(m.get(key).map(|w| w.start + w.len - t).unwrap_or(window_secs).max(1))
    }

    /// Forget `key` (after a success, when attempts are counted up front with `allow`).
    pub fn reset(&self, key: &str) {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).remove(key);
    }

    fn bump(&self, key: &str, window_secs: i64) -> u32 {
        let t = now();
        let mut m = self.0.lock().unwrap_or_else(|p| p.into_inner());
        if m.len() >= LIMITER_CAP {
            m.retain(|_, w| t - w.start < w.len);
            if m.len() >= LIMITER_CAP {
                // Still full of live windows: drop the oldest half, never everything
                // (clearing would let a flood of new keys reset login lockouts).
                let mut starts: Vec<i64> = m.values().map(|w| w.start).collect();
                starts.sort_unstable();
                let cut = starts[starts.len() / 2];
                m.retain(|_, w| w.start > cut);
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
    fn ip_buckets() {
        assert_eq!(ip_bucket("203.0.113.9"), "203.0.113.9");
        assert_eq!(ip_bucket("2001:db8:1:2:aaaa::1"), ip_bucket("2001:db8:1:2:ffff::9"));
        assert_ne!(ip_bucket("2001:db8:1:2::1"), ip_bucket("2001:db8:1:3::1"));
        assert_eq!(ip_bucket("::ffff:198.51.100.7"), "198.51.100.7");
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
