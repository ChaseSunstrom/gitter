use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, header};
use axum::response::{IntoResponse, Response};
use rusqlite::OptionalExtension;
use rusqlite::types::Value;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{ClientIp, J, VISIBLE, like, ok, page, repo_cards, visible_params};
use crate::auth::{
    Authed, SESSION_COOKIE, Sudo, TOKEN_PREFIX, USER_COLS, User, Viewer, clear_cookie, cookie_value,
    create_session, device, has_2fa, hash_password, new_secret, password_login, session_cookie, sha256_hex,
};
use crate::security::{Event, record};
use crate::db::now;
use crate::error::{AppError, R};
use crate::{AppState, validate};

#[derive(Deserialize)]
pub struct SignupReq {
    username: String,
    email: String,
    password: String,
    /// The one-time setup code, only needed for the very first account.
    #[serde(default)]
    setup: String,
    /// An invite code, needed when sign-up is invite-only.
    #[serde(default)]
    invite: String,
}

fn check_password(pw: &str) -> R<()> {
    if pw.chars().count() < 8 {
        return Err(AppError::bad("Use at least 8 characters for your password"));
    }
    if pw.len() > 256 {
        return Err(AppError::bad("That password is too long"));
    }
    Ok(())
}

fn signed_in(st: &AppState, token: &str, user: User) -> Response {
    let cookie = session_cookie(token, st.cfg.secure_cookies);
    (
        [(header::SET_COOKIE, cookie)],
        Json(json!({ "user": user })),
    )
        .into_response()
}

pub async fn signup(
    State(st): State<AppState>,
    ip: ClientIp,
    headers: HeaderMap,
    Json(req): Json<SignupReq>,
) -> R<Response> {
    if !st.limiter.allow(&format!("signup:{}", crate::auth::ip_bucket(&ip.0)), 5, 3600) {
        return Err(AppError::new(
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            "Too many sign-ups from your network. Try again later.",
        ));
    }
    let username = req.username.trim().to_string();
    let email = req.email.trim().to_string();
    if !validate::username(&username) {
        return Err(AppError::bad(
            "Usernames use letters, numbers, '-', '_' or '.', start with a letter or number, and are up to 39 characters",
        ));
    }
    if !validate::email(&email) {
        return Err(AppError::bad("Enter a valid email address"));
    }
    check_password(&req.password)?;
    let hash = hash_password(req.password).await?;
    let mode = st.settings.get().signup;
    let invite_hash = (!req.invite.trim().is_empty()).then(|| sha256_hex(req.invite.trim()));
    let (setup, given) = (st.setup_code.clone(), req.setup.clone());
    let (u2, e2) = (username.clone(), email.clone());
    let user = st
        .db
        .call(move |c| {
            let tx = c.transaction()?;
            let count: i64 = tx.query_row("SELECT count(*) FROM users", [], |r| r.get(0))?;
            // The first account on a fresh instance becomes the admin, even when sign-up is
            // closed, but only with the setup code from the server log, so a stranger can't claim it.
            // Held until the commit, so the code is only used up by an account that really exists.
            let mut code = setup.lock().unwrap_or_else(|p| p.into_inner());
            if count == 0 {
                if let Some(expected) = code.as_ref()
                    && !crate::auth::constant_eq(expected, &given)
                {
                    return Err(AppError::new(
                        axum::http::StatusCode::FORBIDDEN,
                        "To create the admin account, open the setup link printed in the server log",
                    ));
                }
            } else if mode == "closed" {
                return Err(AppError::forbidden());
            } else if mode == "invite" {
                let used = match &invite_hash {
                    Some(h) => tx.execute(
                        "UPDATE invites SET uses = uses + 1 WHERE code_hash = ?1 AND uses < max_uses AND (expires_at IS NULL OR expires_at > ?2)",
                        rusqlite::params![h, now()],
                    )?,
                    None => 0,
                };
                if used == 0 {
                    return Err(AppError::new(
                        axum::http::StatusCode::FORBIDDEN,
                        "Sign-up here is by invitation, and that invite link is missing, used up or expired",
                    ));
                }
            }
            let taken: bool = tx
                .query_row("SELECT 1 FROM users WHERE username = ?1 OR (email = ?2 COLLATE NOCASE AND email != '')", [&u2, &e2], |_| Ok(true))
                .optional()?
                .unwrap_or(false);
            if taken {
                return Err(AppError::conflict("That username or email is already taken"));
            }
            tx.execute(
                "INSERT INTO users(username, email, pass_hash, is_admin, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![u2, e2, hash, (count == 0) as i64, now()],
            )?;
            let id = tx.last_insert_rowid();
            let u = tx.query_row(&format!("SELECT {USER_COLS} FROM users WHERE id = ?1"), [id], User::from_row)?;
            tx.commit()?;
            if count == 0 {
                *code = None;
            }
            Ok(u)
        })
        .await?;
    let token = create_session(&st.db, user.id, &ip.0, &device(&headers)).await?;
    let detail = if user.is_admin { "first account (admin)" } else { "" };
    record(&st.db, Event { actor: Some(&user), subject: Some(user.id), action: "signup", detail: detail.into(), ip: &ip.0 }).await;
    Ok(signed_in(&st, &token, user))
}

#[derive(Deserialize)]
pub struct LoginReq {
    login: String,
    password: String,
}

pub async fn login(
    State(st): State<AppState>,
    ip: ClientIp,
    headers: HeaderMap,
    Json(req): Json<LoginReq>,
) -> R<Response> {
    let login = req.login.trim().to_string();
    // Only failures count, per account+IP and per IP, so nobody can lock someone else out.
    let (k_user, k_ip) = (
        format!("login:{}:{}", login.to_lowercase(), crate::auth::ip_bucket(&ip.0)),
        format!("login-ip:{}", crate::auth::ip_bucket(&ip.0)),
    );
    if st.limiter.blocked(&k_user, 10) || st.limiter.blocked(&k_ip, 40) {
        return Err(AppError::new(
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            "Too many attempts. Wait a few minutes and try again.",
        ));
    }
    match password_login(&st.db, &login, &req.password).await? {
        Some(user) => {
            if has_2fa(&st.db, user.id).await? {
                // The password was right; the session waits for the second factor.
                let challenge = st.challenges.issue(user.id);
                return Ok(Json(json!({ "two_factor": true, "challenge": challenge })).into_response());
            }
            let token = create_session(&st.db, user.id, &ip.0, &device(&headers)).await?;
            record(&st.db, Event { actor: Some(&user), subject: Some(user.id), action: "login", detail: "password".into(), ip: &ip.0 }).await;
            Ok(signed_in(&st, &token, user))
        }
        None => {
            st.limiter.fail(&k_user, 300);
            st.limiter.fail(&k_ip, 300);
            Err(AppError::new(
                axum::http::StatusCode::UNAUTHORIZED,
                "Incorrect username or password",
            ))
        }
    }
}

#[derive(Deserialize)]
pub struct TwoFactorLogin {
    challenge: String,
    code: String,
}

/// Second step of signing in to an account with two-factor auth.
pub async fn login_2fa(
    State(st): State<AppState>,
    ip: ClientIp,
    headers: HeaderMap,
    Json(req): Json<TwoFactorLogin>,
) -> R<Response> {
    let Some(uid) = st.challenges.attempt(&req.challenge) else {
        return Err(AppError::new(axum::http::StatusCode::UNAUTHORIZED, "That sign-in expired. Enter your password again.").with_code("restart"));
    };
    // Counted before checking, so parallel guesses share the budget.
    let key = format!("2fa:{uid}");
    if !st.limiter.allow(&key, 10, 900) {
        return Err(AppError::new(axum::http::StatusCode::TOO_MANY_REQUESTS, "Too many wrong codes. Wait 15 minutes and try again."));
    }
    let user = st
        .db
        .call(move |c| Ok(c.query_row(&format!("SELECT {USER_COLS} FROM users WHERE id = ?1 AND suspended = 0"), [uid], User::from_row)?))
        .await?;
    match crate::security::use_code(&st.db, uid, &req.code).await? {
        Some(how) => {
            st.challenges.finish(&req.challenge);
            st.limiter.reset(&key);
            let token = create_session(&st.db, uid, &ip.0, &device(&headers)).await?;
            record(&st.db, Event { actor: Some(&user), subject: Some(uid), action: "login", detail: format!("password + {how}"), ip: &ip.0 }).await;
            Ok(signed_in(&st, &token, user))
        }
        None => {
            record(&st.db, Event { actor: None, subject: Some(uid), action: "2fa_failed", detail: "wrong code at sign-in".into(), ip: &ip.0 }).await;
            Err(AppError::new(axum::http::StatusCode::UNAUTHORIZED, "That code didn't work. Codes change every 30 seconds, and each works once."))
        }
    }
}

pub async fn logout(State(st): State<AppState>, headers: HeaderMap) -> R<Response> {
    if let Some(raw) = cookie_value(&headers, SESSION_COOKIE) {
        let h = sha256_hex(raw);
        st.db
            .call(move |c| Ok(c.execute("DELETE FROM sessions WHERE token_hash = ?1", [h])?))
            .await?;
    }
    Ok((
        [(header::SET_COOKIE, clear_cookie(st.cfg.secure_cookies))],
        Json(json!({ "ok": true })),
    )
        .into_response())
}

#[derive(Serialize)]
pub struct Profile {
    username: String,
    display_name: String,
    email: String,
    bio: String,
    location: String,
    website: String,
    is_admin: bool,
    created_at: i64,
}

fn profile_row(r: &rusqlite::Row) -> rusqlite::Result<Profile> {
    Ok(Profile {
        username: r.get(0)?,
        display_name: r.get(1)?,
        email: r.get(2)?,
        bio: r.get(3)?,
        location: r.get(4)?,
        website: r.get(5)?,
        is_admin: r.get::<_, i64>(6)? != 0,
        created_at: r.get(7)?,
    })
}
const PROFILE_COLS: &str =
    "username, display_name, email, bio, location, website, is_admin, created_at";

pub async fn me(State(st): State<AppState>, Viewer(v): Viewer) -> J<serde_json::Value> {
    let Some(u) = v else {
        return Ok(Json(
            json!({ "user": null, "signup": st.settings.get().signup_open(), "signup_mode": st.settings.get().signup, "instance": instance_info(&st) }),
        ));
    };
    let id = u.id;
    let p = st
        .db
        .call(move |c| {
            Ok(c.query_row(
                &format!("SELECT {PROFILE_COLS} FROM users WHERE id = ?1"),
                [id],
                profile_row,
            )?)
        })
        .await?;
    let quota = st.settings.get().max_storage_mb;
    let uid = u.id;
    let used: i64 = st
        .db
        .call(move |c| Ok(c.query_row(&format!("SELECT {}", crate::repo::STORAGE_USED), [uid], |r| r.get(0))?))
        .await?;
    let storage = json!({ "used": used, "quota": if quota == 0 || u.is_admin { None } else { Some(quota * 1024 * 1024) } });
    let (mut p, mut u) = (serde_json::to_value(p).map_err(AppError::internal)?, u);
    if u.limits.is_some() {
        // A limited token proves which account it is, not the account's private details.
        p["email"] = json!("");
        u.email.clear();
    }
    Ok(Json(
        json!({ "user": u, "profile": p, "signup": st.settings.get().signup_open(), "signup_mode": st.settings.get().signup, "instance": instance_info(&st), "storage": storage }),
    ))
}

#[derive(Deserialize)]
pub struct UpdateMe {
    display_name: Option<String>,
    email: Option<String>,
    bio: Option<String>,
    location: Option<String>,
    website: Option<String>,
}

fn clip(s: Option<String>, max: usize) -> R<Option<String>> {
    match s {
        Some(s) if s.chars().count() > max => {
            Err(AppError::bad(format!("Keep that under {max} characters")))
        }
        Some(s) => Ok(Some(s.trim().to_string())),
        None => Ok(None),
    }
}

pub async fn update_me(
    State(st): State<AppState>,
    Authed(u): Authed,
    Json(req): Json<UpdateMe>,
) -> J<serde_json::Value> {
    let display = clip(req.display_name, 80)?;
    let email = clip(req.email, 254)?;
    let bio = clip(req.bio, 300)?;
    let location = clip(req.location, 80)?;
    let website = clip(req.website, 200)?;
    if let Some(e) = &email
        && !validate::email(e)
    {
        return Err(AppError::bad("Enter a valid email address"));
    }
    if let Some(w) = &website
        && !w.is_empty()
        && !(w.starts_with("https://") || w.starts_with("http://"))
    {
        return Err(AppError::bad("Website must start with https:// or http://"));
    }
    st.db
        .call(move |c| {
            let tx = c.transaction()?;
            if let Some(e) = &email {
                let taken = tx
                    .query_row(
                        "SELECT 1 FROM users WHERE email = ?1 COLLATE NOCASE AND id != ?2",
                        rusqlite::params![e, u.id],
                        |_| Ok(()),
                    )
                    .optional()?;
                if taken.is_some() {
                    return Err(AppError::conflict(
                        "Another account already uses that email",
                    ));
                }
            }
            for (col, val) in [
                ("display_name", display),
                ("email", email),
                ("bio", bio),
                ("location", location),
                ("website", website),
            ] {
                if let Some(v) = val {
                    tx.execute(
                        &format!("UPDATE users SET {col} = ?1 WHERE id = ?2"),
                        rusqlite::params![v, u.id],
                    )?;
                }
            }
            tx.commit()?;
            Ok(())
        })
        .await?;
    ok()
}

#[derive(Deserialize)]
pub struct PasswordReq {
    current: String,
    new: String,
}

pub async fn change_password(
    State(st): State<AppState>,
    Sudo(u): Sudo,
    ip: ClientIp,
    headers: HeaderMap,
    Json(req): Json<PasswordReq>,
) -> R<Response> {
    check_password(&req.new)?;
    let key = format!("reauth:{}", u.id);
    if !st.limiter.allow(&key, 10, 600) {
        return Err(AppError::new(axum::http::StatusCode::TOO_MANY_REQUESTS, "Too many attempts. Wait a few minutes."));
    }
    if password_login(&st.db, &u.username, &req.current)
        .await?
        .is_none()
    {
        return Err(AppError::bad("Your current password is incorrect"));
    }
    st.limiter.reset(&key);
    let hash = hash_password(req.new).await?;
    st.db
        .call(move |c| {
            let tx = c.transaction()?;
            tx.execute(
                "UPDATE users SET pass_hash = ?1 WHERE id = ?2",
                rusqlite::params![hash, u.id],
            )?;
            tx.execute("DELETE FROM sessions WHERE user_id = ?1", [u.id])?;
            tx.commit()?;
            Ok(())
        })
        .await?;
    // Every other session is now signed out; give this browser a fresh one.
    let token = create_session(&st.db, u.id, &ip.0, &device(&headers)).await?;
    record(&st.db, Event { actor: Some(&u), subject: Some(u.id), action: "password_changed", detail: "other sessions signed out".into(), ip: &ip.0 }).await;
    Ok((
        [(
            header::SET_COOKIE,
            session_cookie(&token, st.cfg.secure_cookies),
        )],
        Json(json!({ "ok": true })),
    )
        .into_response())
}

pub async fn list_tokens(State(st): State<AppState>, Authed(u): Authed) -> J<serde_json::Value> {
    let rows = st
        .db
        .call(move |c| {
            let cond = "t.user_id = ?1";
            token_rows(c, cond, u.id)
        })
        .await?;
    Ok(Json(json!({ "tokens": rows })))
}

/// Tokens matching `cond` (over table `t`, one integer parameter), newest first.
pub fn token_rows(c: &rusqlite::Connection, cond: &str, param: i64) -> R<Vec<serde_json::Value>> {
    let mut s = c.prepare(&format!(
        "SELECT t.id, t.name, t.created_at, t.last_used, t.scope, t.expires_at, u.username,
            (SELECT ou.username || '/' || r.name FROM repos r JOIN users ou ON ou.id = r.owner_id WHERE r.id = t.repo_id)
         FROM tokens t JOIN users u ON u.id = t.user_id WHERE {cond} ORDER BY t.id DESC"
    ))?;
    let v = s
        .query_map([param], |r| {
            let expires: Option<i64> = r.get(5)?;
            Ok(json!({
                "id": r.get::<_, i64>(0)?, "name": r.get::<_, String>(1)?, "created_at": r.get::<_, i64>(2)?,
                "last_used": r.get::<_, Option<i64>>(3)?, "scope": r.get::<_, String>(4)?, "expires_at": expires,
                "expired": expires.is_some_and(|e| e <= now()), "created_by": r.get::<_, String>(6)?,
                "repo": r.get::<_, Option<String>>(7)?,
            }))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(v)
}

#[derive(Deserialize)]
pub struct TokenReq {
    name: String,
    /// "read", "write" or "all" (the default, for older clients).
    #[serde(default = "all_scope")]
    pub scope: String,
    /// Limit the token to one repository: "owner/name".
    #[serde(default)]
    pub repo: String,
    /// Days until it stops working; none means never.
    pub expires_days: Option<i64>,
}

fn all_scope() -> String {
    "all".into()
}

impl TokenReq {
    pub fn deploy(name: String, write: bool, expires_days: Option<i64>) -> TokenReq {
        TokenReq { name, scope: if write { "write" } else { "read" }.into(), repo: String::new(), expires_days }
    }
}

/// Make a token for `u`, optionally tied to `repo_id`. Returns the secret (shown once).
pub async fn issue_token(st: &AppState, u: &User, ip: &str, req: TokenReq, repo: Option<(i64, String)>) -> R<String> {
    let name = req.name.trim().to_string();
    if name.is_empty() || name.len() > 80 {
        return Err(AppError::bad("Give the token a name (up to 80 characters)"));
    }
    let scope = crate::auth::Scope::parse(&req.scope).ok_or_else(|| AppError::bad("Access must be read, write or all"))?;
    if let Some(d) = req.expires_days
        && !(1..=3650).contains(&d)
    {
        return Err(AppError::bad("Expiry is 1 to 3650 days, or never"));
    }
    let expires = req.expires_days.map(|d| now() + d * 86400);
    let (raw, hash) = new_secret(TOKEN_PREFIX);
    let (uid, n2, rid) = (u.id, name.clone(), repo.as_ref().map(|r| r.0));
    st.db
        .call(move |c| {
            let n: i64 = c.query_row("SELECT count(*) FROM tokens WHERE user_id = ?1", [uid], |r| r.get(0))?;
            if n >= 50 {
                return Err(AppError::bad("You have 50 tokens already. Delete some first."));
            }
            c.execute(
                "INSERT INTO tokens(user_id, name, token_hash, created_at, scope, repo_id, expires_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![uid, n2, hash, now(), scope.as_str(), rid, expires],
            )?;
            Ok(())
        })
        .await?;
    let mut detail = format!("{name} ({} access", scope.as_str());
    if let Some((_, full)) = &repo {
        detail.push_str(&format!(" to {full}"));
    }
    if let Some(d) = req.expires_days {
        detail.push_str(&format!(", expires in {d} days"));
    }
    detail.push(')');
    record(&st.db, Event { actor: Some(u), subject: Some(u.id), action: "token_created", detail, ip }).await;
    Ok(raw)
}

pub async fn create_token(
    State(st): State<AppState>,
    Sudo(u): Sudo,
    ip: ClientIp,
    Json(req): Json<TokenReq>,
) -> J<serde_json::Value> {
    let repo = match req.repo.trim() {
        "" => None,
        full => {
            let (o, n) = full.split_once('/').ok_or_else(|| AppError::bad("Write the repository as owner/name"))?;
            let r = crate::repo::Repo::readable(&st, Some(&u), o, n).await?;
            Some((r.id, r.full_name()))
        }
    };
    let raw = issue_token(&st, &u, &ip.0, req, repo).await?;
    Ok(Json(json!({ "token": raw })))
}

pub async fn delete_token(
    State(st): State<AppState>,
    Sudo(u): Sudo,
    ip: ClientIp,
    Path(id): Path<i64>,
) -> J<serde_json::Value> {
    let uid = u.id;
    let name: Option<String> = st
        .db
        .call(move |c| {
            let n = c
                .query_row("SELECT name FROM tokens WHERE id = ?1 AND user_id = ?2", [id, uid], |r| r.get(0))
                .optional()?;
            c.execute("DELETE FROM tokens WHERE id = ?1 AND user_id = ?2", [id, uid])?;
            Ok(n)
        })
        .await?;
    if let Some(name) = name {
        record(&st.db, Event { actor: Some(&u), subject: Some(u.id), action: "token_deleted", detail: name, ip: &ip.0 }).await;
    }
    ok()
}

#[derive(Serialize)]
pub struct EventOut {
    kind: String,
    user: String,
    repo: Option<String>,
    title: String,
    target: String,
    count: i64,
    created_at: i64,
}

fn events(
    c: &rusqlite::Connection,
    cond: &str,
    mut params: Vec<Value>,
    viewer: Option<&User>,
    limit: i64,
) -> rusqlite::Result<Vec<EventOut>> {
    let sql = format!(
        "SELECT e.kind, u.username, ou.username || '/' || r.name, e.title, e.target, e.count, e.created_at
         FROM events e JOIN users u ON u.id = e.user_id
         LEFT JOIN repos r ON r.id = e.repo_id LEFT JOIN users ou ON ou.id = r.owner_id
         WHERE ({cond}) AND (e.repo_id IS NULL OR {VISIBLE})
           AND NOT (e.kind IN ('issue', 'pull', 'comment', 'merge') AND EXISTS (
             SELECT 1 FROM issues hi WHERE hi.repo_id = e.repo_id AND hi.number = CAST(e.target AS INTEGER) AND hi.hidden = 1))
         ORDER BY e.id DESC LIMIT {limit}"
    );
    params.extend(visible_params(viewer));
    let mut s = c.prepare(&sql)?;

    s.query_map(rusqlite::params_from_iter(params), |r| {
        Ok(EventOut {
            kind: r.get(0)?,
            user: r.get(1)?,
            repo: r.get(2)?,
            title: r.get(3)?,
            target: r.get(4)?,
            count: r.get(5)?,
            created_at: r.get(6)?,
        })
    })?
    .collect()
}

pub async fn profile(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    Path(name): Path<String>,
) -> J<serde_json::Value> {
    if !validate::name(&name, 39) {
        return Err(AppError::not_found());
    }
    let out = st
        .db
        .call(move |c| {
            let (id, p) = c.query_row(
                &format!("SELECT id, {PROFILE_COLS} FROM users WHERE username = ?1"),
                [&name],
                |r| {
                    let id: i64 = r.get(0)?;
                    Ok((id, Profile {
                        username: r.get(1)?,
                        display_name: r.get(2)?,
                        email: String::new(),
                        bio: r.get(4)?,
                        location: r.get(5)?,
                        website: r.get(6)?,
                        is_admin: r.get::<_, i64>(7)? != 0,
                        created_at: r.get(8)?,
                    }))
                },
            )?;
            let mut params = vec![Value::Integer(id)];
            params.extend(visible_params(v.as_ref()));
            let repos = repo_cards(c, &format!("WHERE r.owner_id = ? AND {VISIBLE} ORDER BY r.updated_at DESC"), params)?;
            let stars: i64 = c.query_row("SELECT count(*) FROM stars WHERE user_id = ?1", [id], |r| r.get(0))?;
            let since = now() - 371 * 86400;
            let mut hs = c.prepare(
                "SELECT date(e.created_at, 'unixepoch') d, sum(e.count) FROM events e LEFT JOIN repos r ON r.id = e.repo_id
                 WHERE e.user_id = ?1 AND e.created_at > ?2 AND (e.repo_id IS NULL OR r.is_private = 0) GROUP BY d",
            )?;
            let heat: Vec<(String, i64)> = hs.query_map([id, since], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
            let ev = events(c, "e.user_id = ?", vec![Value::Integer(id)], v.as_ref(), 30)?;
            Ok(json!({ "user": p, "repos": repos, "stars": stars, "heatmap": heat, "events": ev }))
        })
        .await?;
    Ok(Json(out))
}

pub async fn user_stars(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    Path(name): Path<String>,
) -> J<serde_json::Value> {
    let repos = st
        .db
        .call(move |c| {
            let mut params = vec![Value::Text(name)];
            params.extend(visible_params(v.as_ref()));
            Ok(repo_cards(
                c,
                &format!("JOIN stars st ON st.repo_id = r.id JOIN users su ON su.id = st.user_id WHERE su.username = ? AND {VISIBLE} ORDER BY st.created_at DESC LIMIT 200"),
                params,
            )?)
        })
        .await?;
    Ok(Json(json!({ "repos": repos })))
}

pub async fn feed(State(st): State<AppState>, Viewer(v): Viewer) -> J<serde_json::Value> {
    let out = st
        .db
        .call(move |c| {
            let Some(u) = v else {
                let ev = events(c, "1=1", vec![], None, 40)?;
                let trending = repo_cards(
                    c,
                    "WHERE r.is_private = 0 AND r.hidden = 0 ORDER BY (SELECT count(*) FROM stars s WHERE s.repo_id = r.id AND s.created_at > strftime('%s','now') - 7*86400) DESC, r.updated_at DESC LIMIT 8",
                    vec![],
                )?;
                return Ok(json!({ "events": ev, "repos": [], "trending": trending }));
            };
            // Through VISIBLE, so a deploy token (one repo only) doesn't list the owner's private repos.
            let mut p = vec![Value::Integer(u.id)];
            p.extend(visible_params(Some(&u)));
            let mine = repo_cards(c, &format!("WHERE r.owner_id = ? AND {VISIBLE} ORDER BY r.updated_at DESC LIMIT 100"), p)?;
            let ev = events(
                c,
                "e.user_id = ? OR e.repo_id IN (SELECT repo_id FROM stars WHERE user_id = ?) OR e.repo_id IN (SELECT id FROM repos WHERE owner_id = ?)",
                vec![Value::Integer(u.id), Value::Integer(u.id), Value::Integer(u.id)],
                Some(&u),
                50,
            )?;
            let global = if ev.len() < 10 { events(c, "1=1", vec![], Some(&u), 30)? } else { vec![] };
            let trending = repo_cards(
                c,
                "WHERE r.is_private = 0 AND r.hidden = 0 ORDER BY (SELECT count(*) FROM stars s WHERE s.repo_id = r.id) DESC, r.updated_at DESC LIMIT 6",
                vec![],
            )?;
            Ok(json!({ "events": ev, "global": global, "repos": mine, "trending": trending }))
        })
        .await?;
    Ok(Json(out))
}

#[derive(Deserialize)]
pub struct ExploreQ {
    q: Option<String>,
    sort: Option<String>,
    page: Option<usize>,
}

pub async fn explore(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    Query(q): Query<ExploreQ>,
) -> J<serde_json::Value> {
    let p = page(q.page);
    let out = st
        .db
        .call(move |c| {
            let order = match q.sort.as_deref() {
                Some("new") => "r.created_at DESC",
                Some("stars") => "stars DESC, r.updated_at DESC",
                _ => "r.updated_at DESC",
            };
            let mut params: Vec<Value> = visible_params(v.as_ref()).into();
            let mut cond = VISIBLE.to_string();
            if let Some(s) = q.q.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                cond.push_str(" AND (r.name LIKE ? ESCAPE '\\' OR r.description LIKE ? ESCAPE '\\' OR u.username LIKE ? ESCAPE '\\')");
                let l = like(s);
                params.extend([Value::Text(l.clone()), Value::Text(l.clone()), Value::Text(l)]);
            }
            let per = 30;
            let repos = repo_cards(c, &format!("WHERE {cond} ORDER BY {order} LIMIT {} OFFSET {}", per + 1, (p - 1) * per), params)?;
            let more = repos.len() > per;
            let repos: Vec<_> = repos.into_iter().take(per).collect();
            Ok(json!({ "repos": repos, "has_more": more, "page": p }))
        })
        .await?;
    Ok(Json(out))
}

#[derive(Deserialize)]
pub struct SearchQ {
    q: String,
}

pub async fn search(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    Query(q): Query<SearchQ>,
) -> J<serde_json::Value> {
    let s = q.q.trim().to_string();
    if s.is_empty() {
        return Ok(Json(json!({ "repos": [], "users": [] })));
    }
    let out = st
        .db
        .call(move |c| {
            let l = like(&s);
            let mut params: Vec<Value> = visible_params(v.as_ref()).into();
            params.extend([Value::Text(l.clone()), Value::Text(l.clone()), Value::Text(s.to_lowercase())]);
            let repos = repo_cards(
                c,
                &format!(
                    "WHERE {VISIBLE} AND (r.name LIKE ? ESCAPE '\\' OR (u.username || '/' || r.name) LIKE ? ESCAPE '\\')
                     ORDER BY (lower(r.name) = ?) DESC, stars DESC, r.updated_at DESC LIMIT 8"
                ),
                params,
            )?;
            let mut us = c.prepare("SELECT username, display_name FROM users WHERE username LIKE ?1 ESCAPE '\\' OR display_name LIKE ?1 ESCAPE '\\' LIMIT 5")?;
            let users: Vec<serde_json::Value> = us
                .query_map([&l], |r| Ok(json!({ "username": r.get::<_, String>(0)?, "display_name": r.get::<_, String>(1)? })))?
                .collect::<Result<_, _>>()?;
            Ok(json!({ "repos": repos, "users": users }))
        })
        .await?;
    Ok(Json(out))
}

fn instance_info(st: &AppState) -> serde_json::Value {
    let s = st.settings.get();
    let needs_setup = st.setup_code.lock().map(|c| c.is_some()).unwrap_or(false);
    json!({ "name": s.instance_name, "announcement": s.announcement, "needs_setup": needs_setup })
}
