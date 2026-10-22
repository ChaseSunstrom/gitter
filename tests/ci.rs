//! Built-in CI end to end: triggers, the queue, the built-in runner, secrets and artifacts.

mod common;

use axum::http::StatusCode;
use serde_json::json;

use common::{App, Client};

/// Commit one file through the web editor (or onto a new branch).
async fn commit(app: &App, who: &Client, repo: &str, branch: &str, new_branch: Option<&str>, path: &str, content: &str) {
    let mut body = json!({ "branch": branch, "message": format!("Change {path}"), "changes": [{ "path": path, "content": content }] });
    if let Some(b) = new_branch {
        body["new_branch"] = json!(b);
    }
    let (s, v) = app.req(who, "POST", &format!("/api/repos/{repo}/contents"), Some(body)).await;
    assert_eq!(s, StatusCode::OK, "commit {path}: {v}");
}

/// (event, ref, pr number, status, error) of every run, oldest first.
async fn runs(app: &App) -> Vec<(String, String, Option<i64>, String, String)> {
    app.state
        .db
        .call(|c| {
            let mut q = c.prepare("SELECT event, ref, pr_number, status, error FROM ci_runs ORDER BY id")?;
            Ok(q.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?.collect::<Result<_, _>>()?)
        })
        .await
        .unwrap()
}

/// (job name, status) of the newest run's jobs.
async fn last_jobs(app: &App) -> Vec<(String, String)> {
    app.state
        .db
        .call(|c| {
            let mut q = c.prepare("SELECT name, status FROM ci_jobs WHERE run_id = (SELECT max(id) FROM ci_runs) ORDER BY id")?;
            Ok(q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?)
        })
        .await
        .unwrap()
}

fn pairs(v: &[(String, String)]) -> Vec<(&str, &str)> {
    v.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect()
}

const CONFIG: &str = "on:
  push:
    branches: [main]
  pull_request:
jobs:
  test:
    steps: [echo test]
  docs:
    paths: [\"docs/**\"]
    steps: [echo docs]
";

#[tokio::test]
async fn pushes_and_pull_requests_queue_runs() {
    let app = App::new();
    let admin = app.signup("root1").await;
    app.req(&admin, "POST", "/api/repos", Some(json!({ "name": "r", "readme": true }))).await;
    commit(&app, &admin, "root1/r", "main", None, ".gitter/ci.yml", CONFIG).await;
    let r = runs(&app).await;
    assert_eq!(r.len(), 1, "{r:?}");
    assert_eq!((r[0].0.as_str(), r[0].1.as_str()), ("push", "refs/heads/main"));
    assert_eq!(pairs(&last_jobs(&app).await), [("test", "queued"), ("docs", "skipped")], "docs/ didn't change");

    commit(&app, &admin, "root1/r", "main", None, "docs/guide.md", "# Guide").await;
    assert_eq!(pairs(&last_jobs(&app).await), [("test", "queued"), ("docs", "queued")]);

    // A branch push outside `branches` doesn't run; opening a pull request from it does.
    commit(&app, &admin, "root1/r", "main", Some("feature"), "src.txt", "x").await;
    assert_eq!(runs(&app).await.len(), 2);
    let (s, v) = app.req(&admin, "POST", "/api/repos/root1/r/pulls", Some(json!({ "title": "Feature", "head": "feature", "base": "main" }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let r = runs(&app).await;
    assert_eq!((r[2].0.as_str(), r[2].2), ("pull_request", v["number"].as_i64()));
    // New commits on the pull request's branch run it again.
    commit(&app, &admin, "root1/r", "feature", None, "src.txt", "y").await;
    assert_eq!(runs(&app).await.len(), 4);
    assert_eq!(runs(&app).await[3].0, "pull_request");

    // A broken config shows up as a failed run that says why.
    commit(&app, &admin, "root1/r", "main", None, ".gitter/ci.yml", "jobs:\n  a:\n    stepz: [x]\n").await;
    let r = runs(&app).await;
    let last = r.last().unwrap();
    assert_eq!(last.3, "failure");
    assert!(last.4.contains("unknown setting"), "{last:?}");

    // CI can be switched off for the whole site.
    let mut s = app.state.settings.get();
    s.ci = "off".into();
    app.state.settings.save(&app.state.db, s).await.unwrap();
    commit(&app, &admin, "root1/r", "main", None, ".gitter/ci.yml", CONFIG).await;
    assert_eq!(runs(&app).await.len(), r.len());
}

#[tokio::test]
async fn only_admins_repositories_run_ci_by_default() {
    let app = App::new();
    let _admin = app.signup("root1").await;
    let bob = app.signup("bob").await;
    app.req(&bob, "POST", "/api/repos", Some(json!({ "name": "r", "readme": true }))).await;
    commit(&app, &bob, "bob/r", "main", None, ".gitter/ci.yml", CONFIG).await;
    assert!(runs(&app).await.is_empty(), "the default is admins only");
    let mut s = app.state.settings.get();
    s.ci = "everyone".into();
    app.state.settings.save(&app.state.db, s).await.unwrap();
    commit(&app, &bob, "bob/r", "main", None, ".gitter/ci.sh", "echo hi").await;
    assert_eq!(runs(&app).await.len(), 1);
}

#[tokio::test]
async fn schedules_run_once_a_minute_on_the_default_branch() {
    let app = App::new();
    let admin = app.signup("root1").await;
    app.req(&admin, "POST", "/api/repos", Some(json!({ "name": "r", "readme": true }))).await;
    let config = "on:\n  schedule: [\"*/5 * * * *\"]\njobs:\n  nightly:\n    steps: [echo]\n";
    commit(&app, &admin, "root1/r", "main", None, ".gitter/ci.yml", config).await;
    assert!(runs(&app).await.is_empty(), "no push trigger");
    let five = 1_800_000_000 - 1_800_000_000 % 300; // a minute divisible by 5
    gitter::ci::trigger::tick(&app.state, five).await.unwrap();
    gitter::ci::trigger::tick(&app.state, five + 30).await.unwrap();
    gitter::ci::trigger::tick(&app.state, five + 60).await.unwrap();
    let r = runs(&app).await;
    assert_eq!(r.len(), 1, "once for that minute, none for the next: {r:?}");
    assert_eq!((r[0].0.as_str(), r[0].1.as_str()), ("schedule", "refs/heads/main"));
}

/// Wait for the newest run to finish (the built-in runner works in the background).
async fn settle(app: &App) -> String {
    for _ in 0..200 {
        let s: String = app
            .state
            .db
            .call(|c| Ok(c.query_row("SELECT status FROM ci_runs ORDER BY id DESC LIMIT 1", [], |r| r.get(0))?))
            .await
            .unwrap();
        if matches!(s.as_str(), "success" | "failure" | "cancelled" | "skipped") {
            return s;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!("the run didn't finish");
}

#[tokio::test]
async fn the_built_in_runner_runs_steps_masks_secrets_and_keeps_artifacts() {
    let app = App::new();
    let admin = app.signup("root1").await;
    app.req(&admin, "POST", "/api/repos", Some(json!({ "name": "r", "readme": true }))).await;
    let repo_id: i64 = app.state.db.call(|c| Ok(c.query_row("SELECT id FROM repos", [], |r| r.get(0))?)).await.unwrap();
    let sealed = gitter::security::seal(&app.state.key, b"hunter2-secret");
    app.state
        .db
        .call(move |c| Ok(c.execute("INSERT INTO ci_secrets(repo_id, name, value, updated_at) VALUES (?1, 'TOKEN', ?2, 0)", rusqlite::params![repo_id, sealed])?))
        .await
        .unwrap();
    tokio::spawn(gitter::ci::runner::builtin(app.state.clone()));
    let config = "jobs:
  build:
    env:
      GREETING: hello
    steps:
      - name: Greet
        run: echo \"$GREETING from $GITTER_REPOSITORY at $GITTER_BRANCH\"
      - run: echo \"the token is $TOKEN\" && mkdir -p out && echo built > out/app.txt && cat README.md
    artifacts: [\"out/**\"]
  test:
    needs: [build]
    steps:
      - run: echo testing; exit 3
      - run: echo never
";
    commit(&app, &admin, "root1/r", "main", None, ".gitter/ci.yml", config).await;
    assert_eq!(settle(&app).await, "failure");
    let jobs: Vec<(i64, String, String, Option<i32>, String)> = app
        .state
        .db
        .call(|c| {
            let mut q = c.prepare("SELECT id, key, status, exit_code, steps FROM ci_jobs ORDER BY id")?;
            Ok(q.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?.collect::<Result<_, _>>()?)
        })
        .await
        .unwrap();
    assert_eq!((jobs[0].2.as_str(), jobs[1].2.as_str(), jobs[1].3), ("success", "failure", Some(3)), "{jobs:?}");
    let steps: serde_json::Value = serde_json::from_str(&jobs[1].4).unwrap();
    assert_eq!((steps[0]["status"].as_str(), steps[1]["status"].as_str()), (Some("failure"), Some("skipped")));
    let log = std::fs::read_to_string(gitter::ci::jobs::log_path(&app.state.cfg.data_dir, jobs[0].0)).unwrap();
    assert!(log.contains("hello from root1/r at main"), "{log}");
    assert!(log.contains("the token is ***") && !log.contains("hunter2"), "secrets are masked: {log}");
    assert!(log.contains("# r"), "the checkout has the repository's files: {log}");
    let steps: serde_json::Value = serde_json::from_str(&jobs[0].4).unwrap();
    let second = steps[1]["offset"].as_u64().unwrap() as usize;
    assert!(log[second..].contains("the token is") && !log[..second].contains("the token is"), "step offsets split the log");
    // The artifacts archive has the matched files.
    let tgz = std::fs::File::open(gitter::ci::jobs::artifact_path(&app.state.cfg.data_dir, jobs[0].0)).unwrap();
    let mut ar = tar::Archive::new(flate2::read::GzDecoder::new(tgz));
    let names: Vec<String> = ar.entries().unwrap().map(|e| e.unwrap().path().unwrap().display().to_string()).collect();
    assert_eq!(names, ["out/app.txt"]);
    let (s, h, _) = raw_get(&app, &admin, &format!("/api/repos/root1/r/ci/jobs/{}/artifacts", jobs[0].0)).await;
    assert_eq!((s, h["content-type"].to_str().unwrap()), (StatusCode::OK, "application/gzip"));
    let (s, _, _) = raw_get(&app, &Client::default(), &format!("/api/repos/root1/r/ci/jobs/{}/artifacts", jobs[0].0 + 100)).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn jobs_with_an_image_run_in_docker() {
    let have = std::process::Command::new("docker").args(["image", "inspect", "alpine:3.22"]).output().is_ok_and(|o| o.status.success());
    if !have {
        eprintln!("skipped: needs docker with the alpine:3.22 image");
        return;
    }
    let app = App::new();
    let admin = app.signup("root1").await;
    app.req(&admin, "POST", "/api/repos", Some(json!({ "name": "r", "readme": true }))).await;
    tokio::spawn(gitter::ci::runner::builtin(app.state.clone()));
    let config = "jobs:\n  box:\n    image: alpine:3.22\n    env: { WHO: container }\n    steps:\n      - run: cat /etc/alpine-release && echo \"hi $WHO\" && ls README.md && echo made > made.txt\n    artifacts: [made.txt]\n";
    commit(&app, &admin, "root1/r", "main", None, ".gitter/ci.yml", config).await;
    assert_eq!(settle(&app).await, "success");
    let id: i64 = app.state.db.call(|c| Ok(c.query_row("SELECT id FROM ci_jobs", [], |r| r.get(0))?)).await.unwrap();
    let log = std::fs::read_to_string(gitter::ci::jobs::log_path(&app.state.cfg.data_dir, id)).unwrap();
    assert!(log.contains("3.22") && log.contains("hi container"), "{log}");
    assert!(gitter::ci::jobs::artifact_path(&app.state.cfg.data_dir, id).exists());
    let work = app.state.cfg.data_dir.join("ci/work");
    assert_eq!(std::fs::read_dir(&work).unwrap().count(), 0, "the workspace is cleaned up even after root-owned files");
}

#[tokio::test]
async fn trust_job_containers_are_sandboxed() {
    let have = std::process::Command::new("docker").args(["image", "inspect", "alpine:3.22"]).output().is_ok_and(|o| o.status.success());
    if !have {
        eprintln!("skipped: needs docker with the alpine:3.22 image");
        return;
    }
    let app = App::new();
    let admin = app.signup("root1").await;
    app.req(&admin, "POST", "/api/repos", Some(json!({ "name": "r", "readme": true }))).await;
    tokio::spawn(gitter::ci::runner::builtin(app.state.clone()));
    let probe = "grep NoNewPrivs /proc/self/status; echo pids=$(cat /sys/fs/cgroup/pids.max) mem=$(cat /sys/fs/cgroup/memory.max) cpu=$(cat /sys/fs/cgroup/cpu.max); echo nets=$(ls /sys/class/net); ls /var/run/docker.sock 2>/dev/null || echo no-socket";
    let config = format!("jobs:\n  box:\n    image: alpine:3.22\n    network: false\n    steps:\n      - run: {probe}\n");
    commit(&app, &admin, "root1/r", "main", None, ".gitter/ci.yml", &config).await;
    assert_eq!(settle(&app).await, "success");
    let id: i64 = app.state.db.call(|c| Ok(c.query_row("SELECT id FROM ci_jobs", [], |r| r.get(0))?)).await.unwrap();
    let log = std::fs::read_to_string(gitter::ci::jobs::log_path(&app.state.cfg.data_dir, id)).unwrap();
    assert!(log.contains("NoNewPrivs:\t1"), "no new privileges: {log}");
    assert!(log.contains("pids=4096") && log.contains(&format!("mem={}", 4096u64 << 20)) && log.contains("cpu=200000 100000"), "the site's limits: {log}");
    assert!(log.contains("nets=lo\n") && log.contains("no-socket"), "no network, no docker socket: {log}");
}

#[tokio::test]
async fn trust_the_builtin_runner_takes_only_runs_admins_started() {
    let app = App::new();
    let admin = app.signup("root1").await;
    let bob = app.signup("bob").await;
    app.req(&admin, "POST", "/api/repos", Some(json!({ "name": "r", "readme": true }))).await;
    assert_eq!(app.req(&admin, "PUT", "/api/repos/root1/r/collaborators/bob", Some(json!({ "role": "write" }))).await.0, StatusCode::OK);
    set_ci(&app, "admins", 0).await;
    let config = "on:\n  push: {}\n  pull_request: {}\n  schedule: [\"*/5 * * * *\"]\njobs:\n  a:\n    steps: [echo]\n";
    commit(&app, &admin, "root1/r", "main", None, ".gitter/ci.yml", config).await;
    commit(&app, &bob, "root1/r", "main", None, "bob.txt", "b").await;
    let runner = |builtin: bool| {
        let st = app.state.clone();
        async move {
            st.db
                .call(move |c| {
                    c.execute("INSERT INTO ci_runners (name, builtin, created_at) VALUES ('r', ?1, 0)", [builtin])?;
                    Ok(c.last_insert_rowid())
                })
                .await
                .unwrap()
        }
    };
    let (builtin, other) = (runner(true).await, runner(false).await);
    let claim = |id: i64, b: bool| {
        let st = app.state.clone();
        async move { gitter::ci::jobs::claim(&st, id, vec![], b).await.unwrap().map(|l| l.run_number) }
    };
    // A write collaborator's push runs the code they pushed: never on the server itself.
    assert_eq!(claim(builtin, true).await, Some(1), "the admin's push");
    assert_eq!(claim(builtin, true).await, None, "not bob's");
    assert_eq!(claim(other, false).await, Some(2), "a separate runner takes bob's");
    // A scheduled run is on behalf of whoever last moved the branch (bob).
    let five = 1_800_000_000 - 1_800_000_000 % 300;
    gitter::ci::trigger::tick(&app.state, five).await.unwrap();
    let r = runs(&app).await;
    assert_eq!(r.last().map(|r| r.0.as_str()), Some("schedule"), "{r:?}");
    assert_eq!(claim(builtin, true).await, None, "bob last pushed main");
    assert_eq!(claim(other, false).await, Some(3));
    // An admin opening a pull request from bob's branch doesn't vouch for bob's code.
    commit(&app, &bob, "root1/r", "main", Some("feat"), "f.txt", "f").await;
    let (s, v) = app.req(&admin, "POST", "/api/repos/root1/r/pulls", Some(json!({ "title": "Bob's", "head": "feat", "base": "main" }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let actor: Option<String> = app
        .state
        .db
        .call(|c| Ok(c.query_row("SELECT u.username FROM ci_runs r JOIN users u ON u.id = r.actor_id WHERE r.event = 'pull_request' ORDER BY r.id DESC LIMIT 1", [], |r| r.get(0))?))
        .await
        .ok();
    assert_eq!(actor.as_deref(), Some("bob"), "the run is on behalf of the branch's last pusher");
}

#[tokio::test]
async fn a_job_over_its_time_limit_is_killed() {
    let app = App::new();
    let admin = app.signup("root1").await;
    app.req(&admin, "POST", "/api/repos", Some(json!({ "name": "r", "readme": true }))).await;
    tokio::spawn(gitter::ci::runner::builtin(app.state.clone()));
    let config = "jobs:\n  slow:\n    timeout: 2s\n    steps:\n      - run: (sleep 30; echo leaked > /dev/null) & sleep 30\n";
    let started = std::time::Instant::now();
    commit(&app, &admin, "root1/r", "main", None, ".gitter/ci.yml", config).await;
    assert_eq!(settle(&app).await, "failure");
    assert!(started.elapsed().as_secs() < 15, "stopped at the limit, not after the sleep");
    let id: i64 = app.state.db.call(|c| Ok(c.query_row("SELECT id FROM ci_jobs", [], |r| r.get(0))?)).await.unwrap();
    let log = std::fs::read_to_string(gitter::ci::jobs::log_path(&app.state.cfg.data_dir, id)).unwrap();
    assert!(log.contains("ran out of time"), "{log}");
}

async fn raw_get(app: &App, who: &Client, path: &str) -> (StatusCode, axum::http::HeaderMap, String) {
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    let mut b = axum::http::Request::get(path).header("host", "test.local");
    if let Some(c) = &who.0 {
        b = b.header("cookie", format!("gitter_session={c}"));
    }
    let resp = app.router.clone().oneshot(b.body(axum::body::Body::empty()).unwrap()).await.unwrap();
    let (s, h) = (resp.status(), resp.headers().clone());
    (s, h, String::from_utf8_lossy(&resp.into_body().collect().await.unwrap().to_bytes()).into_owned())
}

#[tokio::test]
async fn secrets_are_encrypted_and_runs_can_be_rerun_and_cancelled() {
    let app = App::new();
    let admin = app.signup("root1").await;
    let bob = app.signup("bob").await;
    app.req(&admin, "POST", "/api/repos", Some(json!({ "name": "r", "readme": true }))).await;
    // Secrets: write-only, encrypted at rest, for writers only.
    let (s, v) = app.req(&admin, "PUT", "/api/repos/root1/r/ci/secrets/API_KEY", Some(json!({ "value": "s3cr3t-value" }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(app.req(&admin, "PUT", "/api/repos/root1/r/ci/secrets/bad-name", Some(json!({ "value": "x" }))).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(app.req(&admin, "PUT", "/api/repos/root1/r/ci/secrets/GITTER_X", Some(json!({ "value": "x" }))).await.0, StatusCode::BAD_REQUEST);
    assert_ne!(app.req(&bob, "PUT", "/api/repos/root1/r/ci/secrets/API_KEY", Some(json!({ "value": "x" }))).await.0, StatusCode::OK);
    assert_ne!(app.req(&bob, "GET", "/api/repos/root1/r/ci/secrets", None).await.0, StatusCode::OK);
    let (_, list) = app.req(&admin, "GET", "/api/repos/root1/r/ci/secrets", None).await;
    assert_eq!(list["secrets"][0]["name"], "API_KEY");
    assert!(!list.to_string().contains("s3cr3t"), "values are never shown");
    let stored: Vec<u8> = app.state.db.call(|c| Ok(c.query_row("SELECT value FROM ci_secrets", [], |r| r.get(0))?)).await.unwrap();
    assert!(!String::from_utf8_lossy(&stored).contains("s3cr3t"), "encrypted at rest");

    tokio::spawn(gitter::ci::runner::builtin(app.state.clone()));
    let flag = app.state.cfg.data_dir.join("ci/flag");
    let config = "jobs:\n  check:\n    steps:\n      - run: echo \"key=$API_KEY\" && test -f \"$HOME/../../flag\"\n";
    commit(&app, &admin, "root1/r", "main", None, ".gitter/ci.yml", config).await;
    assert_eq!(settle(&app).await, "failure");
    let (_, list) = app.req(&bob, "GET", "/api/repos/root1/r/ci/runs", None).await;
    assert_eq!((list["total"].as_i64(), list["runs"][0]["status"].as_str()), (Some(1), Some("failure")), "{list}");
    let (_, run) = app.req(&bob, "GET", "/api/repos/root1/r/ci/runs/1", None).await;
    let job = run["jobs"][0]["id"].as_i64().unwrap();
    assert_eq!(run["jobs"][0]["steps"][0]["status"], "failure", "{run}");
    let (s, h, log) = raw_get(&app, &bob, &format!("/api/repos/root1/r/ci/jobs/{job}/log")).await;
    assert_eq!((s, h["x-job-status"].to_str().unwrap()), (StatusCode::OK, "failure"));
    assert!(log.contains("key=***") && !log.contains("s3cr3t"), "{log}");
    let (_, _, tail) = raw_get(&app, &bob, &format!("/api/repos/root1/r/ci/jobs/{job}/log?offset={}", log.len() - 10)).await;
    assert_eq!(tail.len(), 10);
    assert_eq!(raw_get(&app, &bob, &format!("/api/repos/root1/r/ci/jobs/{job}/artifacts")).await.0, StatusCode::NOT_FOUND);
    // Only writers re-run; the re-run passes once the flag exists.
    assert_ne!(app.req(&bob, "POST", "/api/repos/root1/r/ci/runs/1/rerun", Some(json!({}))).await.0, StatusCode::OK);
    std::fs::write(&flag, "ok").unwrap();
    assert_eq!(app.req(&admin, "POST", "/api/repos/root1/r/ci/runs/1/rerun", Some(json!({}))).await.0, StatusCode::OK);
    assert_eq!(settle(&app).await, "success");
    let (_, run) = app.req(&admin, "GET", "/api/repos/root1/r/ci/runs/1", None).await;
    assert_eq!(run["jobs"][0]["attempt"], 1, "a re-run starts the job's attempts over");
    let sha = run["sha"].as_str().unwrap().to_string();
    let (_, st) = app.req(&bob, "GET", &format!("/api/repos/root1/r/ci/status?shas={sha},{}", "0".repeat(40)), None).await;
    assert_eq!(st[&sha]["status"], "success", "{st}");
    assert!(st.get("0".repeat(40)).is_none());

    // The Run button, and cancelling.
    let waiting = "on: [manual]\njobs:\n  wait:\n    steps: [\"sleep 30\"]\n";
    commit(&app, &admin, "root1/r", "main", None, ".gitter/ci.yml", waiting).await;
    let (s, v) = app.req(&admin, "POST", "/api/repos/root1/r/ci/runs", Some(json!({}))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let n = v["number"].as_i64().unwrap();
    let path = format!("/api/repos/root1/r/ci/runs/{n}/cancel");
    assert_eq!(app.req(&admin, "POST", &path, None).await.0, StatusCode::OK);
    let (_, run) = app.req(&admin, "GET", &format!("/api/repos/root1/r/ci/runs/{n}"), None).await;
    assert_eq!(run["status"], "cancelled");
}

#[tokio::test]
async fn admins_register_runners_with_one_time_tokens() {
    use tower::ServiceExt;
    let app = App::new();
    let admin = app.signup("root1").await;
    let bob = app.signup("bob").await;
    assert_ne!(app.req(&bob, "POST", "/api/admin/runners", Some(json!({ "name": "x" }))).await.0, StatusCode::OK);
    let (s, v) = app.req(&admin, "POST", "/api/admin/runners", Some(json!({ "name": "big box" }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert!(v["token"].as_str().unwrap().starts_with("grn_"));
    let (_, list) = app.req(&admin, "GET", "/api/admin/runners", None).await;
    assert_eq!(list["runners"][0]["name"], "big box");
    assert!(!list.to_string().contains("grn_"), "the token is shown once");
    // The runner API refuses anything but a runner token.
    let claim = axum::http::Request::post("/ci-runner/claim")
        .header("host", "test.local")
        .header("authorization", "Bearer grn_nope")
        .header("content-type", "application/json")
        .body(axum::body::Body::from("{}"))
        .unwrap();
    assert_eq!(app.router.clone().oneshot(claim).await.unwrap().status(), StatusCode::UNAUTHORIZED);
    let id = v["id"].as_i64().unwrap();
    assert_eq!(app.req(&admin, "DELETE", &format!("/api/admin/runners/{id}"), None).await.0, StatusCode::OK);
}

async fn set_ci(app: &App, mode: &str, builtin_jobs: u32) {
    let mut s = app.state.settings.get();
    s.ci = mode.into();
    s.builtin_runner_jobs = builtin_jobs;
    app.state.settings.save(&app.state.db, s).await.unwrap();
}

async fn job_status(app: &App) -> Vec<String> {
    app.state
        .db
        .call(|c| {
            let mut q = c.prepare("SELECT status FROM ci_jobs ORDER BY id")?;
            Ok(q.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?)
        })
        .await
        .unwrap()
}

#[tokio::test]
async fn a_background_process_cant_hang_its_job() {
    let app = App::new();
    let admin = app.signup("root1").await;
    app.req(&admin, "POST", "/api/repos", Some(json!({ "name": "r", "readme": true }))).await;
    tokio::spawn(gitter::ci::runner::builtin(app.state.clone()));
    // The first step leaves a process holding stdout; the job must still finish promptly.
    let config = "jobs:\n  bg:\n    steps:\n      - run: (sleep 60; echo late) & echo started\n      - run: echo next\n";
    let t = std::time::Instant::now();
    commit(&app, &admin, "root1/r", "main", None, ".gitter/ci.yml", config).await;
    assert_eq!(settle(&app).await, "success");
    assert!(t.elapsed().as_secs() < 20, "took {:?}", t.elapsed());
    let id: i64 = app.state.db.call(|c| Ok(c.query_row("SELECT id FROM ci_jobs", [], |r| r.get(0))?)).await.unwrap();
    let log = std::fs::read_to_string(gitter::ci::jobs::log_path(&app.state.cfg.data_dir, id)).unwrap();
    assert!(log.contains("\nstarted") && log.contains("\nnext") && !log.contains("\nlate"), "{log}");
}

#[tokio::test]
async fn who_and_what_may_run_jobs() {
    let app = App::new();
    let admin = app.signup("root1").await;
    let bob = app.signup("bob").await;
    app.req(&admin, "POST", "/api/repos", Some(json!({ "name": "r", "readme": true }))).await;
    set_ci(&app, "admins", 0).await;
    commit(&app, &admin, "root1/r", "main", None, ".gitter/ci.yml", "jobs:\n  a:\n    steps: [echo]\n").await;
    // Someone who can't push opening a pull request doesn't start a run (runs get the secrets).
    commit(&app, &admin, "root1/r", "main", Some("feature"), "x.txt", "x").await;
    let before = runs(&app).await.len();
    let (s, v) = app.req(&bob, "POST", "/api/repos/root1/r/pulls", Some(json!({ "title": "From bob", "head": "feature", "base": "main" }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(runs(&app).await.len(), before, "readers' pull requests don't run CI");
    // CI off means nothing is handed out, even jobs already queued.
    let runner: i64 = app
        .state
        .db
        .call(|c| {
            c.execute("INSERT INTO ci_runners(name, labels, created_at) VALUES ('box', '[]', 0)", [])?;
            Ok(c.last_insert_rowid())
        })
        .await
        .unwrap();
    set_ci(&app, "off", 0).await;
    assert!(gitter::ci::jobs::claim(&app.state, runner, vec![], false).await.unwrap().is_none());
    // Under "everyone", other runners take anyone's jobs, but the built-in one (the server itself) only admins'.
    set_ci(&app, "everyone", 0).await;
    app.req(&bob, "POST", "/api/repos", Some(json!({ "name": "mine", "readme": true }))).await;
    commit(&app, &bob, "bob/mine", "main", None, ".gitter/ci.yml", "jobs:\n  b:\n    steps: [echo]\n").await;
    app.state.db.call(|c| Ok(c.execute("UPDATE ci_jobs SET status = 'success' WHERE run_id IN (SELECT r.id FROM ci_runs r JOIN repos p ON p.id = r.repo_id WHERE p.name = 'r')", [])?)).await.unwrap();
    assert!(gitter::ci::jobs::claim(&app.state, runner, vec![], true).await.unwrap().is_none(), "the built-in runner skips bob's job");
    let lease = gitter::ci::jobs::claim(&app.state, runner, vec![], false).await.unwrap().expect("a separate runner takes it");
    assert_eq!(lease.repo, "bob/mine");
    // The runner API won't buffer a huge body before checking the token.
    use tower::ServiceExt;
    let big = axum::http::Request::post("/ci-runner/claim")
        .header("host", "test.local")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(vec![b' '; 3 << 20]))
        .unwrap();
    assert_eq!(app.router.clone().oneshot(big).await.unwrap().status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn multi_line_secrets_are_masked_and_streams_resume() {
    let app = App::new();
    let admin = app.signup("root1").await;
    app.req(&admin, "POST", "/api/repos", Some(json!({ "name": "r", "readme": true }))).await;
    let pem = "-----BEGIN KEY-----\nMIIBsecretline1\nMIIBsecretline2\n-----END KEY-----";
    let (s, _) = app.req(&admin, "PUT", "/api/repos/root1/r/ci/secrets/PEM", Some(json!({ "value": pem }))).await;
    assert_eq!(s, StatusCode::OK);
    tokio::spawn(gitter::ci::runner::builtin(app.state.clone()));
    commit(&app, &admin, "root1/r", "main", None, ".gitter/ci.yml", "jobs:\n  k:\n    steps:\n      - run: printf '%s\\n' \"$PEM\" && echo tail-marker\n").await;
    assert_eq!(settle(&app).await, "success");
    let id: i64 = app.state.db.call(|c| Ok(c.query_row("SELECT id FROM ci_jobs", [], |r| r.get(0))?)).await.unwrap();
    let (_, _, log) = raw_get(&app, &admin, &format!("/api/repos/root1/r/ci/jobs/{id}/log")).await;
    assert!(!log.contains("secretline"), "{log}");
    // A reconnecting browser sends Last-Event-ID (a byte offset) and gets only what's after it.
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    let from = log.find("tail-marker").unwrap();
    let req = axum::http::Request::get(format!("/api/repos/root1/r/ci/jobs/{id}/log/stream"))
        .header("host", "test.local")
        .header("cookie", format!("gitter_session={}", admin.0.clone().unwrap()))
        .header("last-event-id", from.to_string())
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.router.clone().oneshot(req).await.unwrap();
    let body = String::from_utf8_lossy(&resp.into_body().collect().await.unwrap().to_bytes()).into_owned();
    assert!(body.contains("tail-marker") && !body.contains("BEGIN"), "{body}");
    // The hourly sweep removes files of jobs that are gone.
    let orphan = gitter::ci::jobs::log_path(&app.state.cfg.data_dir, 9999);
    std::fs::write(&orphan, "x").unwrap();
    gitter::ci::jobs::sweep(&app.state).await.unwrap();
    assert!(!orphan.exists() && gitter::ci::jobs::log_path(&app.state.cfg.data_dir, id).exists());
    assert_eq!(job_status(&app).await, ["success"]);
}

// ---------- other hosts' CI files (T-0097)

/// Every job's log of the newest run, joined.
async fn run_logs(app: &App) -> String {
    let ids: Vec<i64> = app
        .state
        .db
        .call(|c| Ok(c.prepare("SELECT id FROM ci_jobs WHERE run_id = (SELECT max(id) FROM ci_runs) ORDER BY id")?.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?))
        .await
        .unwrap();
    ids.iter().map(|id| std::fs::read_to_string(gitter::ci::jobs::log_path(&app.state.cfg.data_dir, *id)).unwrap_or_default()).collect::<Vec<_>>().join("\n")
}

#[tokio::test]
async fn compat_github_workflows_run_and_each_event_runs_only_its_workflows() {
    let app = App::new();
    let admin = app.signup("root1").await;
    app.req(&admin, "POST", "/api/repos", Some(json!({ "name": "r", "readme": true }))).await;
    tokio::spawn(gitter::ci::runner::builtin(app.state.clone()));
    let prs = "on: pull_request\njobs:\n  review:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo review\n";
    commit(&app, &admin, "root1/r", "main", None, ".github/workflows/pr.yml", prs).await;
    assert!(runs(&app).await.is_empty(), "a pull-request-only workflow doesn't run on a push");
    let ci = "name: CI
on:
  push:
    branches: [main]
  pull_request:
env:
  GREETING: hi
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
      - run: |
          echo \"$GREETING from $GITHUB_REPOSITORY at ${{ github.ref_name }}\"
          echo \"X=1 2\" >> $GITHUB_ENV
      - run: echo \"x is $X\"
      - if: github.event_name == 'pull_request'
        run: echo only-on-prs
  after:
    needs: build
    runs-on: ubuntu-latest
    steps:
      - run: echo after
";
    commit(&app, &admin, "root1/r", "main", None, ".github/workflows/ci.yml", ci).await;
    assert_eq!(settle(&app).await, "success", "{:?}", runs(&app).await);
    assert_eq!(pairs(&last_jobs(&app).await), [("CI / build", "success"), ("CI / after", "success")], "pr.yml doesn't run on a push");
    let log = run_logs(&app).await;
    assert!(log.contains("hi from root1/r at main") && log.contains("x is 1 2"), "{log}");
    assert!(log.contains("Notes on .github/workflows/ci.yml") && log.contains("actions/setup-node@v4"), "{log}");
    assert!(log.contains("Skipped (if: github.event_name == 'pull_request')") && !log.contains("only-on-prs\n"), "{log}");
    // A pull request runs every workflow that wants pull requests.
    commit(&app, &admin, "root1/r", "main", Some("feat"), "f.txt", "x\n").await;
    let (s, _) = app.req(&admin, "POST", "/api/repos/root1/r/pulls", Some(json!({ "title": "F", "head": "feat", "base": "main" }))).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(settle(&app).await, "success");
    let last = runs(&app).await.pop().unwrap();
    assert_eq!(last.0, "pull_request");
    assert_eq!(pairs(&last_jobs(&app).await), [("CI / build", "success"), ("CI / after", "success"), ("pr / review", "success")]);
    assert!(run_logs(&app).await.contains("only-on-prs"));
    // A .gitter/ci.yml wins over them.
    commit(&app, &admin, "root1/r", "main", None, ".gitter/ci.yml", "jobs:\n  own:\n    steps: [echo own]\n").await;
    assert_eq!(settle(&app).await, "success");
    assert_eq!(pairs(&last_jobs(&app).await), [("own", "success")]);
}

#[tokio::test]
async fn compat_gitlab_ci_runs_in_stage_order_and_rules_pick_jobs() {
    let app = App::new();
    let admin = app.signup("root1").await;
    app.req(&admin, "POST", "/api/repos", Some(json!({ "name": "g", "readme": true }))).await;
    tokio::spawn(gitter::ci::runner::builtin(app.state.clone()));
    let gl = "stages: [build, test]
default:
  before_script:
    - echo before
.common: &common
  variables:
    WHO: gitlab
build:
  stage: build
  <<: *common
  script:
    - echo \"built by $WHO\" > built.txt
    - cat built.txt
test:
  stage: test
  script: echo \"testing $CI_COMMIT_BRANCH from $CI_PIPELINE_SOURCE\"
mr only:
  stage: test
  script: echo mr
  rules:
    - if: $CI_PIPELINE_SOURCE == \"merge_request_event\"
";
    commit(&app, &admin, "root1/g", "main", None, ".gitlab-ci.yml", gl).await;
    assert_eq!(settle(&app).await, "success", "{:?}", runs(&app).await);
    assert_eq!(pairs(&last_jobs(&app).await), [("build", "success"), ("test", "success")], "the merge-request job sits out a push");
    let log = run_logs(&app).await;
    assert!(log.contains("before") && log.contains("built by gitlab") && log.contains("testing main from push"), "{log}");
    // A broken file shows on the run, as a .gitter/ci.yml would.
    commit(&app, &admin, "root1/g", "main", None, ".gitlab-ci.yml", "x:\n  stage: nope\n  script: [a]\n").await;
    let last = runs(&app).await.pop().unwrap();
    assert!(last.3 == "failure" && last.4.contains("stage 'nope'"), "{last:?}");
}
