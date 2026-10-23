//! Two Gitter servers working together: a fork on one of another's repository, and pull
//! requests from it, signed by the server they come from.
mod common;

use axum::http::StatusCode;
use common::{App, Client};
use serde_json::{Value, json};
use tower::ServiceExt;

/// A server listening on its own address, federating with others on this machine.
async fn server(federate: bool) -> (App, String) {
    server_with(federate, true).await
}

async fn server_with(federate: bool, private: bool) -> (App, String) {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    let app = App::with_url(&url);
    let mut s = app.state.settings.get();
    s.federation = federate;
    s.federation_private = private;
    app.state.settings.save(&app.state.db, s).await.unwrap();
    let router = app.router.clone();
    tokio::spawn(async move {
        let _ = axum::serve(l, router.into_make_service_with_connect_info::<std::net::SocketAddr>()).await;
    });
    (app, url)
}

async fn commit(app: &App, c: &Client, repo: &str, branch: &str, path: &str, content: &str) -> String {
    let body = json!({ "branch": branch, "message": format!("Change {path}"), "changes": [{ "path": path, "content": content }] });
    let (s, v) = app.req(c, "POST", &format!("/api/repos/{repo}/contents"), Some(body)).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    v["sha"].as_str().unwrap().to_string()
}

/// A request to `app` signed as server `from` (whose key is `key_of`'s), sent as is.
async fn signed(app: &App, key_of: &App, from: &str, path: &str, body: &Value, ts: i64) -> (StatusCode, Value) {
    signed_for(app, &app.state.cfg.url, key_of, from, path, body, ts).await
}

/// The same, signed as if for server `to`.
async fn signed_for(app: &App, to: &str, key_of: &App, from: &str, path: &str, body: &Value, ts: i64) -> (StatusCode, Value) {
    let bytes = body.to_string();
    let k = gitter::federation::key(&key_of.state.cfg.data_dir).unwrap();
    let h = gitter::federation::sign(&k, from, to, "POST", path, ts, bytes.as_bytes());
    raw(app, path, &bytes, &h).await
}

async fn raw(app: &App, path: &str, body: &str, header: &str) -> (StatusCode, Value) {
    use http_body_util::BodyExt;
    let req = axum::http::Request::post(path)
        .header("host", "test.local")
        .header("content-type", "application/json")
        .header(gitter::federation::HEADER, header)
        .body(axum::body::Body::from(body.to_string()))
        .unwrap();
    let resp = app.router.clone().oneshot(req).await.unwrap();
    let s = resp.status();
    let b = resp.into_body().collect().await.unwrap().to_bytes();
    (s, serde_json::from_slice(&b).unwrap_or(Value::Null))
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64
}

/// alice/notes (public) on A, and bob's fork of it on B.
async fn forked() -> (App, String, Client, App, String, Client) {
    let (a, a_url) = server(true).await;
    let (b, b_url) = server(true).await;
    a.signup("root").await;
    let alice = a.signup("alice").await;
    assert_eq!(a.req(&alice, "POST", "/api/repos", Some(json!({ "name": "notes", "readme": true }))).await.0, StatusCode::OK);
    b.signup("root").await;
    let bob = b.signup("bob").await;
    assert_eq!(b.req(&bob, "POST", "/api/repos", Some(json!({ "name": "notes" }))).await.0, StatusCode::OK);
    // Checked first (the new repository page does, before making the repository).
    let (s, v) = b.req(&bob, "POST", "/api/federation/check", Some(json!({ "url": format!("{a_url}/alice/nope") }))).await;
    assert_eq!(s, StatusCode::NOT_FOUND, "{v}");
    let (s, v) = b.req(&bob, "POST", "/api/federation/check", Some(json!({ "url": format!("{a_url}/alice/notes") }))).await;
    assert_eq!((s, v["name"].as_str()), (StatusCode::OK, Some("notes")), "{v}");
    let (s, v) = b.req(&bob, "POST", "/api/repos/bob/notes/federation/fork", Some(json!({ "url": format!("{a_url}/alice/notes") }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(gitter::mirrors::tick(&b.state).await, 1);
    (a, a_url, alice, b, b_url, bob)
}

#[tokio::test]
async fn a_fork_on_another_server_opens_a_pull_request_that_follows_its_pushes() {
    let (a, a_url, alice, b, b_url, bob) = forked().await;
    // Each server says who it is.
    let (s, me) = a.req(&Client::default(), "GET", "/.well-known/gitter", None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!((me["url"].as_str(), me["federation"].as_bool()), (Some(a_url.as_str()), Some(true)), "{me}");
    assert!(gitter::federation::parse_key(me["public_key"].as_str().unwrap()).is_some());
    // The fork has A's files and knows where it came from.
    let (_, readme) = b.req(&bob, "GET", "/api/repos/bob/notes/blob?rev=main&path=README.md", None).await;
    assert!(readme["content"].as_str().is_some_and(|c| c.contains("notes")), "{readme}");
    let (_, repo) = b.req(&bob, "GET", "/api/repos/bob/notes", None).await;
    assert_eq!(repo["remote_fork"]["url"].as_str(), Some(format!("{a_url}/alice/notes").as_str()), "{repo}");

    // bob proposes a branch to alice/notes on A.
    commit(&b, &bob, "bob/notes", "main", "idea.md", "an idea\n").await;
    let body = json!({ "branch": "main", "base": "main", "title": "An idea from B", "body": "Have a look" });
    let (s, v) = b.req(&bob, "POST", "/api/repos/bob/notes/federation/pulls", Some(body)).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["number"].as_i64(), Some(1));
    assert!(v["url"].as_str().unwrap().starts_with(&a_url), "{v}");
    let (_, pr) = a.req(&alice, "GET", "/api/repos/alice/notes/issues/1", None).await;
    let host = gitter::federation::host(&b_url);
    assert_eq!(pr["issue"]["author"].as_str(), Some(format!("bob@{host}").as_str()), "{pr}");
    assert_eq!(pr["issue"]["author_url"].as_str(), Some(format!("{b_url}/bob").as_str()), "{pr}");
    assert_eq!(pr["issue"]["head_repo"].as_str(), Some(format!("{b_url}/bob/notes").as_str()), "{pr}");
    let (_, diff) = a.req(&alice, "GET", "/api/repos/alice/notes/pulls/1/diff", None).await;
    assert!(diff.to_string().contains("idea.md"), "{diff}");
    // B lists it.
    let (_, out) = b.req(&bob, "GET", "/api/repos/bob/notes/federation/pulls", None).await;
    assert_eq!(out["pulls"][0]["number"].as_i64(), Some(1), "{out}");
    // The same branch again is the same pull request.
    let again = json!({ "branch": "main", "base": "main", "title": "Again", "body": "" });
    let (_, v) = b.req(&bob, "POST", "/api/repos/bob/notes/federation/pulls", Some(again)).await;
    assert_eq!(v["number"].as_i64(), Some(1), "{v}");

    // A push to the branch on B reaches the pull request on A.
    let second = commit(&b, &bob, "bob/notes", "main", "idea.md", "a better idea\n").await;
    let mut head = String::new();
    for _ in 0..50 {
        let (_, pr) = a.req(&alice, "GET", "/api/repos/alice/notes/issues/1", None).await;
        head = pr["pull"]["head_sha"].as_str().unwrap_or_default().to_string();
        if head == second {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert_eq!(head, second, "the pull request on A follows the push on B");

    // bob is a person from B here: no sign-in, no email.
    let (s, _) = a.req(&Client::default(), "POST", "/api/auth/login", Some(json!({ "login": format!("bob@{host}"), "password": "x" }))).await;
    assert!(s.is_client_error());
    let name = format!("bob@{host}");
    let (email, remote): (String, Option<String>) = a
        .state
        .db
        .call(move |c| Ok(c.query_row("SELECT email, remote FROM users WHERE username = ?1", [name], |r| Ok((r.get(0)?, r.get(1)?)))?))
        .await
        .unwrap();
    assert_eq!((email.as_str(), remote), ("", Some(format!("{b_url}/bob"))));
}

#[tokio::test]
async fn servers_refuse_what_isnt_theirs_to_ask() {
    let (a, a_url, alice, b, b_url, bob) = forked().await;
    let ask = |n: &str| json!({ "repo": "alice/notes", "base": "main", "fork": "bob/notes", "branch": "main", "title": n, "body": "",
        "author": { "username": "bob", "name": "Bob" } });
    commit(&b, &bob, "bob/notes", "main", "x.md", "x\n").await;
    let path = "/api/federation/pulls";
    // Signed by B with B's key: fine (and only once).
    let k = gitter::federation::key(&b.state.cfg.data_dir).unwrap();
    let h = gitter::federation::sign(&k, &b_url, &a_url, "POST", path, now(), ask("ok").to_string().as_bytes());
    let (s, v) = raw(&a, path, &ask("ok").to_string(), &h).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(raw(&a, path, &ask("ok").to_string(), &h).await.0, StatusCode::UNAUTHORIZED, "a replay");
    // A different body than signed, an old time, no signature, or another server's key.
    let h = gitter::federation::sign(&k, &b_url, &a_url, "POST", path, now(), ask("signed").to_string().as_bytes());
    assert_eq!(raw(&a, path, &ask("sent").to_string(), &h).await.0, StatusCode::UNAUTHORIZED, "tampered");
    assert_eq!(signed(&a, &b, &b_url, path, &ask("old"), now() - 600).await.0, StatusCode::UNAUTHORIZED, "stale");
    assert_eq!(raw(&a, path, &ask("none").to_string(), "").await.0, StatusCode::UNAUTHORIZED, "unsigned");
    assert_eq!(signed(&a, &a, &b_url, path, &ask("forged"), now()).await.0, StatusCode::UNAUTHORIZED, "A's key claiming to be B");
    // Only the fork's own server updates its pull request.
    let (c, c_url) = server(true).await;
    // A request B made for another server isn't taken here.
    assert_eq!(signed_for(&a, &c_url, &b, &b_url, path, &ask("for C"), now()).await.0, StatusCode::UNAUTHORIZED, "signed for C");
    let upd = json!({ "repo": "alice/notes", "number": 1, "fork": "bob/notes", "branch": "main" });
    assert_eq!(signed(&a, &c, &c_url, "/api/federation/pulls/update", &upd, now()).await.0, StatusCode::NOT_FOUND, "C updating B's");
    let ts = now();
    assert_eq!(signed(&a, &b, &b_url, "/api/federation/pulls/update", &upd, ts).await.0, StatusCode::OK);
    assert_eq!(signed(&a, &b, &b_url, "/api/federation/pulls/update", &upd, ts).await.0, StatusCode::OK, "a second push in the same second");
    // A private repository isn't forked or proposed to.
    assert_eq!(a.req(&alice, "POST", "/api/repos", Some(json!({ "name": "secret", "private": true, "readme": true }))).await.0, StatusCode::OK);
    b.req(&bob, "POST", "/api/repos", Some(json!({ "name": "secret" }))).await;
    let (s, _) = b.req(&bob, "POST", "/api/repos/bob/secret/federation/fork", Some(json!({ "url": format!("{a_url}/alice/secret") }))).await;
    assert!(s.is_client_error(), "{s}");
    let private = json!({ "repo": "alice/secret", "base": "main", "fork": "bob/notes", "branch": "main", "title": "p", "body": "", "author": { "username": "bob", "name": "" } });
    assert_eq!(signed(&a, &b, &b_url, path, &private, now()).await.0, StatusCode::NOT_FOUND);
    // A server with federation off answers no one, and its people can't fork from others.
    let mut s = b.state.settings.get();
    s.federation = false;
    b.state.settings.save(&b.state.db, s.clone()).await.unwrap();
    b.req(&bob, "POST", "/api/repos", Some(json!({ "name": "early" }))).await;
    let (code, _) = b.req(&bob, "POST", "/api/repos/bob/early/federation/fork", Some(json!({ "url": format!("{a_url}/alice/notes") }))).await;
    assert_eq!(code, StatusCode::FORBIDDEN, "B is off");
    s.federation = true;
    b.state.settings.save(&b.state.db, s).await.unwrap();
    let mut s = a.state.settings.get();
    s.federation = false;
    a.state.settings.save(&a.state.db, s).await.unwrap();
    assert_eq!(signed(&a, &b, &b_url, path, &ask("off"), now()).await.0, StatusCode::FORBIDDEN);
    b.req(&bob, "POST", "/api/repos", Some(json!({ "name": "late" }))).await;
    let (s, v) = b.req(&bob, "POST", "/api/repos/bob/late/federation/fork", Some(json!({ "url": format!("{a_url}/alice/notes") }))).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "A doesn't work with other servers now: {v}");
    // B asks only for a fork of another server's repository, public, from a branch it has.
    b.req(&bob, "POST", "/api/repos", Some(json!({ "name": "plain", "readme": true }))).await;
    let p = |repo: &str, branch: &str| (format!("/api/repos/bob/{repo}/federation/pulls"), json!({ "branch": branch, "title": "t" }));
    let (u, body) = p("plain", "main");
    assert_eq!(b.req(&bob, "POST", &u, Some(body)).await.0, StatusCode::BAD_REQUEST, "not a remote fork");
    let (u, body) = p("notes", "nope");
    assert_eq!(b.req(&bob, "POST", &u, Some(body)).await.0, StatusCode::BAD_REQUEST, "no such branch");
    assert_eq!(b.req(&bob, "PATCH", "/api/repos/bob/notes", Some(json!({ "private": true }))).await.0, StatusCode::OK);
    let (u, body) = p("notes", "main");
    assert_eq!(b.req(&bob, "POST", &u, Some(body)).await.0, StatusCode::BAD_REQUEST, "a private fork");
}

#[tokio::test]
async fn private_addresses_only_when_the_admin_allows_them() {
    let (a, a_url) = server(true).await;
    a.signup("root").await;
    let alice = a.signup("alice").await;
    a.req(&alice, "POST", "/api/repos", Some(json!({ "name": "notes", "readme": true }))).await;
    let (b, _) = server_with(true, false).await;
    b.signup("root").await;
    let bob = b.signup("bob").await;
    b.req(&bob, "POST", "/api/repos", Some(json!({ "name": "notes" }))).await;
    let (s, v) = b.req(&bob, "POST", "/api/repos/bob/notes/federation/fork", Some(json!({ "url": format!("{a_url}/alice/notes") }))).await;
    assert!(s.is_client_error() || s.is_server_error(), "{s} {v}");
    assert!(v["error"].as_str().unwrap_or_default().contains("127.0.0.1"), "{v}");
}

#[tokio::test]
async fn pull_requests_follow_the_fork_into_any_base_and_after_reopening() {
    let (a, _, alice, b, b_url, bob) = forked().await;
    let body = json!({ "branch": "main", "new_branch": "dev", "message": "dev", "changes": [{ "path": "dev.md", "content": "dev\n" }] });
    assert_eq!(a.req(&alice, "POST", "/api/repos/alice/notes/contents", Some(body)).await.0, StatusCode::OK);
    commit(&b, &bob, "bob/notes", "main", "idea.md", "an idea\n").await;
    let path = "/api/federation/pulls";
    let ask = |base: &str, who: &str, branch: &str| json!({ "repo": "alice/notes", "base": base, "fork": "bob/notes", "branch": branch, "title": base, "body": "",
        "author": { "username": who, "name": "" } });
    // Nothing comes of a branch the fork doesn't have: no pull request, and no person for it.
    assert!(signed(&a, &b, &b_url, path, &ask("main", "ghost", "nope"), now()).await.0.is_client_error());
    let ghost = format!("ghost@{}", gitter::federation::host(&b_url));
    let n: i64 = a.state.db.call(move |c| Ok(c.query_row("SELECT count(*) FROM users WHERE username = ?1", [ghost], |r| r.get(0))?)).await.unwrap();
    assert_eq!(n, 0, "no person made for a request that came to nothing");
    // Into main and into dev: two pull requests.
    let (_, one) = signed(&a, &b, &b_url, path, &ask("main", "bob", "main"), now()).await;
    let (_, two) = signed(&a, &b, &b_url, path, &ask("dev", "bob", "main"), now()).await;
    assert_eq!((one["number"].as_i64(), two["number"].as_i64()), (Some(1), Some(2)), "{one} {two}");
    // Closed while the fork moves on, then reopened: it catches up.
    let ip = "/api/repos/alice/notes/issues/1";
    assert_eq!(a.req(&alice, "PATCH", ip, Some(json!({ "state": "closed" }))).await.0, StatusCode::OK);
    let sha = commit(&b, &bob, "bob/notes", "main", "idea.md", "a later idea\n").await;
    assert_eq!(a.req(&alice, "PATCH", ip, Some(json!({ "state": "open" }))).await.0, StatusCode::OK);
    let (_, pr) = a.req(&alice, "GET", ip, None).await;
    assert_eq!(pr["pull"]["head_sha"].as_str(), Some(sha.as_str()), "{pr}");
}

#[tokio::test]
async fn a_pull_request_gone_on_the_other_server_is_forgotten_here() {
    let (a, _, alice, b, _, bob) = forked().await;
    commit(&b, &bob, "bob/notes", "main", "idea.md", "an idea\n").await;
    let (s, v) = b.req(&bob, "POST", "/api/repos/bob/notes/federation/pulls", Some(json!({ "branch": "main", "title": "t" }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    // alice closes it; bob's next push finds it gone and stops telling A.
    assert_eq!(a.req(&alice, "PATCH", "/api/repos/alice/notes/issues/1", Some(json!({ "state": "closed" }))).await.0, StatusCode::OK);
    commit(&b, &bob, "bob/notes", "main", "idea.md", "more\n").await;
    let mut left = 1;
    for _ in 0..50 {
        let (_, out) = b.req(&bob, "GET", "/api/repos/bob/notes/federation/pulls", None).await;
        left = out["pulls"].as_array().map_or(0, Vec::len);
        if left == 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert_eq!(left, 0);
}

#[tokio::test]
async fn another_server_has_a_few_pull_requests_open_at_a_time() {
    let (a, _, _, b, b_url, bob) = forked().await;
    for i in 0..11 {
        let body = json!({ "branch": "main", "new_branch": format!("b{i}"), "message": "x", "changes": [{ "path": format!("f{i}.md"), "content": "x\n" }] });
        let (s, v) = b.req(&bob, "POST", "/api/repos/bob/notes/contents", Some(body)).await;
        assert_eq!(s, StatusCode::OK, "{v}");
    }
    let ask = |i: i32| json!({ "repo": "alice/notes", "fork": "bob/notes", "branch": format!("b{i}"), "title": format!("{i}"), "body": "", "author": { "username": "bob", "name": "" } });
    for i in 0..10 {
        let (s, v) = signed(&a, &b, &b_url, "/api/federation/pulls", &ask(i), now()).await;
        assert_eq!(s, StatusCode::OK, "{i}: {v}");
    }
    assert_eq!(signed(&a, &b, &b_url, "/api/federation/pulls", &ask(10), now()).await.0, StatusCode::CONFLICT);
}

#[tokio::test]
async fn forged_requests_dont_use_up_a_servers_budget_or_say_whats_out_there() {
    let (a, a_url, _, b, b_url, bob) = forked().await;
    commit(&b, &bob, "bob/notes", "main", "x.md", "x\n").await;
    let path = "/api/federation/pulls";
    let ask = json!({ "repo": "alice/notes", "fork": "bob/notes", "branch": "main", "title": "t", "body": "", "author": { "username": "bob", "name": "" } });
    // Many requests claiming to be B with a made-up signature: refused, and B's own still go through.
    // (The usual limit on writes from one address would stop them first; it's off here.)
    let mut set = a.state.settings.get();
    set.write_rate_per_min = 0;
    a.state.settings.save(&a.state.db, set).await.unwrap();
    let fake = format!("server=\"{b_url}\", ts=\"{}\", nonce=\"{}\", sig=\"{}\"", now(), "ab".repeat(16), "A".repeat(86) + "==");
    for _ in 0..130 {
        assert_eq!(raw(&a, path, &ask.to_string(), &fake).await.0, StatusCode::UNAUTHORIZED);
    }
    let (s, v) = signed(&a, &b, &b_url, path, &ask, now()).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert!(raw(&a, path, &ask.to_string(), &fake).await.1["error"].as_str().unwrap().contains("doesn't match"), "it got as far as the key");
    // A server that isn't there: refused without saying why to whoever asked.
    let nobody = format!("server=\"http://127.0.0.1:1\", ts=\"{}\", nonce=\"{}\", sig=\"{}\"", now(), "cd".repeat(16), "A".repeat(86) + "==");
    let (s, v) = raw(&a, path, &ask.to_string(), &nobody).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    assert!(!v["error"].as_str().unwrap_or_default().contains("127.0.0.1"), "{v}");
    // A person here from another server with the same host (http and https): not mixed up.
    let host = gitter::federation::host(&b_url).to_string();
    let (name, profile) = (format!("carl@{host}"), format!("https://{host}/carl"));
    a.state.db.call(move |c| Ok(c.execute("INSERT INTO users(username, email, pass_hash, remote, created_at) VALUES (?1, '', '!', ?2, 0)", [name, profile])?)).await.unwrap();
    let carl = json!({ "repo": "alice/notes", "fork": "bob/notes", "branch": "main", "base": "main", "title": "t", "body": "", "author": { "username": "carl", "name": "" } });
    let _ = a_url;
    commit(&b, &bob, "bob/notes", "main", "y.md", "y\n").await;
    b.req(&bob, "POST", "/api/repos/bob/notes/contents", Some(json!({ "branch": "main", "new_branch": "carl", "message": "c", "changes": [{ "path": "c.md", "content": "c\n" }] }))).await;
    let carl = { let mut c = carl; c["branch"] = json!("carl"); c };
    assert_eq!(signed(&a, &b, &b_url, path, &carl, now()).await.0, StatusCode::CONFLICT);
    // B's own have a budget too: 120 a minute (some used above).
    let none = json!({ "repo": "alice/none", "fork": "bob/notes", "branch": "main", "title": "t", "body": "", "author": { "username": "bob", "name": "" } });
    let mut last = StatusCode::OK;
    for _ in 0..120 {
        last = signed(&a, &b, &b_url, path, &none, now()).await.0;
    }
    assert_eq!(last, StatusCode::TOO_MANY_REQUESTS, "B's budget ran out");
}

#[tokio::test]
async fn federation_needs_the_servers_public_address() {
    let app = App::with_url("http://localhost:3000");
    let root = app.signup("root").await;
    let (_, mut s) = app.req(&root, "GET", "/api/admin/settings", None).await;
    s["federation"] = json!(true);
    let (code, v) = app.req(&root, "PUT", "/api/admin/settings", Some(s)).await;
    assert_eq!(code, StatusCode::BAD_REQUEST, "{v}");
    assert!(v["error"].as_str().unwrap().contains("GITTER_URL"), "{v}");
}

async fn set_lists(app: &App, allow: &[&str], block: &[&str]) {
    let mut s = app.state.settings.get();
    s.federation_allow = allow.iter().map(|x| x.to_string()).collect();
    s.federation_block = block.iter().map(|x| x.to_string()).collect();
    app.state.settings.save(&app.state.db, s).await.unwrap();
}

#[tokio::test]
async fn listed_servers_are_refused_both_ways_and_admins_see_whos_here() {
    let (a, a_url, alice, b, b_url, bob) = forked().await;
    commit(&b, &bob, "bob/notes", "main", "idea.md", "an idea\n").await;
    let (s, v) = b.req(&bob, "POST", "/api/repos/bob/notes/federation/pulls", Some(json!({ "branch": "main", "title": "t" }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    // The admin page says who has pull requests open here, and what came from each today.
    let (s, fed) = a.req(&alice, "GET", "/api/admin/federation", None).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "admins only: {fed}");
    let (_, list) = a.req(&a_admin(&a).await, "GET", "/api/admin/federation", None).await;
    let row = list["servers"].as_array().unwrap().iter().find(|r| r["server"] == b_url).cloned().unwrap_or_default();
    assert_eq!(row["open_pulls"].as_i64(), Some(1), "{list}");
    assert!(row["fetched_today"].as_u64().unwrap_or(0) > 0, "{list}");
    let ask = json!({ "repo": "alice/none", "fork": "bob/notes", "branch": "main", "title": "t", "body": "", "author": { "username": "bob", "name": "" } });
    let path = "/api/federation/pulls";
    let host_b = gitter::federation::host(&b_url).to_string();
    // Blocked here: B's requests are refused (after its signature checks out), whatever they ask.
    set_lists(&a, &[], &[&host_b]).await;
    assert_eq!(signed(&a, &b, &b_url, path, &ask, now()).await.0, StatusCode::FORBIDDEN);
    // Not on a non-empty allow list: the same.
    set_lists(&a, &["other.example"], &[]).await;
    assert_eq!(signed(&a, &b, &b_url, path, &ask, now()).await.0, StatusCode::FORBIDDEN);
    // On it: through (to the missing repository).
    set_lists(&a, &[&host_b], &[]).await;
    assert_eq!(signed(&a, &b, &b_url, path, &ask, now()).await.0, StatusCode::NOT_FOUND);
    // And B, blocking A, neither forks from it nor proposes to it.
    set_lists(&b, &[], &[gitter::federation::host(&a_url)]).await;
    b.req(&bob, "POST", "/api/repos", Some(json!({ "name": "more" }))).await;
    let (s, _) = b.req(&bob, "POST", "/api/repos/bob/more/federation/fork", Some(json!({ "url": format!("{a_url}/alice/notes") }))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = b.req(&bob, "POST", "/api/repos/bob/notes/federation/pulls", Some(json!({ "branch": "main", "title": "again" }))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}

/// The first account on `app` (an admin), signed in again.
async fn a_admin(app: &App) -> Client {
    let (_, _, cookie) = app.call(&Client::default(), "POST", "/api/auth/login", Some(json!({ "login": "root", "password": "correct horse" })), Some(common::ORIGIN)).await; // pragma: allowlist secret
    Client(cookie)
}

#[tokio::test]
async fn listed_servers_get_nothing_from_here_and_budgets_hold() {
    let (a, a_url, alice, b, b_url, bob) = forked().await;
    commit(&b, &bob, "bob/notes", "main", "idea.md", "an idea\n").await;
    b.req(&bob, "POST", "/api/repos/bob/notes/federation/pulls", Some(json!({ "branch": "main", "title": "t" }))).await;
    let before = pr_head(&a, &alice).await;
    // A blocks B (written with the default port, which names the same server).
    let host_b = gitter::federation::host(&b_url).to_string();
    set_lists(&a, &[], &[&host_b]).await;
    // A pull request reopened here doesn't fetch from it.
    let ip = "/api/repos/alice/notes/issues/1";
    a.req(&alice, "PATCH", ip, Some(json!({ "state": "closed" }))).await;
    commit(&b, &bob, "bob/notes", "main", "idea.md", "while blocked\n").await;
    a.req(&alice, "PATCH", ip, Some(json!({ "state": "open" }))).await;
    assert_eq!(pr_head(&a, &alice).await, before, "nothing fetched from a blocked server");
    set_lists(&a, &[], &[]).await;
    // B, blocking A, doesn't tell it about pushes.
    let (_, out) = b.req(&bob, "GET", "/api/repos/bob/notes/federation/pulls", None).await;
    assert_eq!(out["pulls"].as_array().map(Vec::len), Some(1));
    set_lists(&b, &[], &[gitter::federation::host(&a_url)]).await;
    commit(&b, &bob, "bob/notes", "main", "idea.md", "still blocked\n").await;
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
    assert_eq!(pr_head(&a, &alice).await, before, "B told A nothing");
    set_lists(&b, &[], &[]).await;
    // Past its budget for the day, B's updates aren't fetched.
    gitter::api::federation::charge_today(&b_url, 3 << 30);
    let upd = json!({ "repo": "alice/notes", "number": 1, "fork": "bob/notes", "branch": "main" });
    assert_eq!(signed(&a, &b, &b_url, "/api/federation/pulls/update", &upd, now()).await.0, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn a_fork_waiting_to_copy_stops_once_its_server_is_blocked() {
    let (a, a_url) = server(true).await;
    a.signup("root").await;
    let alice = a.signup("alice").await;
    a.req(&alice, "POST", "/api/repos", Some(json!({ "name": "notes", "readme": true }))).await;
    let (b, _) = server(true).await;
    b.signup("root").await;
    let bob = b.signup("bob").await;
    b.req(&bob, "POST", "/api/repos", Some(json!({ "name": "notes" }))).await;
    let (s, v) = b.req(&bob, "POST", "/api/repos/bob/notes/federation/fork", Some(json!({ "url": format!("{a_url}/alice/notes") }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    set_lists(&b, &[], &[gitter::federation::host(&a_url)]).await;
    gitter::mirrors::tick(&b.state).await;
    let (_, repo) = b.req(&bob, "GET", "/api/repos/bob/notes", None).await;
    assert_eq!(repo["empty"].as_bool(), Some(true), "nothing copied from a blocked server: {repo}");
}

#[tokio::test]
async fn server_lists_take_hosts_only() {
    let app = App::new();
    let root = app.signup("root").await;
    let (_, mut s) = app.req(&root, "GET", "/api/admin/settings", None).await;
    s["federation_block"] = json!(["https://spam.example/alice/notes"]);
    let (code, v) = app.req(&root, "PUT", "/api/admin/settings", Some(s.clone())).await;
    assert_eq!(code, StatusCode::BAD_REQUEST, "a pasted repository address would block nothing: {v}");
    s["federation_block"] = json!(["https://spam.example/", "Other.example:8443"]);
    assert_eq!(app.req(&root, "PUT", "/api/admin/settings", Some(s)).await.0, StatusCode::OK);
}

async fn pr_head(a: &App, alice: &Client) -> String {
    a.req(alice, "GET", "/api/repos/alice/notes/issues/1", None).await.1["pull"]["head_sha"].as_str().unwrap_or_default().to_string()
}
