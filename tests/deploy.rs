//! Deployments and environments: environments with a URL, a branch rule and secrets of their
//! own; deployments recorded over the API or by CI jobs; pull request previews served here.
mod common;

use axum::http::StatusCode;
use common::{App, Client};
use serde_json::{Value, json};

struct World {
    app: App,
    alice: Client,
    bob: Client,
}

/// alice/site (public, with a README and a feature branch); bob has no access to it.
async fn world() -> World {
    let app = App::new();
    // The first account is the site admin; alice and bob are ordinary.
    app.signup("root").await;
    let alice = app.signup("alice").await;
    let bob = app.signup("bob").await;
    assert_eq!(app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "site", "readme": true }))).await.0, StatusCode::OK);
    assert_eq!(app.req(&alice, "POST", "/api/repos/alice/site/branches", Some(json!({ "name": "feature", "from": "main" }))).await.0, StatusCode::OK);
    World { app, alice, bob }
}

async fn deployments(app: &App, c: &Client, query: &str) -> Vec<Value> {
    let (s, v) = app.req(c, "GET", &format!("/api/repos/alice/site/deployments{query}"), None).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    v["deployments"].as_array().cloned().unwrap_or_default()
}

async fn deploy(app: &App, c: &Client, body: Value) -> (StatusCode, Value) {
    app.req(c, "POST", "/api/repos/alice/site/deployments", Some(body)).await
}

#[tokio::test]
async fn environments_and_deployments_over_the_api() {
    let World { app, alice, bob } = world().await;
    // A repository admin makes an environment with a URL and a branch rule.
    let prod = json!({ "name": "production", "url": "https://example.com", "branches": "main" });
    let (s, v) = app.req(&alice, "PUT", "/api/repos/alice/site/environments", Some(prod.clone())).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let env = v["id"].as_i64().unwrap();
    assert!(app.req(&bob, "PUT", "/api/repos/alice/site/environments", Some(prod)).await.0.is_client_error());
    for bad in [json!({ "name": "../x" }), json!({ "name": "ok", "url": "javascript:alert(1)" }), json!({ "name": "" })] {
        assert_eq!(app.req(&alice, "PUT", "/api/repos/alice/site/environments", Some(bad.clone())).await.0, StatusCode::BAD_REQUEST, "{bad}");
    }
    // Anyone who can read the repository sees its environments.
    let (s, v) = app.req(&bob, "GET", "/api/repos/alice/site/environments", None).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["environments"][0]["name"], "production", "{v}");
    assert_eq!(v["environments"][0]["url"], "https://example.com");
    assert_eq!(v["environments"][0]["latest"], Value::Null);

    // A writer records deployments; the branch rule keeps other branches out of production.
    let (s, v) = deploy(&app, &alice, json!({ "environment": "production", "ref": "main" })).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let d1 = v["id"].as_i64().unwrap();
    let (s, v) = deploy(&app, &alice, json!({ "environment": "production", "ref": "feature" })).await;
    assert!(s == StatusCode::FORBIDDEN && v.to_string().contains("main"), "{s} {v}");
    let (s, v) = deploy(&app, &alice, json!({ "environment": "staging", "ref": "feature", "url": "https://staging.example.com" })).await;
    assert_eq!(s, StatusCode::OK, "made on the fly: {v}");
    assert!(deploy(&app, &bob, json!({ "environment": "staging", "ref": "main" })).await.0.is_client_error());
    assert_eq!(deploy(&app, &alice, json!({ "environment": "staging", "ref": "nope" })).await.0, StatusCode::BAD_REQUEST);

    // A success retires the environment's earlier one.
    let path = |id: i64| format!("/api/repos/alice/site/deployments/{id}");
    assert_eq!(app.req(&alice, "PATCH", &path(d1), Some(json!({ "status": "success" }))).await.0, StatusCode::OK);
    let (_, v) = deploy(&app, &alice, json!({ "environment": "production", "ref": "main", "description": "second" })).await;
    let d2 = v["id"].as_i64().unwrap();
    assert_eq!(app.req(&alice, "PATCH", &path(d2), Some(json!({ "status": "in_progress" }))).await.0, StatusCode::OK);
    assert_eq!(app.req(&alice, "PATCH", &path(d2), Some(json!({ "status": "success", "url": "https://example.com/v2" }))).await.0, StatusCode::OK);
    assert_eq!(app.req(&alice, "PATCH", &path(d2), Some(json!({ "status": "done" }))).await.0, StatusCode::BAD_REQUEST);
    assert!(app.req(&bob, "PATCH", &path(d2), Some(json!({ "status": "failure" }))).await.0.is_client_error());
    let list = deployments(&app, &bob, "?environment=production").await;
    let main_sha = app.req(&alice, "GET", "/api/repos/alice/site/commits?rev=main", None).await.1["commits"][0]["sha"].clone();
    assert_eq!(list.len(), 2, "{list:?}");
    assert_eq!((list[0]["id"].as_i64(), list[0]["status"].as_str(), list[0]["url"].as_str()), (Some(d2), Some("success"), Some("https://example.com/v2")));
    assert_eq!((list[0]["ref"].as_str(), list[0]["creator"].as_str(), &list[0]["sha"]), (Some("main"), Some("alice"), &main_sha));
    assert_eq!(list[1]["status"], "inactive");
    let (_, v) = app.req(&bob, "GET", "/api/repos/alice/site/environments", None).await;
    let prod = v["environments"].as_array().unwrap().iter().find(|e| e["name"] == "production").unwrap().clone();
    assert_eq!(prod["latest"]["id"].as_i64(), Some(d2), "{prod}");

    // Environment secrets: names only, for admins.
    let sec = format!("/api/repos/alice/site/environments/{env}/secrets");
    assert_eq!(app.req(&alice, "PUT", &format!("{sec}/DEPLOY_KEY"), Some(json!({ "value": "s3cret" }))).await.0, StatusCode::OK); // pragma: allowlist secret
    assert!(app.req(&bob, "PUT", &format!("{sec}/DEPLOY_KEY"), Some(json!({ "value": "x" }))).await.0.is_client_error());
    assert_eq!(app.req(&alice, "PUT", &format!("{sec}/bad-name"), Some(json!({ "value": "x" }))).await.0, StatusCode::BAD_REQUEST);
    let (s, v) = app.req(&alice, "GET", &sec, None).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["secrets"][0]["name"], "DEPLOY_KEY");
    assert!(!v.to_string().contains("s3cret"));
    assert!(app.req(&bob, "GET", &sec, None).await.0.is_client_error());

    // Deleting the environment takes its deployments with it.
    assert_eq!(app.req(&alice, "DELETE", &format!("/api/repos/alice/site/environments/{env}"), None).await.0, StatusCode::OK);
    assert!(deployments(&app, &alice, "?environment=production").await.is_empty());
}

/// Wait for the newest CI run to end.
async fn settle(app: &App) -> String {
    for _ in 0..300 {
        let s: String = app.state.db.call(|c| Ok(c.query_row("SELECT status FROM ci_runs ORDER BY id DESC LIMIT 1", [], |r| r.get(0))?)).await.unwrap_or_default();
        if matches!(s.as_str(), "success" | "failure" | "cancelled" | "skipped") {
            return s;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!("the run didn't finish");
}

async fn commit(app: &App, who: &Client, repo: &str, branch: &str, new_branch: Option<&str>, path: &str, content: &str) {
    let mut body = json!({ "branch": branch, "message": format!("Change {path}"), "changes": [{ "path": path, "content": content }] });
    if let Some(b) = new_branch {
        body["new_branch"] = json!(b);
    }
    let (s, v) = app.req(who, "POST", &format!("/api/repos/{repo}/contents"), Some(body)).await;
    assert_eq!(s, StatusCode::OK, "commit {path}: {v}");
}

/// The newest run's job `key`: its log.
async fn job_log(app: &App, key: &str) -> String {
    let k = key.to_string();
    let id: i64 = app
        .state
        .db
        .call(move |c| Ok(c.query_row("SELECT j.id FROM ci_jobs j WHERE j.key = ?1 AND j.run_id = (SELECT max(id) FROM ci_runs)", [k], |r| r.get(0))?))
        .await
        .unwrap();
    std::fs::read_to_string(gitter::ci::jobs::log_path(&app.state.cfg.data_dir, id)).unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ci_jobs_deploy_with_their_environments_secrets_and_branch_rules() {
    let app = App::new();
    // The built-in runner runs the site admin's repositories.
    let root = app.signup("root").await;
    assert_eq!(app.req(&root, "POST", "/api/repos", Some(json!({ "name": "app", "readme": true }))).await.0, StatusCode::OK);
    let env = |name: &str, branches: &str| json!({ "name": name, "branches": branches });
    let prod = app.req(&root, "PUT", "/api/repos/root/app/environments", Some(env("production", "main"))).await.1["id"].as_i64().unwrap();
    let stage = app.req(&root, "PUT", "/api/repos/root/app/environments", Some(env("staging", ""))).await.1["id"].as_i64().unwrap();
    let secret = |e: i64, name: &str| format!("/api/repos/root/app/environments/{e}/secrets/{name}");
    assert_eq!(app.req(&root, "PUT", &secret(prod, "PROD_KEY"), Some(json!({ "value": "prod-hush" }))).await.0, StatusCode::OK); // pragma: allowlist secret
    assert_eq!(app.req(&root, "PUT", &secret(stage, "STAGE_KEY"), Some(json!({ "value": "stage-hush" }))).await.0, StatusCode::OK); // pragma: allowlist secret
    tokio::spawn(gitter::ci::runner::builtin(app.state.clone()));
    let config = "jobs:
  prod:
    environment: { name: production, url: https://example.com/app }
    steps: [\"echo prod=$PROD_KEY stage=$STAGE_KEY\"]
  stage:
    environment: staging
    steps: [\"echo stage=$STAGE_KEY prod=$PROD_KEY; exit 4\"]
";
    commit(&app, &root, "root/app", "main", None, ".gitter/ci.yml", config).await;
    assert_eq!(settle(&app).await, "failure");
    // Each job got its own environment's secret (masked in its log), and not the other's.
    let log = job_log(&app, "prod").await;
    assert!(log.contains("prod=***") && !log.contains("stage=***") && !log.contains("stage-hush"), "{log}");
    let log = job_log(&app, "stage").await;
    assert!(log.contains("stage=***") && !log.contains("prod=***") && !log.contains("prod-hush"), "{log}");
    // The deployments followed their jobs.
    let (_, v) = app.req(&root, "GET", "/api/repos/root/app/deployments", None).await;
    let list = v["deployments"].as_array().unwrap().clone();
    let of = |name: &str| list.iter().find(|d| d["environment"] == name).cloned().unwrap_or_default();
    let main = app.req(&root, "GET", "/api/repos/root/app/commits?rev=main", None).await.1["commits"][0]["sha"].clone();
    assert_eq!((of("production")["status"].as_str(), of("production")["url"].as_str()), (Some("success"), Some("https://example.com/app")), "{list:?}");
    assert_eq!((&of("production")["sha"], of("production")["ref"].as_str(), of("production")["run_number"].as_i64()), (&main, Some("main"), Some(1)));
    assert!(of("production")["job_id"].as_i64().is_some());
    assert_eq!(of("staging")["status"], "failure");

    // From another branch, production's rule keeps the job out: it fails saying why, with no secret and no deployment.
    commit(&app, &root, "root/app", "main", Some("feature"), "f.txt", "f\n").await;
    assert_eq!(settle(&app).await, "failure");
    let log = job_log(&app, "prod").await;
    assert!(log.contains("production") && log.contains("main") && !log.contains("prod="), "{log}");
    let (_, v) = app.req(&root, "GET", "/api/repos/root/app/deployments?environment=production", None).await;
    assert_eq!(v["deployments"].as_array().unwrap().len(), 1, "{v}");
    let (_, v) = app.req(&root, "GET", "/api/repos/root/app/deployments?environment=staging", None).await;
    assert_eq!(v["deployments"][0]["ref"], "feature", "staging takes any branch: {v}");
    // An API client can't move a CI job's deployment.
    let id = of("production")["id"].as_i64().unwrap();
    assert_eq!(app.req(&root, "PATCH", &format!("/api/repos/root/app/deployments/{id}"), Some(json!({ "status": "failure" }))).await.0, StatusCode::CONFLICT);
}

/// A request outside the API, as a browser with no cookies makes it.
async fn get_page(app: &App, path: &str) -> (StatusCode, axum::http::HeaderMap, String) {
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    let req = axum::http::Request::get(path).header("host", "test.local").body(axum::body::Body::empty()).unwrap();
    let resp = app.router.clone().oneshot(req).await.unwrap();
    let (s, h) = (resp.status(), resp.headers().clone());
    (s, h, String::from_utf8_lossy(&resp.into_body().collect().await.unwrap().to_bytes()).into_owned())
}

async fn previews(app: &App, c: &Client) -> Vec<Value> {
    let (_, v) = app.req(c, "GET", "/api/repos/root/site/deployments?pr=1", None).await;
    v["deployments"].as_array().cloned().unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pull_requests_get_sandboxed_previews_that_go_away_with_them() {
    let app = App::new();
    let root = app.signup("root").await;
    assert_eq!(app.req(&root, "POST", "/api/repos", Some(json!({ "name": "site", "readme": true, "private": true }))).await.0, StatusCode::OK);
    tokio::spawn(gitter::ci::runner::builtin(app.state.clone()));
    let config = "jobs:
  web:
    preview: public
    steps: [\"mkdir -p public/css && echo \\\"<h1>Hello from $GITTER_BRANCH</h1>\\\" > public/index.html && echo 'h1{}' > public/css/a.css && echo private > outside.txt && ln -s ../outside.txt public/link.txt\"]
";
    commit(&app, &root, "root/site", "main", None, ".gitter/ci.yml", config).await;
    assert_eq!(settle(&app).await, "success");
    // A push isn't a pull request: nothing to preview.
    let (_, v) = app.req(&root, "GET", "/api/repos/root/site/deployments", None).await;
    assert_eq!(v["deployments"], json!([]), "{v}");

    commit(&app, &root, "root/site", "main", Some("feature"), "f.txt", "one\n").await;
    let (s, v) = app.req(&root, "POST", "/api/repos/root/site/pulls", Some(json!({ "title": "F", "body": "", "head": "feature", "base": "main" }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(settle(&app).await, "success");
    let list = previews(&app, &root).await;
    assert_eq!((list.len(), list[0]["environment"].as_str(), list[0]["status"].as_str()), (1, Some("preview/pr-1"), Some("success")), "{list:?}");
    let first = list[0]["preview"].as_str().unwrap().to_string();
    assert!(first.starts_with("/preview/") && first.ends_with('/') && first.len() > 40, "{first}");

    // Served without cookies (the link is the key, even for a private repository), sandboxed.
    let (s, h, body) = get_page(&app, &first).await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert!(body.contains("Hello from feature"), "{body}");
    let csp = h["content-security-policy"].to_str().unwrap();
    assert!(csp.contains("sandbox") && csp.contains("allow-scripts") && !csp.contains("allow-same-origin"), "{csp}");
    assert_eq!(h["access-control-allow-origin"], "*");
    assert!(h["content-type"].to_str().unwrap().starts_with("text/html"));
    let (s, h, _) = get_page(&app, &format!("{first}css/a.css")).await;
    assert_eq!((s, h["content-type"].to_str().unwrap()), (StatusCode::OK, "text/css"));
    // Nothing outside the folder: not the job's other files, not a symlink, not the server's files.
    std::fs::write(app.state.cfg.data_dir.join("ci/probe.txt"), "server file").unwrap();
    for nope in [
        format!("{first}outside.txt"),
        format!("{first}link.txt"),
        format!("{first}..%2f..%2fprobe.txt"),
        format!("{first}%2e%2e/%2e%2e/probe.txt"),
        format!("{first}nope.html"),
        "/preview/not-a-token/".to_string(),
    ] {
        assert_eq!(get_page(&app, &nope).await.0, StatusCode::NOT_FOUND, "{nope}");
    }
    let (s, h, _) = get_page(&app, first.trim_end_matches('/')).await;
    assert!(s.is_redirection() && h["location"] == first.as_str(), "{s}");

    // A new push to the branch replaces the preview.
    commit(&app, &root, "root/site", "feature", None, "f.txt", "two\n").await;
    assert_eq!(settle(&app).await, "success");
    let list = previews(&app, &root).await;
    assert_eq!((list[0]["status"].as_str(), list[1]["status"].as_str(), &list[1]["preview"]), (Some("success"), Some("inactive"), &Value::Null), "{list:?}");
    let second = list[0]["preview"].as_str().unwrap().to_string();
    assert_ne!(first, second);
    assert_eq!(get_page(&app, &first).await.0, StatusCode::NOT_FOUND);
    assert_eq!(get_page(&app, &second).await.0, StatusCode::OK);

    // A repository moderators hid has no previews either.
    app.state.db.call(|c| Ok(c.execute("UPDATE repos SET hidden = 1", [])?)).await.unwrap();
    assert_eq!(get_page(&app, &second).await.0, StatusCode::NOT_FOUND);
    app.state.db.call(|c| Ok(c.execute("UPDATE repos SET hidden = 0", [])?)).await.unwrap();
    assert_eq!(get_page(&app, &second).await.0, StatusCode::OK);

    // Closing the pull request takes it down.
    assert_eq!(app.req(&root, "PATCH", "/api/repos/root/site/issues/1", Some(json!({ "state": "closed" }))).await.0, StatusCode::OK);
    assert_eq!(get_page(&app, &second).await.0, StatusCode::NOT_FOUND);
    assert_eq!(previews(&app, &root).await[0]["status"], "inactive");
    let dir = app.state.cfg.data_dir.join("ci/previews");
    assert_eq!(std::fs::read_dir(&dir).map(|d| d.count()).unwrap_or(0), 0, "its files are gone");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn preview_files_count_toward_the_owners_storage() {
    let app = App::new();
    let root = app.signup("root").await;
    assert_eq!(app.req(&root, "POST", "/api/repos", Some(json!({ "name": "site", "readme": true }))).await.0, StatusCode::OK);
    tokio::spawn(gitter::ci::runner::builtin(app.state.clone()));
    // A preview of 1.2 MB.
    let config = "on: [pull_request]
jobs:
  web:
    preview: public
    steps: [\"mkdir -p public && head -c 1200000 /dev/zero > public/big.bin && echo hi > public/index.html\"]
";
    commit(&app, &root, "root/site", "main", None, ".gitter/ci.yml", config).await;
    commit(&app, &root, "root/site", "main", Some("feature"), "f.txt", "one\n").await;
    let used = || async { app.state.db.call(|c| Ok(gitter::repo::usage(c, 1)?.0)).await.unwrap() };
    let before = used().await;
    assert!(before < 1_000_000, "{before}");
    let (s, v) = app.req(&root, "POST", "/api/repos/root/site/pulls", Some(json!({ "title": "F", "body": "", "head": "feature", "base": "main" }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(settle(&app).await, "success");
    assert_eq!(previews(&app, &root).await[0]["status"], "success");
    let with = used().await;
    assert!(with >= before + 1_200_000, "the preview counts: {before} -> {with}");

    // Room for one preview but not two: 2 MB, the account no longer unlimited (the built-in
    // runner only runs an admin's jobs, so from here on previews are published directly).
    let mut set = app.state.settings.get();
    set.max_storage_mb = 2;
    app.state.settings.save(&app.state.db, set).await.unwrap();
    app.state.db.call(|c| Ok(c.execute("UPDATE users SET is_admin = 0 WHERE id = 1", [])?)).await.unwrap();

    // The same job's preview again, as a second deployment of the pull request that's just
    // finished building.
    let again = || async {
        let job: i64 = app
            .state
            .db
            .call(|c| {
                c.execute(
                    "INSERT INTO deployments (repo_id, environment_id, ref, sha, creator_id, job_id, pr_number, status, created_at, updated_at)
                     SELECT repo_id, environment_id, ref, sha, creator_id, job_id, pr_number, 'in_progress', created_at, updated_at + 1 FROM deployments WHERE preview IS NOT NULL",
                    [],
                )?;
                Ok(c.query_row("SELECT job_id FROM deployments WHERE status = 'in_progress'", [], |r| r.get(0))?)
            })
            .await
            .unwrap();
        gitter::api::deploy::job_finished(&app.state, job, true).await;
        previews(&app, &root).await
    };
    // A replacement isn't blocked by the preview it replaces.
    let list = again().await;
    assert_eq!((list[0]["status"].as_str(), list[1]["status"].as_str(), &list[1]["preview"]), (Some("success"), Some("inactive"), &Value::Null), "{list:?}");
    let with = used().await;
    assert!(with < 2_000_000, "counted once: {with}");

    // Past the space left (1 MB now), a preview is refused.
    let mut set = app.state.settings.get();
    set.max_storage_mb = 1;
    app.state.settings.save(&app.state.db, set).await.unwrap();
    let list = again().await;
    assert_eq!((list[0]["status"].as_str(), &list[0]["preview"]), (Some("failure"), &Value::Null), "{list:?}");
    assert!(list[0]["description"].as_str().unwrap_or_default().contains("storage"), "{list:?}");
    assert_eq!(std::fs::read_dir(app.state.cfg.data_dir.join("ci/previews")).unwrap().count(), 1, "only the live one's files");

    // Closed, its preview's bytes are released.
    assert_eq!(app.req(&root, "PATCH", "/api/repos/root/site/issues/1", Some(json!({ "state": "closed" }))).await.0, StatusCode::OK);
    let after = used().await;
    assert!(after < 1_000_000, "released: {with} -> {after}");
}

#[tokio::test]
async fn the_sweep_stops_counting_a_preview_whose_files_are_gone() {
    let app = App::new();
    let root = app.signup("root").await;
    assert_eq!(app.req(&root, "POST", "/api/repos", Some(json!({ "name": "site" }))).await.0, StatusCode::OK);
    let used = || async { app.state.db.call(|c| Ok(gitter::repo::usage(c, 1)?.0)).await.unwrap() };
    app.state
        .db
        .call(|c| {
            c.execute("INSERT INTO environments (repo_id, name, created_at) VALUES (1, 'preview/pr-1', 0)", [])?;
            c.execute(
                "INSERT INTO deployments (repo_id, environment_id, ref, sha, status, preview, preview_bytes, created_at, updated_at)
                 VALUES (1, 1, 'f', 'x', 'success', 'tok', 5000000, 0, ?1)",
                [gitter::db::now()],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    assert!(used().await >= 5_000_000);
    gitter::api::preview::sweep(&app.state).await;
    let preview: Option<String> = app.state.db.call(|c| Ok(c.query_row("SELECT preview FROM deployments", [], |r| r.get(0))?)).await.unwrap();
    assert_eq!(preview, None);
    assert!(used().await < 5_000_000);
}

#[tokio::test]
async fn a_run_an_agent_started_gets_no_environment_secrets() {
    let World { app, alice, .. } = world().await;
    let env = app.req(&alice, "PUT", "/api/repos/alice/site/environments", Some(json!({ "name": "production" }))).await.1["id"].as_i64().unwrap();
    let path = format!("/api/repos/alice/site/environments/{env}/secrets/KEY");
    assert_eq!(app.req(&alice, "PUT", &path, Some(json!({ "value": "hush" }))).await.0, StatusCode::OK); // pragma: allowlist secret
    let (run, job, repo): (i64, i64, i64) = app
        .state
        .db
        .call(|c| {
            let repo: i64 = c.query_row("SELECT id FROM repos WHERE name = 'site'", [], |r| r.get(0))?;
            c.execute(
                "INSERT INTO ci_runs(repo_id, number, event, ref, sha, title, status, created_at)
                 VALUES (?1, (SELECT coalesce(max(number), 0) + 1 FROM ci_runs WHERE repo_id = ?1), 'push', 'refs/heads/main', ?2, 't', 'running', 0)",
                rusqlite::params![repo, "a".repeat(40)],
            )?;
            let run = c.last_insert_rowid();
            c.execute("INSERT INTO ci_jobs(run_id, name, key, spec, created_at) VALUES (?1, 'j', 'j', '{}', 0)", [run])?;
            Ok((run, c.last_insert_rowid(), repo))
        })
        .await
        .unwrap();
    let spec = gitter::ci::config::parse("jobs:\n  j:\n    environment: production\n    steps: [x]\n").unwrap().jobs.remove(0);
    let lease = gitter::ci::queue::Lease {
        job_id: job,
        attempt: 1,
        token: String::new(),
        run_id: run,
        repo_id: repo,
        repo: "alice/site".into(),
        sha: "a".repeat(40),
        refname: "refs/heads/main".into(),
        event: "push".into(),
        run_number: 1,
        job: spec,
        secrets: vec![],
        limits: Default::default(),
    };
    let mine = gitter::api::deploy::job_started(&app.state, &lease, false).await.unwrap().unwrap();
    assert_eq!(mine, vec![("KEY".to_string(), "hush".to_string())]); // pragma: allowlist secret
    let agents = gitter::api::deploy::job_started(&app.state, &lease, true).await.unwrap().unwrap();
    assert!(agents.is_empty(), "an agent's run gets none: {agents:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn closing_or_merging_a_pull_request_ends_only_its_preview() {
    let app = App::new();
    let root = app.signup("root").await;
    assert_eq!(app.req(&root, "POST", "/api/repos", Some(json!({ "name": "site", "readme": true }))).await.0, StatusCode::OK);
    tokio::spawn(gitter::ci::runner::builtin(app.state.clone()));
    // For pull requests only: a slow preview job, and one deploying the pull request to staging.
    let config = "on: [pull_request]
jobs:
  web:
    preview: public
    steps: [\"sleep 4 && mkdir -p public && echo hi > public/index.html\"]
  stage:
    environment: staging
    steps: [\"echo staged\"]
";
    commit(&app, &root, "root/site", "main", None, ".gitter/ci.yml", config).await;
    commit(&app, &root, "root/site", "main", Some("feature"), "f.txt", "one\n").await;
    let (s, v) = app.req(&root, "POST", "/api/repos/root/site/pulls", Some(json!({ "title": "F", "body": "", "head": "feature", "base": "main" }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    // Closed while the preview is still being built (and once staging is deployed): it never goes up.
    for _ in 0..200 {
        let list = previews(&app, &root).await;
        if list.iter().any(|d| d["environment"] == "preview/pr-1") && list.iter().any(|d| d["environment"] == "staging" && d["status"] == "success") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(previews(&app, &root).await.iter().any(|d| d["environment"] == "preview/pr-1" && d["status"] == "in_progress"), "still building");
    assert_eq!(app.req(&root, "PATCH", "/api/repos/root/site/issues/1", Some(json!({ "state": "closed" }))).await.0, StatusCode::OK);
    settle(&app).await;
    let list = previews(&app, &root).await;
    let of = |name: &str| list.iter().find(|d| d["environment"] == name).cloned().unwrap_or_default();
    assert_eq!((of("preview/pr-1")["status"].as_str(), &of("preview/pr-1")["preview"]), (Some("inactive"), &Value::Null), "{list:?}");
    assert_eq!(of("staging")["status"], "success", "staging keeps what the pull request deployed: {list:?}");
    assert_eq!(std::fs::read_dir(app.state.cfg.data_dir.join("ci/previews")).map(|d| d.count()).unwrap_or(0), 0);

    // Reopened, it previews again; merged by hand (its branch pushed onto main), the preview goes.
    assert_eq!(app.req(&root, "PATCH", "/api/repos/root/site/issues/1", Some(json!({ "state": "open" }))).await.0, StatusCode::OK);
    commit(&app, &root, "root/site", "feature", None, "f.txt", "two\n").await;
    for _ in 0..200 {
        if previews(&app, &root).await.iter().any(|d| d["environment"] == "preview/pr-1" && d["status"] == "success") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let live = previews(&app, &root).await.into_iter().find(|d| d["status"] == "success" && d["environment"] == "preview/pr-1").expect("a new preview");
    let url = live["preview"].as_str().unwrap().to_string();
    assert_eq!(get_page(&app, &url).await.0, StatusCode::OK);
    let id: i64 = app.state.db.call(|c| Ok(c.query_row("SELECT id FROM repos WHERE name = 'site'", [], |r| r.get(0))?)).await.unwrap();
    let path = app.state.git.repo_path(id);
    let tip = app.state.git.resolve(&path, "refs/heads/feature").await.unwrap().unwrap();
    app.state.git.run(&path, &["update-ref", "refs/heads/main", &tip], None, &[]).await.unwrap();
    commit(&app, &root, "root/site", "main", None, "g.txt", "after\n").await;
    let (_, v) = app.req(&root, "GET", "/api/repos/root/site/issues/1", None).await;
    assert_eq!(v["issue"]["state"], "merged", "{v}");
    assert_eq!(get_page(&app, &url).await.0, StatusCode::NOT_FOUND, "merged by hand, the preview goes");
}
