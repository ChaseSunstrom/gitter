//! Signing in with GitHub, GitLab (either may be self-hosted) or any OpenID
//! Connect provider. Admins add providers; the client secret is sealed and
//! never returned. The authorization-code flow uses PKCE (S256) and a state
//! that works once, expires in 10 minutes and is bound by an HttpOnly cookie
//! to the browser that started it. People are matched by the provider's
//! stable id, never by email: an email already used here refuses the sign-up
//! instead of linking, and linking only happens from a signed-in session.
//! Two-factor auth is still asked for. OpenID Connect ID tokens come straight
//! from the token endpoint over the back channel (OIDC Core 3.1.3.7), so their
//! claims are checked (iss, aud, azp, exp, nonce) without a signature check,
//! and userinfo must name the same `sub`.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rusqlite::OptionalExtension;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::admin::Admin;
use super::{ClientIp, J};
use crate::auth::{
    Authed, CurrentSession, Fresh, Session, Sudo, User, Viewer, constant_eq, cookie_value, create_session, device, has_2fa,
    new_secret, session_cookie, sha256_hex, user_by_id,
};
use crate::db::now;
use crate::error::{AppError, R};
use crate::net::{self, enc};
use crate::security::{Event, open_text, record, seal_text};
use crate::{AppState, validate};

pub const CALLBACK: &str = "/api/auth/oauth/callback";
const BINDING: &str = "gitter_oauth";
const STATE_SECS: i64 = 600;
/// The most of a provider's JSON reply that is read.
const MAX_JSON: u64 = 1 << 20;
/// What an account made through a provider has instead of a password hash.
pub const NO_PASSWORD: &str = "!";

struct Provider {
    id: i64,
    kind: String,
    name: String,
    base_url: String,
    client_id: String,
    secret: String,
    allow_private: bool,
    allow_signup: bool,
}

async fn provider(st: &AppState, slug: Option<String>, id: i64) -> R<Option<Provider>> {
    st.db
        .call(move |c| {
            Ok(c.query_row(
                "SELECT id, kind, name, base_url, client_id, secret, allow_private, allow_signup FROM oauth_providers
                 WHERE (slug = ?1 OR id = ?2) AND enabled = 1",
                rusqlite::params![slug, id],
                |r| {
                    Ok(Provider {
                        id: r.get(0)?,
                        kind: r.get(1)?,
                        name: r.get(2)?,
                        base_url: r.get(3)?,
                        client_id: r.get(4)?,
                        secret: r.get(5)?,
                        allow_private: r.get(6)?,
                        allow_signup: r.get(7)?,
                    })
                },
            )
            .optional()?)
        })
        .await
}

/// An http(s) URL, and https unless the provider may be on a private network.
fn check(url: &str, allow_private: bool) -> Result<(), String> {
    let u = net::check_url(url)?;
    if u.scheme_str() != Some("https") && !allow_private {
        return Err("Use an https:// URL (plain http only with private addresses allowed)".into());
    }
    Ok(())
}

fn form(pairs: &[(&str, &str)]) -> String {
    pairs.iter().map(|(k, v)| format!("{k}={}", enc(v))).collect::<Vec<_>>().join("&")
}

/// GET (or POST a form to) a provider and read its JSON answer.
async fn call(url: String, token: Option<String>, body: Option<String>, allow_private: bool) -> Result<Value, String> {
    check(&url, allow_private)?;
    let r = tokio::task::spawn_blocking(move || {
        let mut h = vec![("Accept", "application/json".to_string())];
        if let Some(t) = &token {
            h.push(("Authorization", format!("Bearer {t}")));
        }
        if body.is_some() {
            h.push(("Content-Type", "application/x-www-form-urlencoded".into()));
        }
        net::fetch(&url, &h, body.as_deref().map(str::as_bytes), allow_private, MAX_JSON)
    })
    .await
    .map_err(|e| e.to_string())??;
    let v: Value = serde_json::from_slice(&r.body).map_err(|_| format!("the provider answered {} with something that isn't JSON", r.status))?;
    if !(200..300).contains(&r.status) {
        let why = v["error"].as_str().map(|e| format!(" ({})", short(e))).unwrap_or_default();
        return Err(format!("the provider answered {}{why}", r.status));
    }
    Ok(v)
}

/// A provider's word, kept to a short plain token for messages.
fn short(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric() || "_-. ".contains(*c)).take(80).collect()
}

#[derive(Clone)]
struct Ends {
    authorize: String,
    token: String,
    /// The signed-in user's endpoint (GitHub's /user, GitLab's /api/v4/user, OIDC userinfo).
    user: String,
    issuer: String,
}

async fn ends(p: &Provider) -> Result<Ends, String> {
    let b = &p.base_url;
    let e = match p.kind.as_str() {
        "github" => {
            let api = if b == "https://github.com" { "https://api.github.com".to_string() } else { format!("{b}/api/v3") };
            Ends { authorize: format!("{b}/login/oauth/authorize"), token: format!("{b}/login/oauth/access_token"), user: format!("{api}/user"), issuer: String::new() }
        }
        "gitlab" => Ends { authorize: format!("{b}/oauth/authorize"), token: format!("{b}/oauth/token"), user: format!("{b}/api/v4/user"), issuer: String::new() },
        _ => {
            let d = call(format!("{b}/.well-known/openid-configuration"), None, None, p.allow_private).await?;
            let s = |k: &str| d[k].as_str().map(String::from).ok_or_else(|| format!("its discovery document has no {k}"));
            let e = Ends { authorize: s("authorization_endpoint")?, token: s("token_endpoint")?, user: s("userinfo_endpoint")?, issuer: s("issuer")? };
            if e.issuer.trim_end_matches('/') != b.trim_end_matches('/') {
                return Err("its discovery document names a different issuer".into());
            }
            e
        }
    };
    // Discovered endpoints pass the same checks as the address the admin typed.
    for u in [&e.authorize, &e.token, &e.user] {
        check(u, p.allow_private)?;
    }
    Ok(e)
}

enum Mode {
    Login { invite: String },
    Link(i64),
    /// Confirm the signed-in session (user, session hash) for dangerous actions.
    Reauth(i64, String),
}

struct Pending {
    provider: i64,
    binding: String,
    verifier: String,
    nonce: String,
    mode: Mode,
    next: String,
    ends: Ends,
    started: i64,
    expires: i64,
}

// ponytail: in memory, so a restart drops sign-ins in flight (they just start again).
static PENDING: LazyLock<Mutex<HashMap<String, Pending>>> = LazyLock::new(Default::default);

fn put(state: String, p: Pending) {
    let mut m = PENDING.lock().unwrap_or_else(|p| p.into_inner());
    let t = now();
    m.retain(|_, p| p.expires > t);
    if m.len() > 10_000 {
        // A flood: drop the older half, never everything (that would cancel every sign-in in flight).
        let mut ages: Vec<i64> = m.values().map(|p| p.started).collect();
        ages.sort_unstable();
        let cut = ages[ages.len() / 2];
        m.retain(|_, p| p.started > cut);
    }
    m.insert(state, p);
}

/// The pending sign-in for `state`, used up by asking.
fn take(state: &str) -> Option<Pending> {
    PENDING.lock().unwrap_or_else(|p| p.into_inner()).remove(state).filter(|p| p.expires > now())
}

/// A path on this site: one leading '/', no host, backslash or control character.
fn local(next: &str) -> String {
    let ok = next.starts_with('/')
        && !next.starts_with("//")
        && !next.starts_with("/api/")
        && !next.contains('\\')
        && !next.chars().any(char::is_control)
        && next.len() <= 500;
    if ok { next.into() } else { "/".into() }
}

fn with(path: &str, k: &str, v: &str) -> String {
    format!("{path}{}{k}={}", if path.contains('?') { '&' } else { '?' }, enc(v))
}

fn redirect_uri(st: &AppState) -> String {
    format!("{}{CALLBACK}", st.cfg.url.trim_end_matches('/'))
}

/// Each sign-in has its own cookie, so two tabs signing in at once don't undo each other.
fn binding_name(state: &str) -> String {
    format!("{BINDING}_{}", state.get(..12).unwrap_or_default())
}

fn binding_cookie(st: &AppState, state: &str, value: &str, age: i64) -> String {
    let secure = if st.cfg.secure_cookies { "; Secure" } else { "" };
    format!("{}={value}; Path=/api/auth/oauth; HttpOnly; SameSite=Lax; Max-Age={age}{secure}", binding_name(state))
}

#[derive(Deserialize)]
pub struct StartReq {
    #[serde(default)]
    mode: String,
    #[serde(default)]
    next: String,
    #[serde(default)]
    invite: String,
}

/// Where to send the browser to sign in with `slug`.
pub async fn start(
    State(st): State<AppState>,
    Path(slug): Path<String>,
    Viewer(v): Viewer,
    CurrentSession(cur): CurrentSession,
    Fresh(fresh): Fresh,
    ip: ClientIp,
    Json(req): Json<StartReq>,
) -> R<Response> {
    if !st.limiter.allow(&format!("oauth:{}", crate::auth::ip_bucket(&ip.0)), 30, 600) {
        return Err(AppError::new(StatusCode::TOO_MANY_REQUESTS, "Too many sign-in attempts. Wait a few minutes."));
    }
    let mode = match (req.mode.as_str(), v, cur) {
        ("" | "login", _, _) => Mode::Login { invite: req.invite.trim().chars().take(200).collect() },
        ("link", Some(u), Some(_)) if fresh => Mode::Link(u.id),
        ("link", Some(_), Some(_)) => return Err(AppError::reauth()),
        // An account with a password confirms with it: a provider's remembered session is weaker.
        ("reauth", Some(u), Some(_)) if has_password(&st, u.id).await? => return Err(AppError::bad("Confirm with your password")),
        ("reauth", Some(u), Some(s)) => Mode::Reauth(u.id, s.hash),
        ("link" | "reauth", _, _) => return Err(AppError::unauthorized()),
        _ => return Err(AppError::bad("mode is login, link or reauth")),
    };
    let p = provider(&st, Some(slug), 0).await?.ok_or_else(AppError::not_found)?;
    let ends = ends(&p).await.map_err(|e| AppError::new(StatusCode::BAD_GATEWAY, format!("Couldn't reach {}: {e}", p.name)))?;
    let [state, verifier, nonce, binding] = [(); 4].map(|_| new_secret("").0);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let scope = match p.kind.as_str() {
        "github" => "read:user user:email",
        "gitlab" => "read_user",
        _ => "openid profile email",
    };
    let redirect = redirect_uri(&st);
    let mut q = vec![
        ("response_type", "code"),
        ("client_id", p.client_id.as_str()),
        ("redirect_uri", &redirect),
        ("scope", scope),
        ("state", &state),
        ("code_challenge", &challenge),
        ("code_challenge_method", "S256"),
    ];
    if p.kind == "oidc" {
        q.push(("nonce", &nonce));
        if matches!(mode, Mode::Reauth(..)) {
            // Sign in again now, not a remembered session; checked through auth_time.
            q.extend([("prompt", "login"), ("max_age", "0")]);
        }
    }
    let url = format!("{}{}{}", ends.authorize, if ends.authorize.contains('?') { '&' } else { '?' }, form(&q));
    let cookie = binding_cookie(&st, &state, &binding, STATE_SECS);
    let t = now();
    put(state, Pending { provider: p.id, binding: sha256_hex(&binding), verifier, nonce, mode, next: local(&req.next), ends, started: t, expires: t + STATE_SECS });
    Ok(([(header::SET_COOKIE, cookie)], Json(json!({ "url": url }))).into_response())
}

#[derive(Deserialize)]
pub struct CallbackQ {
    #[serde(default)]
    code: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    error: String,
}

/// Where the provider sends the browser back. Always a redirect into the app:
/// errors go along as `oauth_error`, a 2FA account as a `challenge`.
pub async fn callback(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    CurrentSession(cur): CurrentSession,
    ip: ClientIp,
    headers: HeaderMap,
    Query(q): Query<CallbackQ>,
) -> Response {
    let mut cookies = vec![binding_cookie(&st, &q.state, "", 0)];
    let (to, cookie) = match take(&q.state) {
        None => (with("/login", "oauth_error", "That sign-in expired or was already used. Try again."), None),
        Some(p) => {
            let fail = match p.mode {
                Mode::Login { .. } => with("/login", "next", &p.next),
                _ => p.next.clone(),
            };
            let bound = cookie_value(&headers, &binding_name(&q.state)).is_some_and(|b| constant_eq(&sha256_hex(b), &p.binding));
            match finish(&st, p, bound, v, cur, &ip.0, &headers, &q).await {
                Ok(done) => done,
                Err(msg) => (with(&fail, "oauth_error", &msg), None),
            }
        }
    };
    cookies.extend(cookie);
    let mut r = (StatusCode::SEE_OTHER, [(header::LOCATION, to), (header::CACHE_CONTROL, "no-store".into())]).into_response();
    for c in cookies {
        if let Ok(v) = c.parse() {
            r.headers_mut().append(header::SET_COOKIE, v);
        }
    }
    r
}

fn oops(e: AppError) -> String {
    eprintln!("sign-in: {e:?}");
    "Something went wrong signing you in. Try again.".into()
}

type Landing = (String, Option<String>);

#[allow(clippy::too_many_arguments)]
async fn finish(st: &AppState, p: Pending, bound: bool, v: Option<User>, cur: Option<Session>, ip: &str, headers: &HeaderMap, q: &CallbackQ) -> Result<Landing, String> {
    if !bound {
        return Err(format!("This sign-in was started in another browser, or not at {}. Start it again there.", st.cfg.url));
    }
    let prov = provider(st, None, p.provider).await.map_err(oops)?.ok_or("That sign-in provider has been turned off")?;
    if !q.error.is_empty() {
        return Err(format!("{} didn't sign you in ({})", prov.name, short(&q.error)));
    }
    if q.code.is_empty() {
        return Err(format!("{} sent no code back", prov.name));
    }
    let who = profile(st, &prov, &p, &q.code).await.map_err(|e| format!("Couldn't sign in with {}: {e}", prov.name))?;
    let mine = |uid: i64| v.as_ref().is_some_and(|u| u.id == uid);
    match p.mode {
        Mode::Login { invite } => login(st, &prov, who, invite, p.next, ip, headers).await,
        Mode::Link(uid) if mine(uid) => link(st, &prov, who, uid, ip).await.map(|_| (p.next, None)),
        Mode::Reauth(uid, hash) if mine(uid) && cur.as_ref().is_some_and(|s| s.hash == hash) => reauth(st, &prov, who, uid, hash, p.next).await,
        _ => Err("You were signed out in the meantime. Sign in and try again.".into()),
    }
}

/// The person the provider vouches for.
struct Who {
    subject: String,
    login: String,
    /// Only an address the provider says is verified; otherwise empty.
    email: String,
    name: String,
}

/// A provider's name or login: no control characters, at most 100 characters.
fn text(v: &Value) -> String {
    v.as_str().unwrap_or_default().trim().chars().filter(|c| !c.is_control()).take(100).collect()
}

/// An address as given (never shortened, which could make it someone else's), or nothing.
fn verified_email(v: &Value) -> String {
    let e = v.as_str().unwrap_or_default().trim();
    if validate::email(e) && e.len() <= 254 { e.into() } else { String::new() }
}

async fn profile(st: &AppState, p: &Provider, pend: &Pending, code: &str) -> Result<Who, String> {
    let secret = open_text(&st.key, &p.secret).ok_or("its client secret can't be read (the server key changed); an admin needs to enter it again")?;
    let redirect = redirect_uri(st);
    let body = form(&[
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", &redirect),
        ("client_id", &p.client_id),
        ("client_secret", &secret),
        ("code_verifier", &pend.verifier),
    ]);
    let t = call(pend.ends.token.clone(), None, Some(body), p.allow_private).await?;
    // GitHub answers a bad code with 200 and an error.
    if let Some(e) = t["error"].as_str() {
        return Err(format!("the code was refused ({})", short(e)));
    }
    let token = t["access_token"].as_str().filter(|s| !s.is_empty()).ok_or("no access token came back")?.to_string();
    let user = |url: String| call(url, Some(token.clone()), None, p.allow_private);
    let id = |u: &Value| u["id"].as_i64().map(|n| n.to_string()).ok_or("no user id came back");
    match p.kind.as_str() {
        "github" => {
            let u = user(pend.ends.user.clone()).await?;
            // Needs the user:email scope; without it there's simply no address.
            let emails = user(format!("{}/emails", pend.ends.user)).await.unwrap_or_default();
            let primary = emails.as_array().into_iter().flatten().find(|e| e["primary"] == true && e["verified"] == true);
            Ok(Who { subject: id(&u)?, login: text(&u["login"]), email: primary.map(|e| verified_email(&e["email"])).unwrap_or_default(), name: text(&u["name"]) })
        }
        "gitlab" => {
            let u = user(pend.ends.user.clone()).await?;
            let email = if u["confirmed_at"].is_string() { verified_email(&u["email"]) } else { String::new() };
            Ok(Who { subject: id(&u)?, login: text(&u["username"]), email, name: text(&u["name"]) })
        }
        _ => {
            let claims = id_token(&t, p, pend)?;
            let sub = claims["sub"].as_str().filter(|s| !s.is_empty() && s.len() <= 255).ok_or("the ID token names nobody")?.to_string();
            let u = user(pend.ends.user.clone()).await?;
            if u["sub"].as_str() != Some(sub.as_str()) {
                return Err("its user info names someone other than its ID token".into());
            }
            let claim = |k: &str| if u[k].is_null() { claims[k].clone() } else { u[k].clone() };
            let ok = matches!(claim("email_verified"), Value::Bool(true)) || claim("email_verified") == "true";
            let email = if ok { verified_email(&claim("email")) } else { String::new() };
            let login = [text(&claim("preferred_username")), text(&claim("nickname")), email.split('@').next().unwrap_or_default().to_string()]
                .into_iter()
                .find(|s| !s.is_empty())
                .unwrap_or_default();
            Ok(Who { subject: sub, login, email, name: text(&claim("name")) })
        }
    }
}

fn id_token(t: &Value, p: &Provider, pend: &Pending) -> Result<Value, String> {
    let jwt = t["id_token"].as_str().ok_or("no ID token came back")?;
    let c: Value = jwt
        .split('.')
        .nth(1)
        .and_then(|b| URL_SAFE_NO_PAD.decode(b.trim_end_matches('=')).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .ok_or("its ID token can't be read")?;
    let cid = Some(p.client_id.as_str());
    let aud: Vec<&str> = match &c["aud"] {
        Value::String(a) => vec![a.as_str()],
        Value::Array(a) => a.iter().filter_map(Value::as_str).collect(),
        _ => vec![],
    };
    if c["iss"].as_str() != Some(pend.ends.issuer.as_str()) {
        return Err("its ID token is from another issuer".into());
    }
    if !aud.contains(&p.client_id.as_str()) || ((aud.len() > 1 || !c["azp"].is_null()) && c["azp"].as_str() != cid) {
        return Err("its ID token is for another application".into());
    }
    if c["exp"].as_i64().is_none_or(|e| e <= now()) {
        return Err("its ID token has expired".into());
    }
    if !constant_eq(c["nonce"].as_str().unwrap_or_default(), &pend.nonce) {
        return Err("its ID token doesn't belong to this sign-in".into());
    }
    // A confirmation must be a sign-in made just now (a minute's leeway for clocks).
    if matches!(pend.mode, Mode::Reauth(..)) && c["auth_time"].as_i64().is_none_or(|t| t < pend.started - 60) {
        return Err("it didn't ask you to sign in again".into());
    }
    Ok(c)
}

async fn owner(st: &AppState, provider: i64, subject: String) -> Result<Option<i64>, String> {
    st.db
        .call(move |c| Ok(c.query_row("SELECT user_id FROM identities WHERE provider_id = ?1 AND subject = ?2", rusqlite::params![provider, subject], |r| r.get(0)).optional()?))
        .await
        .map_err(oops)
}

async fn login(st: &AppState, p: &Provider, who: Who, invite: String, next: String, ip: &str, headers: &HeaderMap) -> Result<Landing, String> {
    let detail = format!("{} ({})", p.name, who.login);
    let (user, action) = match owner(st, p.id, who.subject.clone()).await? {
        Some(uid) => (user_by_id(&st.db, uid).await.map_err(oops)?.ok_or("That account can't sign in right now")?, "login"),
        None => (signup(st, p, who, invite, ip).await?, "signup"),
    };
    if has_2fa(&st.db, user.id).await.map_err(oops)? {
        let challenge = st.challenges.issue(user.id);
        return Ok((with(&with("/login", "challenge", &challenge), "next", &next), None));
    }
    let token = create_session(&st.db, user.id, ip, &device(headers)).await.map_err(oops)?;
    record(&st.db, Event { actor: Some(&user), subject: Some(user.id), action, detail, ip }).await;
    Ok((next, Some(session_cookie(&token, st.cfg.secure_cookies))))
}

/// A username from the provider's: allowed characters only, then a number if it's taken.
fn username_base(login: &str) -> String {
    let s: String = login.chars().map(|c| if c.is_ascii_alphanumeric() || "-_.".contains(c) { c } else { '-' }).collect();
    let mut s: String = s.trim_start_matches(|c: char| !c.is_ascii_alphanumeric()).chars().take(35).collect();
    while s.contains("..") {
        s = s.replace("..", ".");
    }
    while s.to_ascii_lowercase().ends_with(".git") {
        s.truncate(s.len() - 4);
    }
    if s.is_empty() { "user".into() } else { s }
}

async fn signup(st: &AppState, p: &Provider, who: Who, invite: String, ip: &str) -> Result<User, String> {
    if !p.allow_signup {
        return Err(format!("No account here is linked to that {} account. Sign in another way, then link it under Settings → Security.", p.name));
    }
    // Apart from password sign-ups' 5 an hour: each of these needs its own account at the provider.
    if !st.limiter.allow(&format!("oauth-signup:{}", crate::auth::ip_bucket(ip)), 10, 3600) {
        return Err("Too many sign-ups from your network. Try again later.".into());
    }
    let mode = st.settings.get().signup;
    let invite_hash = (!invite.is_empty()).then(|| sha256_hex(&invite));
    let (pid, pname) = (p.id, p.name.clone());
    let base = username_base(&who.login);
    st.db
        .call(move |c| {
            let tx = c.transaction()?;
            if mode == "closed" {
                return Ok(Err("Sign-up is closed here. Ask an admin for an account.".to_string()));
            }
            if mode == "invite" {
                let used = match &invite_hash {
                    Some(h) => tx.execute(
                        "UPDATE invites SET uses = uses + 1 WHERE code_hash = ?1 AND uses < max_uses AND (expires_at IS NULL OR expires_at > ?2)",
                        rusqlite::params![h, now()],
                    )?,
                    None => 0,
                };
                if used == 0 {
                    return Ok(Err("Sign-up here is by invitation. Open your invite link, then sign up from there.".to_string()));
                }
            }
            if !who.email.is_empty() {
                let taken = tx.query_row("SELECT 1 FROM users WHERE email = ?1 COLLATE NOCASE AND email != ''", [&who.email], |_| Ok(())).optional()?;
                if taken.is_some() {
                    return Ok(Err(format!("An account here already uses {}. Sign in to it, then link {pname} under Settings → Security.", who.email)));
                }
            }
            let mut name = None;
            for n in 1..100 {
                let u = if n == 1 { base.clone() } else { format!("{base}-{n}") };
                if validate::username(&u) && tx.query_row("SELECT 1 FROM users WHERE username = ?1", [&u], |_| Ok(())).optional()?.is_none() {
                    name = Some(u);
                    break;
                }
            }
            let Some(name) = name else { return Ok(Err("Couldn't pick a username. Sign up with a password instead.".to_string())) };
            let display: String = who.name.chars().filter(|c| !c.is_control()).take(100).collect();
            tx.execute(
                "INSERT INTO users(username, email, pass_hash, display_name, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![name, who.email, NO_PASSWORD, display, now()],
            )?;
            let uid = tx.last_insert_rowid();
            tx.execute(
                "INSERT INTO identities(provider_id, subject, user_id, login, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![pid, who.subject, uid, who.login, now()],
            )?;
            let u = tx.query_row(&format!("SELECT {} FROM users WHERE id = ?1", crate::auth::USER_COLS), [uid], User::from_row)?;
            tx.commit()?;
            Ok(Ok(u))
        })
        .await
        .map_err(oops)?
}

async fn link(st: &AppState, p: &Provider, who: Who, uid: i64, ip: &str) -> Result<(), String> {
    let (pid, pname, login) = (p.id, p.name.clone(), who.login.clone());
    st.db
        .call(move |c| {
            let tx = c.transaction()?;
            let owner: Option<i64> = tx.query_row("SELECT user_id FROM identities WHERE provider_id = ?1 AND subject = ?2", rusqlite::params![pid, who.subject], |r| r.get(0)).optional()?;
            match owner {
                Some(o) if o == uid => return Ok(Ok(())),
                Some(_) => return Ok(Err(format!("That {pname} account is already linked to another account here."))),
                None => {}
            }
            if tx.query_row("SELECT 1 FROM identities WHERE provider_id = ?1 AND user_id = ?2", [pid, uid], |_| Ok(())).optional()?.is_some() {
                return Ok(Err(format!("You already linked a different {pname} account. Unlink it first.")));
            }
            tx.execute(
                "INSERT INTO identities(provider_id, subject, user_id, login, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![pid, who.subject, uid, who.login, now()],
            )?;
            tx.commit()?;
            Ok(Ok(()))
        })
        .await
        .map_err(oops)??;
    if let Ok(Some(u)) = user_by_id(&st.db, uid).await {
        record(&st.db, Event { actor: Some(&u), subject: Some(uid), action: "identity_linked", detail: format!("{} ({login})", p.name), ip }).await;
    }
    Ok(())
}

async fn reauth(st: &AppState, p: &Provider, who: Who, uid: i64, hash: String, next: String) -> Result<Landing, String> {
    if owner(st, p.id, who.subject).await? != Some(uid) {
        return Err(format!("That {} account isn't the one linked to yours.", p.name));
    }
    if has_2fa(&st.db, uid).await.map_err(oops)? {
        // The code still has to be entered; /api/auth/reauth takes this challenge instead of a password.
        return Ok((with(&next, "reauth_challenge", &st.challenges.issue(uid)), None));
    }
    st.db
        .call(move |c| Ok(c.execute("UPDATE sessions SET verified_at = ?1 WHERE token_hash = ?2 AND user_id = ?3", rusqlite::params![now(), hash, uid])?))
        .await
        .map_err(oops)?;
    Ok((next, None))
}

// ---------- sign-in methods (Settings → Security) ----------

pub async fn has_password(st: &AppState, uid: i64) -> R<bool> {
    st.db.call(move |c| Ok(c.query_row("SELECT pass_hash != ?1 FROM users WHERE id = ?2", rusqlite::params![NO_PASSWORD, uid], |r| r.get(0))?)).await
}

/// Enabled providers, for the sign-in page.
pub async fn public_list(State(st): State<AppState>) -> J<Value> {
    let rows = st
        .db
        .call(|c| {
            let mut s = c.prepare("SELECT slug, name, kind FROM oauth_providers WHERE enabled = 1 ORDER BY id")?;
            let rows = s.query_map([], |r| Ok(json!({ "slug": r.get::<_, String>(0)?, "name": r.get::<_, String>(1)?, "kind": r.get::<_, String>(2)? })))?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await?;
    Ok(Json(json!({ "providers": rows })))
}

pub async fn identities(State(st): State<AppState>, Authed(u): Authed) -> J<Value> {
    let uid = u.id;
    let rows = st
        .db
        .call(move |c| {
            let mut s = c.prepare(
                "SELECT i.id, i.login, i.created_at, p.slug, p.name, p.kind, p.enabled FROM identities i
                 JOIN oauth_providers p ON p.id = i.provider_id WHERE i.user_id = ?1 ORDER BY i.id",
            )?;
            let rows = s.query_map([uid], |r| {
                Ok(json!({
                    "id": r.get::<_, i64>(0)?, "login": r.get::<_, String>(1)?, "created_at": r.get::<_, i64>(2)?,
                    "provider": { "slug": r.get::<_, String>(3)?, "name": r.get::<_, String>(4)?, "kind": r.get::<_, String>(5)?, "enabled": r.get::<_, bool>(6)? },
                }))
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await?;
    Ok(Json(json!({ "has_password": has_password(&st, uid).await?, "identities": rows })))
}

/// Ways in that still work: a password, or an identity at an enabled provider.
const WAYS_IN: &str = "(SELECT pass_hash != '!' FROM users WHERE id = ?1) /* NO_PASSWORD */
    + (SELECT count(*) FROM identities i JOIN oauth_providers p ON p.id = i.provider_id WHERE i.user_id = ?1 AND p.enabled = 1 AND i.id != ?2)";

pub async fn unlink(State(st): State<AppState>, Sudo(u): Sudo, ip: ClientIp, Path(id): Path<i64>) -> J<Value> {
    let uid = u.id;
    let name = st
        .db
        .call(move |c| {
            let tx = c.transaction()?;
            let Some(name) = tx
                .query_row("SELECT p.name FROM identities i JOIN oauth_providers p ON p.id = i.provider_id WHERE i.id = ?1 AND i.user_id = ?2", [id, uid], |r| r.get::<_, String>(0))
                .optional()?
            else {
                return Err(AppError::not_found());
            };
            let left: i64 = tx.query_row(&format!("SELECT {WAYS_IN}"), [uid, id], |r| r.get(0))?;
            if left == 0 {
                return Err(AppError::bad("This is your only way to sign in. Set a password or link another account first."));
            }
            tx.execute("DELETE FROM identities WHERE id = ?1", [id])?;
            tx.commit()?;
            Ok(name)
        })
        .await?;
    record(&st.db, Event { actor: Some(&u), subject: Some(uid), action: "identity_unlinked", detail: name, ip: &ip.0 }).await;
    Ok(Json(json!({ "ok": true })))
}

// ---------- admin ----------

#[derive(Deserialize)]
pub struct ProviderReq {
    kind: Option<String>,
    slug: Option<String>,
    name: Option<String>,
    base_url: Option<String>,
    client_id: Option<String>,
    client_secret: Option<String>,
    allow_private: Option<bool>,
    enabled: Option<bool>,
    allow_signup: Option<bool>,
}

struct Row {
    slug: String,
    kind: String,
    name: String,
    base_url: String,
    client_id: String,
    allow_private: bool,
    enabled: bool,
    allow_signup: bool,
}

impl Row {
    fn apply(mut self, q: &ProviderReq) -> R<Row> {
        let t = |v: &Option<String>| v.as_ref().map(|s| s.trim().to_string());
        self.slug = t(&q.slug).unwrap_or(self.slug);
        self.name = t(&q.name).unwrap_or(self.name);
        self.base_url = t(&q.base_url).map(|u| u.trim_end_matches('/').to_string()).unwrap_or(self.base_url);
        self.client_id = t(&q.client_id).unwrap_or(self.client_id);
        self.allow_private = q.allow_private.unwrap_or(self.allow_private);
        self.enabled = q.enabled.unwrap_or(self.enabled);
        self.allow_signup = q.allow_signup.unwrap_or(self.allow_signup);
        let b = self.slug.as_bytes();
        if b.is_empty() || b.len() > 30 || !b[0].is_ascii_alphanumeric() || !b.iter().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-') {
            return Err(AppError::bad("The short name uses lowercase letters, digits and '-', up to 30"));
        }
        if self.name.is_empty() || self.name.chars().count() > 60 {
            return Err(AppError::bad("Give the provider a name of up to 60 characters"));
        }
        check(&self.base_url, self.allow_private).map_err(AppError::bad)?;
        if self.base_url.contains(['?', '#']) {
            return Err(AppError::bad("Leave the query and fragment out of the address"));
        }
        if self.client_id.is_empty() || self.client_id.len() > 200 || self.client_id.chars().any(char::is_control) {
            return Err(AppError::bad("Enter the client ID the provider gave you"));
        }
        Ok(self)
    }
}

fn secret_of(q: &ProviderReq) -> R<Option<String>> {
    match q.client_secret.as_deref().map(str::trim) {
        None | Some("") => Ok(None),
        Some(s) if s.len() > 500 || s.chars().any(char::is_control) => Err(AppError::bad("That client secret doesn't look right")),
        Some(s) => Ok(Some(s.to_string())),
    }
}

const COLS: &str = "id, slug, kind, name, base_url, client_id, secret != '', allow_private, enabled, allow_signup, created_at,
    (SELECT count(*) FROM identities WHERE provider_id = oauth_providers.id)";

fn row_json(r: &rusqlite::Row) -> rusqlite::Result<Value> {
    Ok(json!({
        "id": r.get::<_, i64>(0)?, "slug": r.get::<_, String>(1)?, "kind": r.get::<_, String>(2)?, "name": r.get::<_, String>(3)?,
        "base_url": r.get::<_, String>(4)?, "client_id": r.get::<_, String>(5)?, "has_secret": r.get::<_, bool>(6)?,
        "allow_private": r.get::<_, bool>(7)?, "enabled": r.get::<_, bool>(8)?, "allow_signup": r.get::<_, bool>(9)?,
        "created_at": r.get::<_, i64>(10)?, "identities": r.get::<_, i64>(11)?,
    }))
}

async fn one(st: &AppState, id: i64) -> R<Value> {
    st.db.call(move |c| Ok(c.query_row(&format!("SELECT {COLS} FROM oauth_providers WHERE id = ?1"), [id], row_json).optional()?)).await?.ok_or_else(AppError::not_found)
}

pub async fn list(State(st): State<AppState>, _a: Admin) -> J<Value> {
    let rows = st
        .db
        .call(|c| {
            let mut s = c.prepare(&format!("SELECT {COLS} FROM oauth_providers ORDER BY id"))?;
            let rows = s.query_map([], row_json)?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await?;
    Ok(Json(json!({ "providers": rows, "callback_url": redirect_uri(&st) })))
}

pub async fn create(State(st): State<AppState>, Admin(me): Admin, _s: Sudo, ip: ClientIp, Json(q): Json<ProviderReq>) -> J<Value> {
    let kind = q.kind.as_deref().unwrap_or_default().trim().to_string();
    let (name, base) = match kind.as_str() {
        "github" => ("GitHub", "https://github.com"),
        "gitlab" => ("GitLab", "https://gitlab.com"),
        "oidc" => ("OpenID Connect", ""),
        _ => return Err(AppError::bad("The kind is github, gitlab or oidc")),
    };
    let blank = Row { slug: kind.clone(), kind, name: name.into(), base_url: base.into(), client_id: String::new(), allow_private: false, enabled: true, allow_signup: true };
    let r = blank.apply(&q)?;
    let secret = secret_of(&q)?.ok_or_else(|| AppError::bad("Enter the client secret the provider gave you"))?;
    let sealed = seal_text(&st.key, &secret);
    let slug = r.slug.clone();
    let id = st
        .db
        .call(move |c| {
            if c.query_row("SELECT 1 FROM oauth_providers WHERE slug = ?1", [&r.slug], |_| Ok(())).optional()?.is_some() {
                return Err(AppError::conflict("Another provider has that button name (or short name): pick a different one"));
            }
            c.execute(
                "INSERT INTO oauth_providers(slug, kind, name, base_url, client_id, secret, allow_private, enabled, allow_signup, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                rusqlite::params![r.slug, r.kind, r.name, r.base_url, r.client_id, sealed, r.allow_private, r.enabled, r.allow_signup, now()],
            )?;
            Ok(c.last_insert_rowid())
        })
        .await?;
    record(&st.db, Event { actor: Some(&me), subject: None, action: "oauth_provider_added", detail: slug, ip: &ip.0 }).await;
    Ok(Json(one(&st, id).await?))
}

pub async fn update(State(st): State<AppState>, Admin(me): Admin, _s: Sudo, ip: ClientIp, Path(id): Path<i64>, Json(q): Json<ProviderReq>) -> J<Value> {
    let cur = st
        .db
        .call(move |c| {
            Ok(c.query_row("SELECT slug, kind, name, base_url, client_id, allow_private, enabled, allow_signup FROM oauth_providers WHERE id = ?1", [id], |r| {
                Ok(Row {
                    slug: r.get(0)?,
                    kind: r.get(1)?,
                    name: r.get(2)?,
                    base_url: r.get(3)?,
                    client_id: r.get(4)?,
                    allow_private: r.get(5)?,
                    enabled: r.get(6)?,
                    allow_signup: r.get(7)?,
                })
            })
            .optional()?)
        })
        .await?
        .ok_or_else(AppError::not_found)?;
    if q.kind.as_deref().is_some_and(|k| k.trim() != cur.kind) {
        return Err(AppError::bad("A provider's kind can't change. Add a new provider instead."));
    }
    let (old_url, old_id) = (cur.base_url.clone(), cur.client_id.clone());
    let r = cur.apply(&q)?;
    let sealed = secret_of(&q)?.map(|s| seal_text(&st.key, &s));
    // The stored secret only ever goes to the server it was made for.
    if sealed.is_none() && (r.base_url != old_url || r.client_id != old_id) {
        return Err(AppError::bad("Enter the client secret again when the address or client ID changes"));
    }
    let slug = r.slug.clone();
    st.db
        .call(move |c| {
            if c.query_row("SELECT 1 FROM oauth_providers WHERE slug = ?1 AND id != ?2", rusqlite::params![r.slug, id], |_| Ok(())).optional()?.is_some() {
                return Err(AppError::conflict("Another provider has that button name (or short name): pick a different one"));
            }
            c.execute(
                "UPDATE oauth_providers SET slug = ?1, name = ?2, base_url = ?3, client_id = ?4, secret = coalesce(?5, secret),
                 allow_private = ?6, enabled = ?7, allow_signup = ?8 WHERE id = ?9",
                rusqlite::params![r.slug, r.name, r.base_url, r.client_id, sealed, r.allow_private, r.enabled, r.allow_signup, id],
            )?;
            Ok(())
        })
        .await?;
    record(&st.db, Event { actor: Some(&me), subject: None, action: "oauth_provider_changed", detail: slug, ip: &ip.0 }).await;
    Ok(Json(one(&st, id).await?))
}

/// Refused while someone would be left with no way to sign in.
pub async fn remove(State(st): State<AppState>, Admin(me): Admin, _s: Sudo, ip: ClientIp, Path(id): Path<i64>) -> J<Value> {
    let slug = st
        .db
        .call(move |c| {
            let tx = c.transaction()?;
            let Some(slug) = tx.query_row("SELECT slug FROM oauth_providers WHERE id = ?1", [id], |r| r.get::<_, String>(0)).optional()? else {
                return Err(AppError::not_found());
            };
            let stranded: i64 = tx.query_row(
                "SELECT count(*) FROM identities i JOIN users u ON u.id = i.user_id
                 WHERE i.provider_id = ?1 AND u.pass_hash = '!' /* NO_PASSWORD */ AND NOT EXISTS (
                     SELECT 1 FROM identities j JOIN oauth_providers p ON p.id = j.provider_id
                     WHERE j.user_id = i.user_id AND j.provider_id != ?1 AND p.enabled = 1)",
                [id],
                |r| r.get(0),
            )?;
            if stranded > 0 {
                return Err(AppError::conflict(format!(
                    "{stranded} account(s) sign in only through this provider and would be locked out. Turn it off instead, or have them set a password first."
                )));
            }
            tx.execute("DELETE FROM oauth_providers WHERE id = ?1", [id])?;
            tx.commit()?;
            Ok(slug)
        })
        .await?;
    record(&st.db, Event { actor: Some(&me), subject: None, action: "oauth_provider_removed", detail: slug, ip: &ip.0 }).await;
    Ok(Json(json!({ "ok": true })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_stays_on_this_site_and_usernames_are_valid() {
        for (n, want) in [("/a/b?c=1", "/a/b?c=1"), ("//evil", "/"), ("https://evil", "/"), ("/\\evil", "/"), ("/api/me", "/"), ("", "/"), ("/x\ny", "/")] {
            assert_eq!(local(n), want, "{n}");
        }
        for (l, want) in [("Ann Lee", "Ann-Lee"), ("..x..y", "x.y"), ("a...b", "a.b"), ("repo.git", "repo"), ("", "user"), ("äö", "user"), ("-_a", "a")] {
            assert_eq!(username_base(l), want, "{l}");
        }
        assert_eq!(with("/a", "k", "a b&c"), "/a?k=a%20b%26c");
        assert_eq!(with("/a?x=1", "k", "v"), "/a?x=1&k=v");
    }
}
