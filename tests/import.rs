//! Bulk import from GitHub and GitLab, against mock APIs whose git side is
//! another Gitter (the "source"), so the clones are real.
mod common;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use common::{App, Client};
use serde_json::{Value, json};

async fn serve(r: axum::Router) -> String {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    tokio::spawn(async move {
        let _ = axum::serve(l, r.into_make_service_with_connect_info::<std::net::SocketAddr>()).await;
    });
    url
}

#[derive(Default)]
struct Mock {
    base: String,
    token: String,
    main_sha: String,
    feature_sha: String,
    /// Every API path and query asked for, in order.
    calls: Vec<String>,
    /// The next this many API calls answer with a spent rate limit.
    limited: u32,
    /// The second page of labels is linked on another host.
    evil_next: bool,
    /// Every list links to itself as its next page.
    loop_next: bool,
    /// The next this many API calls answer 502.
    flaky: u32,
    /// The source repository's default branch (main when empty).
    default: String,
    /// Issue #4 (with a comment) exists at the source.
    extra: bool,
}

type M = Arc<Mutex<Mock>>;

fn page(q: &HashMap<String, String>) -> u32 {
    q.get("page").and_then(|p| p.parse().ok()).unwrap_or(1)
}

#[allow(clippy::result_large_err)]
fn check(m: &M, h: &HeaderMap, rest: &str, q: &HashMap<String, String>) -> Result<(), Response> {
    let mut g = m.lock().unwrap();
    let mut qs: Vec<String> = q.iter().map(|(k, v)| format!("{k}={v}")).collect();
    qs.sort();
    g.calls.push(format!("{rest}?{}", qs.join("&")));
    if h.get("authorization").and_then(|v| v.to_str().ok()) != Some(&format!("Bearer {}", g.token)) {
        return Err((StatusCode::UNAUTHORIZED, Json(json!({ "message": "Bad credentials" }))).into_response());
    }
    if g.flaky > 0 {
        g.flaky -= 1;
        return Err((StatusCode::BAD_GATEWAY, Json(json!({ "message": "Server Error" }))).into_response());
    }
    if g.limited > 0 {
        g.limited -= 1;
        let reset = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + 60).to_string();
        return Err((
            StatusCode::FORBIDDEN,
            [("x-ratelimit-remaining", "0".to_string()), ("x-ratelimit-reset", reset.clone()), ("ratelimit-reset", reset)],
            Json(json!({ "message": "API rate limit exceeded" })),
        )
            .into_response());
    }
    Ok(())
}

fn reply(body: Value, next: Option<String>) -> Response {
    match next {
        Some(n) => ([("link", format!("<{n}>; rel=\"next\", <{n}>; rel=\"last\""))], Json(body)).into_response(),
        None => Json(body).into_response(),
    }
}

fn user(login: &str) -> Value {
    json!({ "login": login, "username": login })
}

async fn gh(State(m): State<M>, h: HeaderMap, Path(rest): Path<String>, Query(q): Query<HashMap<String, String>>) -> Response {
    if let Err(r) = check(&m, &h, &rest, &q) {
        return r;
    }
    let g = m.lock().unwrap();
    let api = format!("{}/api/v3", g.base);
    let default = if g.default.is_empty() { "main".to_string() } else { g.default.clone() };
    let repo = json!({ "full_name": "octo/hello", "name": "hello", "private": false, "description": "Hi there", "default_branch": default });
    if g.loop_next && rest != "user" {
        let here = format!("{api}/{rest}?per_page=100&page=1");
        return reply(json!([]), Some(here));
    }
    let mut issues = json!([
        { "id": 21, "number": 1, "title": "Crash", "body": "It crashes, @alice", "state": "closed", "user": user("octo"), "labels": [{ "name": "bug" }],
          "milestone": { "number": 1 }, "created_at": "2020-01-02T00:00:00Z", "closed_at": "2020-01-03T00:00:00Z" },
        { "id": 22, "number": 2, "title": "Add feature", "body": "Adds it", "state": "closed", "user": user("octo"), "labels": [],
          "milestone": null, "pull_request": { "url": "x" }, "created_at": "2020-01-03T00:00:00Z", "closed_at": "2020-01-04T00:00:00Z" },
        { "id": 23, "number": 3, "title": "Docs", "body": null, "state": "open", "user": user("bob"), "labels": [], "milestone": null,
          "created_at": "2099-01-05T00:00:00Z", "closed_at": null },
    ]);
    let mut comments = json!([
        { "id": 101, "issue_url": format!("{api}/repos/octo/hello/issues/1"), "body": "Looking", "user": user("bob"), "created_at": "2020-01-02T01:00:00Z" },
        { "id": 102, "issue_url": format!("{api}/repos/octo/hello/issues/2"), "body": "LGTM", "user": user("bob"), "created_at": "2020-01-03T01:00:00Z" },
    ]);
    if g.extra {
        issues.as_array_mut().unwrap().push(json!({ "id": 24, "number": 4, "title": "Remote four", "body": "", "state": "open", "user": user("octo"),
            "labels": [], "milestone": null, "created_at": "2020-01-08T00:00:00Z", "closed_at": null }));
        comments.as_array_mut().unwrap().push(json!({ "id": 104, "issue_url": format!("{api}/repos/octo/hello/issues/4"), "body": "On remote four",
            "user": user("bob"), "created_at": "2020-01-08T01:00:00Z" }));
    }
    let body = match rest.as_str() {
        "user" => user("octo"),
        "user/repos" | "users/octo/repos" => json!([repo]),
        "repos/octo/hello/labels" if page(&q) == 1 => {
            let host = if g.evil_next { "http://evil.example".to_string() } else { api.clone() };
            let next = format!("{host}/repos/octo/hello/labels?per_page=100&page=2");
            return reply(json!([{ "name": "bug", "color": "d73a4a" }, { "name": "needs-triage", "color": "ededed" }]), Some(next));
        }
        "repos/octo/hello/labels" => json!([{ "name": "wontfix", "color": "ffffff" }]),
        "repos/octo/hello/milestones" => json!([{ "id": 11, "number": 1, "title": "v1", "description": "First cut", "due_on": "2030-01-01T07:00:00Z",
            "state": "open", "created_at": "2020-01-01T00:00:00Z", "closed_at": null }]),
        "repos/octo/hello/issues" => issues,
        "repos/octo/hello/pulls" => json!([
            { "id": 32, "number": 2, "title": "Add feature", "body": "Adds it", "state": "closed", "user": user("octo"), "labels": [{ "name": "needs-triage" }],
              "milestone": null, "head": { "ref": "feature", "sha": g.feature_sha }, "base": { "ref": "main", "sha": g.main_sha },
              "merged_at": "2020-01-04T00:00:00Z", "merge_commit_sha": null, "created_at": "2020-01-03T00:00:00Z", "closed_at": "2020-01-04T00:00:00Z" },
        ]),
        "repos/octo/hello/issues/comments" => comments,
        "repos/octo/hello/releases" => json!([
            { "id": 201, "tag_name": "v1.0", "name": "First", "body": "Notes", "draft": false, "prerelease": false,
              "created_at": "2020-01-06T00:00:00Z", "published_at": "2020-01-06T00:00:00Z" },
            // Its tag didn't come with the clone.
            { "id": 202, "tag_name": "v9.9", "name": "Later", "body": "", "draft": false, "prerelease": false,
              "created_at": "2020-01-07T00:00:00Z", "published_at": "2020-01-07T00:00:00Z" },
        ]),
        _ => return (StatusCode::NOT_FOUND, Json(json!({ "message": "Not Found" }))).into_response(),
    };
    reply(body, None)
}

async fn gl(State(m): State<M>, h: HeaderMap, Path(rest): Path<String>, Query(q): Query<HashMap<String, String>>) -> Response {
    if let Err(r) = check(&m, &h, &rest, &q) {
        return r;
    }
    let g = m.lock().unwrap();
    let body = match rest.as_str() {
        "user" => user("octo"),
        "groups/octo/projects" => json!([{ "id": 7, "path_with_namespace": "octo/hello", "path": "hello", "visibility": "private", "description": "Hi there" }]),
        "projects/7/labels" => json!([{ "name": "bug", "color": "#d73a4a" }, { "name": "gl-only", "color": "#123456" }]),
        "projects/7/milestones" => json!([{ "id": 41, "iid": 1, "title": "v1", "description": "", "due_date": "2030-01-01", "state": "active",
            "created_at": "2020-01-01T00:00:00.000Z" }]),
        "projects/7/issues" => json!([
            { "id": 51, "iid": 1, "title": "Crash", "description": "It crashes", "state": "closed", "author": user("octo"), "labels": ["bug"],
              "milestone": { "iid": 1 }, "created_at": "2020-01-02T00:00:00.000Z", "closed_at": "2020-01-03T00:00:00.000Z" },
            { "id": 52, "iid": 2, "title": "Second", "description": null, "state": "opened", "author": user("bob"), "labels": [],
              "milestone": null, "created_at": "2020-01-04T00:00:00.000+02:00", "closed_at": null },
        ]),
        "projects/7/merge_requests" => json!([
            { "id": 61, "iid": 1, "title": "Add feature", "description": "Adds it", "state": "merged", "author": user("octo"), "labels": ["gl-only"],
              "milestone": null, "source_branch": "feature", "target_branch": "main", "sha": g.feature_sha, "merged_at": "2020-01-05T00:00:00.000Z",
              "diff_refs": { "base_sha": "1111111111111111111111111111111111111111" },
              "created_at": "2020-01-04T00:00:00.000Z", "closed_at": null },
        ]),
        "projects/7/issues/1/notes" => json!([
            { "id": 71, "body": "Looking", "author": user("bob"), "created_at": "2020-01-02T01:00:00.000Z", "system": false },
            { "id": 72, "body": "changed the description", "author": user("bob"), "created_at": "2020-01-02T02:00:00.000Z", "system": true },
        ]),
        "projects/7/issues/2/notes" => json!([]),
        "projects/7/merge_requests/1/notes" => json!([{ "id": 73, "body": "LGTM", "author": user("bob"), "created_at": "2020-01-04T01:00:00.000Z", "system": false }]),
        "projects/7/releases" => json!([{ "tag_name": "v1.0", "name": "First", "description": "Notes", "created_at": "2020-01-06T00:00:00.000Z",
            "released_at": "2020-01-06T00:00:00.000Z" }]),
        _ => return (StatusCode::NOT_FOUND, Json(json!({ "message": "404 Not Found" }))).into_response(),
    };
    reply(body, None)
}

struct World {
    dst: App,
    alice: Client,
    m: M,
    token: String,
    _src: App,
}

/// The source: octo/hello with main, a feature branch and tag v1.0, served behind the mock API.
/// The destination: alice (site admin) and bob.
async fn world() -> World {
    let src = App::new();
    let octo = src.signup("octo").await;
    assert_eq!(src.req(&octo, "POST", "/api/repos", Some(json!({ "name": "hello", "readme": true }))).await.0, StatusCode::OK);
    let (_, b) = src.req(&octo, "GET", "/api/repos/octo/hello/commits?rev=main", None).await;
    let main_sha = b["commits"][0]["sha"].as_str().unwrap().to_string();
    assert_eq!(src.req(&octo, "POST", "/api/repos/octo/hello/branches", Some(json!({ "name": "feature", "from": "main" }))).await.0, StatusCode::OK);
    let body = json!({ "branch": "feature", "message": "Add feature", "changes": [{ "path": "f.txt", "content": "f\n" }] });
    let (s, c) = src.req(&octo, "POST", "/api/repos/octo/hello/contents", Some(body)).await;
    assert_eq!(s, StatusCode::OK, "{c}");
    let (_, b) = src.req(&octo, "GET", "/api/repos/octo/hello/commits?rev=feature", None).await;
    let feature_sha = b["commits"][0]["sha"].as_str().unwrap().to_string();
    // A release in the source makes the tag.
    assert_eq!(src.req(&octo, "POST", "/api/repos/octo/hello/releases", Some(json!({ "tag": "v1.0", "target": "main", "title": "src" }))).await.0, StatusCode::OK);
    let (_, t) = src.req(&octo, "POST", "/api/me/tokens", Some(json!({ "name": "import" }))).await;
    let token = t["token"].as_str().unwrap().to_string();

    let m: M = Arc::default();
    let router = axum::Router::new()
        .route("/api/v3/{*rest}", get(gh))
        .route("/api/v4/{*rest}", get(gl))
        .with_state(m.clone())
        .fallback_service(src.router.clone());
    let base = serve(router).await;
    *m.lock().unwrap() = Mock { base, token: token.clone(), main_sha, feature_sha, ..Default::default() };
    let dst = App::new();
    let alice = dst.signup("alice").await;
    dst.signup("bob").await;
    World { dst, alice, m, token, _src: src }
}

/// Puts the (finished) job back to running at `stage`, with its token, as if a step there were lost.
async fn reopen(w: &World, stage: &'static str) {
    let sealed = gitter::security::seal_text(&w.dst.state.key, &w.token);
    w.dst
        .state
        .db
        .call(move |c| {
            c.execute("UPDATE import_items SET stage = ?1, cursor = '', next_at = 0, running_until = 0", [stage])?;
            c.execute("UPDATE import_jobs SET state = 'running', token = ?1", [sealed])?;
            Ok(())
        })
        .await
        .unwrap();
}

/// Runs the import worker until it has nothing due.
async fn drain(app: &App) {
    for _ in 0..500 {
        if !gitter::imports::step(&app.state).await {
            return;
        }
    }
    panic!("the import never settled");
}

async fn one(app: &App, sql: &'static str) -> i64 {
    app.state.db.call(move |c| Ok(c.query_row(sql, [], |r| r.get(0))?)).await.unwrap()
}

async fn text(app: &App, sql: &'static str) -> String {
    app.state.db.call(move |c| Ok(c.query_row(sql, [], |r| r.get(0))?)).await.unwrap()
}

fn job(w: &World, kind: &str, source: &str, repos: &[&str]) -> Value {
    json!({ "kind": kind, "base_url": w.m.lock().unwrap().base, "token": w.token, "source": source, "owner": "alice", "repos": repos, "allow_private": true })
}

async fn start(w: &World, body: Value) -> Value {
    let (s, v) = w.dst.req(&w.alice, "POST", "/api/imports", Some(body)).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    v
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn github_import_keeps_numbers_and_a_second_run_adds_nothing() {
    let w = world().await;
    let app = &w.dst;
    let mut preview = job(&w, "github", "octo", &[]);
    preview.as_object_mut().unwrap().remove("repos");
    let (s, list) = app.req(&w.alice, "POST", "/api/imports/preview", Some(preview)).await;
    assert_eq!(s, StatusCode::OK, "{list}");
    assert_eq!(list["repos"][0]["source"], json!("octo/hello"));
    assert!(!list.to_string().contains(&w.token));

    start(&w, job(&w, "github", "octo", &["octo/hello"])).await;
    drain(app).await;
    let (s, jobs) = app.req(&w.alice, "GET", "/api/imports", None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(jobs["jobs"][0]["state"], json!("done"), "{jobs}");
    assert_eq!(jobs["jobs"][0]["items"][0]["stage"], json!("done"), "{jobs}");
    assert!(!jobs.to_string().contains(&w.token));

    // The code, with its branches and tags.
    let (s, r) = app.req(&w.alice, "GET", "/api/repos/alice/hello", None).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["description"], json!("Hi there"));
    assert!(r["branches"].as_array().unwrap().contains(&json!("feature")) && r["tags"].as_array().unwrap().contains(&json!("v1.0")), "{r}");
    // Labels from both pages, the default "bug" not doubled.
    assert_eq!(text(app, "SELECT group_concat(name, ',') FROM (SELECT name FROM labels ORDER BY name) WHERE name IN ('bug','needs-triage','wontfix')").await, "bug,needs-triage,wontfix");
    assert_eq!(one(app, "SELECT count(*) FROM labels WHERE name = 'bug'").await, 1);
    assert!(w.m.lock().unwrap().calls.iter().any(|c| c.starts_with("repos/octo/hello/labels") && c.contains("page=2")));
    assert_eq!(text(app, "SELECT title || '|' || due_on FROM milestones WHERE number = 1").await, "v1|2030-01-01");
    // Issues and the pull request keep their numbers; text names the original author without mentioning anyone.
    assert_eq!(text(app, "SELECT group_concat(number || ':' || is_pr || ':' || state, ',') FROM (SELECT * FROM issues ORDER BY number)").await, "1:0:closed,2:1:closed,3:0:open");
    let body = text(app, "SELECT body FROM issues WHERE number = 1").await;
    assert!(body.contains("`octo`") && body.contains("It crashes"), "{body}");
    assert_eq!(one(app, "SELECT count(*) FROM notifications").await, 0, "imported text notifies nobody");
    assert_eq!(text(app, "SELECT l.name FROM issue_labels il JOIN labels l ON l.id = il.label_id JOIN issues i ON i.id = il.issue_id WHERE i.number = 1").await, "bug");
    assert_eq!(one(app, "SELECT count(*) FROM issues i JOIN milestones m ON m.id = i.milestone_id WHERE i.number = 1").await, 1);
    // A date in the future is clamped to now.
    assert!(one(app, "SELECT created_at FROM issues WHERE number = 3").await <= one(app, "SELECT CAST(strftime('%s','now') AS INTEGER)").await);
    let feature = w.m.lock().unwrap().feature_sha.clone();
    let pr = text(app, "SELECT p.head_branch || '|' || p.base_branch || '|' || coalesce(p.head_sha, '') || '|' || (p.merged_at IS NOT NULL) FROM pulls p JOIN issues i ON i.id = p.issue_id WHERE i.number = 2").await;
    assert_eq!(pr, format!("feature|main|{feature}|1"));
    assert_eq!(text(app, "SELECT l.name FROM issue_labels il JOIN labels l ON l.id = il.label_id JOIN issues i ON i.id = il.issue_id WHERE i.number = 2").await, "needs-triage");
    let cs = text(app, "SELECT group_concat(i.number || '=' || replace(c.body, char(10), ' '), char(10)) FROM comments c JOIN issues i ON i.id = c.issue_id").await;
    assert_eq!(cs.lines().count(), 2, "{cs}");
    assert!(cs.lines().any(|l| l.starts_with("1=") && l.contains("Looking") && l.contains("`bob`")), "{cs}");
    assert!(cs.lines().any(|l| l.starts_with("2=") && l.contains("LGTM")), "{cs}");
    // A release whose tag isn't here can only be a draft.
    assert_eq!(text(app, "SELECT group_concat(tag || '|' || title || '|' || draft, ',') FROM (SELECT * FROM releases ORDER BY tag)").await, "v1.0|First|0,v9.9|Later|1");

    // Work done here since: a branch, and an issue that takes number 4.
    assert_eq!(app.req(&w.alice, "POST", "/api/repos/alice/hello/branches", Some(json!({ "name": "wip", "from": "main" }))).await.0, StatusCode::OK);
    let (s, _) = app.req(&w.alice, "POST", "/api/repos/alice/hello/issues", Some(json!({ "title": "Local four", "body": "" }))).await;
    assert_eq!(s, StatusCode::OK);
    // The same import again finds everything already there.
    let counts = "SELECT (SELECT count(*) FROM issues) || '/' || (SELECT count(*) FROM comments) || '/' || (SELECT count(*) FROM labels) || '/' || (SELECT count(*) FROM milestones) || '/' || (SELECT count(*) FROM releases) || '/' || (SELECT count(*) FROM repos)";
    let before = text(app, counts).await;
    start(&w, job(&w, "github", "octo", &["octo/hello"])).await;
    drain(app).await;
    assert_eq!(text(app, counts).await, before);
    assert_eq!(text(app, "SELECT state FROM import_jobs ORDER BY id DESC LIMIT 1").await, "done");
    let (_, r) = app.req(&w.alice, "GET", "/api/repos/alice/hello", None).await;
    assert!(r["branches"].as_array().unwrap().contains(&json!("wip")), "a run again leaves the code alone: {r}");
    // But a repository that's still empty (its first copy never landed) is copied this time.
    let path = app.state.git.repo_path(one(app, "SELECT id FROM repos WHERE name = 'hello'").await);
    let refs = std::process::Command::new("git").arg("-C").arg(&path).args(["for-each-ref", "--format=delete %(refname)"]).output().unwrap().stdout;
    let mut up = std::process::Command::new("git").arg("-C").arg(&path).args(["update-ref", "--stdin"]).stdin(std::process::Stdio::piped()).spawn().unwrap();
    std::io::Write::write_all(up.stdin.as_mut().unwrap(), &refs).unwrap();
    assert!(up.wait().unwrap().success());
    start(&w, job(&w, "github", "octo", &["octo/hello"])).await;
    drain(app).await;
    let (_, r) = app.req(&w.alice, "GET", "/api/repos/alice/hello", None).await;
    assert!(r["branches"].as_array().unwrap().contains(&json!("main")), "copied again: {r}");

    // The source gains #4 with a comment; here #4 is taken, so it's skipped (and said), and its comment doesn't land on ours.
    w.m.lock().unwrap().extra = true;
    start(&w, job(&w, "github", "octo", &["octo/hello"])).await;
    drain(app).await;
    assert_eq!(text(app, "SELECT title FROM issues WHERE number = 4").await, "Local four");
    assert_eq!(one(app, "SELECT count(*) FROM comments c JOIN issues i ON i.id = c.issue_id WHERE i.number = 4").await, 0);
    let (_, jobs) = app.req(&w.alice, "GET", "/api/imports", None).await;
    let item = &jobs["jobs"][0]["items"][0];
    assert_eq!(item["stage"], json!("done"), "{item}");
    assert!(item["error"].as_str().unwrap().contains("1 skipped"), "{item}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn gitlab_import_numbers_merge_requests_after_the_issues() {
    let w = world().await;
    let app = &w.dst;
    start(&w, job(&w, "gitlab", "octo", &["octo/hello"])).await;
    // Up to the comments, then as if a big project had already used most of the page budget there:
    // the budget is per issue's pages, so moving on to the next issue starts it again.
    for _ in 0..100 {
        if text(app, "SELECT stage FROM import_items").await == "comments" {
            break;
        }
        assert!(gitter::imports::step(&app.state).await);
    }
    app.state.db.call(|c| Ok(c.execute("UPDATE import_items SET pages = 1999", [])?)).await.unwrap();
    drain(app).await;
    let (_, jobs) = app.req(&w.alice, "GET", "/api/imports", None).await;
    assert_eq!(jobs["jobs"][0]["items"][0]["stage"], json!("done"), "{jobs}");
    let (_, r) = app.req(&w.alice, "GET", "/api/repos/alice/hello", None).await;
    assert_eq!(r["private"], json!(true), "a private project stays private");
    assert_eq!(text(app, "SELECT group_concat(number || ':' || is_pr || ':' || state, ',') FROM (SELECT * FROM issues ORDER BY number)").await, "1:0:closed,2:0:open,3:1:closed");
    assert!(text(app, "SELECT body FROM issues WHERE number = 3").await.contains("!1"), "the merge request's own number is noted");
    assert_eq!(text(app, "SELECT l.name FROM issue_labels il JOIN labels l ON l.id = il.label_id JOIN issues i ON i.id = il.issue_id WHERE i.number = 3").await, "gl-only");
    let feature = w.m.lock().unwrap().feature_sha.clone();
    // A commit the clone didn't bring isn't recorded.
    assert_eq!(text(app, "SELECT head_branch || '|' || coalesce(head_sha, '') || '|' || coalesce(base_sha, 'none') FROM pulls").await, format!("feature|{feature}|none"));
    // System notes stay behind.
    assert_eq!(one(app, "SELECT count(*) FROM comments").await, 2);
    assert_eq!(text(app, "SELECT title || '|' || state FROM milestones").await, "v1|open");
    assert_eq!(text(app, "SELECT tag FROM releases").await, "v1.0");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn rate_limits_reschedule_and_a_lost_cursor_resumes_without_duplicates() {
    let w = world().await;
    let app = &w.dst;
    start(&w, job(&w, "github", "octo", &["octo/hello"])).await;
    // The clone is git, not the API; the first API call after it hits the limit.
    w.m.lock().unwrap().limited = 1;
    drain(app).await;
    assert_eq!(text(app, "SELECT stage FROM import_items").await, "labels", "waits at the stage that was limited");
    assert!(one(app, "SELECT next_at - CAST(strftime('%s','now') AS INTEGER) FROM import_items").await > 30, "waits for the reset");
    assert_eq!(text(app, "SELECT state FROM import_jobs").await, "running");
    app.state.db.call(|c| Ok(c.execute("UPDATE import_items SET next_at = 0", [])?)).await.unwrap();
    drain(app).await;
    assert_eq!(text(app, "SELECT stage FROM import_items").await, "done");
    // A server error waits and tries again instead of failing.
    reopen(&w, "labels").await;
    w.m.lock().unwrap().flaky = 1;
    drain(app).await;
    assert_eq!(text(app, "SELECT stage FROM import_items").await, "labels", "a 502 isn't the end");
    assert!(one(app, "SELECT next_at - CAST(strftime('%s','now') AS INTEGER) FROM import_items").await > 10);
    app.state.db.call(|c| Ok(c.execute("UPDATE import_items SET next_at = 0", [])?)).await.unwrap();
    // A restart while a step held its lease: the lease is released at start.
    app.state.db.call(|c| Ok(c.execute("UPDATE import_items SET running_until = CAST(strftime('%s','now') AS INTEGER) + 900", [])?)).await.unwrap();
    assert!(!gitter::imports::step(&app.state).await, "held by the step that died");
    gitter::imports::recover(&app.state).await;
    drain(app).await;
    assert_eq!(text(app, "SELECT stage FROM import_items").await, "done");
    // As if the worker died after writing a page but before saving where it was.
    let counts = "SELECT (SELECT count(*) FROM issues) || '/' || (SELECT count(*) FROM comments) || '/' || (SELECT count(*) FROM labels) || '/' || (SELECT count(*) FROM issue_labels)";
    let before = text(app, counts).await;
    reopen(&w, "issues").await;
    let calls = w.m.lock().unwrap().calls.len();
    drain(app).await;
    assert_eq!(text(app, "SELECT stage FROM import_items").await, "done");
    assert!(w.m.lock().unwrap().calls.len() > calls + 3, "the pages really were read again");
    assert_eq!(text(app, counts).await, before);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_token_goes_when_the_job_ends_and_never_shows() {
    let w = world().await;
    let app = &w.dst;
    // A next page on another host is refused, and the item fails without the token in its error.
    w.m.lock().unwrap().evil_next = true;
    start(&w, job(&w, "github", "octo", &["octo/hello"])).await;
    assert!(one(app, "SELECT token IS NOT NULL FROM import_jobs").await == 1);
    assert!(!text(app, "SELECT token FROM import_jobs").await.contains(&w.token), "stored sealed");
    drain(app).await;
    let (_, jobs) = app.req(&w.alice, "GET", "/api/imports", None).await;
    let item = &jobs["jobs"][0]["items"][0];
    assert_eq!(item["stage"], json!("failed"), "{jobs}");
    assert!(item["error"].as_str().unwrap().contains("another host"), "{item}");
    assert!(!jobs.to_string().contains(&w.token));
    assert_eq!(one(app, "SELECT token IS NULL FROM import_jobs").await, 1, "wiped at the end");
    assert!(!w.m.lock().unwrap().calls.iter().any(|c| c.contains("evil")), "never asked the other host");

    // Cancelling ends a job at once and wipes its token too.
    w.m.lock().unwrap().evil_next = false;
    let j = start(&w, job(&w, "github", "octo", &["octo/hello", "OCTO/hello"])).await;
    assert_eq!(one(app, "SELECT count(*) FROM import_items WHERE job_id = (SELECT max(id) FROM import_jobs)").await, 1, "the same repository once");
    let bob = {
        let (_, _, c) = app.call(&Client::default(), "POST", "/api/auth/login", Some(json!({ "login": "bob", "password": "correct horse" })), Some(common::ORIGIN)).await;
        Client(c)
    };
    assert_eq!(app.req(&bob, "POST", &format!("/api/imports/{}/cancel", j["id"]), None).await.0, StatusCode::NOT_FOUND, "only its owner (or an admin)");
    // One job at a time per person.
    let (s, _) = app.req(&w.alice, "POST", "/api/imports", Some(job(&w, "github", "octo", &["octo/hello"]))).await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (s, _) = app.req(&w.alice, "POST", &format!("/api/imports/{}/cancel", j["id"]), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(text(app, "SELECT state || '|' || (token IS NULL) FROM import_jobs ORDER BY id DESC LIMIT 1").await, "cancelled|1");
    assert!(!gitter::imports::step(&app.state).await, "nothing left to do");

    // A job nobody moves for a day ends, and its token goes.
    start(&w, job(&w, "github", "octo", &["octo/hello"])).await;
    app.state.db.call(|c| Ok(c.execute("UPDATE import_jobs SET updated_at = 0 WHERE state = 'running'", [])?)).await.unwrap();
    gitter::imports::sweep(&app.state).await;
    assert_eq!(text(app, "SELECT state || '|' || (token IS NULL) FROM import_jobs ORDER BY id DESC LIMIT 1").await, "failed|1");
    // One that keeps going without finishing (a provider that's always rate limiting) ends after a week.
    start(&w, job(&w, "github", "octo", &["octo/hello"])).await;
    app.state.db.call(|c| Ok(c.execute("UPDATE import_jobs SET created_at = 0 WHERE state = 'running'", [])?)).await.unwrap();
    gitter::imports::sweep(&app.state).await;
    assert_eq!(text(app, "SELECT state || '|' || (token IS NULL) FROM import_jobs ORDER BY id DESC LIMIT 1").await, "failed|1");
    // A token the server key can't open fails the item instead of looping.
    start(&w, job(&w, "github", "octo", &["octo/hello"])).await;
    app.state.db.call(|c| Ok(c.execute("UPDATE import_jobs SET token = 'sealed:00' WHERE state = 'running'", [])?)).await.unwrap(); // pragma: allowlist secret
    drain(app).await;
    assert!(text(app, "SELECT error FROM import_items ORDER BY id DESC LIMIT 1").await.contains("can't be read"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_taken_name_stops_and_a_provider_that_pages_forever_is_cut_off() {
    let w = world().await;
    let app = &w.dst;
    assert_eq!(app.req(&w.alice, "POST", "/api/repos", Some(json!({ "name": "hello" }))).await.0, StatusCode::OK);
    start(&w, job(&w, "github", "octo", &["octo/hello"])).await;
    drain(app).await;
    assert!(text(app, "SELECT error FROM import_items").await.contains("already there"));
    assert_eq!(one(app, "SELECT count(*) FROM issues").await, 0);
    // A listing whose pages point back at themselves stops instead of running forever.
    w.m.lock().unwrap().loop_next = true;
    let mut preview = job(&w, "github", "octo", &[]);
    preview.as_object_mut().unwrap().remove("repos");
    let (s, e) = app.req(&w.alice, "POST", "/api/imports/preview", Some(preview)).await;
    assert!(s.is_server_error() && e["error"].as_str().unwrap().contains("same page"), "{s} {e}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_stage_whose_pages_loop_fails_and_the_worker_moves_on() {
    let w = world().await;
    let app = &w.dst;
    start(&w, job(&w, "github", "octo", &["octo/hello"])).await;
    // Through the clone, then every list links to itself.
    for _ in 0..2 {
        assert!(gitter::imports::step(&app.state).await);
    }
    w.m.lock().unwrap().loop_next = true;
    drain(app).await;
    let (_, jobs) = app.req(&w.alice, "GET", "/api/imports", None).await;
    let item = &jobs["jobs"][0]["items"][0];
    assert_eq!(item["stage"], json!("failed"), "{item}");
    assert!(item["error"].as_str().unwrap().contains("same page"), "{item}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn private_addresses_other_peoples_orgs_and_bad_input_are_refused() {
    let w = world().await;
    let app = &w.dst;
    let bob = {
        let (_, _, c) = app.call(&Client::default(), "POST", "/api/auth/login", Some(json!({ "login": "bob", "password": "correct horse" })), Some(common::ORIGIN)).await;
        Client(c)
    };
    let mut b = job(&w, "github", "octo", &["octo/hello"]);
    b["owner"] = json!("bob");
    // Only a site admin may let an import reach private addresses.
    assert_eq!(app.req(&bob, "POST", "/api/imports", Some(b.clone())).await.0, StatusCode::FORBIDDEN);
    b["allow_private"] = json!(false);
    b["base_url"] = json!("https://localhost:9");
    let (s, e) = app.req(&bob, "POST", "/api/imports/preview", Some(b.clone())).await;
    assert!(s.is_client_error() || s.is_server_error(), "{s} {e}");
    assert!(e["error"].as_str().unwrap_or_default().contains("public address"), "{e}");
    // Someone else's account or an org bob doesn't own.
    let (s, _) = app.req(&w.alice, "POST", "/api/orgs", Some(json!({ "name": "acme" }))).await;
    assert_eq!(s, StatusCode::OK);
    for (owner, want) in [("alice", StatusCode::BAD_REQUEST), ("acme", StatusCode::FORBIDDEN)] {
        let mut x = b.clone();
        x["owner"] = json!(owner);
        assert_eq!(app.req(&bob, "POST", "/api/imports", Some(x)).await.0, want, "{owner}");
    }
    for (k, v) in [("kind", json!("bitbucket")), ("base_url", json!("file:///etc")), ("repos", json!([])), ("token", json!(""))] {
        let mut x = job(&w, "github", "octo", &["octo/hello"]);
        x[k] = v;
        assert_eq!(app.req(&w.alice, "POST", "/api/imports", Some(x)).await.0, StatusCode::BAD_REQUEST, "{k}");
    }
    // A repository the source doesn't have.
    assert_eq!(app.req(&w.alice, "POST", "/api/imports", Some(job(&w, "github", "octo", &["octo/nope"]))).await.0, StatusCode::BAD_REQUEST);
    // A wrong token says so without echoing it.
    let mut x = job(&w, "github", "octo", &["octo/hello"]);
    x["token"] = json!("not-the-token-123");
    let (s, e) = app.req(&w.alice, "POST", "/api/imports/preview", Some(x)).await;
    assert!(s.is_client_error() || s.is_server_error());
    assert!(!e.to_string().contains("not-the-token-123"), "{e}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn importing_again_into_an_organization_repository_needs_its_admin() {
    let w = world().await;
    let app = &w.dst;
    assert_eq!(app.req(&w.alice, "POST", "/api/orgs", Some(json!({ "name": "acme" }))).await.0, StatusCode::OK);
    let mut b = job(&w, "github", "octo", &["octo/hello"]);
    b["owner"] = json!("acme");
    start(&w, b).await;
    drain(app).await;
    assert_eq!(text(app, "SELECT stage FROM import_items").await, "done");
    // bob may create repositories in acme (members write), but doesn't administer acme/hello.
    assert_eq!(app.req(&w.alice, "PUT", "/api/orgs/acme/members/bob", Some(json!({ "role": "member" }))).await.0, StatusCode::OK);
    app.state.db.call(|c| Ok(c.execute("UPDATE orgs SET base_role = 'write'", [])?)).await.unwrap();
    // His job as the worker finds it (written directly: only site admins may reach the mock's private address).
    let sealed = gitter::security::seal_text(&app.state.key, &w.token);
    let base = w.m.lock().unwrap().base.clone();
    app.state
        .db
        .call(move |c| {
            c.execute(
                "INSERT INTO import_jobs(user_id, owner_id, kind, base_url, token, allow_private, state, created_at, updated_at)
                 SELECT (SELECT id FROM users WHERE username = 'bob'), (SELECT id FROM users WHERE username = 'acme'), 'github', ?1, ?2, 1, 'running', 0, 0",
                rusqlite::params![base, sealed],
            )?;
            c.execute("INSERT INTO import_items(job_id, source, name) VALUES (last_insert_rowid(), 'octo/hello', 'hello')", [])?;
            Ok(())
        })
        .await
        .unwrap();
    drain(app).await;
    let err = text(app, "SELECT stage || ': ' || error FROM import_items ORDER BY id DESC LIMIT 1").await;
    assert!(err.starts_with("failed") && err.contains("only its admins"), "{err}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_sources_default_branch_stays_the_default() {
    let w = world().await;
    w.m.lock().unwrap().default = "feature".into();
    start(&w, job(&w, "github", "octo", &["octo/hello"])).await;
    drain(&w.dst).await;
    let (_, r) = w.dst.req(&w.alice, "GET", "/api/repos/alice/hello", None).await;
    assert_eq!(r["default_branch"].as_str(), Some("feature"), "{r}");
}
