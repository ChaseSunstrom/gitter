//! The `gitter` command-line client against a real server, and `gitter ci run`.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

use common::App;

/// A test server on a real port (the CLI is another process).
async fn serve(app: &App) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let router = app.router.clone();
    tokio::spawn(async move {
        let _ = axum::serve(listener, router.into_make_service_with_connect_info::<std::net::SocketAddr>()).await;
    });
    url
}

/// Runs `gitter args…` in `cwd` with `home` as its home folder: (exit code, stdout, stderr).
fn gitter(home: &Path, cwd: &Path, args: &[&str]) -> (i32, String, String) {
    let out = cmd(env!("CARGO_BIN_EXE_gitter"), home, cwd).args(args).output().unwrap();
    (out.status.code().unwrap_or(-1), String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

/// Like `gitter`, but it must succeed; its stdout.
fn ok(home: &Path, cwd: &Path, args: &[&str]) -> String {
    let (code, out, err) = gitter(home, cwd, args);
    assert_eq!(code, 0, "gitter {args:?} failed:\n{out}\n{err}");
    out
}

fn cmd(program: &str, home: &Path, cwd: &Path) -> Command {
    let mut c = Command::new(program);
    c.current_dir(cwd)
        .env("HOME", home)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("GITTER_TOKEN")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_AUTHOR_NAME", "Alice")
        .env("GIT_AUTHOR_EMAIL", "alice@example.com")
        .env("GIT_COMMITTER_NAME", "Alice")
        .env("GIT_COMMITTER_EMAIL", "alice@example.com");
    for (k, _) in std::env::vars() {
        if k.starts_with("GIT_CONFIG_KEY") || k.starts_with("GIT_CONFIG_VALUE") || k == "GIT_CONFIG_COUNT" || k == "GIT_DIR" || k == "GIT_WORK_TREE" {
            c.env_remove(k);
        }
    }
    c
}

fn git(home: &Path, cwd: &Path, args: &[&str]) -> String {
    let out = cmd("git", home, cwd).args(args).output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

async fn token(app: &App, name: &str) -> String {
    let who = app.signup(name).await;
    let (_, t) = app.req(&who, "POST", "/api/me/tokens", Some(json!({ "name": "cli" }))).await;
    t["token"].as_str().unwrap().to_string()
}

fn json_of(s: &str) -> Value {
    serde_json::from_str(s).unwrap_or_else(|e| panic!("not JSON ({e}): {s}"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn login_repo_issue_and_pr_commands_work_against_a_server() {
    let app = App::new();
    let url = serve(&app).await;
    let tok = token(&app, "alice").await;
    let tmp = tempfile::tempdir().unwrap();
    let (home, work) = (tmp.path().join("home"), tmp.path().join("work"));
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&work).unwrap();

    // A refused token isn't saved; a good one is, readable only by its owner.
    let (code, _, err) = gitter(&home, &work, &["login", &url, "--token", "gtr_nope"]);
    assert_ne!(code, 0);
    assert!(err.contains("token"), "{err}");
    let out = ok(&home, &work, &["login", &format!("{url}/"), "--token", &tok]);
    assert!(out.contains("alice"), "{out}");
    let cfg = home.join(".config/gitter/config.json");
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&cfg).unwrap().permissions().mode() & 0o777, 0o600);
    }
    let saved = json_of(&std::fs::read_to_string(&cfg).unwrap());
    assert_eq!(saved["hosts"][&url]["token"], tok.as_str(), "{saved}");

    // Repositories.
    let out = ok(&home, &work, &["repo", "create", "proj", "--description", "A project", "--readme"]);
    assert!(out.contains("alice/proj"), "{out}");
    let list = json_of(&ok(&home, &work, &["repo", "list", "--json"]));
    assert!(list.as_array().unwrap().iter().any(|r| r["name"] == "proj"), "{list}");
    assert!(ok(&home, &work, &["repo", "list"]).contains("proj"));
    assert!(ok(&home, &work, &["repo", "view", "alice/proj"]).contains("A project"));
    ok(&home, &work, &["repo", "clone", "alice/proj"]);
    let clone = work.join("proj");
    assert!(clone.join("README.md").exists());
    // Inside the clone the repository comes from its remote.
    assert_eq!(json_of(&ok(&home, &clone, &["repo", "view", "--json"]))["name"], "proj");

    // Issues.
    let (code, _, err) = gitter(&home, &work, &["issue", "list"]);
    assert_ne!(code, 0, "outside a clone without -R there's no repository");
    assert!(err.contains("-R"), "{err}");
    let out = ok(&home, &clone, &["issue", "create", "--title", "Bug", "--body", "it breaks"]);
    assert!(out.contains("#1"), "{out}");
    assert!(ok(&home, &work, &["issue", "list", "-R", "alice/proj"]).contains("Bug"));
    ok(&home, &clone, &["issue", "comment", "1", "--body", "confirmed"]);
    let view = ok(&home, &clone, &["issue", "view", "1"]);
    assert!(view.contains("it breaks") && view.contains("confirmed"), "{view}");
    ok(&home, &clone, &["issue", "close", "1"]);
    assert!(!ok(&home, &clone, &["issue", "list"]).contains("Bug"));
    assert!(ok(&home, &clone, &["issue", "list", "--state", "closed"]).contains("Bug"));
    let (code, _, err) = gitter(&home, &clone, &["issue", "view", "99"]);
    assert_ne!(code, 0);
    assert!(!err.is_empty());

    // Pull requests: create pushes the branch first.
    git(&home, &clone, &["checkout", "-q", "-b", "feature"]);
    std::fs::write(clone.join("f.txt"), "feature\n").unwrap();
    git(&home, &clone, &["add", "f.txt"]);
    git(&home, &clone, &["commit", "-q", "-m", "Add f"]);
    let out = ok(&home, &clone, &["pr", "create"]);
    assert!(out.contains("#2"), "{out}");
    let pr = json_of(&ok(&home, &clone, &["pr", "view", "2", "--json"]));
    assert_eq!((pr["issue"]["title"].as_str(), pr["pull"]["head"].as_str()), (Some("Add f"), Some("feature")), "{pr}");
    assert!(ok(&home, &clone, &["pr", "list"]).contains("Add f"));

    // Checkout in a second clone fetches the head.
    ok(&home, &work, &["repo", "clone", "alice/proj", "other"]);
    let other = work.join("other");
    ok(&home, &other, &["pr", "checkout", "2"]);
    assert_eq!(git(&home, &other, &["branch", "--show-current"]), "feature");
    assert_eq!(std::fs::read_to_string(other.join("f.txt")).unwrap(), "feature\n");
    // Again, with the branch already here: fine; with local commits on it: refused before switching.
    git(&home, &other, &["switch", "-q", "main"]);
    ok(&home, &other, &["pr", "checkout", "2"]);
    git(&home, &other, &["commit", "-q", "--allow-empty", "-m", "local only"]);
    git(&home, &other, &["switch", "-q", "main"]);
    let (code, _, err) = gitter(&home, &other, &["pr", "checkout", "2"]);
    assert!(code != 0 && err.contains("has commits"), "{err}");
    assert_eq!(git(&home, &other, &["branch", "--show-current"]), "main");

    ok(&home, &clone, &["pr", "merge", "2"]);
    assert_eq!(json_of(&ok(&home, &clone, &["pr", "view", "2", "--json"]))["issue"]["state"], "merged");
}

/// Wait for the newest run to finish.
async fn settle(app: &App) {
    for _ in 0..300 {
        let s: String = app.state.db.call(|c| Ok(c.query_row("SELECT status FROM ci_runs ORDER BY id DESC LIMIT 1", [], |r| r.get(0))?)).await.unwrap_or_default();
        if gitter::ci::queue::terminal(&s) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!("the run didn't finish");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ci_commands_show_runs_logs_and_rerun() {
    let app = App::new();
    let url = serve(&app).await;
    let tok = token(&app, "root1").await;
    tokio::spawn(gitter::ci::runner::builtin(app.state.clone()));
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().to_path_buf();
    ok(&home, &home, &["login", &url, "--token", &tok]);
    ok(&home, &home, &["repo", "create", "r", "--readme"]);
    ok(&home, &home, &["repo", "clone", "root1/r"]);
    let clone = home.join("r");
    std::fs::create_dir_all(clone.join(".gitter")).unwrap();
    let count = home.join("count");
    std::fs::write(clone.join(".gitter/ci.yml"), format!("jobs:\n  hello:\n    steps: [\"echo hello from ci\", \"echo run >> {}\", \"sleep 1\", \"exit 5\"]\n", count.display())).unwrap();
    git(&home, &clone, &["add", "."]);
    git(&home, &clone, &["commit", "-q", "-m", "CI"]);
    // A plain push, with the token the way the CLI hands it to git.
    let basic = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, format!("x-access-token:{tok}"));
    git(&home, &clone, &["-c", &format!("http.extraHeader=Authorization: Basic {basic}"), "push", "-q", "origin", "main"]);
    settle(&app).await;

    let status = ok(&home, &clone, &["ci", "status"]);
    assert!(status.contains("failure"), "{status}");
    let run = json_of(&ok(&home, &clone, &["ci", "status", "1", "--json"]));
    assert_eq!(run["jobs"][0]["exit_code"], 5, "{run}");
    let logs = ok(&home, &clone, &["ci", "logs", "1"]);
    assert!(logs.contains("hello from ci"), "{logs}");
    assert!(ok(&home, &clone, &["ci", "rerun", "1"]).contains("Re-running"));
    // Straight away: the job is queued or running, and logs follows it to the end.
    let logs = ok(&home, &clone, &["ci", "logs", "1", "--job", "hello"]);
    assert!(logs.contains("Job failed (exit code 5)"), "{logs}");
    assert_eq!(logs.lines().filter(|l| *l == "hello from ci").count(), 1, "the rerun's own log: {logs}");
    settle(&app).await;
    assert_eq!(std::fs::read_to_string(&count).unwrap(), "run\nrun\n", "the job ran again");
    let (code, _, err) = gitter(&home, &clone, &["ci", "rerun", "99"]);
    assert!(code != 0 && err.contains("Not found"), "{err}");
}

/// A git repository with `.gitter/ci.yml` (and any other files).
fn local_repo(home: &Path, files: &[(&str, &str)]) -> PathBuf {
    let dir = home.join(format!("repo{}", rand::random::<u32>()));
    std::fs::create_dir_all(dir.join(".gitter")).unwrap();
    git(home, &dir, &["init", "-q"]);
    for (p, c) in files {
        std::fs::write(dir.join(p), c).unwrap();
    }
    dir
}

#[test]
fn ci_run_plans_and_runs_a_local_pipeline() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    let config = "jobs:
  build:
    steps:
      - run: echo built > built.txt && echo \"building $GITTER_JOB\" && sh helper.sh
  test:
    needs: [build]
    matrix:
      n: [1, 2]
    steps:
      - run: echo \"testing ${{ matrix.n }}\"
  lint:
    steps:
      - run: echo linting; exit 4
      - run: echo never-printed
  deploy:
    needs: [lint, test]
    steps: [echo deploying]
";
    // helper.sh isn't committed (or even added): the copy has it anyway.
    let dir = local_repo(home, &[(".gitter/ci.yml", config), ("helper.sh", "echo helped")]);
    let (code, out, err) = gitter(home, &dir, &["ci", "run"]);
    assert_eq!(code, 1, "a failing job fails the run:\n{out}\n{err}");
    for want in ["building build", "helped", "testing 1", "testing 2", "linting"] {
        assert!(out.contains(want), "{want}: {out}");
    }
    assert!(!out.contains("never-printed") && !out.contains("deploying"), "{out}");
    let (b, t) = (out.find("building build").unwrap(), out.find("testing 1").unwrap());
    assert!(b < t, "needs run first: {out}");
    assert!(out.contains("deploy") && out.contains("skipped"), "{out}");
    assert!(!dir.join("built.txt").exists(), "the work tree is left alone");

    // A passing pipeline exits 0; --here runs in the folder itself.
    let dir = local_repo(home, &[(".gitter/ci.yml", "jobs:\n  a:\n    steps: [\"echo ok > here.txt\"]\n")]);
    ok(home, &dir, &["ci", "run", "--here"]);
    assert!(dir.join("here.txt").exists());

    // A broken file says why.
    let dir = local_repo(home, &[(".gitter/ci.yml", "jobs:\n  a:\n    stepz: [x]\n")]);
    let (code, _, err) = gitter(home, &dir, &["ci", "run"]);
    assert_ne!(code, 0);
    assert!(err.contains("unknown setting"), "{err}");
}
