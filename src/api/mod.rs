pub mod account;
pub mod admin;
pub mod code;
pub mod issues;
pub mod repos;
pub mod security;

use std::net::SocketAddr;

use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::request::Parts;
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};
use rusqlite::types::Value;
use serde::Serialize;

use crate::AppState;
use crate::error::{AppError, R};

pub type J<T> = R<Json<T>>;

pub fn ok() -> J<serde_json::Value> {
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub fn router() -> Router<AppState> {
    let repo = "/repos/{owner}/{repo}";
    Router::new()
        .route("/auth/signup", post(account::signup))
        .route("/auth/login", post(account::login))
        .route("/auth/logout", post(account::logout))
        .route("/auth/login/2fa", post(account::login_2fa))
        .route("/auth/reauth", post(security::reauth))
        .route("/me/security", get(security::overview))
        .route("/me/audit", get(security::my_log))
        .route("/me/2fa/setup", post(security::setup))
        .route("/me/2fa/enable", post(security::enable))
        .route("/me/2fa/disable", post(security::disable))
        .route("/me/2fa/recovery", post(security::new_codes))
        .route("/me/sessions/revoke-others", post(security::revoke_others))
        .route("/me/sessions/{id}", delete(security::revoke))
        .route("/me", get(account::me).patch(account::update_me))
        .route("/me/password", post(account::change_password))
        .route(
            "/me/tokens",
            get(account::list_tokens).post(account::create_token),
        )
        .route("/me/tokens/{id}", delete(account::delete_token))
        .route("/users/{user}", get(account::profile))
        .route("/users/{user}/stars", get(account::user_stars))
        .route("/feed", get(account::feed))
        .route("/explore", get(account::explore))
        .route("/search", get(account::search))
        .route("/admin/overview", get(admin::overview))
        .route("/admin/users", get(admin::users))
        .route("/admin/audit", get(security::admin_log))
        .route(
            "/admin/users/{user}",
            patch(admin::update_user).delete(admin::delete_user),
        )
        .route(
            "/admin/settings",
            get(admin::get_settings).put(admin::put_settings),
        )
        .route("/repos", post(repos::create))
        .route(
            repo,
            get(repos::show).patch(repos::update).delete(repos::remove),
        )
        .route(
            &format!("{repo}/star"),
            post(repos::star).delete(repos::unstar),
        )
        .route(&format!("{repo}/fork"), post(repos::fork))
        .route(
            &format!("{repo}/deploy-tokens"),
            get(repos::deploy_tokens).post(repos::create_deploy_token),
        )
        .route(&format!("{repo}/deploy-tokens/{{id}}"), delete(repos::delete_deploy_token))
        .route(
            &format!("{repo}/branches"),
            get(repos::branches)
                .post(repos::create_branch)
                .delete(repos::delete_branch),
        )
        .route(&format!("{repo}/tags"), get(repos::tags))
        .route(&format!("{repo}/languages"), get(code::languages))
        .route(&format!("{repo}/tree"), get(code::tree))
        .route(&format!("{repo}/tree-commits"), get(code::tree_commits))
        .route(&format!("{repo}/blob"), get(code::blob))
        .route(&format!("{repo}/files"), get(code::files))
        .route(&format!("{repo}/grep"), get(code::grep))
        .route(&format!("{repo}/commits"), get(code::commits))
        .route(&format!("{repo}/commit/{{sha}}"), get(code::commit))
        .route(&format!("{repo}/compare"), get(code::compare))
        .route(&format!("{repo}/blame"), get(code::blame))
        .route(&format!("{repo}/contents"), post(code::write_contents))
        .route(&format!("{repo}/contributors"), get(code::contributors))
        .route(
            &format!("{repo}/labels"),
            get(issues::labels).post(issues::create_label),
        )
        .route(
            &format!("{repo}/labels/{{id}}"),
            delete(issues::delete_label),
        )
        .route(
            &format!("{repo}/issues"),
            get(issues::list).post(issues::create),
        )
        .route(
            &format!("{repo}/issues/{{n}}"),
            get(issues::show).patch(issues::update),
        )
        .route(
            &format!("{repo}/issues/{{n}}/comments"),
            post(issues::comment),
        )
        .route(
            &format!("{repo}/comments/{{id}}"),
            patch(issues::edit_comment).delete(issues::delete_comment),
        )
        .route(&format!("{repo}/pulls"), post(issues::create_pull))
        .route(&format!("{repo}/pulls/{{n}}/diff"), get(issues::pull_diff))
        .route(
            &format!("{repo}/pulls/{{n}}/merge"),
            post(issues::merge_pull),
        )
        .fallback(|| async { AppError::not_found() })
}

/// Limited tokens only reach what their scope allows: reads everywhere, and for
/// `write` tokens also changes inside a repository (issues, PRs, files,
/// branches). Account, token, security and admin routes, and creating, forking,
/// deleting or reconfiguring repositories, need a full token or the web.
pub async fn scope_guard(
    axum::extract::State(st): axum::extract::State<AppState>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let (mut parts, body) = req.into_parts();
    let v = match crate::auth::Viewer::from_request_parts(&mut parts, &st).await {
        Ok(v) => v.0,
        Err(e) => return e.into_response(),
    };
    if let Some(l) = v.as_ref().and_then(|u| u.limits.as_ref())
        && (!token_may(l, &parts.method, parts.uri.path()) || !in_its_repo(&st, l, &parts).await)
    {
        let one = if l.repo_id.is_some() { " to one repository" } else { "" };
        return AppError::new(
            axum::http::StatusCode::FORBIDDEN,
            format!("This token can't do that (it has {} access{one})", l.scope.as_str()),
        )
        .into_response();
    }
    next.run(axum::extract::Request::from_parts(parts, body)).await
}

/// A deploy token's writes must target its own repository: issues, comments
/// and stars elsewhere would be made as its owner.
async fn in_its_repo(st: &AppState, l: &crate::auth::Limits, parts: &Parts) -> bool {
    let Some(want) = l.repo_id else { return true };
    if matches!(parts.method, axum::http::Method::GET | axum::http::Method::HEAD) {
        return true;
    }
    let segs: Vec<String> = parts.uri.path().trim_matches('/').split('/').map(String::from).collect();
    let (Some(o), Some(n)) = (segs.get(1).cloned(), segs.get(2).cloned()) else { return false };
    let id: Option<i64> = st
        .db
        .call(move |c| {
            use rusqlite::OptionalExtension;
            Ok(c.query_row(
                "SELECT r.id FROM repos r JOIN users u ON u.id = r.owner_id WHERE u.username = ?1 AND r.name = ?2",
                [o, n],
                |r| r.get(0),
            )
            .optional()?)
        })
        .await
        .unwrap_or(None);
    id == Some(want)
}

/// Account-level routes no limited token may touch, even to read.
const ACCOUNT_ROUTES: &[&str] = &["/admin", "/me/tokens", "/me/security", "/me/audit", "/me/2fa", "/me/sessions", "/me/password", "/auth"];

fn token_may(l: &crate::auth::Limits, method: &axum::http::Method, path: &str) -> bool {
    if ACCOUNT_ROUTES.iter().any(|p| path == *p || path.starts_with(&format!("{p}/"))) {
        return false;
    }
    if matches!(*method, axum::http::Method::GET | axum::http::Method::HEAD) {
        return true;
    }
    // Writes: only inside a repository (/repos/{owner}/{repo}/{thing}...), never forking it.
    let segs: Vec<&str> = path.trim_matches('/').split('/').collect();
    l.scope != crate::auth::Scope::Read
        && segs.len() >= 4
        && segs[0] == "repos"
        && !matches!(segs[3], "fork" | "deploy-tokens")
}

/// The client's IP: the socket address, or the last X-Forwarded-For hop (added by
/// our own proxy; earlier hops are client-controlled) when behind a trusted proxy.
pub struct ClientIp(pub String);

impl FromRequestParts<AppState> for ClientIp {
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, st: &AppState) -> R<Self> {
        if st.cfg.trust_proxy
            && let Some(ip) = parts
                .headers
                .get("x-forwarded-for")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.rsplit(',').next())
        {
            return Ok(ClientIp(ip.trim().to_string()));
        }
        let ip = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|c| c.0.ip().to_string())
            .unwrap_or_else(|| "unknown".into());
        Ok(ClientIp(ip))
    }
}

/// Visible-repo card used by lists (explore, profiles, stars).
#[derive(Serialize)]
pub struct RepoCard {
    pub owner: String,
    pub name: String,
    pub description: String,
    pub private: bool,
    pub language: String,
    pub stars: i64,
    pub forks: i64,
    pub updated_at: i64,
    pub fork_of: Option<String>,
}

pub const CARD_SELECT: &str = "SELECT u.username, r.name, r.description, r.is_private, r.language,
    (SELECT count(*) FROM stars s WHERE s.repo_id = r.id) AS stars,
    (SELECT count(*) FROM repos f WHERE f.forked_from = r.id) AS forks,
    r.updated_at,
    (SELECT pu.username || '/' || p.name FROM repos p JOIN users pu ON pu.id = p.owner_id WHERE p.id = r.forked_from AND p.is_private = 0)
    FROM repos r JOIN users u ON u.id = r.owner_id";

/// SQL condition: repo `r` is visible to the viewer. Binds one parameter (viewer id, or -1).
pub const VISIBLE: &str = "(r.is_private = 0 OR r.owner_id = ? OR EXISTS (SELECT 1 FROM users a WHERE a.id = ? AND a.is_admin = 1))";

pub fn visible_params(viewer: Option<&crate::auth::User>) -> [Value; 2] {
    // A deploy token lists repositories as if signed out (it's for one repo only).
    let viewer = viewer.filter(|u| u.limits.as_ref().is_none_or(|l| l.repo_id.is_none()));
    let id = viewer.map(|u| u.id).unwrap_or(-1);
    [Value::Integer(id), Value::Integer(id)]
}

pub fn repo_cards(
    c: &rusqlite::Connection,
    tail: &str,
    params: Vec<Value>,
) -> rusqlite::Result<Vec<RepoCard>> {
    let mut stmt = c.prepare(&format!("{CARD_SELECT} {tail}"))?;
    let rows = stmt.query_map(rusqlite::params_from_iter(params), |r| {
        Ok(RepoCard {
            owner: r.get(0)?,
            name: r.get(1)?,
            description: r.get(2)?,
            private: r.get::<_, i64>(3)? != 0,
            language: r.get(4)?,
            stars: r.get(5)?,
            forks: r.get(6)?,
            updated_at: r.get(7)?,
            fork_of: r.get(8)?,
        })
    })?;
    rows.collect()
}

/// Escape a user string for use inside a SQL LIKE pattern (with ESCAPE '\').
pub fn like(s: &str) -> String {
    let mut o = String::from("%");
    for ch in s.chars() {
        if matches!(ch, '%' | '_' | '\\') {
            o.push('\\');
        }
        o.push(ch);
    }
    o.push('%');
    o
}

pub fn page(p: Option<usize>) -> usize {
    p.unwrap_or(1).clamp(1, 10_000)
}
