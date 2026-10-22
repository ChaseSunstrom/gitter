//! Repository mirrors: pull mirrors (fetched on a schedule and on "sync now",
//! read-only here) and push mirrors (pushed after every change, retried).
//! The remotes are repositories on the same test server, over real HTTP.

mod common;

use std::path::Path;
use std::process::Command;

use axum::http::StatusCode;
use serde_json::{Value, json};

use common::{App, Client};

/// A test server on a real port, for the git CLI.
async fn serve(app: &App) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let router = app.router.clone();
    tokio::spawn(async move {
        let _ = axum::serve(listener, router.into_make_service_with_connect_info::<std::net::SocketAddr>()).await;
    });
    url
}

fn git_cmd(home: &Path, cwd: &Path, args: &[&str]) -> Command {
    let mut c = Command::new("git");
    c.current_dir(cwd)
        .args(args)
        .env("HOME", home)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_AUTHOR_NAME", "Root")
        .env("GIT_AUTHOR_EMAIL", "root@example.com")
        .env("GIT_COMMITTER_NAME", "Root")
        .env("GIT_COMMITTER_EMAIL", "root@example.com");
    for (k, _) in std::env::vars() {
        if k.starts_with("GIT_CONFIG_KEY") || k.starts_with("GIT_CONFIG_VALUE") || k == "GIT_CONFIG_COUNT" || k == "GIT_DIR" || k == "GIT_WORK_TREE" {
            c.env_remove(k);
        }
    }
    c
}

/// The commit `refname` points at in repository `id`, if any.
fn rev(app: &App, id: i64, refname: &str) -> Option<String> {
    let p = app.state.git.repo_path(id);
    let out = Command::new("git").arg(format!("--git-dir={}", p.display())).args(["rev-parse", "--verify", "-q", refname]).output().unwrap();
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

async fn repo_id(app: &App, _viewer: &Client, full: &str) -> i64 {
    let (owner, name) = full.split_once('/').unwrap();
    let (o, n) = (owner.to_string(), name.to_string());
    app.state
        .db
        .call(move |c| Ok(c.query_row("SELECT r.id FROM repos r JOIN users u ON u.id = r.owner_id WHERE u.username = ?1 AND r.name = ?2", [o, n], |r| r.get(0))?))
        .await
        .unwrap()
}

async fn commit(app: &App, c: &Client, full: &str, path: &str) -> StatusCode {
    let body = json!({ "branch": "main", "message": format!("add {path}"), "changes": [{ "path": path, "content": "x\n" }] });
    app.req(c, "POST", &format!("/api/repos/{full}/contents"), Some(body)).await.0
}

async fn tick(app: &App) -> usize {
    gitter::mirrors::tick(&app.state).await
}

/// Pretend time passed: every mirror is due.
async fn all_due(app: &App) {
    app.state.db.call(|c| Ok(c.execute("UPDATE mirrors SET next_at = 0", [])?)).await.unwrap();
}

async fn mirrors(app: &App, c: &Client, full: &str) -> Value {
    let (s, v) = app.req(c, "GET", &format!("/api/repos/{full}/mirrors"), None).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    v
}

struct Setup {
    app: App,
    root: Client,
    alice: Client,
    url: String,
    token: String,
}

/// root1 (site admin), and alice with a private repository `up` (one commit) to mirror.
async fn setup() -> Setup {
    let app = App::new();
    let root = app.signup("root1").await;
    let alice = app.signup("alice").await;
    let (s, _) = app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "up", "readme": true, "private": true }))).await;
    assert_eq!(s, StatusCode::OK);
    let (_, t) = app.req(&alice, "POST", "/api/me/tokens", Some(json!({ "name": "mirror" }))).await;
    let token = t["token"].as_str().unwrap().to_string();
    let url = serve(&app).await;
    Setup { app, root, alice, url, token }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pull_mirror_follows_its_remote_and_refuses_changes_here() {
    let Setup { app, root, alice, url, token } = setup().await;
    let up = repo_id(&app, &alice, "alice/up").await;
    assert_eq!(app.req(&root, "POST", "/api/repos", Some(json!({ "name": "mir" }))).await.0, StatusCode::OK);
    let mir = repo_id(&app, &root, "root1/mir").await;
    let remote = format!("{url}/alice/up.git");
    let add = |interval: i64| json!({ "direction": "pull", "url": remote, "username": "alice", "password": token, "interval_minutes": interval, "allow_private": true });

    let (s, v) = app.req(&root, "POST", "/api/repos/root1/mir/mirrors", Some(add(5))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "at least 10 minutes: {v}");
    let (s, v) = app.req(&root, "POST", "/api/repos/root1/mir/mirrors", Some(add(60))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let id = v["id"].as_i64().unwrap();
    let (s, _) = app.req(&root, "POST", "/api/repos/root1/mir/mirrors", Some(add(60))).await;
    assert_eq!(s, StatusCode::CONFLICT, "one pull mirror per repository");

    // The first sync is due at once.
    assert_eq!(tick(&app).await, 1);
    assert_eq!(rev(&app, mir, "refs/heads/main"), rev(&app, up, "refs/heads/main"));
    assert!(rev(&app, mir, "refs/heads/main").is_some());
    let v = mirrors(&app, &root, "root1/mir").await;
    let m = &v["mirrors"][0];
    assert_eq!((m["direction"].as_str(), m["last_error"].as_str(), m["has_password"].as_bool()), (Some("pull"), Some(""), Some(true)), "{v}");
    assert!(m["last_ok_at"].as_i64().is_some());
    let wait = m["next_at"].as_i64().unwrap() - gitter::db::now();
    assert!((3590..=3600).contains(&wait), "next sync in an hour, not {wait}");
    assert!(!v.to_string().contains(&token), "the password never comes back: {v}");
    let (_, meta) = app.req(&root, "GET", "/api/repos/root1/mir", None).await;
    assert_eq!(meta["mirror"]["url"].as_str(), Some(remote.as_str()), "the badge: {meta}");

    // A new commit upstream: nothing until it's due, then "sync now" fetches it.
    assert_eq!(commit(&app, &alice, "alice/up", "one.txt").await, StatusCode::OK);
    assert_eq!(tick(&app).await, 0);
    assert_ne!(rev(&app, mir, "refs/heads/main"), rev(&app, up, "refs/heads/main"));
    let (s, v) = app.req(&root, "POST", &format!("/api/repos/root1/mir/mirrors/{id}/sync"), None).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(tick(&app).await, 1);
    assert_eq!(rev(&app, mir, "refs/heads/main"), rev(&app, up, "refs/heads/main"));

    // On schedule: branches and tags come and go with the remote.
    let (s, _) = app.req(&alice, "POST", "/api/repos/alice/up/branches", Some(json!({ "name": "feat", "from": "main" }))).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(commit(&app, &alice, "alice/up", "two.txt").await, StatusCode::OK);
    all_due(&app).await;
    assert_eq!(tick(&app).await, 1);
    assert_eq!(rev(&app, mir, "refs/heads/main"), rev(&app, up, "refs/heads/main"));
    assert!(rev(&app, mir, "refs/heads/feat").is_some());
    assert_eq!(app.req(&alice, "DELETE", "/api/repos/alice/up/branches?name=feat", None).await.0, StatusCode::OK);
    all_due(&app).await;
    tick(&app).await;
    assert!(rev(&app, mir, "refs/heads/feat").is_none(), "pruned");

    // Read-only here, even for its admins: git pushes, web commits, branches, merges, reverts, suggestions, tags.
    let main = rev(&app, mir, "refs/heads/main").unwrap();
    let refused = |s: StatusCode, v: &Value, what: &str| {
        assert_eq!(s, StatusCode::FORBIDDEN, "{what}: {v}");
        assert!(v["error"].as_str().unwrap_or_default().contains("mirror"), "{what}: {v}");
    };
    let (s, v) = app.req(&root, "POST", "/api/repos/root1/mir/contents", Some(json!({ "branch": "main", "message": "x", "changes": [{ "path": "x", "content": "x" }] }))).await;
    refused(s, &v, "web commit");
    let (s, v) = app.req(&root, "POST", "/api/repos/root1/mir/contents", Some(json!({ "branch": "main", "new_branch": "nb", "message": "x", "changes": [{ "path": "x", "content": "x" }] }))).await;
    refused(s, &v, "web commit to a new branch");
    let (s, v) = app.req(&root, "POST", "/api/repos/root1/mir/branches", Some(json!({ "name": "x", "from": "main" }))).await;
    refused(s, &v, "new branch");
    let (s, v) = app.req(&root, "DELETE", "/api/repos/root1/mir/branches?name=feat", None).await;
    refused(s, &v, "branch deletion");
    let (s, v) = app.req(&root, "POST", "/api/repos/root1/mir/revert", Some(json!({ "sha": main }))).await;
    refused(s, &v, "revert");
    let (s, v) = app.req(&root, "POST", "/api/repos/root1/mir/pulls/1/merge", Some(json!({ "method": "merge" }))).await;
    refused(s, &v, "merge");
    let (s, v) = app.req(&root, "POST", "/api/repos/root1/mir/pulls/1/comments/1/apply", Some(json!({ "updated_at": 0 }))).await;
    refused(s, &v, "suggestion");
    let (s, v) = app.req(&root, "POST", "/api/repos/root1/mir/releases", Some(json!({ "tag": "v9" }))).await;
    refused(s, &v, "new tag");
    let (_, t) = app.req(&root, "POST", "/api/me/tokens", Some(json!({ "name": "cli" }))).await;
    let rt = t["token"].as_str().unwrap();
    let (work, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (w, h) = (work.path(), home.path());
    let clone = url.replace("http://", &format!("http://root1:{rt}@")) + "/root1/mir.git";
    assert!(git_cmd(h, w, &["clone", "-q", &clone, "."]).status().unwrap().success());
    std::fs::write(w.join("local.txt"), "x").unwrap();
    assert!(git_cmd(h, w, &["add", "-A"]).status().unwrap().success());
    assert!(git_cmd(h, w, &["commit", "-qm", "local"]).status().unwrap().success());
    let out = git_cmd(h, w, &["push", "-q", "origin", "HEAD:main"]).output().unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success() && err.contains("mirror"), "git push refused: {err}");
    assert_eq!(rev(&app, mir, "refs/heads/main").as_deref(), Some(main.as_str()));

    // No longer a mirror: an ordinary repository again.
    assert_eq!(app.req(&root, "DELETE", &format!("/api/repos/root1/mir/mirrors/{id}"), None).await.0, StatusCode::OK);
    assert_eq!(commit(&app, &root, "root1/mir", "mine.txt").await, StatusCode::OK);
    let (_, meta) = app.req(&root, "GET", "/api/repos/root1/mir", None).await;
    assert!(meta["mirror"].is_null(), "{meta}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mirror_failures_back_off_and_never_show_credentials() {
    let Setup { app, root, alice, url, token } = setup().await;
    let remote = format!("{url}/alice/up.git");
    assert_eq!(app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "m2" }))).await.0, StatusCode::OK);

    // URLs: http(s) only, no credentials in them, and only site admins reach private addresses.
    for bad in ["file:///etc", "ext::sh -c id", "ssh://git@example.com/x.git", "https://alice:pw@example.com/x.git", "https://user@example.com/x.git"] {
        let (s, v) = app.req(&alice, "POST", "/api/repos/alice/m2/mirrors", Some(json!({ "direction": "pull", "url": bad }))).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{bad}: {v}");
    }
    let (s, _) = app.req(&alice, "POST", "/api/repos/alice/m2/mirrors", Some(json!({ "direction": "pull", "url": remote, "allow_private": true }))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, v) = app.req(&alice, "POST", "/api/repos/alice/m2/mirrors", Some(json!({ "direction": "pull", "url": remote, "username": "alice", "password": token }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let id = v["id"].as_i64().unwrap();
    let (s, _) = app.req(&alice, "PATCH", &format!("/api/repos/alice/m2/mirrors/{id}"), Some(json!({ "allow_private": true }))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // A private address is refused, and each failure waits longer (10 minutes, then 20...).
    assert_eq!(tick(&app).await, 1);
    let m = mirrors(&app, &alice, "alice/m2").await["mirrors"][0].clone();
    assert!(m["last_error"].as_str().unwrap().contains("public address"), "{m}");
    assert_eq!(m["failures"].as_i64(), Some(1));
    let wait = m["next_at"].as_i64().unwrap() - gitter::db::now();
    assert!((590..=600).contains(&wait), "{wait}");
    all_due(&app).await;
    tick(&app).await;
    let m = mirrors(&app, &alice, "alice/m2").await["mirrors"][0].clone();
    let wait = m["next_at"].as_i64().unwrap() - gitter::db::now();
    assert_eq!(m["failures"].as_i64(), Some(2));
    assert!((1190..=1200).contains(&wait), "{wait}");
    // Never more than a day apart.
    app.state.db.call(|c| Ok(c.execute("UPDATE mirrors SET failures = 40, next_at = 0", [])?)).await.unwrap();
    tick(&app).await;
    let m = mirrors(&app, &alice, "alice/m2").await["mirrors"][0].clone();
    assert!(m["next_at"].as_i64().unwrap() - gitter::db::now() <= 86400, "{m}");
    assert!(rev(&app, repo_id(&app, &alice, "alice/m2").await, "refs/heads/main").is_none(), "nothing fetched");

    // A wrong password: the error says so without the password, here or in the API.
    let secret = "wrong-secret-0123456789";
    assert_eq!(app.req(&root, "POST", "/api/repos", Some(json!({ "name": "m3" }))).await.0, StatusCode::OK);
    let (s, v) = app.req(&root, "POST", "/api/repos/root1/m3/mirrors", Some(json!({ "direction": "pull", "url": remote, "username": "alice", "password": secret, "allow_private": true }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    tick(&app).await;
    let v = mirrors(&app, &root, "root1/m3").await;
    let err = v["mirrors"][0]["last_error"].as_str().unwrap();
    assert!(!err.is_empty(), "{v}");
    assert!(!v.to_string().contains(secret) && !v.to_string().contains(&token), "{v}");
    let stored: String = app.state.db.call(|c| Ok(c.query_row("SELECT url || last_error FROM mirrors WHERE last_error != '' ORDER BY id DESC", [], |r| r.get(0))?)).await.unwrap();
    assert!(!stored.contains(secret));

    // The git command carries the password in its environment only, never its arguments.
    let path = app.state.git.repo_path(1);
    let remote_cfg = gitter::mirrors::Remote { url: remote.clone(), username: "alice".into(), password: secret.into(), allow_private: true };
    let cmd = gitter::mirrors::git(&app.state, &path, &remote_cfg).await.unwrap();
    let args: Vec<String> = cmd.as_std().get_args().map(|a| a.to_string_lossy().into_owned()).collect();
    use base64::Engine;
    let basic = base64::engine::general_purpose::STANDARD.encode(format!("alice:{secret}"));
    assert!(args.iter().all(|a| !a.contains(secret) && !a.contains(&basic)), "{args:?}");
    assert!(args.iter().any(|a| a == "protocol.allow=never") && args.iter().any(|a| a == "http.followRedirects=false"), "{args:?}");
    let envs: Vec<String> = cmd.as_std().get_envs().filter_map(|(_, v)| v.map(|v| v.to_string_lossy().into_owned())).collect();
    assert!(envs.iter().any(|e| e.contains(&basic)), "the password goes in the environment");
    // Without "allow private", the host is looked up here and git may connect only to what was checked.
    let public = gitter::mirrors::Remote { allow_private: false, ..remote_cfg };
    let e = gitter::mirrors::git(&app.state, &path, &public).await.unwrap_err();
    assert!(e.contains("public address"), "{e}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn push_mirror_pushes_after_changes_and_retries() {
    let Setup { app, root, alice, url, token } = setup().await;
    assert_eq!(app.req(&root, "POST", "/api/repos", Some(json!({ "name": "src", "readme": true }))).await.0, StatusCode::OK);
    let src = repo_id(&app, &root, "root1/src").await;
    let remote = format!("{url}/alice/dst.git");
    let (s, v) = app
        .req(&root, "POST", "/api/repos/root1/src/mirrors", Some(json!({ "direction": "push", "url": remote, "username": "alice", "password": token, "allow_private": true })))
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");

    // The remote isn't there yet: the push fails and is tried again a minute later.
    assert_eq!(tick(&app).await, 1);
    let m = mirrors(&app, &root, "root1/src").await["mirrors"][0].clone();
    assert_eq!(m["failures"].as_i64(), Some(1), "{m}");
    assert!(!m["last_error"].as_str().unwrap().is_empty());
    let wait = m["next_at"].as_i64().unwrap() - gitter::db::now();
    assert!((50..=60).contains(&wait), "{wait}");
    assert_eq!(app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "dst" }))).await.0, StatusCode::OK);
    let dst = repo_id(&app, &alice, "alice/dst").await;
    all_due(&app).await;
    assert_eq!(tick(&app).await, 1);
    assert_eq!(rev(&app, dst, "refs/heads/main"), rev(&app, src, "refs/heads/main"));
    let m = mirrors(&app, &root, "root1/src").await["mirrors"][0].clone();
    assert_eq!((m["failures"].as_i64(), m["last_error"].as_str()), (Some(0), Some("")), "{m}");
    assert!(m["next_at"].is_null(), "nothing waiting: {m}");

    // A burst of changes is one mirror push.
    assert_eq!(commit(&app, &root, "root1/src", "a.txt").await, StatusCode::OK);
    assert_eq!(commit(&app, &root, "root1/src", "b.txt").await, StatusCode::OK);
    assert_eq!(tick(&app).await, 1);
    assert_eq!(tick(&app).await, 0);
    assert_eq!(rev(&app, dst, "refs/heads/main"), rev(&app, src, "refs/heads/main"));
    assert!(!mirrors(&app, &root, "root1/src").await.to_string().contains(&token));
}
