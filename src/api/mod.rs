pub mod account;
pub mod admin;
pub mod checks;
pub mod ci;
pub mod ci_extras;
pub mod orgs;
pub mod code;
pub mod discussions;
pub mod insights;
pub mod issues;
pub mod lifecycle;
pub mod milestones;
pub mod mirrors;
pub mod moderation;
pub mod releases;
pub mod repos;
pub mod reviews;
pub mod scanning;
pub mod security;
pub mod snippets;
pub mod webhooks;
pub mod wiki;
pub mod mcp;
pub mod notifications;
pub mod oauth;
pub mod revert;

use std::net::SocketAddr;

use axum::extract::{ConnectInfo, DefaultBodyLimit, FromRequestParts};
use axum::http::request::Parts;
use axum::routing::{delete, get, patch, post, put};
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
        .route("/auth/providers", get(oauth::public_list))
        .route("/auth/oauth/callback", get(oauth::callback))
        .route("/auth/oauth/{slug}/start", post(oauth::start))
        .route("/imports", get(crate::imports::mine).post(crate::imports::start))
        .route("/imports/preview", post(crate::imports::preview))
        .route("/imports/{id}/cancel", post(crate::imports::cancel))
        .route("/me/identities", get(oauth::identities))
        .route("/me/identities/{id}", delete(oauth::unlink))
        .route("/admin/oauth-providers", get(oauth::list).post(oauth::create))
        .route("/admin/oauth-providers/{id}", patch(oauth::update).delete(oauth::remove))
        .route("/me/security", get(security::overview))
        .route("/me/audit", get(security::my_log))
        .route("/me/2fa/setup", post(security::setup))
        .route("/me/2fa/enable", post(security::enable))
        .route("/me/2fa/disable", post(security::disable))
        .route("/me/2fa/recovery", post(security::new_codes))
        .route("/me/sessions/revoke-others", post(security::revoke_others))
        .route("/me/sessions/{id}", delete(security::revoke))
        .route("/me", get(account::me).patch(account::update_me).delete(lifecycle::delete_me))
        .route("/me/transfers", get(lifecycle::my_transfers))
        .route("/me/transfers/{id}/accept", post(lifecycle::accept))
        .route("/me/transfers/{id}/decline", post(lifecycle::decline))
        .route("/me/trash", get(lifecycle::my_trash))
        .route("/me/trash/{id}/restore", post(lifecycle::restore))
        .route("/me/export", get(lifecycle::export_status).post(lifecycle::start_export))
        .route("/me/export/download", get(lifecycle::download_export))
        .route("/admin/trash", get(lifecycle::all_trash))
        .route("/admin/trash/{id}", delete(lifecycle::purge_now))
        .route("/me/password", post(account::change_password))
        .route(
            "/me/tokens",
            get(account::list_tokens).post(account::create_token),
        )
        .route("/me/tokens/{id}", delete(account::delete_token))
        .route(
            "/me/signing-keys",
            get(account::list_signing_keys).post(account::add_signing_key),
        )
        .route("/me/signing-keys/challenge", post(account::signing_key_challenge))
        .route("/me/signing-keys/{id}", delete(account::delete_signing_key))
        .route("/me/ssh-keys", get(account::list_ssh_keys).post(account::add_ssh_key))
        .route("/me/ssh-keys/{id}", delete(account::delete_ssh_key))
        .route("/users/{user}", get(account::profile))
        .route("/users/{user}/stars", get(account::user_stars))
        .route("/feed", get(account::feed))
        .route("/explore", get(account::explore))
        .route("/search", get(account::search))
        .route("/admin/overview", get(admin::overview))
        .route("/admin/users", get(admin::users))
        .route("/admin/audit", get(security::admin_log))
        .route("/admin/invites", get(admin::invites).post(admin::create_invite))
        .route("/admin/invites/{id}", delete(admin::delete_invite))
        .route("/orgs", post(orgs::create))
        .route("/orgs/{org}", get(orgs::show).patch(orgs::update).delete(orgs::delete))
        .route("/orgs/{org}/members/{user}", put(orgs::set_member).delete(orgs::remove_member))
        .route("/me/orgs", get(orgs::mine))
        .route("/admin/runners", get(ci::runners).post(ci::create_runner))
        .route("/admin/runners/{id}", delete(ci::delete_runner))
        .route("/admin/reports", get(moderation::queue))
        .route("/admin/reports/{id}/resolve", post(moderation::resolve))
        .route("/reports", post(moderation::report))
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
            &format!("{repo}/transfer"),
            get(lifecycle::offer).post(lifecycle::offer_transfer).delete(lifecycle::cancel_transfer),
        )
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
        .route(&format!("{repo}/network"), get(code::network))
        .route(&format!("{repo}/blame"), get(code::blame))
        .route(&format!("{repo}/contents"), post(code::write_contents))
        .route(&format!("{repo}/revert"), post(revert::revert))
        .route(&format!("{repo}/contributors"), get(code::contributors))
        .route(&format!("{repo}/graph"), get(insights::graph))
        .route(&format!("{repo}/history"), get(insights::history))
        .route(&format!("{repo}/traffic"), get(insights::traffic))
        .route(&format!("{repo}/traffic/view"), post(insights::view))
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
        .route(&format!("{repo}/milestones"), get(milestones::list).post(milestones::create))
        .route(
            &format!("{repo}/milestones/{{n}}"),
            get(milestones::show).patch(milestones::update).delete(milestones::delete),
        )
        .route(&format!("{repo}/assignees"), get(milestones::assignable))
        .route(&format!("{repo}/issue-templates"), get(milestones::templates))
        .route(&format!("{repo}/pulls"), post(issues::create_pull))
        .route(&format!("{repo}/pulls/{{n}}"), patch(issues::update_pull))
        .route(&format!("{repo}/pulls/{{n}}/diff"), get(issues::pull_diff))
        .route(&format!("{repo}/pulls/{{n}}/reviews"), get(reviews::list).post(reviews::submit))
        .route(&format!("{repo}/pulls/{{n}}/reviews/pending"), delete(reviews::discard))
        .route(&format!("{repo}/pulls/{{n}}/comments"), post(reviews::comment))
        .route(&format!("{repo}/pulls/{{n}}/comments/{{id}}"), patch(reviews::edit).delete(reviews::delete))
        .route(&format!("{repo}/pulls/{{n}}/comments/{{id}}/resolve"), post(reviews::resolve))
        .route(&format!("{repo}/pulls/{{n}}/comments/{{id}}/apply"), post(reviews::apply))
        .route(
            &format!("{repo}/pulls/{{n}}/merge"),
            post(issues::merge_pull),
        )
        .route(&format!("{repo}/collaborators"), get(orgs::collaborators))
        .route(&format!("{repo}/collaborators/{{user}}"), put(orgs::set_collaborator).delete(orgs::remove_collaborator))
        .route(&format!("{repo}/ci/runs"), get(ci::runs).post(ci::start))
        .route(&format!("{repo}/ci/runs/{{n}}"), get(ci::run))
        .route(&format!("{repo}/ci/runs/{{n}}/cancel"), post(ci::cancel))
        .route(&format!("{repo}/ci/runs/{{n}}/rerun"), post(ci::rerun))
        .route(&format!("{repo}/ci/runs/{{n}}/approve"), post(ci::approve))
        .route(&format!("{repo}/ci/badge.svg"), get(checks::badge))
        .route(&format!("{repo}/branch-rules"), get(checks::rules).post(checks::create_rule))
        .route(&format!("{repo}/branch-rules/{{id}}"), put(checks::update_rule).delete(checks::delete_rule))
        .route(&format!("{repo}/statuses/{{sha}}"), post(checks::post_status))
        .route(&format!("{repo}/checks/{{sha}}"), get(checks::checks))
        .route(&format!("{repo}/pulls/{{n}}/auto-merge"), put(checks::set_auto_merge).delete(checks::clear_auto_merge))
        .route(&format!("{repo}/pulls/{{n}}/queue"), post(crate::mergequeue::enqueue).delete(crate::mergequeue::dequeue))
        .route(&format!("{repo}/merge-queue"), get(crate::mergequeue::list))
        .route(&format!("{repo}/ci/jobs/{{id}}/log"), get(ci::log))
        .route(&format!("{repo}/ci/jobs/{{id}}/log/stream"), get(ci::log_stream))
        .route(&format!("{repo}/ci/jobs/{{id}}/artifacts"), get(ci::artifacts))
        .route(&format!("{repo}/ci/status"), get(ci::statuses))
        .route(&format!("{repo}/ci/runs/{{n}}/tests"), get(ci_extras::run_tests))
        .route(&format!("{repo}/ci/report"), get(ci_extras::report))
        .route(&format!("{repo}/ci/flaky"), get(ci_extras::flaky))
        .route(&format!("{repo}/ci/secrets"), get(ci::secrets))
        .route(&format!("{repo}/ci/secrets/{{name}}"), put(ci::set_secret).delete(ci::delete_secret))
        .route(&format!("{repo}/hooks"), get(webhooks::list).post(webhooks::create))
        .route(
            &format!("{repo}/hooks/{{id}}"),
            get(webhooks::show).patch(webhooks::update).delete(webhooks::remove),
        )
        .route(&format!("{repo}/hooks/{{id}}/ping"), post(webhooks::ping))
        .route(&format!("{repo}/hooks/{{id}}/deliveries/{{did}}"), get(webhooks::delivery))
        .route(&format!("{repo}/hooks/{{id}}/deliveries/{{did}}/redeliver"), post(webhooks::redeliver))
        .route(&format!("{repo}/mirrors"), get(mirrors::list).post(mirrors::create))
        .route(&format!("{repo}/mirrors/{{id}}"), patch(mirrors::update).delete(mirrors::remove))
        .route(&format!("{repo}/mirrors/{{id}}/sync"), post(mirrors::sync))
        .route(&format!("{repo}/releases"), get(releases::list).post(releases::create))
        .route(&format!("{repo}/releases/latest"), get(releases::latest))
        .route(
            &format!("{repo}/releases/{{id}}"),
            get(releases::show).patch(releases::update).delete(releases::delete),
        )
        // Release files stream in (up to the Git LFS file limit), past the API's 40 MB cap.
        .route(&format!("{repo}/releases/{{id}}/assets"), post(releases::upload).layer(DefaultBodyLimit::disable()))
        .route(
            &format!("{repo}/releases/{{id}}/assets/{{asset}}"),
            get(releases::download).delete(releases::delete_asset),
        )
        .route(&format!("{repo}/security"), get(scanning::overview))
        .route(&format!("{repo}/security/scan"), post(scanning::scan))
        .route(&format!("{repo}/security/bypasses"), post(scanning::allow))
        .route(&format!("{repo}/dependencies"), get(scanning::dependencies))
        .route(&format!("{repo}/sbom"), get(scanning::sbom))
        // osv.dev's per-ecosystem zips are large.
        .route("/admin/osv", get(scanning::osv_status).post(scanning::osv_import).layer(DefaultBodyLimit::max(512 << 20)))
        .route(&format!("{repo}/packages"), get(crate::registry::list))
        .route(&format!("{repo}/packages/images"), delete(crate::registry::image_delete))
        .route(
            &format!("{repo}/packages/generic/{{pkg}}/{{version}}/{{file}}"),
            get(crate::registry::generic_get)
                .put(crate::registry::generic_put)
                .delete(crate::registry::generic_delete)
                .layer(DefaultBodyLimit::disable()),
        )
        .route(&format!("{repo}/wiki"), get(wiki::pages).post(wiki::create))
        .route(
            &format!("{repo}/wiki/{{page}}"),
            get(wiki::show).put(wiki::update).delete(wiki::delete),
        )
        .route(&format!("{repo}/wiki/{{page}}/history"), get(wiki::history))
        .route(
            &format!("{repo}/discussions"),
            get(discussions::list).post(discussions::create),
        )
        .route(
            &format!("{repo}/discussions/{{n}}"),
            get(discussions::show).patch(discussions::update).delete(discussions::delete),
        )
        .route(&format!("{repo}/discussions/{{n}}/comments"), post(discussions::comment))
        .route(
            &format!("{repo}/discussions/{{n}}/comments/{{id}}"),
            patch(discussions::edit_comment).delete(discussions::delete_comment),
        )
        .route(&format!("{repo}/discussions/{{n}}/answer"), post(discussions::answer))
        .route(&format!("{repo}/discussions/{{n}}/vote"), post(discussions::vote))
        .route("/snippets", get(snippets::list).post(snippets::create))
        .route(
            "/snippets/{id}",
            get(snippets::show).put(snippets::update).delete(snippets::delete),
        )
        .route("/snippets/{id}/revisions", get(snippets::revisions))
        .route("/notifications", get(notifications::list))
        .route("/notifications/count", get(notifications::count))
        .route("/notifications/read", post(notifications::read_all))
        .route("/notifications/{id}/read", post(notifications::read))
        .route("/notifications/{id}/done", post(notifications::done))
        .route("/notifications/{id}/unsubscribe", post(notifications::unsubscribe))
        .route(&format!("{repo}/subscription"), get(notifications::watch).put(notifications::set_watch))
        .route(
            &format!("{repo}/issues/{{n}}/subscription"),
            get(notifications::subscription::<false>).put(notifications::set_subscription::<false>),
        )
        .route(
            &format!("{repo}/discussions/{{n}}/subscription"),
            get(notifications::subscription::<true>).put(notifications::set_subscription::<true>),
        )
        .fallback(|| async { AppError::not_found() })
}

/// The API with its guards (body cap, token limits, rate limits, cross-site
/// check), as mounted at /api and as MCP tool calls go through it.
pub fn guarded(st: &AppState) -> Router<AppState> {
    router()
        .layer(DefaultBodyLimit::max(40 * 1024 * 1024))
        .layer(axum::middleware::from_fn_with_state(st.clone(), scope_guard))
        .layer(axum::middleware::from_fn_with_state(st.clone(), rate_limit))
        .layer(axum::middleware::from_fn_with_state(st.clone(), crate::web::csrf))
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
        let msg = match l.agent {
            Some(_) => "Agent tokens can't do that".to_string(),
            None => format!("This token can't do that (it has {} access{one})", l.scope.as_str()),
        };
        return AppError::new(axum::http::StatusCode::FORBIDDEN, msg).into_response();
    }
    next.run(axum::extract::Request::from_parts(parts, body)).await
}

/// A deploy token's writes must target its own repository: issues, comments
/// and stars elsewhere would be made as its owner.
async fn in_its_repo(st: &AppState, l: &crate::auth::Limits, parts: &Parts) -> bool {
    if l.repo_id.is_none() && l.agent.is_none() {
        return true;
    }
    if matches!(parts.method, axum::http::Method::GET | axum::http::Method::HEAD) {
        return true;
    }
    let segs: Vec<String> = parts.uri.path().trim_matches('/').split('/').map(String::from).collect();
    let (Some(o), Some(n)) = (segs.get(1).cloned(), segs.get(2).cloned()) else { return false };
    // Through the same lookup as `Repo::load`, so a write to a renamed repo's old path gets its redirect.
    let found = st.db.call(move |c| Ok(crate::repo::lookup(c, &o, &n)?)).await.unwrap_or(None);
    found.is_some_and(|(r, _)| l.covers(r.id))
}

/// Account-level routes no limited token may touch, even to read.
const ACCOUNT_ROUTES: &[&str] = &["/admin", "/me/tokens", "/me/security", "/me/audit", "/me/2fa", "/me/sessions", "/me/password", "/me/identities", "/imports", "/me/export", "/me/trash", "/me/transfers", "/auth"];

fn token_may(l: &crate::auth::Limits, method: &axum::http::Method, path: &str) -> bool {
    if ACCOUNT_ROUTES.iter().any(|p| path == *p || path.starts_with(&format!("{p}/"))) {
        return false;
    }
    let segs: Vec<&str> = path.trim_matches('/').split('/').collect();
    if l.agent.is_some() {
        return agent_may(method, &segs);
    }
    // Webhooks and mirrors are the repository's configuration, and hold credentials.
    if segs.len() >= 4 && segs[0] == "repos" && matches!(segs[3], "hooks" | "mirrors") {
        return false;
    }
    if matches!(*method, axum::http::Method::GET | axum::http::Method::HEAD) {
        return true;
    }
    // Writes: only inside a repository (/repos/{owner}/{repo}/{thing}...), never forking it,
    // and never granting access (deploy tokens, collaborators) or giving the repository away:
    // a leaked token mustn't leave access behind once it's revoked.
    l.scope != crate::auth::Scope::Read
        && segs.len() >= 4
        && segs[0] == "repos"
        && !matches!(segs[3], "fork" | "deploy-tokens" | "transfer" | "collaborators")
}

/// POST /repos/{owner}/{repo}/traffic/view: counting a page view, which browsing mustn't
/// pay for out of the write allowance. Exactly that route, nothing that merely ends like it.
fn is_view_count(method: &axum::http::Method, path: &str) -> bool {
    let segs: Vec<&str> = path.trim_matches('/').split('/').collect();
    *method == axum::http::Method::POST && segs.len() == 5 && segs[0] == "repos" && segs[3] == "traffic" && segs[4] == "view"
}

/// What an agent token's MCP tool calls may reach: inside a repository only,
/// reading code, issues, pull requests and CI, and writing issues, comments,
/// pull requests and commits. Never settings, collaborators, hooks, secrets,
/// tokens or merges (an allow-list, so new areas stay closed to agents).
fn agent_may(method: &axum::http::Method, segs: &[&str]) -> bool {
    if segs.len() < 3 || segs[0] != "repos" {
        return false;
    }
    let area = segs.get(3).copied().unwrap_or("");
    if matches!(*method, axum::http::Method::GET | axum::http::Method::HEAD) {
        // Exactly what the tools read (`mcp::request`): not drafts, artifacts or settings.
        return matches!(area, "tree" | "blob" | "grep" | "issues")
            || (area == "ci" && (segs.get(4) == Some(&"runs") || (segs.get(4) == Some(&"jobs") && segs.len() == 7 && segs[6] == "log")));
    }
    *method == axum::http::Method::POST
        && match area {
            "issues" | "contents" => true,
            "pulls" => segs.len() == 4,
            _ => false,
        }
}

/// Per-address limits on all API requests, and per-account limits on changes
/// (admins exempt), so one client can't flood a public instance.
pub async fn rate_limit(
    axum::extract::State(st): axum::extract::State<AppState>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let s = st.settings.get();
    let (mut parts, body) = req.into_parts();
    let ip = crate::auth::ip_bucket(&client_ip(&st, &parts.headers, &parts.extensions));
    if s.api_rate_per_min > 0
        && let Err(wait) = st.limiter.check(&format!("api:{ip}"), s.api_rate_per_min, 60)
    {
        return AppError::slow_down("Too many requests from your network. Slow down a little.", wait).into_response();
    }
    // A page-view count isn't a change: browsing mustn't use up the write allowance.
    let mutating = !matches!(parts.method, axum::http::Method::GET | axum::http::Method::HEAD | axum::http::Method::OPTIONS)
        && !is_view_count(&parts.method, parts.uri.path());
    if mutating && s.write_rate_per_min > 0 {
        let v = crate::auth::Viewer::from_request_parts(&mut parts, &st).await.ok().and_then(|v| v.0);
        let key = match &v {
            // An agent's changes count apart from its owner's (and admins' agents are limited too).
            Some(u) if u.agent().is_some() => u.agent().map(|a| format!("write-agent:{}", a.token_id)),
            Some(u) if u.is_admin => None,
            Some(u) => Some(format!("write:{}", u.id)),
            None => Some(format!("write-ip:{ip}")),
        };
        if let Some(k) = key
            && let Err(wait) = st.limiter.check(&k, s.write_rate_per_min, 60)
        {
            return AppError::slow_down("You're making changes very quickly. Wait a moment and try again.", wait).into_response();
        }
    }
    next.run(axum::extract::Request::from_parts(parts, body)).await
}

/// The client's address: the socket's, or behind a trusted proxy the last
/// X-Forwarded-For hop (added by our own proxy; earlier hops are client-controlled).
pub fn client_ip(st: &AppState, headers: &axum::http::HeaderMap, ext: &axum::http::Extensions) -> String {
    if st.cfg.trust_proxy
        && let Some(ip) = headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.rsplit(',').next())
    {
        return ip.trim().to_string();
    }
    ext.get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip().to_string())
        .unwrap_or_else(|| "unknown".into())
}

/// The client's IP: the socket address, or the last X-Forwarded-For hop (added by
/// our own proxy; earlier hops are client-controlled) when behind a trusted proxy.
pub struct ClientIp(pub String);

impl FromRequestParts<AppState> for ClientIp {
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, st: &AppState) -> R<Self> {
        Ok(ClientIp(client_ip(st, &parts.headers, &parts.extensions)))
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
    (SELECT count(*) FROM repos f WHERE f.forked_from = r.id AND f.deleted_at IS NULL) AS forks,
    r.updated_at,
    (SELECT pu.username || '/' || p.name FROM repos p JOIN users pu ON pu.id = p.owner_id
     WHERE p.id = r.forked_from AND p.is_private = 0 AND p.hidden = 0 AND p.deleted_at IS NULL)
    FROM repos r JOIN users u ON u.id = r.owner_id";

/// Repositories `r` the viewer can see (the same rule as `repo::access`, and never
/// while in the trash); takes `visible_params`.
pub const VISIBLE: &str = "(r.deleted_at IS NULL AND ((r.is_private = 0 AND r.hidden = 0) OR r.owner_id = ? OR EXISTS (SELECT 1 FROM users a WHERE a.id = ? AND a.is_admin = 1)
    OR EXISTS (SELECT 1 FROM repo_collaborators rc WHERE rc.repo_id = r.id AND rc.user_id = ? AND (r.hidden = 0 OR rc.role = 'admin'))
    OR EXISTS (SELECT 1 FROM org_members om JOIN orgs o ON o.id = om.org_id WHERE om.org_id = r.owner_id AND om.user_id = ?
        AND (om.role = 'owner' OR (r.hidden = 0 AND o.base_role IN ('read', 'write'))))))";

pub fn visible_params(viewer: Option<&crate::auth::User>) -> [Value; 4] {
    // A deploy token lists repositories as if signed out (it's for one repo only).
    let viewer = viewer.filter(|u| u.limits.as_ref().is_none_or(|l| l.repo_id.is_none() && l.agent.is_none()));
    let id = Value::Integer(viewer.map(|u| u.id).unwrap_or(-1));
    [id.clone(), id.clone(), id.clone(), id]
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Method;

    #[test]
    fn agent_tokens_never_reach_settings_secrets_tokens_or_merges() {
        let a = crate::auth::Agent { token_id: 1, name: "bot".into(), repos: vec![1], tools: vec![] };
        let l = crate::auth::Limits { scope: crate::auth::Scope::Write, repo_id: None, agent: Some(std::sync::Arc::new(a)) };
        for (m, p) in [
            (Method::GET, "/repos/a/b/blob"),
            (Method::GET, "/repos/a/b/issues/3"),
            (Method::GET, "/repos/a/b/ci/runs/1"),
            (Method::GET, "/repos/a/b/ci/jobs/9/log"),
            (Method::POST, "/repos/a/b/issues"),
            (Method::POST, "/repos/a/b/issues/3/comments"),
            (Method::POST, "/repos/a/b/pulls"),
            (Method::POST, "/repos/a/b/contents"),
        ] {
            assert!(token_may(&l, &m, p), "{m} {p}");
        }
        for (m, p) in [
            (Method::GET, "/me"),
            (Method::GET, "/me/tokens"),
            (Method::POST, "/me/tokens"),
            (Method::GET, "/search"),
            (Method::GET, "/repos/a/b"),
            (Method::PATCH, "/repos/a/b"),
            (Method::DELETE, "/repos/a/b"),
            (Method::GET, "/repos/a/b/ci/secrets"),
            (Method::PUT, "/repos/a/b/ci/secrets/X"),
            (Method::POST, "/repos/a/b/ci/runs"),
            (Method::GET, "/repos/a/b/collaborators"),
            (Method::GET, "/repos/a/b/hooks"),
            (Method::GET, "/repos/a/b/deploy-tokens"),
            (Method::GET, "/repos/a/b/transfer"),
            (Method::POST, "/repos/a/b/pulls/1/merge"),
            (Method::PATCH, "/repos/a/b/issues/1"),
            (Method::DELETE, "/repos/a/b/branches"),
            (Method::POST, "/repos/a/b/branches"),
            (Method::POST, "/repos/a/b/fork"),
            (Method::POST, "/repos/a/b/releases"),
            (Method::GET, "/repos/a/b/wiki"),
            (Method::GET, "/repos/a/b/milestones"),
            (Method::POST, "/repos/a/b/milestones"),
            (Method::PATCH, "/repos/a/b/milestones/1"),
            (Method::GET, "/repos/a/b/assignees"),
        ] {
            assert!(!token_may(&l, &m, p), "{m} {p}");
        }
    }
}
