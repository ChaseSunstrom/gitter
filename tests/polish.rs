//! Polish: reverting from the web, inert raw files for the rich viewers, the
//! PWA manifest, and `gitter demo`.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use common::{App, Client};

async fn commit(app: &App, who: &Client, repo: &str, branch: &str, new_branch: Option<&str>, changes: Value) -> String {
    let mut body = json!({ "branch": branch, "message": "Change", "changes": changes });
    if let Some(nb) = new_branch {
        body["new_branch"] = json!(nb);
    }
    let (s, v) = app.req(who, "POST", &format!("/api/repos/{repo}/contents"), Some(body)).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    v["sha"].as_str().unwrap().to_string()
}

async fn file(app: &App, who: &Client, repo: &str, rev: &str, path: &str) -> Option<String> {
    let (s, v) = app.req(who, "GET", &format!("/api/repos/{repo}/blob?rev={rev}&path={path}"), None).await;
    (s == StatusCode::OK).then(|| v["content"].as_str().unwrap_or("").to_string())
}

async fn get(app: &App, path: &str) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let resp = app.router.clone().oneshot(Request::get(path).header("host", "test.local").body(Body::empty()).unwrap()).await.unwrap();
    let (s, h) = (resp.status(), resp.headers().clone());
    (s, h, resp.into_body().collect().await.unwrap().to_bytes().to_vec())
}

#[tokio::test]
async fn revert_a_commit_opens_a_pull_request_with_the_inverse_change() {
    let app = App::new();
    let alice = app.signup("alice").await;
    app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "r", "readme": true }))).await;
    commit(&app, &alice, "alice/r", "main", None, json!([{ "path": "a.txt", "content": "one\n" }])).await;
    let sha = commit(&app, &alice, "alice/r", "main", None, json!([{ "path": "a.txt", "content": "two\n" }, { "path": "b.txt", "content": "new\n" }])).await;

    let (s, v) = app.req(&alice, "POST", "/api/repos/alice/r/revert", Some(json!({ "sha": sha }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let (n, branch) = (v["number"].as_i64().unwrap(), v["branch"].as_str().unwrap().to_string());
    assert_eq!(branch, format!("revert-{}", &sha[..7]));
    // The branch holds the inverse change; main is untouched until the PR is merged.
    assert_eq!(file(&app, &alice, "alice/r", &branch, "a.txt").await.as_deref(), Some("one\n"));
    assert_eq!(file(&app, &alice, "alice/r", &branch, "b.txt").await, None);
    assert_eq!(file(&app, &alice, "alice/r", "main", "a.txt").await.as_deref(), Some("two\n"));
    let (_, pr) = app.req(&alice, "GET", &format!("/api/repos/alice/r/issues/{n}"), None).await;
    assert_eq!(pr["issue"]["title"], "Revert \"Change\"");
    assert_eq!(pr["pull"]["base"], "main");
    assert_eq!(pr["pull"]["head"], branch);
    assert!(pr["issue"]["body"].as_str().unwrap().contains(&sha), "{pr}");
    let (_, log) = app.req(&alice, "GET", &format!("/api/repos/alice/r/commits?rev={branch}"), None).await;
    assert_eq!(log["commits"][0]["subject"], "Revert \"Change\"");

    // Twice: the revert branch is there already, so no second PR.
    let (s, v) = app.req(&alice, "POST", "/api/repos/alice/r/revert", Some(json!({ "sha": sha }))).await;
    assert_eq!(s, StatusCode::CONFLICT, "{v}");
    assert!(v["error"].as_str().unwrap().contains(&branch), "{v}");

    // A commit already undone on the target has nothing to revert.
    let (s, _) = app.req(&alice, "POST", &format!("/api/repos/alice/r/pulls/{n}/merge"), Some(json!({ "method": "merge", "delete_branch": true }))).await;
    assert_eq!(s, StatusCode::OK);
    let (s, v) = app.req(&alice, "POST", "/api/repos/alice/r/revert", Some(json!({ "sha": sha }))).await;
    assert_eq!(s, StatusCode::CONFLICT, "{v}");
    assert!(v["error"].as_str().unwrap().contains("nothing to revert"), "{v}");

    // The first commit has no parent to go back to; nonsense is a 4xx.
    let (_, log) = app.req(&alice, "GET", "/api/repos/alice/r/commits?rev=main", None).await;
    let root = log["commits"].as_array().unwrap().last().unwrap()["sha"].as_str().unwrap().to_string();
    let (s, _) = app.req(&alice, "POST", "/api/repos/alice/r/revert", Some(json!({ "sha": root }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    for body in [json!({}), json!({ "sha": "nope" }), json!({ "sha": sha, "branch": "no-such" }), json!({ "sha": sha, "pull": 1 })] {
        let (s, _) = app.req(&alice, "POST", "/api/repos/alice/r/revert", Some(body.clone())).await;
        assert!(s.is_client_error(), "{body}: {s}");
    }
}

#[tokio::test]
async fn revert_a_merged_pull_request_for_every_merge_method() {
    let app = App::new();
    let alice = app.signup("alice").await;
    for method in ["merge", "squash", "ff"] {
        let repo = format!("alice/{method}");
        app.req(&alice, "POST", "/api/repos", Some(json!({ "name": method, "readme": true }))).await;
        commit(&app, &alice, &repo, "main", Some("feature"), json!([{ "path": "f.txt", "content": "feature\n" }])).await;
        commit(&app, &alice, &repo, "feature", None, json!([{ "path": "README.md", "content": "changed\n" }])).await;
        let (_, pr) = app.req(&alice, "POST", &format!("/api/repos/{repo}/pulls"), Some(json!({ "title": "Add feature", "head": "feature", "base": "main" }))).await;
        let n = pr["number"].as_i64().unwrap();
        let readme = file(&app, &alice, &repo, "main", "README.md").await;

        // Only a merged PR can be reverted.
        let (s, _) = app.req(&alice, "POST", &format!("/api/repos/{repo}/revert"), Some(json!({ "pull": n }))).await;
        assert_eq!(s, StatusCode::CONFLICT, "{method}: open PR");
        let (s, _) = app.req(&alice, "POST", &format!("/api/repos/{repo}/pulls/{n}/merge"), Some(json!({ "method": method }))).await;
        assert_eq!(s, StatusCode::OK, "{method}");
        // Main moves on after the merge; the revert still applies on top.
        commit(&app, &alice, &repo, "main", None, json!([{ "path": "later.txt", "content": "later\n" }])).await;

        let (s, v) = app.req(&alice, "POST", &format!("/api/repos/{repo}/revert"), Some(json!({ "pull": n }))).await;
        assert_eq!(s, StatusCode::OK, "{method}: {v}");
        let branch = v["branch"].as_str().unwrap().to_string();
        assert_eq!(branch, format!("revert-pr-{n}"));
        assert_eq!(file(&app, &alice, &repo, &branch, "f.txt").await, None, "{method}");
        assert_eq!(file(&app, &alice, &repo, &branch, "README.md").await, readme, "{method}");
        assert_eq!(file(&app, &alice, &repo, &branch, "later.txt").await.as_deref(), Some("later\n"), "{method}");
        let (_, rp) = app.req(&alice, "GET", &format!("/api/repos/{repo}/issues/{}", v["number"]), None).await;
        assert_eq!(rp["issue"]["title"], "Revert \"Add feature\"", "{method}");
        assert!(rp["issue"]["body"].as_str().unwrap().contains(&format!("#{n}")), "{method}: {rp}");
    }
}

#[tokio::test]
async fn revert_conflicts_and_permissions_give_clear_errors() {
    let app = App::new();
    let alice = app.signup("alice").await;
    let bob = app.signup("bob").await;
    app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "c", "readme": true }))).await;
    commit(&app, &alice, "alice/c", "main", None, json!([{ "path": "a.txt", "content": "one\n" }])).await;
    let sha = commit(&app, &alice, "alice/c", "main", None, json!([{ "path": "a.txt", "content": "two\n" }])).await;
    commit(&app, &alice, "alice/c", "main", None, json!([{ "path": "a.txt", "content": "three\n" }])).await;

    let (s, v) = app.req(&alice, "POST", "/api/repos/alice/c/revert", Some(json!({ "sha": sha }))).await;
    assert_eq!(s, StatusCode::CONFLICT, "{v}");
    let msg = v["error"].as_str().unwrap();
    assert!(msg.contains("a.txt") && msg.contains("conflict"), "{msg}");
    let (_, br) = app.req(&alice, "GET", "/api/repos/alice/c/branches", None).await;
    assert_eq!(br["branches"].as_array().unwrap().len(), 1, "no branch left behind: {br}");

    // Reading isn't enough, and signed-out people are asked to sign in.
    let (s, _) = app.req(&bob, "POST", "/api/repos/alice/c/revert", Some(json!({ "sha": sha }))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = app.req(&Client::default(), "POST", "/api/repos/alice/c/revert", Some(json!({ "sha": sha }))).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn raw_serves_pdf_and_media_with_their_types_and_nothing_runs() {
    use base64::Engine;
    let app = App::new();
    let alice = app.signup("alice").await;
    app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "v", "readme": true }))).await;
    let b64 = |b: &[u8]| base64::engine::general_purpose::STANDARD.encode(b);
    commit(&app, &alice, "alice/v", "main", None, json!([
        { "path": "doc.pdf", "content": b64(b"%PDF-1.4\n\0binary"), "base64": true },
        { "path": "song.mp3", "content": b64(b"ID3\0\0"), "base64": true },
        { "path": "clip.webm", "content": b64(b"\x1aE\xdf\xa3\0"), "base64": true },
        { "path": "page.html", "content": "<script>alert(1)</script>" },
    ])).await;
    for (path, ctype) in [("doc.pdf", "application/pdf"), ("song.mp3", "audio/mpeg"), ("clip.webm", "video/webm"), ("page.html", "text/plain; charset=utf-8")] {
        let (s, h, _) = get(&app, &format!("/raw/alice/v/main/{path}")).await;
        assert_eq!(s, StatusCode::OK, "{path}");
        assert_eq!(h["content-type"], ctype, "{path}");
        let csp = h["content-security-policy"].to_str().unwrap();
        assert!(csp.starts_with("default-src 'none'") && !csp.contains("script-src"), "{path}: {csp}");
        assert_eq!(h["x-content-type-options"], "nosniff");
        if path == "doc.pdf" {
            // Only PDFs may be framed, and only by this site's own pages (the PDF viewer).
            assert!(csp.contains("frame-ancestors 'self'"), "{csp}");
            assert_eq!(h["x-frame-options"], "SAMEORIGIN");
        } else {
            assert!(csp.contains("sandbox"), "{path}: {csp}");
            assert_eq!(h["x-frame-options"], "DENY", "{path}");
        }
    }
}

fn demo_cfg(dir: &std::path::Path) -> gitter::Config {
    gitter::Config {
        data_dir: dir.to_path_buf(),
        addr: String::new(),
        url: "http://demo.local".into(),
        secure_cookies: false,
        trust_proxy: false,
        signup: gitter::SignupMode::Open,
        max_push_bytes: 100 << 20,
        max_repos_per_user: 50,
    }
}

async fn count(db: &gitter::db::Db, sql: &'static str) -> i64 {
    db.call(move |c| Ok(c.query_row(sql, [], |r| r.get(0))?)).await.unwrap()
}

#[tokio::test]
async fn demo_seeds_an_empty_data_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("data");
    let seeded = gitter::demo::seed(&demo_cfg(&dir)).await.expect("seed");
    assert!(seeded.users.len() >= 3 && seeded.users[0] == "alice", "{:?}", seeded.users);
    assert!(!seeded.password.is_empty());

    let db = gitter::db::Db::open(&dir.join("gitter.db")).unwrap();
    assert!(count(&db, "SELECT count(*) FROM users").await >= 3);
    assert_eq!(count(&db, "SELECT count(*) FROM users WHERE is_admin = 1").await, 1);
    assert!(count(&db, "SELECT count(*) FROM repos").await >= 2);
    assert!(count(&db, "SELECT count(*) FROM issues WHERE is_pr = 0").await >= 3);
    assert!(count(&db, "SELECT count(*) FROM issues WHERE is_pr = 1 AND state = 'open'").await >= 1);
    assert!(count(&db, "SELECT count(*) FROM issues WHERE is_pr = 1 AND state = 'merged'").await >= 1);
    assert!(count(&db, "SELECT count(*) FROM comments").await >= 3);
    assert!(count(&db, "SELECT count(*) FROM ci_runs").await >= 1);

    // The seeded instance answers like any other: wiki pages, files for each viewer, a signed-in demo account.
    let st = gitter::AppState::new(demo_cfg(&dir), db);
    let app = gitter::router(st);
    let call = |p: String| {
        let app = app.clone();
        async move {
            let resp = app.oneshot(Request::get(&p).header("host", "demo.local").body(Body::empty()).unwrap()).await.unwrap();
            let s = resp.status();
            (s, serde_json::from_slice::<Value>(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap_or(Value::Null))
        }
    };
    let (s, w) = call("/api/repos/alice/hello/wiki".into()).await;
    assert_eq!(s, StatusCode::OK);
    assert!(w["pages"].as_array().unwrap().len() >= 2, "{w}");
    for p in ["data/planets.csv", "notebooks/orbits.ipynb", "models/pyramid.stl"] {
        let (s, _) = call(format!("/api/repos/alice/hello/blob?path={p}")).await;
        assert_eq!(s, StatusCode::OK, "{p}");
    }
    let login = Request::post("/api/auth/login")
        .header("host", "demo.local")
        .header("content-type", "application/json")
        .body(Body::from(json!({ "login": "bob", "password": seeded.password }).to_string()))
        .unwrap();
    assert_eq!(app.clone().oneshot(login).await.unwrap().status(), StatusCode::OK);
}

#[tokio::test]
async fn demo_refuses_a_non_empty_data_dir() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("notes.txt"), "mine").unwrap();
    let err = gitter::demo::seed(&demo_cfg(tmp.path())).await.err().expect("must refuse");
    assert!(err.to_string().contains("isn't empty"), "{err}");
    assert_eq!(std::fs::read_to_string(tmp.path().join("notes.txt")).unwrap(), "mine");
    assert!(!tmp.path().join("gitter.db").exists());

    // A second run over a seeded directory refuses too.
    let dir = tmp.path().join("fresh");
    gitter::demo::seed(&demo_cfg(&dir)).await.expect("seed");
    assert!(gitter::demo::seed(&demo_cfg(&dir)).await.is_err());
}

#[tokio::test]
async fn demo_instance_serves_the_pwa_manifest_and_icons() {
    let app = App::new();
    let (s, h, body) = get(&app, "/manifest.webmanifest").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(h["content-type"], "application/manifest+json");
    let m: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(m["display"], "standalone");
    assert_eq!(m["start_url"], "/");
    assert!(m["name"].as_str().is_some_and(|n| n.contains("Gitter")));
    let icons = m["icons"].as_array().unwrap();
    for size in ["192x192", "512x512"] {
        let icon = icons.iter().find(|i| i["sizes"] == size).unwrap_or_else(|| panic!("no {size} icon"));
        let (s, h, png) = get(&app, icon["src"].as_str().unwrap()).await;
        assert_eq!(s, StatusCode::OK, "{icon}");
        assert_eq!(h["content-type"], "image/png");
        assert!(png.starts_with(b"\x89PNG"), "{size}");
    }
    let (_, _, page) = get(&app, "/").await;
    let page = String::from_utf8(page).unwrap();
    assert!(page.contains(r#"<link rel="manifest" href="/manifest.webmanifest">"#));
    assert!(page.contains("apple-touch-icon"));
}
