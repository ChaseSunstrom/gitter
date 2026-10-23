//! Editing one file together: a session per file and branch, changes put in one order by the
//! server (each transformed over the ones its sender hadn't seen), presence, and a commit that
//! credits everyone who typed.
mod common;

use axum::http::StatusCode;
use common::{App, Client};
use serde_json::{Value, json};

struct World {
    app: App,
    alice: Client,
    bob: Client,
    carol: Client,
}

/// alice/notes with a README; bob can write to it, carol can only read it.
async fn world() -> World {
    let app = App::new();
    app.signup("root").await;
    let alice = app.signup("alice").await;
    let bob = app.signup("bob").await;
    let carol = app.signup("carol").await;
    assert_eq!(app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "notes", "readme": true }))).await.0, StatusCode::OK);
    let body = json!({ "branch": "main", "message": "Plan", "changes": [{ "path": "plan.md", "content": "hello\n" }] });
    assert_eq!(app.req(&alice, "POST", "/api/repos/alice/notes/contents", Some(body)).await.0, StatusCode::OK);
    for (who, role) in [("bob", "write"), ("carol", "read")] {
        assert_eq!(app.req(&alice, "PUT", &format!("/api/repos/alice/notes/collaborators/{who}"), Some(json!({ "role": role }))).await.0, StatusCode::OK);
    }
    World { app, alice, bob, carol }
}

const BASE: &str = "/api/repos/alice/notes/collab";

async fn join(app: &App, c: &Client, path: &str) -> (StatusCode, Value) {
    app.req(c, "POST", BASE, Some(json!({ "branch": "main", "path": path }))).await
}

async fn change(app: &App, c: &Client, id: &str, client: &str, revision: u64, op: Value) -> (StatusCode, Value) {
    app.req(c, "POST", &format!("{BASE}/{id}/ops"), Some(json!({ "client": client, "revision": revision, "op": op }))).await
}

#[tokio::test]
async fn two_people_editing_at_once_end_with_the_same_text() {
    let World { app, alice, bob, carol } = world().await;
    let (s, a) = join(&app, &alice, "plan.md").await;
    assert_eq!(s, StatusCode::OK, "{a}");
    assert_eq!((a["text"].as_str(), a["version"].as_u64()), (Some("hello\n"), Some(0)));
    let id = a["id"].as_str().unwrap().to_string();
    // The same file and branch is the same session.
    let (_, b) = join(&app, &bob, "plan.md").await;
    assert_eq!(b["id"].as_str(), Some(id.as_str()));
    let people: Vec<&str> = b["people"].as_array().unwrap().iter().filter_map(|p| p["user"].as_str()).collect();
    assert!(people.contains(&"alice") && people.contains(&"bob"), "{b}");
    // Readers can't join, and a session isn't another repository's.
    assert!(join(&app, &carol, "plan.md").await.0.is_client_error());
    assert!(app.req(&carol, "GET", &format!("{BASE}/{id}"), None).await.0.is_client_error());
    assert_eq!(app.req(&bob, "POST", "/api/repos", Some(json!({ "name": "other", "readme": true }))).await.0, StatusCode::OK);
    assert_eq!(app.req(&bob, "GET", &format!("/api/repos/bob/other/collab/{id}"), None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(app.req(&bob, "POST", &format!("/api/repos/bob/other/collab/{id}/ops"), Some(json!({ "client": "x", "revision": 0, "op": [6] }))).await.0, StatusCode::NOT_FOUND);

    // Both type at version 0: alice puts "A" in front, bob appends "B" (before the newline).
    let (s, v) = change(&app, &alice, &id, "a1", 0, json!(["A", 6])).await;
    assert_eq!((s, v["version"].as_u64()), (StatusCode::OK, Some(1)), "{v}");
    let (s, v) = change(&app, &bob, &id, "b1", 0, json!([5, "B", 1])).await;
    assert_eq!((s, v["version"].as_u64()), (StatusCode::OK, Some(2)), "{v}");
    // Overlapping deletes: both delete "hello" at version 2; bob's arrives after alice's.
    let (s, _) = change(&app, &alice, &id, "a2", 2, json!([1, -5, "Hi", 2])).await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = change(&app, &bob, &id, "b2", 2, json!([1, -5, 2])).await;
    assert_eq!(s, StatusCode::OK);
    let (_, st) = app.req(&bob, "GET", &format!("{BASE}/{id}"), None).await;
    assert_eq!((st["text"].as_str(), st["version"].as_u64()), (Some("AHiB\n"), Some(4)), "{st}");

    // A change for a text it doesn't fit, or from too far back, is refused.
    assert_eq!(change(&app, &alice, &id, "a3", 4, json!([3])).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(change(&app, &alice, &id, "a3", 9, json!([5])).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(change(&app, &alice, &id, "a3", 4, json!([5, "x".repeat(2_000_000)])).await.0, StatusCode::BAD_REQUEST, "too big");

    // Whoever typed is a co-author for the others' commits.
    let (_, st) = app.req(&alice, "GET", &format!("{BASE}/{id}"), None).await;
    let co = st["co_authors"].as_array().unwrap();
    assert!(co.len() == 1 && co[0].as_str().unwrap().starts_with("Co-authored-by: bob <") && co[0].as_str().unwrap().contains("noreply"), "{st}");

    // alice commits the shared text from the session's base; then the session moves on to that commit.
    let base = st["base"].as_str().unwrap().to_string();
    let msg = format!("Plan together\n\n{}", co[0].as_str().unwrap());
    let body = json!({ "branch": "main", "expected_sha": base, "message": msg, "changes": [{ "path": "plan.md", "content": st["text"] }] });
    let (s, v) = app.req(&alice, "POST", "/api/repos/alice/notes/contents", Some(body.clone())).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let sha = v["sha"].as_str().unwrap().to_string();
    assert_eq!(app.req(&alice, "POST", &format!("{BASE}/{id}/committed"), Some(json!({ "sha": sha }))).await.0, StatusCode::OK);
    let (_, st) = app.req(&bob, "GET", &format!("{BASE}/{id}"), None).await;
    assert_eq!(st["base"].as_str(), Some(sha.as_str()));
    // A sha that isn't the branch's head doesn't move it.
    assert_eq!(app.req(&alice, "POST", &format!("{BASE}/{id}/committed"), Some(json!({ "sha": base }))).await.0, StatusCode::CONFLICT);
    let (_, log) = app.req(&alice, "GET", "/api/repos/alice/notes/commits?rev=main", None).await;
    assert!(log["commits"][0]["body"].as_str().unwrap_or_default().contains("Co-authored-by: bob"), "{log}");
}

#[tokio::test]
async fn presence_and_the_event_stream() {
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    let World { app, alice, bob, .. } = world().await;
    let id = join(&app, &alice, "plan.md").await.1["id"].as_str().unwrap().to_string();
    join(&app, &bob, "plan.md").await;
    assert_eq!(app.req(&bob, "POST", &format!("{BASE}/{id}/presence"), Some(json!({ "cursor": [2, 4] }))).await.0, StatusCode::OK);
    let (_, st) = app.req(&alice, "GET", &format!("{BASE}/{id}"), None).await;
    let bobs = st["people"].as_array().unwrap().iter().find(|p| p["user"] == "bob").cloned().unwrap();
    assert_eq!(bobs["cursor"], json!([2, 4]), "{st}");
    // A change moves the others' cursors with the text.
    change(&app, &alice, &id, "a1", 0, json!(["12", 6])).await;
    let (_, st) = app.req(&alice, "GET", &format!("{BASE}/{id}"), None).await;
    let bobs = st["people"].as_array().unwrap().iter().find(|p| p["user"] == "bob").cloned().unwrap();
    assert_eq!(bobs["cursor"], json!([4, 6]), "{st}");
    // The stream starts with the whole state.
    let req = axum::http::Request::get(format!("{BASE}/{id}/events"))
        .header("host", "test.local")
        .header("cookie", format!("gitter_session={}", alice.0.clone().unwrap()))
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let mut body = resp.into_body();
    let frame = tokio::time::timeout(std::time::Duration::from_secs(5), body.frame()).await.expect("an event").unwrap().unwrap();
    let text = String::from_utf8_lossy(frame.data_ref().unwrap()).into_owned();
    assert!(text.contains("event: init") && text.contains("12hello"), "{text}");
}

/// The event stream of session `id` as `c` (it stays open while the body is kept).
async fn stream(app: &App, c: &Client, id: &str) -> axum::body::Body {
    use tower::ServiceExt;
    let req = axum::http::Request::get(format!("{BASE}/{id}/events"))
        .header("host", "test.local")
        .header("cookie", format!("gitter_session={}", c.0.clone().unwrap()))
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    resp.into_body()
}

/// The next event on a stream, or None when it ended (or nothing came within a second).
async fn next(body: &mut axum::body::Body) -> Option<String> {
    use http_body_util::BodyExt;
    loop {
        let f = tokio::time::timeout(std::time::Duration::from_secs(1), body.frame()).await.ok()??.ok()?;
        if let Some(d) = f.data_ref() {
            let t = String::from_utf8_lossy(d).into_owned();
            if t.contains("data:") {
                return Some(t);
            }
        }
    }
}

#[tokio::test]
async fn whoever_typed_stays_credited_after_leaving_and_names_cant_add_lines() {
    let World { app, alice, bob, .. } = world().await;
    let name = "Bob\nSigned-off-by: Vic <v@x.io>";
    assert_eq!(app.req(&bob, "PATCH", "/api/me", Some(json!({ "display_name": name }))).await.0, StatusCode::OK);
    let id = join(&app, &alice, "plan.md").await.1["id"].as_str().unwrap().to_string();
    join(&app, &bob, "plan.md").await;
    let mut s = stream(&app, &bob, &id).await;
    assert!(next(&mut s).await.unwrap().contains("event: init"));
    assert_eq!(change(&app, &bob, &id, "b1", 0, json!(["x", 6])).await.0, StatusCode::OK);
    drop(s); // bob closes the tab
    let (_, st) = app.req(&alice, "GET", &format!("{BASE}/{id}"), None).await;
    assert!(!st["people"].as_array().unwrap().iter().any(|p| p["user"] == "bob"), "bob left: {st}");
    let co: Vec<&str> = st["co_authors"].as_array().unwrap().iter().filter_map(|c| c.as_str()).collect();
    assert_eq!(co.len(), 1, "{st}");
    assert!(co[0].starts_with("Co-authored-by: Bob") && !co[0].contains('\n') && co[0].matches('<').count() == 1, "{co:?}");
}

#[tokio::test]
async fn people_opening_the_link_at_once_share_one_session() {
    let World { app, alice, bob, .. } = world().await;
    let (a, b) = tokio::join!(join(&app, &alice, "plan.md"), join(&app, &bob, "plan.md"));
    assert_eq!((a.0, b.0), (StatusCode::OK, StatusCode::OK));
    assert_eq!(a.1["id"], b.1["id"]);
}

#[tokio::test]
async fn typing_has_its_own_budget_and_cursors_need_a_seat() {
    let World { app, alice, bob, carol } = world().await;
    let id = join(&app, &alice, "plan.md").await.1["id"].as_str().unwrap().to_string();
    // Not in the session: a cursor from bob changes nothing (no seat taken).
    assert!(app.req(&bob, "POST", &format!("{BASE}/{id}/presence"), Some(json!({ "cursor": [1, 1] }))).await.0.is_client_error());
    let (_, st) = app.req(&alice, "GET", &format!("{BASE}/{id}"), None).await;
    assert_eq!(st["people"].as_array().unwrap().len(), 1, "{st}");
    let _ = carol;
    // A write allowance of 3 a minute doesn't hold back typing.
    let mut s = app.state.settings.get();
    s.write_rate_per_min = 3;
    app.state.settings.save(&app.state.db, s).await.unwrap();
    for v in 0..6u64 {
        let (code, body) = change(&app, &alice, &id, "a", v, json!(["x", 6 + v as i64])).await;
        assert_eq!(code, StatusCode::OK, "{body}");
    }
    // But cursors have a budget: 600 a minute.
    let mut last = StatusCode::OK;
    for _ in 0..601 {
        last = app.req(&alice, "POST", &format!("{BASE}/{id}/presence"), Some(json!({ "cursor": [1, 1] }))).await.0;
    }
    assert_eq!(last, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn only_existing_files_a_few_sessions_each_and_no_huge_counts() {
    let World { app, alice, bob, .. } = world().await;
    assert_eq!(join(&app, &alice, "nope.md").await.0, StatusCode::NOT_FOUND);
    let files: Vec<Value> = (0..11).map(|i| json!({ "path": format!("f{i}.txt"), "content": "x\n" })).collect();
    let body = json!({ "branch": "main", "message": "Files", "changes": files });
    assert_eq!(app.req(&alice, "POST", "/api/repos/alice/notes/contents", Some(body)).await.0, StatusCode::OK);
    for i in 0..10 {
        assert_eq!(join(&app, &alice, &format!("f{i}.txt")).await.0, StatusCode::OK, "{i}");
    }
    assert_eq!(join(&app, &alice, "f10.txt").await.0, StatusCode::CONFLICT, "ten sessions started each");
    let (s, b) = join(&app, &bob, "f10.txt").await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let id = b["id"].as_str().unwrap().to_string();
    // Counts past the longest text are refused, not added up until they wrap.
    let huge = json!([9223372036854775807i64, -9223372036854775807i64, 2]);
    assert_eq!(change(&app, &bob, &id, "b", 0, huge).await.0, StatusCode::BAD_REQUEST);
    // A body far bigger than any change is refused before it's read.
    let big = "y".repeat(9 << 20);
    assert_eq!(change(&app, &bob, &id, "b", 0, json!([big, 2])).await.0, StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn the_session_follows_the_branch_when_it_moves_underneath() {
    let World { app, alice, bob, .. } = world().await;
    let commit = |path: &str, content: &str| json!({ "branch": "main", "message": "Outside", "changes": [{ "path": path, "content": content }] });
    let id = join(&app, &alice, "plan.md").await.1["id"].as_str().unwrap().to_string();
    assert_eq!(change(&app, &alice, &id, "a", 0, json!(["typed ", 6])).await.0, StatusCode::OK);
    // Someone pushes another file: the session moves on to it, and its commit goes through.
    let (_, v) = app.req(&bob, "POST", "/api/repos/alice/notes/contents", Some(commit("other.md", "o\n"))).await;
    let head = v["sha"].as_str().unwrap().to_string();
    let (_, st) = app.req(&alice, "GET", &format!("{BASE}/{id}"), None).await;
    assert_eq!(st["base"].as_str(), Some(head.as_str()), "{st}");
    let body = json!({ "branch": "main", "expected_sha": head, "message": "Together", "changes": [{ "path": "plan.md", "content": st["text"] }] });
    let (s, v) = app.req(&alice, "POST", "/api/repos/alice/notes/contents", Some(body)).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let mine = v["sha"].as_str().unwrap().to_string();
    // The committer's note got lost: the next look still finds the text is what's on the branch.
    let (_, st) = app.req(&bob, "GET", &format!("{BASE}/{id}"), None).await;
    assert_eq!(st["base"].as_str(), Some(mine.as_str()), "{st}");
    // Someone changes this file outside while the session has unsaved typing: the base stays.
    assert_eq!(change(&app, &alice, &id, "a", st["version"].as_u64().unwrap(), json!(["more ", 12])).await.0, StatusCode::OK);
    let (_, v) = app.req(&bob, "POST", "/api/repos/alice/notes/contents", Some(commit("plan.md", "theirs\n"))).await;
    assert!(v["sha"].is_string(), "{v}");
    let (_, st) = app.req(&alice, "GET", &format!("{BASE}/{id}"), None).await;
    assert_eq!(st["base"].as_str(), Some(mine.as_str()), "unsaved typing isn't dropped: {st}");
    // And the head, when it isn't a commit right on top of the session's base, doesn't become it.
    let (_, v) = app.req(&bob, "POST", "/api/repos/alice/notes/contents", Some(commit("other.md", "again\n"))).await;
    let later = v["sha"].as_str().unwrap().to_string();
    assert_eq!(app.req(&alice, "POST", &format!("{BASE}/{id}/committed"), Some(json!({ "sha": later }))).await.0, StatusCode::CONFLICT);
    // A session nobody typed in takes the branch's new text.
    let rid = join(&app, &alice, "README.md").await.1["id"].as_str().unwrap().to_string();
    app.req(&bob, "POST", "/api/repos/alice/notes/contents", Some(commit("README.md", "new readme\n"))).await;
    let (_, st) = app.req(&alice, "GET", &format!("{BASE}/{rid}"), None).await;
    assert_eq!(st["text"].as_str(), Some("new readme\n"), "{st}");
}

#[tokio::test]
async fn a_removed_collaborator_stops_hearing_the_session() {
    let World { app, alice, bob, .. } = world().await;
    let id = join(&app, &alice, "plan.md").await.1["id"].as_str().unwrap().to_string();
    join(&app, &bob, "plan.md").await;
    let mut s = stream(&app, &bob, &id).await;
    assert!(next(&mut s).await.unwrap().contains("event: init"));
    assert_eq!(app.req(&alice, "DELETE", "/api/repos/alice/notes/collaborators/bob", None).await.0, StatusCode::OK);
    tokio::time::sleep(std::time::Duration::from_millis(2100)).await;
    assert_eq!(change(&app, &alice, &id, "a", 0, json!(["secret ", 6])).await.0, StatusCode::OK);
    let mut got = String::new();
    while let Some(ev) = next(&mut s).await {
        got.push_str(&ev);
    }
    assert!(!got.contains("secret"), "{got}");
}
