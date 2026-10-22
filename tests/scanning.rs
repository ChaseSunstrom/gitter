//! Push protection (git pushes and web commits), the on-demand secret scan,
//! dependencies, OSV vulnerabilities and SBOMs.

mod common;

use std::path::Path;
use std::process::Command;

use axum::http::StatusCode;
use serde_json::{Value, json};

use common::{App, Client};

// Secrets are built at run time so this file doesn't trip scanners itself.
fn aws() -> String {
    format!("AKIA{}", "Q3EGRIW5ZKVNOM2T")
}
fn gtr() -> String {
    format!("gtr_{}", "aB3dE5fG7hJ9kL1mN3pQ5rS7tU9vW1xY3zA5bC7dE9f")
}
fn pem() -> String {
    format!("-----BEGIN RSA {}-----\nMIIEpAIBAAKCAQEA3Tz2mr7SZiAMfQyuvBjM2Tg\n-----END RSA {}-----\n", "PRIVATE KEY", "PRIVATE KEY")
}

/// A test server on a real port, for the git CLI.
async fn serve(app: &App) -> String {
    gitter::scan::set_hook_exe(env!("CARGO_BIN_EXE_gitter"));
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
    let out = git_cmd(home, cwd, args).output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// `git push`: (succeeded, stderr).
fn push(home: &Path, cwd: &Path) -> (bool, String) {
    let out = git_cmd(home, cwd, &["push", "-q", "origin", "HEAD:main"]).output().unwrap();
    (out.status.success(), String::from_utf8_lossy(&out.stderr).into_owned())
}

fn commit_file(home: &Path, cwd: &Path, path: &str, content: &str) {
    if let Some(dir) = Path::new(path).parent() {
        std::fs::create_dir_all(cwd.join(dir)).unwrap();
    }
    std::fs::write(cwd.join(path), content).unwrap();
    git(home, cwd, &["add", "-A"]);
    git(home, cwd, &["commit", "-qm", &format!("add {path}")]);
}

/// The fingerprints a refused push's message offers to allow.
fn allow_link(stderr: &str) -> Vec<String> {
    let at = stderr.find("?allow=").unwrap_or_else(|| panic!("no allow link in: {stderr}"));
    stderr[at + 7..].split_whitespace().next().unwrap().split(',').map(String::from).collect()
}

struct Setup {
    app: App,
    alice: Client,
    work: tempfile::TempDir,
    home: tempfile::TempDir,
}

/// root1 (site admin), alice with a repository `r` cloned into `work`.
async fn setup() -> Setup {
    let app = App::new();
    app.signup("root1").await;
    let alice = app.signup("alice").await;
    let (s, _) = app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "r", "readme": true }))).await;
    assert_eq!(s, StatusCode::OK);
    let (_, t) = app.req(&alice, "POST", "/api/me/tokens", Some(json!({ "name": "cli" }))).await;
    let token = t["token"].as_str().unwrap().to_string();
    let url = serve(&app).await;
    let remote = url.replace("http://", &format!("http://alice:{token}@")) + "/alice/r.git";
    let (work, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    git(home.path(), work.path(), &["clone", "-q", &remote, "."]);
    Setup { app, alice, work, home }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn git_push_with_a_secret_is_refused_until_allowed_once() {
    let Setup { app, alice, work, home } = setup().await;
    let (h, w) = (home.path(), work.path());
    commit_file(h, w, "fine.txt", "nothing secret\n");
    commit_file(h, w, "conf/aws.env", &format!("# keys\nAWS_ACCESS_KEY_ID={}\n", aws()));
    let (ok, err) = push(h, w);
    assert!(!ok, "refused");
    assert!(err.contains("conf/aws.env:2") && err.contains("AWS access key ID") && err.contains("AKIA…2T"), "{err}");
    assert!(!err.contains(&aws()), "the secret isn't printed back: {err}");
    let main = |app: &App| {
        let p = app.state.git.repo_path(1);
        std::process::Command::new("git").arg(format!("--git-dir={}", p.display())).args(["rev-parse", "main"]).output().unwrap().stdout
    };
    let before = main(&app);
    let fps = allow_link(&err);
    assert_eq!(fps.len(), 1);

    // Someone else can't allow it for alice, and nobody can allow a made-up finding.
    let bob = app.signup("bob").await;
    let allow = |fps: Vec<String>| json!({ "fingerprints": fps, "reason": "test" });
    assert_ne!(app.req(&bob, "POST", "/api/repos/alice/r/security/bypasses", Some(allow(fps.clone()))).await.0, StatusCode::OK);
    let (s, _) = app.req(&alice, "POST", "/api/repos/alice/r/security/bypasses", Some(allow(vec!["0".repeat(20)]))).await;
    assert_eq!(s, StatusCode::NOT_FOUND, "only something a push refused can be allowed");
    let (s, _) = app.req(&alice, "POST", "/api/repos/alice/r/security/bypasses", Some(json!({ "fingerprints": fps, "reason": "nope" }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "a reason from the list");
    // The viewer sees what was refused, masked.
    let (_, sec) = app.req(&alice, "GET", "/api/repos/alice/r/security", None).await;
    assert_eq!(sec["blocked"][0]["fingerprint"], fps[0], "{sec}");
    assert_eq!(sec["blocked"][0]["path"], "conf/aws.env");
    assert!(!sec.to_string().contains(&aws()));
    assert_eq!(main(&app), before, "nothing landed");

    let (s, v) = app.req(&alice, "POST", "/api/repos/alice/r/security/bypasses", Some(allow(fps.clone()))).await;
    assert_eq!((s, v["allowed"].as_i64()), (StatusCode::OK, Some(1)), "{v}");
    let (ok, err) = push(h, w);
    assert!(ok, "allowed once: {err}");
    assert_ne!(main(&app), before);
    let (_, sec) = app.req(&alice, "GET", "/api/repos/alice/r/security", None).await;
    assert_eq!(sec["blocked"], json!([]), "what went through isn't listed as refused");

    // Once: the same secret added again is refused again; in another file it's another finding.
    commit_file(h, w, "conf/aws.env", &format!("# keys\nAWS_ACCESS_KEY_ID={0}\nAGAIN={0}\n", aws()));
    let (ok, err) = push(h, w);
    assert!(!ok, "the allowance was used up: {err}");
    assert_eq!(allow_link(&err), fps, "same file, same secret: same finding");
    git(h, w, &["reset", "-q", "--hard", "HEAD~1"]);
    commit_file(h, w, "other.env", &format!("K={}\n", aws()));
    let (ok, err) = push(h, w);
    assert!(!ok);
    assert_ne!(allow_link(&err), fps);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn private_keys_and_gitter_tokens_are_refused_with_file_line_and_kind() {
    // `_app`, not `..`: the server's data folder lives as long as the App.
    let Setup { app: _app, work, home, .. } = setup().await;
    let (h, w) = (home.path(), work.path());
    commit_file(h, w, "id_rsa", &pem());
    commit_file(h, w, "src/deploy.sh", &format!("#!/bin/sh\n\nexport TOKEN={}\n", gtr()));
    let (ok, err) = push(h, w);
    assert!(!ok);
    assert!(err.contains("id_rsa:1  Private key  -----BEGIN RSA PRIVATE KEY-----"), "{err}");
    assert!(err.contains("src/deploy.sh:3  Gitter access token  gtr_…9f"), "{err}");
    assert!(err.contains("2 things"), "{err}");
    assert_eq!(allow_link(&err).len(), 2);
    assert!(!err.contains(&gtr()) && !err.contains("MIIEpAIBAAKCAQEA3Tz2mr7SZiAMfQyuvBjM2Tg"), "{err}");
    // A clean push still works, and a branch made from pushed history isn't rescanned.
    git(h, w, &["reset", "-q", "--hard", "origin/main"]);
    commit_file(h, w, "notes.md", "token = get_token()\n");
    let (ok, err) = push(h, w);
    assert!(ok, "{err}");
    let out = git_cmd(h, w, &["push", "-q", "origin", "HEAD:refs/heads/copy"]).output().unwrap();
    assert!(out.status.success());
}

#[tokio::test]
async fn web_commits_with_a_secret_are_refused_until_allowed_once() {
    let app = App::new();
    app.signup("root1").await;
    let alice = app.signup("alice").await;
    app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "r", "readme": true }))).await;
    let body = json!({ "branch": "main", "message": "keys", "changes": [{ "path": "keys/prod.env", "content": format!("A=1\nAWS_ACCESS_KEY_ID={}\n", aws()) }] });
    let (s, v) = app.req(&alice, "POST", "/api/repos/alice/r/contents", Some(body.clone())).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{v}");
    assert_eq!(v["code"], "secret_detected");
    let f = &v["details"]["findings"][0];
    assert_eq!((f["path"].as_str(), f["line"].as_i64(), f["kind"].as_str()), (Some("keys/prod.env"), Some(2), Some("AWS access key ID")), "{v}");
    assert!(v["error"].as_str().unwrap().contains("keys/prod.env:2"));
    assert!(!v.to_string().contains(&aws()), "never echoed: {v}");
    let (_, files) = app.req(&alice, "GET", "/api/repos/alice/r/tree", None).await;
    assert!(!files.to_string().contains("keys"), "nothing committed");

    let fp = f["fingerprint"].as_str().unwrap().to_string();
    let (s, _) = app.req(&alice, "POST", "/api/repos/alice/r/security/bypasses", Some(json!({ "fingerprints": [fp], "reason": "false_positive" }))).await;
    assert_eq!(s, StatusCode::OK);
    let (s, v) = app.req(&alice, "POST", "/api/repos/alice/r/contents", Some(body)).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    // Editing the file again without adding the secret is fine; adding it again isn't.
    let (s, _) = app.req(&alice, "POST", "/api/repos/alice/r/contents", Some(json!({ "branch": "main", "message": "m", "changes": [{ "path": "keys/prod.env", "content": format!("A=2\nAWS_ACCESS_KEY_ID={}\n", aws()) }] }))).await;
    assert_eq!(s, StatusCode::OK, "an existing line isn't new");
    let (s, _) = app.req(&alice, "POST", "/api/repos/alice/r/contents", Some(json!({ "branch": "main", "message": "m", "changes": [{ "path": "keys/prod.env", "content": format!("A=2\nAWS_ACCESS_KEY_ID={0}\nB={0}\n", aws()) }] }))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "used up");
    // The audit log says who allowed what, masked.
    let n: i64 = app.state.db.call(|c| Ok(c.query_row("SELECT count(*) FROM audit WHERE action = 'secret_allowed' AND detail LIKE '%AKIA…2T%' AND detail NOT LIKE '%Q3EGRIW5%'", [], |r| r.get(0))?)).await.unwrap();
    assert_eq!(n, 1);
}

#[tokio::test]
async fn admins_scan_the_default_branch_on_demand() {
    let app = App::new();
    app.signup("root1").await;
    let alice = app.signup("alice").await;
    let bob = app.signup("bob").await;
    app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "r", "readme": true }))).await;
    // Put a secret on main past push protection (as history from before it existed).
    let repo = app.state.git.repo_path(1);
    let g = |args: &[&str], input: Option<&str>| {
        let mut c = std::process::Command::new("git");
        c.arg(format!("--git-dir={}", repo.display())).args(args).env("GIT_AUTHOR_NAME", "a").env("GIT_AUTHOR_EMAIL", "a@x").env("GIT_COMMITTER_NAME", "a").env("GIT_COMMITTER_EMAIL", "a@x");
        if let Some(i) = input {
            use std::io::Write;
            c.stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped());
            let mut ch = c.spawn().unwrap();
            ch.stdin.take().unwrap().write_all(i.as_bytes()).unwrap();
            return String::from_utf8(ch.wait_with_output().unwrap().stdout).unwrap().trim().to_string();
        }
        String::from_utf8(c.output().unwrap().stdout).unwrap().trim().to_string()
    };
    let blob = g(&["hash-object", "-w", "--stdin"], Some(&format!("x\ny\nkey: {}\n", gtr())));
    let tree = g(&["mktree"], Some(&format!("100644 blob {blob}\tsettings.yml\n")));
    let parent = g(&["rev-parse", "main"], None);
    let commit = g(&["commit-tree", &tree, "-p", &parent, "-m", "old"], None);
    g(&["update-ref", "refs/heads/main", &commit], None);

    let (s, _) = app.req(&bob, "POST", "/api/repos/alice/r/security/scan", None).await;
    assert_ne!(s, StatusCode::OK, "admins only");
    let (s, v) = app.req(&alice, "POST", "/api/repos/alice/r/security/scan", None).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["scan"]["commit"], commit);
    let f = &v["scan"]["findings"][0];
    assert_eq!((f["path"].as_str(), f["line"].as_i64(), f["kind"].as_str()), (Some("settings.yml"), Some(3), Some("Gitter access token")), "{v}");
    assert!(!v.to_string().contains(&gtr()));
    let (_, mine) = app.req(&alice, "GET", "/api/repos/alice/r/security", None).await;
    assert_eq!(mine["scan"]["findings"].as_array().map(Vec::len), Some(1), "{mine}");
    let (_, theirs) = app.req(&bob, "GET", "/api/repos/alice/r/security", None).await;
    assert!(theirs["scan"].is_null(), "findings are for admins: {theirs}");
}

// ---------- dependencies, vulnerabilities, SBOM ----------

const CARGO_LOCK: &str = r#"version = 3

[[package]]
name = "gitter"
version = "0.1.0"
dependencies = ["smallvec"]

[[package]]
name = "smallvec"
version = "1.6.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "abc"
"#;

const PACKAGE_LOCK: &str = r#"{
  "name": "web", "lockfileVersion": 3,
  "packages": {
    "": { "name": "web", "version": "1.0.0" },
    "node_modules/lodash": { "version": "4.17.20" },
    "node_modules/@babel/core": { "version": "7.22.0" },
    "node_modules/@babel/core/node_modules/semver": { "version": "6.3.0" }
  }
}"#;

const GO_SUM: &str = "golang.org/x/text v0.3.7 h1:abc=\ngolang.org/x/text v0.3.7/go.mod h1:def=\ngithub.com/pkg/errors v0.9.1 h1:x=\n";

const REQUIREMENTS: &str = "# web\nDjango==3.2.0 \\\n    --hash=sha256:abc\nrequests[socks]==2.31.0 ; python_version > '3'\n-r other.txt\nflask>=2\n";

const OSV: &str = r#"[
  { "id": "GHSA-test-0001", "summary": "smallvec buffer overflow", "aliases": ["CVE-2021-25900"], "modified": "2021-01-01T00:00:00Z",
    "database_specific": { "severity": "CRITICAL" },
    "affected": [{ "package": { "ecosystem": "crates.io", "name": "smallvec" },
      "ranges": [{ "type": "SEMVER", "events": [{ "introduced": "0.6.3" }, { "fixed": "0.6.14" }, { "introduced": "1.0.0" }, { "fixed": "1.6.1" }] }] }] },
  { "id": "GHSA-test-0002", "summary": "lodash prototype pollution",
    "affected": [{ "package": { "ecosystem": "npm", "name": "lodash" }, "versions": ["4.17.19", "4.17.20"] }] },
  { "id": "GHSA-test-0003", "summary": "fixed already",
    "affected": [{ "package": { "ecosystem": "PyPI", "name": "django" }, "ranges": [{ "type": "ECOSYSTEM", "events": [{ "introduced": "0" }, { "fixed": "3.1.0" }] }] }] },
  { "id": "GHSA-test-0004", "summary": "withdrawn", "withdrawn": "2022-01-01T00:00:00Z",
    "affected": [{ "package": { "ecosystem": "Go", "name": "golang.org/x/text" }, "versions": ["v0.3.7"] }] }
]"#;

async fn deps_repo(app: &App) -> Client {
    app.signup("root1").await;
    let alice = app.signup("alice").await;
    app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "r", "readme": true }))).await;
    let changes = json!([
        { "path": "Cargo.lock", "content": CARGO_LOCK },
        { "path": "web/package-lock.json", "content": PACKAGE_LOCK },
        { "path": "go.sum", "content": GO_SUM },
        { "path": "requirements.txt", "content": REQUIREMENTS },
        { "path": "vendor/x/go.sum", "content": "ignored v1.0.0 h1:x=\n" },
    ]);
    let (s, v) = app.req(&alice, "POST", "/api/repos/alice/r/contents", Some(json!({ "branch": "main", "message": "locks", "changes": changes }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    alice
}

#[tokio::test]
async fn deps_are_parsed_from_lockfiles() {
    let app = App::new();
    let alice = deps_repo(&app).await;
    let (s, v) = app.req(&alice, "GET", "/api/repos/alice/r/dependencies", None).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let got: Vec<String> = v["dependencies"].as_array().unwrap().iter().map(|d| format!("{} {} {}", d["ecosystem"].as_str().unwrap(), d["name"].as_str().unwrap(), d["version"].as_str().unwrap())).collect();
    for want in [
        "crates.io smallvec 1.6.0",
        "npm lodash 4.17.20",
        "npm @babel/core 7.22.0",
        "npm semver 6.3.0",
        "Go golang.org/x/text v0.3.7",
        "Go github.com/pkg/errors v0.9.1",
        "PyPI Django 3.2.0",
        "PyPI requests 2.31.0",
    ] {
        assert!(got.contains(&want.to_string()), "{want} in {got:?}");
    }
    assert!(!got.iter().any(|g| g.contains("gitter 0.1.0") || g.contains("flask") || g.contains("ignored") || g.contains(" web ")), "{got:?}");
    assert_eq!(got.iter().filter(|g| g.contains("x/text")).count(), 1, "go.mod lines don't double up");
    let files: Vec<&str> = v["files"].as_array().unwrap().iter().map(|f| f["path"].as_str().unwrap()).collect();
    assert_eq!(files, ["Cargo.lock", "go.sum", "requirements.txt", "web/package-lock.json"]);
    assert!(v["vulnerabilities"].as_array().unwrap().is_empty(), "no OSV data yet");
}

#[tokio::test]
async fn deps_vulnerabilities_come_from_an_imported_osv_snapshot() {
    let app = App::new();
    let alice = deps_repo(&app).await;
    let (_, _, cookie) = app.call(&Client::default(), "POST", "/api/auth/login", Some(json!({ "login": "root1", "password": "correct horse" })), Some(common::ORIGIN)).await;
    let root = Client(cookie);
    assert!(root.0.is_some(), "root1 signed in");
    // Only the site admin imports.
    let import = |who: &Client, body: Vec<u8>| {
        let req = axum::http::Request::post("/api/admin/osv")
            .header("host", "test.local")
            .header("origin", common::ORIGIN)
            .header("cookie", format!("gitter_session={}", who.0.clone().unwrap_or_default()))
            .header("content-type", "application/octet-stream")
            .body(axum::body::Body::from(body))
            .unwrap();
        let router = app.router.clone();
        async move {
            let resp = tower::ServiceExt::oneshot(router, req).await.unwrap();
            let s = resp.status();
            let b = http_body_util::BodyExt::collect(resp.into_body()).await.unwrap().to_bytes();
            (s, serde_json::from_slice::<Value>(&b).unwrap_or(Value::Null))
        }
    };
    let (s, _) = import(&alice, OSV.into()).await;
    assert_ne!(s, StatusCode::OK, "site admins only");
    let (s, v) = import(&root, OSV.into()).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!((v["imported"].as_i64(), v["withdrawn"].as_i64()), (Some(3), Some(1)), "{v}");
    // A zip of OSV files (what osv.dev publishes per ecosystem) works too, and re-importing replaces.
    let (s, v) = import(&root, zip(&[("GHSA-test-0002.json", br#"{ "id": "GHSA-test-0002", "summary": "lodash, updated", "affected": [{ "package": { "ecosystem": "npm", "name": "lodash" }, "versions": ["4.17.20"] }] }"#)])).await;
    assert_eq!((s, v["imported"].as_i64()), (StatusCode::OK, Some(1)), "{v}");
    let (s, _) = import(&root, b"not osv".to_vec()).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    let (_, v) = app.req(&alice, "GET", "/api/repos/alice/r/dependencies", None).await;
    let vulns: Vec<String> = v["vulnerabilities"].as_array().unwrap().iter().map(|x| format!("{} {} {} {}", x["id"].as_str().unwrap(), x["name"].as_str().unwrap(), x["version"].as_str().unwrap(), x["fixed"].as_str().unwrap_or("-"))).collect();
    assert_eq!(vulns, ["GHSA-test-0001 smallvec 1.6.0 1.6.1", "GHSA-test-0002 lodash 4.17.20 -"], "{v}");
    assert_eq!(v["vulnerabilities"][0]["severity"], "CRITICAL");
    assert_eq!(v["vulnerabilities"][1]["summary"], "lodash, updated");
    assert_eq!(v["osv"]["advisories"], 3);
}

#[tokio::test]
async fn deps_download_as_spdx_and_cyclonedx_sboms() {
    let app = App::new();
    let alice = deps_repo(&app).await;
    let (s, spdx) = app.req(&alice, "GET", "/api/repos/alice/r/sbom?format=spdx", None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(spdx["spdxVersion"], "SPDX-2.3");
    assert_eq!(spdx["dataLicense"], "CC0-1.0");
    assert!(spdx["documentNamespace"].as_str().unwrap().starts_with("http://test.local/alice/r/sbom/"));
    let pkgs = spdx["packages"].as_array().unwrap();
    let lodash = pkgs.iter().find(|p| p["name"] == "lodash").unwrap();
    assert_eq!(lodash["versionInfo"], "4.17.20");
    assert_eq!(lodash["externalRefs"][0]["referenceLocator"], "pkg:npm/lodash@4.17.20");
    let babel = pkgs.iter().find(|p| p["name"] == "@babel/core").unwrap();
    assert_eq!(babel["externalRefs"][0]["referenceLocator"], "pkg:npm/%40babel/core@7.22.0");
    assert!(spdx["relationships"].as_array().unwrap().iter().any(|r| r["relationshipType"] == "DESCRIBES"));
    let ids: std::collections::HashSet<&str> = pkgs.iter().map(|p| p["SPDXID"].as_str().unwrap()).collect();
    assert_eq!(ids.len(), pkgs.len(), "SPDX ids are unique");
    assert!(ids.iter().all(|i| i.starts_with("SPDXRef-") && i[8..].chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')));

    let (s, cdx) = app.req(&alice, "GET", "/api/repos/alice/r/sbom?format=cyclonedx", None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!((cdx["bomFormat"].as_str(), cdx["specVersion"].as_str()), (Some("CycloneDX"), Some("1.5")));
    assert!(cdx["serialNumber"].as_str().unwrap().starts_with("urn:uuid:"));
    let comps = cdx["components"].as_array().unwrap();
    assert!(comps.iter().any(|c| c["purl"] == "pkg:cargo/smallvec@1.6.0" && c["type"] == "library"));
    assert!(comps.iter().any(|c| c["purl"] == "pkg:golang/golang.org/x/text@v0.3.7"));
    assert!(comps.iter().any(|c| c["purl"] == "pkg:pypi/django@3.2.0"));
    assert_eq!(app.req(&alice, "GET", "/api/repos/alice/r/sbom?format=xml", None).await.0, StatusCode::BAD_REQUEST);
    // Private repositories' SBOMs are private.
    assert_eq!(app.req(&alice, "PATCH", "/api/repos/alice/r", Some(json!({ "private": true }))).await.0, StatusCode::OK);
    assert_eq!(app.req(&Client::default(), "GET", "/api/repos/alice/r/sbom?format=spdx", None).await.0, StatusCode::NOT_FOUND);
}

/// A zip archive of deflated files.
fn zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Write;
    let (mut out, mut central) = (Vec::new(), Vec::new());
    for (name, data) in files {
        let mut enc = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(data).unwrap();
        let comp = enc.finish().unwrap();
        let off = out.len() as u32;
        let le16 = |v: &mut Vec<u8>, x: u16| v.extend_from_slice(&x.to_le_bytes());
        let le32 = |v: &mut Vec<u8>, x: u32| v.extend_from_slice(&x.to_le_bytes());
        le32(&mut out, 0x04034b50);
        for x in [20u16, 0, 8, 0, 0] {
            le16(&mut out, x);
        }
        for x in [0u32, comp.len() as u32, data.len() as u32] {
            le32(&mut out, x);
        }
        le16(&mut out, name.len() as u16);
        le16(&mut out, 0);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&comp);
        le32(&mut central, 0x02014b50);
        for x in [20u16, 20, 0, 8, 0, 0] {
            le16(&mut central, x);
        }
        for x in [0u32, comp.len() as u32, data.len() as u32] {
            le32(&mut central, x);
        }
        for x in [name.len() as u16, 0, 0, 0, 0] {
            le16(&mut central, x);
        }
        le32(&mut central, 0);
        le32(&mut central, off);
        central.extend_from_slice(name.as_bytes());
    }
    let (cd_off, cd_len) = (out.len() as u32, central.len() as u32);
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x06054b50u32.to_le_bytes());
    for x in [0u16, 0, files.len() as u16, files.len() as u16] {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out.extend_from_slice(&cd_len.to_le_bytes());
    out.extend_from_slice(&cd_off.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}
