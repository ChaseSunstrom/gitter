//! An MCP (Model Context Protocol) server for AI agents: JSON-RPC 2.0 over the
//! streamable HTTP transport at `POST /mcp`, answered with plain JSON (no SSE
//! stream). Only agent tokens get in. Each tool call is an ordinary API request
//! made as the token's user through the same guards as /api (`api::guarded`),
//! so the access rules stay in one place: `repo::access` keeps the agent to its
//! repositories, and `scope_guard` (`agent_may`) to code, issues, pull requests,
//! CI and commits, never settings, secrets, tokens or merges.

use std::net::SocketAddr;

use axum::body::Body;
use axum::extract::{ConnectInfo, Request, State};
use axum::http::{Method, StatusCode, header, request::Parts};
use axum::response::{IntoResponse, Response};
use futures_util::StreamExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::AppState;
use crate::auth::{ACTING_AGENT, Agent, User};
use crate::repo::Repo;

pub const PROTOCOL: &str = "2025-06-18";
const SUPPORTED: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];
/// JSON-RPC messages per minute per agent token (apart from its owner's limits).
const CALLS_PER_MIN: u32 = 120;
/// The most of one reply handed back to the agent.
const MAX_TEXT: usize = 256 * 1024;
/// The most repositories one agent token may be given.
pub const MAX_REPOS: usize = 20;

pub struct Tool {
    pub name: &'static str,
    pub title: &'static str,
    description: &'static str,
    write: bool,
    /// (name, JSON type, required, description)
    params: &'static [(&'static str, &'static str, bool, &'static str)],
}

type Param = (&'static str, &'static str, bool, &'static str);
const REPO: Param = ("repo", "string", true, "The repository, as owner/name");
const REF: Param = ("ref", "string", false, "Branch, tag or commit (default: the default branch)");

/// Every tool but `list_repos` (always there) is chosen per token.
pub const TOOLS: &[Tool] = &[
    Tool { name: "list_repos", title: "List repositories", description: "The repositories this token may use.", write: false, params: &[] },
    Tool { name: "search_code", title: "Search code", description: "Search a repository's files for text.", write: false, params: &[REPO, ("query", "string", true, "Text to find"), REF] },
    Tool { name: "read_file", title: "Read files", description: "Read one file.", write: false, params: &[REPO, ("path", "string", true, "File path"), REF] },
    Tool { name: "list_tree", title: "List folders", description: "List a folder's files and subfolders.", write: false, params: &[REPO, ("path", "string", false, "Folder path (default: the top)"), REF] },
    Tool {
        name: "list_issues",
        title: "List issues and pull requests",
        description: "List issues, or pull requests with pulls: true (30 a page).",
        write: false,
        params: &[REPO, ("pulls", "boolean", false, "List pull requests instead of issues"), ("state", "string", false, "open (default), closed or all"), ("query", "string", false, "Text in the title or body"), ("page", "integer", false, "Page, from 1")],
    },
    Tool { name: "get_issue", title: "Read issues and pull requests", description: "An issue or pull request with its comments (and merge status).", write: false, params: &[REPO, ("number", "integer", true, "Issue or pull request number")] },
    Tool { name: "create_issue", title: "Open issues", description: "Open an issue.", write: true, params: &[REPO, ("title", "string", true, "Title"), ("body", "string", false, "Description (Markdown)")] },
    Tool { name: "comment", title: "Comment", description: "Comment on an issue or pull request.", write: true, params: &[REPO, ("number", "integer", true, "Issue or pull request number"), ("body", "string", true, "Comment (Markdown)")] },
    Tool {
        name: "open_pull_request",
        title: "Open pull requests",
        description: "Open a pull request from one branch into another. Agents can't merge.",
        write: true,
        params: &[REPO, ("head", "string", true, "Branch with the changes"), ("base", "string", true, "Branch to merge into"), ("title", "string", true, "Title"), ("body", "string", false, "Description (Markdown)")],
    },
    Tool { name: "list_ci_runs", title: "List CI runs", description: "Recent CI runs, newest first.", write: false, params: &[REPO, ("branch", "string", false, "Only runs for this branch"), ("page", "integer", false, "Page, from 1")] },
    Tool { name: "get_ci_run", title: "Read CI runs", description: "One CI run with its jobs and their steps.", write: false, params: &[REPO, ("number", "integer", true, "Run number")] },
    Tool { name: "get_ci_log", title: "Read CI logs", description: "A CI job's log as text, from offset (bytes).", write: false, params: &[REPO, ("job_id", "integer", true, "Job id (from get_ci_run)"), ("offset", "integer", false, "Start here (bytes)")] },
    Tool {
        name: "commit_files",
        title: "Commit to branches",
        description: "Commit file changes to a branch, or to a new branch made from it. Never to the default branch: open a pull request instead.",
        write: true,
        params: &[REPO, ("branch", "string", true, "Branch to commit to (or to start new_branch from)"), ("new_branch", "string", false, "Create this branch for the commit"), ("message", "string", true, "Commit message"), ("files", "array", true, "Changes: {path, content} to write, {path, delete: true} to remove; base64: true for binary content")],
    },
];

pub fn tool(name: &str) -> Option<&'static Tool> {
    TOOLS.iter().find(|t| t.name == name)
}

fn allowed(a: &Agent, name: &str) -> bool {
    name == "list_repos" || a.tools.iter().any(|t| t == name)
}

fn schema(t: &Tool) -> Value {
    let mut props = serde_json::Map::new();
    for (name, ty, _, desc) in t.params {
        let mut p = json!({ "type": ty, "description": desc });
        if *ty == "array" {
            p["items"] = json!({ "type": "object", "required": ["path"], "properties": {
                "path": { "type": "string" }, "content": { "type": "string" },
                "base64": { "type": "boolean" }, "delete": { "type": "boolean" } } });
        }
        props.insert(name.to_string(), p);
    }
    let required: Vec<&str> = t.params.iter().filter(|p| p.2).map(|p| p.0).collect();
    json!({ "type": "object", "properties": props, "required": required })
}

/// The JSON-RPC reply for `id`: a result, or an error (code, message).
fn reply(status: StatusCode, id: Value, out: Result<Value, (i64, String)>) -> Response {
    let body = match out {
        Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
        Err((code, msg)) => json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": msg } }),
    };
    (status, axum::Json(body)).into_response()
}

fn refuse(status: StatusCode, msg: &str) -> Response {
    let mut r = reply(status, Value::Null, Err((-32001, msg.to_string())));
    if status == StatusCode::UNAUTHORIZED {
        r.headers_mut().insert(header::WWW_AUTHENTICATE, header::HeaderValue::from_static("Bearer"));
    }
    r
}

/// `GET` (the optional server-to-client stream) and `DELETE` (sessions): not offered.
pub async fn no_stream() -> Response {
    (StatusCode::METHOD_NOT_ALLOWED, [(header::ALLOW, "POST")]).into_response()
}

pub async fn handle(State(st): State<AppState>, req: Request) -> Response {
    let (parts, body) = req.into_parts();
    // Browsers send Origin: only our own pages may call (a DNS-rebinding guard).
    if let Some(o) = parts.headers.get(header::ORIGIN)
        && o.to_str().ok().map(|o| o.trim_end_matches('/')) != Some(st.cfg.url.trim_end_matches('/'))
    {
        return refuse(StatusCode::FORBIDDEN, "Cross-site request blocked");
    }
    let token = parts
        .headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer ").or_else(|| v.strip_prefix("bearer ")))
        .map(str::trim);
    let Some(token) = token else { return refuse(StatusCode::UNAUTHORIZED, "Send an agent token: Authorization: Bearer gtr_…") };
    // Guessing tokens from one address is slowed down, as for passwords.
    let fails = format!("mcp-auth:{}", crate::auth::ip_bucket(&super::client_ip(&st, &parts.headers, &parts.extensions)));
    if st.limiter.blocked(&fails, 20) {
        return refuse(StatusCode::TOO_MANY_REQUESTS, "Too many wrong tokens. Wait a minute.");
    }
    let u = match crate::auth::user_by_token(&st.db, token).await {
        Ok(Some(u)) => u,
        Ok(None) => {
            st.limiter.fail(&fails, 60);
            return refuse(StatusCode::UNAUTHORIZED, "That token doesn't work (wrong, expired or deleted)");
        }
        Err(e) => return e.into_response(),
    };
    let Some(agent) = u.limits.as_ref().and_then(|l| l.agent.clone()) else {
        return refuse(StatusCode::FORBIDDEN, "Use an agent token (Settings → Access tokens → Agent)");
    };
    if let Err(wait) = st.limiter.check(&format!("agent:{}", agent.token_id), CALLS_PER_MIN, 60) {
        let mut r = refuse(StatusCode::TOO_MANY_REQUESTS, "This agent is making calls too quickly. Slow down.");
        if let Ok(v) = header::HeaderValue::from_str(&wait.to_string()) {
            r.headers_mut().insert(header::RETRY_AFTER, v);
        }
        return r;
    }
    let Ok(bytes) = axum::body::to_bytes(body, 4 << 20).await else {
        return refuse(StatusCode::PAYLOAD_TOO_LARGE, "That message is over 4 MB");
    };
    let Ok(msg) = serde_json::from_slice::<Value>(&bytes) else {
        return reply(StatusCode::BAD_REQUEST, Value::Null, Err((-32700, "Parse error: send one JSON-RPC message".into())));
    };
    let (Some(obj), true) = (msg.as_object(), msg["jsonrpc"] == "2.0") else {
        return reply(StatusCode::BAD_REQUEST, Value::Null, Err((-32600, "Send one JSON-RPC 2.0 message (batches aren't supported)".into())));
    };
    // Notifications and replies to us need no answer.
    let (Some(id), Some(method)) = (obj.get("id").cloned(), obj.get("method").and_then(Value::as_str)) else {
        return StatusCode::ACCEPTED.into_response();
    };
    let params = obj.get("params").cloned().unwrap_or_else(|| json!({}));
    let out = match method {
        "initialize" => {
            let v = params["protocolVersion"].as_str().filter(|v| SUPPORTED.contains(v)).unwrap_or(PROTOCOL);
            Ok(json!({
                "protocolVersion": v,
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": "gitter", "version": env!("CARGO_PKG_VERSION") },
                "instructions": "Gitter, a code host. Repositories are owner/name; list_repos shows the ones this token may use. Changes go on a branch and through a pull request: agents can't commit to default branches or merge.",
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => {
            let tools: Vec<Value> = TOOLS
                .iter()
                .filter(|t| allowed(&agent, t.name))
                .map(|t| json!({ "name": t.name, "title": t.title, "description": t.description,
                    "inputSchema": schema(t), "annotations": { "readOnlyHint": !t.write } }))
                .collect();
            Ok(json!({ "tools": tools }))
        }
        "tools/call" => call(&st, &u, &agent, &parts, &params).await,
        m => Err((-32601, format!("Unknown method {}", m.chars().take(100).collect::<String>()))),
    };
    reply(StatusCode::OK, id, out)
}

type Invalid = (i64, String);

fn bad(msg: impl Into<String>) -> Invalid {
    (-32602, msg.into())
}

fn text_of<'a>(args: &'a Value, k: &str) -> Option<&'a str> {
    args.get(k).and_then(Value::as_str).filter(|s| !s.is_empty())
}

fn need<'a>(args: &'a Value, k: &str) -> Result<&'a str, Invalid> {
    text_of(args, k).ok_or_else(|| bad(format!("{k} is required")))
}

fn int(args: &Value, k: &str) -> Result<Option<i64>, Invalid> {
    match args.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_i64().filter(|n| *n >= 0).map(Some).ok_or_else(|| bad(format!("{k} must be a whole number"))),
    }
}

fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect()
}

fn query(pairs: &[(&str, Option<String>)]) -> String {
    let q: Vec<String> = pairs.iter().filter_map(|(k, v)| v.as_ref().map(|v| format!("{k}={}", enc(v)))).collect();
    if q.is_empty() { String::new() } else { format!("?{}", q.join("&")) }
}

/// The API request a tool call makes: (method, path under /api, JSON body).
fn request(name: &str, base: &str, a: &Value) -> Result<(Method, String, Option<Value>), Invalid> {
    let s = |k: &str| text_of(a, k).map(String::from);
    let num = |k: &str| int(a, k)?.ok_or_else(|| bad(format!("{k} is required")));
    Ok(match name {
        "search_code" => (Method::GET, format!("{base}/grep{}", query(&[("q", Some(need(a, "query")?.into())), ("rev", s("ref"))])), None),
        "read_file" => (Method::GET, format!("{base}/blob{}", query(&[("path", Some(need(a, "path")?.into())), ("rev", s("ref"))])), None),
        "list_tree" => (Method::GET, format!("{base}/tree{}", query(&[("path", s("path")), ("rev", s("ref"))])), None),
        "list_issues" => {
            let kind = if a["pulls"].as_bool() == Some(true) { "pr" } else { "issue" };
            let page = int(a, "page")?.map(|p| p.to_string());
            (Method::GET, format!("{base}/issues{}", query(&[("kind", Some(kind.into())), ("state", s("state")), ("q", s("query")), ("page", page)])), None)
        }
        "get_issue" => (Method::GET, format!("{base}/issues/{}", num("number")?), None),
        "create_issue" => (Method::POST, format!("{base}/issues"), Some(json!({ "title": need(a, "title")?, "body": s("body").unwrap_or_default() }))),
        "comment" => (Method::POST, format!("{base}/issues/{}/comments", num("number")?), Some(json!({ "body": need(a, "body")? }))),
        "open_pull_request" => (
            Method::POST,
            format!("{base}/pulls"),
            Some(json!({ "head": need(a, "head")?, "base": need(a, "base")?, "title": need(a, "title")?, "body": s("body").unwrap_or_default() })),
        ),
        "list_ci_runs" => (Method::GET, format!("{base}/ci/runs{}", query(&[("branch", s("branch")), ("page", int(a, "page")?.map(|p| p.to_string()))])), None),
        "get_ci_run" => (Method::GET, format!("{base}/ci/runs/{}", num("number")?), None),
        "get_ci_log" => (Method::GET, format!("{base}/ci/jobs/{}/log{}", num("job_id")?, query(&[("offset", int(a, "offset")?.map(|o| o.to_string()))])), None),
        "commit_files" => {
            let files = a.get("files").filter(|f| f.as_array().is_some_and(|f| !f.is_empty())).ok_or_else(|| bad("files is required"))?;
            let mut body = json!({ "branch": need(a, "branch")?, "message": need(a, "message")?, "changes": files });
            if let Some(nb) = s("new_branch") {
                body["new_branch"] = json!(nb);
            }
            (Method::POST, format!("{base}/contents"), Some(body))
        }
        _ => return Err(bad(format!("Unknown tool {name}"))),
    })
}

async fn call(st: &AppState, u: &User, agent: &Agent, parts: &Parts, params: &Value) -> Result<Value, Invalid> {
    let name = params["name"].as_str().ok_or_else(|| bad("Name the tool to call"))?;
    let t = tool(name).ok_or_else(|| bad(format!("Unknown tool {}", name.chars().take(100).collect::<String>())))?;
    if !allowed(agent, name) {
        return Err(bad(format!("This agent token can't use {name}")));
    }
    let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
    if !args.is_object() {
        return Err(bad("arguments must be an object"));
    }
    if name == "list_repos" {
        return Ok(result(false, list_repos(st, u, agent).await.to_string()));
    }
    let full = need(&args, "repo")?;
    let Some((o, n)) = full.split_once('/').filter(|(o, n)| crate::validate::name(o, 39) && crate::validate::name(n, 100)) else {
        return Err(bad("repo must be owner/name"));
    };
    let (method, path, body) = request(name, &format!("/repos/{o}/{n}"), &args)?;
    let (status, text) = ACTING_AGENT.scope(agent.name.clone(), through_api(st, u, parts, method, &path, body)).await;
    if !status.is_success() {
        let msg = serde_json::from_str::<Value>(&text).ok().and_then(|v| v["error"].as_str().map(String::from));
        return Ok(result(true, msg.unwrap_or_else(|| format!("Failed ({status})"))));
    }
    if t.write {
        let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        let mut detail = format!("{}: {name} {o}/{n}", agent.name);
        if let Some(num) = v["number"].as_i64() {
            detail.push_str(&format!(" #{num}"));
        } else if let Some(b) = v["branch"].as_str() {
            detail.push_str(&format!(" on {b}"));
        }
        let ip = super::client_ip(st, &parts.headers, &parts.extensions);
        let e = crate::security::Event { actor: Some(u), subject: Some(u.id), action: "agent_action", detail, ip: &ip };
        crate::security::record(&st.db, e).await;
    }
    let mut text = text;
    if name == "get_ci_log" && text.len() >= MAX_TEXT {
        let next = int(&args, "offset")?.unwrap_or(0) + MAX_TEXT as i64;
        text.push_str(&format!("\n[more: call again with offset {next}]"));
    }
    Ok(result(false, text))
}

fn result(is_error: bool, text: String) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": is_error })
}

/// One request to the API as `u`, through its guards; the status and (up to
/// `MAX_TEXT` bytes of) the body.
async fn through_api(st: &AppState, u: &User, parts: &Parts, method: Method, path: &str, body: Option<Value>) -> (StatusCode, String) {
    let mut b = Request::builder().method(method).uri(path).header(header::CONTENT_TYPE, "application/json");
    // Rate limits key on the caller's address, as for its own requests.
    if let Some(v) = parts.headers.get("x-forwarded-for") {
        b = b.header("x-forwarded-for", v);
    }
    let Ok(mut req) = b.body(body.map(|v| Body::from(v.to_string())).unwrap_or_else(Body::empty)) else {
        return (StatusCode::BAD_REQUEST, json!({ "error": "That isn't a valid request" }).to_string());
    };
    if let Some(c) = parts.extensions.get::<ConnectInfo<SocketAddr>>() {
        req.extensions_mut().insert(*c);
    }
    crate::auth::act_as(req.extensions_mut(), u.clone());
    // ponytail: builds the API router per call (well under a millisecond); keep one in AppState if calls get hot.
    let resp = match super::guarded(st).with_state(st.clone()).oneshot(req).await {
        Ok(r) => r,
        Err(never) => match never {},
    };
    let status = resp.status();
    let mut stream = resp.into_body().into_data_stream();
    let mut buf = Vec::new();
    while let Some(Ok(chunk)) = stream.next().await {
        buf.extend_from_slice(&chunk);
        if buf.len() >= MAX_TEXT {
            buf.truncate(MAX_TEXT);
            break;
        }
    }
    (status, String::from_utf8_lossy(&buf).into_owned())
}

/// The token's repositories that its user can still read.
async fn list_repos(st: &AppState, u: &User, agent: &Agent) -> Value {
    let ids = agent.repos.clone();
    let names: Vec<(String, String)> = st
        .db
        .call(move |c| {
            let mut out = Vec::new();
            for id in ids {
                let row = c.query_row(
                    "SELECT u.username, r.name FROM repos r JOIN users u ON u.id = r.owner_id WHERE r.id = ?1 AND r.deleted_at IS NULL",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                );
                if let Ok(x) = row {
                    out.push(x);
                }
            }
            Ok(out)
        })
        .await
        .unwrap_or_default();
    let mut out = Vec::new();
    for (o, n) in names {
        if let Ok(r) = Repo::readable(st, Some(u), &o, &n).await {
            out.push(json!({ "repo": r.full_name(), "private": r.private, "default_branch": r.default_branch, "can_write": r.can_write }));
        }
    }
    json!({ "repos": out })
}
