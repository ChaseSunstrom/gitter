//! In-process API tests: the real router over a temporary data directory.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use common::{App, Client, ORIGIN};


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

#[tokio::test]
async fn rate_limits_answer_429_with_retry_after() {
    let app = App::new();
    let admin = app.signup("root1").await;
    let bob = app.signup("bob").await;
    let mut s = app.state.settings.get();
    s.write_rate_per_min = 3;
    app.state.settings.save(&app.state.db, s).await.unwrap();
    app.req(&bob, "POST", "/api/repos", Some(json!({ "name": "r" }))).await;
    let mut last = StatusCode::OK;
    for i in 0..5 {
        last = app.req(&bob, "POST", "/api/repos/bob/r/issues", Some(json!({ "title": format!("t{i}") }))).await.0;
    }
    assert_eq!(last, StatusCode::TOO_MANY_REQUESTS);
    let req = Request::post("/api/repos/bob/r/issues")
        .header("host", "test.local")
        .header("origin", ORIGIN)
        .header("cookie", format!("gitter_session={}", bob.0.clone().unwrap()))
        .header("content-type", "application/json")
        .body(Body::from(json!({ "title": "x" }).to_string()))
        .unwrap();
    let resp = app.router.clone().oneshot(req).await.unwrap();
    assert!(resp.headers().get("retry-after").is_some(), "429 carries Retry-After");
    // Admins aren't held back.
    app.req(&admin, "POST", "/api/repos", Some(json!({ "name": "a" }))).await;
    for i in 0..5 {
        let (s, _) = app.req(&admin, "POST", "/api/repos/root1/a/issues", Some(json!({ "title": format!("t{i}") }))).await;
        assert_eq!(s, StatusCode::OK);
    }
}

#[tokio::test]
async fn storage_quota_blocks_pushes_and_forks_past_the_limit() {
    let app = App::new();
    let _admin = app.signup("root1").await;
    let bob = app.signup("bob").await;
    app.req(&bob, "POST", "/api/repos", Some(json!({ "name": "r", "private": true, "readme": true }))).await;
    let (_, t) = app.req(&bob, "POST", "/api/me/tokens", Some(json!({ "name": "t" }))).await;
    let token = t["token"].as_str().unwrap().to_string();
    // Pretend bob already uses 3 MB with a 2 MB quota.
    let mut s = app.state.settings.get();
    s.max_storage_mb = 2;
    app.state.settings.save(&app.state.db, s).await.unwrap();
    app.state.db.call(|c| Ok(c.execute("UPDATE repos SET size_bytes = 3145728", [])?)).await.unwrap();
    let push = {
        use base64::Engine;
        let auth = base64::engine::general_purpose::STANDARD.encode(format!("bob:{token}"));
        Request::post("/bob/r.git/git-receive-pack")
            .header("host", "test.local")
            .header("authorization", format!("Basic {auth}"))
            .header("content-type", "application/x-git-receive-pack-request")
            .body(Body::from("0000"))
            .unwrap()
    };
    assert_eq!(status_with(&app, push).await, StatusCode::FORBIDDEN, "out of storage");
    let (_, me) = app.req(&bob, "GET", "/api/me", None).await;
    assert_eq!(me["storage"]["used"], 3145728);
    assert_eq!(me["storage"]["quota"], 2097152);
    let (s, _) = app.req(&bob, "POST", "/api/repos/bob/r/fork", Some(json!({ "name": "r2" }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "a fork that doesn't fit is refused");
    // Size is recorded after a change.
    app.state.db.call(|c| Ok(c.execute("UPDATE repos SET size_bytes = 0", [])?)).await.unwrap();
    app.req(&bob, "POST", "/api/repos/bob/r/contents", Some(json!({ "branch": "main", "message": "m", "changes": [{ "path": "x", "content": "y" }] }))).await;
    let size: i64 = app.state.db.call(|c| Ok(c.query_row("SELECT size_bytes FROM repos", [], |r| r.get(0))?)).await.unwrap();
    assert!(size > 0, "size tracked after a commit");
}

#[tokio::test]
async fn invite_only_signup_needs_a_live_invite() {
    let app = App::new();
    let admin = app.signup("root1").await;
    let mut s = app.state.settings.get();
    s.signup = "invite".into();
    app.state.settings.save(&app.state.db, s).await.unwrap();
    let body = |name: &str, invite: &str| json!({ "username": name, "email": format!("{name}@example.com"), "password": "correct horse", "invite": invite });
    let (s, _, _) = app.call(&Client::default(), "POST", "/api/auth/signup", Some(body("eve", "")), Some(ORIGIN)).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, inv) = app.req(&admin, "POST", "/api/admin/invites", Some(json!({ "note": "team", "max_uses": 1, "expires_days": 7 }))).await;
    assert_eq!(s, StatusCode::OK, "{inv}");
    let code = inv["code"].as_str().unwrap().to_string();
    assert!(inv["url"].as_str().unwrap().contains(&format!("/signup?invite={code}")));
    let (s, _, _) = app.call(&Client::default(), "POST", "/api/auth/signup", Some(body("ann", &code)), Some(ORIGIN)).await;
    assert_eq!(s, StatusCode::OK);
    let (s, _, _) = app.call(&Client::default(), "POST", "/api/auth/signup", Some(body("ben", &code)), Some(ORIGIN)).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "a single-use invite is used up");
    let (_, list) = app.req(&admin, "GET", "/api/admin/invites", None).await;
    assert_eq!(list["invites"][0]["uses"], 1);
    assert_eq!(list["invites"][0]["live"], false);
}

#[tokio::test]
async fn moderation_reports_hide_and_suspend() {
    let app = App::new();
    let admin = app.signup("root1").await;
    let bob = app.signup("bob").await;
    let carol = app.signup("carol").await;
    app.req(&bob, "POST", "/api/repos", Some(json!({ "name": "spam", "readme": true }))).await;
    app.req(&bob, "POST", "/api/repos/bob/spam/issues", Some(json!({ "title": "buy pills" }))).await;
    let (_, c) = app.req(&carol, "POST", "/api/repos/bob/spam/issues/1/comments", Some(json!({ "body": "rude words" }))).await;
    let cid = c["id"].as_i64().expect("comment id");
    let rep = |kind: &str, extra: Value| {
        let mut b = json!({ "kind": kind, "owner": "bob", "repo": "spam", "reason": "spam", "details": "please look" });
        b.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        b
    };
    assert_eq!(app.req(&carol, "POST", "/api/reports", Some(rep("repo", json!({})))).await.0, StatusCode::OK);
    assert_eq!(app.req(&carol, "POST", "/api/reports", Some(rep("repo", json!({})))).await.0, StatusCode::CONFLICT, "no duplicate open reports");
    assert_eq!(app.req(&bob, "POST", "/api/reports", Some(rep("comment", json!({ "comment": cid })))).await.0, StatusCode::OK);
    assert_eq!(app.req(&carol, "POST", "/api/reports", Some(rep("issue", json!({ "number": 1 })))).await.0, StatusCode::OK);
    let (s, _) = app.req(&bob, "GET", "/api/admin/reports", None).await;
    assert_ne!(s, StatusCode::OK, "only admins see the queue");
    let (_, q) = app.req(&admin, "GET", "/api/admin/reports", None).await;
    assert_eq!(q["open"], 3);
    let id_of = |kind: &str| q["reports"].as_array().unwrap().iter().find(|r| r["kind"] == kind).unwrap()["id"].as_i64().unwrap();

    // Hide the comment: carol (author) and admins still read it, others see it as hidden.
    app.req(&admin, "POST", &format!("/api/admin/reports/{}/resolve", id_of("comment")), Some(json!({ "action": "hide" }))).await;
    let (_, issue) = app.req(&bob, "GET", "/api/repos/bob/spam/issues/1", None).await;
    let com = issue["comments"].as_array().unwrap().iter().find(|c| c["id"] == cid).unwrap().clone();
    assert_eq!((com["hidden"].as_bool(), com["body"].as_str()), (Some(true), Some("")));
    let (_, issue) = app.req(&carol, "GET", "/api/repos/bob/spam/issues/1", None).await;
    assert!(issue.to_string().contains("rude words"), "the author still sees it");

    // Hide the issue: gone for others, still there for its author.
    app.req(&admin, "POST", &format!("/api/admin/reports/{}/resolve", id_of("issue")), Some(json!({ "action": "hide" }))).await;
    assert_eq!(app.req(&carol, "GET", "/api/repos/bob/spam/issues/1", None).await.0, StatusCode::NOT_FOUND);
    let (_, list) = app.req(&carol, "GET", "/api/repos/bob/spam/issues", None).await;
    assert_eq!(list["items"].as_array().unwrap().len(), 0);

    // Hide the repo: gone for everyone but its owner and admins.
    app.req(&admin, "POST", &format!("/api/admin/reports/{}/resolve", id_of("repo")), Some(json!({ "action": "hide", "note": "spam" }))).await;
    assert_eq!(app.req(&carol, "GET", "/api/repos/bob/spam", None).await.0, StatusCode::NOT_FOUND);
    let (_, own) = app.req(&bob, "GET", "/api/repos/bob/spam", None).await;
    assert_eq!(own["hidden"], true);
    let (_, ex, _) = app.call(&Client::default(), "GET", "/api/explore", None, None).await;
    assert!(!ex.to_string().contains("\"spam\""));

    // Suspending signs the account out.
    assert_eq!(app.req(&carol, "POST", "/api/reports", Some(json!({ "kind": "user", "user": "bob", "reason": "abuse" }))).await.0, StatusCode::OK);
    let (_, q) = app.req(&admin, "GET", "/api/admin/reports", None).await;
    let uid = q["reports"].as_array().unwrap().iter().find(|r| r["kind"] == "user").unwrap()["id"].as_i64().unwrap();
    let (s, _) = app.req(&admin, "POST", &format!("/api/admin/reports/{uid}/resolve"), Some(json!({ "action": "suspend" }))).await;
    assert_eq!(s, StatusCode::OK);
    assert!(app.req(&bob, "GET", "/api/me", None).await.1["user"].is_null());
    let (_, q) = app.req(&admin, "GET", "/api/admin/reports", None).await;
    assert_eq!(q["open"], 0);
}

#[tokio::test]
async fn quotas_and_hiding_have_no_side_doors() {
    let app = App::new();
    let admin = app.signup("root1").await;
    let bob = app.signup("bob").await;
    app.req(&bob, "POST", "/api/repos", Some(json!({ "name": "r", "readme": true }))).await;
    app.req(&bob, "POST", "/api/repos/bob/r/issues", Some(json!({ "title": "secret plans" }))).await;
    // Web commits respect the storage quota too.
    let mut s = app.state.settings.get();
    s.max_storage_mb = 1;
    app.state.settings.save(&app.state.db, s).await.unwrap();
    app.state.db.call(|c| Ok(c.execute("UPDATE repos SET size_bytes = 2000000", [])?)).await.unwrap();
    let (s, e) = app.req(&bob, "POST", "/api/repos/bob/r/contents", Some(json!({ "branch": "main", "message": "m", "changes": [{ "path": "x", "content": "y" }] }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{e}");
    // A hidden repo can't be forked back into view, and a hidden issue's title leaves the feed.
    app.state.db.call(|c| Ok(c.execute("UPDATE repos SET hidden = 1, size_bytes = 0", [])?)).await.unwrap();
    let (s, _) = app.req(&bob, "POST", "/api/repos/bob/r/fork", Some(json!({ "name": "again" }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    app.state.db.call(|c| Ok(c.execute("UPDATE repos SET hidden = 0", [])?)).await.unwrap();
    let (_, before, _) = app.call(&Client::default(), "GET", "/api/feed", None, None).await;
    assert!(before.to_string().contains("secret plans"));
    app.state.db.call(|c| Ok(c.execute("UPDATE issues SET hidden = 1", [])?)).await.unwrap();
    let (_, after, _) = app.call(&Client::default(), "GET", "/api/feed", None, None).await;
    assert!(!after.to_string().contains("secret plans"), "{after}");
    // Deleting a reported repo takes its reports with it.
    let carol = app.signup("carol").await;
    app.req(&carol, "POST", "/api/reports", Some(json!({ "kind": "repo", "owner": "bob", "repo": "r", "reason": "spam" }))).await;
    app.req(&bob, "DELETE", "/api/repos/bob/r", Some(json!({ "confirm": "bob/r" }))).await;
    let (_, q) = app.req(&admin, "GET", "/api/admin/reports", None).await;
    assert_eq!(q["open"], 0);
}

fn basic(user: &str, secret: &str) -> String {
    use base64::Engine;
    format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(format!("{user}:{secret}")))
}

async fn send_raw(app: &App, method: &str, path: &str, auth: Option<&str>, body: Vec<u8>) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let mut b = Request::builder()
        .method(method)
        .uri(path.strip_prefix(ORIGIN).unwrap_or(path))
        .header("host", "test.local")
        .header("content-type", "application/vnd.git-lfs+json");
    if let Some(a) = auth {
        b = b.header("authorization", a);
    }
    let resp = app.router.clone().oneshot(b.body(Body::from(body)).unwrap()).await.unwrap();
    let (s, h) = (resp.status(), resp.headers().clone());
    (s, h, resp.into_body().collect().await.unwrap().to_bytes().to_vec())
}

async fn lfs_batch(app: &App, repo: &str, auth: Option<&str>, op: &str, oid: &str, size: usize) -> (StatusCode, Value) {
    let body = json!({ "operation": op, "transfers": ["basic"], "objects": [{ "oid": oid, "size": size }], "hash_algo": "sha256" });
    let (s, _, b) = send_raw(app, "POST", &format!("/{repo}.git/info/lfs/objects/batch"), auth, body.to_string().into_bytes()).await;
    (s, serde_json::from_slice(&b).unwrap_or(Value::Null))
}

#[tokio::test]
async fn lfs_batch_upload_and_download_check_access_and_hashes() {
    use sha2::{Digest, Sha256};
    let app = App::new();
    let alice = app.signup("alice").await;
    let bob = app.signup("bob").await;
    for (n, private) in [("p", true), ("q", true), ("pub", false)] {
        app.req(&alice, "POST", "/api/repos", Some(json!({ "name": n, "private": private, "readme": true }))).await;
    }
    let mk = |body: Value| {
        let (app, alice) = (&app, alice.clone());
        async move { app.req(&alice, "POST", "/api/me/tokens", Some(body)).await.1["token"].as_str().unwrap().to_string() }
    };
    let full = basic("alice", &mk(json!({ "name": "ci" })).await);
    let read = basic("alice", &mk(json!({ "name": "ro", "scope": "read" })).await);
    let bob_pw = basic("bob", "correct horse");
    let data = b"big binary asset\n".repeat(1000);
    let oid = hex::encode(Sha256::digest(&data));

    // Who may ask: anonymous gets a challenge, others' private repos don't exist, read tokens can't upload.
    let (s, _) = lfs_batch(&app, "alice/p", None, "download", &oid, data.len()).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (_, h, _) = send_raw(&app, "POST", "/alice/p.git/info/lfs/objects/batch", None, b"{}".to_vec()).await;
    assert!(h.get("www-authenticate").is_some(), "git-lfs needs the challenge to send credentials");
    assert_eq!(lfs_batch(&app, "alice/p", Some(&bob_pw), "download", &oid, data.len()).await.0, StatusCode::NOT_FOUND);
    assert_eq!(lfs_batch(&app, "alice/p", Some(&read), "upload", &oid, data.len()).await.0, StatusCode::FORBIDDEN);
    assert_eq!(lfs_batch(&app, "alice/p", Some(&full), "upload", "../../etc", 1).await.1["objects"][0]["error"]["code"], 422);

    // Upload: a wrong hash is refused and leaves nothing behind; the right bytes are kept.
    let (s, up) = lfs_batch(&app, "alice/p", Some(&full), "upload", &oid, data.len()).await;
    assert_eq!(s, StatusCode::OK, "{up}");
    assert_eq!(up["transfer"], "basic");
    let action = &up["objects"][0]["actions"]["upload"];
    let href = action["href"].as_str().unwrap().to_string();
    assert_eq!(href, format!("{ORIGIN}/alice/p.git/info/lfs/objects/{oid}"));
    assert_eq!(action["header"]["Authorization"], full.as_str());
    let mut wrong = data.clone();
    wrong[0] ^= 1;
    assert_eq!(send_raw(&app, "PUT", &href, Some(&full), wrong).await.0, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(send_raw(&app, "PUT", &href, Some(&read), data.clone()).await.0, StatusCode::FORBIDDEN);
    assert_eq!(send_raw(&app, "PUT", &href, None, data.clone()).await.0, StatusCode::UNAUTHORIZED);
    let (_, down) = lfs_batch(&app, "alice/p", Some(&full), "download", &oid, data.len()).await;
    assert_eq!(down["objects"][0]["error"]["code"], 404, "a refused upload stores nothing: {down}");
    assert_eq!(send_raw(&app, "PUT", &href, Some(&full), data.clone()).await.0, StatusCode::OK);
    let (_, again) = lfs_batch(&app, "alice/p", Some(&full), "upload", &oid, data.len()).await;
    assert!(again["objects"][0]["actions"].is_null(), "already uploaded: {again}");

    // Download with any read access; the object belongs to this repo only.
    let (s, down) = lfs_batch(&app, "alice/p", Some(&read), "download", &oid, data.len()).await;
    assert_eq!(s, StatusCode::OK);
    let href = down["objects"][0]["actions"]["download"]["href"].as_str().unwrap().to_string();
    let (s, _, got) = send_raw(&app, "GET", &href, Some(&read), vec![]).await;
    assert_eq!((s, got == data), (StatusCode::OK, true));
    assert_eq!(send_raw(&app, "GET", &href, None, vec![]).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(send_raw(&app, "GET", &href, Some(&bob_pw), vec![]).await.0, StatusCode::NOT_FOUND);
    let (_, other) = lfs_batch(&app, "alice/q", Some(&full), "download", &oid, data.len()).await;
    assert_eq!(other["objects"][0]["error"]["code"], 404, "knowing an oid doesn't open another repo's object");
    let q_href = format!("/alice/q.git/info/lfs/objects/{oid}");
    assert_eq!(send_raw(&app, "GET", &q_href, Some(&full), vec![]).await.0, StatusCode::NOT_FOUND);

    // A fork can fetch its parent's objects.
    let pub_href = format!("/alice/pub.git/info/lfs/objects/{oid}");
    assert_eq!(send_raw(&app, "PUT", &pub_href, Some(&full), data.clone()).await.0, StatusCode::OK);
    app.req(&bob, "POST", "/api/repos/alice/pub/fork", Some(json!({}))).await;
    let (_, fork) = lfs_batch(&app, "bob/pub", None, "download", &oid, data.len()).await;
    assert!(fork["objects"][0]["actions"]["download"]["href"].is_string(), "{fork}");

    // Per-object size limit, in the batch answer and while streaming.
    let mut s = app.state.settings.get();
    s.max_lfs_mb = 1;
    app.state.settings.save(&app.state.db, s).await.unwrap();
    let big = vec![7u8; 1024 * 1024 + 1];
    let big_oid = hex::encode(Sha256::digest(&big));
    let (_, b) = lfs_batch(&app, "alice/p", Some(&full), "upload", &big_oid, big.len()).await;
    assert_eq!(b["objects"][0]["error"]["code"], 422, "{b}");
    let big_href = format!("/alice/p.git/info/lfs/objects/{big_oid}");
    assert_eq!(send_raw(&app, "PUT", &big_href, Some(&full), big).await.0, StatusCode::PAYLOAD_TOO_LARGE);

    // No locking API: git-lfs skips lock checks on a 404.
    assert_eq!(send_raw(&app, "POST", "/alice/p.git/info/lfs/locks/verify", Some(&full), b"{}".to_vec()).await.0, StatusCode::NOT_FOUND);

    // The web view shows the pointer as an LFS file, and /raw serves the real bytes.
    let pointer = format!("version https://git-lfs.github.com/spec/v1\noid sha256:{oid}\nsize {}\n", data.len());
    let (_, br) = app.req(&alice, "GET", "/api/repos/alice/p/branches", None).await;
    let head = br["branches"][0]["sha"].as_str().unwrap().to_string();
    let change = json!({ "branch": "main", "expected_sha": head, "message": "Add asset", "changes": [{ "path": "asset.bin", "content": pointer }] });
    assert_eq!(app.req(&alice, "POST", "/api/repos/alice/p/contents", Some(change)).await.0, StatusCode::OK);
    let (_, blob) = app.req(&alice, "GET", "/api/repos/alice/p/blob?path=asset.bin", None).await;
    assert_eq!((blob["lfs"]["oid"].as_str(), blob["lfs"]["size"].as_u64(), blob["lfs"]["stored"].as_bool()), (Some(oid.as_str()), Some(data.len() as u64), Some(true)), "{blob}");
    assert_eq!(blob["editable"], false);
    assert_eq!(send_raw(&app, "GET", "/raw/alice/p/main/asset.bin", None, vec![]).await.0, StatusCode::NOT_FOUND);
    let resp = app
        .router
        .clone()
        .oneshot(Request::get("/raw/alice/p/main/asset.bin").header("host", "test.local").header("cookie", format!("gitter_session={}", alice.0.clone().unwrap())).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(resp.headers()["content-security-policy"].to_str().unwrap().contains("sandbox"));
    let got = resp.into_body().collect().await.unwrap().to_bytes().to_vec();
    assert!(got == data, "raw serves the stored object, not the pointer");
}

#[tokio::test]
async fn lfs_limits_storage_slots_and_caching() {
    use sha2::{Digest, Sha256};
    let app = App::new();
    let _admin = app.signup("root1").await;
    let bob = app.signup("bob").await;
    app.req(&bob, "POST", "/api/repos", Some(json!({ "name": "r", "readme": true }))).await;
    let pw = basic("bob", "correct horse");
    let mut s = app.state.settings.get();
    s.max_storage_mb = 1;
    app.state.settings.save(&app.state.db, s).await.unwrap();
    app.state.db.call(|c| Ok(c.execute("UPDATE repos SET size_bytes = 0", [])?)).await.unwrap();

    // LFS objects count toward the account's storage quota.
    let a = vec![1u8; 600 * 1024];
    let a_oid = hex::encode(Sha256::digest(&a));
    let a_href = format!("/bob/r.git/info/lfs/objects/{a_oid}");
    assert_eq!(send_raw(&app, "PUT", &a_href, Some(&pw), a.clone()).await.0, StatusCode::OK);
    let (_, me) = app.req(&bob, "GET", "/api/me", None).await;
    assert_eq!(me["storage"]["used"], a.len(), "{me}");
    let b = vec![2u8; 600 * 1024];
    let b_oid = hex::encode(Sha256::digest(&b));
    let (_, up) = lfs_batch(&app, "bob/r", Some(&pw), "upload", &b_oid, b.len()).await;
    assert_eq!(up["objects"][0]["error"]["code"], 422, "out of storage: {up}");
    let b_href = format!("/bob/r.git/info/lfs/objects/{b_oid}");
    assert_eq!(send_raw(&app, "PUT", &b_href, Some(&pw), b).await.0, StatusCode::PAYLOAD_TOO_LARGE);
    // Announcing an object the repo already holds needs no room (a new branch, `git lfs push --all`).
    let (_, again) = lfs_batch(&app, "bob/r", Some(&pw), "upload", &a_oid, a.len()).await;
    assert!(again["objects"][0]["error"].is_null(), "{again}");
    // Forking charges the LFS files the fork shares.
    let carol = app.signup("carol").await;
    app.req(&carol, "POST", "/api/repos", Some(json!({ "name": "mine" }))).await;
    app.state.db.call(|c| Ok(c.execute("UPDATE repos SET size_bytes = 500000 WHERE name = 'mine'", [])?)).await.unwrap();
    let (s, f) = app.req(&carol, "POST", "/api/repos/bob/r/fork", Some(json!({}))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{f}");

    // Batch answers carry credentials, so nothing may cache them.
    let (_, h, _) = send_raw(&app, "POST", "/bob/r.git/info/lfs/objects/batch", Some(&pw), br#"{"operation":"download","objects":[]}"#.to_vec()).await;
    assert_eq!(h["cache-control"], "no-store");

    // Transfers from one address are capped (separately from clones, so git-lfs's 8 parallel transfers fit).
    let held: Vec<_> = (0..8).map(|_| gitter::lfs::slot(&app.state, "unknown").unwrap()).collect();
    assert_eq!(send_raw(&app, "GET", &a_href, Some(&pw), vec![]).await.0, StatusCode::TOO_MANY_REQUESTS);
    drop(held);
    assert_eq!(send_raw(&app, "GET", &a_href, Some(&pw), vec![]).await.0, StatusCode::OK);

    // A pointer whose object isn't uploaded yet isn't cached forever at its commit.
    let pointer = format!("version https://git-lfs.github.com/spec/v1\noid sha256:{b_oid}\nsize 614400\n");
    let (_, br) = app.req(&bob, "GET", "/api/repos/bob/r/branches", None).await;
    let head = br["branches"][0]["sha"].as_str().unwrap().to_string();
    let change = json!({ "branch": "main", "expected_sha": head, "message": "m", "changes": [{ "path": "x.bin", "content": pointer }] });
    let (s, c) = app.req(&bob, "POST", "/api/repos/bob/r/contents", Some(change)).await;
    assert_eq!(s, StatusCode::OK, "{c}");
    let (_, br) = app.req(&bob, "GET", "/api/repos/bob/r/branches", None).await;
    let sha = br["branches"][0]["sha"].as_str().unwrap().to_string();
    let (s, h, _) = send_raw(&app, "GET", &format!("/raw/bob/r/{sha}/x.bin"), None, vec![]).await;
    assert_eq!(s, StatusCode::OK);
    assert!(h["cache-control"].to_str().unwrap().contains("no-store"), "{:?}", h["cache-control"]);

    // The boot sweep removes stale temp files and objects no repository holds, and keeps the rest.
    let data = &app.state.cfg.data_dir;
    let orphan = "0".repeat(64);
    let stale = |p: &std::path::Path| {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        if !p.exists() {
            std::fs::write(p, b"x").unwrap();
        }
        let f = std::fs::File::options().write(true).open(p).unwrap();
        f.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(7200)).unwrap();
    };
    let (kept, gone, tmp) = (gitter::lfs::object_path(data, &a_oid), gitter::lfs::object_path(data, &orphan), data.join("lfs/tmp/cut-off"));
    for p in [&kept, &gone, &tmp] {
        stale(p);
    }
    gitter::lfs::sweep(&app.state).await;
    assert_eq!((kept.exists(), gone.exists(), tmp.exists()), (true, false, false));

    // Parallel uploads can't overshoot the quota together.
    let mut s = app.state.settings.get();
    s.max_storage_mb = 2;
    app.state.settings.save(&app.state.db, s).await.unwrap();
    let (c, d) = (vec![3u8; 1 << 20], vec![4u8; 1 << 20]);
    let href = |x: &[u8]| format!("/bob/r.git/info/lfs/objects/{}", hex::encode(Sha256::digest(x)));
    let (hc, hd) = (href(&c), href(&d));
    let (rc, rd) = tokio::join!(send_raw(&app, "PUT", &hc, Some(&pw), c.clone()), send_raw(&app, "PUT", &hd, Some(&pw), d.clone()));
    let mut got = [rc.0, rd.0];
    got.sort();
    assert_eq!(got, [StatusCode::OK, StatusCode::PAYLOAD_TOO_LARGE]);
    let (_, me) = app.req(&bob, "GET", "/api/me", None).await;
    assert!(me["storage"]["used"].as_u64().unwrap() <= 2 << 20, "{me}");

    // Deleting the repository frees its LFS files once the trash is emptied.
    let (s, _) = app.req(&bob, "DELETE", "/api/repos/bob/r", Some(json!({ "confirm": "bob/r" }))).await;
    assert_eq!(s, StatusCode::OK);
    assert!(kept.exists(), "kept while the repository can still be restored");
    purge_trash(&app).await;
    assert!(!kept.exists(), "no repository holds it any more");
}

/// Purge everything in the trash, as the upkeep loop does a week after each delete.
async fn purge_trash(app: &App) {
    app.state.db.call(|c| Ok(c.execute("UPDATE repos SET deleted_at = 1 WHERE deleted_at IS NOT NULL", [])?)).await.unwrap();
    gitter::api::lifecycle::sweep(&app.state).await.unwrap();
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

    // Deleting the repository deletes its wiki (when the trash is emptied).
    let id: i64 = app.state.db.call(|c| Ok(c.query_row("SELECT id FROM repos WHERE name = 'pub'", [], |r| r.get(0))?)).await.unwrap();
    let dir = app.state.cfg.data_dir.join(format!("wikis/{id}.git"));
    assert!(dir.is_dir());
    let (s, _) = app.req(&alice, "DELETE", "/api/repos/alice/pub", Some(json!({ "confirm": "alice/pub" }))).await;
    assert_eq!(s, StatusCode::OK);
    purge_trash(&app).await;
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

#[tokio::test]
async fn repo_limited_tokens_cannot_make_snippets_or_touch_hidden_wikis() {
    let app = App::new();
    let _admin = app.signup("root1").await;
    let alice = app.signup("alice").await;
    app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "a", "readme": true }))).await;
    let (_, t) = app.req(&alice, "POST", "/api/repos/alice/a/deploy-tokens", Some(json!({ "name": "box", "write": true }))).await;
    let deploy = t["token"].as_str().unwrap().to_string();
    let (_, t) = app.req(&alice, "POST", "/api/me/tokens", Some(json!({ "name": "one", "repo": "alice/a" }))).await;
    let one_repo = t["token"].as_str().unwrap().to_string();
    let snippet = json!({ "title": "x", "files": [{ "name": "a.txt", "content": "hi" }] });
    for tok in [&deploy, &one_repo] {
        assert_eq!(bearer(&app, "POST", "/api/snippets", tok, Some(snippet.clone())).await, StatusCode::FORBIDDEN);
    }
    // A moderator-hidden repository's wiki and discussions are hidden with it.
    app.req(&alice, "PUT", "/api/repos/alice/a/wiki/Home", Some(json!({ "content": "secret plans", "message": "m" }))).await;
    app.state.db.call(|c| Ok(c.execute("UPDATE repos SET hidden = 1", [])?)).await.unwrap();
    let anon = Client::default();
    assert_eq!(app.req(&anon, "GET", "/api/repos/alice/a/wiki", None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(app.req(&anon, "GET", "/api/repos/alice/a/discussions", None).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn snippets_and_wikis_count_toward_storage() {
    let app = App::new();
    let _admin = app.signup("root1").await;
    let bob = app.signup("bob").await;
    app.req(&bob, "POST", "/api/repos", Some(json!({ "name": "r" }))).await;
    let (s, v) = app.req(&bob, "POST", "/api/snippets", Some(json!({ "title": "t", "files": [{ "name": "a.txt", "content": "x".repeat(600_000) }] }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let id = v["id"].as_str().unwrap().to_string();
    let (_, me) = app.req(&bob, "GET", "/api/me", None).await;
    let used = me["storage"]["used"].as_u64().unwrap();
    assert!(used > 0, "the snippet's repository counts: {me}");
    app.state.db.call(|c| Ok(c.execute("UPDATE repos SET size_bytes = 0", [])?)).await.unwrap();
    let mut s = app.state.settings.get();
    s.max_storage_mb = 1;
    app.state.settings.save(&app.state.db, s).await.unwrap();
    // About 1 MB in use now (the snippet is stored compressed, so pad the account to the limit).
    app.state.db.call(|c| Ok(c.execute("UPDATE snippets SET size_bytes = 1000000", [])?)).await.unwrap();
    let big = json!({ "title": "t2", "files": [{ "name": "b.txt", "content": "y".repeat(100_000) }] });
    let (s, e) = app.req(&bob, "POST", "/api/snippets", Some(big.clone())).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "out of storage: {e}");
    let (s, e) = app.req(&bob, "PUT", &format!("/api/snippets/{id}"), Some(big)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "out of storage: {e}");
    let (s, e) = app.req(&bob, "POST", "/api/repos/bob/r/wiki", Some(json!({ "name": "Home", "content": "z".repeat(100_000) }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "out of storage: {e}");
}

// ---------- signed commits ----------

/// Whether a command-line tool is installed (being able to start it is enough).
fn have_tool(tool: &str) -> bool {
    let quiet = || std::process::Stdio::null();
    std::process::Command::new(tool).arg("--help").stdout(quiet()).stderr(quiet()).status().is_ok()
}

/// Run a program away from the user's git config; panics with its stderr on failure. Returns stdout.
fn run_tool(prog: &str, args: &[&str], env: &[(&str, &str)], input: Option<&[u8]>) -> String {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = std::process::Command::new(prog)
        .args(args)
        .envs(env.iter().copied())
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(input.unwrap_or_default()).unwrap();
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "{prog} {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Add `file` on top of main as `email`, signed when `sign` holds the git options for it. Returns the sha.
fn commit_on_main(repo: &std::path::Path, file: &str, email: &str, sign: &[String], env: &[(&str, &str)]) -> String {
    let gd = format!("--git-dir={}", repo.display());
    let mut env = env.to_vec();
    env.extend([("GIT_AUTHOR_NAME", "Alice"), ("GIT_AUTHOR_EMAIL", email), ("GIT_COMMITTER_NAME", "Alice"), ("GIT_COMMITTER_EMAIL", email)]);
    let git = |args: &[&str], input: Option<&[u8]>| {
        let mut a = vec![gd.as_str()];
        a.extend(args);
        run_tool("git", &a, &env, input).trim().to_string()
    };
    let head = git(&["rev-parse", "main"], None);
    let blob = git(&["hash-object", "-w", "--stdin"], Some(file.as_bytes()));
    let listing = git(&["ls-tree", "main"], None);
    let tree = git(&["mktree"], Some(format!("{listing}\n100644 blob {blob}\t{file}\n").as_bytes()));
    let mut args: Vec<&str> = sign.iter().map(String::as_str).collect();
    args.extend(["commit-tree", "-p", head.as_str(), "-m", file, tree.as_str()]);
    if !sign.is_empty() {
        args.push("-S");
    }
    let sha = git(&args, None);
    git(&["update-ref", "refs/heads/main", &sha], None);
    sha
}

/// A copy of signed commit `sha` with its message changed but the signature kept.
fn tamper(repo: &std::path::Path, sha: &str) -> String {
    let gd = format!("--git-dir={}", repo.display());
    let raw = run_tool("git", &[&gd, "cat-file", "commit", sha], &[], None);
    let forged = raw.replace("\n\n", "\n\nchanged after signing: ");
    run_tool("git", &[&gd, "hash-object", "-t", "commit", "-w", "--stdin"], &[], Some(forged.as_bytes())).trim().to_string()
}

/// Commit `sha` with its signature header replaced by `sig`.
fn resign(repo: &std::path::Path, sha: &str, sig: &str) -> String {
    let gd = format!("--git-dir={}", repo.display());
    let raw = run_tool("git", &[&gd, "cat-file", "commit", sha], &[], None);
    let (head, body) = raw.split_once("\n\n").unwrap();
    let (mut out, mut in_sig) = (Vec::new(), false);
    for l in head.lines() {
        in_sig = l.starts_with("gpgsig ") || (in_sig && l.starts_with(' '));
        if !in_sig {
            out.push(l.to_string());
        }
    }
    let mut lines = sig.trim_end().lines();
    out.push(format!("gpgsig {}", lines.next().unwrap()));
    out.extend(lines.map(|l| format!(" {l}")));
    let forged = format!("{}\n\n{body}", out.join("\n"));
    run_tool("git", &[&gd, "hash-object", "-t", "commit", "-w", "--stdin"], &[], Some(forged.as_bytes())).trim().to_string()
}

async fn commit_verification(app: &App, c: &Client, sha: &str) -> Value {
    let (s, v) = app.req(c, "GET", &format!("/api/repos/alice/r/commit/{sha}"), None).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    v["commit"]["verification"].clone()
}

#[tokio::test]
async fn signed_commits_ssh_keys_verify_and_say_why_not() {
    if !have_tool("ssh-keygen") {
        eprintln!("ssh-keygen isn't installed: SSH signature checks skipped");
        return;
    }
    let app = App::new();
    let alice = app.signup("alice").await;
    app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "r", "readme": true }))).await;
    let repo = app.state.git.repo_path(1);
    let dir = tempfile::tempdir().unwrap();
    let (key, other) = (dir.path().join("k"), dir.path().join("o"));
    for k in [&key, &other] {
        run_tool("ssh-keygen", &["-q", "-t", "ed25519", "-N", "", "-C", "laptop", "-f", k.to_str().unwrap()], &[], None);
    }
    let pubfile = dir.path().join("k.pub");
    let (s, k) = app.req(&alice, "POST", "/api/me/signing-keys", Some(json!({ "key": std::fs::read_to_string(&pubfile).unwrap() }))).await;
    assert_eq!(s, StatusCode::OK, "{k}");
    assert_eq!((k["kind"].as_str(), k["title"].as_str()), (Some("ssh"), Some("laptop")), "the key's comment is the default title");
    let fp = k["fingerprint"].as_str().unwrap().to_string();
    let listed = run_tool("ssh-keygen", &["-l", "-f", pubfile.to_str().unwrap()], &[], None);
    assert_eq!(listed.split_whitespace().nth(1), Some(fp.as_str()), "same fingerprint as ssh-keygen");
    // Someone else adding alice's public key changes nothing for her commits.
    let carol = app.signup("carol").await;
    let (s, _) = app.req(&carol, "POST", "/api/me/signing-keys", Some(json!({ "key": std::fs::read_to_string(&pubfile).unwrap() }))).await;
    assert_eq!(s, StatusCode::OK);

    let ssh = |k: &std::path::Path| vec!["-c".into(), "gpg.format=ssh".into(), "-c".into(), format!("user.signingkey={}", k.display())];
    let good = commit_on_main(&repo, "a.txt", "alice@example.com", &ssh(&key), &[]);
    let noreply = commit_on_main(&repo, "b.txt", "1+alice@users.noreply.gitter", &ssh(&key), &[]);
    let wrong_email = commit_on_main(&repo, "c.txt", "mallory@example.com", &ssh(&key), &[]);
    let unknown = commit_on_main(&repo, "d.txt", "alice@example.com", &ssh(&other), &[]);
    let plain = commit_on_main(&repo, "e.txt", "alice@example.com", &[], &[]);
    let tampered = tamper(&repo, &good);

    let (s, list) = app.req(&alice, "GET", "/api/repos/alice/r/commits?rev=main", None).await;
    assert_eq!(s, StatusCode::OK, "{list}");
    let v = |list: &Value, sha: &str| list["commits"].as_array().unwrap().iter().find(|c| c["sha"] == sha).unwrap()["verification"].clone();
    let g = v(&list, &good);
    assert_eq!((g["status"].as_str(), g["signer"].as_str(), g["key_fingerprint"].as_str()), (Some("verified"), Some("alice"), Some(fp.as_str())), "{g}");
    assert_eq!(v(&list, &noreply)["status"], "verified", "the account's no-reply address counts as its email");
    let w = v(&list, &wrong_email);
    assert_eq!(w["status"], "unverified");
    assert!(w["reason"].as_str().unwrap().contains("committer email"), "{w}");
    let u = v(&list, &unknown);
    assert_eq!(u["status"], "unverified");
    assert!(u["reason"].as_str().unwrap().contains("isn't registered"), "{u}");
    assert_eq!(v(&list, &plain)["status"], "unsigned");

    // The commit page and blame say the same.
    let t = commit_verification(&app, &alice, &tampered).await;
    assert_eq!(t["status"], "unverified");
    assert!(t["reason"].as_str().unwrap().contains("doesn't match"), "{t}");
    assert_eq!(commit_verification(&app, &alice, &good).await["status"], "verified");
    let (_, bl) = app.req(&alice, "GET", "/api/repos/alice/r/blame?rev=main&path=a.txt", None).await;
    assert_eq!(bl["chunks"][0]["verification"]["status"], "verified", "{bl}");

    // Removing the key takes "Verified" away at once.
    let (s, _) = app.req(&alice, "DELETE", &format!("/api/me/signing-keys/{}", k["id"]), None).await;
    assert_eq!(s, StatusCode::OK);
    let (_, list) = app.req(&alice, "GET", "/api/repos/alice/r/commits?rev=main", None).await;
    assert_eq!(v(&list, &good)["status"], "unverified");
}

#[tokio::test]
async fn signed_commits_gpg_keys_verify_once_registered() {
    if !have_tool("gpg") {
        eprintln!("gpg isn't installed: GPG signature checks skipped");
        return;
    }
    let app = App::new();
    let alice = app.signup("alice").await;
    app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "r", "readme": true }))).await;
    let repo = app.state.git.repo_path(1);
    let home = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    std::fs::set_permissions(home.path(), std::os::unix::fs::PermissionsExt::from_mode(0o700)).unwrap();
    let env = [("GNUPGHOME", home.path().to_str().unwrap())];
    run_tool("gpg", &["--batch", "--passphrase", "", "--quick-gen-key", "Alice <alice@example.com>", "ed25519", "sign", "never"], &env, None);
    let colons = run_tool("gpg", &["--batch", "--with-colons", "--list-keys"], &env, None);
    let fpr = colons.lines().find(|l| l.starts_with("fpr:")).unwrap().split(':').nth(9).unwrap().to_string();
    let armored = run_tool("gpg", &["--batch", "--armor", "--export", &fpr], &env, None);
    let sign = vec!["-c".into(), "gpg.format=openpgp".into(), "-c".into(), format!("user.signingkey={fpr}")];
    let good = commit_on_main(&repo, "a.txt", "alice@example.com", &sign, &env);
    let tampered = tamper(&repo, &good);

    let before = commit_verification(&app, &alice, &good).await;
    assert_eq!(before["status"], "unverified", "{before}");
    assert!(before["reason"].as_str().unwrap().contains("isn't registered"), "{before}");
    let (s, k) = app.req(&alice, "POST", "/api/me/signing-keys", Some(json!({ "title": "work", "key": armored }))).await;
    assert_eq!(s, StatusCode::OK, "{k}");
    assert_eq!((k["kind"].as_str(), k["fingerprint"].as_str()), (Some("gpg"), Some(fpr.as_str())));
    let after = commit_verification(&app, &alice, &good).await;
    assert_eq!((after["status"].as_str(), after["signer"].as_str()), (Some("verified"), Some("alice")), "adding a key re-checks commits seen before: {after}");
    assert_eq!(after["key_fingerprint"], fpr.as_str());
    let t = commit_verification(&app, &alice, &tampered).await;
    assert_eq!(t["status"], "unverified");
    assert!(t["reason"].as_str().unwrap().contains("doesn't match"), "{t}");
    // A signed message relabelled as a signature: gpg would call its own text well signed
    // and ignore the commit, so it must not verify the commit.
    let msg = run_tool("gpg", &["--batch", "--armor", "-z", "0", "--sign", "-u", &fpr], &env, Some(b"any text alice once signed"));
    let forged = resign(&repo, &good, &msg.replace("PGP MESSAGE", "PGP SIGNATURE"));
    let f = commit_verification(&app, &alice, &forged).await;
    assert_eq!(f["status"], "unverified", "{f}");

    // A restart without the keyring (a restore doesn't carry it) rebuilds it from the database.
    std::fs::remove_dir_all(app.state.cfg.data_dir.join("gpg")).unwrap();
    let fresh = gitter::router(gitter::AppState::new((*app.state.cfg).clone(), app.state.db.clone()));
    let req = Request::get(format!("/api/repos/alice/r/commit/{good}")).header("host", "test.local").body(Body::empty()).unwrap();
    let resp = fresh.oneshot(req).await.unwrap();
    let body: Value = serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["commit"]["verification"]["status"], "verified", "{body}");
    let _ = std::process::Command::new("gpgconf").args(["--kill", "all"]).envs(env).status();
}

#[tokio::test]
async fn signed_commits_keys_are_validated_owned_and_audited() {
    let app = App::new();
    let alice = app.signup("alice").await;
    let bob = app.signup("bob").await;
    let key = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFxsPyhOovC3igNqTo8u07KB2pQB9asL+9wpOgr9jf9G test";
    let add = |c: &Client, k: String| {
        let c = c.clone();
        let app = &app;
        async move { app.req(&c, "POST", "/api/me/signing-keys", Some(json!({ "key": k }))).await }
    };
    for (bad, why) in [
        ("hello".to_string(), "not a key"),
        (key.replace("ssh-ed25519", "ssh-rsa"), "the type doesn't match the key"),
        (format!("{key}\n{key}"), "two keys"),
        (format!("ssh-ed25519 {}", "A".repeat(17_000)), "over 16 KB"),
    ] {
        let (s, e) = add(&alice, bad).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{why}: {e}");
    }
    let (s, e) = add(&alice, "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaA==\n-----END OPENSSH PRIVATE KEY-----\n".into()).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(e["error"].as_str().unwrap().contains("private"), "{e}");

    let (s, k) = add(&alice, format!("  {key}\n")).await;
    assert_eq!(s, StatusCode::OK, "{k}");
    assert_eq!(k["fingerprint"], "SHA256:tOn8xeW71HpDpSvWJO1qQFsmVDC03PosHw9pOnPgg1M");
    assert_eq!(add(&alice, key.into()).await.0, StatusCode::CONFLICT, "already added");
    // Adding someone else's public key doesn't lock them out (and can't verify as them).
    let (s, b) = add(&bob, key.into()).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (_, theirs) = app.req(&bob, "GET", "/api/me/signing-keys", None).await;
    assert_eq!(theirs["keys"].as_array().unwrap().len(), 1);
    app.req(&bob, "DELETE", &format!("/api/me/signing-keys/{}", k["id"]), None).await;
    let (_, mine) = app.req(&alice, "GET", "/api/me/signing-keys", None).await;
    assert_eq!(mine["keys"].as_array().unwrap().len(), 1, "bob can't remove alice's key");
    assert_eq!(audit_count(&app, "signing_key_added").await, 2);

    // Adding or removing a key is a sensitive action: it needs a recent password.
    app.state.db.call(|c| Ok(c.execute("UPDATE sessions SET verified_at = verified_at - 3600", [])?)).await.unwrap();
    let (s, e) = app.req(&alice, "DELETE", &format!("/api/me/signing-keys/{}", k["id"]), None).await;
    assert_eq!((s, e["code"].as_str()), (StatusCode::FORBIDDEN, Some("reauth")));
    app.req(&alice, "POST", "/api/auth/reauth", Some(json!({ "password": "correct horse" }))).await;
    let (s, _) = app.req(&alice, "DELETE", &format!("/api/me/signing-keys/{}", k["id"]), None).await;
    assert_eq!(s, StatusCode::OK);
    let (_, mine) = app.req(&alice, "GET", "/api/me/signing-keys", None).await;
    assert_eq!(mine["keys"].as_array().unwrap().len(), 0);
    assert_eq!(audit_count(&app, "signing_key_deleted").await, 1);
}

/// A request as `c` (cookie and Origin), keeping the response headers.
async fn with_headers(app: &App, c: &Client, method: &str, path: &str, body: Option<Value>) -> (StatusCode, axum::http::HeaderMap) {
    let mut b = Request::builder().method(method).uri(path).header("host", "test.local").header("origin", ORIGIN);
    if let Some(cookie) = &c.0 {
        b = b.header("cookie", format!("gitter_session={cookie}"));
    }
    let req = match body {
        Some(v) => b.header("content-type", "application/json").body(Body::from(v.to_string())).unwrap(),
        None => b.body(Body::empty()).unwrap(),
    };
    let resp = app.router.clone().oneshot(req).await.unwrap();
    (resp.status(), resp.headers().clone())
}

fn location(h: &axum::http::HeaderMap) -> String {
    h.get("location").and_then(|v| v.to_str().ok()).unwrap_or("").to_string()
}

#[tokio::test]
async fn rename_transfer_redirects_old_urls_and_needs_acceptance() {
    let app = App::new();
    let _root = app.signup("root1").await;
    let alice = app.signup("alice").await;
    let bob = app.signup("bob").await;
    let carol = app.signup("carol").await;
    let anon = Client::default();
    app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "proj", "readme": true }))).await;
    let token = |who: Client| {
        let app = &app;
        async move { app.req(&who, "POST", "/api/me/tokens", Some(json!({ "name": "git" }))).await.1["token"].as_str().unwrap().to_string() }
    };
    let alice_tok = token(alice.clone()).await;

    // Rename: the web and the API answer 301 to the new name, keeping the rest of the path.
    let (s, r) = app.req(&alice, "PATCH", "/api/repos/alice/proj", Some(json!({ "name": "renamed" }))).await;
    assert_eq!((s, r["name"].as_str()), (StatusCode::OK, Some("renamed")));
    let (s, h) = with_headers(&app, &anon, "GET", "/api/repos/alice/proj/issues?state=open", None).await;
    assert_eq!((s, location(&h).as_str()), (StatusCode::MOVED_PERMANENTLY, "/api/repos/alice/renamed/issues?state=open"));
    assert_eq!(h.get("cache-control").and_then(|v| v.to_str().ok()), Some("no-store"), "a name can be reused, so redirects aren't cached");
    let (s, h) = with_headers(&app, &anon, "GET", "/alice/proj/issues", None).await;
    assert_eq!((s, location(&h).as_str()), (StatusCode::MOVED_PERMANENTLY, "/alice/renamed/issues"));
    let (s, h) = with_headers(&app, &anon, "GET", "/raw/alice/proj/main/README.md", None).await;
    assert_eq!((s, location(&h).as_str()), (StatusCode::MOVED_PERMANENTLY, "/raw/alice/renamed/main/README.md"));
    // Writes keep their method (308), so a client that follows them doesn't turn a POST into a GET.
    let (s, h) = with_headers(&app, &alice, "POST", "/api/repos/alice/proj/issues", Some(json!({ "title": "x" }))).await;
    assert_eq!((s, location(&h).as_str()), (StatusCode::PERMANENT_REDIRECT, "/api/repos/alice/renamed/issues"));
    // Git and Git LFS are served right at the old URL: git doesn't follow redirects when pushing.
    assert_eq!(status_with(&app, git_refs("proj", "git-upload-pack", &alice_tok)).await, StatusCode::OK);
    assert_eq!(status_with(&app, git_refs("proj", "git-receive-pack", &alice_tok)).await, StatusCode::OK);
    let (s, b) = lfs_batch(&app, "alice/proj", Some(&basic("alice", &alice_tok)), "upload", &"a".repeat(64), 1).await;
    assert_eq!(s, StatusCode::OK, "{b}");

    // A private repository's new name isn't given away to people who can't see it.
    app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "hush", "private": true }))).await;
    app.req(&alice, "PATCH", "/api/repos/alice/hush", Some(json!({ "name": "hush2" }))).await;
    assert_eq!(with_headers(&app, &anon, "GET", "/api/repos/alice/hush", None).await.0, StatusCode::NOT_FOUND);
    let (s, h) = with_headers(&app, &anon, "GET", "/alice/hush", None).await;
    assert_eq!((s, location(&h).as_str()), (StatusCode::OK, ""), "the app shell, which shows a 404");
    assert_eq!(with_headers(&app, &alice, "GET", "/api/repos/alice/hush", None).await.0, StatusCode::MOVED_PERMANENTLY);

    // A new repository that takes the old name drops the redirect, for good.
    app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "proj" }))).await;
    let (s, v) = app.req(&anon, "GET", "/api/repos/alice/proj", None).await;
    assert_eq!((s, v["empty"].as_bool()), (StatusCode::OK, Some(true)), "{v}");
    app.req(&alice, "DELETE", "/api/repos/alice/proj", Some(json!({ "confirm": "alice/proj" }))).await;
    assert_eq!(app.req(&anon, "GET", "/api/repos/alice/proj", None).await.0, StatusCode::NOT_FOUND);

    // Transfers are offers: nothing moves until the recipient accepts.
    let offer = |to: &str, full: &str| json!({ "to": to, "confirm": full });
    let (s, _) = app.req(&alice, "POST", "/api/repos/alice/renamed/transfer", Some(offer("bob", "alice/wrong"))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "typed confirmation");
    let (s, _) = app.req(&alice, "POST", "/api/repos/alice/renamed/transfer", Some(offer("nobody-here", "alice/renamed"))).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, e) = app.req(&alice, "POST", "/api/repos/alice/renamed/transfer", Some(offer("bob", "alice/renamed"))).await;
    assert_eq!(s, StatusCode::OK, "{e}");
    assert_eq!(app.req(&bob, "GET", "/api/repos/bob/renamed", None).await.0, StatusCode::NOT_FOUND);
    let (_, out) = app.req(&alice, "GET", "/api/repos/alice/renamed/transfer", None).await;
    assert_eq!(out["offer"]["to"], "bob");
    let (_, list) = app.req(&bob, "GET", "/api/me/transfers", None).await;
    let inc = &list["incoming"][0];
    assert_eq!((inc["repo"].as_str(), inc["from"].as_str()), (Some("alice/renamed"), Some("alice")), "{list}");
    let accept = format!("/api/me/transfers/{}/accept", inc["id"].as_i64().unwrap());
    assert_eq!(app.req(&carol, "POST", &accept, None).await.0, StatusCode::NOT_FOUND, "only the recipient accepts");

    // Accepting checks the recipient's repository limit and storage quota.
    app.req(&bob, "POST", "/api/repos", Some(json!({ "name": "mine" }))).await;
    let mut set = app.state.settings.get();
    set.max_repos_per_user = 1;
    app.state.settings.save(&app.state.db, set.clone()).await.unwrap();
    let (s, e) = app.req(&bob, "POST", &accept, None).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "over the repository limit: {e}");
    set.max_repos_per_user = 50;
    set.max_storage_mb = 1;
    app.state.settings.save(&app.state.db, set.clone()).await.unwrap();
    app.state.db.call(|c| Ok(c.execute("UPDATE repos SET size_bytes = 2000000 WHERE name = 'renamed'", [])?)).await.unwrap();
    let (s, e) = app.req(&bob, "POST", &accept, None).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "over the storage quota: {e}");
    set.max_storage_mb = 0;
    app.state.settings.save(&app.state.db, set).await.unwrap();
    let (s, e) = app.req(&bob, "POST", &accept, None).await;
    assert_eq!(s, StatusCode::OK, "{e}");
    assert_eq!(audit_count(&app, "repo_transferred").await, 1);

    // The old URLs follow it to its new owner; the old git remote works for the new owner only.
    let (s, h) = with_headers(&app, &anon, "GET", "/api/repos/alice/renamed", None).await;
    assert_eq!((s, location(&h).as_str()), (StatusCode::MOVED_PERMANENTLY, "/api/repos/bob/renamed"));
    assert_eq!(app.req(&bob, "GET", "/api/repos/bob/renamed", None).await.1["is_owner"], true);
    let bob_tok = token(bob.clone()).await;
    assert_eq!(status_with(&app, git_refs("renamed", "git-receive-pack", &bob_tok)).await, StatusCode::OK);
    assert_eq!(status_with(&app, git_refs("renamed", "git-receive-pack", &alice_tok)).await, StatusCode::FORBIDDEN);
    assert_eq!(app.req(&bob, "GET", "/api/me/transfers", None).await.1["incoming"].as_array().unwrap().len(), 0);

    // Declined and expired offers move nothing.
    app.req(&bob, "POST", "/api/repos/bob/mine/transfer", Some(offer("carol", "bob/mine"))).await;
    let (_, l) = app.req(&carol, "GET", "/api/me/transfers", None).await;
    let id = l["incoming"][0]["id"].as_i64().unwrap();
    assert_eq!(app.req(&carol, "POST", &format!("/api/me/transfers/{id}/decline"), None).await.0, StatusCode::OK);
    assert_eq!(app.req(&carol, "POST", &format!("/api/me/transfers/{id}/accept"), None).await.0, StatusCode::NOT_FOUND);
    app.req(&bob, "POST", "/api/repos/bob/mine/transfer", Some(offer("carol", "bob/mine"))).await;
    app.state.db.call(|c| Ok(c.execute("UPDATE repo_transfers SET expires_at = 1", [])?)).await.unwrap();
    assert_eq!(app.req(&carol, "GET", "/api/me/transfers", None).await.1["incoming"].as_array().unwrap().len(), 0);
    assert_eq!(app.req(&carol, "POST", &format!("/api/me/transfers/{id}/accept"), None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(app.req(&bob, "GET", "/api/repos/bob/mine", None).await.1["is_owner"], true);
}

#[tokio::test]
async fn trash_keeps_deleted_repos_for_a_week_then_purges_them() {
    use sha2::{Digest, Sha256};
    let app = App::new();
    let root = app.signup("root1").await;
    let alice = app.signup("alice").await;
    let bob = app.signup("bob").await;
    let anon = Client::default();
    app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "t", "readme": true }))).await;
    app.req(&bob, "POST", "/api/repos/alice/t/star", None).await;
    let pw = basic("alice", "correct horse");
    let data = b"lfs payload\n".repeat(100);
    let oid = hex::encode(Sha256::digest(&data));
    assert_eq!(send_raw(&app, "PUT", &format!("/alice/t.git/info/lfs/objects/{oid}"), Some(&pw), data.clone()).await.0, StatusCode::OK);
    let obj = gitter::lfs::object_path(&app.state.cfg.data_dir, &oid);
    assert!(obj.is_file());
    let used = app.req(&alice, "GET", "/api/me", None).await.1["storage"]["used"].as_i64().unwrap();
    let id: i64 = app.state.db.call(|c| Ok(c.query_row("SELECT id FROM repos WHERE name = 't'", [], |r| r.get(0))?)).await.unwrap();
    let data_dir = app.state.cfg.data_dir.clone();
    let (live, trashed) = (data_dir.join(format!("repos/{id}.git")), data_dir.join(format!("trash/{id}.git")));

    // Deleting moves it to the trash: gone from every view, but kept on disk and still counted.
    assert_eq!(app.req(&alice, "DELETE", "/api/repos/alice/t", Some(json!({ "confirm": "alice/t" }))).await.0, StatusCode::OK);
    assert!(!live.exists() && trashed.is_dir());
    assert_eq!(app.req(&alice, "GET", "/api/repos/alice/t", None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(app.req(&alice, "GET", "/api/repos/alice/t/issues", None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(raw_get(&app, "/raw/alice/t/main/README.md").await.0, StatusCode::NOT_FOUND);
    assert_eq!(send_raw(&app, "GET", "/alice/t.git/info/refs?service=git-upload-pack", Some(&pw), vec![]).await.0, StatusCode::NOT_FOUND);
    assert_eq!(send_raw(&app, "GET", &format!("/alice/t.git/info/lfs/objects/{oid}"), Some(&pw), vec![]).await.0, StatusCode::NOT_FOUND);
    for (who, p) in [(&anon, "/api/explore"), (&anon, "/api/users/alice"), (&alice, "/api/search?q=t"), (&alice, "/api/feed"), (&anon, "/api/users/bob/stars"), (&anon, "/api/feed")] {
        let v = app.req(who, "GET", p, None).await.1.to_string();
        assert!(!v.contains("\"t\"") && !v.contains("alice/t"), "{p} still shows the deleted repo: {v}");
    }
    assert_eq!(app.req(&alice, "GET", "/api/me", None).await.1["storage"]["used"].as_i64().unwrap(), used, "counted until purged");
    let (_, tr) = app.req(&alice, "GET", "/api/me/trash", None).await;
    assert_eq!((tr["repos"][0]["id"].as_i64(), tr["repos"][0]["name"].as_str()), (Some(id), Some("t")), "{tr}");
    assert!(tr["repos"][0]["purge_at"].as_i64().unwrap() > gitter::db::now() + 6 * 86400);

    // The name is free meanwhile; restoring needs it free again.
    assert_eq!(app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "t" }))).await.0, StatusCode::OK);
    let restore = format!("/api/me/trash/{id}/restore");
    assert_eq!(app.req(&alice, "POST", &restore, None).await.0, StatusCode::CONFLICT);
    assert_eq!(app.req(&bob, "POST", &restore, None).await.0, StatusCode::NOT_FOUND, "only the owner restores");
    app.req(&alice, "DELETE", "/api/repos/alice/t", Some(json!({ "confirm": "alice/t" }))).await;
    let (s, e) = app.req(&alice, "POST", &restore, None).await;
    assert_eq!(s, StatusCode::OK, "{e}");
    let (s, r) = app.req(&anon, "GET", "/api/repos/alice/t", None).await;
    assert_eq!((s, r["empty"].as_bool(), r["stars"].as_i64()), (StatusCode::OK, Some(false), Some(1)), "back with its history and stars");
    assert!(live.is_dir() && !trashed.exists());
    assert_eq!(audit_count(&app, "repo_restored").await, 1);

    // A week later the upkeep loop purges it, LFS files included; younger ones stay.
    app.req(&alice, "DELETE", "/api/repos/alice/t", Some(json!({ "confirm": "alice/t" }))).await;
    app.state.db.call(move |c| Ok(c.execute("UPDATE repos SET deleted_at = deleted_at - 8 * 86400 WHERE id = ?1", [id])?)).await.unwrap();
    gitter::api::lifecycle::sweep(&app.state).await.unwrap();
    assert!(!trashed.exists(), "folder purged");
    assert!(!obj.exists(), "its LFS file is collected");
    let gone: i64 = app.state.db.call(move |c| Ok(c.query_row("SELECT count(*) FROM repos WHERE id = ?1", [id], |r| r.get(0))?)).await.unwrap();
    assert_eq!(gone, 0);
    assert_eq!(audit_count(&app, "repo_purged").await, 1);
    let (_, tr) = app.req(&alice, "GET", "/api/me/trash", None).await;
    let other = tr["repos"][0]["id"].as_i64().expect("the younger one stays");

    // Admins can purge right away.
    let purge = format!("/api/admin/trash/{other}");
    assert_eq!(app.req(&alice, "DELETE", &purge, None).await.0, StatusCode::FORBIDDEN);
    assert_eq!(app.req(&root, "GET", "/api/admin/trash", None).await.1["repos"][0]["name"], "t");
    assert_eq!(app.req(&root, "DELETE", &purge, None).await.0, StatusCode::OK);
    assert_eq!(app.req(&alice, "GET", "/api/me/trash", None).await.1["repos"].as_array().unwrap().len(), 0);
    assert!(!data_dir.join(format!("trash/{other}.git")).exists());
}

#[tokio::test]
async fn export_and_self_delete_cover_the_whole_account() {
    use std::io::Read;
    let app = App::new();
    let root = app.signup("root1").await;
    let alice = app.signup("alice").await;
    let bob = app.signup("bob").await;
    app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "code", "readme": true }))).await;
    app.req(&alice, "POST", "/api/repos/alice/code/wiki", Some(json!({ "name": "Home", "content": "wiki words" }))).await;
    app.req(&alice, "POST", "/api/repos/alice/code/issues", Some(json!({ "title": "Exported issue", "body": "b" }))).await;
    app.req(&alice, "POST", "/api/repos/alice/code/issues/1/comments", Some(json!({ "body": "Exported comment" }))).await;
    app.req(&alice, "POST", "/api/repos/alice/code/discussions", Some(json!({ "title": "Exported talk", "category": "general" }))).await;
    let (_, sn) = app.req(&alice, "POST", "/api/snippets", Some(json!({ "title": "Snip", "files": [{ "name": "a.txt", "content": "hi" }] }))).await;
    let snip = sn["id"].as_str().unwrap().to_string();

    // The export is built in the background, then downloadable.
    let (s, e) = app.req(&alice, "POST", "/api/me/export", None).await;
    assert_eq!(s, StatusCode::OK, "{e}");
    let mut status = Value::Null;
    for _ in 0..400 {
        status = app.req(&alice, "GET", "/api/me/export", None).await.1;
        if status["export"]["status"] != "running" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    assert_eq!(status["export"]["status"], "ready", "{status}");
    let download = |who: &Client| {
        let req = Request::get("/api/me/export/download").header("host", "test.local").header("cookie", format!("gitter_session={}", who.0.clone().unwrap()));
        app.router.clone().oneshot(req.body(Body::empty()).unwrap())
    };
    let resp = download(&alice).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(resp.headers()["content-disposition"].to_str().unwrap().starts_with("attachment"));
    let bytes = resp.into_body().collect().await.unwrap().to_bytes().to_vec();
    let mut files = std::collections::HashMap::new();
    for e in tar::Archive::new(flate2::read::GzDecoder::new(&bytes[..])).entries().unwrap() {
        let mut e = e.unwrap();
        let name = e.path().unwrap().to_string_lossy().into_owned();
        let mut buf = Vec::new();
        e.read_to_end(&mut buf).unwrap();
        files.insert(name, String::from_utf8_lossy(&buf).into_owned());
    }
    let has = |name: &str, text: &str| files.get(name).is_some_and(|f| f.contains(text));
    assert!(has("profile.json", "alice@example.com"), "{:?}", files.keys());
    assert!(has("issues.json", "Exported issue") && has("comments.json", "Exported comment") && has("discussions.json", "Exported talk"));
    assert!(has("snippets.json", "Snip") && has("security-log.json", "signup"));
    for f in ["repositories/code.bundle", "wikis/code.bundle", &format!("snippets/{snip}.bundle")] {
        assert!(files.contains_key(f), "{f} missing: {:?}", files.keys());
    }
    let dir = tempfile::tempdir().unwrap();
    let bundle = dir.path().join("code.bundle");
    let resp = download(&alice).await.unwrap();
    let raw = resp.into_body().collect().await.unwrap().to_bytes().to_vec();
    tar::Archive::new(flate2::read::GzDecoder::new(&raw[..])).unpack(dir.path().join("x")).unwrap();
    std::fs::copy(dir.path().join("x/repositories/code.bundle"), &bundle).unwrap();
    let heads = std::process::Command::new("git").args(["bundle", "list-heads"]).arg(&bundle).output().unwrap();
    assert!(String::from_utf8_lossy(&heads.stdout).contains("refs/heads/main"), "a real git bundle");
    assert_eq!(download(&bob).await.unwrap().status(), StatusCode::NOT_FOUND, "everyone downloads only their own");

    // One at a time, and gone after a day.
    app.state.db.call(|c| Ok(c.execute("UPDATE exports SET status = 'running'", [])?)).await.unwrap();
    assert_eq!(app.req(&alice, "POST", "/api/me/export", None).await.0, StatusCode::CONFLICT);
    app.state.db.call(|c| Ok(c.execute("UPDATE exports SET status = 'ready', expires_at = 1", [])?)).await.unwrap();
    assert_eq!(download(&alice).await.unwrap().status(), StatusCode::NOT_FOUND);

    // Self-delete: typed username, never the last admin, never with a token; repos (trashed ones too) go with it.
    app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "old" }))).await;
    app.req(&alice, "DELETE", "/api/repos/alice/old", Some(json!({ "confirm": "alice/old" }))).await;
    let ids: Vec<i64> = app.state.db.call(|c| {
        let mut s = c.prepare("SELECT r.id FROM repos r JOIN users u ON u.id = r.owner_id WHERE u.username = 'alice' ORDER BY r.id")?;
        Ok(s.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?)
    }).await.unwrap();
    assert_eq!(ids.len(), 2);
    assert_eq!(app.req(&root, "DELETE", "/api/me", Some(json!({ "confirm": "root1" }))).await.0, StatusCode::BAD_REQUEST, "the last admin stays");
    assert_eq!(app.req(&alice, "DELETE", "/api/me", Some(json!({ "confirm": "bob" }))).await.0, StatusCode::BAD_REQUEST);
    let (_, t) = app.req(&alice, "POST", "/api/me/tokens", Some(json!({ "name": "all" }))).await;
    assert_eq!(bearer(&app, "DELETE", "/api/me", t["token"].as_str().unwrap(), Some(json!({ "confirm": "alice" }))).await, StatusCode::FORBIDDEN);
    let (s, _, cookie) = app.call(&alice, "DELETE", "/api/me", Some(json!({ "confirm": "alice" })), Some(ORIGIN)).await;
    assert_eq!((s, cookie.as_deref()), (StatusCode::OK, Some("")), "deleted and signed out");
    assert!(app.req(&alice, "GET", "/api/me", None).await.1["user"].is_null());
    assert_eq!(app.req(&bob, "GET", "/api/repos/alice/code", None).await.0, StatusCode::NOT_FOUND);
    let d = app.state.cfg.data_dir.clone();
    for p in [format!("repos/{}.git", ids[0]), format!("wikis/{}.git", ids[0]), format!("trash/{}.git", ids[1]), format!("snippets/{snip}.git")] {
        assert!(!d.join(&p).exists(), "{p} should be gone");
    }
    assert_eq!(std::fs::read_dir(d.join("exports")).map(|r| r.count()).unwrap_or(0), 0, "the export file goes too");
    assert_eq!(audit_count(&app, "user_deleted").await, 1);
}

// ---------------------------------------------------------------- webhooks

type Seen = std::sync::Arc<std::sync::Mutex<Vec<(std::collections::HashMap<String, String>, String)>>>;

/// A local HTTP server that records each request (lower-cased headers, body)
/// and answers with the status in `status` (302 points somewhere else).
struct Receiver {
    url: String,
    seen: Seen,
    status: std::sync::Arc<std::sync::atomic::AtomicU16>,
}

impl Receiver {
    fn new() -> Receiver {
        use std::io::{BufRead, Read, Write};
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://127.0.0.1:{}/hook", l.local_addr().unwrap().port());
        let (seen, status): (Seen, _) = (Default::default(), std::sync::Arc::new(std::sync::atomic::AtomicU16::new(200)));
        let (s2, st2) = (seen.clone(), status.clone());
        std::thread::spawn(move || {
            for conn in l.incoming().flatten() {
                let mut r = std::io::BufReader::new(conn.try_clone().unwrap());
                let (mut line, mut head) = (String::new(), std::collections::HashMap::new());
                r.read_line(&mut line).unwrap();
                loop {
                    line.clear();
                    r.read_line(&mut line).unwrap();
                    let Some((k, v)) = line.trim_end().split_once(':') else { break };
                    head.insert(k.to_ascii_lowercase(), v.trim().to_string());
                }
                let mut body = vec![0; head.get("content-length").and_then(|v| v.parse().ok()).unwrap_or(0)];
                r.read_exact(&mut body).unwrap();
                s2.lock().unwrap().push((head, String::from_utf8_lossy(&body).into_owned()));
                let code = st2.load(std::sync::atomic::Ordering::SeqCst);
                let to = if code == 302 { "Location: http://127.0.0.1:9/elsewhere\r\n" } else { "" };
                let _ = write!(&conn, "HTTP/1.1 {code} Hi\r\n{to}Content-Length: 2\r\nConnection: close\r\n\r\nok");
            }
        });
        Receiver { url, seen, status }
    }

    fn events(&self) -> Vec<String> {
        self.seen.lock().unwrap().iter().map(|(h, _)| h["x-gitter-event"].clone()).collect()
    }

    fn last(&self) -> (std::collections::HashMap<String, String>, Value) {
        let (h, b) = self.seen.lock().unwrap().last().cloned().unwrap();
        (h, serde_json::from_str(&b).unwrap())
    }
}

/// (status, attempts, seconds until the next try) of the newest delivery.
async fn newest_delivery(app: &App) -> (String, i64, Option<i64>) {
    app.state
        .db
        .call(|c| Ok(c.query_row("SELECT status, attempts, next_at - strftime('%s','now') FROM webhook_deliveries ORDER BY id DESC LIMIT 1", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?))
        .await
        .unwrap()
}

async fn make_due(app: &App) {
    app.state.db.call(|c| Ok(c.execute("UPDATE webhook_deliveries SET next_at = 0 WHERE next_at IS NOT NULL", [])?)).await.unwrap();
}

#[tokio::test]
async fn webhook_deliveries_are_signed_logged_retried_and_redelivered() {
    use hmac::Mac;
    let app = App::new();
    let admin = app.signup("root1").await;
    app.req(&admin, "POST", "/api/repos", Some(json!({ "name": "r", "readme": true }))).await;
    let rx = Receiver::new();
    let hook = json!({ "url": rx.url, "secret": "s3cret", "events": ["issues", "push", "pull_request"], "allow_private": true });
    let (s, v) = app.req(&admin, "POST", "/api/repos/root1/r/hooks", Some(hook)).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let id = v["id"].as_i64().unwrap();

    // A ping on create, signed with the secret.
    gitter::webhooks::tick(&app.state).await;
    assert_eq!(rx.events(), ["ping"]);
    let (h, body) = rx.seen.lock().unwrap()[0].clone();
    let mut mac = hmac::Hmac::<sha2::Sha256>::new_from_slice(b"s3cret").unwrap();
    mac.update(body.as_bytes());
    assert_eq!(h["x-gitter-signature-256"], format!("sha256={}", hex::encode(mac.finalize().into_bytes())));
    assert_eq!(h["x-gitter-delivery"].len(), 36, "a uuid");
    assert_eq!(h["content-type"], "application/json");
    let ping: Value = serde_json::from_str(&body).unwrap();
    assert_eq!((ping["hook"]["id"].as_i64(), ping["repository"]["full_name"].as_str()), (Some(id), Some("root1/r")));
    assert_eq!(ping["sender"]["username"], "root1");

    // Issues, a comment it didn't ask for, a web commit (push), a pull request and its merge.
    app.req(&admin, "POST", "/api/repos/root1/r/issues", Some(json!({ "title": "Bug", "body": "It breaks" }))).await;
    app.req(&admin, "POST", "/api/repos/root1/r/issues/1/comments", Some(json!({ "body": "On it", "state": "closed" }))).await;
    let change = json!({ "branch": "main", "new_branch": "fix", "message": "Fix the bug", "changes": [{ "path": "fix.txt", "content": "fixed" }] });
    assert_eq!(app.req(&admin, "POST", "/api/repos/root1/r/contents", Some(change)).await.0, StatusCode::OK);
    let (s, v) = app.req(&admin, "POST", "/api/repos/root1/r/pulls", Some(json!({ "title": "Fix", "head": "fix", "base": "main" }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    gitter::webhooks::tick(&app.state).await;
    // Deliveries go out in parallel, so look them up rather than rely on their order.
    let got = |event: &str, action: &str| -> Value {
        let seen = rx.seen.lock().unwrap();
        let all = seen.iter().filter(|(h, _)| h["x-gitter-event"] == event).map(|(_, b)| serde_json::from_str::<Value>(b).unwrap());
        all.filter(|p| action.is_empty() || p["action"] == action).last().unwrap_or_else(|| panic!("no {event} {action}"))
    };
    let mut events = rx.events();
    events.sort();
    assert_eq!(events, ["issues", "issues", "ping", "pull_request", "push"], "no issue_comment: it wasn't asked for");
    let opened = got("issues", "opened");
    assert_eq!((opened["issue"]["title"].as_str(), opened["issue"]["body"].as_str()), (Some("Bug"), Some("It breaks")));
    assert_eq!(got("issues", "closed")["issue"]["state"], "closed");
    let push = got("push", "");
    assert_eq!((push["ref"].as_str(), push["created"].as_bool()), (Some("refs/heads/fix"), Some(true)));
    assert_eq!(push["head_commit"]["message"], "Fix the bug");
    assert_eq!(got("pull_request", "opened")["pull_request"]["head"], "fix");
    let (s, v) = app.req(&admin, "POST", "/api/repos/root1/r/pulls/2/merge", Some(json!({ "method": "merge" }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    gitter::webhooks::tick(&app.state).await;
    assert_eq!(rx.events().len(), 7);
    assert_eq!(got("pull_request", "merged")["pull_request"]["merged"], true);
    assert_eq!(got("push", "")["ref"], "refs/heads/main");

    // The log: newest first, with what was sent and what came back.
    let (s, v) = app.req(&admin, "GET", &format!("/api/repos/root1/r/hooks/{id}"), None).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let log = v["deliveries"].as_array().unwrap();
    assert_eq!(log.len(), 7);
    assert!(log.iter().all(|d| d["status"] == "ok" && d["response_code"] == 200), "{log:?}");
    let ping_id = log.last().unwrap()["id"].as_i64().unwrap();
    let (_, d) = app.req(&admin, "GET", &format!("/api/repos/root1/r/hooks/{id}/deliveries/{ping_id}"), None).await;
    assert_eq!(d["request_headers"]["X-Gitter-Event"], "ping");
    assert_eq!(d["payload"].as_str().unwrap(), body);
    assert_eq!(d["response_body"], "ok");
    assert!(v["hook"].get("secret").is_none(), "the secret is never shown");

    // A failing receiver: retried after 1 m, 5 m, 30 m, 2 h and 6 h, then given up.
    rx.status.store(500, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(app.req(&admin, "POST", &format!("/api/repos/root1/r/hooks/{id}/ping"), None).await.0, StatusCode::OK);
    for (i, wait) in [60, 300, 1800, 7200, 21600].into_iter().enumerate() {
        gitter::webhooks::tick(&app.state).await;
        let (status, attempts, next) = newest_delivery(&app).await;
        assert_eq!((status.as_str(), attempts), ("pending", i as i64 + 1));
        assert!(next.is_some_and(|n| (wait - 5..=wait).contains(&n)), "retry {i} after {next:?}, want {wait}");
        gitter::webhooks::tick(&app.state).await;
        assert_eq!(newest_delivery(&app).await.1, i as i64 + 1, "not before it's due");
        make_due(&app).await;
    }
    gitter::webhooks::tick(&app.state).await;
    assert_eq!(newest_delivery(&app).await, ("failed".into(), 6, None));
    let guids: Vec<String> = rx.seen.lock().unwrap()[7..].iter().map(|(h, _)| h["x-gitter-delivery"].clone()).collect();
    assert!(guids.len() == 6 && guids.iter().all(|g| *g == guids[0]), "retries keep the delivery id: {guids:?}");

    // Redeliver once the receiver works again.
    rx.status.store(200, std::sync::atomic::Ordering::SeqCst);
    let (_, v) = app.req(&admin, "GET", &format!("/api/repos/root1/r/hooks/{id}"), None).await;
    let failed = v["deliveries"][0]["id"].as_i64().unwrap();
    let (s, v) = app.req(&admin, "POST", &format!("/api/repos/root1/r/hooks/{id}/deliveries/{failed}/redeliver"), None).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    gitter::webhooks::tick(&app.state).await;
    assert_eq!(newest_delivery(&app).await.0, "ok");
    assert_eq!(rx.last().0["x-gitter-delivery"], guids[0]);

    // Deliveries live in the database: a restarted server sends what was queued.
    app.req(&admin, "POST", "/api/repos/root1/r/issues", Some(json!({ "title": "After restart" }))).await;
    let restarted = gitter::AppState::new((*app.state.cfg).clone(), app.state.db.clone());
    gitter::webhooks::tick(&restarted).await;
    assert_eq!(rx.last().1["issue"]["title"], "After restart");

    // Turned off: nothing is sent; deleted: its log goes with it.
    let (s, v) = app.req(&admin, "PATCH", &format!("/api/repos/root1/r/hooks/{id}"), Some(json!({ "active": false }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    app.req(&admin, "POST", "/api/repos/root1/r/issues", Some(json!({ "title": "Quiet" }))).await;
    let before = rx.seen.lock().unwrap().len();
    gitter::webhooks::tick(&app.state).await;
    assert_eq!(rx.seen.lock().unwrap().len(), before);
    assert_eq!(app.req(&admin, "DELETE", &format!("/api/repos/root1/r/hooks/{id}"), None).await.0, StatusCode::OK);
    let left: i64 = app.state.db.call(|c| Ok(c.query_row("SELECT count(*) FROM webhook_deliveries", [], |r| r.get(0))?)).await.unwrap();
    assert_eq!(left, 0);
}

#[tokio::test]
async fn webhook_private_addresses_are_refused_unless_an_admin_allows_them() {
    let app = App::new();
    let admin = app.signup("root1").await;
    let bob = app.signup("bob").await;
    app.req(&bob, "POST", "/api/repos", Some(json!({ "name": "r", "readme": true }))).await;
    let rx = Receiver::new();
    const SECRET: &str = "plain-secret-value";
    let create = |url: &str, private: bool| json!({ "url": url, "secret": SECRET, "events": ["*"], "allow_private": private });

    // Bad URLs, and the admin-only switch.
    for bad in ["ftp://example.com/", "https://u:p@example.com/", "not a url", "http:///x"] {
        let (s, v) = app.req(&bob, "POST", "/api/repos/bob/r/hooks", Some(create(bad, false))).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{bad}: {v}");
    }
    let (s, v) = app.req(&bob, "POST", "/api/repos/bob/r/hooks", Some(create(&rx.url, true))).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{v}");
    assert_eq!(app.req(&bob, "POST", "/api/repos/bob/r/hooks", Some(json!({ "url": rx.url, "events": ["*"] }))).await.0, StatusCode::BAD_REQUEST, "a secret is required");
    assert_eq!(app.req(&bob, "POST", "/api/repos/bob/r/hooks", Some(json!({ "url": rx.url, "secret": "k", "events": ["nope"] }))).await.0, StatusCode::BAD_REQUEST);

    // By default 127.0.0.1 and localhost are never contacted.
    let (s, v) = app.req(&bob, "POST", "/api/repos/bob/r/hooks", Some(create(&rx.url, false))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let id = v["id"].as_i64().unwrap();
    let local = rx.url.replace("127.0.0.1", "localhost");
    let (_, v) = app.req(&bob, "POST", "/api/repos/bob/r/hooks", Some(create(&local, false))).await;
    let id2 = v["id"].as_i64().unwrap();
    gitter::webhooks::tick(&app.state).await;
    assert!(rx.events().is_empty(), "nothing reached the receiver");
    for h in [id, id2] {
        let (_, v) = app.req(&bob, "GET", &format!("/api/repos/bob/r/hooks/{h}"), None).await;
        let d = &v["deliveries"][0];
        assert_eq!(d["status"], "pending", "{d}");
        assert!(d["error"].as_str().unwrap().contains("private or reserved address"), "{d}");
        assert!(!d["error"].as_str().unwrap().contains("127.0.0.1"), "no address in the message: {d}");
        assert!(d["response_code"].is_null());
    }
    // The secret is sealed at rest.
    let raw: Vec<u8> = app.state.db.call(move |c| Ok(c.query_row("SELECT secret FROM webhooks WHERE id = ?1", [id], |r| r.get(0))?)).await.unwrap();
    assert!(!raw.windows(SECRET.len()).any(|w| w == SECRET.as_bytes()), "sealed");
    assert_eq!(gitter::security::open(&app.state.key, &raw).as_deref(), Some(SECRET.as_bytes()));

    // Others can't see or change the hooks, nor can limited tokens (URLs can hold credentials);
    // only the admin can allow private addresses.
    let tok = app.req(&bob, "POST", "/api/me/tokens", Some(json!({ "name": "w", "scope": "write", "repo": "bob/r" }))).await.1["token"].as_str().unwrap().to_string();
    assert_eq!(bearer(&app, "GET", "/api/repos/bob/r/hooks", &tok, None).await, StatusCode::FORBIDDEN);
    assert_eq!(bearer(&app, "POST", &format!("/api/repos/bob/r/hooks/{id}/ping"), &tok, None).await, StatusCode::FORBIDDEN);
    let carol = app.signup("carol").await;
    assert_eq!(app.req(&carol, "GET", "/api/repos/bob/r/hooks", None).await.0, StatusCode::FORBIDDEN);
    assert_eq!(app.req(&carol, "PATCH", &format!("/api/repos/bob/r/hooks/{id}"), Some(json!({ "active": false }))).await.0, StatusCode::FORBIDDEN);
    // Not even with write access: hooks are for the repository's admins.
    app.req(&bob, "PUT", "/api/repos/bob/r/collaborators/carol", Some(json!({ "role": "write" }))).await;
    assert_eq!(app.req(&carol, "GET", "/api/repos/bob/r/hooks", None).await.0, StatusCode::FORBIDDEN);
    assert_eq!(app.req(&carol, "GET", &format!("/api/repos/bob/r/hooks/{id}"), None).await.0, StatusCode::FORBIDDEN);
    assert_eq!(app.req(&bob, "PATCH", &format!("/api/repos/bob/r/hooks/{id}"), Some(json!({ "allow_private": true }))).await.0, StatusCode::FORBIDDEN);
    let (s, v) = app.req(&admin, "PATCH", &format!("/api/repos/bob/r/hooks/{id}"), Some(json!({ "allow_private": true }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    make_due(&app).await;
    gitter::webhooks::tick(&app.state).await;
    assert_eq!(rx.events(), ["ping"], "allowed now (the localhost hook still isn't)");

    // A redirect is a reply, not followed.
    rx.status.store(302, std::sync::atomic::Ordering::SeqCst);
    app.req(&bob, "POST", &format!("/api/repos/bob/r/hooks/{id}/ping"), None).await;
    gitter::webhooks::tick(&app.state).await;
    let (_, v) = app.req(&bob, "GET", &format!("/api/repos/bob/r/hooks/{id}"), None).await;
    assert_eq!((v["deliveries"][0]["response_code"].as_i64(), v["deliveries"][0]["status"].as_str()), (Some(302), Some("pending")));

    // The owner changing the URL drops the admin's permission.
    let moved = format!("{}/v2", rx.url);
    let (s, v) = app.req(&bob, "PATCH", &format!("/api/repos/bob/r/hooks/{id}"), Some(json!({ "url": moved }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let (_, v) = app.req(&bob, "GET", &format!("/api/repos/bob/r/hooks/{id}"), None).await;
    assert_eq!((v["hook"]["url"].as_str(), v["hook"]["allow_private"].as_bool()), (Some(moved.as_str()), Some(false)));
}

#[tokio::test]
async fn webhook_ci_runs_are_announced_once_when_they_finish() {
    let app = App::new();
    let admin = app.signup("root1").await;
    app.req(&admin, "POST", "/api/repos", Some(json!({ "name": "r", "readme": true }))).await;
    let rx = Receiver::new();
    let hook = json!({ "url": rx.url, "secret": "k", "events": ["ci_run"], "allow_private": true });
    assert_eq!(app.req(&admin, "POST", "/api/repos/root1/r/hooks", Some(hook)).await.0, StatusCode::OK);
    gitter::webhooks::tick(&app.state).await;
    let commit = |path: &str, content: &str| json!({ "branch": "main", "message": "CI", "changes": [{ "path": path, "content": content }] });
    // A broken pipeline fails at once; a good one is cancelled.
    app.req(&admin, "POST", "/api/repos/root1/r/contents", Some(commit(".gitter/ci.yml", "jobs:\n  a:\n    stepz: [x]\n"))).await;
    app.req(&admin, "POST", "/api/repos/root1/r/contents", Some(commit(".gitter/ci.yml", "jobs:\n  a:\n    steps: [echo]\n"))).await;
    gitter::webhooks::tick(&app.state).await;
    assert_eq!(rx.events(), ["ping", "ci_run"], "the queued run isn't finished");
    let (_, p) = rx.last();
    assert_eq!((p["action"].as_str(), p["run"]["number"].as_i64(), p["run"]["status"].as_str()), (Some("completed"), Some(1), Some("failure")));
    assert_eq!(p["sender"]["username"], "root1");
    assert_eq!(app.req(&admin, "POST", "/api/repos/root1/r/ci/runs/2/cancel", None).await.0, StatusCode::OK);
    gitter::webhooks::tick(&app.state).await;
    gitter::webhooks::tick(&app.state).await;
    assert_eq!(rx.events(), ["ping", "ci_run", "ci_run"]);
    assert_eq!(rx.last().1["run"]["status"], "cancelled");
}

/// A raw-body request as `c` (release asset uploads and downloads): status, headers, body.
async fn raw_as(app: &App, c: &Client, method: &str, path: &str, ctype: &str, body: Vec<u8>) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let mut b = Request::builder().method(method).uri(path).header("host", "test.local").header("origin", ORIGIN);
    if !ctype.is_empty() {
        b = b.header("content-type", ctype);
    }
    if let Some(cookie) = &c.0 {
        b = b.header("cookie", format!("gitter_session={cookie}"));
    }
    let resp = app.router.clone().oneshot(b.body(Body::from(body)).unwrap()).await.unwrap();
    let (s, h) = (resp.status(), resp.headers().clone());
    (s, h, resp.into_body().collect().await.unwrap().to_bytes().to_vec())
}

async fn repo_id(app: &App, name: &'static str) -> i64 {
    app.state.db.call(move |c| Ok(c.query_row("SELECT id FROM repos WHERE name = ?1", [name], |r| r.get(0))?)).await.unwrap()
}

#[tokio::test]
async fn releases_from_tags_drafts_latest_and_access() {
    let app = App::new();
    let alice = app.signup("alice").await;
    let bob = app.signup("bob").await;
    let anon = Client::default();
    for (n, private) in [("app", false), ("priv", true)] {
        app.req(&alice, "POST", "/api/repos", Some(json!({ "name": n, "private": private, "readme": true }))).await;
    }
    let base = "/api/repos/alice/app/releases";
    let path = app.state.git.repo_path(repo_id(&app, "app").await);
    let (_, br) = app.req(&alice, "GET", "/api/repos/alice/app/branches", None).await;
    let main = br["branches"][0]["sha"].as_str().unwrap().to_string();

    // An existing tag is used as it is: the target only matters for a new one.
    app.state.git.ok(&path, &["tag", "v0.9", "main"]).await.unwrap();
    let (s, v) = app.req(&alice, "POST", base, Some(json!({ "tag": "v0.9", "target": "nope" }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    // A new tag at a branch: an annotated tag at its head.
    let (s, v) = app.req(&alice, "POST", base, Some(json!({ "tag": "v1.0", "target": "main", "title": "First", "notes": "Hello **world**" }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let v1 = v["id"].as_i64().unwrap();
    let kind = app.state.git.ok(&path, &["cat-file", "-t", "refs/tags/v1.0"]).await.unwrap();
    assert_eq!(String::from_utf8_lossy(&kind).trim(), "tag");
    let (_, r) = app.req(&anon, "GET", &format!("{base}/{v1}"), None).await;
    assert_eq!((r["tag"].as_str(), r["sha"].as_str(), r["title"].as_str(), r["author"].as_str()), (Some("v1.0"), Some(main.as_str()), Some("First"), Some("alice")), "{r}");

    // One release per tag; bad names, unknown targets and non-writers are refused.
    assert_eq!(app.req(&alice, "POST", base, Some(json!({ "tag": "v1.0" }))).await.0, StatusCode::CONFLICT);
    for bad in [json!({ "tag": "../x" }), json!({ "tag": "v9", "target": "nope" }), json!({ "tag": "" }), json!({ "tag": "v9", "notes": "x".repeat(70_000) })] {
        assert_eq!(app.req(&alice, "POST", base, Some(bad.clone())).await.0, StatusCode::BAD_REQUEST, "{bad}");
    }
    assert_eq!(app.req(&bob, "POST", base, Some(json!({ "tag": "v2" }))).await.0, StatusCode::FORBIDDEN);
    assert_eq!(app.req(&anon, "POST", base, Some(json!({ "tag": "v2" }))).await.0, StatusCode::UNAUTHORIZED);

    // Drafts are for writers only, listed first; prereleases are never the latest.
    let (_, v) = app.req(&alice, "POST", base, Some(json!({ "tag": "v2.0", "title": "Two", "draft": true }))).await;
    let v2 = v["id"].as_i64().unwrap();
    let tag_names = || async { names(&app.req(&Client::default(), "GET", "/api/repos/alice/app/tags", None).await.1["tags"], "name") };
    assert!(!tag_names().await.contains(&"v2.0".to_string()), "a draft's new tag waits until it's published");
    assert_eq!(app.req(&alice, "POST", base, Some(json!({ "tag": "v3", "target": "nope", "draft": true }))).await.0, StatusCode::BAD_REQUEST);
    app.req(&alice, "POST", base, Some(json!({ "tag": "v1.1-rc", "prerelease": true }))).await;
    let tags = |v: &Value| names(&v["releases"], "tag");
    assert_eq!(tags(&app.req(&anon, "GET", base, None).await.1), ["v1.1-rc", "v1.0", "v0.9"]);
    assert_eq!(tags(&app.req(&alice, "GET", base, None).await.1), ["v2.0", "v1.1-rc", "v1.0", "v0.9"]);
    assert_eq!(app.req(&bob, "GET", &format!("{base}/{v2}"), None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(app.req(&anon, "GET", &format!("{base}/latest"), None).await.1["tag"], "v1.0");
    let (_, repo) = app.req(&anon, "GET", "/api/repos/alice/app", None).await;
    assert_eq!((repo["latest_release"]["tag"].as_str(), repo["releases"].as_i64()), (Some("v1.0"), Some(3)), "{repo}");

    // Publishing a draft makes it the latest; edits need write access.
    let (s, v) = app.req(&alice, "PATCH", &format!("{base}/{v2}"), Some(json!({ "draft": false, "notes": "Now out" }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let (_, r) = app.req(&anon, "GET", &format!("{base}/{v2}"), None).await;
    assert_eq!((r["draft"].as_bool(), r["notes"].as_str(), r["title"].as_str()), (Some(false), Some("Now out"), Some("Two")), "{r}");
    assert!(r["published_at"].as_i64().is_some(), "{r}");
    assert_eq!(r["sha"], main.as_str(), "publishing made the tag at the default branch: {r}");
    assert!(tag_names().await.contains(&"v2.0".to_string()));
    assert_eq!(app.req(&anon, "GET", &format!("{base}/latest"), None).await.1["tag"], "v2.0");
    assert_eq!(app.req(&bob, "PATCH", &format!("{base}/{v1}"), Some(json!({ "title": "pwned" }))).await.0, StatusCode::FORBIDDEN);

    // A draft of an existing tag that's deleted later isn't published at a guessed commit.
    app.state.git.ok(&path, &["tag", "v4", "main"]).await.unwrap();
    let (_, v) = app.req(&alice, "POST", base, Some(json!({ "tag": "v4", "draft": true }))).await;
    let v4 = v["id"].as_i64().unwrap();
    app.state.git.ok(&path, &["tag", "-d", "v4"]).await.unwrap();
    assert_eq!(app.req(&alice, "PATCH", &format!("{base}/{v4}"), Some(json!({ "draft": false }))).await.0, StatusCode::CONFLICT);

    // Private repositories keep their releases private.
    app.req(&alice, "POST", "/api/repos/alice/priv/releases", Some(json!({ "tag": "v1" }))).await;
    assert_eq!(app.req(&bob, "GET", "/api/repos/alice/priv/releases", None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(app.req(&anon, "GET", "/api/repos/alice/priv/releases/latest", None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(tags(&app.req(&alice, "GET", "/api/repos/alice/priv/releases", None).await.1), ["v1"]);

    // Deleting a release keeps its tag.
    assert_eq!(app.req(&bob, "DELETE", &format!("{base}/{v2}"), None).await.0, StatusCode::FORBIDDEN);
    assert_eq!(app.req(&alice, "DELETE", &format!("{base}/{v2}"), None).await.0, StatusCode::OK);
    assert_eq!(app.req(&anon, "GET", &format!("{base}/{v2}"), None).await.0, StatusCode::NOT_FOUND);
    let (_, t) = app.req(&anon, "GET", "/api/repos/alice/app/tags", None).await;
    assert!(names(&t["tags"], "name").contains(&"v2.0".to_string()), "{t}");
    assert_eq!(audit_count(&app, "release_deleted").await, 1);
}

#[tokio::test]
async fn releases_assets_stream_with_quota_counts_and_cleanup() {
    use sha2::{Digest, Sha256};
    let app = App::new();
    let admin = app.signup("root1").await;
    let alice = app.signup("alice").await;
    let bob = app.signup("bob").await;
    let anon = Client::default();
    for (n, private) in [("app", false), ("priv", true)] {
        app.req(&alice, "POST", "/api/repos", Some(json!({ "name": n, "private": private, "readme": true }))).await;
    }
    let release = |repo: &'static str, body: Value| {
        let (app, alice) = (&app, alice.clone());
        async move { app.req(&alice, "POST", &format!("/api/repos/alice/{repo}/releases"), Some(body)).await.1["id"].as_i64().unwrap() }
    };
    let upload = |url: String, name: &str, body: Vec<u8>| {
        let (app, alice, url) = (&app, alice.clone(), format!("{url}?name={name}"));
        async move {
            let (s, _, b) = raw_as(app, &alice, "POST", &url, "", body).await;
            (s, serde_json::from_slice::<Value>(&b).unwrap_or(Value::Null))
        }
    };
    let data_dir = app.state.cfg.data_dir.clone();
    let id = release("app", json!({ "tag": "v1" })).await;
    let assets = format!("/api/repos/alice/app/releases/{id}/assets");
    let data = b"#!/bin/sh\necho hi\n".repeat(100);

    // Uploads stream in; the name is cut down to a safe file name.
    let (s, _, b) = raw_as(&app, &alice, "POST", &format!("{assets}?name=dist%2F..%2Fmy%20tool%22.sh"), "text/x-sh", data.clone()).await;
    let a: Value = serde_json::from_slice(&b).unwrap_or(Value::Null);
    assert_eq!(s, StatusCode::OK, "{a}");
    assert_eq!((a["name"].as_str(), a["size"].as_u64(), a["content_type"].as_str()), (Some("my.tool..sh"), Some(data.len() as u64), Some("text/x-sh")), "{a}");
    assert_eq!(a["sha256"], hex::encode(Sha256::digest(&data)));
    let aid = a["id"].as_i64().unwrap();
    let file = data_dir.join(format!("releases/{id}/{aid}"));
    assert!(file.is_file());
    assert_eq!(upload(assets.clone(), "my.tool..sh", data.clone()).await.0, StatusCode::CONFLICT);
    assert_eq!(upload(assets.clone(), "", data.clone()).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(raw_as(&app, &bob, "POST", &format!("{assets}?name=x"), "", data.clone()).await.0, StatusCode::FORBIDDEN);
    assert_eq!(raw_as(&app, &anon, "POST", &format!("{assets}?name=x"), "", data.clone()).await.0, StatusCode::UNAUTHORIZED);

    // Anyone who can read the repo downloads it as an attachment, and each download counts.
    let url = format!("{assets}/{aid}");
    let (s, h, got) = raw_as(&app, &anon, "GET", &url, "", vec![]).await;
    assert_eq!((s, got == data), (StatusCode::OK, true));
    assert_eq!(h["content-disposition"], "attachment; filename=\"my.tool..sh\"");
    assert_eq!(h["content-type"], "application/octet-stream");
    raw_as(&app, &bob, "GET", &url, "", vec![]).await;
    let (_, r) = app.req(&anon, "GET", &format!("/api/repos/alice/app/releases/{id}"), None).await;
    assert_eq!(r["assets"][0]["downloads"], 2, "{r}");
    // Downloads aren't gzipped, so browsers know their size.
    let req = Request::get(&url).header("host", "test.local").header("accept-encoding", "gzip").body(Body::empty()).unwrap();
    let resp = app.router.clone().oneshot(req).await.unwrap();
    assert!(resp.headers().get("content-encoding").is_none() && resp.headers()["content-length"] == data.len().to_string().as_str());
    // Files past the API's 40 MB body cap stream in too.
    let (s, big) = upload(assets.clone(), "big.bin", vec![0u8; 41 << 20]).await;
    assert_eq!(s, StatusCode::OK, "{big}");
    assert_eq!(app.req(&alice, "DELETE", &format!("{assets}/{}", big["id"]), None).await.0, StatusCode::OK);

    // Private repositories' and drafts' assets: only for those who may see them, never cached.
    let pid = release("priv", json!({ "tag": "v1" })).await;
    let (_, pa) = upload(format!("/api/repos/alice/priv/releases/{pid}/assets"), "secret.bin", b"secret".to_vec()).await;
    let purl = format!("/api/repos/alice/priv/releases/{pid}/assets/{}", pa["id"]);
    let did = release("app", json!({ "tag": "v2", "draft": true })).await;
    let (_, da) = upload(format!("/api/repos/alice/app/releases/{did}/assets"), "next.bin", b"draft".to_vec()).await;
    let durl = format!("/api/repos/alice/app/releases/{did}/assets/{}", da["id"]);
    for u in [&purl, &durl] {
        assert_eq!(raw_as(&app, &anon, "GET", u, "", vec![]).await.0, StatusCode::NOT_FOUND, "{u}");
        assert_eq!(raw_as(&app, &bob, "GET", u, "", vec![]).await.0, StatusCode::NOT_FOUND, "{u}");
        let (s, h, _) = raw_as(&app, &alice, "GET", u, "", vec![]).await;
        assert_eq!(s, StatusCode::OK, "{u}");
        assert!(h["cache-control"].to_str().unwrap().contains("no-store"), "{u}");
    }
    // An asset id doesn't open through another repository's or release's URL.
    assert_eq!(raw_as(&app, &alice, "GET", &format!("{assets}/{}", pa["id"]), "", vec![]).await.0, StatusCode::NOT_FOUND);
    assert_eq!(raw_as(&app, &alice, "GET", &format!("{assets}/{}", da["id"]), "", vec![]).await.0, StatusCode::NOT_FOUND);

    // A release holds at most 1000 files.
    let many = release("app", json!({ "tag": "many" })).await;
    app.state
        .db
        .call(move |c| {
            for i in 0..1000 {
                c.execute("INSERT INTO release_assets(release_id, name, size, created_at) VALUES (?1, ?2, 0, 0)", rusqlite::params![many, format!("f{i}")])?;
            }
            Ok(())
        })
        .await
        .unwrap();
    assert_eq!(upload(format!("/api/repos/alice/app/releases/{many}/assets"), "one-more", b"x".to_vec()).await.0, StatusCode::BAD_REQUEST);

    // Assets count toward the owner's storage, in total and per file; refused uploads leave nothing.
    let mut s = app.state.settings.get();
    s.max_storage_mb = 1;
    app.state.settings.save(&app.state.db, s).await.unwrap();
    app.state.db.call(|c| Ok(c.execute("UPDATE repos SET size_bytes = 0", [])?)).await.unwrap();
    let (_, me) = app.req(&alice, "GET", "/api/me", None).await;
    assert_eq!(me["storage"]["used"], data.len() + 6 + 5, "{me}");
    let big = vec![1u8; 700 * 1024];
    assert_eq!(upload(assets.clone(), "a.bin", big.clone()).await.0, StatusCode::OK);
    assert_eq!(upload(assets.clone(), "b.bin", big.clone()).await.0, StatusCode::PAYLOAD_TOO_LARGE);
    let mut s = app.state.settings.get();
    (s.max_storage_mb, s.max_lfs_mb) = (0, 1);
    app.state.settings.save(&app.state.db, s).await.unwrap();
    assert_eq!(upload(assets.clone(), "huge.bin", vec![2u8; (1 << 20) + 1]).await.0, StatusCode::PAYLOAD_TOO_LARGE);
    // Parallel uploads can't overshoot the quota together.
    let mut s = app.state.settings.get();
    s.max_storage_mb = 2;
    app.state.settings.save(&app.state.db, s).await.unwrap();
    let (x, y) = tokio::join!(upload(assets.clone(), "c.bin", big.clone()), upload(assets.clone(), "d.bin", big.clone()));
    let mut got = [x.0, y.0];
    got.sort();
    assert_eq!(got, [StatusCode::OK, StatusCode::PAYLOAD_TOO_LARGE]);
    let (_, me) = app.req(&alice, "GET", "/api/me", None).await;
    assert!(me["storage"]["used"].as_u64().unwrap() <= 2 << 20, "{me}");
    assert_eq!(std::fs::read_dir(data_dir.join("releases/tmp")).unwrap().count(), 0, "no temp files left");

    // Deleting an asset, a release, the repository or the account removes the files.
    assert_eq!(app.req(&bob, "DELETE", &url, None).await.0, StatusCode::FORBIDDEN);
    assert_eq!(app.req(&alice, "DELETE", &url, None).await.0, StatusCode::OK);
    assert!(!file.exists());
    assert_eq!(raw_as(&app, &anon, "GET", &url, "", vec![]).await.0, StatusCode::NOT_FOUND);
    assert_eq!(app.req(&alice, "DELETE", &format!("/api/repos/alice/app/releases/{id}"), None).await.0, StatusCode::OK);
    assert!(!data_dir.join(format!("releases/{id}")).exists());
    assert!(data_dir.join(format!("releases/{pid}")).is_dir());
    assert_eq!(app.req(&alice, "DELETE", "/api/repos/alice/priv", Some(json!({ "confirm": "alice/priv" }))).await.0, StatusCode::OK);
    assert!(data_dir.join(format!("releases/{pid}")).is_dir(), "kept while the repo is in the trash");
    let priv_id: i64 = app.state.db.call(|c| Ok(c.query_row("SELECT id FROM repos WHERE deleted_name = 'priv'", [], |r| r.get(0))?)).await.unwrap();
    assert_eq!(app.req(&admin, "DELETE", &format!("/api/admin/trash/{priv_id}"), None).await.0, StatusCode::OK);
    assert!(!data_dir.join(format!("releases/{pid}")).exists(), "purging the trash removes them");
    assert!(data_dir.join(format!("releases/{did}")).is_dir());
    let (s, v) = app.req(&admin, "DELETE", "/api/admin/users/alice", Some(json!({ "confirm": "alice" }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert!(!data_dir.join(format!("releases/{did}")).exists());
}

#[tokio::test]
async fn totp_secrets_are_sealed_at_rest() {
    use gitter::security::{seal_totp_secrets, step_now, totp_code, use_code};
    let app = App::new();
    let alice = app.signup("alice").await;
    let (_, setup) = app.req(&alice, "POST", "/api/me/2fa/setup", None).await;
    let secret = setup["secret"].as_str().unwrap().to_string();
    let (s, _) = app.req(&alice, "POST", "/api/me/2fa/enable", Some(json!({ "code": totp_code(&secret, step_now()).unwrap() }))).await;
    assert_eq!(s, StatusCode::OK);
    let stored: String = app.state.db.call(|c| Ok(c.query_row("SELECT totp_secret FROM users WHERE username = 'alice'", [], |r| r.get(0))?)).await.unwrap();
    assert!(stored.starts_with("sealed:") && !stored.contains(&secret), "{stored}");
    // A secret stored before sealing still works, and the boot pass seals it.
    let bob = app.signup("bob").await;
    drop(bob);
    let legacy = gitter::security::new_totp_secret();
    let l2 = legacy.clone();
    let bob_id: i64 = app
        .state
        .db
        .call(move |c| {
            c.execute("UPDATE users SET totp_secret = ?1 WHERE username = 'bob'", [&l2])?;
            Ok(c.query_row("SELECT id FROM users WHERE username = 'bob'", [], |r| r.get(0))?)
        })
        .await
        .unwrap();
    let code = totp_code(&legacy, step_now()).unwrap();
    assert!(use_code(&app.state.db, app.state.key.clone(), bob_id, &code).await.unwrap().is_some());
    assert_eq!(seal_totp_secrets(&app.state.db, app.state.key.clone()).await.unwrap(), 1);
    let sealed: String = app.state.db.call(move |c| Ok(c.query_row("SELECT totp_secret FROM users WHERE id = ?1", [bob_id], |r| r.get(0))?)).await.unwrap();
    assert!(sealed.starts_with("sealed:"));
    assert_eq!(gitter::security::open_text(&app.state.key, &sealed).as_deref(), Some(legacy.as_str()));
    assert_eq!(seal_totp_secrets(&app.state.db, app.state.key.clone()).await.unwrap(), 0, "already sealed");
}

/// Push status for `user:password` to `repo` (a minimal receive-pack request).
async fn can_push(app: &App, repo: &str, user: &str) -> StatusCode {
    let req = Request::post(format!("/{repo}.git/git-receive-pack"))
        .header("host", "test.local")
        .header("authorization", basic(user, "correct horse"))
        .header("content-type", "application/x-git-receive-pack-request")
        .body(Body::from("0000"))
        .unwrap();
    status_with(app, req).await
}

#[tokio::test]
async fn collaborators_get_exactly_their_role() {
    let app = App::new();
    let _admin = app.signup("root1").await;
    let alice = app.signup("alice").await;
    let bob = app.signup("bob").await;
    app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "p", "private": true, "readme": true }))).await;
    assert_eq!(app.req(&bob, "GET", "/api/repos/alice/p", None).await.0, StatusCode::NOT_FOUND);
    // Only admins manage access.
    assert_ne!(app.req(&bob, "PUT", "/api/repos/alice/p/collaborators/bob", Some(json!({ "role": "admin" }))).await.0, StatusCode::OK);
    assert_eq!(app.req(&alice, "PUT", "/api/repos/alice/p/collaborators/bob", Some(json!({ "role": "read" }))).await.0, StatusCode::OK);
    let (s, meta) = app.req(&bob, "GET", "/api/repos/alice/p", None).await;
    assert_eq!((s, meta["can_write"].as_bool()), (StatusCode::OK, Some(false)));
    let (_, list) = app.req(&bob, "GET", "/api/explore?q=p", None).await;
    assert!(list.to_string().contains("alice"), "private repos you can read are listed: {list}");
    assert_eq!(can_push(&app, "alice/p", "bob").await, StatusCode::FORBIDDEN);
    // Write: push and manage issues, but no settings.
    app.req(&alice, "PUT", "/api/repos/alice/p/collaborators/bob", Some(json!({ "role": "write" }))).await;
    assert_ne!(can_push(&app, "alice/p", "bob").await, StatusCode::FORBIDDEN);
    assert_eq!(app.req(&bob, "POST", "/api/repos/alice/p/labels", Some(json!({ "name": "bug2", "color": "#ff0000" }))).await.0, StatusCode::OK);
    assert_eq!(app.req(&bob, "PATCH", "/api/repos/alice/p", Some(json!({ "description": "x" }))).await.0, StatusCode::FORBIDDEN);
    assert_eq!(app.req(&bob, "GET", "/api/repos/alice/p/deploy-tokens", None).await.0, StatusCode::FORBIDDEN);
    assert_eq!(app.req(&bob, "PUT", "/api/repos/alice/p/ci/secrets/X", Some(json!({ "value": "y" }))).await.0, StatusCode::FORBIDDEN);
    // Admin: settings too.
    app.req(&alice, "PUT", "/api/repos/alice/p/collaborators/bob", Some(json!({ "role": "admin" }))).await;
    assert_eq!(app.req(&bob, "PATCH", "/api/repos/alice/p", Some(json!({ "description": "by bob" }))).await.0, StatusCode::OK);
    let (_, acc) = app.req(&bob, "GET", "/api/repos/alice/p/collaborators", None).await;
    assert!(acc["access"].to_string().contains("can do everything"), "{acc}");
    assert_eq!(acc["collaborators"][0]["username"], "bob");
    // No token, not even a full-access one, adds collaborators: a leaked token mustn't leave access behind.
    let (_, t) = app.req(&alice, "POST", "/api/me/tokens", Some(json!({ "name": "full" }))).await;
    let t = t["token"].as_str().unwrap().to_string();
    assert_eq!(bearer(&app, "PUT", "/api/repos/alice/p/collaborators/root1", &t, Some(json!({ "role": "admin" }))).await, StatusCode::FORBIDDEN);
    // Removing them revokes access; bad roles and organizations are refused.
    assert_eq!(app.req(&alice, "PUT", "/api/repos/alice/p/collaborators/bob", Some(json!({ "role": "god" }))).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(app.req(&alice, "DELETE", "/api/repos/alice/p/collaborators/bob", None).await.0, StatusCode::OK);
    assert_eq!(app.req(&bob, "GET", "/api/repos/alice/p", None).await.0, StatusCode::NOT_FOUND);
    // A moderator-hidden repo is for its admins only.
    app.req(&alice, "PUT", "/api/repos/alice/p/collaborators/bob", Some(json!({ "role": "write" }))).await;
    app.state.db.call(|c| Ok(c.execute("UPDATE repos SET hidden = 1", [])?)).await.unwrap();
    assert_eq!(app.req(&bob, "GET", "/api/repos/alice/p", None).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn organizations_own_repos_and_share_them_with_members() {
    let app = App::new();
    let _admin = app.signup("root1").await;
    let alice = app.signup("alice").await;
    let bob = app.signup("bob").await;
    let (s, v) = app.req(&alice, "POST", "/api/orgs", Some(json!({ "name": "acme", "display_name": "Acme Inc" }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(app.req(&bob, "POST", "/api/orgs", Some(json!({ "name": "acme" }))).await.0, StatusCode::CONFLICT);
    assert_eq!(app.req(&bob, "POST", "/api/orgs", Some(json!({ "name": "alice" }))).await.0, StatusCode::CONFLICT, "one namespace with users");
    // Repos in the org: by those who'd have write on all of them.
    let (s, v) = app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "x", "owner": "acme", "private": true, "readme": true }))).await;
    assert_eq!((s, v["owner"].as_str()), (StatusCode::OK, Some("acme")), "{v}");
    assert_ne!(app.req(&bob, "POST", "/api/repos", Some(json!({ "name": "y", "owner": "acme" }))).await.0, StatusCode::OK);
    assert_eq!(app.req(&bob, "GET", "/api/repos/acme/x", None).await.0, StatusCode::NOT_FOUND);
    // Members get the base permission (read by default).
    assert_eq!(app.req(&alice, "PUT", "/api/orgs/acme/members/bob", Some(json!({ "role": "member" }))).await.0, StatusCode::OK);
    let (s, meta) = app.req(&bob, "GET", "/api/repos/acme/x", None).await;
    assert_eq!((s, meta["can_write"].as_bool()), (StatusCode::OK, Some(false)));
    assert_eq!(can_push(&app, "acme/x", "bob").await, StatusCode::FORBIDDEN);
    let listed = |v: Value| v["repos"].as_array().map_or(0, Vec::len);
    assert_eq!(listed(app.req(&bob, "GET", "/api/users/acme", None).await.1), 1, "lists follow the base role");
    assert_eq!(listed(app.req(&bob, "GET", "/api/feed", None).await.1), 1, "the dashboard lists repos shared with you");
    assert_eq!(app.req(&alice, "PATCH", "/api/orgs/acme", Some(json!({ "base_role": "write" }))).await.0, StatusCode::OK);
    assert_ne!(can_push(&app, "acme/x", "bob").await, StatusCode::FORBIDDEN);
    assert_eq!(app.req(&bob, "PATCH", "/api/repos/acme/x", Some(json!({ "description": "x" }))).await.0, StatusCode::FORBIDDEN, "members aren't admins");
    assert_eq!(app.req(&bob, "PATCH", "/api/orgs/acme", Some(json!({ "base_role": "none" }))).await.0, StatusCode::FORBIDDEN);
    app.req(&alice, "PATCH", "/api/orgs/acme", Some(json!({ "base_role": "none" }))).await;
    assert_eq!(app.req(&bob, "GET", "/api/repos/acme/x", None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(listed(app.req(&bob, "GET", "/api/users/acme", None).await.1), 0);
    assert_eq!(listed(app.req(&bob, "GET", "/api/feed", None).await.1), 0);
    // Owners administer every org repo; the last owner can't step down or leave.
    app.req(&alice, "PUT", "/api/orgs/acme/members/bob", Some(json!({ "role": "owner" }))).await;
    assert_eq!(app.req(&bob, "PATCH", "/api/repos/acme/x", Some(json!({ "description": "owned" }))).await.0, StatusCode::OK);
    assert_eq!(app.req(&bob, "DELETE", "/api/orgs/acme/members/bob", None).await.0, StatusCode::OK, "bob leaves");
    assert_eq!(app.req(&alice, "DELETE", "/api/orgs/acme/members/alice", None).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(app.req(&alice, "PUT", "/api/orgs/acme/members/alice", Some(json!({ "role": "member" }))).await.0, StatusCode::BAD_REQUEST);
    let (_, org) = app.req(&Client::default(), "GET", "/api/orgs/acme", None).await;
    assert_eq!((org["org"]["display_name"].as_str(), org["members"].as_array().map(Vec::len)), (Some("Acme Inc"), Some(0)), "members are private");
    let (_, org) = app.req(&alice, "GET", "/api/orgs/acme", None).await;
    assert_eq!(org["members"][0]["role"], "owner");
    let (_, mine) = app.req(&alice, "GET", "/api/me/orgs", None).await;
    assert_eq!(mine["orgs"][0]["name"], "acme");
    // Organizations can't sign in, and one that owns repos can't be deleted.
    app.state
        .db
        .call(|c| Ok(c.execute("UPDATE users SET pass_hash = (SELECT pass_hash FROM users WHERE username = 'alice') WHERE username = 'acme'", [])?))
        .await
        .unwrap();
    let (s, _, _) = login(&app, json!({ "login": "acme", "password": "correct horse" })).await;
    assert_ne!(s, StatusCode::OK, "even with a password that matches");
    assert_eq!(app.req(&alice, "DELETE", "/api/orgs/acme", Some(json!({ "confirm": "acme" }))).await.0, StatusCode::BAD_REQUEST);
    app.req(&alice, "DELETE", "/api/repos/acme/x", Some(json!({ "confirm": "acme/x" }))).await;
    assert_eq!(app.req(&alice, "DELETE", "/api/orgs/acme", Some(json!({ "confirm": "acme" }))).await.0, StatusCode::OK);
    assert_eq!(app.req(&alice, "GET", "/api/orgs/acme", None).await.0, StatusCode::NOT_FOUND);
}

async fn user_id(app: &App, name: &str) -> i64 {
    let n = name.to_string();
    app.state.db.call(move |c| Ok(c.query_row("SELECT id FROM users WHERE username = ?1", [n], |r| r.get(0))?)).await.unwrap()
}

#[tokio::test]
async fn organizations_share_their_creators_limits_and_trust() {
    let app = App::new();
    let root = app.signup("root1").await;
    let alice = app.signup("alice").await;
    let bob = app.signup("bob").await;
    // Adding a site admin as an owner doesn't make an organization trusted with CI on the server.
    app.req(&alice, "POST", "/api/orgs", Some(json!({ "name": "evil" }))).await;
    assert_eq!(app.req(&alice, "PUT", "/api/orgs/evil/members/root1", Some(json!({ "role": "owner" }))).await.0, StatusCode::OK);
    assert!(!gitter::ci::trigger::enabled(&app.state, user_id(&app, "evil").await).await);
    app.req(&root, "POST", "/api/orgs", Some(json!({ "name": "staff" }))).await;
    assert!(gitter::ci::trigger::enabled(&app.state, user_id(&app, "staff").await).await, "an org an admin made is");
    // An organization's repositories count toward its creator's repository limit and storage.
    let mut s = app.state.settings.get();
    s.max_repos_per_user = 2;
    s.max_storage_mb = 100;
    app.state.settings.save(&app.state.db, s).await.unwrap();
    assert_eq!(app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "a1", "private": true }))).await.0, StatusCode::OK);
    assert_eq!(app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "o1", "owner": "evil" }))).await.0, StatusCode::OK);
    let (s, v) = app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "o2", "owner": "evil" }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{v}");
    app.state.db.call(|c| Ok(c.execute("UPDATE repos SET size_bytes = 5000000 WHERE name = 'o1'", [])?)).await.unwrap();
    let (a, e) = (user_id(&app, "alice").await, user_id(&app, "evil").await);
    let left = gitter::repo::storage_left(&app.state, a).await.unwrap().unwrap();
    assert!(left <= 100 * 1024 * 1024 - 5_000_000, "{left}");
    assert_eq!(gitter::repo::storage_left(&app.state, e).await.unwrap(), Some(left));
    // Who belongs to an organization is for its members to see.
    assert_eq!(app.req(&bob, "GET", "/api/orgs/evil", None).await.1["members"], json!([]));
    assert_eq!(app.req(&bob, "GET", "/api/users/alice", None).await.1["orgs"], json!([]));
    assert_eq!(app.req(&alice, "GET", "/api/users/alice", None).await.1["orgs"], json!(["evil"]));
    // A token can't drop its owner's access; and an org's only owner can't be deleted out from under it.
    app.req(&alice, "PUT", "/api/repos/alice/a1/collaborators/bob", Some(json!({ "role": "write" }))).await;
    let (_, t) = app.req(&bob, "POST", "/api/me/tokens", Some(json!({ "name": "t", "scope": "write" }))).await;
    let t = t["token"].as_str().unwrap().to_string();
    assert_eq!(bearer(&app, "DELETE", "/api/repos/alice/a1/collaborators/bob", &t, None).await, StatusCode::FORBIDDEN);
    assert_eq!(app.req(&bob, "DELETE", "/api/repos/alice/a1/collaborators/bob", None).await.0, StatusCode::OK, "bob leaves");
    app.req(&root, "DELETE", "/api/orgs/evil/members/root1", None).await;
    let (s, v) = app.req(&root, "DELETE", "/api/admin/users/alice", Some(json!({ "confirm": "alice" }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{v}");
    assert!(v.to_string().contains("only owner of evil"), "{v}");
}

#[tokio::test]
async fn webhooks_log_drops_and_pause_for_suspended_owners() {
    let app = App::new();
    let admin = app.signup("root1").await;
    let bob = app.signup("bob").await;
    app.req(&bob, "POST", "/api/repos", Some(json!({ "name": "r", "readme": true }))).await;
    let rx = Receiver::new();
    let (s, v) = app.req(&admin, "POST", "/api/repos/bob/r/hooks", Some(json!({ "url": rx.url, "secret": "s", "events": ["*"], "allow_private": true }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let id = v["id"].as_i64().unwrap();
    // A receiver with a full queue: the next event is dropped, and the drop is on the hook's log.
    app.state
        .db
        .call(move |c| {
            for _ in 0..gitter::webhooks::MAX_WAITING {
                c.execute("INSERT INTO webhook_deliveries(hook_id, guid, event, payload, next_at, created_at) VALUES (?1, 'g', 'push', '{}', 9999999999, 0)", [id])?;
            }
            Ok(())
        })
        .await
        .unwrap();
    app.req(&admin, "POST", "/api/repos/bob/r/issues", Some(json!({ "title": "Dropped" }))).await;
    let (status, _, next) = newest_delivery(&app).await;
    assert_eq!((status.as_str(), next), ("failed", None));
    let (_, v) = app.req(&admin, "GET", &format!("/api/repos/bob/r/hooks/{id}"), None).await;
    assert!(v.to_string().contains("Dropped:"), "{v}");
    // A suspended owner's hooks don't send.
    let (s, v) = app.req(&admin, "PATCH", "/api/admin/users/bob", Some(json!({ "suspended": true }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    gitter::webhooks::tick(&app.state).await;
    assert!(rx.events().is_empty(), "the ping waiting for bob's hook wasn't sent");
}

#[tokio::test]
async fn transfers_and_trash_follow_org_ownership() {
    let app = App::new();
    let _root = app.signup("root1").await;
    let alice = app.signup("alice").await;
    let bob = app.signup("bob").await;
    let carol = app.signup("carol").await;
    app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "proj", "readme": true }))).await;
    // Admin collaborators manage settings but can't give the repository away.
    app.req(&alice, "PUT", "/api/repos/alice/proj/collaborators/carol", Some(json!({ "role": "admin" }))).await;
    let offer = |to: &str| json!({ "to": to, "confirm": "alice/proj" });
    assert_eq!(app.req(&carol, "POST", "/api/repos/alice/proj/transfer", Some(offer("carol"))).await.0, StatusCode::FORBIDDEN);
    // To an organization: one of its owners accepts; the old owner's secrets stay behind.
    app.req(&bob, "POST", "/api/orgs", Some(json!({ "name": "acme" }))).await;
    assert_eq!(app.req(&alice, "PUT", "/api/repos/alice/proj/ci/secrets/TOKEN", Some(json!({ "value": "s3cret" }))).await.0, StatusCode::OK);
    assert_eq!(app.req(&alice, "POST", "/api/repos/alice/proj/transfer", Some(offer("acme"))).await.0, StatusCode::OK);
    let (_, list) = app.req(&bob, "GET", "/api/me/transfers", None).await;
    let id = list["incoming"][0]["id"].as_i64().expect("an owner of acme sees the offer");
    assert_eq!(app.req(&carol, "POST", &format!("/api/me/transfers/{id}/accept"), None).await.0, StatusCode::NOT_FOUND);
    let (s, v) = app.req(&bob, "POST", &format!("/api/me/transfers/{id}/accept"), None).await;
    assert_eq!((s, v["owner"].as_str()), (StatusCode::OK, Some("acme")), "{v}");
    let secrets: i64 = app.state.db.call(move |c| Ok(c.query_row("SELECT count(*) FROM ci_secrets WHERE repo_id = ?1", [id], |r| r.get(0))?)).await.unwrap();
    assert_eq!(secrets, 0, "CI secrets don't move with the code");
    // Org owners see and restore its deleted repositories; members don't.
    app.req(&bob, "PUT", "/api/orgs/acme/members/carol", Some(json!({ "role": "member" }))).await;
    assert_eq!(app.req(&bob, "DELETE", "/api/repos/acme/proj", Some(json!({ "confirm": "acme/proj" }))).await.0, StatusCode::OK);
    assert_eq!(app.req(&carol, "POST", &format!("/api/me/trash/{id}/restore"), None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(app.req(&bob, "GET", "/api/me/trash", None).await.1["repos"][0]["name"], "proj");
    let (s, v) = app.req(&bob, "POST", &format!("/api/me/trash/{id}/restore"), None).await;
    assert_eq!((s, v["owner"].as_str()), (StatusCode::OK, Some("acme")), "{v}");
    // A limited token can't list your trash or transfer offers (private repo names).
    let (_, t) = app.req(&bob, "POST", "/api/me/tokens", Some(json!({ "name": "ci", "scope": "write" }))).await;
    let t = t["token"].as_str().unwrap().to_string();
    assert_eq!(bearer(&app, "GET", "/api/me/trash", &t, None).await, StatusCode::FORBIDDEN);
    assert_eq!(bearer(&app, "GET", "/api/me/transfers", &t, None).await, StatusCode::FORBIDDEN);
    // Deleting an organization purges what's left in its trash.
    app.req(&bob, "DELETE", "/api/repos/acme/proj", Some(json!({ "confirm": "acme/proj" }))).await;
    assert_eq!(app.req(&bob, "DELETE", "/api/orgs/acme", Some(json!({ "confirm": "acme" }))).await.0, StatusCode::OK);
    assert!(!app.state.cfg.data_dir.join(format!("trash/{id}.git")).exists());
}

// ---------------------------------------------------------------- MCP and agent tokens

const AGENT_TOOLS: &[&str] = &[
    "search_code", "read_file", "list_tree", "list_issues", "get_issue", "create_issue", "comment",
    "open_pull_request", "list_ci_runs", "get_ci_run", "get_ci_log", "commit_files",
];

/// One raw body POSTed to /mcp with `token` (None: no Authorization header).
async fn mcp_raw(app: &App, token: Option<&str>, body: &str) -> (StatusCode, Value) {
    let mut b = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header("host", "test.local")
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream");
    if let Some(t) = token {
        b = b.header("authorization", format!("Bearer {t}"));
    }
    let resp = app.router.clone().oneshot(b.body(Body::from(body.to_string())).unwrap()).await.unwrap();
    let s = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (s, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

async fn mcp(app: &App, token: &str, method: &str, params: Value) -> Value {
    let (s, v) = mcp_raw(app, Some(token), &json!({ "jsonrpc": "2.0", "id": 7, "method": method, "params": params }).to_string()).await;
    assert_eq!(s, StatusCode::OK, "{method}: {v}");
    assert_eq!((v["jsonrpc"].as_str(), v["id"].as_i64()), (Some("2.0"), Some(7)), "{v}");
    v
}

/// tools/call: (isError, text).
async fn tool(app: &App, token: &str, name: &str, args: Value) -> (bool, String) {
    let v = mcp(app, token, "tools/call", json!({ "name": name, "arguments": args })).await;
    let r = &v["result"];
    assert!(r.is_object(), "{name}: {v}");
    (r["isError"].as_bool().unwrap_or(false), r["content"][0]["text"].as_str().unwrap_or_default().to_string())
}

async fn agent_token(app: &App, who: &Client, repos: &[&str], tools: &[&str]) -> String {
    let (s, t) = app
        .req(who, "POST", "/api/me/tokens", Some(json!({ "name": "helper", "kind": "agent", "repos": repos, "tools": tools, "expires_days": 30 })))
        .await;
    assert_eq!(s, StatusCode::OK, "{t}");
    t["token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn mcp_serves_initialize_tools_list_and_every_tool() {
    let app = App::new();
    let alice = app.signup("alice").await;
    app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "proj", "readme": true }))).await;
    let t = agent_token(&app, &alice, &["alice/proj"], AGENT_TOOLS).await;
    let (_, list) = app.req(&alice, "GET", "/api/me/tokens", None).await;
    let row = &list["tokens"][0];
    assert_eq!((row["kind"].as_str(), row["repos"][0].as_str()), (Some("agent"), Some("alice/proj")), "{row}");
    assert_eq!(row["tools"].as_array().unwrap().len(), AGENT_TOOLS.len());

    let v = mcp(&app, &t, "initialize", json!({ "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "test", "version": "1" } })).await;
    assert_eq!(v["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(v["result"]["serverInfo"]["name"], "gitter");
    assert!(v["result"]["capabilities"]["tools"].is_object(), "{v}");
    let (s, _) = mcp_raw(&app, Some(&t), r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).await;
    assert_eq!(s, StatusCode::ACCEPTED, "notifications get no reply");
    assert_eq!(mcp(&app, &t, "ping", json!({})).await["result"], json!({}));
    let v = mcp(&app, &t, "tools/list", json!({})).await;
    let tools = v["result"]["tools"].as_array().unwrap();
    for n in AGENT_TOOLS {
        let d = tools.iter().find(|x| x["name"] == *n).unwrap_or_else(|| panic!("{n} listed"));
        assert_eq!(d["inputSchema"]["type"], "object", "{d}");
    }

    // Reads.
    let (e, txt) = tool(&app, &t, "list_repos", json!({})).await;
    assert!(!e && txt.contains("alice/proj"), "{txt}");
    let (e, txt) = tool(&app, &t, "list_tree", json!({ "repo": "alice/proj" })).await;
    assert!(!e && txt.contains("README.md"), "{txt}");
    let (e, txt) = tool(&app, &t, "read_file", json!({ "repo": "alice/proj", "path": "README.md" })).await;
    assert!(!e && txt.contains("# proj"), "{txt}");
    let (e, txt) = tool(&app, &t, "search_code", json!({ "repo": "alice/proj", "query": "proj" })).await;
    assert!(!e && txt.contains("README.md"), "{txt}");

    // Writes, each marked as the agent's.
    let (e, txt) = tool(&app, &t, "create_issue", json!({ "repo": "alice/proj", "title": "Flaky test", "body": "Seen twice" })).await;
    assert!(!e, "{txt}");
    let (e, txt) = tool(&app, &t, "comment", json!({ "repo": "alice/proj", "number": 1, "body": "Looking into it" })).await;
    assert!(!e, "{txt}");
    let (_, issue) = app.req(&alice, "GET", "/api/repos/alice/proj/issues/1", None).await;
    assert_eq!((issue["issue"]["agent"].as_str(), issue["comments"][0]["agent"].as_str()), (Some("helper"), Some("helper")), "{issue}");
    let change = json!([{ "path": "fix.txt", "content": "fixed\n" }]);
    let (e, txt) = tool(&app, &t, "commit_files", json!({ "repo": "alice/proj", "branch": "main", "message": "Fix", "files": change })).await;
    assert!(e && txt.contains("default branch"), "agents never commit to the default branch: {txt}");
    let (e, txt) = tool(&app, &t, "commit_files", json!({ "repo": "alice/proj", "branch": "main", "new_branch": "agent/fix", "message": "Fix", "files": change })).await;
    assert!(!e && txt.contains("agent/fix"), "{txt}");
    let (e, txt) = tool(&app, &t, "open_pull_request", json!({ "repo": "alice/proj", "head": "agent/fix", "base": "main", "title": "Fix it" })).await;
    assert!(!e, "{txt}");
    let (e, txt) = tool(&app, &t, "list_issues", json!({ "repo": "alice/proj" })).await;
    assert!(!e && txt.contains("Flaky test") && !txt.contains("Fix it"), "{txt}");
    let (e, txt) = tool(&app, &t, "list_issues", json!({ "repo": "alice/proj", "pulls": true })).await;
    assert!(!e && txt.contains("Fix it"), "{txt}");
    let (e, txt) = tool(&app, &t, "get_issue", json!({ "repo": "alice/proj", "number": 2 })).await;
    assert!(!e && txt.contains("agent/fix"), "{txt}");
    let (e, txt) = tool(&app, &t, "list_ci_runs", json!({ "repo": "alice/proj" })).await;
    assert!(!e, "{txt}");
    assert!(tool(&app, &t, "get_ci_run", json!({ "repo": "alice/proj", "number": 1 })).await.0, "no such run");
    assert!(tool(&app, &t, "get_ci_log", json!({ "repo": "alice/proj", "job_id": 999 })).await.0, "no such job");

    // The feed and the audit log say an agent did it, and which token.
    let kinds: Vec<String> = app
        .state
        .db
        .call(|c| {
            let mut q = c.prepare("SELECT kind FROM events WHERE agent = 'helper' ORDER BY id")?;
            Ok(q.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?)
        })
        .await
        .unwrap();
    assert_eq!(kinds, ["issue", "comment", "push", "pull"]);
    let (_, log) = app.req(&alice, "GET", "/api/me/audit", None).await;
    let acts: Vec<String> = log["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["action"] == "agent_action")
        .map(|e| e["detail"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(acts.len(), 4, "one entry per change: {acts:?}");
    assert!(acts.iter().all(|d| d.starts_with("helper: ")), "{acts:?}");
    assert!(acts.iter().any(|d| d.contains("create_issue alice/proj")), "{acts:?}");

    // Protocol errors.
    assert_eq!(mcp(&app, &t, "resources/list", json!({})).await["error"]["code"], -32601);
    assert_eq!(mcp_raw(&app, Some(&t), "not json").await.1["error"]["code"], -32700);
    assert_eq!(mcp_raw(&app, Some(&t), "[]").await.1["error"]["code"], -32600);
    assert_eq!(mcp(&app, &t, "tools/call", json!({ "name": "nope", "arguments": {} })).await["error"]["code"], -32602);
    assert_eq!(mcp(&app, &t, "tools/call", json!({ "name": "read_file", "arguments": { "path": "x" } })).await["error"]["code"], -32602);
    assert_eq!(mcp(&app, &t, "tools/call", json!({ "name": "read_file", "arguments": { "repo": "../../me", "path": "x" } })).await["error"]["code"], -32602);
}

#[tokio::test]
async fn mcp_agent_tokens_only_reach_their_repos_and_tools() {
    let app = App::new();
    let alice = app.signup("alice").await;
    let bob = app.signup("bob").await;
    for (who, name, private) in [(&alice, "proj", false), (&alice, "secret", true), (&bob, "pub", false)] {
        app.req(who, "POST", "/api/repos", Some(json!({ "name": name, "private": private, "readme": true }))).await;
    }
    // Repositories and tools are checked when the token is made.
    for (repos, tools, want) in [
        (json!(["alice/proj"]), json!(["nope"]), StatusCode::BAD_REQUEST),
        (json!([]), json!(["read_file"]), StatusCode::BAD_REQUEST),
        (json!(["alice/proj"]), json!([]), StatusCode::BAD_REQUEST),
        (json!(["alice/missing"]), json!(["read_file"]), StatusCode::NOT_FOUND),
        (json!(["bob/secret"]), json!(["read_file"]), StatusCode::NOT_FOUND),
    ] {
        let (s, v) = app.req(&alice, "POST", "/api/me/tokens", Some(json!({ "name": "x", "kind": "agent", "repos": repos, "tools": tools }))).await;
        assert_eq!(s, want, "{repos} {tools}: {v}");
    }
    let t = agent_token(&app, &alice, &["alice/proj"], &["read_file", "create_issue"]).await;
    let v = mcp(&app, &t, "tools/list", json!({})).await;
    let mut names: Vec<&str> = v["result"]["tools"].as_array().unwrap().iter().map(|x| x["name"].as_str().unwrap()).collect();
    names.sort();
    assert_eq!(names, ["create_issue", "list_repos", "read_file"]);
    let v = mcp(&app, &t, "tools/call", json!({ "name": "comment", "arguments": { "repo": "alice/proj", "number": 1, "body": "hi" } })).await;
    assert_eq!(v["error"]["code"], -32602, "a tool it wasn't given: {v}");
    let readme = |repo: &str| json!({ "repo": repo, "path": "README.md" });
    assert!(!tool(&app, &t, "read_file", readme("alice/proj")).await.0);
    assert!(tool(&app, &t, "read_file", readme("alice/secret")).await.0, "its owner's other repos are out of reach");
    assert!(tool(&app, &t, "read_file", readme("bob/pub")).await.0, "public repos it wasn't given too");
    assert!(tool(&app, &t, "create_issue", json!({ "repo": "bob/pub", "title": "spam" })).await.0);

    // The token does nothing outside /mcp: not the API, git, raw files, settings, secrets, tokens or merges.
    for (m, p) in [
        ("GET", "/api/me"),
        ("GET", "/api/repos/alice/proj"),
        ("GET", "/api/repos/alice/secret/blob?path=README.md"),
        ("GET", "/api/me/tokens"),
        ("POST", "/api/me/tokens"),
        ("PATCH", "/api/repos/alice/proj"),
        ("GET", "/api/repos/alice/proj/ci/secrets"),
        ("GET", "/api/repos/alice/proj/collaborators"),
        ("GET", "/api/repos/alice/proj/hooks"),
        ("POST", "/api/repos/alice/proj/pulls/1/merge"),
        ("GET", "/raw/alice/secret/main/README.md"),
    ] {
        assert_eq!(bearer(&app, m, p, &t, Some(json!({}))).await, StatusCode::FORBIDDEN, "{m} {p}");
    }
    use base64::Engine;
    let basic = base64::engine::general_purpose::STANDARD.encode(format!("alice:{t}"));
    let req = Request::builder()
        .uri("/alice/secret.git/info/refs?service=git-upload-pack")
        .header("authorization", format!("Basic {basic}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(status_with(&app, req).await, StatusCode::FORBIDDEN, "no git with an agent token");

    // /mcp itself takes only agent tokens.
    assert_eq!(mcp_raw(&app, None, r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#).await.0, StatusCode::UNAUTHORIZED);
    let (_, full) = app.req(&alice, "POST", "/api/me/tokens", Some(json!({ "name": "laptop" }))).await;
    let full = full["token"].as_str().unwrap();
    assert_eq!(mcp_raw(&app, Some(full), r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#).await.0, StatusCode::FORBIDDEN);

    // Agents have their own rate limit, apart from their owner's.
    let mut limited = false;
    for _ in 0..300 {
        if mcp_raw(&app, Some(&t), r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#).await.0 == StatusCode::TOO_MANY_REQUESTS {
            limited = true;
            break;
        }
    }
    assert!(limited, "an agent token is rate limited");
    assert_eq!(app.req(&alice, "POST", "/api/repos/alice/proj/issues", Some(json!({ "title": "still me" }))).await.0, StatusCode::OK);
    // Deleting the token stops it at once.
    let (_, list) = app.req(&alice, "GET", "/api/me/tokens", None).await;
    let id = list["tokens"].as_array().unwrap().iter().find(|x| x["kind"] == "agent").unwrap()["id"].as_i64().unwrap();
    app.req(&alice, "DELETE", &format!("/api/me/tokens/{id}"), None).await;
    assert_eq!(mcp_raw(&app, Some(&t), r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#).await.0, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn mcp_agent_ci_runs_get_no_secrets_and_never_the_builtin_runner() {
    let app = App::new();
    let root = app.signup("root1").await;
    app.req(&root, "POST", "/api/repos", Some(json!({ "name": "r", "readme": true }))).await;
    let (s, _) = app.req(&root, "PUT", "/api/repos/root1/r/ci/secrets/TOKEN", Some(json!({ "value": "hunter2-secret" }))).await;
    assert_eq!(s, StatusCode::OK);
    let t = agent_token(&app, &root, &["root1/r"], &["commit_files", "list_ci_runs", "get_ci_run", "get_ci_log"]).await;
    let ci = json!([{ "path": ".gitter/ci.yml", "content": "jobs:\n  build:\n    steps: [\"echo token=[$TOKEN]\"]\n" }]);
    let (e, txt) = tool(&app, &t, "commit_files", json!({ "repo": "root1/r", "branch": "main", "new_branch": "agent/ci", "message": "CI", "files": ci })).await;
    assert!(!e, "{txt}");
    // A person's push to the agent's branch runs the agent's workflow too, so it's marked the same.
    let fixup = json!({ "branch": "agent/ci", "message": "Fixup", "changes": [{ "path": "a.txt", "content": "a\n" }] });
    assert_eq!(app.req(&root, "POST", "/api/repos/root1/r/contents", Some(fixup)).await.0, StatusCode::OK);
    let agents: Vec<Option<String>> = app
        .state
        .db
        .call(|c| Ok(c.prepare("SELECT agent FROM ci_runs ORDER BY number")?.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?))
        .await
        .unwrap();
    assert_eq!(agents, [Some("helper".to_string()), Some("helper".to_string())]);
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
    let (builtin, own) = (runner(true).await, runner(false).await);
    // The built-in runner is the server itself: it never runs code an agent wrote.
    assert!(gitter::ci::jobs::claim(&app.state, builtin, vec![], true).await.unwrap().is_none(), "agent runs stay off the server");
    let human = json!({ "branch": "main", "new_branch": "people", "message": "CI", "changes": [{ "path": ".gitter/ci.yml", "content": "jobs:\n  b:\n    steps: [\"true\"]\n" }] });
    assert_eq!(app.req(&root, "POST", "/api/repos/root1/r/contents", Some(human)).await.0, StatusCode::OK);
    let lease = gitter::ci::jobs::claim(&app.state, builtin, vec![], true).await.unwrap().expect("a person's run");
    assert_eq!((lease.run_number, lease.secrets.len()), (3, 1), "a person's own branch runs with secrets");
    // Another runner may take agent runs, without secrets.
    for n in [1, 2] {
        let lease = gitter::ci::jobs::claim(&app.state, own, vec![], false).await.unwrap().expect("agent run");
        assert_eq!((lease.run_number, lease.secrets.len()), (n, 0));
        let (e, log) = tool(&app, &t, "get_ci_log", json!({ "repo": "root1/r", "job_id": lease.job_id })).await;
        assert!(!e && log.contains("gets no CI secrets"), "{log}");
    }
}

#[tokio::test]
async fn mcp_agents_commit_only_to_branches_agents_created() {
    let app = App::new();
    let alice = app.signup("alice").await;
    app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "p", "readme": true }))).await;
    app.req(&alice, "POST", "/api/repos/alice/p/branches", Some(json!({ "name": "release", "from": "main" }))).await;
    let t = agent_token(&app, &alice, &["alice/p"], &["commit_files"]).await;
    let commit = |branch: &str, new: Option<&str>| {
        let mut a = json!({ "repo": "alice/p", "branch": branch, "message": "m", "files": [{ "path": "x.txt", "content": "x\n" }] });
        if let Some(n) = new {
            a["new_branch"] = json!(n);
        }
        a
    };
    let (e, txt) = tool(&app, &t, "commit_files", commit("release", None)).await;
    assert!(e && txt.contains("branches an agent created"), "a person's branch is off limits: {txt}");
    assert!(!tool(&app, &t, "commit_files", commit("main", Some("agent/a"))).await.0);
    let (e, txt) = tool(&app, &t, "commit_files", commit("agent/a", None)).await;
    assert!(!e, "its own branch takes more commits: {txt}");
    // Deleted and made again by a person, the name is theirs.
    assert_eq!(app.req(&alice, "DELETE", "/api/repos/alice/p/branches?name=agent/a", None).await.0, StatusCode::OK);
    app.req(&alice, "POST", "/api/repos/alice/p/branches", Some(json!({ "name": "agent/a", "from": "main" }))).await;
    assert!(tool(&app, &t, "commit_files", commit("agent/a", None)).await.0, "a person's new branch under an old name");
    // Agents can't fill a repository with branches.
    app.state
        .db
        .call(|c| {
            for i in 0..100 {
                c.execute("INSERT INTO agent_branches (repo_id, branch, agent, created_at) VALUES (1, ?1, 'helper', 0)", [format!("b{i}")])?;
            }
            Ok(())
        })
        .await
        .unwrap();
    let (e, txt) = tool(&app, &t, "commit_files", commit("main", Some("agent/z"))).await;
    assert!(e && txt.contains("100 branches"), "{txt}");
}

// ---------- insights: commit graph, file history, traffic (T-0053)

/// Run git against a bare repository as a push would, returning trimmed stdout.
fn insights_git(repo: &std::path::Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .arg(format!("--git-dir={}", repo.display()))
        .args(["-c", "user.name=a", "-c", "user.email=a@b.c"])
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

async fn insights_write(app: &App, c: &Client, repo: &str, branch: &str, msg: &str, changes: Value) {
    let (s, e) = app
        .req(c, "POST", &format!("/api/repos/{repo}/contents"), Some(json!({ "branch": branch, "message": msg, "changes": changes })))
        .await;
    assert_eq!(s, StatusCode::OK, "{msg}: {e}");
}

/// Every row's lines join up with the rows around it: what leaves the bottom of
/// one row (pass + down) is what enters the top of the next (pass + up).
fn insights_check_lanes(rows: &[Value], mut active: Vec<i64>) -> Vec<i64> {
    let lanes = |v: &Value| -> Vec<i64> { v.as_array().unwrap().iter().map(|x| x.as_i64().unwrap()).collect() };
    for r in rows {
        let (lane, pass, up, down) = (r["lane"].as_i64().unwrap(), lanes(&r["pass"]), lanes(&r["up"]), lanes(&r["down"]));
        let mut top: Vec<i64> = pass.iter().chain(&up).copied().collect();
        top.sort();
        top.dedup();
        active.sort();
        assert_eq!(top, active, "row {} starts where the last ended: {r}", r["sha"]);
        assert!(up.is_empty() || up.contains(&lane), "lines from above meet at the dot: {r}");
        assert!(!pass.contains(&lane), "nothing passes through the dot: {r}");
        active = pass.iter().chain(&down).copied().collect();
        active.sort();
        active.dedup();
    }
    active
}

#[tokio::test]
async fn insights_graph_lanes_cover_branches_and_merges() {
    let app = App::new();
    let _root = app.signup("root1").await;
    let alice = app.signup("alice").await;
    app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "g", "readme": true }))).await;
    app.req(&alice, "POST", "/api/repos/alice/g/branches", Some(json!({ "name": "side", "from": "main" }))).await;
    insights_write(&app, &alice, "alice/g", "side", "Side work", json!([{ "path": "side.txt", "content": "s\n" }])).await;
    insights_write(&app, &alice, "alice/g", "main", "Main work", json!([{ "path": "main.txt", "content": "m\n" }])).await;
    let repo = app.state.git.repo_path(1);
    let (m, s) = (insights_git(&repo, &["rev-parse", "main"]), insights_git(&repo, &["rev-parse", "side"]));
    let merge = insights_git(&repo, &["commit-tree", &format!("{m}^{{tree}}"), "-p", &m, "-p", &s, "-m", "Merge side"]);
    insights_git(&repo, &["update-ref", "refs/heads/main", &merge]);
    // An unmerged branch shows up too: the graph spans every branch.
    app.req(&alice, "POST", "/api/repos/alice/g/branches", Some(json!({ "name": "wip", "from": "side" }))).await;
    insights_write(&app, &alice, "alice/g", "wip", "Unfinished", json!([{ "path": "wip.txt", "content": "w\n" }])).await;

    let (st, d) = app.req(&alice, "GET", "/api/repos/alice/g/graph", None).await;
    assert_eq!(st, StatusCode::OK, "{d}");
    let rows = d["commits"].as_array().unwrap();
    let subjects: Vec<&str> = rows.iter().map(|r| r["subject"].as_str().unwrap()).collect();
    assert_eq!(rows.len(), 5, "{subjects:?}");
    for want in ["Merge side", "Side work", "Main work", "Unfinished", "Initial commit"] {
        assert!(subjects.contains(&want), "{want} in {subjects:?}");
    }
    let merge_row = rows.iter().find(|r| r["subject"] == "Merge side").unwrap();
    let down = merge_row["down"].as_array().unwrap();
    assert_eq!(down.len(), 2, "a merge leaves on two lines: {merge_row}");
    assert_ne!(down[0], down[1]);
    assert_eq!(merge_row["parents"].as_array().unwrap().len(), 2);
    let end = insights_check_lanes(rows, vec![]);
    assert!(end.is_empty(), "every line ends at the root commit: {end:?}");
    assert!(rows.last().unwrap()["down"].as_array().unwrap().is_empty(), "the root has no parent line");
    assert!(d["width"].as_i64().unwrap() >= 2);
    assert_eq!(d["has_more"], false);

    // Pages carry the lanes over: page 2 starts with the lines page 1 left open.
    let (_, p1) = app.req(&alice, "GET", "/api/repos/alice/g/graph?per=2", None).await;
    let (_, p2) = app.req(&alice, "GET", "/api/repos/alice/g/graph?per=2&page=2", None).await;
    assert_eq!(p1["has_more"], true);
    let open = insights_check_lanes(p1["commits"].as_array().unwrap(), vec![]);
    assert!(!open.is_empty());
    insights_check_lanes(p2["commits"].as_array().unwrap(), open);
    assert_eq!(p2["commits"][0]["sha"], rows[2]["sha"]);

    // Private repositories stay private; filters don't apply to the all-branches graph.
    app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "hid", "private": true, "readme": true }))).await;
    let bob = app.signup("bob").await;
    assert_eq!(app.req(&bob, "GET", "/api/repos/alice/hid/graph", None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(app.req(&Client::default(), "GET", "/api/repos/alice/hid/graph", None).await.0, StatusCode::NOT_FOUND);
    let (s, e) = app.req(&alice, "GET", "/api/repos/alice/g/graph?page=500", None).await;
    assert_eq!(s, StatusCode::OK, "{e}");
    assert_eq!(e["commits"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn insights_file_history_follows_renames() {
    let app = App::new();
    let _root = app.signup("root1").await;
    let alice = app.signup("alice").await;
    app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "h", "readme": true }))).await;
    let text = "one\ntwo\nthree\nfour\nfive\nsix\nseven\n";
    insights_write(&app, &alice, "alice/h", "main", "Add notes", json!([{ "path": "docs/a.txt", "content": text }])).await;
    insights_write(&app, &alice, "alice/h", "main", "Edit notes", json!([{ "path": "docs/a.txt", "content": format!("{text}eight\n") }])).await;
    insights_write(&app, &alice, "alice/h", "main", "Unrelated", json!([{ "path": "other.txt", "content": "x\n" }])).await;
    insights_write(&app, &alice, "alice/h", "main", "Rename notes", json!([
        { "path": "docs/a.txt", "delete": true }, { "path": "b.txt", "content": format!("{text}eight\n") }
    ])).await;
    insights_write(&app, &alice, "alice/h", "main", "Edit renamed", json!([{ "path": "b.txt", "content": format!("{text}eight\nnine\n") }])).await;

    let (s, d) = app.req(&alice, "GET", "/api/repos/alice/h/history?rev=main&path=b.txt", None).await;
    assert_eq!(s, StatusCode::OK, "{d}");
    let v = d["versions"].as_array().unwrap();
    let got: Vec<(&str, &str, &str)> = v
        .iter()
        .map(|x| (x["commit"]["subject"].as_str().unwrap(), x["status"].as_str().unwrap(), x["path"].as_str().unwrap()))
        .collect();
    assert_eq!(
        got,
        vec![
            ("Edit renamed", "modified", "b.txt"),
            ("Rename notes", "renamed", "b.txt"),
            ("Edit notes", "modified", "docs/a.txt"),
            ("Add notes", "added", "docs/a.txt"),
        ]
    );
    assert_eq!(v[1]["old_path"], "docs/a.txt");
    assert_eq!(d["has_more"], false);
    // Each version's diff loads through the commit view, renames paired up.
    let sha = v[1]["commit"]["sha"].as_str().unwrap();
    let (_, c) = app.req(&alice, "GET", &format!("/api/repos/alice/h/commit/{sha}?path=b.txt&old=docs/a.txt"), None).await;
    assert_eq!(c["diff"]["files"].as_array().unwrap().len(), 1, "{c}");
    let (_, p) = app.req(&alice, "GET", "/api/repos/alice/h/history?rev=main&path=b.txt&per=3&page=2", None).await;
    assert_eq!(p["versions"].as_array().unwrap().len(), 1);
    assert_eq!(p["versions"][0]["commit"]["subject"], "Add notes");

    assert_eq!(app.req(&alice, "GET", "/api/repos/alice/h/history?rev=main&path=", None).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(app.req(&alice, "GET", "/api/repos/alice/h/history?rev=main&path=../x", None).await.0, StatusCode::BAD_REQUEST);
    let (_, none) = app.req(&alice, "GET", "/api/repos/alice/h/history?rev=main&path=nope.txt", None).await;
    assert_eq!(none["versions"].as_array().unwrap().len(), 0);
    app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "hid", "private": true, "readme": true }))).await;
    let bob = app.signup("bob").await;
    assert_eq!(app.req(&bob, "GET", "/api/repos/alice/hid/history?path=README.md", None).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn insights_traffic_counts_for_admins_only() {
    let app = App::new();
    let _root = app.signup("root1").await;
    let alice = app.signup("alice").await;
    let bob = app.signup("bob").await;
    let anon = Client::default();
    app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "pub", "readme": true }))).await;
    app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "priv", "private": true, "readme": true }))).await;
    let view = |path: &str, referrer: &str| json!({ "path": path, "referrer": referrer });
    for (c, p, r) in [
        (&anon, "/", "https://news.example.com/item?id=1"),
        (&bob, "/blob/main/README.md", "https://news.example.com/other"),
        (&alice, "/blob/main/README.md", "http://test.local/alice"),
    ] {
        let (s, e) = app.req(c, "POST", "/api/repos/alice/pub/traffic/view", Some(view(p, r))).await;
        assert_eq!(s, StatusCode::OK, "{e}");
    }
    // Junk is dropped, not stored: a path off the repository, an over-long one.
    app.req(&anon, "POST", "/api/repos/alice/pub/traffic/view", Some(view(&"/x".repeat(400), "javascript:alert(1)"))).await;
    // A clone (or fetch) starts with the upload-pack ref advertisement.
    let refs = Request::get("/alice/pub.git/info/refs?service=git-upload-pack").body(Body::empty()).unwrap();
    assert_eq!(app.router.clone().oneshot(refs).await.unwrap().status(), StatusCode::OK);

    let (s, t) = app.req(&alice, "GET", "/api/repos/alice/pub/traffic", None).await;
    assert_eq!(s, StatusCode::OK, "{t}");
    let days = t["days"].as_array().unwrap();
    assert_eq!(days.len(), 14);
    let today = days.last().unwrap();
    assert_eq!((today["views"].as_i64(), today["clones"].as_i64()), (Some(4), Some(1)), "{t}");
    assert_eq!(today["unique_views"], 1, "the same address is one visitor: {t}");
    assert_eq!(today["unique_clones"], 1);
    assert_eq!(t["views"], 4);
    assert_eq!(t["referrers"], json!([{ "name": "news.example.com", "count": 2 }]), "own-site and junk referrers are skipped");
    assert_eq!(t["paths"][0], json!({ "name": "/blob/main/README.md", "count": 2 }), "{t}");
    assert!(t["paths"].as_array().unwrap().iter().all(|p| p["name"].as_str().unwrap().len() <= 200));
    // A day's list filled with junk early doesn't lock out a page that's actually visited.
    for i in 0..110 {
        app.req(&anon, "POST", "/api/repos/alice/pub/traffic/view", Some(view(&format!("/tree/junk{i}"), ""))).await;
    }
    for _ in 0..3 {
        app.req(&anon, "POST", "/api/repos/alice/pub/traffic/view", Some(view("/issues", ""))).await;
    }
    let (_, t2) = app.req(&alice, "GET", "/api/repos/alice/pub/traffic", None).await;
    assert!(t2["paths"].as_array().unwrap().iter().any(|p| p["name"] == "/issues"), "{t2}");
    // Stored by repository id, with no address in it.
    let leaked: i64 = app
        .state
        .db
        .call(|c| Ok(c.query_row("SELECT count(*) FROM traffic_visitors WHERE hash LIKE '%unknown%' OR hash LIKE '%127.0.0.1%'", [], |r| r.get(0))?))
        .await
        .unwrap();
    assert_eq!(leaked, 0);

    // Readers and strangers get nothing; private repositories don't count outsiders.
    assert_eq!(app.req(&bob, "GET", "/api/repos/alice/pub/traffic", None).await.0, StatusCode::FORBIDDEN);
    assert_eq!(app.req(&anon, "GET", "/api/repos/alice/pub/traffic", None).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(app.req(&bob, "POST", "/api/repos/alice/priv/traffic/view", Some(view("/", ""))).await.0, StatusCode::NOT_FOUND);
    assert_eq!(app.req(&bob, "GET", "/api/repos/alice/priv/traffic", None).await.0, StatusCode::NOT_FOUND);
    let (_, p) = app.req(&alice, "GET", "/api/repos/alice/priv/traffic", None).await;
    assert_eq!(p["views"], 0);
    // An admin collaborator sees it, and it follows a rename.
    app.req(&alice, "PUT", "/api/repos/alice/pub/collaborators/bob", Some(json!({ "role": "admin" }))).await;
    assert_eq!(app.req(&bob, "GET", "/api/repos/alice/pub/traffic", None).await.0, StatusCode::OK);
    let (s, e) = app.req(&alice, "PATCH", "/api/repos/alice/pub", Some(json!({ "name": "renamed" }))).await;
    assert_eq!(s, StatusCode::OK, "{e}");
    assert_eq!(app.req(&alice, "GET", "/api/repos/alice/renamed/traffic", None).await.1["views"], 117, "4 views above, 113 more here");
    // A trashed repository is gone: no counting, no report.
    app.req(&alice, "DELETE", "/api/repos/alice/renamed", Some(json!({ "confirm": "alice/renamed" }))).await;
    assert_eq!(app.req(&alice, "GET", "/api/repos/alice/renamed/traffic", None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(app.req(&anon, "POST", "/api/repos/alice/renamed/traffic/view", Some(view("/", ""))).await.0, StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------- code review

/// Commit `content` to `path` on `branch` (or a new branch from it) through the web-edit API.
async fn put_file(app: &App, c: &Client, branch: &str, new_branch: Option<&str>, path: &str, content: &str) -> String {
    let mut body = json!({ "branch": branch, "message": format!("Write {path}"), "changes": [{ "path": path, "content": content }] });
    if let Some(nb) = new_branch {
        body["new_branch"] = json!(nb);
    }
    let (s, v) = app.req(c, "POST", "/api/repos/alice/r/contents", Some(body)).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    v["sha"].as_str().unwrap().to_string()
}

/// alice/r with a.txt on main and a pull request #1 from `feat` changing it.
async fn review_fixture(app: &App, alice: &Client) {
    app.req(alice, "POST", "/api/repos", Some(json!({ "name": "r", "readme": true }))).await;
    put_file(app, alice, "main", None, "a.txt", "one\ntwo\nthree\n").await;
    put_file(app, alice, "main", Some("feat"), "a.txt", "one\nTWO\nthree\nfour\n").await;
    let (s, v) = app.req(alice, "POST", "/api/repos/alice/r/pulls", Some(json!({ "title": "Shout", "head": "feat", "base": "main" }))).await;
    assert_eq!((s, v["number"].as_i64()), (StatusCode::OK, Some(1)), "{v}");
}

fn threads(v: &Value) -> Vec<Value> {
    v["threads"].as_array().cloned().unwrap_or_default()
}

#[tokio::test]
async fn review_threads_states_and_outdated_comments() {
    let app = App::new();
    let alice = app.signup("alice").await;
    let bob = app.signup("bob").await;
    let carol = app.signup("carol").await;
    review_fixture(&app, &alice).await;
    let pr = "/api/repos/alice/r/pulls/1";

    // A single comment on a line is visible to everyone at once.
    let (s, c1) = app.req(&bob, "POST", &format!("{pr}/comments"), Some(json!({ "path": "a.txt", "side": "new", "line": 2, "body": "Why shout?" }))).await;
    assert_eq!(s, StatusCode::OK, "{c1}");
    let c1 = c1["id"].as_i64().unwrap();
    // Comments in a pending review are only for their author until it's submitted.
    let (s, v) = app.req(&bob, "POST", &format!("{pr}/comments"), Some(json!({ "path": "a.txt", "side": "new", "start_line": 3, "line": 4, "body": "A range", "pending": true }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(threads(&app.req(&carol, "GET", &format!("{pr}/reviews"), None).await.1).len(), 1);
    let (_, mine) = app.req(&bob, "GET", &format!("{pr}/reviews"), None).await;
    assert_eq!((threads(&mine).len(), mine["pending"]["comments"].as_i64()), (2, Some(1)), "{mine}");
    let t = threads(&mine).into_iter().find(|t| t["line"] == 4).unwrap();
    assert_eq!((t["start_line"].as_i64(), t["original"].as_str()), (Some(3), Some("three\nfour")), "{t}");
    // Anchors must exist in the pull request's files.
    for bad in [json!({ "path": "a.txt", "side": "new", "line": 99, "body": "x" }), json!({ "path": "nope.txt", "side": "new", "line": 1, "body": "x" }),
        json!({ "path": "a.txt", "side": "up", "line": 1, "body": "x" }), json!({ "path": "a.txt", "side": "new", "line": 2, "start_line": 3, "body": "x" }),
        json!({ "path": "a.txt", "side": "new", "line": 1, "body": "" })] {
        assert_eq!(app.req(&bob, "POST", &format!("{pr}/comments"), Some(bad.clone())).await.0, StatusCode::BAD_REQUEST, "{bad}");
    }
    assert_eq!(app.req(&Client::default(), "POST", &format!("{pr}/comments"), Some(json!({ "path": "a.txt", "side": "new", "line": 1, "body": "x" }))).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(app.req(&bob, "POST", &format!("{pr}/comments"), Some(json!({ "path": "README.md", "side": "new", "line": 1, "body": "x" }))).await.0, StatusCode::BAD_REQUEST, "only files the PR changes");
    // Nobody else can see, or reply to, a pending thread.
    let hidden = threads(&mine).into_iter().find(|t| t["line"] == 4).unwrap()["id"].as_i64().unwrap();
    assert_eq!(app.req(&carol, "POST", &format!("{pr}/comments"), Some(json!({ "in_reply_to": hidden, "body": "peek" }))).await.0, StatusCode::BAD_REQUEST);

    // A pending review can be thrown away; the review then has nothing pending.
    let (_, tmp) = app.req(&carol, "POST", &format!("{pr}/comments"), Some(json!({ "path": "a.txt", "side": "new", "line": 1, "body": "draft", "pending": true }))).await;
    assert!(tmp["id"].is_i64());
    assert_eq!(app.req(&carol, "DELETE", &format!("{pr}/reviews/pending"), None).await.0, StatusCode::OK);
    assert!(app.req(&carol, "GET", &format!("{pr}/reviews"), None).await.1["pending"].is_null());
    // Reviews: the author can't approve their own; the latest decision per reviewer counts.
    assert_eq!(app.req(&alice, "POST", &format!("{pr}/reviews"), Some(json!({ "event": "approve" }))).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(app.req(&bob, "POST", &format!("{pr}/reviews"), Some(json!({ "event": "comment" }))).await.0, StatusCode::OK, "the pending comment is enough");
    assert_eq!(threads(&app.req(&carol, "GET", &format!("{pr}/reviews"), None).await.1).len(), 2, "submitted comments are public");
    assert_eq!(app.req(&carol, "POST", &format!("{pr}/reviews"), Some(json!({ "event": "comment" }))).await.0, StatusCode::BAD_REQUEST, "an empty review");
    let (s, v) = app.req(&bob, "POST", &format!("{pr}/reviews"), Some(json!({ "event": "request_changes", "body": "Please don't shout" }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(app.req(&bob, "POST", &format!("{pr}/reviews"), Some(json!({ "event": "request_changes" }))).await.0, StatusCode::BAD_REQUEST, "say what to change");
    let (_, all) = app.req(&carol, "GET", &format!("{pr}/reviews"), None).await;
    assert_eq!(all["summary"]["changes_requested"], json!(["bob"]), "{all}");
    let (_, list) = app.req(&carol, "GET", "/api/repos/alice/r/issues?kind=pr&state=open", None).await;
    assert_eq!(list["items"][0]["review_state"], "changes_requested", "{list}");
    assert_eq!(app.req(&bob, "POST", &format!("{pr}/reviews"), Some(json!({ "event": "approve" }))).await.0, StatusCode::OK);
    let (_, all) = app.req(&carol, "GET", &format!("{pr}/reviews"), None).await;
    assert_eq!((all["summary"]["approved"].clone(), all["summary"]["changes_requested"].clone()), (json!(["bob"]), json!([])), "{all}");
    let (_, issue) = app.req(&carol, "GET", pr.replace("/pulls/", "/issues/").as_str(), None).await;
    assert_eq!(issue["issue"]["review_state"], "approved", "{issue}");

    // Replies join the thread; the PR's author, the thread's author and writers resolve it.
    assert_eq!(app.req(&alice, "POST", &format!("{pr}/comments"), Some(json!({ "in_reply_to": c1, "body": "For emphasis" }))).await.0, StatusCode::OK);
    assert_eq!(app.req(&carol, "POST", &format!("{pr}/comments/{c1}/resolve"), Some(json!({ "resolved": true }))).await.0, StatusCode::FORBIDDEN);
    assert_eq!(app.req(&alice, "POST", &format!("{pr}/comments/{c1}/resolve"), Some(json!({ "resolved": true }))).await.0, StatusCode::OK);
    let (_, all) = app.req(&carol, "GET", &format!("{pr}/reviews"), None).await;
    let t1 = threads(&all).into_iter().find(|t| t["id"] == c1).unwrap();
    assert_eq!((t1["comments"].as_array().unwrap().len(), t1["resolved_by"].as_str()), (2, Some("alice")), "{t1}");
    // Edits are the author's; deleting a thread's first comment takes the thread.
    assert_eq!(app.req(&carol, "PATCH", &format!("{pr}/comments/{c1}"), Some(json!({ "body": "pwned" }))).await.0, StatusCode::FORBIDDEN);
    assert_eq!(app.req(&bob, "PATCH", &format!("{pr}/comments/{c1}"), Some(json!({ "body": "Why shout, though?" }))).await.0, StatusCode::OK);

    // A push that changes a commented line makes that comment outdated; others stay put.
    put_file(&app, &alice, "feat", None, "a.txt", "one\nTwo!\nthree\nfour\n").await;
    let (_, all) = app.req(&carol, "GET", &format!("{pr}/reviews"), None).await;
    let by_line = |l: i64| threads(&all).into_iter().find(|t| t["line"] == l).unwrap();
    assert_eq!((by_line(2)["outdated"].as_bool(), by_line(4)["outdated"].as_bool()), (Some(true), Some(false)), "{all}");
    assert_eq!(app.req(&bob, "DELETE", &format!("{pr}/comments/{c1}"), None).await.0, StatusCode::OK);
    assert_eq!(threads(&app.req(&carol, "GET", &format!("{pr}/reviews"), None).await.1).len(), 1);
}

#[tokio::test]
async fn review_suggestions_apply_as_commits() {
    let app = App::new();
    let alice = app.signup("alice").await;
    let bob = app.signup("bob").await;
    let carol = app.signup("carol").await;
    review_fixture(&app, &alice).await;
    app.req(&alice, "PUT", "/api/repos/alice/r/collaborators/carol", Some(json!({ "role": "write" }))).await;
    let pr = "/api/repos/alice/r/pulls/1";
    let suggest = |line: i64, text: &str| json!({ "path": "a.txt", "side": "new", "line": line, "body": format!("Try:\n```suggestion\n{text}\n```") });
    let (_, s1) = app.req(&bob, "POST", &format!("{pr}/comments"), Some(suggest(2, "Two"))).await;
    let s1 = s1["id"].as_i64().unwrap();
    let (_, s2) = app.req(&bob, "POST", &format!("{pr}/comments"), Some(suggest(3, "Three"))).await;
    let s2 = s2["id"].as_i64().unwrap();
    let (_, plain) = app.req(&bob, "POST", &format!("{pr}/comments"), Some(json!({ "path": "a.txt", "side": "new", "line": 1, "body": "no suggestion" }))).await;

    // What the applier saw: the comment's updated_at, so an edit since is refused.
    let seen = |id: i64| {
        let app = &app;
        let carol = carol.clone();
        async move {
            let (_, all) = app.req(&carol, "GET", "/api/repos/alice/r/pulls/1/reviews", None).await;
            let t = threads(&all).into_iter().find(|t| t["id"] == id).unwrap();
            json!({ "updated_at": t["comments"][0]["updated_at"] })
        }
    };
    // Only people who can push to the head branch apply suggestions.
    assert_eq!(app.req(&bob, "POST", &format!("{pr}/comments/{s1}/apply"), Some(seen(s1).await)).await.0, StatusCode::FORBIDDEN);
    let pid = plain["id"].as_i64().unwrap();
    assert_eq!(app.req(&carol, "POST", &format!("{pr}/comments/{pid}/apply"), Some(seen(pid).await)).await.0, StatusCode::BAD_REQUEST, "nothing to apply");
    assert_eq!(app.req(&carol, "POST", &format!("{pr}/comments/{s1}/apply"), Some(json!({ "updated_at": 1 }))).await.0, StatusCode::CONFLICT, "edited since it was read");
    let (s, v) = app.req(&carol, "POST", &format!("{pr}/comments/{s1}/apply"), Some(seen(s1).await)).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let (_, text, _) = raw_get(&app, "/raw/alice/r/feat/a.txt").await;
    assert_eq!(text, "one\nTwo\nthree\nfour\n");
    let (_, all) = app.req(&bob, "GET", &format!("{pr}/reviews"), None).await;
    let t = threads(&all).into_iter().find(|t| t["id"] == s1).unwrap();
    assert_eq!(t["resolved_by"], "carol", "applying resolves the thread: {t}");
    assert_eq!(app.req(&carol, "POST", &format!("{pr}/comments/{s1}/apply"), Some(seen(s1).await)).await.0, StatusCode::CONFLICT, "applied already");
    // A suggestion whose lines changed since is refused rather than applied on top.
    let s2_seen = seen(s2).await;
    put_file(&app, &alice, "feat", None, "a.txt", "one\nTwo\n3\nfour\n").await;
    assert_eq!(app.req(&carol, "POST", &format!("{pr}/comments/{s2}/apply"), Some(s2_seen)).await.0, StatusCode::CONFLICT);
    // Only the suggested lines change: a CRLF file keeps its line endings.
    put_file(&app, &alice, "feat", None, "w.txt", "x\r\ny\r\nz\r\n").await;
    let (s, w) = app.req(&bob, "POST", &format!("{pr}/comments"), Some(json!({ "path": "w.txt", "side": "new", "line": 2, "body": "```suggestion\nY\n```" }))).await;
    assert_eq!(s, StatusCode::OK, "{w}");
    let wid = w["id"].as_i64().unwrap();
    assert_eq!(app.req(&carol, "POST", &format!("{pr}/comments/{wid}/apply"), Some(seen(wid).await)).await.0, StatusCode::OK);
    let (_, text, _) = raw_get(&app, "/raw/alice/r/feat/w.txt").await;
    assert_eq!(text, "x\r\nY\r\nz\r\n");
}
