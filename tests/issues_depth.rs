//! Milestones, assignees and issue / pull request templates.

mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};

use common::{App, Client};

async fn repo(app: &App, who: &Client, name: &str) {
    let (s, v) = app.req(who, "POST", "/api/repos", Some(json!({ "name": name, "readme": true }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
}

async fn issue(app: &App, who: &Client, repo: &str, title: &str) -> i64 {
    let (s, v) = app.req(who, "POST", &format!("/api/repos/{repo}/issues"), Some(json!({ "title": title }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    v["number"].as_i64().unwrap()
}

fn numbers(v: &Value) -> Vec<i64> {
    v["items"].as_array().unwrap().iter().map(|i| i["number"].as_i64().unwrap()).collect()
}

#[tokio::test]
async fn milestones_track_progress_and_filter_issues() {
    let app = App::new();
    let alice = app.signup("alice").await;
    let bob = app.signup("bob").await;
    repo(&app, &alice, "p").await;
    let api = "/api/repos/alice/p";

    // Only writers make milestones; titles are checked.
    let (s, _) = app.req(&bob, "POST", &format!("{api}/milestones"), Some(json!({ "title": "v1" }))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = app.req(&alice, "POST", &format!("{api}/milestones"), Some(json!({ "title": "  " }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = app.req(&alice, "POST", &format!("{api}/milestones"), Some(json!({ "title": "v1", "due_on": "next week" }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "due dates are YYYY-MM-DD");
    let (s, v) = app
        .req(&alice, "POST", &format!("{api}/milestones"), Some(json!({ "title": "v1", "description": "First cut", "due_on": "2027-01-31" })))
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let m1 = v["number"].as_i64().unwrap();
    let (s, _) = app.req(&alice, "POST", &format!("{api}/milestones"), Some(json!({ "title": "v1" }))).await;
    assert_eq!(s, StatusCode::CONFLICT, "titles are unique per repository");

    let a = issue(&app, &alice, "alice/p", "one").await;
    let b = issue(&app, &bob, "alice/p", "two").await;
    let c = issue(&app, &alice, "alice/p", "three").await;
    // The author of an issue who can't write can't put it in a milestone (like labels).
    let (s, _) = app.req(&bob, "PATCH", &format!("{api}/issues/{b}"), Some(json!({ "milestone": m1 }))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    for n in [a, b] {
        let (s, v) = app.req(&alice, "PATCH", &format!("{api}/issues/{n}"), Some(json!({ "milestone": m1 }))).await;
        assert_eq!(s, StatusCode::OK, "{v}");
    }
    let (s, _) = app.req(&alice, "PATCH", &format!("{api}/issues/{c}"), Some(json!({ "milestone": 999 }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "no such milestone");
    app.req(&alice, "PATCH", &format!("{api}/issues/{b}"), Some(json!({ "state": "closed" }))).await;

    let (_, v) = app.req(&bob, "GET", &format!("{api}/milestones/{m1}"), None).await;
    assert_eq!((v["title"].as_str(), v["open"].as_i64(), v["closed"].as_i64()), (Some("v1"), Some(1), Some(1)), "{v}");
    assert_eq!(v["due_on"], "2027-01-31");
    let (_, v) = app.req(&Client::default(), "GET", &format!("{api}/milestones"), None).await;
    assert_eq!(v["milestones"].as_array().map(Vec::len), Some(1), "{v}");
    assert_eq!(v["milestones"][0]["open"], 1);

    // Issues carry their milestone, and lists filter by it.
    let (_, v) = app.req(&bob, "GET", &format!("{api}/issues/{a}"), None).await;
    assert_eq!(v["issue"]["milestone"]["title"], "v1", "{v}");
    let (_, v) = app.req(&bob, "GET", &format!("{api}/issues?state=all&milestone=v1"), None).await;
    assert_eq!(numbers(&v), vec![b, a]);
    let (_, v) = app.req(&bob, "GET", &format!("{api}/issues?state=all&milestone=none"), None).await;
    assert_eq!(numbers(&v), vec![c], "issues without a milestone");

    // Clearing with 0; editing; closing; deleting leaves the issues without one.
    app.req(&alice, "PATCH", &format!("{api}/issues/{a}"), Some(json!({ "milestone": 0 }))).await;
    let (_, v) = app.req(&bob, "GET", &format!("{api}/issues/{a}"), None).await;
    assert!(v["issue"]["milestone"].is_null(), "{v}");
    let (s, _) = app.req(&bob, "PATCH", &format!("{api}/milestones/{m1}"), Some(json!({ "state": "closed" }))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, v) = app
        .req(&alice, "PATCH", &format!("{api}/milestones/{m1}"), Some(json!({ "title": "v1.0", "due_on": "", "state": "closed" })))
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let (_, v) = app.req(&bob, "GET", &format!("{api}/milestones/{m1}"), None).await;
    assert_eq!((v["title"].as_str(), v["state"].as_str(), v["due_on"].is_null()), (Some("v1.0"), Some("closed"), true), "{v}");
    let (_, v) = app.req(&bob, "GET", &format!("{api}/milestones"), None).await;
    assert_eq!(v["milestones"].as_array().map(Vec::len), Some(0), "closed ones are listed apart");
    let (_, v) = app.req(&bob, "GET", &format!("{api}/milestones?state=closed"), None).await;
    assert_eq!(v["milestones"].as_array().map(Vec::len), Some(1));
    // A milestone from another repository can't be used here.
    repo(&app, &alice, "q").await;
    let (_, v) = app.req(&alice, "POST", "/api/repos/alice/q/milestones", Some(json!({ "title": "other" }))).await;
    let other = v["number"].as_i64().unwrap();
    assert_eq!(other, 1, "numbered per repository");
    let (s, _) = app.req(&alice, "DELETE", &format!("{api}/milestones/{m1}"), None).await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = app.req(&alice, "GET", &format!("{api}/milestones/{m1}"), None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (_, v) = app.req(&bob, "GET", &format!("{api}/issues/{b}"), None).await;
    assert!(v["issue"]["milestone"].is_null(), "{v}");
    let (s, _) = app.req(&alice, "PATCH", &format!("{api}/issues/{a}"), Some(json!({ "milestone": other }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "milestone 1 here is gone, and q's isn't ours");

    // The account export lists the milestones of your repositories and each issue's.
    let (_, v) = app.req(&alice, "POST", &format!("{api}/milestones"), Some(json!({ "title": "v2" }))).await;
    app.req(&alice, "PATCH", &format!("{api}/issues/{c}"), Some(json!({ "milestone": v["number"] }))).await;
    let data = app
        .state
        .db
        .call(|c| {
            let uid: i64 = c.query_row("SELECT id FROM users WHERE username = 'alice'", [], |r| r.get(0))?;
            Ok(gitter::api::lifecycle::export_json(c, uid)?.0)
        })
        .await
        .unwrap();
    let file = |name: &str| -> Value { serde_json::from_slice(&data.iter().find(|(n, _)| *n == name).unwrap().1).unwrap() };
    let ms = file("milestones.json");
    assert!(ms.as_array().unwrap().iter().any(|m| m["title"] == "v2" && m["repo"] == "alice/p"), "{ms}");
    let issues = file("issues.json");
    assert!(issues.as_array().unwrap().iter().any(|i| i["number"] == c && i["milestone"] == "v2"), "{issues}");
}

#[tokio::test]
async fn assignees_are_people_who_can_write() {
    let app = App::new();
    let alice = app.signup("alice").await;
    let bob = app.signup("bob").await;
    let carol = app.signup("carol").await;
    let dave = app.signup("dave").await;
    repo(&app, &alice, "p").await;
    let api = "/api/repos/alice/p";
    app.req(&alice, "PUT", &format!("{api}/collaborators/bob"), Some(json!({ "role": "write" }))).await;
    app.req(&alice, "PUT", &format!("{api}/collaborators/carol"), Some(json!({ "role": "read" }))).await;
    let a = issue(&app, &alice, "alice/p", "one").await;
    let b = issue(&app, &dave, "alice/p", "two").await;

    // Who can be assigned: for writers only.
    let (_, v) = app.req(&bob, "GET", &format!("{api}/assignees"), None).await;
    assert_eq!(v["users"], json!(["alice", "bob"]), "{v}");
    assert_eq!(app.req(&carol, "GET", &format!("{api}/assignees"), None).await.0, StatusCode::FORBIDDEN);

    let (s, v) = app.req(&bob, "PATCH", &format!("{api}/issues/{a}"), Some(json!({ "assignees": ["bob", "alice", "bob"] }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let (_, v) = app.req(&carol, "GET", &format!("{api}/issues/{a}"), None).await;
    assert_eq!(v["issue"]["assignees"], json!(["alice", "bob"]), "{v}");
    for who in ["carol", "dave", "nobody"] {
        let (s, v) = app.req(&alice, "PATCH", &format!("{api}/issues/{a}"), Some(json!({ "assignees": [who] }))).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{who}: {v}");
    }
    // An author without write access can't assign, not even themselves; nor can readers.
    let (s, _) = app.req(&dave, "PATCH", &format!("{api}/issues/{b}"), Some(json!({ "assignees": ["alice"] }))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = app.req(&carol, "PATCH", &format!("{api}/issues/{b}"), Some(json!({ "assignees": ["alice"] }))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let many: Vec<String> = (0..11).map(|i| format!("u{i}")).collect();
    let (s, _) = app.req(&alice, "PATCH", &format!("{api}/issues/{b}"), Some(json!({ "assignees": many }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "ten at most");
    app.req(&alice, "PATCH", &format!("{api}/issues/{b}"), Some(json!({ "assignees": ["alice"] }))).await;

    // Lists show and filter by assignee.
    let (_, v) = app.req(&carol, "GET", &format!("{api}/issues?assignee=bob"), None).await;
    assert_eq!(numbers(&v), vec![a]);
    assert_eq!(v["items"][0]["assignees"], json!(["alice", "bob"]));
    let (_, v) = app.req(&carol, "GET", &format!("{api}/issues?assignee=alice"), None).await;
    assert_eq!(numbers(&v), vec![b, a]);
    let (_, v) = app.req(&carol, "GET", &format!("{api}/issues?assignee=none"), None).await;
    assert_eq!(numbers(&v), Vec::<i64>::new());

    // Someone who loses write access stops being shown as assigned (and matched by the filter).
    app.req(&alice, "DELETE", &format!("{api}/collaborators/bob"), None).await;
    let (_, v) = app.req(&alice, "GET", &format!("{api}/issues/{a}"), None).await;
    assert_eq!(v["issue"]["assignees"], json!(["alice"]), "{v}");
    let (_, v) = app.req(&alice, "GET", &format!("{api}/issues?assignee=bob"), None).await;
    assert_eq!(numbers(&v), Vec::<i64>::new());
    // Clearing.
    app.req(&alice, "PATCH", &format!("{api}/issues/{a}"), Some(json!({ "assignees": [] }))).await;
    let (_, v) = app.req(&alice, "GET", &format!("{api}/issues/{a}"), None).await;
    assert_eq!(v["issue"]["assignees"], json!([]));

    // In an organization's repository: members with write, not the org itself.
    app.req(&alice, "POST", "/api/orgs", Some(json!({ "name": "acme" }))).await;
    app.req(&alice, "POST", "/api/repos", Some(json!({ "name": "x", "owner": "acme", "readme": true }))).await;
    app.req(&alice, "PUT", "/api/orgs/acme/members/dave", Some(json!({ "role": "member" }))).await;
    let n = issue(&app, &alice, "acme/x", "org issue").await;
    let (s, _) = app.req(&alice, "PATCH", &format!("/api/repos/acme/x/issues/{n}"), Some(json!({ "assignees": ["dave"] }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "read-only members can't be assigned");
    let (s, _) = app.req(&alice, "PATCH", &format!("/api/repos/acme/x/issues/{n}"), Some(json!({ "assignees": ["acme"] }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "the organization isn't a person");
    app.req(&alice, "PATCH", "/api/orgs/acme", Some(json!({ "base_role": "write" }))).await;
    let (s, v) = app.req(&alice, "PATCH", &format!("/api/repos/acme/x/issues/{n}"), Some(json!({ "assignees": ["dave", "alice"] }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let (_, v) = app.req(&alice, "GET", "/api/repos/acme/x/assignees", None).await;
    assert_eq!(v["users"], json!(["alice", "dave"]), "{v}");
}

#[tokio::test]
async fn templates_come_from_the_default_branch() {
    let app = App::new();
    let alice = app.signup("alice").await;
    let bob = app.signup("bob").await;
    repo(&app, &alice, "p").await;
    let api = "/api/repos/alice/p";

    let (_, v) = app.req(&bob, "GET", &format!("{api}/issue-templates"), None).await;
    assert_eq!((v["issue"].as_array().map(Vec::len), v["pull"].is_null()), (Some(0), true), "{v}");

    let bug = "---\nname: Bug report\nabout: \"Something isn't working\"\ntitle: '[bug] '\nlabels: [bug, triage]\n---\n## Steps\n\n1.\n";
    let idea = "---\nname: Idea\nlabels:\n  - enhancement\n---\nWhat would help?\n";
    let changes = json!([
        { "path": ".gitter/ISSUE_TEMPLATE/bug.md", "content": bug },
        { "path": ".gitter/ISSUE_TEMPLATE/idea.md", "content": idea },
        { "path": ".gitter/ISSUE_TEMPLATE/plain.md", "content": "Just text, no front matter.\n" },
        { "path": ".gitter/ISSUE_TEMPLATE/notes.txt", "content": "not a template" },
        { "path": ".gitter/pull_request_template.md", "content": "## What changed\n\n## How to test\n" },
    ]);
    let (s, v) = app
        .req(&alice, "POST", &format!("{api}/contents"), Some(json!({ "branch": "main", "message": "templates", "changes": changes })))
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");

    let (_, v) = app.req(&bob, "GET", &format!("{api}/issue-templates"), None).await;
    let t = v["issue"].as_array().unwrap();
    assert_eq!(t.len(), 3, "{v}");
    assert_eq!(t[0]["file"], "bug.md");
    assert_eq!(t[0]["name"], "Bug report");
    assert_eq!(t[0]["about"], "Something isn't working");
    assert_eq!(t[0]["title"], "[bug] ");
    assert_eq!(t[0]["labels"], json!(["bug", "triage"]));
    assert_eq!(t[0]["body"], "## Steps\n\n1.\n");
    assert_eq!(t[1]["labels"], json!(["enhancement"]));
    assert_eq!(t[2]["name"], "plain", "named after the file without front matter");
    assert_eq!(t[2]["body"], "Just text, no front matter.\n");
    assert_eq!(v["pull"], "## What changed\n\n## How to test\n");

    // Opening from a template applies its labels that exist, even for people who can't label.
    app.req(&alice, "POST", &format!("{api}/labels"), Some(json!({ "name": "bug", "color": "#d73a4a" }))).await;
    let (s, v) = app
        .req(&bob, "POST", &format!("{api}/issues"), Some(json!({ "title": "[bug] it broke", "body": "x", "template": "bug.md" })))
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let (_, v) = app.req(&bob, "GET", &format!("{api}/issues/{}", v["number"]), None).await;
    let names: Vec<&str> = v["issue"]["labels"].as_array().unwrap().iter().map(|l| l["name"].as_str().unwrap()).collect();
    assert_eq!(names, vec!["bug"], "{v}");
    let (s, _) = app
        .req(&bob, "POST", &format!("{api}/issues"), Some(json!({ "title": "x", "template": "../../etc/passwd" })))
        .await;
    assert_eq!(s, StatusCode::OK, "an unknown template is just ignored");

    // Private repositories keep their templates to readers.
    app.req(&alice, "PATCH", api, Some(json!({ "private": true }))).await;
    assert_eq!(app.req(&bob, "GET", &format!("{api}/issue-templates"), None).await.0, StatusCode::NOT_FOUND);
}
