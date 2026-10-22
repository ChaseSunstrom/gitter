//! Signing in through GitHub, GitLab and OpenID Connect providers, against a
//! mock provider that checks PKCE and the client secret like a real one.
mod common;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::Json;
use axum::body::Body;
use axum::extract::{Form, State};
use axum::http::{HeaderMap, Request, StatusCode};
use axum::routing::{get, post};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use common::{App, Client, ORIGIN};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tower::ServiceExt;

const SECRET: &str = "s3cret-value"; // pragma: allowlist secret

#[derive(Clone, Default)]
struct Who {
    sub: String,
    login: String,
    email: String,
    verified: bool,
}

fn who(sub: &str, login: &str, email: &str, verified: bool) -> Who {
    Who { sub: sub.into(), login: login.into(), email: email.into(), verified }
}

#[derive(Default)]
struct Mock {
    base: String,
    /// code -> (who, code_challenge, nonce)
    codes: HashMap<String, (Who, String, String)>,
    tokens: HashMap<String, Who>,
    /// Overrides for the next ID token's claims.
    claims: Value,
    /// A different `sub` from userinfo than in the ID token.
    userinfo_sub: Option<String>,
    issued: usize,
}

type M = Arc<Mutex<Mock>>;

async fn mock() -> M {
    let m: M = Arc::default();
    let app = axum::Router::new()
        .route("/.well-known/openid-configuration", get(discovery))
        .route("/token", post(token))
        .route("/login/oauth/access_token", post(token))
        .route("/oauth/token", post(token))
        .route("/userinfo", get(userinfo))
        .route("/api/v3/user", get(gh_user))
        .route("/api/v3/user/emails", get(gh_emails))
        .route("/api/v4/user", get(gl_user))
        .with_state(m.clone());
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    m.lock().unwrap().base = format!("http://{}", l.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    m
}

async fn discovery(State(m): State<M>) -> Json<Value> {
    let b = m.lock().unwrap().base.clone();
    Json(json!({
        "issuer": b,
        "authorization_endpoint": format!("{b}/authorize"),
        "token_endpoint": format!("{b}/token"),
        "userinfo_endpoint": format!("{b}/userinfo"),
    }))
}

fn b64(v: &Value) -> String {
    URL_SAFE_NO_PAD.encode(v.to_string())
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64
}

async fn token(State(m): State<M>, Form(f): Form<HashMap<String, String>>) -> (StatusCode, Json<Value>) {
    let mut g = m.lock().unwrap();
    let bad = (StatusCode::BAD_REQUEST, Json(json!({ "error": "invalid_grant" })));
    let Some((who, challenge, nonce)) = f.get("code").and_then(|c| g.codes.remove(c)) else { return bad };
    let field = |k: &str| f.get(k).map(String::as_str).unwrap_or_default();
    let ok = field("client_id") == "cid"
        && field("client_secret") == SECRET
        && field("grant_type") == "authorization_code"
        && field("redirect_uri") == format!("{ORIGIN}/api/auth/oauth/callback")
        && URL_SAFE_NO_PAD.encode(Sha256::digest(field("code_verifier").as_bytes())) == challenge;
    if !ok {
        return bad;
    }
    g.issued += 1;
    let at = format!("at-{}", g.issued);
    g.tokens.insert(at.clone(), who.clone());
    let mut claims = json!({ "iss": g.base, "aud": "cid", "sub": who.sub, "exp": now() + 300, "iat": now(), "auth_time": now(), "nonce": nonce });
    for (k, v) in g.claims.as_object().cloned().unwrap_or_default() {
        claims[k] = v;
    }
    let id_token = format!("{}.{}.c2ln", b64(&json!({ "alg": "RS256" })), b64(&claims));
    (StatusCode::OK, Json(json!({ "access_token": at, "token_type": "bearer", "id_token": id_token })))
}

fn bearer(m: &M, h: &HeaderMap) -> Option<Who> {
    let t = h.get("authorization")?.to_str().ok()?.strip_prefix("Bearer ")?.to_string();
    m.lock().unwrap().tokens.get(&t).cloned()
}

async fn userinfo(State(m): State<M>, h: HeaderMap) -> (StatusCode, Json<Value>) {
    let Some(w) = bearer(&m, &h) else { return (StatusCode::UNAUTHORIZED, Json(json!({}))) };
    let sub = m.lock().unwrap().userinfo_sub.clone().unwrap_or(w.sub);
    (StatusCode::OK, Json(json!({ "sub": sub, "preferred_username": w.login, "email": w.email, "email_verified": w.verified })))
}

async fn gh_user(State(m): State<M>, h: HeaderMap) -> (StatusCode, Json<Value>) {
    let Some(w) = bearer(&m, &h) else { return (StatusCode::UNAUTHORIZED, Json(json!({}))) };
    (StatusCode::OK, Json(json!({ "id": w.sub.parse::<i64>().unwrap(), "login": w.login, "email": null })))
}

async fn gh_emails(State(m): State<M>, h: HeaderMap) -> (StatusCode, Json<Value>) {
    let Some(w) = bearer(&m, &h) else { return (StatusCode::UNAUTHORIZED, Json(json!({}))) };
    let other = json!({ "email": "secondary@gh.test", "primary": false, "verified": true });
    (StatusCode::OK, Json(json!([other, { "email": w.email, "primary": true, "verified": w.verified }])))
}

async fn gl_user(State(m): State<M>, h: HeaderMap) -> (StatusCode, Json<Value>) {
    let Some(w) = bearer(&m, &h) else { return (StatusCode::UNAUTHORIZED, Json(json!({}))) };
    let confirmed = w.verified.then_some("2020-01-01T00:00:00Z");
    (StatusCode::OK, Json(json!({ "id": w.sub.parse::<i64>().unwrap(), "username": w.login, "email": w.email, "confirmed_at": confirmed })))
}

// ---------- driving the app like a browser ----------

#[derive(Debug)]
struct Resp {
    status: StatusCode,
    json: Value,
    cookies: Vec<String>,
    location: String,
}

impl Resp {
    fn cookie(&self, name: &str) -> Option<String> {
        self.cookies.iter().find_map(|c| c.split(';').next()?.strip_prefix(&format!("{name}=")).map(String::from)).filter(|v| !v.is_empty())
    }
}

async fn raw(app: &App, method: &str, path: &str, cookies: &[String], body: Option<Value>) -> Resp {
    let mut b = Request::builder().method(method).uri(path).header("host", "test.local").header("origin", ORIGIN);
    if !cookies.is_empty() {
        b = b.header("cookie", cookies.join("; "));
    }
    let req = match body {
        Some(v) => b.header("content-type", "application/json").body(Body::from(v.to_string())).unwrap(),
        None => b.body(Body::empty()).unwrap(),
    };
    let resp = app.router.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let h = resp.headers().clone();
    let cookies = h.get_all("set-cookie").iter().filter_map(|v| v.to_str().ok().map(String::from)).collect();
    let location = h.get("location").and_then(|v| v.to_str().ok()).unwrap_or_default().to_string();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    Resp { status, json: serde_json::from_slice(&bytes).unwrap_or(Value::Null), cookies, location }
}

fn session(c: &Client) -> Vec<String> {
    c.0.iter().map(|s| format!("gitter_session={s}")).collect()
}

fn dec(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => {
                out.push(u8::from_str_radix(&s[i + 1..i + 3], 16).unwrap());
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8(out).unwrap()
}

fn query(url: &str) -> HashMap<String, String> {
    let q = url.split_once('?').map(|x| x.1).unwrap_or_default();
    q.split('&').filter_map(|kv| kv.split_once('=')).map(|(k, v)| (dec(k), dec(v))).collect()
}

/// What a started sign-in hands the browser: the provider URL's query and the binding cookie
/// (`name=value`: each sign-in has its own, so two tabs don't trip over each other).
struct Started {
    q: HashMap<String, String>,
    url: String,
    binding: String,
}

#[allow(clippy::result_large_err)]
async fn start(app: &App, c: &Client, slug: &str, body: Value) -> Result<Started, Resp> {
    let r = raw(app, "POST", &format!("/api/auth/oauth/{slug}/start"), &session(c), Some(body)).await;
    if r.status != StatusCode::OK {
        return Err(r);
    }
    let url = r.json["url"].as_str().unwrap().to_string();
    let binding = r.cookies.iter().find_map(|c| c.split(';').next().filter(|kv| kv.starts_with("gitter_oauth_"))).expect("binding cookie").to_string();
    Ok(Started { q: query(&url), url, binding })
}

/// The provider approves `who` and sends the browser back with a code.
fn approve(m: &M, s: &Started, w: &Who) -> String {
    let mut g = m.lock().unwrap();
    let code = format!("code-{}", g.codes.len() + g.issued + 1);
    g.codes.insert(code.clone(), (w.clone(), s.q["code_challenge"].clone(), s.q.get("nonce").cloned().unwrap_or_default()));
    code
}

async fn callback(app: &App, c: &Client, binding: Option<&str>, code: &str, state: &str) -> Resp {
    let mut cookies = session(c);
    cookies.extend(binding.map(String::from));
    raw(app, "GET", &format!("/api/auth/oauth/callback?code={code}&state={state}"), &cookies, None).await
}

/// The whole round trip. The result redirects to where the browser lands.
async fn sign_in(app: &App, m: &M, c: &Client, slug: &str, body: Value, w: &Who) -> Resp {
    let s = start(app, c, slug, body).await.unwrap_or_else(|r| panic!("start: {} {}", r.status, r.json));
    let code = approve(m, &s, w);
    callback(app, c, Some(&s.binding), &code, &s.q["state"]).await
}

fn ok(r: &Resp) -> Client {
    assert!(r.status.is_redirection(), "{} {}", r.status, r.json);
    assert!(!r.location.contains("oauth_error"), "landed on {}", r.location);
    Client(r.cookie("gitter_session"))
}

fn refused(r: &Resp) -> String {
    assert!(r.status.is_redirection(), "{} {}", r.status, r.json);
    assert!(r.location.contains("oauth_error="), "landed on {}", r.location);
    assert_eq!(r.cookie("gitter_session"), None, "no session on a refused sign-in");
    query(&r.location)["oauth_error"].clone()
}

async fn provider(app: &App, admin: &Client, m: &M, kind: &str, slug: &str, extra: Value) -> Value {
    let base = m.lock().unwrap().base.clone();
    let mut body = json!({ "kind": kind, "slug": slug, "name": format!("Mock {kind}"), "base_url": base, "client_id": "cid", "client_secret": SECRET, "allow_private": true });
    for (k, v) in extra.as_object().cloned().unwrap_or_default() {
        body[k] = v;
    }
    let (s, v) = app.req(admin, "POST", "/api/admin/oauth-providers", Some(body)).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    v
}

async fn count(app: &App, sql: &'static str) -> i64 {
    app.state.db.call(move |c| Ok(c.query_row(sql, [], |r| r.get(0))?)).await.unwrap()
}

async fn me(app: &App, c: &Client) -> Value {
    app.req(c, "GET", "/api/me", None).await.1
}

// ---------- tests ----------

#[tokio::test]
async fn admins_manage_providers_and_the_client_secret_never_comes_back() {
    let app = App::new();
    let m = mock().await;
    let root = app.signup("root").await;
    let bob = app.signup("bob").await;
    let base = m.lock().unwrap().base.clone();
    let body = json!({ "kind": "oidc", "slug": "mock", "name": "Mock", "base_url": base, "client_id": "cid", "client_secret": SECRET, "allow_private": true });
    assert_eq!(app.req(&bob, "POST", "/api/admin/oauth-providers", Some(body.clone())).await.0, StatusCode::FORBIDDEN);
    for (k, v) in [("kind", json!("saml")), ("base_url", json!("javascript:alert(1)")), ("slug", json!("../x")), ("client_id", json!(""))] {
        let mut b = body.clone();
        b[k] = v;
        assert_eq!(app.req(&root, "POST", "/api/admin/oauth-providers", Some(b)).await.0, StatusCode::BAD_REQUEST, "{k}");
    }
    // Plain http only with private addresses allowed (a lab or test server).
    let mut b = body.clone();
    b["base_url"] = json!("http://idp.example.com");
    b["allow_private"] = json!(false);
    assert_eq!(app.req(&root, "POST", "/api/admin/oauth-providers", Some(b)).await.0, StatusCode::BAD_REQUEST);

    let created = provider(&app, &root, &m, "oidc", "mock", json!({})).await;
    assert_eq!(app.req(&root, "POST", "/api/admin/oauth-providers", Some(body)).await.0, StatusCode::CONFLICT);
    let (_, list) = app.req(&root, "GET", "/api/admin/oauth-providers", None).await;
    for v in [&created, &list] {
        assert!(!v.to_string().contains(SECRET), "secret leaked: {v}");
    }
    assert_eq!(list["providers"][0]["has_secret"], json!(true));
    assert_eq!(list["callback_url"], json!(format!("{ORIGIN}/api/auth/oauth/callback")));
    let stored: String = app.state.db.call(|c| Ok(c.query_row("SELECT secret FROM oauth_providers", [], |r| r.get(0))?)).await.unwrap();
    assert!(stored.starts_with("sealed:"));

    let (_, public) = app.req(&Client::default(), "GET", "/api/auth/providers", None).await;
    assert_eq!(public, json!({ "providers": [{ "slug": "mock", "name": "Mock oidc", "kind": "oidc" }] }));
    let id = created["id"].as_i64().unwrap();
    app.req(&root, "PATCH", &format!("/api/admin/oauth-providers/{id}"), Some(json!({ "enabled": false }))).await;
    assert_eq!(app.req(&Client::default(), "GET", "/api/auth/providers", None).await.1["providers"], json!([]));
    assert!(start(&app, &Client::default(), "mock", json!({ "mode": "login" })).await.is_err());
    // A new address or client ID needs the secret again, so it's never sent somewhere new.
    let path = format!("/api/admin/oauth-providers/{id}");
    assert_eq!(app.req(&root, "PATCH", &path, Some(json!({ "base_url": "https://elsewhere.example" }))).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(app.req(&root, "PATCH", &path, Some(json!({ "client_id": "other" }))).await.0, StatusCode::BAD_REQUEST);
    // An empty secret keeps the stored one: the token exchange still passes the mock's check.
    let (s, _) = app.req(&root, "PATCH", &format!("/api/admin/oauth-providers/{id}"), Some(json!({ "enabled": true, "client_secret": "" }))).await;
    assert_eq!(s, StatusCode::OK);
    ok(&sign_in(&app, &m, &Client::default(), "mock", json!({ "mode": "login" }), &who("u-1", "carol", "carol@idp.test", true)).await);
}

#[tokio::test]
async fn openid_connect_signs_up_then_signs_in_and_each_state_works_once() {
    let app = App::new();
    let m = mock().await;
    let root = app.signup("root").await;
    provider(&app, &root, &m, "oidc", "mock", json!({})).await;
    let carol = who("u-1", "carol", "carol@idp.test", true);
    let anon = Client::default();

    let r = sign_in(&app, &m, &anon, "mock", json!({ "mode": "login", "next": "/carol" }), &carol).await;
    let c = ok(&r);
    assert_eq!(r.location, "/carol");
    let v = me(&app, &c).await;
    assert_eq!((v["user"]["username"].as_str(), v["profile"]["email"].as_str()), (Some("carol"), Some("carol@idp.test")));
    // The provider was asked for a code with PKCE (S256) and, for OpenID Connect, a nonce.
    let s = start(&app, &anon, "mock", json!({ "mode": "login" })).await.unwrap();
    assert!(s.url.starts_with(&format!("{}/authorize?", m.lock().unwrap().base)));
    assert_eq!((s.q["code_challenge_method"].as_str(), s.q["response_type"].as_str()), ("S256", "code"));
    assert!(s.q["nonce"].len() >= 20 && s.q["state"].len() >= 20);

    // A state works once: the second callback with it is refused before any exchange.
    let code = approve(&m, &s, &carol);
    ok(&callback(&app, &anon, Some(&s.binding), &code, &s.q["state"]).await);
    let again = approve(&m, &s, &carol);
    refused(&callback(&app, &anon, Some(&s.binding), &again, &s.q["state"]).await);
    // The browser that started the sign-in must finish it (no cookie, or someone else's).
    let s = start(&app, &anon, "mock", json!({ "mode": "login" })).await.unwrap();
    let code = approve(&m, &s, &carol);
    refused(&callback(&app, &anon, None, &code, &s.q["state"]).await);
    let s = start(&app, &anon, "mock", json!({ "mode": "login" })).await.unwrap();
    let code = approve(&m, &s, &carol);
    let name = s.binding.split('=').next().unwrap();
    refused(&callback(&app, &anon, Some(&format!("{name}=someone-elses-cookie")), &code, &s.q["state"]).await);
    refused(&callback(&app, &anon, Some(&s.binding), &code, "made-up-state").await);
    // Two sign-ins started in one browser (two tabs) both finish.
    let (s1, s2) = (start(&app, &anon, "mock", json!({ "mode": "login" })).await.unwrap(), start(&app, &anon, "mock", json!({ "mode": "login" })).await.unwrap());
    let both = [s1.binding.clone(), s2.binding.clone()];
    let (c1, c2) = (approve(&m, &s1, &carol), approve(&m, &s2, &carol));
    for (code, st) in [(c2, &s2), (c1, &s1)] {
        ok(&raw(&app, "GET", &format!("/api/auth/oauth/callback?code={code}&state={}", st.q["state"]), &both, None).await);
    }
    // The provider saying no comes back as a message, not a session.
    let s = start(&app, &anon, "mock", json!({ "mode": "login" })).await.unwrap();
    let r = raw(&app, "GET", &format!("/api/auth/oauth/callback?error=access_denied&state={}", s.q["state"]), &[s.binding.clone()], None).await;
    refused(&r);

    // Signing in again finds the same account by the provider's id, not a new one.
    let c2 = ok(&sign_in(&app, &m, &anon, "mock", json!({ "mode": "login" }), &carol).await);
    assert_eq!(me(&app, &c2).await["user"]["username"], json!("carol"));
    assert_eq!(count(&app, "SELECT count(*) FROM users").await, 2);
    // An account made by a provider has no password to sign in with.
    for pw in ["", "!", "x"] {
        let (s, _) = app.req(&anon, "POST", "/api/auth/login", Some(json!({ "login": "carol", "password": pw }))).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
    }
    // Only same-site paths come back from `next`.
    for (next, lands) in [("//evil.example/x", "/"), ("https://evil.example", "/"), ("/\\evil.example", "/"), ("/ok?x=1", "/ok?x=1")] {
        let r = sign_in(&app, &m, &anon, "mock", json!({ "mode": "login", "next": next }), &carol).await;
        ok(&r);
        assert_eq!(r.location, lands, "{next}");
    }
    // A taken username gets a suffix instead.
    let c3 = ok(&sign_in(&app, &m, &anon, "mock", json!({ "mode": "login" }), &who("u-2", "root", "", false)).await);
    let name = me(&app, &c3).await["user"]["username"].as_str().unwrap().to_string();
    assert!(name.starts_with("root") && name != "root", "{name}");
}

#[tokio::test]
async fn id_token_claims_and_userinfo_must_match() {
    let app = App::new();
    let m = mock().await;
    let root = app.signup("root").await;
    provider(&app, &root, &m, "oidc", "mock", json!({})).await;
    let carol = who("u-1", "carol", "carol@idp.test", true);
    let cases: [(Value, Option<&str>); 6] = [
        (json!({ "nonce": "another-nonce" }), None),
        (json!({ "aud": "someone-else" }), None),
        (json!({ "aud": ["someone-else"] }), None),
        (json!({ "iss": "https://evil.example" }), None),
        (json!({ "exp": 1 }), None),
        (json!({}), Some("u-other")),
    ];
    for (claims, sub) in cases {
        {
            let mut g = m.lock().unwrap();
            g.claims = claims.clone();
            g.userinfo_sub = sub.map(String::from);
        }
        refused(&sign_in(&app, &m, &Client::default(), "mock", json!({ "mode": "login" }), &carol).await);
        assert_eq!(count(&app, "SELECT count(*) FROM users").await, 1, "{claims} {sub:?}");
    }
    {
        let mut g = m.lock().unwrap();
        g.claims = json!({ "aud": ["cid", "other"], "azp": "cid" });
        g.userinfo_sub = None;
    }
    ok(&sign_in(&app, &m, &Client::default(), "mock", json!({ "mode": "login" }), &carol).await);
}

#[tokio::test]
async fn only_verified_emails_are_kept_and_a_taken_email_never_links_or_signs_up() {
    let app = App::new();
    let m = mock().await;
    let root = app.signup("root").await;
    app.signup("alice").await;
    provider(&app, &root, &m, "oidc", "mock", json!({})).await;
    provider(&app, &root, &m, "github", "gh", json!({})).await;
    provider(&app, &root, &m, "gitlab", "gl", json!({})).await;
    let anon = Client::default();

    let why = refused(&sign_in(&app, &m, &anon, "mock", json!({ "mode": "login" }), &who("u-9", "alice2", "ALICE@example.com", true)).await);
    assert!(why.contains("already"), "{why}");
    assert_eq!(count(&app, "SELECT count(*) FROM identities").await, 0);
    assert_eq!(count(&app, "SELECT count(*) FROM users").await, 2);
    // An unverified address is dropped, so it can't collide or claim anything.
    let c = ok(&sign_in(&app, &m, &anon, "mock", json!({ "mode": "login" }), &who("u-10", "dave", "alice@example.com", false)).await);
    assert_eq!(me(&app, &c).await["profile"]["email"], json!(""));

    // GitHub: the primary address from /user/emails when it's verified.
    let c = ok(&sign_in(&app, &m, &anon, "gh", json!({ "mode": "login" }), &who("42", "erin", "erin@gh.test", true)).await);
    let v = me(&app, &c).await;
    assert_eq!((v["user"]["username"].as_str(), v["profile"]["email"].as_str()), (Some("erin"), Some("erin@gh.test")));
    let c = ok(&sign_in(&app, &m, &anon, "gh", json!({ "mode": "login" }), &who("43", "fay", "fay@gh.test", false)).await);
    assert_eq!(me(&app, &c).await["profile"]["email"], json!(""));
    // GitLab: the address once it's confirmed.
    let c = ok(&sign_in(&app, &m, &anon, "gl", json!({ "mode": "login" }), &who("44", "gina", "gina@gl.test", true)).await);
    assert_eq!(me(&app, &c).await["profile"]["email"], json!("gina@gl.test"));
    let c = ok(&sign_in(&app, &m, &anon, "gl", json!({ "mode": "login" }), &who("45", "hal", "hal@gl.test", false)).await);
    assert_eq!(me(&app, &c).await["profile"]["email"], json!(""));
    // The same numeric id on two providers is two different people.
    let c = ok(&sign_in(&app, &m, &anon, "gl", json!({ "mode": "login" }), &who("42", "ivy", "", false)).await);
    assert_eq!(me(&app, &c).await["user"]["username"], json!("ivy"));
}

#[tokio::test]
async fn sign_up_needs_both_the_provider_and_the_instance_to_allow_it() {
    let app = App::new();
    let m = mock().await;
    let root = app.signup("root").await;
    let p = provider(&app, &root, &m, "oidc", "mock", json!({ "allow_signup": false })).await;
    let anon = Client::default();
    let carol = who("u-1", "carol", "carol@idp.test", true);
    refused(&sign_in(&app, &m, &anon, "mock", json!({ "mode": "login" }), &carol).await);
    assert_eq!(count(&app, "SELECT count(*) FROM users").await, 1);
    app.req(&root, "PATCH", &format!("/api/admin/oauth-providers/{}", p["id"]), Some(json!({ "allow_signup": true }))).await;
    ok(&sign_in(&app, &m, &anon, "mock", json!({ "mode": "login" }), &carol).await);

    let (_, mut cur) = app.req(&root, "GET", "/api/admin/settings", None).await;
    cur["signup"] = json!("closed");
    assert_eq!(app.req(&root, "PUT", "/api/admin/settings", Some(cur.clone())).await.0, StatusCode::OK);
    refused(&sign_in(&app, &m, &anon, "mock", json!({ "mode": "login" }), &who("u-2", "dan", "", false)).await);
    // Someone who already has an account still signs in.
    ok(&sign_in(&app, &m, &anon, "mock", json!({ "mode": "login" }), &carol).await);

    // Invite-only: the invite from the sign-up link is used up like a password sign-up's.
    cur["signup"] = json!("invite");
    app.req(&root, "PUT", "/api/admin/settings", Some(cur)).await;
    let (_, inv) = app.req(&root, "POST", "/api/admin/invites", Some(json!({ "max_uses": 1 }))).await;
    let code = inv["code"].as_str().unwrap().to_string();
    refused(&sign_in(&app, &m, &anon, "mock", json!({ "mode": "login" }), &who("u-3", "eve", "", false)).await);
    ok(&sign_in(&app, &m, &anon, "mock", json!({ "mode": "login", "invite": code }), &who("u-3", "eve", "", false)).await);
    refused(&sign_in(&app, &m, &anon, "mock", json!({ "mode": "login", "invite": code }), &who("u-4", "fin", "", false)).await);
}

#[tokio::test]
async fn two_factor_is_still_asked_for_after_the_provider() {
    use gitter::security::{step_now, totp_code};
    let app = App::new();
    let m = mock().await;
    let root = app.signup("root").await;
    provider(&app, &root, &m, "oidc", "mock", json!({})).await;
    let alice = app.signup("alice").await;
    let (_, setup) = app.req(&alice, "POST", "/api/me/2fa/setup", None).await;
    let secret = setup["secret"].as_str().unwrap().to_string();
    let now = step_now();
    app.req(&alice, "POST", "/api/me/2fa/enable", Some(json!({ "code": totp_code(&secret, now).unwrap() }))).await;
    let id = who("u-7", "alice-idp", "", false);
    let r = sign_in(&app, &m, &alice, "mock", json!({ "mode": "link", "next": "/settings/security" }), &id).await;
    ok(&r);
    assert_eq!(r.location, "/settings/security");

    let r = sign_in(&app, &m, &Client::default(), "mock", json!({ "mode": "login", "next": "/alice" }), &id).await;
    assert!(r.status.is_redirection() && r.cookie("gitter_session").is_none(), "{} {:?}", r.location, r.cookies);
    let q = query(&r.location);
    assert!(r.location.starts_with("/login?"), "{}", r.location);
    assert_eq!(q["next"], "/alice");
    let body = json!({ "challenge": q["challenge"], "code": totp_code(&secret, now + 1).unwrap() });
    let (s, v, cookie) = app.call(&Client::default(), "POST", "/api/auth/login/2fa", Some(body), Some(ORIGIN)).await;
    assert_eq!((s, v["user"]["username"].as_str()), (StatusCode::OK, Some("alice")));
    assert!(cookie.is_some());
}

#[tokio::test]
async fn linking_unlinking_and_the_last_way_to_sign_in() {
    let app = App::new();
    let m = mock().await;
    let root = app.signup("root").await;
    provider(&app, &root, &m, "oidc", "mock", json!({})).await;
    let gh = provider(&app, &root, &m, "github", "gh", json!({})).await;
    let anon = Client::default();
    let carol = ok(&sign_in(&app, &m, &anon, "mock", json!({ "mode": "login" }), &who("u-1", "carol", "", false)).await);

    let (_, ids) = app.req(&carol, "GET", "/api/me/identities", None).await;
    assert_eq!(ids["has_password"], json!(false));
    assert_eq!(ids["identities"].as_array().unwrap().len(), 1);
    assert_eq!(ids["identities"][0]["provider"]["slug"], json!("mock"));
    let first = ids["identities"][0]["id"].as_i64().unwrap();
    let (s, e) = app.req(&carol, "DELETE", &format!("/api/me/identities/{first}"), None).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{e}");

    // Linking needs a signed-in, recently confirmed session.
    assert!(start(&app, &anon, "gh", json!({ "mode": "link" })).await.is_err());
    ok(&sign_in(&app, &m, &carol, "gh", json!({ "mode": "link" }), &who("77", "carol-gh", "", false)).await);
    let (_, ids) = app.req(&carol, "GET", "/api/me/identities", None).await;
    assert_eq!(ids["identities"].as_array().unwrap().len(), 2);
    assert_eq!(app.req(&carol, "DELETE", &format!("/api/me/identities/{first}"), None).await.0, StatusCode::OK);
    let gh_id = app.req(&carol, "GET", "/api/me/identities", None).await.1["identities"][0]["id"].as_i64().unwrap();
    assert_eq!(app.req(&carol, "DELETE", &format!("/api/me/identities/{gh_id}"), None).await.0, StatusCode::BAD_REQUEST);
    // Signing in with the linked account finds carol.
    let c = ok(&sign_in(&app, &m, &anon, "gh", json!({ "mode": "login" }), &who("77", "carol-gh", "", false)).await);
    assert_eq!(me(&app, &c).await["user"]["username"], json!("carol"));

    // One provider account belongs to one person.
    let bob = app.signup("bob").await;
    let why = refused(&sign_in(&app, &m, &bob, "gh", json!({ "mode": "link" }), &who("77", "carol-gh", "", false)).await);
    assert!(why.contains("another"), "{why}");

    // Deleting the provider would lock carol out, so it's refused until she has a password.
    let gh_path = format!("/api/admin/oauth-providers/{}", gh["id"]);
    assert_eq!(app.req(&root, "DELETE", &gh_path, None).await.0, StatusCode::CONFLICT);
    let (s, e) = app.req(&carol, "POST", "/api/me/password", Some(json!({ "current": "", "new": "a new horse" }))).await;
    assert_eq!(s, StatusCode::OK, "{e}");
    let (s, _) = app.req(&anon, "POST", "/api/auth/login", Some(json!({ "login": "carol", "password": "a new horse" }))).await;
    assert_eq!(s, StatusCode::OK);
    // A password change signs out other sessions and hands this browser a new one.
    let carol = app.call(&anon, "POST", "/api/auth/login", Some(json!({ "login": "carol", "password": "a new horse" })), Some(ORIGIN)).await.2.map(|c| Client(Some(c))).unwrap();
    // Now that she has a password, changing it needs the current one.
    let (s, _) = app.req(&carol, "POST", "/api/me/password", Some(json!({ "current": "", "new": "third horse" }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    // A disabled provider doesn't count as a way in, but a password does.
    app.req(&root, "PATCH", &gh_path, Some(json!({ "enabled": false }))).await;
    let gh_id = app.req(&carol, "GET", "/api/me/identities", None).await.1["identities"][0]["id"].as_i64().unwrap();
    assert_eq!(app.req(&carol, "DELETE", &format!("/api/me/identities/{gh_id}"), None).await.0, StatusCode::OK);
    assert_eq!(app.req(&root, "DELETE", &gh_path, None).await.0, StatusCode::OK);
}

#[tokio::test]
async fn an_account_without_a_password_confirms_through_its_provider() {
    let app = App::new();
    let m = mock().await;
    let root = app.signup("root").await;
    provider(&app, &root, &m, "oidc", "mock", json!({})).await;
    provider(&app, &root, &m, "github", "gh", json!({})).await;
    let anon = Client::default();
    let me_id = who("u-1", "carol", "", false);
    let carol = ok(&sign_in(&app, &m, &anon, "mock", json!({ "mode": "login" }), &me_id).await);
    app.state.db.call(|c| Ok(c.execute("UPDATE sessions SET verified_at = 0", [])?)).await.unwrap();
    let r = start(&app, &carol, "gh", json!({ "mode": "link" })).await.err().unwrap();
    assert_eq!((r.status, r.json["code"].as_str()), (StatusCode::FORBIDDEN, Some("reauth")));
    // Someone else's provider account doesn't confirm carol.
    refused(&sign_in(&app, &m, &carol, "mock", json!({ "mode": "reauth" }), &who("u-2", "mallory", "", false)).await);
    // OpenID Connect is asked to sign the person in again, and must say it did just now.
    let s = start(&app, &carol, "mock", json!({ "mode": "reauth" })).await.unwrap();
    assert_eq!((s.q["prompt"].as_str(), s.q["max_age"].as_str()), ("login", "0"));
    for auth_time in [json!(null), json!(1)] {
        m.lock().unwrap().claims = json!({ "auth_time": auth_time });
        refused(&sign_in(&app, &m, &carol, "mock", json!({ "mode": "reauth" }), &me_id).await);
    }
    m.lock().unwrap().claims = json!({});
    let r = sign_in(&app, &m, &carol, "mock", json!({ "mode": "reauth", "next": "/settings/security" }), &me_id).await;
    ok(&r);
    assert_eq!(r.location, "/settings/security");
    assert!(start(&app, &carol, "gh", json!({ "mode": "link" })).await.is_ok());

    // An account with a password confirms with it, not through a provider.
    let alice = app.signup("alice").await;
    ok(&sign_in(&app, &m, &alice, "mock", json!({ "mode": "link" }), &who("u-5", "alice-idp", "", false)).await);
    let r = start(&app, &alice, "mock", json!({ "mode": "reauth" })).await.err().expect("refused");
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn two_factor_accounts_confirm_through_the_provider_and_then_the_code() {
    use gitter::security::{step_now, totp_code};
    let app = App::new();
    let m = mock().await;
    let root = app.signup("root").await;
    provider(&app, &root, &m, "oidc", "mock", json!({})).await;
    let me_id = who("u-1", "carol", "", false);
    let carol = ok(&sign_in(&app, &m, &Client::default(), "mock", json!({ "mode": "login" }), &me_id).await);
    let (_, setup) = app.req(&carol, "POST", "/api/me/2fa/setup", None).await;
    let secret = setup["secret"].as_str().unwrap().to_string();
    let now = step_now();
    assert_eq!(app.req(&carol, "POST", "/api/me/2fa/enable", Some(json!({ "code": totp_code(&secret, now).unwrap() }))).await.0, StatusCode::OK);
    app.state.db.call(|c| Ok(c.execute("UPDATE sessions SET verified_at = 0", [])?)).await.unwrap();

    let r = sign_in(&app, &m, &carol, "mock", json!({ "mode": "reauth", "next": "/settings/security" }), &me_id).await;
    ok(&r);
    let q = query(&r.location);
    assert!(r.location.starts_with("/settings/security?"), "{}", r.location);
    let challenge = q["reauth_challenge"].clone();
    // Still not confirmed: the code is missing.
    assert_eq!(start(&app, &carol, "mock", json!({ "mode": "link" })).await.err().map(|r| r.status), Some(StatusCode::FORBIDDEN));
    let (s, e) = app.req(&carol, "POST", "/api/auth/reauth", Some(json!({ "challenge": challenge }))).await;
    assert_eq!((s, e["code"].as_str()), (StatusCode::UNAUTHORIZED, Some("need_code")));
    let (s, _) = app.req(&carol, "POST", "/api/auth/reauth", Some(json!({ "challenge": "made-up", "code": totp_code(&secret, now + 1).unwrap() }))).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    // Someone else's challenge doesn't confirm carol.
    let bob = app.signup("bob").await;
    let (s, _) = app.req(&bob, "POST", "/api/auth/reauth", Some(json!({ "challenge": challenge, "code": totp_code(&secret, now + 1).unwrap() }))).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, e) = app.req(&carol, "POST", "/api/auth/reauth", Some(json!({ "challenge": challenge, "code": totp_code(&secret, now + 1).unwrap() }))).await;
    assert_eq!(s, StatusCode::OK, "{e}");
    assert!(start(&app, &carol, "mock", json!({ "mode": "link" })).await.is_ok());
}

#[tokio::test]
async fn suspended_accounts_and_private_addresses_are_refused() {
    let app = App::new();
    let m = mock().await;
    let root = app.signup("root").await;
    provider(&app, &root, &m, "oidc", "mock", json!({})).await;
    let carol = who("u-1", "carol", "", false);
    ok(&sign_in(&app, &m, &Client::default(), "mock", json!({ "mode": "login" }), &carol).await);
    app.state.db.call(|c| Ok(c.execute("UPDATE users SET suspended = 1 WHERE username = 'carol'", [])?)).await.unwrap();
    refused(&sign_in(&app, &m, &Client::default(), "mock", json!({ "mode": "login" }), &carol).await);
    assert_eq!(count(&app, "SELECT count(*) FROM users").await, 2, "no second account for a suspended person");

    // Without the admin's say-so, a provider on this machine is never contacted.
    let lan = json!({ "base_url": "https://localhost:9", "allow_private": false });
    provider(&app, &root, &m, "github", "lan", lan.clone()).await;
    let why = refused(&sign_in(&app, &m, &Client::default(), "lan", json!({ "mode": "login" }), &who("5", "x", "", false)).await);
    assert!(why.contains("public address"), "{why}");
    provider(&app, &root, &m, "oidc", "lan-id", lan).await;
    let r = start(&app, &Client::default(), "lan-id", json!({ "mode": "login" })).await.err().expect("refused");
    assert_eq!(r.status, StatusCode::BAD_GATEWAY);
}

#[tokio::test]
async fn provider_sign_ups_have_their_own_budget_per_address() {
    let app = App::new();
    let m = mock().await;
    let root = app.signup("root").await;
    provider(&app, &root, &m, "oidc", "mock", json!({})).await;
    for n in 0..10 {
        ok(&sign_in(&app, &m, &Client::default(), "mock", json!({ "mode": "login" }), &who(&format!("u-{n}"), &format!("p{n}"), "", false)).await);
    }
    let why = refused(&sign_in(&app, &m, &Client::default(), "mock", json!({ "mode": "login" }), &who("u-99", "late", "", false)).await);
    assert!(why.contains("Too many"), "{why}");
    // Signing in to an existing account isn't a sign-up.
    ok(&sign_in(&app, &m, &Client::default(), "mock", json!({ "mode": "login" }), &who("u-1", "p1", "", false)).await);
}
