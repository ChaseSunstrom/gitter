pub mod account;
pub mod admin;
pub mod code;
pub mod issues;
pub mod repos;

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
