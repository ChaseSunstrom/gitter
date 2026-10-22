//! The notifications inbox: who gets notified (participants, @mentions,
//! assignees, watchers), read/done/unsubscribe/ignore, the unread count, and
//! private repositories never notifying people who can't read them.

mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};

use common::{App, Client};

async fn ok(app: &App, who: &Client, method: &str, path: &str, body: Option<Value>) -> Value {
    let (s, v) = app.req(who, method, path, body).await;
    assert_eq!(s, StatusCode::OK, "{method} {path}: {v}");
    v
}

async fn count(app: &App, who: &Client) -> i64 {
    ok(app, who, "GET", "/api/notifications/count", None).await["unread"].as_i64().unwrap()
}

async fn inbox(app: &App, who: &Client, query: &str) -> Vec<Value> {
    ok(app, who, "GET", &format!("/api/notifications{query}"), None).await["items"].as_array().unwrap().clone()
}

async fn issue(app: &App, who: &Client, repo: &str, title: &str, body: &str) -> i64 {
    ok(app, who, "POST", &format!("/api/repos/{repo}/issues"), Some(json!({ "title": title, "body": body }))).await["number"].as_i64().unwrap()
}

async fn comment(app: &App, who: &Client, repo: &str, n: i64, body: &str) {
    ok(app, who, "POST", &format!("/api/repos/{repo}/issues/{n}/comments"), Some(json!({ "body": body }))).await;
}

#[tokio::test]
async fn participants_mentions_assignees_and_watchers_get_one_notification_per_thread() {
    let app = App::new();
    let alice = app.signup("alice").await;
    let bob = app.signup("bob").await;
    let carol = app.signup("carol").await;
    let dave = app.signup("dave").await;
    ok(&app, &alice, "POST", "/api/repos", Some(json!({ "name": "p", "readme": true }))).await;

    // A mention notifies; one in code doesn't; the author isn't notified of their own issue.
    let n = issue(&app, &bob, "alice/p", "Crash", "cc @carol, not `@dave` or\n```\n@dave\n```\nnor bob@example.com").await;
    assert_eq!(count(&app, &bob).await, 0, "nobody is notified of what they did themselves");
    assert_eq!(count(&app, &dave).await, 0, "mentions in code don't count");
    let items = inbox(&app, &carol, "").await;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["reason"], "mention");
    assert_eq!(items[0]["repo"], "alice/p");
    assert_eq!(items[0]["kind"], "issue");
    assert_eq!(items[0]["number"], n);
    assert_eq!(items[0]["title"], "Crash");
    assert_eq!(items[0]["actor"], "bob");
    assert_eq!(items[0]["unread"], true);
    // The owner watches their own repository by default.
    assert_eq!(inbox(&app, &alice, "").await[0]["reason"], "watching");

    // Comments: participants (the author, earlier commenters, mentioned people) hear of them,
    // one notification per thread however many comments.
    comment(&app, &carol, "alice/p", n, "Same here").await;
    assert_eq!(inbox(&app, &bob, "").await[0]["reason"], "author");
    comment(&app, &bob, "alice/p", n, "Fixed?").await;
    comment(&app, &bob, "alice/p", n, "Fixed now").await;
    assert_eq!(count(&app, &carol).await, 1);
    assert_eq!(inbox(&app, &carol, "").await.len(), 1);
    assert_eq!(inbox(&app, &carol, "?filter=mentions").await.len(), 1, "the strongest reason sticks");
    assert_eq!(inbox(&app, &carol, "?filter=participating").await.len(), 1);
    assert_eq!(inbox(&app, &alice, "?filter=participating").await.len(), 0, "watching isn't participating");

    // Read, then unread again on news; done hides it until there's news.
    let id = inbox(&app, &carol, "").await[0]["id"].as_i64().unwrap();
    ok(&app, &carol, "POST", &format!("/api/notifications/{id}/read"), None).await;
    assert_eq!(count(&app, &carol).await, 0);
    assert_eq!(inbox(&app, &carol, "").await[0]["unread"], false);
    assert_eq!(inbox(&app, &carol, "?filter=unread").await.len(), 0);
    let (s, _) = app.req(&bob, "POST", &format!("/api/notifications/{id}/read"), None).await;
    assert_eq!(s, StatusCode::NOT_FOUND, "only your own notifications");
    comment(&app, &bob, "alice/p", n, "Really fixed").await;
    assert_eq!(count(&app, &carol).await, 1);
    ok(&app, &carol, "POST", &format!("/api/notifications/{id}/done"), None).await;
    assert_eq!(count(&app, &carol).await, 0);
    assert!(inbox(&app, &carol, "").await.is_empty());
    assert_eq!(inbox(&app, &carol, "?filter=done").await.len(), 1);
    comment(&app, &bob, "alice/p", n, "Once more").await;
    assert_eq!(inbox(&app, &carol, "").await.len(), 1, "news brings it back");

    // Unsubscribing from the thread stops it, except for a new mention.
    let sub = "/api/repos/alice/p/issues/1/subscription";
    assert_eq!(ok(&app, &carol, "GET", sub, None).await["subscribed"], true);
    ok(&app, &carol, "PUT", sub, Some(json!({ "subscribed": false }))).await;
    assert_eq!(ok(&app, &carol, "GET", sub, None).await["subscribed"], false);
    ok(&app, &carol, "POST", "/api/notifications/read", Some(json!({}))).await;
    comment(&app, &bob, "alice/p", n, "Ping").await;
    assert_eq!(count(&app, &carol).await, 0, "unsubscribed");
    comment(&app, &bob, "alice/p", n, "@Carol please look").await;
    assert_eq!(count(&app, &carol).await, 1, "a mention still reaches you");

    // Dave subscribes to the thread by hand.
    ok(&app, &dave, "PUT", sub, Some(json!({ "subscribed": true }))).await;
    comment(&app, &carol, "alice/p", n, "Looking").await;
    assert_eq!(inbox(&app, &dave, "").await[0]["reason"], "subscribed");

    // Ignoring a repository silences it, mentions included. (A repository named wrong marks nothing.)
    let before = count(&app, &dave).await;
    assert!(before > 0);
    assert_eq!(app.req(&dave, "POST", "/api/notifications/read", Some(json!({ "repo": "nope" }))).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(count(&app, &dave).await, before);
    ok(&app, &dave, "POST", "/api/notifications/read", Some(json!({ "repo": "alice/p" }))).await;
    ok(&app, &dave, "PUT", "/api/repos/alice/p/subscription", Some(json!({ "level": "ignoring" }))).await;
    assert_eq!(ok(&app, &dave, "GET", "/api/repos/alice/p/subscription", None).await["level"], "ignoring");
    issue(&app, &bob, "alice/p", "Hey", "@dave look").await;
    assert_eq!(count(&app, &dave).await, 0, "ignored");

    // Watching: every new thread.
    ok(&app, &carol, "PUT", "/api/repos/alice/p/subscription", Some(json!({ "level": "watching" }))).await;
    ok(&app, &carol, "POST", "/api/notifications/read", Some(json!({}))).await;
    let m = issue(&app, &alice, "alice/p", "Plan", "").await;
    let items = inbox(&app, &carol, "?filter=unread").await;
    assert_eq!(items.len(), 1);
    assert_eq!((items[0]["number"].as_i64(), items[0]["reason"].as_str()), (Some(m), Some("watching")));
    ok(&app, &carol, "PUT", "/api/repos/alice/p/subscription", Some(json!({ "level": "participating" }))).await;
    // The owner can stop watching their own repository.
    ok(&app, &alice, "PUT", "/api/repos/alice/p/subscription", Some(json!({ "level": "participating" }))).await;
    ok(&app, &alice, "POST", "/api/notifications/read", Some(json!({}))).await;
    issue(&app, &bob, "alice/p", "Quiet", "").await;
    assert_eq!(count(&app, &alice).await, 0, "not watching any more");
    let (s, _) = app.req(&alice, "PUT", "/api/repos/alice/p/subscription", Some(json!({ "level": "loud" }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Assigning someone notifies them (only them).
    ok(&app, &alice, "PUT", "/api/repos/alice/p/collaborators/dave", Some(json!({ "role": "write" }))).await;
    ok(&app, &dave, "PUT", "/api/repos/alice/p/subscription", Some(json!({ "level": "participating" }))).await;
    comment(&app, &bob, "alice/p", m, "Me too").await;
    ok(&app, &bob, "POST", "/api/notifications/read", Some(json!({}))).await;
    ok(&app, &alice, "PATCH", &format!("/api/repos/alice/p/issues/{m}"), Some(json!({ "assignees": ["dave"] }))).await;
    let items = inbox(&app, &dave, "?filter=unread").await;
    assert_eq!(items.len(), 1);
    assert_eq!((items[0]["number"].as_i64(), items[0]["reason"].as_str()), (Some(m), Some("assign")));
    assert_eq!(count(&app, &bob).await, 0, "an assignment tells the assignee only");
    // ...and then they hear of the thread's news.
    comment(&app, &carol, "alice/p", m, "Thanks").await;
    assert_eq!(inbox(&app, &dave, "?filter=participating").await.len(), 1);
    // Closing tells participants.
    ok(&app, &dave, "POST", "/api/notifications/read", Some(json!({}))).await;
    ok(&app, &alice, "PATCH", &format!("/api/repos/alice/p/issues/{m}"), Some(json!({ "state": "closed" }))).await;
    assert_eq!(count(&app, &dave).await, 1);

    // Filter by repository.
    ok(&app, &alice, "POST", "/api/repos", Some(json!({ "name": "q" }))).await;
    issue(&app, &alice, "alice/q", "Elsewhere", "@carol").await;
    assert_eq!(inbox(&app, &carol, "?repo=alice/q").await.len(), 1);
    assert!(inbox(&app, &carol, "?repo=alice/q").await.iter().all(|i| i["repo"] == "alice/q"));

    // Signed out: no inbox.
    let (s, _) = app.req(&Client::default(), "GET", "/api/notifications/count", None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn private_repositories_never_notify_people_who_cannot_read_them() {
    let app = App::new();
    let alice = app.signup("alice").await;
    let bob = app.signup("bob").await;
    let eve = app.signup("eve").await;
    ok(&app, &alice, "POST", "/api/repos", Some(json!({ "name": "s", "private": true }))).await;

    let n = issue(&app, &alice, "alice/s", "Secret plan", "@bob @eve have a look").await;
    assert_eq!(count(&app, &bob).await, 0);
    assert_eq!(count(&app, &eve).await, 0);
    // Outsiders can't watch or subscribe to what they can't see.
    let (s, _) = app.req(&eve, "PUT", "/api/repos/alice/s/subscription", Some(json!({ "level": "watching" }))).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = app.req(&eve, "PUT", &format!("/api/repos/alice/s/issues/{n}/subscription"), Some(json!({ "subscribed": true }))).await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    // A collaborator is notified; once removed, the notification goes from their inbox and count.
    ok(&app, &alice, "PUT", "/api/repos/alice/s/collaborators/bob", Some(json!({ "role": "read" }))).await;
    comment(&app, &alice, "alice/s", n, "@bob now you can").await;
    assert_eq!(count(&app, &bob).await, 1);
    ok(&app, &bob, "PUT", "/api/repos/alice/s/subscription", Some(json!({ "level": "watching" }))).await;
    let (s, _) = app.req(&alice, "DELETE", "/api/repos/alice/s/collaborators/bob", None).await;
    assert!(s.is_success());
    assert_eq!(count(&app, &bob).await, 0);
    assert!(inbox(&app, &bob, "").await.is_empty());
    // Still watching on paper, but nothing reaches them.
    issue(&app, &alice, "alice/s", "More secrets", "@bob").await;
    let visible: i64 = app
        .state
        .db
        .call(|c| Ok(c.query_row("SELECT count(*) FROM notifications n JOIN users u ON u.id = n.user_id WHERE u.username = 'bob'", [], |r| r.get(0))?))
        .await
        .unwrap();
    assert_eq!(visible, 1, "only the one from when bob could read it");

    // A public repository turned private stops notifying outsiders too.
    ok(&app, &alice, "POST", "/api/repos", Some(json!({ "name": "pub" }))).await;
    let m = issue(&app, &eve, "alice/pub", "Hi", "").await;
    ok(&app, &alice, "PATCH", "/api/repos/alice/pub", Some(json!({ "private": true }))).await;
    comment(&app, &alice, "alice/pub", m, "Closing the doors").await;
    assert_eq!(count(&app, &eve).await, 0);
}

#[tokio::test]
async fn a_failed_ci_run_tells_whoever_pushed() {
    let app = App::new();
    let root = app.signup("root1").await;
    ok(&app, &root, "POST", "/api/repos", Some(json!({ "name": "r", "readme": true }))).await;
    ok(&app, &root, "PUT", "/api/repos/root1/r/subscription", Some(json!({ "level": "participating" }))).await;
    tokio::spawn(gitter::ci::runner::builtin(app.state.clone()));
    let config = "jobs:\n  test:\n    steps: [\"exit 3\"]\n";
    ok(&app, &root, "POST", "/api/repos/root1/r/contents", Some(json!({ "branch": "main", "message": "CI", "changes": [{ "path": ".gitter/ci.yml", "content": config }] }))).await;
    let mut items = vec![];
    for _ in 0..200 {
        items = inbox(&app, &root, "").await;
        if !items.is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert_eq!(items.len(), 1, "the run should fail and notify");
    assert_eq!((items[0]["kind"].as_str(), items[0]["reason"].as_str(), items[0]["state"].as_str()), (Some("ci"), Some("ci"), Some("failure")));
    assert_eq!(items[0]["url"], "/root1/r/ci/1");
}

#[tokio::test]
async fn discussions_and_reviews_notify_and_merges_tell_participants() {
    let app = App::new();
    let alice = app.signup("alice").await;
    let bob = app.signup("bob").await;
    let carol = app.signup("carol").await;
    ok(&app, &alice, "POST", "/api/repos", Some(json!({ "name": "p", "readme": true }))).await;
    ok(&app, &alice, "PUT", "/api/repos/alice/p/subscription", Some(json!({ "level": "participating" }))).await;

    // Discussions: mentions, then replies reach the author and participants.
    let d = ok(&app, &bob, "POST", "/api/repos/alice/p/discussions", Some(json!({ "title": "Ideas", "body": "@carol thoughts?", "category": "ideas" })))
        .await["number"]
        .as_i64()
        .unwrap();
    let items = inbox(&app, &carol, "").await;
    assert_eq!((items[0]["kind"].as_str(), items[0]["number"].as_i64(), items[0]["reason"].as_str()), (Some("discussion"), Some(d), Some("mention")));
    ok(&app, &carol, "POST", &format!("/api/repos/alice/p/discussions/{d}/comments"), Some(json!({ "body": "Yes" }))).await;
    assert_eq!(inbox(&app, &bob, "").await[0]["reason"], "author");
    let sub = format!("/api/repos/alice/p/discussions/{d}/subscription");
    ok(&app, &bob, "PUT", &sub, Some(json!({ "subscribed": false }))).await;
    ok(&app, &bob, "POST", "/api/notifications/read", Some(json!({}))).await;
    ok(&app, &carol, "POST", &format!("/api/repos/alice/p/discussions/{d}/comments"), Some(json!({ "body": "More" }))).await;
    assert_eq!(count(&app, &bob).await, 0);

    // A pull request: a submitted review's mention notifies; the merge tells participants.
    ok(&app, &alice, "POST", "/api/repos/alice/p/contents", Some(json!({ "branch": "main", "new_branch": "f", "message": "F", "changes": [{ "path": "f.txt", "content": "x\n" }] }))).await;
    let pr = ok(&app, &alice, "POST", "/api/repos/alice/p/pulls", Some(json!({ "title": "Add f", "head": "f", "base": "main" }))).await["number"].as_i64().unwrap();
    ok(&app, &carol, "POST", "/api/notifications/read", Some(json!({}))).await;
    ok(&app, &bob, "POST", &format!("/api/repos/alice/p/pulls/{pr}/reviews"), Some(json!({ "event": "comment", "body": "LGTM, @carol?" }))).await;
    let items = inbox(&app, &carol, "?filter=unread").await;
    assert_eq!((items[0]["kind"].as_str(), items[0]["number"].as_i64(), items[0]["reason"].as_str()), (Some("pr"), Some(pr), Some("mention")));
    assert_eq!(inbox(&app, &alice, "?filter=unread").await[0]["reason"], "author");
    ok(&app, &bob, "POST", "/api/notifications/read", Some(json!({}))).await;
    ok(&app, &alice, "POST", &format!("/api/repos/alice/p/pulls/{pr}/merge"), Some(json!({}))).await;
    assert_eq!(count(&app, &bob).await, 1, "the reviewer hears of the merge");
    assert_eq!(count(&app, &alice).await, 1, "still just bob's review: the merge was alice's own");
}

/// Notifications rows for `user`, readable or not.
async fn rows(app: &App, user: &'static str) -> i64 {
    app.state
        .db
        .call(move |c| Ok(c.query_row("SELECT count(*) FROM notifications n JOIN users u ON u.id = n.user_id WHERE u.username = ?1", [user], |r| r.get(0))?))
        .await
        .unwrap()
}

#[tokio::test]
async fn mentions_in_indented_list_items_notify_but_indented_code_does_not() {
    let app = App::new();
    let alice = app.signup("alice").await;
    let bob = app.signup("bob").await;
    let carol = app.signup("carol").await;
    let dave = app.signup("dave").await;
    ok(&app, &alice, "POST", "/api/repos", Some(json!({ "name": "p" }))).await;
    issue(&app, &alice, "alice/p", "Plan", "- first\n    - cc @bob\n- second\n\n    more on it, @carol\n\nThen:\n\n    @dave in code").await;
    assert_eq!(count(&app, &bob).await, 1, "a nested list item isn't code");
    assert_eq!(count(&app, &carol).await, 1, "nor is a list item's second paragraph");
    assert_eq!(count(&app, &dave).await, 0, "an indented code block is");
}

#[tokio::test]
async fn a_mention_past_a_reviews_200th_line_comment_notifies() {
    let app = App::new();
    let alice = app.signup("alice").await;
    let bob = app.signup("bob").await;
    let carol = app.signup("carol").await;
    ok(&app, &alice, "POST", "/api/repos", Some(json!({ "name": "p", "readme": true }))).await;
    ok(&app, &alice, "POST", "/api/repos/alice/p/contents", Some(json!({ "branch": "main", "new_branch": "f", "message": "F", "changes": [{ "path": "f.txt", "content": "x\n" }] }))).await;
    let pr = ok(&app, &alice, "POST", "/api/repos/alice/p/pulls", Some(json!({ "title": "Add f", "head": "f", "base": "main" }))).await["number"].as_i64().unwrap();
    let line = |body: &str| json!({ "path": "f.txt", "side": "new", "line": 1, "body": body, "pending": true });
    ok(&app, &bob, "POST", &format!("/api/repos/alice/p/pulls/{pr}/comments"), Some(line("nit"))).await;
    // 250 more pending line comments (straight into the database: the API takes 60 in 10 minutes).
    app.state
        .db
        .call(|c| {
            for _ in 0..250 {
                c.execute(
                    "INSERT INTO review_comments(issue_id, review_id, author_id, path, side, line, start_line, commit_sha, original, in_reply_to, body, created_at, updated_at)
                     SELECT issue_id, review_id, author_id, path, side, line, start_line, commit_sha, original, in_reply_to, body, created_at, updated_at
                     FROM review_comments WHERE id = (SELECT min(id) FROM review_comments)",
                    [],
                )?;
            }
            Ok(())
        })
        .await
        .unwrap();
    ok(&app, &bob, "POST", &format!("/api/repos/alice/p/pulls/{pr}/comments"), Some(line("@carol what do you think?"))).await;
    ok(&app, &bob, "POST", &format!("/api/repos/alice/p/pulls/{pr}/reviews"), Some(json!({ "event": "comment" }))).await;
    let items = inbox(&app, &carol, "").await;
    assert_eq!(items.len(), 1, "the mention in the 252nd line comment");
    assert_eq!((items[0]["number"].as_i64(), items[0]["reason"].as_str()), (Some(pr), Some("mention")));
}

#[tokio::test]
async fn old_read_and_done_notifications_are_pruned() {
    let app = App::new();
    let alice = app.signup("alice").await;
    let bob = app.signup("bob").await;
    ok(&app, &alice, "POST", "/api/repos", Some(json!({ "name": "p" }))).await;
    // (unread, done, days since its news, kept?)
    let cases = [(0, 1, 100, false), (0, 1, 50, true), (0, 0, 200, false), (0, 0, 100, true), (1, 0, 400, true)];
    for (i, (unread, done, days, _)) in cases.into_iter().enumerate() {
        let n = issue(&app, &bob, "alice/p", &format!("Issue {i}"), "").await;
        let t = gitter::db::now() - days * 86400;
        app.state
            .db
            .call(move |c| Ok(c.execute("UPDATE notifications SET unread = ?1, done = ?2, updated_at = ?3 WHERE issue_id = (SELECT id FROM issues WHERE number = ?4)", [unread, done, t, n])?))
            .await
            .unwrap();
    }
    assert_eq!(rows(&app, "alice").await, 5);
    // One far along the ids: the sweep goes 10,000 ids at a time and must reach it.
    app.state
        .db
        .call(|c| Ok(c.execute("UPDATE notifications SET id = id + 25000 WHERE issue_id = (SELECT id FROM issues WHERE title = 'Issue 2')", [])?))
        .await
        .unwrap();
    assert_eq!(gitter::api::notifications::sweep(&app.state).await.unwrap(), 2);
    let left: Vec<String> = app
        .state
        .db
        .call(|c| Ok(c.prepare("SELECT i.title FROM notifications n JOIN issues i ON i.id = n.issue_id ORDER BY i.number")?.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?))
        .await
        .unwrap();
    let kept: Vec<String> = cases.iter().enumerate().filter(|(_, c)| c.3).map(|(i, _)| format!("Issue {i}")).collect();
    assert_eq!(left, kept, "done ones go after 90 days, read ones after 180, unread ones stay");
    let facet = &ok(&app, &alice, "GET", "/api/notifications", None).await["repos"];
    assert_eq!(facet, &json!([{ "repo": "alice/p", "count": 2, "unread": 1 }]), "the repository facet counts open ones");
    assert_eq!(gitter::api::notifications::sweep(&app.state).await.unwrap(), 0, "a second sweep finds nothing");
}

#[tokio::test]
async fn hidden_issues_and_suspended_accounts_never_notify() {
    let app = App::new();
    let alice = app.signup("alice").await; // the first account: an admin
    let bob = app.signup("bob").await;
    let carol = app.signup("carol").await;
    let dave = app.signup("dave").await;
    ok(&app, &alice, "POST", "/api/repos", Some(json!({ "name": "p" }))).await;

    // A moderator hides bob's issue: only bob and admins see it now, so only they hear of it.
    let n = issue(&app, &bob, "alice/p", "Spam", "").await;
    comment(&app, &carol, "alice/p", n, "Hm").await;
    app.state.db.call(move |c| Ok(c.execute("UPDATE issues SET hidden = 1 WHERE number = ?1", [n])?)).await.unwrap();
    ok(&app, &alice, "POST", "/api/notifications/read", Some(json!({}))).await;
    comment(&app, &bob, "alice/p", n, "@dave @carol look").await;
    assert_eq!(rows(&app, "carol").await, 0, "a participant who can't see it any more");
    assert_eq!(rows(&app, "dave").await, 0, "nor someone mentioned");
    assert_eq!(count(&app, &alice).await, 1, "the owner, an admin, still can");

    // A suspended account hears of nothing, its own threads and mentions included.
    let m = issue(&app, &dave, "alice/p", "Ideas", "").await;
    ok(&app, &alice, "PATCH", "/api/admin/users/dave", Some(json!({ "suspended": true }))).await;
    comment(&app, &carol, "alice/p", m, "@dave what about this").await;
    assert_eq!(rows(&app, "dave").await, 0);
    assert_eq!(rows(&app, "carol").await, 0, "and carol wrote it herself");
}

#[tokio::test]
async fn mentions_are_capped_per_author_per_hour() {
    let app = App::new();
    let alice = app.signup("alice").await;
    let bob = app.signup("bob").await;
    let carol = app.signup("carol").await;
    let dave = app.signup("dave").await;
    ok(&app, &alice, "POST", "/api/repos", Some(json!({ "name": "p" }))).await;
    let n = issue(&app, &alice, "alice/p", "Thread", "").await;
    // 200 mentions per author per hour: four comments naming 50 people each use them up.
    let fifty: String = (0..50).map(|i| format!("@ghost{i} ")).collect();
    for _ in 0..4 {
        comment(&app, &bob, "alice/p", n, &fifty).await;
    }
    comment(&app, &bob, "alice/p", n, "@carol too").await;
    assert_eq!(count(&app, &carol).await, 0, "bob's mentions are used up for the hour");
    comment(&app, &dave, "alice/p", n, "@carol").await;
    assert_eq!(count(&app, &carol).await, 1, "the cap is per author");
}
