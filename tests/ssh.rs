//! Git over SSH with the system `ssh` client against the in-process server.

mod common;

use std::path::{Path, PathBuf};

use axum::http::StatusCode;
use serde_json::json;

use common::App;

/// One `ssh` identity: its private key file.
struct Key(PathBuf);

impl Key {
    async fn new(dir: &Path, name: &str) -> Key {
        let path = dir.join(name);
        let out = tokio::process::Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-C", &format!("{name}@laptop"), "-f"])
            .arg(&path)
            .output()
            .await
            .unwrap();
        assert!(out.status.success(), "ssh-keygen: {}", String::from_utf8_lossy(&out.stderr));
        Key(path)
    }
    fn public(&self) -> String {
        std::fs::read_to_string(self.0.with_extension("pub")).unwrap().trim().to_string()
    }
    /// `ssh` that uses only this key and trusts any host key.
    fn ssh_command(&self) -> String {
        format!(
            "ssh -F /dev/null -i {} -o IdentitiesOnly=yes -o IdentityAgent=none -o BatchMode=yes -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR",
            self.0.display()
        )
    }
}

/// Runs a program with a clean git environment: (success, stdout + stderr).
async fn run(cwd: &Path, prog: &str, args: &[&str], key: &Key) -> (bool, String) {
    let mut c = tokio::process::Command::new(prog);
    c.current_dir(cwd)
        .args(args)
        .env("HOME", cwd)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_SSH_COMMAND", key.ssh_command())
        .env("GIT_AUTHOR_NAME", "Alice")
        .env("GIT_AUTHOR_EMAIL", "alice@example.com")
        .env("GIT_COMMITTER_NAME", "Alice")
        .env("GIT_COMMITTER_EMAIL", "alice@example.com")
        .env_remove("SSH_AUTH_SOCK")
        .kill_on_drop(true);
    for (k, _) in std::env::vars() {
        if k.starts_with("GIT_CONFIG_KEY") || k.starts_with("GIT_CONFIG_VALUE") || k == "GIT_CONFIG_COUNT" || k == "GIT_DIR" || k == "GIT_WORK_TREE" {
            c.env_remove(k);
        }
    }
    let out = tokio::time::timeout(std::time::Duration::from_secs(60), c.output()).await.unwrap_or_else(|_| panic!("command hung: {prog} {args:?}")).unwrap();
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.success(), text)
}

/// `ssh -p PORT git@127.0.0.1 <command>` with `key`.
async fn ssh(cwd: &Path, port: u16, key: &Key, extra: &[&str], command: &str) -> (bool, String) {
    let mut args: Vec<String> = key.ssh_command().split(' ').skip(1).map(String::from).collect();
    args.extend(["-p".to_string(), port.to_string()]);
    args.extend(extra.iter().map(|s| s.to_string()));
    args.push("git@127.0.0.1".into());
    if !command.is_empty() {
        args.push(command.into());
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run(cwd, "ssh", &refs, key).await
}

async fn add_key(app: &App, who: &common::Client, key: &Key) -> i64 {
    let (s, v) = app.req(who, "POST", "/api/me/ssh-keys", Some(json!({ "title": "", "key": key.public() }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    v["id"].as_i64().unwrap()
}

async fn pushes(app: &App, repo: &str) -> i64 {
    let name = repo.to_string();
    app.state
        .db
        .call(move |c| Ok(c.query_row("SELECT count(*) FROM events e JOIN repos r ON r.id = e.repo_id WHERE r.name = ?1 AND e.kind = 'push'", [name], |r| r.get(0))?))
        .await
        .unwrap()
}

async fn size(app: &App, repo: &str) -> i64 {
    let name = repo.to_string();
    app.state.db.call(move |c| Ok(c.query_row("SELECT size_bytes FROM repos WHERE name = ?1", [name], |r| r.get(0))?)).await.unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn git_over_ssh_follows_the_http_access_rules() {
    let app = App::new();
    let addr = gitter::ssh::start(&app.state, "127.0.0.1:0", None).await.unwrap();
    let port = addr.port();
    let base = format!("ssh://git@127.0.0.1:{port}");
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let (ak, bk, stranger) = (Key::new(dir, "alice").await, Key::new(dir, "bob").await, Key::new(dir, "stranger").await);

    let alice = app.signup("alice").await;
    let bob = app.signup("bob").await;
    let alice_key = add_key(&app, &alice, &ak).await;
    add_key(&app, &bob, &bk).await;
    // A key belongs to one account.
    let (s, _) = app.req(&bob, "POST", "/api/me/ssh-keys", Some(json!({ "key": ak.public() }))).await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (s, v) = app.req(&alice, "GET", "/api/me/ssh-keys", None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["keys"][0]["title"], "alice@laptop", "the comment is the default title");
    assert!(v["keys"][0]["fingerprint"].as_str().unwrap().starts_with("SHA256:"));
    assert_eq!(v["enabled"], true);

    for (name, private) in [("pub", false), ("priv", true)] {
        let (s, v) = app.req(&alice, "POST", "/api/repos", Some(json!({ "name": name, "readme": true, "private": private }))).await;
        assert_eq!(s, StatusCode::OK, "{v}");
    }
    let (_, meta) = app.req(&alice, "GET", "/api/repos/alice/pub", None).await;
    assert_eq!(meta["ssh_url"], format!("ssh://git@test.local:{port}/alice/pub.git"), "the clone box offers SSH, on the site's host name");

    // Clone: public for any key, private only for keys with access, unknown keys never.
    let (ok, out) = run(dir, "git", &["clone", &format!("{base}/alice/pub.git"), "bob-pub"], &bk).await;
    assert!(ok, "{out}");
    assert!(dir.join("bob-pub/README.md").exists());
    let (ok, out) = run(dir, "git", &["clone", &format!("{base}/alice/priv.git"), "bob-priv"], &bk).await;
    assert!(!ok && out.contains("not found"), "{out}");
    let (ok, out) = run(dir, "git", &["clone", &format!("{base}/alice/priv"), "alice-priv"], &ak).await;
    assert!(ok, "{out}");
    let (ok, out) = run(dir, "git", &["clone", &format!("{base}/alice/pub.git"), "x"], &stranger).await;
    assert!(!ok && out.contains("Permission denied"), "{out}");

    // Push: needs write access, and runs the usual post-push work.
    let work = dir.join("alice-priv");
    std::fs::write(work.join("new.txt"), "over ssh\n").unwrap();
    assert!(run(&work, "git", &["add", "."], &ak).await.0);
    assert!(run(&work, "git", &["commit", "-qm", "Over SSH"], &ak).await.0);
    let (before_pushes, before_size) = (pushes(&app, "priv").await, size(&app, "priv").await);
    let (ok, out) = run(&work, "git", &["push", "origin", "HEAD:main"], &ak).await;
    assert!(ok, "{out}");
    let (_, log) = app.req(&alice, "GET", "/api/repos/alice/priv/commits?rev=main", None).await;
    assert!(log.to_string().contains("Over SSH"), "{log}");
    for _ in 0..50 {
        if pushes(&app, "priv").await > before_pushes {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert_eq!(pushes(&app, "priv").await, before_pushes + 1, "the push is in the feed");
    assert!(size(&app, "priv").await > before_size, "the size is refreshed");

    // Branch rules hold over SSH too (the shared transfer command adds the hook).
    let rule = json!({ "pattern": "main", "enforce_admins": true });
    assert_eq!(app.req(&alice, "POST", "/api/repos/alice/priv/branch-rules", Some(rule)).await.0, StatusCode::OK);
    std::fs::write(work.join("more.txt"), "more\n").unwrap();
    assert!(run(&work, "git", &["add", "."], &ak).await.0);
    assert!(run(&work, "git", &["commit", "-qm", "Straight to main"], &ak).await.0);
    let (ok, out) = run(&work, "git", &["push", "origin", "HEAD:main"], &ak).await;
    assert!(!ok && out.contains("main is protected"), "{out}");
    // Push protection holds over SSH too.
    std::fs::write(work.join("aws.env"), format!("KEY=AKIA{}\n", "Q3EGRIW5ZKVNOM2T")).unwrap();
    assert!(run(&work, "git", &["add", "."], &ak).await.0);
    assert!(run(&work, "git", &["commit", "-qm", "Oops"], &ak).await.0);
    let (ok, out) = run(&work, "git", &["push", "origin", "HEAD:refs/heads/leak"], &ak).await;
    assert!(!ok && out.contains("Push refused") && out.contains("aws.env:1"), "{out}");
    assert!(run(&work, "git", &["reset", "-q", "--hard", "HEAD~1"], &ak).await.0);
    // A second command on the same connection works (ssh connection sharing).
    let (ok, out) = run(&work, "git", &["push", "origin", "HEAD:refs/heads/topic"], &ak).await;
    assert!(ok, "{out}");

    let bwork = dir.join("bob-pub");
    std::fs::write(bwork.join("evil.txt"), "x\n").unwrap();
    assert!(run(&bwork, "git", &["add", "."], &bk).await.0);
    assert!(run(&bwork, "git", &["commit", "-qm", "Not allowed"], &bk).await.0);
    let (ok, out) = run(&bwork, "git", &["push", "origin", "HEAD:main"], &bk).await;
    assert!(!ok && out.contains("push access"), "{out}");

    // Nothing but the three git commands, with a plain owner/repo path.
    let (ok, out) = ssh(dir, port, &bk, &[], "").await;
    assert!(!ok && out.contains("Hi bob") && out.contains("shell"), "{out}");
    for cmd in ["ls", "git-upload-pack '../../etc'", "git-upload-pack 'alice/pub.git'; ls", "sh -c id", "git-upload-pack 'alice/pub.git' --help", "scp -f x"] {
        let (ok, out) = ssh(dir, port, &bk, &[], cmd).await;
        assert!(!ok && out.contains("Gitter runs only git-upload-pack"), "{cmd}: {out}");
    }
    let (ok, out) = ssh(dir, port, &bk, &["-W", "127.0.0.1:22"], "").await;
    assert!(!ok && out.contains("forwarding failed"), "no forwarding: {out}");
    // The scp-like path (no leading slash) works for a plain git command too.
    let (ok, out) = ssh(dir, port, &bk, &[], "git-upload-pack 'alice/pub.git'").await;
    assert!(out.contains("refs/heads/main"), "{ok} {out}");
    // The scp-like form (no leading slash) works for upload-archive too.
    let (ok, out) = run(dir, "git", &["archive", &format!("--remote={base}/alice/pub.git"), "-o", "pub.tar", "main"], &bk).await;
    assert!(ok, "{out}");

    // Removed keys and suspended accounts are refused.
    let (s, _) = app.req(&alice, "DELETE", &format!("/api/me/ssh-keys/{alice_key}"), None).await;
    assert_eq!(s, StatusCode::OK);
    let (ok, out) = run(dir, "git", &["ls-remote", &format!("{base}/alice/pub.git")], &ak).await;
    assert!(!ok && out.contains("Permission denied"), "{out}");
    app.state.db.call(|c| Ok(c.execute("UPDATE users SET suspended = 1 WHERE username = 'bob'", [])?)).await.unwrap();
    let (ok, out) = run(dir, "git", &["ls-remote", &format!("{base}/alice/pub.git")], &bk).await;
    assert!(!ok && out.contains("Permission denied"), "{out}");
}

#[tokio::test]
async fn ssh_keys_are_checked() {
    let app = App::new();
    let alice = app.signup("alice").await;
    let tmp = tempfile::tempdir().unwrap();
    let out = tokio::process::Command::new("ssh-keygen")
        .args(["-q", "-t", "rsa", "-b", "1024", "-N", "", "-f"])
        .arg(tmp.path().join("weak"))
        .output()
        .await
        .unwrap();
    assert!(out.status.success());
    let weak = std::fs::read_to_string(tmp.path().join("weak.pub")).unwrap();
    for bad in [weak.as_str(), "ssh-ed25519 AAAA", "-----BEGIN OPENSSH PRIVATE KEY-----\nxx\n-----END OPENSSH PRIVATE KEY-----", "command=\"ls\" ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFxsPyhOovC3igNqTo8u07KB2pQB9asL+9wpOgr9jf9G"] {
        let (s, v) = app.req(&alice, "POST", "/api/me/ssh-keys", Some(json!({ "key": bad }))).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{bad}: {v}");
    }
    let (s, v) = app.req(&alice, "GET", "/api/me/ssh-keys", None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["enabled"], false, "SSH is off unless GITTER_SSH_ADDR is set");
    app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "r" }))).await;
    let (_, meta) = app.req(&alice, "GET", "/api/repos/alice/r", None).await;
    assert!(meta["clone_url"].is_string() && meta["ssh_url"].is_null(), "{meta}");
}
