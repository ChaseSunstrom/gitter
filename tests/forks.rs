//! Pull requests from forks: opened from a branch in a fork, kept in step with
//! pushes there, merged here, and edited by maintainers only when allowed.
mod common;

use axum::http::StatusCode;
use common::{App, Client};
use serde_json::{Value, json};

async fn serve(app: &App) -> String {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    let router = app.router.clone();
    tokio::spawn(async move {
        let _ = axum::serve(l, router.into_make_service_with_connect_info::<std::net::SocketAddr>()).await;
    });
    url
}

async fn commit(app: &App, c: &Client, repo: &str, branch: &str, path: &str, content: &str) {
    let body = json!({ "branch": branch, "message": format!("Change {path}"), "changes": [{ "path": path, "content": content }] });
    let (s, v) = app.req(c, "POST", &format!("/api/repos/{repo}/contents"), Some(body)).await;
    assert_eq!(s, StatusCode::OK, "{v}");
}

async fn head(app: &App, c: &Client, repo: &str, rev: &str) -> String {
    let (_, v) = app.req(c, "GET", &format!("/api/repos/{repo}/commits?rev={rev}"), None).await;
    v["commits"][0]["sha"].as_str().unwrap_or_default().to_string()
}

async fn path(app: &App, repo: &str) -> std::path::PathBuf {
    let (owner, name) = repo.split_once('/').unwrap();
    let (o, nm) = (owner.to_string(), name.to_string());
    let id: i64 = app
        .state
        .db
        .call(move |c| Ok(c.query_row("SELECT r.id FROM repos r JOIN users u ON u.id = r.owner_id WHERE u.username = ?1 AND r.name = ?2", [o, nm], |r| r.get(0))?))
        .await
        .unwrap();
    app.state.git.repo_path(id)
}

async fn pull_ref(app: &App, repo: &str, n: i64) -> Option<String> {
    app.state.git.resolve(&path(app, repo).await, &format!("refs/pull/{n}/head")).await.unwrap()
}

/// Every ref in `repo`, hidden ones too.
async fn refs(app: &App, repo: &str) -> String {
    app.state.git.run(&path(app, repo).await, &["for-each-ref", "--format=%(refname)"], None, &[]).await.unwrap().text()
}

/// A git push's after-work (the pull request following it) finishes after git hears back.
async fn wait_pull_ref(app: &App, repo: &str, n: i64, want: &str) -> Option<String> {
    for _ in 0..100 {
        let got = pull_ref(app, repo, n).await;
        if got.as_deref() == Some(want) {
            return got;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    pull_ref(app, repo, n).await
}

async fn pr(app: &App, c: &Client, repo: &str, n: i64) -> Value {
    let (s, v) = app.req(c, "GET", &format!("/api/repos/{repo}/issues/{n}"), None).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    v["pull"].clone()
}

struct World {
    app: App,
    alice: Client,
    bob: Client,
}

/// alice/hello (with a CI script), forked as bob/hello, where bob has a feature branch.
async fn world() -> World {
    let app = App::new();
    // The first account is the site admin, who can write anywhere; the others are ordinary.
    let root = app.signup("root").await;
    // CI for everyone's repositories, so a run that shouldn't start would.
    let (_, mut cur) = app.req(&root, "GET", "/api/admin/settings", None).await;
    cur["ci"] = json!("everyone");
    assert_eq!(app.req(&root, "PUT", "/api/admin/settings", Some(cur)).await.0, StatusCode::OK);
    let alice = app.signup("alice").await;
    let bob = app.signup("bob").await;
    app.signup("carol").await;
    assert_eq!(app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "hello", "readme": true }))).await.0, StatusCode::OK);
    commit(&app, &alice, "alice/hello", "main", ".gitter/ci.sh", "echo hi\n").await;
    let (s, v) = app.req(&bob, "POST", "/api/repos/alice/hello/fork", Some(json!({}))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(app.req(&bob, "POST", "/api/repos/bob/hello/branches", Some(json!({ "name": "feature", "from": "main" }))).await.0, StatusCode::OK);
    commit(&app, &bob, "bob/hello", "feature", "f.txt", "one\n").await;
    World { app, alice, bob }
}

fn open(head: &str, head_repo: &str, extra: Value) -> Value {
    let mut b = json!({ "title": "Add f", "body": "From my fork", "head": head, "head_repo": head_repo, "base": "main" });
    for (k, v) in extra.as_object().cloned().unwrap_or_default() {
        b[k] = v;
    }
    b
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_fork_branch_opens_a_pull_request_that_follows_it_and_merges_here() {
    let World { app, alice, bob } = world().await;
    // alice/hello has a branch of the same name; it isn't the pull request's.
    assert_eq!(app.req(&alice, "POST", "/api/repos/alice/hello/branches", Some(json!({ "name": "feature", "from": "main" }))).await.0, StatusCode::OK);
    let (s, v) = app.req(&bob, "POST", "/api/repos/alice/hello/pulls", Some(open("feature", "bob/hello", json!({})))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let n = v["number"].as_i64().unwrap();
    let p = pr(&app, &alice, "alice/hello", n).await;
    assert_eq!((p["head"].as_str(), p["head_repo"].as_str(), p["from_fork"].as_bool()), (Some("feature"), Some("bob/hello"), Some(true)), "{p}");
    // Its commits are here now, under the pull request's own ref.
    let tip = head(&app, &bob, "bob/hello", "feature").await;
    assert_eq!(pull_ref(&app, "alice/hello", n).await.as_deref(), Some(tip.as_str()));
    assert_eq!(p["head_sha"].as_str(), Some(tip.as_str()));
    let (_, d) = app.req(&alice, "GET", &format!("/api/repos/alice/hello/pulls/{n}/diff"), None).await;
    assert!(d.to_string().contains("f.txt"), "{d}");
    // Pull requests from forks don't start CI here: their code would run with this repository's secrets.
    // Not even when a maintainer here opens it (alice into a new branch dev, her own feature branch differing).
    commit(&app, &alice, "alice/hello", "feature", "mine.txt", "alice's\n").await;
    assert_eq!(app.req(&alice, "POST", "/api/repos/alice/hello/branches", Some(json!({ "name": "dev", "from": "main" }))).await.0, StatusCode::OK);
    let mut dev = open("feature", "bob/hello", json!({}));
    dev["base"] = json!("dev");
    let (s, v) = app.req(&alice, "POST", "/api/repos/alice/hello/pulls", Some(dev)).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let runs: i64 = app.state.db.call(|c| Ok(c.query_row("SELECT count(*) FROM ci_runs WHERE pr_number IS NOT NULL", [], |r| r.get(0))?)).await.unwrap();
    assert_eq!(runs, 0);
    let pushed: i64 = app.state.db.call(|c| Ok(c.query_row("SELECT count(*) FROM ci_runs", [], |r| r.get(0))?)).await.unwrap();
    assert!(pushed > 0, "CI does run here (for pushes), so the check above means something");
    assert!(p["ci_note"].as_str().unwrap_or_default().contains("fork"), "{p}");

    // A push to the fork's branch moves the pull request.
    commit(&app, &bob, "bob/hello", "feature", "f.txt", "two\n").await;
    let tip2 = head(&app, &bob, "bob/hello", "feature").await;
    assert_ne!(tip, tip2);
    assert_eq!(pull_ref(&app, "alice/hello", n).await.as_deref(), Some(tip2.as_str()));
    // Closed, it stops following the branch; reopened, it catches up.
    let ip = format!("/api/repos/alice/hello/issues/{n}");
    assert_eq!(app.req(&alice, "PATCH", &ip, Some(json!({ "state": "closed" }))).await.0, StatusCode::OK);
    commit(&app, &bob, "bob/hello", "feature", "f.txt", "three\n").await;
    let tip3 = head(&app, &bob, "bob/hello", "feature").await;
    assert_eq!(pull_ref(&app, "alice/hello", n).await.as_deref(), Some(tip2.as_str()));
    assert_eq!(app.req(&alice, "PATCH", &ip, Some(json!({ "state": "open" }))).await.0, StatusCode::OK);
    assert_eq!(pull_ref(&app, "alice/hello", n).await.as_deref(), Some(tip3.as_str()));
    // The merge queue tests with CI, and a suggestion would land on a branch here: neither for a fork.
    let (s, q) = app.req(&alice, "POST", &format!("/api/repos/alice/hello/pulls/{n}/queue"), Some(json!({ "method": "merge" }))).await;
    assert!(s == StatusCode::CONFLICT && q.to_string().contains("fork"), "{s} {q}");
    let (s, a) = app.req(&alice, "POST", &format!("/api/repos/alice/hello/pulls/{n}/comments/1/apply"), Some(json!({ "updated_at": 0 }))).await;
    assert!(s == StatusCode::BAD_REQUEST && a.to_string().contains("fork"), "{s} {a}");
    // Merging takes the fork's commits; the fork's branch is the fork's, so it stays.
    let (s, m) = app.req(&alice, "POST", &format!("/api/repos/alice/hello/pulls/{n}/merge"), Some(json!({ "method": "merge", "delete_branch": true }))).await;
    assert_eq!(s, StatusCode::OK, "{m}");
    let id: i64 = app.state.db.call(|c| Ok(c.query_row("SELECT id FROM repos WHERE name = 'hello' AND owner_id = (SELECT id FROM users WHERE username = 'alice')", [], |r| r.get(0))?)).await.unwrap();
    let f = app.state.git.run(&app.state.git.repo_path(id), &["show", "main:f.txt"], None, &[]).await.unwrap().text();
    assert_eq!(f, "three\n");
    assert_eq!(head(&app, &bob, "bob/hello", "feature").await, tip3, "the fork's branch is left alone");
    let (_, r) = app.req(&alice, "GET", "/api/repos/alice/hello", None).await;
    assert!(r["branches"].as_array().unwrap().contains(&json!("feature")), "nor is a branch here with its name: {r}");
    assert_eq!(app.req(&alice, "GET", &format!("/api/repos/alice/hello/issues/{n}"), None).await.1["issue"]["state"].as_str(), Some("merged"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn only_forks_in_the_same_family_that_the_opener_can_read() {
    let World { app, alice, bob } = world().await;
    let carol = {
        let (_, _, c) = app.call(&Client::default(), "POST", "/api/auth/login", Some(json!({ "login": "carol", "password": "correct horse" })), Some(common::ORIGIN)).await;
        Client(c)
    };
    // Comparing across forks, and the family a pull request can come from.
    let (s, c) = app.req(&carol, "GET", "/api/repos/alice/hello/compare?base=main&head=feature&head_repo=bob/hello", None).await;
    assert_eq!(s, StatusCode::OK, "{c}");
    assert!(c["ahead"].as_i64().unwrap() >= 1 && c.to_string().contains("f.txt"), "{c}");
    let (_, net) = app.req(&carol, "GET", "/api/repos/alice/hello/network", None).await;
    assert_eq!(net["repos"], json!([{ "full_name": "bob/hello", "default_branch": "main", "parent": false }]), "{net}");
    let (_, net) = app.req(&carol, "GET", "/api/repos/bob/hello/network", None).await;
    assert_eq!(net["repos"][0]["full_name"], json!("alice/hello"), "the parent first: {net}");
    // An unrelated repository with the same name isn't a fork.
    assert_eq!(app.req(&carol, "POST", "/api/repos", Some(json!({ "name": "hello", "readme": true }))).await.0, StatusCode::OK);
    let (s, _) = app.req(&carol, "POST", "/api/repos/alice/hello/pulls", Some(open("main", "carol/hello", json!({})))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    // A branch the fork doesn't have, one with nothing new, or a target that isn't there.
    let (s, _) = app.req(&bob, "POST", "/api/repos/alice/hello/pulls", Some(open("nope", "bob/hello", json!({})))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = app.req(&bob, "POST", "/api/repos/alice/hello/pulls", Some(open("main", "bob/hello", json!({})))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let mut nope = open("feature", "bob/hello", json!({}));
    nope["base"] = json!("nope");
    assert_eq!(app.req(&bob, "POST", "/api/repos/alice/hello/pulls", Some(nope)).await.0, StatusCode::BAD_REQUEST);
    // None of that, nor comparing, leaves a copy of the fork's branch behind.
    let left = refs(&app, "alice/hello").await;
    assert!(!left.contains("refs/pull/") && !left.contains("gitter-compare"), "{left}");
    // A private fork carol can't see doesn't exist for her.
    assert_eq!(app.req(&bob, "PATCH", "/api/repos/bob/hello", Some(json!({ "private": true }))).await.0, StatusCode::OK);
    assert_eq!(app.req(&carol, "GET", "/api/repos/alice/hello/network", None).await.1["repos"], json!([]));
    let (s, _) = app.req(&carol, "GET", "/api/repos/alice/hello/compare?base=main&head=feature&head_repo=bob/hello", None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = app.req(&carol, "POST", "/api/repos/alice/hello/pulls", Some(open("feature", "bob/hello", json!({})))).await;
    assert!(s == StatusCode::NOT_FOUND || s == StatusCode::BAD_REQUEST, "{s}");
    // Able to read it, she still can't copy it into another repository through a pull request.
    assert_eq!(app.req(&bob, "PUT", "/api/repos/bob/hello/collaborators/carol", Some(json!({ "role": "read" }))).await.0, StatusCode::OK);
    let (s, v) = app.req(&carol, "POST", "/api/repos/alice/hello/pulls", Some(open("feature", "bob/hello", json!({})))).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{v}");
    // The same branch can't have two open pull requests into the same branch.
    assert_eq!(app.req(&bob, "POST", "/api/repos/alice/hello/pulls", Some(open("feature", "bob/hello", json!({})))).await.0, StatusCode::OK);
    assert_eq!(app.req(&bob, "POST", "/api/repos/alice/hello/pulls", Some(open("feature", "bob/hello", json!({})))).await.0, StatusCode::CONFLICT);
    // The other way: from the parent into the fork.
    commit(&app, &alice, "alice/hello", "main", "up.txt", "news\n").await;
    let (s, v) = app.req(&bob, "POST", "/api/repos/bob/hello/pulls", Some(open("main", "alice/hello", json!({ "title": "Catch up" })))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    // What's copied from a fork counts toward the storage of the repository it goes into: none left, none copied.
    let mut set = app.state.settings.get();
    set.max_storage_mb = 1;
    app.state.settings.save(&app.state.db, set).await.unwrap();
    app.state.db.call(|c| Ok(c.execute("UPDATE repos SET size_bytes = 2000000", [])?)).await.unwrap();
    let (s, c) = app.req(&bob, "GET", "/api/repos/alice/hello/compare?base=main&head=feature&head_repo=bob/hello", None).await;
    assert!(s == StatusCode::BAD_REQUEST && c.to_string().contains("out of storage"), "{s} {c}");
}

fn git(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    std::process::Command::new("git")
        .current_dir(dir)
        .args(["-c", "user.name=A", "-c", "user.email=a@example.com", "-c", "protocol.version=2"])
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn maintainers_push_to_the_fork_branch_only_when_allowed_and_only_that_branch() {
    let World { app, alice, bob } = world().await;
    let url = serve(&app).await;
    let (s, v) = app.req(&bob, "POST", "/api/repos/alice/hello/pulls", Some(open("feature", "bob/hello", json!({})))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let n = v["number"].as_i64().unwrap();
    assert_eq!(pr(&app, &alice, "alice/hello", n).await["maintainer_can_modify"], json!(false));
    let (_, t) = app.req(&alice, "POST", "/api/me/tokens", Some(json!({ "name": "push" }))).await;
    let token = t["token"].as_str().unwrap().to_string();
    let dir = tempfile::tempdir().unwrap();
    let remote = url.replacen("http://", &format!("http://alice:{token}@"), 1) + "/bob/hello.git"; // pragma: allowlist secret
    assert!(git(dir.path(), &["clone", "-q", &remote, "w"]).status.success());
    let w = dir.path().join("w");
    assert!(git(&w, &["checkout", "-q", "feature"]).status.success());
    std::fs::write(w.join("m.txt"), "from a maintainer\n").unwrap();
    assert!(git(&w, &["add", "m.txt"]).status.success());
    assert!(git(&w, &["commit", "-q", "-m", "Maintainer fix"]).status.success());
    // Not allowed yet: alice can't write to bob's fork.
    assert!(!git(&w, &["push", "-q", "origin", "feature"]).status.success());

    // bob allows edits from maintainers; only he (the author) can.
    let path = format!("/api/repos/alice/hello/pulls/{n}");
    assert_eq!(app.req(&alice, "PATCH", &path, Some(json!({ "maintainer_can_modify": true }))).await.0, StatusCode::FORBIDDEN);
    assert_eq!(app.req(&bob, "PATCH", &path, Some(json!({ "maintainer_can_modify": true }))).await.0, StatusCode::OK);
    let out = git(&w, &["push", "-q", "origin", "feature"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let tip = String::from_utf8(git(&w, &["rev-parse", "HEAD"]).stdout).unwrap().trim().to_string();
    assert_eq!(head(&app, &bob, "bob/hello", "feature").await, tip);
    assert_eq!(wait_pull_ref(&app, "alice/hello", n, &tip).await.as_deref(), Some(tip.as_str()), "the pull request follows");
    // The fork's CI (with the fork's secrets) doesn't run what a maintainer pushed.
    let runs: i64 = app
        .state
        .db
        .call(|c| {
            Ok(c.query_row(
                "SELECT count(*) FROM ci_runs r JOIN users a ON a.id = r.actor_id JOIN repos p ON p.id = r.repo_id JOIN users o ON o.id = p.owner_id WHERE a.username = 'alice' AND o.username = 'bob'",
                [],
                |x| x.get(0),
            )?)
        })
        .await
        .unwrap();
    assert_eq!(runs, 0);
    // Someone without write access to alice/hello isn't a maintainer of it.
    let (_, t2) = app.req(&{
        let (_, _, c) = app.call(&Client::default(), "POST", "/api/auth/login", Some(json!({ "login": "carol", "password": "correct horse" })), Some(common::ORIGIN)).await;
        Client(c)
    }, "POST", "/api/me/tokens", Some(json!({ "name": "push" }))).await;
    let carol_remote = url.replacen("http://", &format!("http://carol:{}@", t2["token"].as_str().unwrap()), 1) + "/bob/hello.git"; // pragma: allowlist secret
    assert!(!git(&w, &["push", "-q", &carol_remote, "HEAD:feature"]).status.success());
    // Nobody pushes a pull request's ref, not even with write access to the repository.
    let alice_remote = url.replacen("http://", &format!("http://alice:{token}@"), 1) + "/alice/hello.git"; // pragma: allowlist secret
    let out = git(&w, &["push", "-q", &alice_remote, &format!("+HEAD~1:refs/pull/{n}/head")]);
    assert!(!out.status.success() && String::from_utf8_lossy(&out.stderr).contains("pull request refs"), "{}", String::from_utf8_lossy(&out.stderr));
    // What a comparison across forks copied here isn't shown to git clients.
    app.req(&alice, "GET", "/api/repos/alice/hello/compare?base=main&head=feature&head_repo=bob/hello", None).await;
    let refs = String::from_utf8(git(&w, &["ls-remote", &alice_remote]).stdout).unwrap();
    assert!(refs.contains(&format!("refs/pull/{n}/head")) && !refs.contains("gitter-compare"), "{refs}");
    // Only someone who can write to the fork grants that: carol forks alice/hello too, opens a pull
    // request into her own fork from bob's branch and "allows" edits, and still can't push there.
    let carol = {
        let (_, _, c) = app.call(&Client::default(), "POST", "/api/auth/login", Some(json!({ "login": "carol", "password": "correct horse" })), Some(common::ORIGIN)).await;
        Client(c)
    };
    assert_eq!(app.req(&carol, "POST", "/api/repos/alice/hello/fork", Some(json!({}))).await.0, StatusCode::OK);
    let (s, v) = app.req(&carol, "POST", "/api/repos/carol/hello/pulls", Some(open("feature", "bob/hello", json!({ "maintainer_can_modify": true })))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let cn = v["number"].as_i64().unwrap();
    assert_eq!(pr(&app, &carol, "carol/hello", cn).await["maintainer_can_modify"], json!(false), "not hers to grant");
    assert_eq!(app.req(&carol, "PATCH", &format!("/api/repos/carol/hello/pulls/{cn}"), Some(json!({ "maintainer_can_modify": true }))).await.0, StatusCode::FORBIDDEN);
    app.state.db.call(|c| Ok(c.execute("UPDATE pulls SET maintainer_can_modify = 1", [])?)).await.unwrap();
    assert!(!git(&w, &["push", "-q", &carol_remote, "HEAD:feature"]).status.success(), "a grant its author can't give doesn't count");
    // Any other branch of the fork, or deleting this one, stays bob's.
    assert!(git(&w, &["checkout", "-q", "-b", "other"]).status.success());
    let out = git(&w, &["push", "-q", "origin", "other"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("feature"), "says which branch: {}", String::from_utf8_lossy(&out.stderr));
    assert!(!git(&w, &["push", "-q", "origin", "HEAD:main"]).status.success());
    assert!(!git(&w, &["push", "-q", "origin", ":feature"]).status.success());
    // Once it's merged (or closed), the permission ends.
    let (s, m) = app.req(&alice, "POST", &format!("{path}/merge"), Some(json!({ "method": "merge" }))).await;
    assert_eq!(s, StatusCode::OK, "{m}");
    assert!(git(&w, &["checkout", "-q", "feature"]).status.success());
    std::fs::write(w.join("m.txt"), "again\n").unwrap();
    assert!(git(&w, &["commit", "-qam", "After the merge"]).status.success());
    let state = app.req(&alice, "GET", &format!("/api/repos/alice/hello/issues/{n}"), None).await.1["issue"]["state"].clone();
    let out = git(&w, &["push", "origin", "feature"]);
    assert!(!out.status.success(), "{state} {}", String::from_utf8_lossy(&out.stderr));
}
