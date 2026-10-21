//! In-process API tests: the real router over a temporary data directory.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use gitter::{AppState, Config, SignupMode, db::Db};

const ORIGIN: &str = "http://test.local";

struct App {
    router: axum::Router,
    state: AppState,
    _dir: tempfile::TempDir,
}

/// A signed-in (or anonymous) client: just a session cookie.
#[derive(Clone, Default)]
struct Client(Option<String>);

impl App {
    fn new() -> App {
        let dir = tempfile::tempdir().unwrap();
        for d in ["repos", "home"] {
            std::fs::create_dir_all(dir.path().join(d)).unwrap();
        }
        let cfg = Config {
            data_dir: dir.path().to_path_buf(),
            addr: String::new(),
            url: ORIGIN.into(),
            secure_cookies: false,
            trust_proxy: false,
            signup: SignupMode::Open,
            max_push_bytes: 10 * 1024 * 1024,
            max_repos_per_user: 50,
        };
        let state = AppState::new(cfg, Db::memory().unwrap());
        App {
            router: gitter::router(state.clone()),
            state,
            _dir: dir,
        }
    }

    async fn call(
        &self,
        c: &Client,
        method: &str,
        path: &str,
        body: Option<Value>,
        origin: Option<&str>,
    ) -> (StatusCode, Value, Option<String>) {
        let mut b = Request::builder()
            .method(method)
            .uri(path)
            .header("host", "test.local");
        if let Some(o) = origin {
            b = b.header("origin", o);
        }
        if let Some(cookie) = &c.0 {
            b = b.header("cookie", format!("gitter_session={cookie}"));
        }
        let req = match body {
            Some(v) => b
                .header("content-type", "application/json")
                .body(Body::from(v.to_string()))
                .unwrap(),
            None => b.body(Body::empty()).unwrap(),
        };
        let resp = self.router.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let set_cookie = resp
            .headers()
            .get("set-cookie")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(';').next())
            .and_then(|kv| kv.strip_prefix("gitter_session="))
            .map(String::from);
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, json, set_cookie)
    }

    async fn req(
        &self,
        c: &Client,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let (s, v, _) = self.call(c, method, path, body, Some(ORIGIN)).await;
        (s, v)
    }

    async fn signup(&self, name: &str) -> Client {
        let body = json!({ "username": name, "email": format!("{name}@example.com"), "password": "correct horse" });
        let (s, _, cookie) = self
            .call(
                &Client::default(),
                "POST",
                "/api/auth/signup",
                Some(body),
                Some(ORIGIN),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "signup {name}");
        Client(cookie)
    }
}

#[tokio::test]
async fn cross_site_writes_are_blocked() {
    let app = App::new();
    let alice = app.signup("alice").await;
    let (s, _, _) = app
        .call(
            &alice,
            "POST",
            "/api/repos",
            Some(json!({ "name": "x" })),
            Some("https://evil.example"),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _, _) = app
        .call(
            &alice,
            "POST",
            "/api/repos",
            Some(json!({ "name": "x" })),
            None,
        )
        .await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "cookie writes without an Origin are refused"
    );
    let (s, _) = app
        .req(&alice, "POST", "/api/repos", Some(json!({ "name": "x" })))
        .await;
    assert_eq!(s, StatusCode::OK);
}

#[tokio::test]
async fn private_repos_are_invisible_to_others() {
    let app = App::new();
    let alice = app.signup("alice").await;
    let bob = app.signup("bob").await;
    app.req(
        &alice,
        "POST",
        "/api/repos",
        Some(json!({ "name": "secret", "private": true, "readme": true })),
    )
    .await;
    assert_eq!(
        app.req(&alice, "GET", "/api/repos/alice/secret", None)
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        app.req(&bob, "GET", "/api/repos/alice/secret", None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.req(&Client::default(), "GET", "/api/repos/alice/secret", None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.req(&bob, "GET", "/api/repos/alice/secret/issues", None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    let (_, found) = app.req(&bob, "GET", "/api/search?q=secret", None).await;
    assert_eq!(found["repos"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn only_authors_and_writers_edit_comments_and_labels() {
    let app = App::new();
    let alice = app.signup("alice").await;
    let bob = app.signup("bob").await;
    let carol = app.signup("carol").await;
    app.req(
        &alice,
        "POST",
        "/api/repos",
        Some(json!({ "name": "proj", "readme": true })),
    )
    .await;
    app.req(
        &bob,
        "POST",
        "/api/repos/alice/proj/issues",
        Some(json!({ "title": "Bug", "body": "it breaks" })),
    )
    .await;
    let (_, c) = app
        .req(
            &bob,
            "POST",
            "/api/repos/alice/proj/issues/1/comments",
            Some(json!({ "body": "details" })),
        )
        .await;
    let id = c["id"].as_i64().unwrap();
    let edit = |who: Client| {
        let app = &app;
        async move {
            app.req(
                &who,
                "PATCH",
                &format!("/api/repos/alice/proj/comments/{id}"),
                Some(json!({ "body": "changed" })),
            )
            .await
            .0
        }
    };
    assert_eq!(edit(carol.clone()).await, StatusCode::FORBIDDEN);
    assert_eq!(edit(bob.clone()).await, StatusCode::OK);
    assert_eq!(
        edit(alice.clone()).await,
        StatusCode::OK,
        "repo owner can moderate"
    );
    let label = json!({ "name": "urgent", "color": "#ff0000" });
    assert_eq!(
        app.req(
            &bob,
            "POST",
            "/api/repos/alice/proj/labels",
            Some(label.clone())
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        app.req(&alice, "POST", "/api/repos/alice/proj/labels", Some(label))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        app.req(
            &carol,
            "PATCH",
            "/api/repos/alice/proj/issues/1",
            Some(json!({ "state": "closed" }))
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        app.req(
            &bob,
            "PATCH",
            "/api/repos/alice/proj/issues/1",
            Some(json!({ "state": "closed" }))
        )
        .await
        .0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn bearer_tokens_authenticate_and_bad_ones_fail() {
    let app = App::new();
    let alice = app.signup("alice").await;
    let (_, t) = app
        .req(
            &alice,
            "POST",
            "/api/me/tokens",
            Some(json!({ "name": "ci" })),
        )
        .await;
    let token = t["token"].as_str().unwrap().to_string();
    let get = |auth: String| {
        let app = &app;
        async move {
            let req = Request::builder()
                .uri("/api/me")
                .header("authorization", auth)
                .body(Body::empty())
                .unwrap();
            let resp = app.router.clone().oneshot(req).await.unwrap();
            let status = resp.status();
            let v: Value =
                serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes())
                    .unwrap_or(Value::Null);
            (status, v)
        }
    };
    let (s, v) = get(format!("Bearer {token}")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["user"]["username"], "alice");
    assert_eq!(
        get("Bearer gtr_nope".into()).await.0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn emails_are_unique_and_password_change_signs_out_elsewhere() {
    let app = App::new();
    let alice = app.signup("alice").await;
    let bob = app.signup("bob").await;
    let (s, _) = app
        .req(
            &bob,
            "PATCH",
            "/api/me",
            Some(json!({ "email": "ALICE@example.com" })),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);

    let (_, _, other) = app
        .call(
            &Client::default(),
            "POST",
            "/api/auth/login",
            Some(json!({ "login": "alice", "password": "correct horse" })),
            Some(ORIGIN),
        )
        .await;
    let other = Client(other);
    assert_eq!(
        app.req(&other, "GET", "/api/me", None).await.1["user"]["username"],
        "alice"
    );
    let (s, _, fresh) = app
        .call(
            &alice,
            "POST",
            "/api/me/password",
            Some(json!({ "current": "correct horse", "new": "battery staple" })),
            Some(ORIGIN),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert!(fresh.is_some(), "this browser gets a new session");
    assert!(
        app.req(&other, "GET", "/api/me", None).await.1["user"].is_null(),
        "other sessions are signed out"
    );
    assert_eq!(
        app.req(&Client(fresh), "GET", "/api/me", None).await.1["user"]["username"],
        "alice"
    );
}

#[tokio::test]
async fn web_commits_keep_exec_bits_and_hide_account_email() {
    let app = App::new();
    let alice = app.signup("alice").await;
    app.req(
        &alice,
        "POST",
        "/api/repos",
        Some(json!({ "name": "tools", "readme": true })),
    )
    .await;
    let repo = app.state.git.repo_path(1);
    // Add an executable script with plumbing, as a push would.
    let git = |args: &[&str], input: Option<&str>| {
        use std::io::Write;
        let mut c = std::process::Command::new("git");
        c.arg(format!("--git-dir={}", repo.display()))
            .args(args)
            .stdout(std::process::Stdio::piped())
            .stdin(std::process::Stdio::piped());
        let mut child = c.spawn().unwrap();
        if let Some(i) = input {
            child.stdin.take().unwrap().write_all(i.as_bytes()).unwrap();
        } else {
            drop(child.stdin.take());
        }
        String::from_utf8(child.wait_with_output().unwrap().stdout)
            .unwrap()
            .trim()
            .to_string()
    };
    let blob = git(
        &["hash-object", "-w", "--stdin"],
        Some("#!/bin/sh\necho hi\n"),
    );
    let head = git(&["rev-parse", "main"], None);
    let readme = git(&["rev-parse", "main:README.md"], None);
    let tree = git(
        &["mktree"],
        Some(&format!(
            "100644 blob {readme}\tREADME.md\n100755 blob {blob}\trun.sh\n"
        )),
    );
    let commit = git(
        &[
            "-c",
            "user.name=a",
            "-c",
            "user.email=a@b.c",
            "commit-tree",
            &tree,
            "-p",
            &head,
            "-m",
            "add script",
        ],
        None,
    );
    git(&["update-ref", "refs/heads/main", &commit], None);

    let (s, _) = app
        .req(&alice, "POST", "/api/repos/alice/tools/contents", Some(json!({
            "branch": "main", "message": "Tweak", "changes": [{ "path": "run.sh", "content": "#!/bin/sh\necho hello\n" }]
        })))
        .await;
    assert_eq!(s, StatusCode::OK);
    assert!(
        git(&["ls-tree", "main", "run.sh"], None).starts_with("100755"),
        "exec bit kept"
    );
    let email = git(&["log", "-1", "--format=%ae", "main"], None);
    assert!(
        email.ends_with("@users.noreply.gitter"),
        "commit email is the no-reply address, got {email}"
    );
    let (_, blob) = app
        .req(
            &alice,
            "GET",
            "/api/repos/alice/tools/blob?rev=main&path=run.sh",
            None,
        )
        .await;
    assert_eq!(blob["editable"], true);
    assert_eq!(blob["mode"], "100755");

    // Deleting a file works in a bare repository (no work tree).
    let (s, e) = app
        .req(&alice, "POST", "/api/repos/alice/tools/contents", Some(json!({
            "branch": "main", "message": "Drop script", "changes": [{ "path": "run.sh", "delete": true }]
        })))
        .await;
    assert_eq!(s, StatusCode::OK, "{e}");
    assert_eq!(git(&["ls-tree", "--name-only", "main"], None), "README.md");
}

#[tokio::test]
async fn non_utf8_files_are_not_editable() {
    let app = App::new();
    let alice = app.signup("alice").await;
    app.req(
        &alice,
        "POST",
        "/api/repos",
        Some(json!({ "name": "legacy" })),
    )
    .await;
    let b64 = "Y2Fm6Qo="; // "caf\xe9\n" (Latin-1)
    let crlf = "YQ0KYg0K"; // "a\r\nb\r\n"
    let (s, _) = app
        .req(&alice, "POST", "/api/repos/alice/legacy/contents", Some(json!({
            "branch": "main", "message": "init",
            "changes": [{ "path": "latin1.txt", "content": b64, "base64": true }, { "path": "dos.txt", "content": crlf, "base64": true }]
        })))
        .await;
    assert_eq!(s, StatusCode::OK);
    let (_, latin) = app
        .req(
            &alice,
            "GET",
            "/api/repos/alice/legacy/blob?rev=main&path=latin1.txt",
            None,
        )
        .await;
    assert_eq!(latin["editable"], false);
    let (_, dos) = app
        .req(
            &alice,
            "GET",
            "/api/repos/alice/legacy/blob?rev=main&path=dos.txt",
            None,
        )
        .await;
    assert_eq!(dos["editable"], true);
    assert_eq!(dos["eol"], "crlf");
}

#[tokio::test]
async fn healthz_reports_ok() {
    let app = App::new();
    let (s, v) = app.req(&Client::default(), "GET", "/healthz", None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["ok"], true);
}

#[tokio::test]
async fn admin_controls_settings_and_accounts() {
    let app = App::new();
    let admin = app.signup("root").await;
    let bob = app.signup("bob").await;
    assert_eq!(
        app.req(&bob, "GET", "/api/admin/overview", None).await.0,
        StatusCode::FORBIDDEN
    );
    let (s, o) = app.req(&admin, "GET", "/api/admin/overview", None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(o["users"], 2);

    // Closing sign-up in the UI takes effect immediately.
    let (_, mut cur) = app.req(&admin, "GET", "/api/admin/settings", None).await;
    cur["signup"] = json!("closed");
    cur["instance_name"] = json!("Acme Git");
    assert_eq!(
        app.req(&admin, "PUT", "/api/admin/settings", Some(cur))
            .await
            .0,
        StatusCode::OK
    );
    let body =
        json!({ "username": "eve", "email": "eve@example.com", "password": "correct horse" });
    assert_eq!(
        app.req(&Client::default(), "POST", "/api/auth/signup", Some(body))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        app.req(&Client::default(), "GET", "/api/me", None).await.1["instance"]["name"],
        "Acme Git"
    );

    // Suspending signs the person out and blocks new sign-ins.
    assert_eq!(
        app.req(
            &admin,
            "PATCH",
            "/api/admin/users/bob",
            Some(json!({ "suspended": true }))
        )
        .await
        .0,
        StatusCode::OK
    );
    assert!(app.req(&bob, "GET", "/api/me", None).await.1["user"].is_null());
    let login = json!({ "login": "bob", "password": "correct horse" });
    assert_eq!(
        app.req(&Client::default(), "POST", "/api/auth/login", Some(login))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    // Admins can't lock themselves out.
    assert_eq!(
        app.req(
            &admin,
            "PATCH",
            "/api/admin/users/root",
            Some(json!({ "is_admin": false }))
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn first_account_needs_the_setup_code() {
    let app = App::new();
    *app.state.setup_code.lock().unwrap() = Some("s3tup-code".into());
    let mut body =
        json!({ "username": "root", "email": "root@example.com", "password": "correct horse" });
    assert_eq!(
        app.req(
            &Client::default(),
            "POST",
            "/api/auth/signup",
            Some(body.clone())
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    body["setup"] = json!("wrong-code");
    assert_eq!(
        app.req(
            &Client::default(),
            "POST",
            "/api/auth/signup",
            Some(body.clone())
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    body["setup"] = json!("s3tup-code");
    let (s, v) = app
        .req(&Client::default(), "POST", "/api/auth/signup", Some(body))
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["user"]["is_admin"], true);
    assert!(
        app.state.setup_code.lock().unwrap().is_none(),
        "the code works once"
    );
    // Everyone after the admin signs up normally.
    app.signup("bob").await;
}

#[tokio::test]
async fn the_last_admin_cannot_be_removed() {
    let app = App::new();
    let root = app.signup("root").await;
    let ops = app.signup("ops").await;
    assert_eq!(
        app.req(
            &root,
            "PATCH",
            "/api/admin/users/ops",
            Some(json!({ "is_admin": true }))
        )
        .await
        .0,
        StatusCode::OK
    );
    // ops demotes root: fine, ops is still an admin.
    assert_eq!(
        app.req(
            &ops,
            "PATCH",
            "/api/admin/users/root",
            Some(json!({ "is_admin": false }))
        )
        .await
        .0,
        StatusCode::OK
    );
    // Nobody else can now remove ops, the last admin; and ops can't change itself.
    assert_eq!(
        app.req(
            &ops,
            "PATCH",
            "/api/admin/users/ops",
            Some(json!({ "is_admin": false }))
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let (_, me) = app.req(&root, "GET", "/api/me", None).await;
    assert_eq!(me["user"]["is_admin"], false);
}

#[tokio::test]
async fn merge_rolls_back_when_it_cannot_be_recorded() {
    let app = App::new();
    let alice = app.signup("alice").await;
    app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "m", "readme": true })))
        .await;
    let (s, _) = app
        .req(&alice, "POST", "/api/repos/alice/m/contents", Some(json!({
            "branch": "main", "new_branch": "feature", "message": "Add",
            "changes": [{ "path": "a.txt", "content": "a\n" }]
        })))
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, pr) = app
        .req(&alice, "POST", "/api/repos/alice/m/pulls", Some(json!({
            "title": "Add a", "head": "feature", "base": "main"
        })))
        .await;
    assert_eq!(s, StatusCode::OK, "{pr}");
    let n = pr["number"].as_i64().unwrap();
    let main_before = app.state.git.resolve(&app.state.git.repo_path(1), "refs/heads/main").await.unwrap();
    app.state
        .db
        .call(|c| {
            Ok(c.execute_batch(
                "CREATE TRIGGER no_merge BEFORE UPDATE ON pulls BEGIN SELECT RAISE(ABORT, 'disk full'); END",
            )?)
        })
        .await
        .unwrap();
    for method in ["squash", "merge", "ff"] {
        let (s, _) = app
            .req(&alice, "POST", &format!("/api/repos/alice/m/pulls/{n}/merge"), Some(json!({ "method": method })))
            .await;
        assert!(!s.is_success(), "{method}: merge must fail, got {s}");
        let main_after = app.state.git.resolve(&app.state.git.repo_path(1), "refs/heads/main").await.unwrap();
        assert_eq!(main_before, main_after, "{method}: main must not move when the merge isn't recorded");
    }
    let (_, issue) = app.req(&alice, "GET", &format!("/api/repos/alice/m/issues/{n}"), None).await;
    assert_eq!(issue["issue"]["state"], "open");
}

#[tokio::test]
async fn single_file_diff_loads_a_file_too_big_for_the_full_view() {
    let app = App::new();
    let alice = app.signup("alice").await;
    app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "big", "readme": true })))
        .await;
    let content: String = (0..5000).map(|i| format!("row {i}\n")).collect();
    let (s, w) = app
        .req(&alice, "POST", "/api/repos/alice/big/contents", Some(json!({
            "branch": "main", "message": "Add table", "changes": [{ "path": "t.csv", "content": content }]
        })))
        .await;
    assert_eq!(s, StatusCode::OK, "{w}");
    let sha = w["sha"].as_str().expect("commit sha").to_string();
    let (_, full) = app.req(&alice, "GET", &format!("/api/repos/alice/big/commit/{sha}"), None).await;
    assert_eq!(full["diff"]["files"][0]["too_large"], true);
    let (s, one) = app
        .req(&alice, "GET", &format!("/api/repos/alice/big/commit/{sha}?path=t.csv"), None)
        .await;
    assert_eq!(s, StatusCode::OK);
    let f = &one["diff"]["files"][0];
    assert_eq!(f["too_large"], false);
    assert_eq!(f["hunks"][0]["lines"].as_array().unwrap().len(), 5000);
    let (s, _) = app
        .req(&alice, "GET", &format!("/api/repos/alice/big/commit/{sha}?path=../etc"), None)
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

async fn status_with(app: &App, req: Request<Body>) -> StatusCode {
    app.router.clone().oneshot(req).await.unwrap().status()
}

async fn login(app: &App, body: Value) -> (StatusCode, Value, Option<String>) {
    app.call(&Client::default(), "POST", "/api/auth/login", Some(body), Some(ORIGIN)).await
}

#[tokio::test]
async fn two_factor_sign_in_codes_and_git() {
    use gitter::security::{step_now, totp_code};
    let app = App::new();
    let alice = app.signup("alice").await;
    app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "p", "private": true, "readme": true }))).await;
    let (s, setup) = app.req(&alice, "POST", "/api/me/2fa/setup", None).await;
    assert_eq!(s, StatusCode::OK, "{setup}");
    let secret = setup["secret"].as_str().unwrap().to_string();
    assert!(setup["qr"].as_str().unwrap().contains("<svg"));
    assert!(setup["uri"].as_str().unwrap().starts_with("otpauth://totp/"));
    let (s, _) = app.req(&alice, "POST", "/api/me/2fa/enable", Some(json!({ "code": "000000" }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "a wrong code doesn't enable 2FA");
    let now = step_now();
    let (s, on) = app.req(&alice, "POST", "/api/me/2fa/enable", Some(json!({ "code": totp_code(&secret, now).unwrap() }))).await;
    assert_eq!(s, StatusCode::OK, "{on}");
    let codes: Vec<String> = serde_json::from_value(on["recovery_codes"].clone()).unwrap();
    assert_eq!(codes.len(), 10);

    // The password alone no longer signs in.
    let pw = json!({ "login": "alice", "password": "correct horse" });
    let (s, step1, cookie) = login(&app, pw.clone()).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(step1["two_factor"], true);
    assert!(cookie.is_none(), "no session before the code");
    let challenge = step1["challenge"].as_str().unwrap().to_string();
    let two = |code: String| json!({ "challenge": challenge, "code": code });
    let (s, _, _) = app.call(&Client::default(), "POST", "/api/auth/login/2fa", Some(two("123456".into())), Some(ORIGIN)).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    // The code used to enable is spent; the next one works, once.
    let next = totp_code(&secret, now + 1).unwrap();
    let (s, _, cookie) = app.call(&Client::default(), "POST", "/api/auth/login/2fa", Some(two(next.clone())), Some(ORIGIN)).await;
    assert_eq!(s, StatusCode::OK);
    assert!(cookie.is_some());
    let (_, step1, _) = login(&app, pw.clone()).await;
    let replay = json!({ "challenge": step1["challenge"], "code": next });
    let (s, _, _) = app.call(&Client::default(), "POST", "/api/auth/login/2fa", Some(replay), Some(ORIGIN)).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "a code can't be replayed");

    // A recovery code works exactly once.
    for (i, expect) in [StatusCode::OK, StatusCode::UNAUTHORIZED].into_iter().enumerate() {
        let (_, step1, _) = login(&app, pw.clone()).await;
        let body = json!({ "challenge": step1["challenge"], "code": codes[0].to_uppercase() });
        let (s, _, _) = app.call(&Client::default(), "POST", "/api/auth/login/2fa", Some(body), Some(ORIGIN)).await;
        assert_eq!(s, expect, "recovery code use #{}", i + 1);
    }

    // git over HTTP: the password is refused, a token still works.
    let basic = |secret: &str| {
        use base64::Engine;
        format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(format!("alice:{secret}")))
    };
    let git = |auth: String| {
        Request::get("/alice/p.git/info/refs?service=git-upload-pack")
            .header("host", "test.local")
            .header("authorization", auth)
            .body(Body::empty())
            .unwrap()
    };
    assert_eq!(status_with(&app, git(basic("correct horse"))).await, StatusCode::UNAUTHORIZED);
    let (_, t) = app.req(&alice, "POST", "/api/me/tokens", Some(json!({ "name": "ci" }))).await;
    assert_eq!(status_with(&app, git(basic(t["token"].as_str().unwrap()))).await, StatusCode::OK);

    let (_, sec) = app.req(&alice, "GET", "/api/me/security", None).await;
    assert_eq!(sec["two_factor"], true);
    assert_eq!(sec["recovery_left"], 9);
}

#[tokio::test]
async fn sessions_are_listed_and_revocable() {
    let app = App::new();
    let first = app.signup("alice").await;
    let (_, _, second) = login(&app, json!({ "login": "alice", "password": "correct horse" })).await;
    let second = Client(second);
    let (_, sec) = app.req(&first, "GET", "/api/me/security", None).await;
    let sessions = sec["sessions"].as_array().unwrap();
    assert_eq!(sessions.len(), 2);
    let other = sessions.iter().find(|s| s["current"] == false).unwrap()["id"].as_str().unwrap().to_string();
    let (s, _) = app.req(&first, "DELETE", &format!("/api/me/sessions/{other}"), None).await;
    assert_eq!(s, StatusCode::OK);
    assert!(app.req(&second, "GET", "/api/me", None).await.1["user"].is_null(), "revoked session is signed out");
    assert_eq!(app.req(&first, "GET", "/api/me", None).await.1["user"]["username"], "alice");
    let (_, _, third) = login(&app, json!({ "login": "alice", "password": "correct horse" })).await;
    let (_, r) = app.req(&first, "POST", "/api/me/sessions/revoke-others", None).await;
    assert_eq!(r["revoked"], 1);
    assert!(app.req(&Client(third), "GET", "/api/me", None).await.1["user"].is_null());
}

#[tokio::test]
async fn step_up_is_needed_for_dangerous_actions() {
    let app = App::new();
    let alice = app.signup("alice").await;
    app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "r" }))).await;
    // Pretend the password was last confirmed an hour ago.
    app.state.db.call(|c| Ok(c.execute("UPDATE sessions SET verified_at = verified_at - 3600", [])?)).await.unwrap();
    let (s, e) = app.req(&alice, "DELETE", "/api/repos/alice/r", Some(json!({ "confirm": "alice/r" }))).await;
    assert_eq!((s, e["code"].as_str()), (StatusCode::FORBIDDEN, Some("reauth")));
    let (s, _) = app.req(&alice, "POST", "/api/me/tokens", Some(json!({ "name": "x" }))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = app.req(&alice, "PATCH", "/api/repos/alice/r", Some(json!({ "private": true }))).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "changing visibility needs it too");
    let (s, _) = app.req(&alice, "PATCH", "/api/repos/alice/r", Some(json!({ "description": "fine" }))).await;
    assert_eq!(s, StatusCode::OK, "harmless edits don't");
    let (s, _) = app.req(&alice, "POST", "/api/auth/reauth", Some(json!({ "password": "wrong" }))).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _) = app.req(&alice, "POST", "/api/auth/reauth", Some(json!({ "password": "correct horse" }))).await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = app.req(&alice, "DELETE", "/api/repos/alice/r", Some(json!({ "confirm": "alice/r" }))).await;
    assert_eq!(s, StatusCode::OK);
}

#[tokio::test]
async fn audit_log_records_security_events_in_an_intact_chain() {
    let app = App::new();
    let alice = app.signup("alice").await;
    let bob = app.signup("bob").await;
    app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "r" }))).await;
    app.req(&alice, "DELETE", "/api/repos/alice/r", Some(json!({ "confirm": "alice/r" }))).await;
    app.req(&alice, "PATCH", "/api/admin/users/bob", Some(json!({ "suspended": true }))).await;
    let (_, mine) = app.req(&alice, "GET", "/api/me/audit", None).await;
    let actions: Vec<&str> = mine["events"].as_array().unwrap().iter().map(|e| e["action"].as_str().unwrap()).collect();
    assert!(actions.contains(&"repo_deleted") && actions.contains(&"signup"), "{actions:?}");
    let (s, _) = app.req(&bob, "GET", "/api/admin/audit", None).await;
    assert_ne!(s, StatusCode::OK, "only admins see everyone's log");
    let (_, all) = app.req(&alice, "GET", "/api/admin/audit", None).await;
    assert_eq!(all["integrity"]["ok"], true);
    assert!(all["events"].as_array().unwrap().iter().any(|e| e["action"] == "user_suspended" && e["subject"] == "bob"));
    app.state.db.call(|c| Ok(c.execute("UPDATE audit SET detail = 'nothing' WHERE action = 'repo_deleted'", [])?)).await.unwrap();
    let (_, all) = app.req(&alice, "GET", "/api/admin/audit", None).await;
    assert_eq!(all["integrity"]["ok"], false, "editing a row breaks the chain");
}

async fn bearer(app: &App, method: &str, path: &str, token: &str, body: Option<Value>) -> StatusCode {
    let b = Request::builder().method(method).uri(path).header("host", "test.local").header("authorization", format!("Bearer {token}"));
    let req = match body {
        Some(v) => b.header("content-type", "application/json").body(Body::from(v.to_string())).unwrap(),
        None => b.body(Body::empty()).unwrap(),
    };
    status_with(app, req).await
}

#[tokio::test]
async fn access_tokens_cannot_do_sudo_actions() {
    let app = App::new();
    let alice = app.signup("alice").await;
    app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "r" }))).await;
    let (_, t) = app.req(&alice, "POST", "/api/me/tokens", Some(json!({ "name": "ci" }))).await;
    let t = t["token"].as_str().unwrap().to_string();
    for (m, p, b) in [
        ("POST", "/api/me/2fa/setup", None),
        ("POST", "/api/me/2fa/disable", None),
        ("POST", "/api/me/tokens", Some(json!({ "name": "more" }))),
        ("DELETE", "/api/repos/alice/r", Some(json!({ "confirm": "alice/r" }))),
        ("PATCH", "/api/repos/alice/r", Some(json!({ "private": true }))),
        ("POST", "/api/me/password", Some(json!({ "current": "correct horse", "new": "another horse" }))),
        ("PATCH", "/api/admin/users/alice", Some(json!({ "reset_2fa": true }))),
    ] {
        assert_eq!(bearer(&app, m, p, &t, b).await, StatusCode::FORBIDDEN, "{m} {p} with a token");
    }
    assert_eq!(bearer(&app, "PATCH", "/api/repos/alice/r", &t, Some(json!({ "description": "ok" }))).await, StatusCode::OK);
}

#[tokio::test]
async fn password_change_needs_a_fresh_confirmation_and_codes_have_a_shared_budget() {
    use gitter::security::{step_now, totp_code};
    let app = App::new();
    let alice = app.signup("alice").await;
    app.state.db.call(|c| Ok(c.execute("UPDATE sessions SET verified_at = 0", [])?)).await.unwrap();
    let (s, e) = app.req(&alice, "POST", "/api/me/password", Some(json!({ "current": "correct horse", "new": "another horse" }))).await;
    assert_eq!((s, e["code"].as_str()), (StatusCode::FORBIDDEN, Some("reauth")));
    app.req(&alice, "POST", "/api/auth/reauth", Some(json!({ "password": "correct horse" }))).await;
    let (_, setup) = app.req(&alice, "POST", "/api/me/2fa/setup", None).await;
    let secret = setup["secret"].as_str().unwrap().to_string();
    let now = step_now();
    app.req(&alice, "POST", "/api/me/2fa/enable", Some(json!({ "code": totp_code(&secret, now).unwrap() }))).await;
    // Five tries per sign-in, counted before checking: the sixth fails even with the right code.
    let (_, step1, _) = login(&app, json!({ "login": "alice", "password": "correct horse" })).await;
    for _ in 0..5 {
        let body = json!({ "challenge": step1["challenge"], "code": "000000" });
        app.call(&Client::default(), "POST", "/api/auth/login/2fa", Some(body), Some(ORIGIN)).await;
    }
    let body = json!({ "challenge": step1["challenge"], "code": totp_code(&secret, now + 1).unwrap() });
    let (s, _, _) = app.call(&Client::default(), "POST", "/api/auth/login/2fa", Some(body), Some(ORIGIN)).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_new_account_never_inherits_a_deleted_accounts_log() {
    let app = App::new();
    let admin = app.signup("alice").await;
    let bob = app.signup("bob").await;
    let (_, t) = app.req(&bob, "POST", "/api/me/tokens", Some(json!({ "name": "bobs-secret-token" }))).await;
    assert!(t["token"].is_string());
    let (s, _) = app.req(&admin, "DELETE", "/api/admin/users/bob", Some(json!({ "confirm": "bob" }))).await;
    assert_eq!(s, StatusCode::OK);
    let carol = app.signup("carol").await; // SQLite hands carol bob's old id
    let (_, mine) = app.req(&carol, "GET", "/api/me/audit", None).await;
    let text = mine.to_string();
    assert!(!text.contains("bobs-secret-token"), "{text}");
    let (_, all) = app.req(&admin, "GET", "/api/admin/audit", None).await;
    assert_eq!(all["integrity"]["ok"], true, "detaching ids keeps the seal");
    assert!(all["events"].as_array().unwrap().iter().any(|e| e["action"] == "token_created" && e["subject"] == "bob"));
}

fn git_refs(repo: &str, service: &str, token: &str) -> Request<Body> {
    use base64::Engine;
    let auth = base64::engine::general_purpose::STANDARD.encode(format!("x-access-token:{token}"));
    Request::get(format!("/alice/{repo}.git/info/refs?service={service}"))
        .header("host", "test.local")
        .header("authorization", format!("Basic {auth}"))
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn scoped_tokens_limit_what_they_can_do() {
    let app = App::new();
    let alice = app.signup("alice").await;
    for n in ["a", "b"] {
        app.req(&alice, "POST", "/api/repos", Some(json!({ "name": n, "private": true, "readme": true }))).await;
    }
    let mk = |body: Value| {
        let app = &app;
        let alice = alice.clone();
        async move { app.req(&alice, "POST", "/api/me/tokens", Some(body)).await.1["token"].as_str().unwrap().to_string() }
    };
    let read = mk(json!({ "name": "read", "scope": "read" })).await;
    let only_a = mk(json!({ "name": "a-only", "scope": "write", "repo": "alice/a" })).await;
    let expired = mk(json!({ "name": "old", "scope": "all", "expires_days": 1 })).await;
    app.state.db.call(|c| Ok(c.execute("UPDATE tokens SET expires_at = 1 WHERE name = 'old'", [])?)).await.unwrap();

    // Read-only: clone yes, push no, API writes no, account routes no.
    assert_eq!(status_with(&app, git_refs("a", "git-upload-pack", &read)).await, StatusCode::OK);
    assert_eq!(status_with(&app, git_refs("a", "git-receive-pack", &read)).await, StatusCode::FORBIDDEN);
    assert_eq!(bearer(&app, "POST", "/api/repos/alice/a/issues", &read, Some(json!({ "title": "x" }))).await, StatusCode::FORBIDDEN);
    assert_eq!(bearer(&app, "GET", "/api/repos/alice/b", &read, None).await, StatusCode::OK);
    assert_eq!(bearer(&app, "GET", "/api/me/tokens", &read, None).await, StatusCode::FORBIDDEN);

    // One repository: works there (including writes), the other private repo doesn't exist for it.
    assert_eq!(bearer(&app, "POST", "/api/repos/alice/a/issues", &only_a, Some(json!({ "title": "ok" }))).await, StatusCode::OK);
    assert_eq!(status_with(&app, git_refs("a", "git-receive-pack", &only_a)).await, StatusCode::OK);
    assert_eq!(bearer(&app, "GET", "/api/repos/alice/b", &only_a, None).await, StatusCode::NOT_FOUND);
    assert_eq!(status_with(&app, git_refs("b", "git-upload-pack", &only_a)).await, StatusCode::NOT_FOUND);
    assert_eq!(bearer(&app, "POST", "/api/repos", &only_a, Some(json!({ "name": "c" }))).await, StatusCode::FORBIDDEN);
    assert_eq!(bearer(&app, "PATCH", "/api/repos/alice/a", &only_a, Some(json!({ "description": "x" }))).await, StatusCode::FORBIDDEN);

    // Expired: nothing.
    assert_eq!(bearer(&app, "GET", "/api/me", &expired, None).await, StatusCode::UNAUTHORIZED);
    let (_, list) = app.req(&alice, "GET", "/api/me/tokens", None).await;
    let old = list["tokens"].as_array().unwrap().iter().find(|t| t["name"] == "old").unwrap().clone();
    assert_eq!(old["expired"], true);
    let a = list["tokens"].as_array().unwrap().iter().find(|t| t["name"] == "a-only").unwrap().clone();
    assert_eq!((a["scope"].as_str(), a["repo"].as_str()), (Some("write"), Some("alice/a")));
}

#[tokio::test]
async fn deploy_tokens_belong_to_one_repository() {
    let app = App::new();
    let alice = app.signup("alice").await;
    let bob = app.signup("bob").await;
    for n in ["a", "b"] {
        app.req(&alice, "POST", "/api/repos", Some(json!({ "name": n, "private": true, "readme": true }))).await;
    }
    let (s, _) = app.req(&bob, "POST", "/api/repos/alice/a/deploy-tokens", Some(json!({ "name": "ci" }))).await;
    assert_ne!(s, StatusCode::OK, "only people who can write to the repo make deploy tokens");
    let (s, t) = app.req(&alice, "POST", "/api/repos/alice/a/deploy-tokens", Some(json!({ "name": "deploy box", "expires_days": 90 }))).await;
    assert_eq!(s, StatusCode::OK, "{t}");
    let t = t["token"].as_str().unwrap().to_string();
    let (_, list) = app.req(&alice, "GET", "/api/repos/alice/a/deploy-tokens", None).await;
    let row = &list["tokens"][0];
    assert_eq!((row["name"].as_str(), row["scope"].as_str()), (Some("deploy box"), Some("read")));
    assert_eq!(status_with(&app, git_refs("a", "git-upload-pack", &t)).await, StatusCode::OK);
    assert_eq!(status_with(&app, git_refs("a", "git-receive-pack", &t)).await, StatusCode::FORBIDDEN, "read-only unless asked");
    assert_eq!(status_with(&app, git_refs("b", "git-upload-pack", &t)).await, StatusCode::NOT_FOUND);
    assert_eq!(bearer(&app, "GET", "/api/repos/alice/a/deploy-tokens", &t, None).await, StatusCode::FORBIDDEN);
    let id = row["id"].as_i64().unwrap();
    let (s, _) = app.req(&alice, "DELETE", &format!("/api/repos/alice/a/deploy-tokens/{id}"), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(status_with(&app, git_refs("a", "git-upload-pack", &t)).await, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn deploy_tokens_cannot_act_or_peek_outside_their_repo() {
    let app = App::new();
    let alice = app.signup("alice").await;
    let bob = app.signup("bob").await;
    app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "a", "readme": true }))).await;
    app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "hidden-plans", "private": true }))).await;
    app.req(&bob, "POST", "/api/repos", Some(json!({ "name": "pub", "readme": true }))).await;
    let (_, t) = app.req(&alice, "POST", "/api/repos/alice/a/deploy-tokens", Some(json!({ "name": "ci", "write": true }))).await;
    let t = t["token"].as_str().unwrap().to_string();
    assert_eq!(bearer(&app, "POST", "/api/repos/alice/a/issues", &t, Some(json!({ "title": "ok here" }))).await, StatusCode::OK);
    for (m, p, b) in [
        ("POST", "/api/repos/bob/pub/issues", Some(json!({ "title": "spam as alice" }))),
        ("POST", "/api/repos/bob/pub/star", None),
    ] {
        assert_eq!(bearer(&app, m, p, &t, b).await, StatusCode::FORBIDDEN, "{m} {p}");
    }
    let resp = app
        .router
        .clone()
        .oneshot(Request::get("/api/feed").header("host", "test.local").header("authorization", format!("Bearer {t}")).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let body = String::from_utf8(resp.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    assert!(!body.contains("hidden-plans"), "feed leaks private repos to a deploy token: {body}");
    let resp = app
        .router
        .clone()
        .oneshot(Request::get("/api/me").header("host", "test.local").header("authorization", format!("Bearer {t}")).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let body = String::from_utf8(resp.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    assert!(!body.contains("alice@example.com"), "{body}");
}

fn names(v: &Value, key: &str) -> Vec<String> {
    v.as_array().unwrap().iter().map(|x| x[key].as_str().unwrap().to_string()).collect()
}

async fn audit_count(app: &App, action: &'static str) -> i64 {
    app.state.db.call(move |c| Ok(c.query_row("SELECT count(*) FROM audit WHERE action = ?1", [action], |r| r.get(0))?)).await.unwrap()
}

#[tokio::test]
async fn wiki_pages_have_history_conflicts_and_follow_repo_privacy() {
    let app = App::new();
    let alice = app.signup("alice").await;
    let bob = app.signup("bob").await;
    let anon = Client::default();
    app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "pub", "readme": true }))).await;
    app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "priv", "private": true }))).await;
    let w = "/api/repos/alice/pub/wiki";

    let (s, v) = app.req(&anon, "GET", w, None).await;
    assert_eq!((s, v["pages"].as_array().map(Vec::len)), (StatusCode::OK, Some(0)), "no wiki yet: no pages");
    let (s, v) = app.req(&anon, "GET", &format!("{w}/Home"), None).await;
    assert_eq!((s, v["page"].is_null()), (StatusCode::OK, true), "a missing page is null, not an error");

    let (s, _) = app.req(&bob, "POST", w, Some(json!({ "name": "Home", "content": "hi" }))).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "only people who can write the repo edit its wiki");
    let (s, v) = app.req(&alice, "POST", w, Some(json!({ "name": "Home", "content": "# Welcome" }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let (s, _) = app.req(&alice, "POST", w, Some(json!({ "name": "Home", "content": "again" }))).await;
    assert_eq!(s, StatusCode::CONFLICT, "a page name is taken once");
    for bad in ["../evil", "a/b", ".hidden", ""] {
        let (s, _) = app.req(&alice, "POST", w, Some(json!({ "name": bad, "content": "x" }))).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{bad:?}");
    }
    app.req(&alice, "POST", w, Some(json!({ "name": "Getting started", "content": "Step 1" }))).await;

    let (_, v) = app.req(&anon, "GET", &format!("{w}/Home"), None).await;
    assert_eq!(v["page"]["content"], "# Welcome");
    assert_eq!(names(&v["pages"], "name"), ["Getting-started", "Home"]);
    let home_oid = v["page"]["oid"].as_str().unwrap().to_string();
    let (_, v) = app.req(&anon, "GET", &format!("{w}/Getting-started"), None).await;
    let gs_oid = v["page"]["oid"].as_str().unwrap().to_string();
    assert_eq!(v["page"]["title"], "Getting started");

    // Edits are checked against the page the editor started from.
    let edit = |content: &str, oid: &str| json!({ "name": "Home", "content": content, "expected_oid": oid });
    let (s, v) = app.req(&alice, "PUT", &format!("{w}/Home"), Some(edit("# Welcome!", &home_oid))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let (s, _) = app.req(&alice, "PUT", &format!("{w}/Home"), Some(edit("stale", &home_oid))).await;
    assert_eq!(s, StatusCode::CONFLICT, "a stale edit is refused");
    // Another page changed meanwhile, but this one didn't: a rename goes through.
    let (s, v) = app.req(&alice, "PUT", &format!("{w}/Getting-started"), Some(json!({ "name": "Setup", "content": "Step 1", "expected_oid": gs_oid }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let (_, v) = app.req(&anon, "GET", w, None).await;
    assert_eq!(names(&v["pages"], "name"), ["Home", "Setup"]);
    let (s, _) = app.req(&bob, "PUT", &format!("{w}/Home"), Some(edit("x", ""))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // History, and old revisions stay readable.
    let (s, h) = app.req(&anon, "GET", &format!("{w}/Home/history"), None).await;
    assert_eq!(s, StatusCode::OK, "{h}");
    let commits = h["commits"].as_array().unwrap();
    assert_eq!(commits.len(), 2, "{h}");
    let first = commits[1]["sha"].as_str().unwrap();
    let (_, v) = app.req(&anon, "GET", &format!("{w}/Home?rev={first}"), None).await;
    assert_eq!((v["page"]["content"].as_str(), v["page"]["latest"].as_bool()), (Some("# Welcome"), Some(false)));

    let (s, _) = app.req(&bob, "DELETE", &format!("{w}/Setup"), None).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = app.req(&alice, "DELETE", &format!("{w}/Setup"), None).await;
    assert_eq!(s, StatusCode::OK);
    let (_, v) = app.req(&anon, "GET", w, None).await;
    assert_eq!(names(&v["pages"], "name"), ["Home"]);
    assert_eq!(audit_count(&app, "wiki_page_deleted").await, 1);

    // A private repo's wiki is as private as the repo.
    let (s, _) = app.req(&alice, "POST", "/api/repos/alice/priv/wiki", Some(json!({ "name": "Home", "content": "plans" }))).await;
    assert_eq!(s, StatusCode::OK);
    for c in [&bob, &anon] {
        assert_eq!(app.req(c, "GET", "/api/repos/alice/priv/wiki", None).await.0, StatusCode::NOT_FOUND);
        assert_eq!(app.req(c, "GET", "/api/repos/alice/priv/wiki/Home", None).await.0, StatusCode::NOT_FOUND);
        assert_eq!(app.req(c, "GET", "/api/repos/alice/priv/wiki/Home/history", None).await.0, StatusCode::NOT_FOUND);
    }

    // Deleting the repository deletes its wiki.
    let id: i64 = app.state.db.call(|c| Ok(c.query_row("SELECT id FROM repos WHERE name = 'pub'", [], |r| r.get(0))?)).await.unwrap();
    let dir = app.state.cfg.data_dir.join(format!("wikis/{id}.git"));
    assert!(dir.is_dir());
    let (s, _) = app.req(&alice, "DELETE", "/api/repos/alice/pub", Some(json!({ "confirm": "alice/pub" }))).await;
    assert_eq!(s, StatusCode::OK);
    assert!(!dir.exists());
}

#[tokio::test]
async fn discussion_categories_threads_answers_and_votes() {
    let app = App::new();
    let alice = app.signup("alice").await;
    let bob = app.signup("bob").await;
    let carol = app.signup("carol").await;
    let anon = Client::default();
    app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "pub", "readme": true }))).await;
    app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "priv", "private": true }))).await;
    let d = "/api/repos/alice/pub/discussions";
    let start = |cat: &str| json!({ "title": "How do I build it", "body": "Which command?", "category": cat });

    let (s, v) = app.req(&bob, "POST", d, Some(start("qa"))).await;
    assert_eq!((s, v["number"].as_i64()), (StatusCode::OK, Some(1)), "{v}");
    assert_eq!(app.req(&bob, "POST", d, Some(start("announcements"))).await.0, StatusCode::FORBIDDEN, "only maintainers announce");
    assert_eq!(app.req(&bob, "POST", d, Some(start("nope"))).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(app.req(&anon, "POST", d, Some(start("qa"))).await.0, StatusCode::UNAUTHORIZED);
    let (s, v) = app.req(&alice, "POST", d, Some(start("announcements"))).await;
    assert_eq!((s, v["number"].as_i64()), (StatusCode::OK, Some(2)));

    let (s, c1) = app.req(&alice, "POST", &format!("{d}/1/comments"), Some(json!({ "body": "Run `cargo build`" }))).await;
    assert_eq!(s, StatusCode::OK, "{c1}");
    let c1 = c1["id"].as_i64().unwrap();
    let (_, c2) = app.req(&bob, "POST", &format!("{d}/1/comments"), Some(json!({ "body": "Thanks!" }))).await;
    let c2 = c2["id"].as_i64().unwrap();
    assert_eq!(app.req(&carol, "POST", &format!("{d}/1/comments"), Some(json!({ "body": "  " }))).await.0, StatusCode::BAD_REQUEST);

    // The asker (or a maintainer) marks the answer, in Q&A only.
    assert_eq!(app.req(&carol, "POST", &format!("{d}/1/answer"), Some(json!({ "comment_id": c1 }))).await.0, StatusCode::FORBIDDEN);
    assert_eq!(app.req(&bob, "POST", &format!("{d}/1/answer"), Some(json!({ "comment_id": c1 }))).await.0, StatusCode::OK);
    assert_eq!(app.req(&alice, "POST", &format!("{d}/2/answer"), Some(json!({ "comment_id": c1 }))).await.0, StatusCode::BAD_REQUEST);

    // One vote per person, on the thread or a reply.
    for (c, up) in [(&bob, true), (&bob, true), (&carol, true), (&carol, false), (&carol, true)] {
        assert_eq!(app.req(c, "POST", &format!("{d}/1/vote"), Some(json!({ "up": up }))).await.0, StatusCode::OK);
    }
    app.req(&carol, "POST", &format!("{d}/1/vote"), Some(json!({ "up": true, "comment_id": c1 }))).await;
    assert_eq!(app.req(&anon, "POST", &format!("{d}/1/vote"), Some(json!({ "up": true }))).await.0, StatusCode::UNAUTHORIZED);
    let (s, t) = app.req(&carol, "GET", &format!("{d}/1"), None).await;
    assert_eq!(s, StatusCode::OK, "{t}");
    assert_eq!((t["discussion"]["votes"].as_i64(), t["discussion"]["voted"].as_bool()), (Some(2), Some(true)), "{t}");
    assert_eq!(t["discussion"]["answer_id"].as_i64(), Some(c1));
    let first = &t["comments"][0];
    assert_eq!((first["is_answer"].as_bool(), first["votes"].as_i64(), first["voted"].as_bool()), (Some(true), Some(1), Some(true)));
    assert_eq!(t["comments"][1]["can_edit"], false, "carol can't edit bob's reply");

    // Lists filter by category and sort by votes.
    let (_, l) = app.req(&anon, "GET", &format!("{d}?category=qa"), None).await;
    assert_eq!(l["items"].as_array().unwrap().len(), 1, "{l}");
    assert_eq!(l["items"][0]["answered"], true);
    let (_, l) = app.req(&anon, "GET", &format!("{d}?sort=top"), None).await;
    assert_eq!(l["items"][0]["number"], 1);
    assert!(l["categories"].as_array().unwrap().iter().any(|c| c["id"] == "announcements" && c["count"] == 1), "{l}");

    // Authors and maintainers edit; nobody else.
    assert_eq!(app.req(&carol, "PATCH", &format!("{d}/1"), Some(json!({ "title": "pwned" }))).await.0, StatusCode::FORBIDDEN);
    assert_eq!(app.req(&bob, "PATCH", &format!("{d}/1"), Some(json!({ "title": "How do I build?" }))).await.0, StatusCode::OK);
    assert_eq!(app.req(&carol, "PATCH", &format!("{d}/1/comments/{c2}"), Some(json!({ "body": "pwned" }))).await.0, StatusCode::FORBIDDEN);
    assert_eq!(app.req(&bob, "PATCH", &format!("{d}/1/comments/{c2}"), Some(json!({ "body": "Thanks, that worked" }))).await.0, StatusCode::OK);
    assert_eq!(app.req(&bob, "PATCH", &format!("{d}/2/comments/{c2}"), Some(json!({ "body": "x" }))).await.0, StatusCode::NOT_FOUND, "a reply belongs to its own thread");

    // Deleting the answer unmarks it; deleting a thread is for maintainers and is logged.
    assert_eq!(app.req(&alice, "DELETE", &format!("{d}/1/comments/{c1}"), None).await.0, StatusCode::OK);
    let (_, t) = app.req(&anon, "GET", &format!("{d}/1"), None).await;
    assert!(t["discussion"]["answer_id"].is_null(), "{t}");
    assert_eq!(app.req(&bob, "DELETE", &format!("{d}/1"), None).await.0, StatusCode::FORBIDDEN);
    assert_eq!(app.req(&alice, "DELETE", &format!("{d}/1"), None).await.0, StatusCode::OK);
    assert_eq!(app.req(&anon, "GET", &format!("{d}/1"), None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(audit_count(&app, "discussion_deleted").await, 1);

    // A private repo's discussions follow its access rules.
    assert_eq!(app.req(&alice, "POST", "/api/repos/alice/priv/discussions", Some(start("general"))).await.0, StatusCode::OK);
    for c in [&bob, &anon] {
        assert_eq!(app.req(c, "GET", "/api/repos/alice/priv/discussions", None).await.0, StatusCode::NOT_FOUND);
        assert_eq!(app.req(c, "GET", "/api/repos/alice/priv/discussions/1", None).await.0, StatusCode::NOT_FOUND);
    }
    assert_eq!(app.req(&bob, "POST", "/api/repos/alice/priv/discussions/1/comments", Some(json!({ "body": "hi" }))).await.0, StatusCode::NOT_FOUND);
}

async fn raw_get(app: &App, path: &str) -> (StatusCode, String, String) {
    let resp = app.router.clone().oneshot(Request::get(path).header("host", "test.local").body(Body::empty()).unwrap()).await.unwrap();
    let csp = resp.headers().get("content-security-policy").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    let s = resp.status();
    let body = String::from_utf8_lossy(&resp.into_body().collect().await.unwrap().to_bytes()).into_owned();
    (s, body, csp)
}

#[tokio::test]
async fn snippets_public_secret_revisions_and_raw() {
    let app = App::new();
    let alice = app.signup("alice").await;
    let bob = app.signup("bob").await;
    let anon = Client::default();
    let files = json!([{ "name": "hello.rs", "content": "fn main() {}\n" }, { "name": "notes.md", "content": "# hi" }]);
    let (s, v) = app.req(&alice, "POST", "/api/snippets", Some(json!({ "title": "Hello", "files": files }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let public = v["id"].as_str().unwrap().to_string();
    let (_, v) = app.req(&alice, "POST", "/api/snippets", Some(json!({ "title": "Keys", "secret": true, "files": [{ "name": "", "content": "shh" }] }))).await;
    let secret = v["id"].as_str().unwrap().to_string();
    assert_eq!(secret.len(), 32, "secret links are unguessable");
    assert_eq!(app.req(&anon, "POST", "/api/snippets", Some(json!({ "files": files }))).await.0, StatusCode::UNAUTHORIZED);
    for bad in [json!([]), json!([{ "name": "../x", "content": "x" }]), json!([{ "name": "a", "content": "x" }, { "name": "a", "content": "y" }]), json!([{ "name": "a", "content": "" }])] {
        assert_eq!(app.req(&alice, "POST", "/api/snippets", Some(json!({ "files": bad }))).await.0, StatusCode::BAD_REQUEST, "{bad}");
    }

    // Secret snippets are unlisted: only their owner sees them in a list.
    let ids = |v: &Value| names(&v["items"], "id");
    let (_, l) = app.req(&anon, "GET", "/api/snippets", None).await;
    assert_eq!(ids(&l), [public.as_str()]);
    assert_eq!(l["items"][0]["files"], json!(["hello.rs", "notes.md"]));
    let (_, l) = app.req(&bob, "GET", "/api/snippets?user=alice", None).await;
    assert_eq!(ids(&l), [public.as_str()]);
    let (_, l) = app.req(&alice, "GET", "/api/snippets?user=alice", None).await;
    assert_eq!(ids(&l).len(), 2);
    // ...but open for anyone with the link.
    let (s, v) = app.req(&anon, "GET", &format!("/api/snippets/{secret}"), None).await;
    assert_eq!((s, v["files"][0]["name"].as_str()), (StatusCode::OK, Some("snippetfile1.txt")), "{v}");
    assert_eq!(v["can_edit"], false);
    assert_eq!(app.req(&anon, "GET", "/api/snippets/0123", None).await.0, StatusCode::NOT_FOUND);

    // Edits make revisions; stale edits are refused; only the owner edits.
    let (_, v) = app.req(&alice, "GET", &format!("/api/snippets/{public}"), None).await;
    let sha = v["sha"].as_str().unwrap().to_string();
    let one = json!({ "title": "Hello", "files": [{ "name": "hello.rs", "content": "fn main() { println!(\"hi\"); }\n" }], "expected_sha": sha });
    assert_eq!(app.req(&bob, "PUT", &format!("/api/snippets/{public}"), Some(one.clone())).await.0, StatusCode::FORBIDDEN);
    let (s, v) = app.req(&alice, "PUT", &format!("/api/snippets/{public}"), Some(one.clone())).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(app.req(&alice, "PUT", &format!("/api/snippets/{public}"), Some(one)).await.0, StatusCode::CONFLICT);
    let (_, v) = app.req(&anon, "GET", &format!("/api/snippets/{public}"), None).await;
    assert_eq!((v["files"].as_array().unwrap().len(), v["revisions"].as_i64()), (1, Some(2)), "{v}");
    let head = v["sha"].as_str().unwrap().to_string();
    // Changing only the title adds no revision.
    let retitle = json!({ "title": "Hello world", "files": [{ "name": "hello.rs", "content": "fn main() { println!(\"hi\"); }\n" }] });
    assert_eq!(app.req(&alice, "PUT", &format!("/api/snippets/{public}"), Some(retitle)).await.0, StatusCode::OK);
    let (_, r) = app.req(&anon, "GET", &format!("/api/snippets/{public}/revisions"), None).await;
    assert_eq!(names(&r["commits"], "sha"), [head.clone(), sha.clone()], "{r}");
    let (_, v) = app.req(&anon, "GET", &format!("/api/snippets/{public}?rev={sha}"), None).await;
    assert_eq!((v["files"].as_array().unwrap().len(), v["title"].as_str()), (2, Some("Hello world")));

    // Raw files are served as inert text.
    let (s, body, csp) = raw_get(&app, &format!("/raw/snippets/{public}/{sha}/notes.md")).await;
    assert_eq!((s, body.as_str()), (StatusCode::OK, "# hi"));
    assert!(csp.contains("sandbox"), "{csp}");
    let (_, body, _) = raw_get(&app, &format!("/raw/snippets/{public}/latest/hello.rs")).await;
    assert!(body.contains("println"), "{body}");
    assert_eq!(raw_get(&app, &format!("/raw/snippets/{public}/latest/notes.md")).await.0, StatusCode::NOT_FOUND);

    // Only the owner deletes; the folder goes and the deletion is logged.
    let dir = app.state.cfg.data_dir.join(format!("snippets/{public}.git"));
    assert!(dir.is_dir());
    assert_eq!(app.req(&bob, "DELETE", &format!("/api/snippets/{public}"), None).await.0, StatusCode::FORBIDDEN);
    assert_eq!(app.req(&alice, "DELETE", &format!("/api/snippets/{public}"), None).await.0, StatusCode::OK);
    assert_eq!(app.req(&anon, "GET", &format!("/api/snippets/{public}"), None).await.0, StatusCode::NOT_FOUND);
    assert!(!dir.exists());
    assert_eq!(audit_count(&app, "snippet_deleted").await, 1);
}
