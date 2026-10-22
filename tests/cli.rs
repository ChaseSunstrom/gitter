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
        // The test's own global config (`gitter auth setup-git` writes there), never the real ~/.gitconfig.
        .env("GIT_CONFIG_GLOBAL", home.join(".gitconfig"))
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

/// Like `gitter`, with `input` on its stdin.
fn gitter_in(home: &Path, cwd: &Path, args: &[&str], input: &str) -> (i32, String, String) {
    use std::io::Write;
    let mut child = cmd(env!("CARGO_BIN_EXE_gitter"), home, cwd)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    (out.status.code().unwrap_or(-1), String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
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
    // -R naming a repository this folder's remote doesn't point at: which branch goes there must be said.
    let (code, _, err) = gitter(&home, &clone, &["pr", "create", "-R", "alice/elsewhere"]);
    assert!(code != 0 && err.contains("--head"), "{err}");
    let out = ok(&home, &clone, &["pr", "create", "-R", "alice/proj"]);
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn setup_git_lets_plain_git_clone_and_push() {
    let app = App::new();
    let url = serve(&app).await;
    let tok = token(&app, "alice").await;
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().to_path_buf();
    let (code, _, err) = gitter(&home, &home, &["auth", "setup-git"]);
    assert!(code != 0 && err.contains("login"), "nothing to set up before a login: {err}");
    // gh's helper for another server isn't gitter's.
    git(&home, &home, &["config", "--global", "credential.https://github.com.helper", "!'/home/J Smith/bin/gh' auth git-credential"]);
    let (_, _, err) = gitter(&home, &home, &["login", &url, "--token", &tok]);
    assert!(!err.contains("setup-git"), "no hint before setup-git was ever run: {err}");
    ok(&home, &home, &["repo", "create", "secret", "--private", "--readme"]);
    let remote = format!("{url}/alice/secret.git");
    // Before setup-git, plain git has no credentials for the private repository.
    let out = cmd("git", &home, &home).args(["clone", "-q", &remote, "before"]).output().unwrap();
    assert!(!out.status.success());

    let out = ok(&home, &home, &["auth", "setup-git"]);
    assert!(out.contains(&url), "{out}");
    ok(&home, &home, &["auth", "setup-git"]); // a second run doesn't add a second helper
    let key = format!("credential.{url}.helper");
    let out = cmd("git", &home, &home).args(["config", "--global", "--get-all", &key]).output().unwrap();
    let helpers: Vec<String> = String::from_utf8_lossy(&out.stdout).lines().map(String::from).collect();
    assert!(helpers.len() == 2 && helpers[0].is_empty() && helpers[1].contains("auth git-credential"), "an empty reset, then gitter's: {helpers:?}");

    git(&home, &home, &["clone", "-q", &remote, "plain"]);
    let clone = home.join("plain");
    assert!(clone.join("README.md").exists());
    assert!(!git(&home, &clone, &["config", "--get", "remote.origin.url"]).contains(&tok), "no token in the remote URL");
    std::fs::write(clone.join("x.txt"), "x\n").unwrap();
    git(&home, &clone, &["add", "x.txt"]);
    git(&home, &clone, &["commit", "-q", "-m", "x"]);
    git(&home, &clone, &["push", "-q", "origin", "main"]);
    let gitconfig = std::fs::read_to_string(home.join(".gitconfig")).unwrap();
    assert!(!gitconfig.contains(&tok), "the token stays out of git's files: {gitconfig}");

    // The helper itself: answers only for a logged-in server and a matching user; store and erase do nothing.
    let host = url.strip_prefix("http://").unwrap();
    let (code, out, _) = gitter_in(&home, &home, &["auth", "git-credential", "get"], &format!("protocol=http\nhost={host}\n\n"));
    assert_eq!(code, 0);
    assert_eq!(out, format!("username=x-access-token\npassword={tok}\n"));
    for other in [
        "protocol=http\nhost=elsewhere.example\n\n".to_string(),
        format!("protocol=https\nhost={host}\n\n"),
        format!("protocol=http\nhost={host}\nusername=bob\n\n"),
    ] {
        let (code, out, _) = gitter_in(&home, &home, &["auth", "git-credential", "get"], &other);
        assert!(code == 0 && out.is_empty(), "{other:?}: {out}");
    }
    let (_, out, _) = gitter_in(&home, &home, &["auth", "git-credential", "get"], &format!("protocol=http\nhost={host}\nusername=alice\n\n"));
    assert!(out.contains(&tok), "the logged-in user's own name is fine: {out}");
    for verb in ["store", "erase"] {
        let (code, out, _) = gitter_in(&home, &home, &["auth", "git-credential", verb], &format!("protocol=http\nhost={host}\nusername=x\npassword=y\n\n"));
        assert!(code == 0 && out.is_empty(), "{verb}: {out}");
    }
    let cfg = home.join(".config/gitter/config.json");
    let mut saved = json_of(&std::fs::read_to_string(&cfg).unwrap());
    assert_eq!(saved["hosts"][&url]["token"], tok.as_str(), "erase leaves the login alone");

    // Over 64 KiB of input: no answer (not a cut-off last line).
    let big = format!("protocol=http\nhost={host}\nx={}\n\n", "a".repeat(70 << 10));
    let (code, out, _) = gitter_in(&home, &home, &["auth", "git-credential", "get"], &big);
    assert!(code == 0 && out.is_empty(), "{out}");

    // Two logins on one host (the second under a path): each helper entry answers only with its own token.
    let sub = format!("{url}/sub");
    let tok2 = "gtr_subpathtoken"; // pragma: allowlist secret
    saved["hosts"][&sub] = json!({ "user": "alice", "token": tok2 });
    std::fs::write(&cfg, saved.to_string()).unwrap();
    let ask = |server: &str, path: Option<&str>| {
        let input = format!("protocol=http\nhost={host}\n{}\n", path.map(|p| format!("path={p}\n")).unwrap_or_default());
        let mut args = vec!["auth", "git-credential"];
        if !server.is_empty() {
            args.push(server);
        }
        args.push("get");
        let (code, out, err) = gitter_in(&home, &home, &args, &input);
        assert_eq!(code, 0, "{err}");
        out
    };
    let answer = |t: &str| format!("username=x-access-token\npassword={t}\n");
    assert_eq!(ask("", None), "", "the old helper form is ambiguous on a shared host: no answer");
    assert_eq!(ask(&url, None), answer(&tok));
    assert_eq!(ask(&sub, None), answer(tok2));
    assert_eq!(ask(&sub, Some("sub/x.git")), answer(tok2));
    assert_eq!(ask(&sub, Some("alice/x.git")), "", "outside its path");
    assert_eq!(ask(&sub, Some("subway/x.git")), "", "a path prefix is whole segments");
    assert_eq!(ask("http://elsewhere.example", None), "", "not a logged-in server");
    // And through git itself, with both helpers set up.
    ok(&home, &home, &["auth", "setup-git"]);
    let fill = |u: &str| {
        let out = gitter_like_git_fill(&home, u);
        out.lines().find_map(|l| l.strip_prefix("password=").map(String::from)).unwrap_or_default()
    };
    assert_eq!(fill(&format!("{url}/other.git")), tok);
    assert_eq!(fill(&format!("{sub}/x.git")), tok2);

    // A login to another server after setup-git: a hint to run it again.
    let url2 = serve(&app).await;
    let (code, _, err) = gitter(&home, &home, &["login", &url2, "--token", &tok]);
    assert!(code == 0 && err.contains("gitter auth setup-git"), "{err}");
}

/// `git credential fill` for `url`: what git's helpers answer.
fn gitter_like_git_fill(home: &Path, url: &str) -> String {
    use std::io::Write;
    let mut child = cmd("git", home, home)
        .args(["credential", "fill"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(format!("url={url}\n\n").as_bytes()).unwrap();
    String::from_utf8_lossy(&child.wait_with_output().unwrap().stdout).into_owned()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn login_prompt_hides_the_token_and_reads_a_piped_one() {
    let app = App::new();
    let url = serve(&app).await;
    let tok = token(&app, "alice").await;
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().to_path_buf();

    // Piped: read as is.
    let (code, out, err) = gitter_in(&home, &home, &["login", &url], &format!("{tok}\n"));
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(out.contains("alice"), "{out}");

    // On a terminal (script(1) gives it one): what's typed isn't echoed back.
    if !Command::new("script").arg("--version").output().is_ok_and(|o| o.status.success()) {
        eprintln!("skipped the terminal half: script(1) from util-linux isn't installed");
        return;
    }
    std::fs::remove_file(home.join(".config/gitter/config.json")).unwrap();
    let line = format!("'{}' login '{url}'", env!("CARGO_BIN_EXE_gitter"));
    let (ok_exit, seen) = on_terminal(&home, &line, &format!("{tok}\n"));
    assert!(ok_exit && seen.contains("Logged in"), "{seen}");
    assert!(seen.contains("Paste") && !seen.contains(&tok), "the token was echoed: {seen}");

    // Ctrl-C at the prompt: echo is back on afterwards (the shell traps INT so it carries on to check).
    let line = format!(
        "trap true INT; '{}' login '{url}'; echo \"EXIT=$?\"; if stty -a | grep -qw -- -echo; then echo ECHO-OFF; else echo ECHO-ON; fi",
        env!("CARGO_BIN_EXE_gitter")
    );
    let (_, seen) = on_terminal(&home, &line, "\x03");
    assert!(seen.contains("EXIT=130") && seen.contains("ECHO-ON"), "{seen}");
}

/// Runs the shell `line` on a terminal, types `input` once it shows the token prompt: (exited 0, all it showed).
fn on_terminal(home: &Path, line: &str, input: &str) -> (bool, String) {
    use std::io::{Read, Write};
    let mut child = cmd("script", home, home)
        .args(["-q", "-e", "-c", line, "/dev/null"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("script(1) from util-linux");
    let seen = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let mut stdout = child.stdout.take().unwrap();
    let reader = {
        let seen = seen.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 1024];
            while let Ok(n) = stdout.read(&mut buf) {
                if n == 0 {
                    break;
                }
                seen.lock().unwrap().push_str(&String::from_utf8_lossy(&buf[..n]));
            }
        })
    };
    // The prompt comes once echo is off: type only then.
    for _ in 0..200 {
        if seen.lock().unwrap().contains("Paste") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(input.as_bytes()).unwrap();
    for _ in 0..200 {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let _ = child.kill();
    let status = child.wait().unwrap();
    drop(stdin);
    reader.join().unwrap();
    let seen = seen.lock().unwrap().clone();
    (status.success(), seen)
}

#[test]
fn a_flag_a_command_does_not_take_is_refused_with_its_usage() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    for (args, usage) in [
        (&["repo", "list", "--title", "x"][..], "gitter repo list"),
        (&["ci", "run", "--job", "x"][..], "gitter ci run"),
        (&["issue", "list", "--body", "x"][..], "gitter issue list"),
        (&["pr", "checkout", "1", "--json"][..], "gitter pr view|checkout N"),
        (&["login", "http://127.0.0.1:1", "--json"][..], "gitter login URL"),
        (&["auth", "setup-git", "-R", "a/b"][..], "gitter auth setup-git"),
        (&["repo", "view", "--bogus"][..], "gitter repo view"),
    ] {
        let (code, _, err) = gitter(home, home, args);
        assert!(code != 0 && err.contains("doesn't take") && err.contains(usage), "{args:?}: {err}");
    }
    // Not the whole list: just the command's own lines.
    let (_, _, err) = gitter(home, home, &["repo", "list", "--title", "x"]);
    assert!(!err.contains("gitter issue"), "{err}");
    // A mistyped --token=… names the flag, not the value after it.
    let (code, _, err) = gitter(home, home, &["login", "http://127.0.0.1:1", "--tokens=SECRETVALUE"]); // pragma: allowlist secret
    assert!(code != 0 && err.contains("--tokens") && !err.contains("SECRETVALUE"), "{err}");
}

#[test]
fn logout_removes_the_token_and_only_gitters_git_helper() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    let (a, b) = ("http://127.0.0.1:1", "http://127.0.0.1:2/sub");
    let cfg = home.join(".config/gitter/config.json");
    std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
    let hosts = json!({ "default": a, "hosts": {
        a: { "user": "alice", "token": "gtr_tokena" }, // pragma: allowlist secret
        b: { "user": "alice", "token": "gtr_tokenb" }, // pragma: allowlist secret
    }});
    std::fs::write(&cfg, hosts.to_string()).unwrap();
    let (code, _, err) = gitter(home, home, &["logout"]);
    assert!(code != 0 && err.contains(a) && err.contains(b), "two logins: which one must be said: {err}");
    let (code, _, err) = gitter(home, home, &["logout", a, "--json"]);
    assert!(code != 0 && err.contains("doesn't take") && err.contains("gitter logout"), "{err}");

    ok(home, home, &["auth", "setup-git"]);
    let helpers = |server: &str| -> Vec<String> {
        let out = cmd("git", home, home).args(["config", "--global", "--get-all", &format!("credential.{server}.helper")]).output().unwrap();
        String::from_utf8_lossy(&out.stdout).lines().map(String::from).collect()
    };
    // The user's own helpers: after gitter's on its key (one looks like gh's), and on another server.
    let key = format!("credential.{a}.helper");
    git(home, home, &["config", "--global", "--add", &key, "!/usr/bin/gh auth git-credential"]);
    git(home, home, &["config", "--global", "--add", &key, "cache"]);
    git(home, home, &["config", "--global", "credential.https://github.com.helper", "store"]);

    let out = ok(home, home, &["logout", &format!("{a}/")]);
    assert!(out.contains(a), "{out}");
    let saved = json_of(&std::fs::read_to_string(&cfg).unwrap());
    assert!(saved["hosts"].get(a).is_none() && saved["hosts"][b]["token"] == "gtr_tokenb", "{saved}");
    assert_eq!(saved["default"], b, "the default moves to a login that's left");
    assert_eq!(helpers(a), ["!/usr/bin/gh auth git-credential", "cache"], "gitter's reset and helper gone, the user's kept");
    assert_eq!(helpers(b).len(), 2, "the other server's helper stays");
    assert_eq!(helpers("https://github.com"), ["store"]);
    let (code, _, err) = gitter(home, home, &["logout", a]);
    assert!(code != 0 && err.contains("not logged in"), "a second logout: {err}");

    // One login left: no argument needed, and its key goes entirely.
    ok(home, home, &["logout"]);
    let saved = json_of(&std::fs::read_to_string(&cfg).unwrap());
    assert!(saved["hosts"].as_object().unwrap().is_empty() && saved["default"] == "", "{saved}");
    assert!(helpers(b).is_empty(), "{:?}", helpers(b));
    assert_eq!(helpers("https://github.com"), ["store"]);
    let (code, _, err) = gitter(home, home, &["logout"]);
    assert!(code != 0 && err.contains("not logged in"), "{err}");
}

#[test]
fn logout_tells_gitters_helpers_from_others() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    let servers = ["http://127.0.0.1:1", "http://127.0.0.1:2", "http://127.0.0.1:3", "http://127.0.0.1:4"];
    let cfg = home.join(".config/gitter/config.json");
    std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
    let hosts: serde_json::Map<String, Value> = servers.iter().map(|s| (s.to_string(), json!({ "user": "alice", "token": "gtr_t" }))).collect(); // pragma: allowlist secret
    std::fs::write(&cfg, json!({ "default": servers[0], "hosts": hosts }).to_string()).unwrap();
    let key = |s: &str| format!("credential.{s}.helper");
    let set = |s: &str, vals: &[&str]| {
        for v in vals {
            git(home, home, &["config", "--global", "--add", &key(s), v]);
        }
    };
    let helpers = |s: &str| -> Vec<String> {
        let out = cmd("git", home, home).args(["config", "--global", "--get-all", &key(s)]).output().unwrap();
        String::from_utf8_lossy(&out.stdout).lines().map(String::from).collect()
    };
    let ours = |s: &str| format!("!'{}' auth git-credential '{s}'", env!("CARGO_BIN_EXE_gitter"));
    let gh = "!'/home/J Smith/bin/gh' auth git-credential";
    // The old form (no server), from a gitter elsewhere; and gh's, quoted the same way.
    set(servers[0], &["", "!'/opt/x/gitter' auth git-credential", gh]);
    // The user's own helper before gitter's reset and helper.
    set(servers[1], &["mine", "", &ours(servers[1])]);
    // Only the helper, no reset.
    set(servers[2], &[&ours(servers[2])]);
    for s in &servers[..3] {
        ok(home, home, &["logout", s]);
    }
    assert_eq!(helpers(servers[0]), [gh], "the old form goes, gh's stays");
    assert_eq!(helpers(servers[1]), ["mine"]);
    assert!(helpers(servers[2]).is_empty(), "{:?}", helpers(servers[2]));

    // git's config can't be read: a warning, not a claim that its helper is gone.
    std::fs::write(home.join(".gitconfig"), "[credential\n").unwrap();
    let (code, out, err) = gitter(home, home, &["logout", servers[3]]);
    assert!(code == 0 && out.contains("Logged out") && !out.contains("git no longer"), "{out}");
    assert!(err.contains("can't read git's config"), "{err}");
}

#[test]
fn help_after_a_command_prints_only_its_usage() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    for (args, want, not) in [
        (&["repo", "list", "--help"][..], "gitter repo list", "gitter issue"),
        (&["help", "pr", "merge"][..], "gitter pr merge N", "gitter repo"),
        (&["issue", "view", "3", "-h"][..], "gitter issue view|close N", "gitter issue list"),
        (&["ci", "run", "--help"][..], "copy of the work tree", "gitter ci status"),
        (&["login", "--help"][..], "gitter login URL", "gitter repo"),
        (&["logout", "--help"][..], "gitter logout [SERVER]", "gitter login"),
        (&["repo", "--help"][..], "gitter repo clone", "gitter issue"),
        (&["help", "auth"][..], "gitter auth setup-git", "gitter repo"),
        (&["runner", "--help"][..], "gitter runner --url URL", "gitter serve"),
    ] {
        let (code, out, err) = gitter(home, home, args);
        assert!(code == 0 && out.contains(want) && !out.contains(not), "{args:?}: {out}\n{err}");
    }
    // Alone, or for no command the client knows: the server's whole list, which names logout too.
    for args in [&["help"][..], &["--help"][..], &["help", "bogus"][..]] {
        let (code, out, _) = gitter(home, home, args);
        assert!(code == 0 && out.contains("Settings come from environment variables") && out.contains("gitter logout"), "{args:?}: {out}");
    }
    // A command word alone: the client's whole list (only it explains how the repository is found).
    let (code, out, _) = gitter(home, home, &["repo"]);
    assert!(code == 0 && out.contains("The repository is the one whose git remote"), "{out}");
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

    // --here would hand the folder itself to a container (mounted, its files chowned): refused, nothing run.
    let dir = local_repo(home, &[(".gitter/ci.yml", "jobs:\n  a:\n    image: alpine\n    steps: [\"echo ok > here.txt\"]\n")]);
    let (code, _, err) = gitter(home, &dir, &["ci", "run", "--here"]);
    assert!(code != 0 && err.contains("image") && err.contains("--here"), "{err}");
    assert!(!dir.join("here.txt").exists());

    // A broken file says why.
    let dir = local_repo(home, &[(".gitter/ci.yml", "jobs:\n  a:\n    stepz: [x]\n")]);
    let (code, _, err) = gitter(home, &dir, &["ci", "run"]);
    assert_ne!(code, 0);
    assert!(err.contains("unknown setting"), "{err}");
}
