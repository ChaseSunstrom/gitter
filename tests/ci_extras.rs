//! CI extras: caches, JUnit test reports with annotations, coverage, flaky tests.

mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};

use common::{App, Client};
use gitter::ci::cache::{self, Scope};
use gitter::ci::reports;

/// Commit one file through the web editor (or onto a new branch).
async fn commit(app: &App, who: &Client, repo: &str, branch: &str, new_branch: Option<&str>, path: &str, content: &str) {
    let mut body = json!({ "branch": branch, "message": format!("Change {path}"), "changes": [{ "path": path, "content": content }] });
    if let Some(b) = new_branch {
        body["new_branch"] = json!(b);
    }
    let (s, v) = app.req(who, "POST", &format!("/api/repos/{repo}/contents"), Some(body)).await;
    assert_eq!(s, StatusCode::OK, "commit {path}: {v}");
}

/// Wait for the newest run to finish (runners work in the background).
async fn settle(app: &App) -> String {
    for _ in 0..300 {
        let s: String = app.state.db.call(|c| Ok(c.query_row("SELECT status FROM ci_runs ORDER BY id DESC LIMIT 1", [], |r| r.get(0))?)).await.unwrap();
        if matches!(s.as_str(), "success" | "failure" | "cancelled" | "skipped") {
            return s;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!("the run didn't finish");
}

/// The log of the newest run's first job.
async fn last_log(app: &App) -> String {
    let id: i64 = app.state.db.call(|c| Ok(c.query_row("SELECT max(id) FROM ci_jobs", [], |r| r.get(0))?)).await.unwrap();
    std::fs::read_to_string(gitter::ci::jobs::log_path(&app.state.cfg.data_dir, id)).unwrap_or_default()
}

/// The `cache=[…]` line a job printed: what its restored cache held.
fn seen(log: &str) -> String {
    log.lines().find_map(|l| l.strip_prefix("cache=[")).map(|l| l.trim_end_matches(']').trim().to_string()).unwrap_or_else(|| panic!("{log}"))
}

async fn caches(app: &App) -> Vec<(String, String)> {
    app.state
        .db
        .call(|c| {
            let mut q = c.prepare("SELECT ref, key FROM ci_caches ORDER BY id")?;
            Ok(q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?)
        })
        .await
        .unwrap()
}

#[tokio::test]
async fn caches_restore_by_key_template_and_fall_back_to_the_default_branch() {
    let app = App::new();
    let admin = app.signup("root1").await;
    app.req(&admin, "POST", "/api/repos", Some(json!({ "name": "r", "readme": true }))).await;
    tokio::spawn(gitter::ci::runner::builtin(app.state.clone()));
    commit(&app, &admin, "root1/r", "main", None, "lock.txt", "1\n").await;
    let config = "jobs:
  build:
    cache:
      key: deps-{{ hash lock.txt }}
      paths: [deps]
      restore-keys: [deps-]
    steps:
      - run: echo \"cache=[$(cat deps/x 2>/dev/null | tr '\\n' ' ')]\" && mkdir -p deps && echo $GITTER_BRANCH >> deps/x
";
    commit(&app, &admin, "root1/r", "main", None, ".gitter/ci.yml", config).await;
    assert_eq!(settle(&app).await, "success");
    assert_eq!(seen(&last_log(&app).await), "", "nothing to restore yet");
    let c = caches(&app).await;
    assert_eq!(c.len(), 1, "{c:?}");
    let k1 = c[0].1.clone();
    assert!(k1.starts_with("deps-") && k1.len() == 5 + 64, "the template hashed lock.txt: {k1}");

    // Same lock file on a new branch: the default branch's cache, and no second copy.
    commit(&app, &admin, "root1/r", "main", Some("feature"), "a.txt", "a").await;
    assert_eq!(settle(&app).await, "success");
    assert_eq!(seen(&last_log(&app).await), "main");
    assert_eq!(caches(&app).await.len(), 1, "an exact hit isn't saved again");

    // A changed lock file falls back on the prefix, then saves under the branch.
    commit(&app, &admin, "root1/r", "feature", None, "lock.txt", "2\n").await;
    assert_eq!(settle(&app).await, "success");
    assert_eq!(seen(&last_log(&app).await), "main");
    let c = caches(&app).await;
    assert_eq!(c.len(), 2);
    let k2 = c[1].1.clone();
    assert_eq!(c[1].0, "refs/heads/feature");
    assert_ne!(k1, k2);

    // The default branch never restores a branch's cache, even with the same key.
    commit(&app, &admin, "root1/r", "main", None, "lock.txt", "2\n").await;
    assert_eq!(settle(&app).await, "success");
    assert_eq!(seen(&last_log(&app).await), "main", "main's own older cache, not feature's");
    let c = caches(&app).await;
    assert_eq!((c.len(), c[2].0.as_str(), c[2].1.as_str()), (3, "refs/heads/main", k2.as_str()));
    let ids: Vec<i64> = app.state.db.call(|c| Ok(c.prepare("SELECT id FROM ci_caches ORDER BY id")?.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?)).await.unwrap();
    for id in &ids {
        assert!(cache::path(&app.state.cfg.data_dir, *id).exists());
    }

    // Over the per-repository cap, the least recently used go first (files too).
    app.state.db.call(|c| Ok(c.execute("UPDATE ci_caches SET size = 100, used_at = CAST(strftime('%s', 'now') AS INTEGER) + id", [])?)).await.unwrap();
    let gone = app.state.db.call(|c| Ok(cache::evict(c, 1, 150)?)).await.unwrap();
    assert_eq!(gone, ids[..2].to_vec(), "oldest use first, until under the cap");
    assert_eq!(caches(&app).await.len(), 1);
    gitter::ci::cache::sweep(&app.state).await.unwrap();
    assert!(!cache::path(&app.state.cfg.data_dir, ids[0]).exists() && cache::path(&app.state.cfg.data_dir, ids[2]).exists());
}

#[tokio::test]
async fn caches_never_cross_into_more_trusted_runs() {
    let app = App::new();
    app.signup("root1").await;
    app.state
        .db
        .call(|c| {
            c.execute("INSERT INTO repos(owner_id, name, created_at, updated_at) VALUES (1, 'r', 0, 0)", [])?;
            for (i, (r, trust, key)) in [("refs/heads/main", 0, "k"), ("refs/heads/pr", 1, "k"), ("refs/heads/main", 2, "k-old"), ("refs/heads/pr", 0, "k-x")].iter().enumerate() {
                c.execute(
                    "INSERT INTO ci_caches(repo_id, ref, trust, key, size, created_at, used_at) VALUES (1, ?1, ?2, ?3, 1, ?4, ?4)",
                    rusqlite::params![r, trust, key, i as i64],
                )?;
            }
            let look = |refname: &str, trust: i64, key: &str, restore: &[&str]| {
                let s = Scope { repo_id: 1, refname: refname.into(), default_ref: "refs/heads/main".into(), trust };
                cache::lookup(c, &s, key, &restore.iter().map(|x| x.to_string()).collect::<Vec<_>>()).unwrap().map(|x| x.1)
            };
            // An admin's run of main: only caches admins' runs wrote on main.
            assert_eq!(look("refs/heads/main", cache::ADMIN, "k", &[]), None, "not the agent's");
            assert_eq!(look("refs/heads/main", cache::ADMIN, "k", &["k-"]).as_deref(), Some("k-old"));
            // A person's run of a branch: its own, or main's from people or admins; never an agent's.
            assert_eq!(look("refs/heads/pr", cache::USER, "k", &[]).as_deref(), Some("k"));
            assert_eq!(look("refs/heads/pr", cache::USER, "k-", &["k-"]).as_deref(), Some("k-old"), "k-x is an agent's");
            // An agent's run may read anything in its scope.
            assert_eq!(look("refs/heads/pr", cache::AGENT, "k-x", &[]).as_deref(), Some("k-x"));
            // Another branch never sees pr's.
            assert_eq!(look("refs/heads/other", cache::AGENT, "k-x", &[]), None);
            Ok(())
        })
        .await
        .unwrap();
}

#[test]
fn key_templates_and_cache_config() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Cargo.lock"), "x").unwrap();
    std::fs::create_dir_all(dir.path().join("sub")).unwrap();
    std::fs::write(dir.path().join("sub/Cargo.lock"), "y").unwrap();
    let a = cache::expand_key("deps-{{ hash Cargo.lock }}-{{ os }}", dir.path());
    assert!(a.starts_with("deps-") && a.ends_with(&format!("-{}", std::env::consts::OS)), "{a}");
    // Patterns work as in `artifacts`: a bare name matches at any depth.
    let b = cache::expand_key("deps-{{ hash sub/Cargo.lock }}", dir.path());
    assert_ne!(a[..69], b[..], "a bare name covers both lock files");
    assert_eq!(b, cache::expand_key("deps-{{ hash sub/*.lock }}", dir.path()));
    std::fs::write(dir.path().join("Cargo.lock"), "changed").unwrap();
    assert_ne!(cache::expand_key("deps-{{ hash Cargo.lock }}-{{ os }}", dir.path()), a);
    assert_eq!(cache::expand_key("a b/c", dir.path()), "a-b-c", "other characters become dashes");
    assert!(cache::check_key("deps-{{ nope x }}").is_err());
    assert!(cache::check_key("deps-{{ hash").is_err());
    assert!(cache::check_key("").is_err());
    let p = gitter::ci::config::parse(
        "jobs:\n  t:\n    matrix: { v: [1, 2] }\n    cache:\n      key: k-${{ matrix.v }}-{{ hash a }}\n      paths: [target/, .cargo]\n    reports: [\"junit/*.xml\"]\n    coverage: lcov.info\n    steps: [x]\n",
    )
    .unwrap();
    let c = p.jobs[1].cache.clone().unwrap();
    assert_eq!((c.key.as_str(), c.paths.clone()), ("k-2-{{ hash a }}", vec!["target".to_string(), ".cargo".into()]));
    assert_eq!((p.jobs[0].reports.clone(), p.jobs[0].coverage.clone()), (vec!["junit/*.xml".to_string()], vec!["lcov.info".to_string()]));
    for bad in ["paths: [/etc]", "paths: [../x]", "paths: [\"~/.cargo\"]", "paths: []", "paths: [.git]"] {
        let y = format!("jobs:\n  t:\n    cache:\n      key: k\n      {bad}\n    steps: [x]\n");
        assert!(gitter::ci::config::parse(&y).is_err(), "{bad}");
    }
    // Specs stored before caches existed still load.
    let old = r#"{"name":"a","key":"a","image":null,"needs":[],"env":{},"paths":[],"timeout_secs":60,"steps":[],"artifacts":[],"runs_on":[]}"#;
    let j: gitter::ci::config::Job = serde_json::from_str(old).unwrap();
    assert!(j.cache.is_none() && j.reports.is_empty());
}

const FAILING: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!-- made by a test runner -->
<testsuites>
  <testsuite name="math" tests="4">
    <testcase classname="math" name="adds" file="./src/lib.rs" line="3"><failure message="expected 4, got 5">assertion failed</failure></testcase>
    <testcase classname="math" name="subtracts" time="0.1"/>
    <testcase classname="math" name="later"><skipped/></testcase>
    <testcase classname="io" name="reads"><error message="boom &amp; bust &#x263A;"><![CDATA[thread panicked at /workspace/tests/io.rs:12:5: <oops>]]></error></testcase>
  </testsuite>
</testsuites>
"#;

const PASSING: &str = r#"<testsuites><testsuite name="math">
  <testcase classname="math" name="adds"/><testcase classname="math" name="subtracts"/>
  <testcase classname="math" name="later"><skipped/></testcase><testcase classname="io" name="reads"/>
</testsuite></testsuites>"#;

#[test]
fn junit_and_coverage_parsing_is_bounded() {
    let cases = reports::parse_junit(FAILING.as_bytes()).unwrap();
    let st: Vec<(&str, &str, &str)> = cases.iter().map(|c| (c.suite.as_str(), c.name.as_str(), c.status)).collect();
    assert_eq!(st, [("math", "adds", "failed"), ("math", "subtracts", "passed"), ("math", "later", "skipped"), ("io", "reads", "failed")]);
    assert_eq!((cases[0].file.as_deref(), cases[0].line, cases[0].message.as_str()), (Some("src/lib.rs"), Some(3), "expected 4, got 5\nassertion failed"));
    assert_eq!((cases[3].file.as_deref(), cases[3].line), (Some("tests/io.rs"), Some(12)), "file:line found in the output");
    assert!(cases[3].message.starts_with("boom & bust ☺\nthread panicked") && cases[3].message.contains("<oops>"), "{}", cases[3].message);
    // A rerun inside one report (surefire, pytest-rerunfailures) is flaky too.
    let r = reports::parse_junit(b"<testsuite><testcase name=\"t\"><flakyFailure message=\"x\"/></testcase></testsuite>").unwrap();
    assert!(r[0].flaky && r[0].status == "passed");
    // No DTDs (so no entity expansion), no runaway nesting, no unknown entities expanded.
    let lol = "<?xml version=\"1.0\"?><!DOCTYPE lolz [<!ENTITY lol \"lol\"><!ENTITY lol2 \"&lol;&lol;\">]><testsuite><testcase name=\"&lol2;\"/></testsuite>";
    assert!(reports::parse_junit(lol.as_bytes()).is_err());
    let deep = format!("<testsuite>{}{}</testsuite>", "<a>".repeat(10_000), "</a>".repeat(10_000));
    assert!(reports::parse_junit(deep.as_bytes()).is_err());
    let many = format!("<testsuite>{}</testsuite>", "<x/>".repeat(1_000_001));
    assert!(reports::parse_junit(many.as_bytes()).is_err());
    assert_eq!(reports::parse_junit(b"<testsuite><testcase name=\"a&foo;b\"/></testsuite>").unwrap()[0].name, "a&foo;b");
    assert!(reports::parse_junit(b"<testsuite><testcase name=\"a\"></testsuite>").is_err(), "mismatched tags");
    assert!(reports::parse_junit(b"not xml at all").is_err());
    // Coverage: Cobertura counts or rate, LCOV totals.
    assert_eq!(reports::parse_coverage(br#"<?xml version="1.0"?><coverage line-rate="0.8125" lines-covered="13" lines-valid="16"><packages/></coverage>"#), Some((13.0, 16.0)));
    assert_eq!(reports::parse_coverage(br#"<coverage line-rate="0.5"></coverage>"#), Some((50.0, 100.0)));
    assert_eq!(reports::parse_coverage(b"TN:\nSF:src/a.rs\nDA:1,1\nLF:4\nLH:3\nend_of_record\nSF:b\nLF:6\nLH:1\nend_of_record\n"), Some((4.0, 10.0)));
    assert_eq!(reports::parse_coverage(b"<coverage/>"), None);
    assert_eq!(reports::parse_coverage(b"hello"), None);
}

/// A test server on a real port, for a `gitter runner` (`Backend::Remote`).
async fn serve(app: &App) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let router = app.router.clone();
    tokio::spawn(async move {
        let _ = axum::serve(listener, router.into_make_service_with_connect_info::<std::net::SocketAddr>()).await;
    });
    url
}

#[tokio::test]
async fn reports_through_a_remote_runner_show_failures_coverage_and_flaky_tests() {
    let app = App::new();
    let admin = app.signup("root1").await;
    app.req(&admin, "POST", "/api/repos", Some(json!({ "name": "r", "readme": true }))).await;
    let (_, v) = app.req(&admin, "POST", "/api/admin/runners", Some(json!({ "name": "remote" }))).await;
    let token = v["token"].as_str().unwrap().to_string();
    let url = serve(&app).await;
    let work = tempfile::tempdir().unwrap();
    let marks = tempfile::tempdir().unwrap();
    let runner = gitter::ci::runner::Runner {
        backend: std::sync::Arc::new(gitter::ci::runner::Backend::remote(&url, &token)),
        labels: vec![],
        work: work.path().to_path_buf(),
    };
    tokio::spawn(gitter::ci::runner::serve(runner, || 1, None));

    // Fails the first time it runs on a runner, passes after (a flaky test).
    commit(&app, &admin, "root1/r", "main", None, "fail.xml", FAILING).await;
    commit(&app, &admin, "root1/r", "main", None, "pass.xml", PASSING).await;
    commit(&app, &admin, "root1/r", "main", None, "cov/cobertura.xml", r#"<coverage line-rate="0.8125" lines-covered="13" lines-valid="16"/>"#).await;
    commit(&app, &admin, "root1/r", "main", None, "cov/lcov.info", "SF:a\nLF:4\nLH:3\nend_of_record\n").await;
    let mark = marks.path().join("ran");
    let config = format!(
        "jobs:
  test:
    reports: [\"out/*.xml\"]
    coverage: [\"cov/**\"]
    cache:
      key: c
      paths: [keep]
    steps:
      - run: mkdir -p out keep && echo kept > keep/f
      - run: if [ -e {m} ]; then cp pass.xml out/junit.xml; else touch {m}; cp fail.xml out/junit.xml; exit 1; fi
",
        m = mark.display()
    );
    commit(&app, &admin, "root1/r", "main", None, ".gitter/ci.yml", &config).await;
    assert_eq!(settle(&app).await, "failure");
    let n: i64 = app.state.db.call(|c| Ok(c.query_row("SELECT max(number) FROM ci_runs", [], |r| r.get(0))?)).await.unwrap();
    let (s, t) = app.req(&admin, "GET", &format!("/api/repos/root1/r/ci/runs/{n}/tests"), None).await;
    assert_eq!(s, StatusCode::OK, "{t}");
    assert_eq!((t["jobs"][0]["tests"].as_i64(), t["jobs"][0]["failed"].as_i64(), t["jobs"][0]["skipped"].as_i64()), (Some(4), Some(2), Some(1)), "{t}");
    assert_eq!(t["coverage"].as_f64(), Some(80.0), "(13 + 3) / (16 + 4): {t}");
    let failed: Vec<(String, String, Value)> = t["results"].as_array().unwrap().iter().map(|r| (r["name"].as_str().unwrap().into(), r["status"].as_str().unwrap().into(), r["line"].clone())).collect();
    assert_eq!(failed, [("adds".to_string(), "failed".to_string(), json!(3)), ("reads".into(), "failed".into(), json!(12))]);
    assert_eq!(t["results"][0]["message"], "expected 4, got 5\nassertion failed");
    assert!(caches(&app).await.is_empty(), "a failed job saves no cache");
    let sha: String = app.state.db.call(|c| Ok(c.query_row("SELECT sha FROM ci_runs ORDER BY id DESC LIMIT 1", [], |r| r.get(0))?)).await.unwrap();
    // The commit's failures as annotations (for the pull request's files view).
    let (_, a) = app.req(&admin, "GET", &format!("/api/repos/root1/r/ci/report?sha={sha}"), None).await;
    let ann: Vec<(String, i64)> = a["annotations"].as_array().unwrap().iter().map(|x| (x["path"].as_str().unwrap().into(), x["line"].as_i64().unwrap())).collect();
    assert_eq!(ann, [("src/lib.rs".to_string(), 3), ("tests/io.rs".into(), 12)], "{a}");
    assert_eq!((a["coverage"].as_f64(), a["failed"].as_i64(), a["run"].as_i64()), (Some(80.0), Some(2), Some(n)));
    // Readers of a private repository's neighbours see nothing.
    assert_eq!(app.req(&Client::default(), "GET", "/api/repos/root1/nope/ci/flaky", None).await.0, StatusCode::NOT_FOUND);

    // Re-run the failed job on the same commit: it passes, so both failures were flaky.
    assert_eq!(app.req(&admin, "POST", &format!("/api/repos/root1/r/ci/runs/{n}/rerun"), Some(json!({}))).await.0, StatusCode::OK);
    assert_eq!(settle(&app).await, "success");
    let (_, t) = app.req(&admin, "GET", &format!("/api/repos/root1/r/ci/runs/{n}/tests"), None).await;
    assert_eq!((t["jobs"][0]["failed"].as_i64(), t["jobs"][0]["flaky"].as_i64()), (Some(0), Some(2)), "{t}");
    let marked: Vec<(&str, bool)> = t["results"].as_array().unwrap().iter().map(|r| (r["name"].as_str().unwrap(), r["flaky"].as_bool().unwrap())).collect();
    assert_eq!(marked, [("adds", true), ("reads", true)]);
    let (_, f) = app.req(&admin, "GET", "/api/repos/root1/r/ci/flaky", None).await;
    let names: Vec<&str> = f["tests"].as_array().unwrap().iter().map(|x| x["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["io::reads", "math::adds"], "{f}");
    assert_eq!((f["tests"][0]["flips"].as_i64(), f["tests"][0]["last_run"].as_i64()), (Some(1), Some(n)));
    let (_, a) = app.req(&admin, "GET", &format!("/api/repos/root1/r/ci/report?sha={sha}"), None).await;
    assert_eq!(a["annotations"].as_array().map(Vec::len), Some(0), "the passing attempt replaced the failures: {a}");
    // The cache went up through the runner API, under the run's branch.
    assert_eq!(caches(&app).await, [("refs/heads/main".to_string(), "c".to_string())]);
}

#[tokio::test]
async fn the_runner_api_refuses_reports_and_caches_without_the_jobs_token() {
    use tower::ServiceExt;
    let app = App::new();
    for (method, path) in [("PUT", "/ci-runner/jobs/1/reports"), ("PUT", "/ci-runner/jobs/1/cache?key=k"), ("GET", "/ci-runner/jobs/1/cache?key=k")] {
        let req = axum::http::Request::builder()
            .method(method)
            .uri(path)
            .header("host", "test.local")
            .header("authorization", "Bearer gcj_nope")
            .body(axum::body::Body::from("x"))
            .unwrap();
        assert_eq!(app.router.clone().oneshot(req).await.unwrap().status(), StatusCode::FORBIDDEN, "{method} {path}");
    }
}
