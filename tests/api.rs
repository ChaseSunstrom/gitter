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

    // Deleting the repository frees its LFS files right away.
    let (s, _) = app.req(&bob, "DELETE", "/api/repos/bob/r", Some(json!({ "confirm": "bob/r" }))).await;
    assert_eq!(s, StatusCode::OK);
    assert!(!kept.exists(), "no repository holds it any more");
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
