use axum::Json;
use axum::extract::{Path, Query, State};
use rusqlite::OptionalExtension;
use rusqlite::types::Value;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{J, like, ok};
use crate::auth::{Authed, User, Viewer};
use crate::db::now;
use crate::error::{AppError, R};
use crate::repo::{Repo, record_event, touch};
use crate::smart_http::RefUpdate;
use crate::{AppState, validate};

#[derive(Serialize, Clone)]
pub struct Label {
    id: i64,
    name: String,
    color: String,
}

#[derive(Serialize)]
pub struct IssueRow {
    pub(super) id: i64,
    number: i64,
    pub(super) title: String,
    body: String,
    pub(super) state: String,
    pub(super) is_pr: bool,
    author: String,
    pub(super) author_id: i64,
    created_at: i64,
    updated_at: i64,
    closed_at: Option<i64>,
    comments: i64,
    labels: Vec<Label>,
    head: Option<String>,
    base: Option<String>,
    /// Hidden by a moderator: shown only to its author and admins.
    hidden: bool,
    /// The agent token that opened it for `author`, if one did.
    agent: Option<String>,
    /// Pull requests: "approved" or "changes_requested" by reviewers' latest decisions.
    review_state: Option<String>,
    /// { number, title } of its milestone, if it has one.
    milestone: Option<serde_json::Value>,
    /// Usernames, only of assignees who can still write.
    assignees: Vec<String>,
}

const ISSUE_SELECT: &str = "SELECT i.id, i.number, i.title, i.body, i.state, i.is_pr, u.username, i.author_id, i.created_at,
    i.updated_at, i.closed_at, (SELECT count(*) FROM comments c WHERE c.issue_id = i.id AND c.kind = 'comment'),
    p.head_branch, p.base_branch, i.hidden,
    CASE WHEN i.is_pr = 0 THEN NULL
    WHEN EXISTS (SELECT 1 FROM reviews v WHERE v.issue_id = i.id AND v.state = 'changes_requested' AND NOT EXISTS (
        SELECT 1 FROM reviews x WHERE x.issue_id = v.issue_id AND x.author_id = v.author_id AND x.state IN ('approved', 'changes_requested')
        AND (x.submitted_at > v.submitted_at OR (x.submitted_at = v.submitted_at AND x.id > v.id)))) THEN 'changes_requested'
    WHEN EXISTS (SELECT 1 FROM reviews v WHERE v.issue_id = i.id AND v.state = 'approved') THEN 'approved' END,
    i.agent, ms.number, ms.title
    FROM issues i JOIN users u ON u.id = i.author_id LEFT JOIN pulls p ON p.issue_id = i.id LEFT JOIN milestones ms ON ms.id = i.milestone_id";

fn issue_row(r: &rusqlite::Row) -> rusqlite::Result<IssueRow> {
    Ok(IssueRow {
        id: r.get(0)?,
        number: r.get(1)?,
        title: r.get(2)?,
        body: r.get(3)?,
        state: r.get(4)?,
        is_pr: r.get::<_, i64>(5)? != 0,
        author: r.get(6)?,
        author_id: r.get(7)?,
        created_at: r.get(8)?,
        updated_at: r.get(9)?,
        closed_at: r.get(10)?,
        comments: r.get(11)?,
        labels: vec![],
        head: r.get(12)?,
        base: r.get(13)?,
        hidden: r.get::<_, i64>(14)? != 0,
        review_state: r.get(15)?,
        agent: r.get(16)?,
        milestone: r
            .get::<_, Option<i64>>(17)?
            .map(|n| r.get::<_, String>(18).map(|t| json!({ "number": n, "title": t })))
            .transpose()?,
        assignees: vec![],
    })
}

/// Fill in the labels and assignees of `items`.
fn decorate(c: &rusqlite::Connection, items: &mut [IssueRow]) -> rusqlite::Result<()> {
    let ids: Vec<i64> = items.iter().map(|i| i.id).collect();
    let (mut labels, mut people) = (labels_for(c, &ids)?, super::milestones::assignees_for(c, &ids)?);
    for i in items {
        i.labels = labels.remove(&i.id).unwrap_or_default();
        i.assignees = people.remove(&i.id).unwrap_or_default();
    }
    Ok(())
}

fn labels_for(
    c: &rusqlite::Connection,
    issue_ids: &[i64],
) -> rusqlite::Result<std::collections::HashMap<i64, Vec<Label>>> {
    let mut m: std::collections::HashMap<i64, Vec<Label>> = Default::default();
    if issue_ids.is_empty() {
        return Ok(m);
    }
    let marks = vec!["?"; issue_ids.len()].join(",");
    let mut s = c.prepare(&format!(
        "SELECT il.issue_id, l.id, l.name, l.color FROM issue_labels il JOIN labels l ON l.id = il.label_id WHERE il.issue_id IN ({marks}) ORDER BY l.name"
    ))?;
    let rows = s.query_map(rusqlite::params_from_iter(issue_ids), |r| {
        Ok((
            r.get::<_, i64>(0)?,
            Label {
                id: r.get(1)?,
                name: r.get(2)?,
                color: r.get(3)?,
            },
        ))
    })?;
    for row in rows {
        let (iid, l) = row?;
        m.entry(iid).or_default().push(l);
    }
    Ok(m)
}

#[derive(Deserialize)]
pub struct ListQ {
    state: Option<String>,
    kind: Option<String>,
    q: Option<String>,
    label: Option<String>,
    author: Option<String>,
    /// A milestone's title, or "none".
    milestone: Option<String>,
    /// A username, or "none".
    assignee: Option<String>,
    page: Option<usize>,
}

pub async fn list(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    Path((o, n)): Path<(String, String)>,
    Query(q): Query<ListQ>,
) -> J<serde_json::Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let page = super::page(q.page);
    let rid = r.id;
    let (vid, admin) = (v.as_ref().map(|u| u.id).unwrap_or(-1), v.as_ref().is_some_and(|u| u.is_admin));
    let out = st
        .db
        .call(move |c| {
            let is_pr = (q.kind.as_deref() == Some("pr")) as i64;
            let mut cond = String::from("i.repo_id = ? AND i.is_pr = ?");
            let mut params: Vec<Value> = vec![Value::Integer(rid), Value::Integer(is_pr)];
            if !admin {
                cond.push_str(" AND (i.hidden = 0 OR i.author_id = ?)");
                params.push(Value::Integer(vid));
            }
            match q.state.as_deref() {
                Some("closed") => cond.push_str(" AND i.state != 'open'"),
                Some("all") => {}
                _ => cond.push_str(" AND i.state = 'open'"),
            }
            if let Some(s) = q.q.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                if let Ok(num) = s.trim_start_matches('#').parse::<i64>() {
                    cond.push_str(" AND (i.number = ? OR i.title LIKE ? ESCAPE '\\')");
                    params.push(Value::Integer(num));
                } else {
                    cond.push_str(" AND (i.title LIKE ? ESCAPE '\\' OR i.body LIKE ? ESCAPE '\\')");
                    params.push(Value::Text(like(s)));
                }
                params.push(Value::Text(like(s)));
            }
            if let Some(l) = q.label.as_deref().filter(|s| !s.is_empty()) {
                cond.push_str(" AND EXISTS (SELECT 1 FROM issue_labels il JOIN labels l ON l.id = il.label_id WHERE il.issue_id = i.id AND l.name = ?)");
                params.push(Value::Text(l.to_string()));
            }
            if let Some(a) = q.author.as_deref().filter(|s| !s.is_empty()) {
                cond.push_str(" AND u.username = ?");
                params.push(Value::Text(a.to_string()));
            }
            match q.milestone.as_deref().filter(|s| !s.is_empty()) {
                Some("none") => cond.push_str(" AND i.milestone_id IS NULL"),
                Some(m) => {
                    cond.push_str(" AND ms.title = ?");
                    params.push(Value::Text(m.to_string()));
                }
                None => {}
            }
            if let Some(a) = q.assignee.as_deref().filter(|s| !s.is_empty()) {
                let who = if a == "none" { "" } else { " AND au.username = ?" };
                let neg = if a == "none" { "NOT " } else { "" };
                cond.push_str(&format!(
                    " AND {neg}EXISTS (SELECT 1 FROM issue_assignees ia JOIN users au ON au.id = ia.user_id JOIN repos r ON r.id = i.repo_id
                     WHERE ia.issue_id = i.id{who} AND {})",
                    super::milestones::writer_sql("ia.user_id")
                ));
                if a != "none" {
                    params.push(Value::Text(a.to_string()));
                }
            }
            let per = 30;
            let mut s = c.prepare(&format!("{ISSUE_SELECT} WHERE {cond} ORDER BY i.number DESC LIMIT {} OFFSET {}", per + 1, (page - 1) * per))?;
            let mut items: Vec<IssueRow> = s.query_map(rusqlite::params_from_iter(params), issue_row)?.collect::<Result<_, _>>()?;
            let more = items.len() > per;
            items.truncate(per);
            decorate(c, &mut items)?;
            for i in &mut items {
                i.body = String::new();
            }
            let count = |state: &str| -> rusqlite::Result<i64> {
                c.query_row(
                    &format!("SELECT count(*) FROM issues WHERE repo_id = ?1 AND is_pr = ?2 AND state {}", if state == "open" { "= 'open'" } else { "!= 'open'" }),
                    [rid, is_pr],
                    |r| r.get(0),
                )
            };
            Ok(json!({ "items": items, "has_more": more, "page": page, "open": count("open")?, "closed": count("closed")? }))
        })
        .await?;
    Ok(Json(out))
}

#[derive(Deserialize)]
pub struct CreateReq {
    title: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    labels: Vec<i64>,
    /// The issue template it was opened from (a file in .gitter/ISSUE_TEMPLATE):
    /// its labels apply whoever opens it, like GitHub.
    template: Option<String>,
}

pub(super) fn check_text(title: &str, body: &str) -> R<()> {
    if title.trim().is_empty() {
        return Err(AppError::bad("Add a title"));
    }
    if title.chars().count() > 256 {
        return Err(AppError::bad("Keep the title under 256 characters"));
    }
    if body.len() > 65_536 {
        return Err(AppError::bad("That's too long (64 KB max)"));
    }
    Ok(())
}

/// Insert an issue (or the issue half of a PR) with the next number. Returns (id, number).
fn insert_issue(
    tx: &rusqlite::Transaction,
    repo_id: i64,
    author: i64,
    title: &str,
    body: &str,
    is_pr: bool,
    agent: Option<&str>,
) -> R<(i64, i64)> {
    let num: i64 = tx.query_row(
        "SELECT coalesce(max(number), 0) + 1 FROM issues WHERE repo_id = ?1",
        [repo_id],
        |r| r.get(0),
    )?;
    let t = now();
    tx.execute(
        "INSERT INTO issues(repo_id, number, author_id, title, body, is_pr, created_at, updated_at, agent) VALUES (?1,?2,?3,?4,?5,?6,?7,?7,?8)",
        rusqlite::params![repo_id, num, author, title.trim(), body, is_pr as i64, t, agent],
    )?;
    Ok((tx.last_insert_rowid(), num))
}

pub async fn create(
    State(st): State<AppState>,
    Authed(u): Authed,
    Path((o, n)): Path<(String, String)>,
    Json(req): Json<CreateReq>,
) -> J<serde_json::Value> {
    let r = Repo::readable(&st, Some(&u), &o, &n).await?;
    check_text(&req.title, &req.body)?;
    if !st.limiter.allow(&format!("issue:{}", u.id), 30, 3600) {
        return Err(AppError::new(
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            "You're opening issues very quickly. Try again later.",
        ));
    }
    let (rid, uid, can_label) = (r.id, u.id, r.can_write);
    let agent = u.agent().map(|a| a.name.clone());
    let title = req.title.clone();
    // Looked up among the templates there are, so the name never reaches git.
    let template_labels = match &req.template {
        Some(f) => super::milestones::load_templates(&st, &r)
            .await
            .map(|t| t.0)
            .unwrap_or_default()
            .into_iter()
            .find(|t| t.file == *f)
            .map(|t| t.labels)
            .unwrap_or_default(),
        None => vec![],
    };
    let num = st
        .db
        .call(move |c| {
            let tx = c.transaction()?;
            let (id, num) = insert_issue(&tx, rid, uid, &req.title, &req.body, false, agent.as_deref())?;
            if can_label {
                for l in req.labels {
                    tx.execute("INSERT OR IGNORE INTO issue_labels(issue_id, label_id) SELECT ?1, id FROM labels WHERE id = ?2 AND repo_id = ?3", [id, l, rid])?;
                }
            }
            for name in template_labels {
                tx.execute(
                    "INSERT OR IGNORE INTO issue_labels(issue_id, label_id) SELECT ?1, id FROM labels WHERE repo_id = ?2 AND name = ?3",
                    rusqlite::params![id, rid, name],
                )?;
            }
            tx.commit()?;
            Ok(num)
        })
        .await?;
    record_event(
        &st.db,
        u.id,
        Some(r.id),
        "issue",
        &title,
        &num.to_string(),
        1,
    )
    .await?;
    crate::webhooks::issue(&st, r.id, u.id, num, false, "opened").await;
    Ok(Json(json!({ "number": num })))
}

/// An issue or PR by number, as `viewer` may see it (moderator-hidden ones only
/// for their author and admins).
pub(super) async fn load_issue(st: &AppState, r: &Repo, number: i64, viewer: Option<&User>) -> R<IssueRow> {
    let rid = r.id;
    let (vid, admin) = (viewer.map(|u| u.id).unwrap_or(-1), viewer.is_some_and(|u| u.is_admin));
    let i = st.db
        .call(move |c| {
            let mut i = [c.query_row(
                &format!("{ISSUE_SELECT} WHERE i.repo_id = ?1 AND i.number = ?2"),
                [rid, number],
                issue_row,
            )?];
            decorate(c, &mut i)?;
            let [i] = i;
            Ok(i)
        })
        .await?;
    if i.hidden && !(admin || i.author_id == vid) {
        return Err(AppError::not_found());
    }
    Ok(i)
}

pub async fn show(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    Path((o, n, num)): Path<(String, String, i64)>,
) -> J<serde_json::Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let mut issue = load_issue(&st, &r, num, v.as_ref()).await?;
    let pr = if issue.is_pr {
        Some(pull_status(&st, &r, &mut issue).await?)
    } else {
        None
    };
    let (iid, rid) = (issue.id, r.id);
    let vid = v.as_ref().map(|u| u.id).unwrap_or(-1);
    let viewer_admin = v.as_ref().is_some_and(|u| u.is_admin);
    let can_write = r.can_write;
    let (comments, all_labels, participants) = st
        .db
        .call(move |c| {
            let mut s = c.prepare(
                "SELECT c.id, u.username, c.author_id, c.kind, c.body, c.created_at, c.updated_at, c.hidden, c.agent FROM comments c
                 JOIN users u ON u.id = c.author_id WHERE c.issue_id = ?1 ORDER BY c.id",
            )?;
            let comments: Vec<serde_json::Value> = s
                .query_map([iid], |r| {
                    let author_id: i64 = r.get(2)?;
                    let hidden = r.get::<_, i64>(7)? != 0;
                    // A hidden comment keeps its place in the thread, but only its author and admins read it.
                    let body: String = if hidden && !(viewer_admin || author_id == vid) { String::new() } else { r.get(4)? };
                    Ok(json!({ "id": r.get::<_, i64>(0)?, "author": r.get::<_, String>(1)?, "kind": r.get::<_, String>(3)?,
                        "body": body, "created_at": r.get::<_, i64>(5)?, "updated_at": r.get::<_, i64>(6)?,
                        "can_edit": author_id == vid || can_write, "hidden": hidden, "agent": r.get::<_, Option<String>>(8)? }))
                })?
                .collect::<Result<_, _>>()?;
            let mut ls = c.prepare("SELECT id, name, color FROM labels WHERE repo_id = ?1 ORDER BY name")?;
            let labels: Vec<Label> = ls.query_map([rid], |r| Ok(Label { id: r.get(0)?, name: r.get(1)?, color: r.get(2)? }))?.collect::<Result<_, _>>()?;
            let mut ps = c.prepare(
                "SELECT DISTINCT u.username FROM users u WHERE u.id IN (SELECT author_id FROM comments WHERE issue_id = ?1 UNION SELECT author_id FROM issues WHERE id = ?1)",
            )?;
            let people: Vec<String> = ps.query_map([iid], |r| r.get(0))?.collect::<Result<_, _>>()?;
            Ok((comments, labels, people))
        })
        .await?;
    let can_edit = v.as_ref().is_some_and(|u| u.id == issue.author_id) || r.can_write;
    Ok(Json(
        json!({ "issue": issue, "pull": pr, "comments": comments, "all_labels": all_labels,
        "participants": participants, "can_write": r.can_write, "can_edit": can_edit, "signed_in": v.is_some() }),
    ))
}

/// head branch, base branch, head sha, base sha, merged sha, merged by, merged at
type PullRow = (
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<i64>,
);

/// A PR's head branch and the head and base shas its diff shows (the branches
/// while it's open, the recorded shas after), without merge checks.
pub(super) async fn pull_shas(st: &AppState, r: &Repo, issue: &IssueRow) -> R<(String, Option<String>, Option<String>)> {
    let iid = issue.id;
    let (head_b, base_b, head0, base0): (String, String, Option<String>, Option<String>) = st
        .db
        .call(move |c| Ok(c.query_row("SELECT head_branch, base_branch, head_sha, base_sha FROM pulls WHERE issue_id = ?1", [iid], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?))
        .await?;
    let head = st.git.resolve(&r.path, &format!("refs/heads/{head_b}")).await?;
    let base = st.git.resolve(&r.path, &format!("refs/heads/{base_b}")).await?;
    Ok(if issue.state == "open" { (head_b, head.or(head0), base.or(base0)) } else { (head_b, head0.or(head), base0.or(base)) })
}

/// Head/base state of a PR: mergeable, conflicts, or merged. Marks a PR merged
/// when its head was merged into the base outside Gitter (e.g. a manual push).
pub(super) async fn pull_status(st: &AppState, r: &Repo, issue: &mut IssueRow) -> R<serde_json::Value> {
    let iid = issue.id;
    let (head_b, base_b, head_sha0, base_sha0, merged_sha, merged_by, merged_at): PullRow = st
        .db
        .call(move |c| {
            Ok(c.query_row(
                "SELECT p.head_branch, p.base_branch, p.head_sha, p.base_sha, p.merged_sha, u.username, p.merged_at
                 FROM pulls p LEFT JOIN users u ON u.id = p.merged_by WHERE p.issue_id = ?1",
                [iid],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)),
            )?)
        })
        .await?;
    let head = st
        .git
        .resolve(&r.path, &format!("refs/heads/{head_b}"))
        .await?;
    let base = st
        .git
        .resolve(&r.path, &format!("refs/heads/{base_b}"))
        .await?;
    // A closed or merged PR is pinned to the shas it had then; an open one follows its branches.
    let (show_head, show_base) = if issue.state == "open" {
        (
            head.clone().or(head_sha0.clone()),
            base.clone().or(base_sha0.clone()),
        )
    } else {
        (
            head_sha0.clone().or(head.clone()),
            base_sha0.clone().or(base.clone()),
        )
    };
    let mut status = json!({ "head": head_b, "base": base_b, "head_sha": show_head,
        "base_sha": show_base, "merged_sha": merged_sha, "merged_by": merged_by, "merged_at": merged_at,
        "head_exists": head.is_some(), "base_exists": base.is_some() });
    if issue.state != "open" {
        return Ok(status);
    }
    let (Some(h), Some(b)) = (head, base) else {
        status["mergeable"] = json!(false);
        status["reason"] = json!("missing_branch");
        return Ok(status);
    };
    let mb = st.git.merge_base(&r.path, &h, &b).await?;
    if mb.as_deref() == Some(h.as_str()) && h != b {
        // Already contained in the base: someone merged it by hand.
        let (iid2, hs, bs) = (iid, h.clone(), b.clone());
        st.db
            .call(move |c| {
                let t = now();
                c.execute("UPDATE pulls SET merged_sha = ?1, head_sha = ?1, base_sha = ?2, merged_at = ?3 WHERE issue_id = ?4", rusqlite::params![hs, bs, t, iid2])?;
                c.execute("UPDATE issues SET state = 'merged', closed_at = ?1, updated_at = ?1 WHERE id = ?2", rusqlite::params![t, iid2])?;
                Ok(())
            })
            .await?;
        issue.state = "merged".into();
        status["merged_sha"] = json!(h);
        return Ok(status);
    }
    match st.git.merge_tree(&r.path, &b, &h).await? {
        Ok(_) => status["mergeable"] = json!(true),
        Err(files) => {
            status["mergeable"] = json!(false);
            status["reason"] = json!("conflicts");
            status["conflicts"] = json!(files);
        }
    }
    status["can_ff"] = json!(mb.as_deref() == Some(b.as_str()));
    // What the branch rules still ask for, the checks on the head, and auto-merge.
    let gate = crate::protect::gate(st, r, iid, &base_b, &h).await?;
    let auto: Option<(String, Option<String>)> = st
        .db
        .call(move |c| {
            Ok(c.query_row("SELECT p.auto_merge, u.username FROM pulls p LEFT JOIN users u ON u.id = p.auto_merge_by WHERE p.issue_id = ?1 AND p.auto_merge IS NOT NULL", [iid], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()?)
        })
        .await?;
    status["checks"] = json!(gate.checks);
    status["checks_state"] = json!(crate::protect::combined(&gate.checks));
    status["rule"] = json!(gate.rule);
    status["blocked"] = json!(gate.missing);
    status["can_bypass"] = json!(gate.can_bypass);
    status["auto_merge"] = json!(auto.map(|(m, by)| json!({ "method": m, "by": by })));
    Ok(status)
}

#[derive(Deserialize)]
pub struct UpdateReq {
    title: Option<String>,
    body: Option<String>,
    state: Option<String>,
    labels: Option<Vec<i64>>,
    /// A milestone number, or 0 for none.
    milestone: Option<i64>,
    /// Usernames (replacing the current ones): up to 10 people who can write.
    assignees: Option<Vec<String>>,
}

const MAX_ASSIGNEES: usize = 10;

pub async fn update(
    State(st): State<AppState>,
    Authed(u): Authed,
    Path((o, n, num)): Path<(String, String, i64)>,
    Json(req): Json<UpdateReq>,
) -> J<serde_json::Value> {
    let r = Repo::readable(&st, Some(&u), &o, &n).await?;
    let issue = load_issue(&st, &r, num, Some(&u)).await?;
    let is_author = issue.author_id == u.id;
    if !(is_author || r.can_write) {
        return Err(AppError::forbidden());
    }
    // Labels, milestones and assignees are the maintainers' to set, not the author's.
    if (req.labels.is_some() || req.milestone.is_some() || req.assignees.is_some()) && !r.can_write {
        return Err(AppError::forbidden());
    }
    let assignees = req.assignees.clone().map(|mut a| {
        a.sort();
        a.dedup();
        a
    });
    if assignees.as_ref().is_some_and(|a| a.len() > MAX_ASSIGNEES) {
        return Err(AppError::bad(format!("Assign up to {MAX_ASSIGNEES} people")));
    }
    if let Some(t) = &req.title {
        check_text(t, req.body.as_deref().unwrap_or(""))?;
    } else if let Some(b) = &req.body {
        check_text("x", b)?;
    }
    let new_state = match req.state.as_deref() {
        None => None,
        Some("open") if issue.state == "closed" => Some("open"),
        Some("closed") if issue.state == "open" => Some("closed"),
        Some("open") | Some("closed") => None,
        Some(_) => return Err(AppError::bad("Unknown state")),
    };
    let (iid, rid, uid) = (issue.id, r.id, u.id);
    let edited = req.title.is_some() || req.body.is_some() || req.labels.is_some() || req.milestone.is_some() || assignees.is_some();
    st.db
        .call(move |c| {
            let tx = c.transaction()?;
            let t = now();
            if let Some(title) = req.title {
                tx.execute("UPDATE issues SET title = ?1 WHERE id = ?2", rusqlite::params![title.trim(), iid])?;
            }
            if let Some(body) = req.body {
                tx.execute("UPDATE issues SET body = ?1 WHERE id = ?2", rusqlite::params![body, iid])?;
            }
            if let Some(s) = new_state {
                let closed = if s == "closed" { Some(t) } else { None };
                tx.execute("UPDATE issues SET state = ?1, closed_at = ?2 WHERE id = ?3", rusqlite::params![s, closed, iid])?;
                let kind = if s == "closed" { "closed" } else { "reopened" };
                tx.execute(
                    "INSERT INTO comments(issue_id, author_id, kind, body, created_at, updated_at) VALUES (?1, ?2, ?3, '', ?4, ?4)",
                    rusqlite::params![iid, uid, kind, t],
                )?;
            }
            if let Some(ls) = req.labels {
                tx.execute("DELETE FROM issue_labels WHERE issue_id = ?1", [iid])?;
                for l in ls {
                    tx.execute("INSERT OR IGNORE INTO issue_labels(issue_id, label_id) SELECT ?1, id FROM labels WHERE id = ?2 AND repo_id = ?3", [iid, l, rid])?;
                }
            }
            if let Some(m) = req.milestone {
                let id: Option<i64> = if m == 0 {
                    None
                } else {
                    let found = tx.query_row("SELECT id FROM milestones WHERE repo_id = ?1 AND number = ?2", [rid, m], |r| r.get(0)).optional()?;
                    Some(found.ok_or_else(|| AppError::bad("That milestone doesn't exist"))?)
                };
                tx.execute("UPDATE issues SET milestone_id = ?1 WHERE id = ?2", rusqlite::params![id, iid])?;
            }
            if let Some(names) = assignees {
                tx.execute("DELETE FROM issue_assignees WHERE issue_id = ?1", [iid])?;
                let sql = format!(
                    "SELECT u.id FROM users u JOIN repos r ON r.id = ?2 WHERE u.username = ?1 AND {}",
                    super::milestones::writer_sql("u.id")
                );
                for name in names {
                    let who: Option<i64> = tx.query_row(&sql, rusqlite::params![name, rid], |r| r.get(0)).optional()?;
                    let shown: String = name.chars().take(39).collect();
                    let who = who.ok_or_else(|| AppError::bad(format!("{shown} can't be assigned: only people with write access can")))?;
                    tx.execute("INSERT INTO issue_assignees(issue_id, user_id) VALUES (?1, ?2)", [iid, who])?;
                }
            }
            tx.execute("UPDATE issues SET updated_at = ?1 WHERE id = ?2", rusqlite::params![t, iid])?;
            tx.commit()?;
            Ok(())
        })
        .await?;
    if edited {
        crate::webhooks::issue(&st, r.id, u.id, num, issue.is_pr, "edited").await;
    }
    if let Some(s) = new_state {
        crate::webhooks::issue(&st, r.id, u.id, num, issue.is_pr, if s == "closed" { "closed" } else { "reopened" }).await;
    }
    ok()
}

#[derive(Deserialize)]
pub struct CommentReq {
    body: String,
    /// Optionally close or reopen in the same action.
    state: Option<String>,
}

pub async fn comment(
    State(st): State<AppState>,
    Authed(u): Authed,
    Path((o, n, num)): Path<(String, String, i64)>,
    Json(req): Json<CommentReq>,
) -> J<serde_json::Value> {
    let r = Repo::readable(&st, Some(&u), &o, &n).await?;
    let issue = load_issue(&st, &r, num, Some(&u)).await?;
    let body = req.body.trim().to_string();
    if body.len() > 65_536 {
        return Err(AppError::bad("That's too long (64 KB max)"));
    }
    if !st.limiter.allow(&format!("comment:{}", u.id), 60, 600) {
        return Err(AppError::new(
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            "You're commenting very quickly. Try again in a few minutes.",
        ));
    }
    let may_state = issue.author_id == u.id || r.can_write;
    let new_state = match req.state.as_deref() {
        Some("closed") if may_state && issue.state == "open" => Some("closed"),
        Some("open") if may_state && issue.state == "closed" => Some("open"),
        _ => None,
    };
    if body.is_empty() && new_state.is_none() {
        return Err(AppError::bad("Write a comment"));
    }
    let (iid, uid) = (issue.id, u.id);
    let agent = u.agent().map(|a| a.name.clone());
    let id = st
        .db
        .call(move |c| {
            let tx = c.transaction()?;
            let t = now();
            let mut id = 0;
            if !body.is_empty() {
                tx.execute(
                    "INSERT INTO comments(issue_id, author_id, body, created_at, updated_at, agent) VALUES (?1, ?2, ?3, ?4, ?4, ?5)",
                    rusqlite::params![iid, uid, body, t, agent],
                )?;
                id = tx.last_insert_rowid();
            }
            if let Some(s) = new_state {
                let closed = if s == "closed" { Some(t) } else { None };
                tx.execute("UPDATE issues SET state = ?1, closed_at = ?2 WHERE id = ?3", rusqlite::params![s, closed, iid])?;
                let kind = if s == "closed" { "closed" } else { "reopened" };
                tx.execute(
                    "INSERT INTO comments(issue_id, author_id, kind, body, created_at, updated_at, agent) VALUES (?1, ?2, ?3, '', ?4, ?4, ?5)",
                    rusqlite::params![iid, uid, kind, t, agent],
                )?;
            }
            tx.execute("UPDATE issues SET updated_at = ?1 WHERE id = ?2", rusqlite::params![t, iid])?;
            tx.commit()?;
            Ok(id)
        })
        .await?;
    record_event(
        &st.db,
        u.id,
        Some(r.id),
        "comment",
        &issue.title,
        &num.to_string(),
        1,
    )
    .await?;
    if id > 0 {
        crate::webhooks::comment(&st, r.id, u.id, id, "created").await;
    }
    if let Some(s) = new_state {
        crate::webhooks::issue(&st, r.id, u.id, num, issue.is_pr, if s == "closed" { "closed" } else { "reopened" }).await;
    }
    Ok(Json(json!({ "id": id })))
}

#[derive(Deserialize)]
pub struct EditCommentReq {
    body: String,
}

async fn comment_owner(st: &AppState, r: &Repo, id: i64) -> R<i64> {
    let rid = r.id;
    st.db
        .call(move |c| {
            Ok(c.query_row(
                "SELECT c.author_id FROM comments c JOIN issues i ON i.id = c.issue_id WHERE c.id = ?1 AND i.repo_id = ?2 AND c.kind = 'comment'",
                [id, rid],
                |r| r.get(0),
            )?)
        })
        .await
}

pub async fn edit_comment(
    State(st): State<AppState>,
    Authed(u): Authed,
    Path((o, n, id)): Path<(String, String, i64)>,
    Json(req): Json<EditCommentReq>,
) -> J<serde_json::Value> {
    let r = Repo::readable(&st, Some(&u), &o, &n).await?;
    if comment_owner(&st, &r, id).await? != u.id && !r.can_write {
        return Err(AppError::forbidden());
    }
    let body = req.body.trim().to_string();
    if body.is_empty() || body.len() > 65_536 {
        return Err(AppError::bad("Write a comment"));
    }
    st.db
        .call(move |c| {
            Ok(c.execute(
                "UPDATE comments SET body = ?1, updated_at = ?2 WHERE id = ?3",
                rusqlite::params![body, now(), id],
            )?)
        })
        .await?;
    crate::webhooks::comment(&st, r.id, u.id, id, "edited").await;
    ok()
}

pub async fn delete_comment(
    State(st): State<AppState>,
    Authed(u): Authed,
    Path((o, n, id)): Path<(String, String, i64)>,
) -> J<serde_json::Value> {
    let r = Repo::readable(&st, Some(&u), &o, &n).await?;
    if comment_owner(&st, &r, id).await? != u.id && !r.can_write {
        return Err(AppError::forbidden());
    }
    let (rid, uid, base) = (r.id, u.id, st.cfg.url.clone());
    let queued = st
        .db
        .call(move |c| {
            // The event is built from the comment, so before it goes, and only kept if it does.
            let tx = c.transaction()?;
            let queued = crate::webhooks::comment_now(&tx, &base, rid, uid, id, "deleted").unwrap_or_else(|e| {
                eprintln!("webhooks: {e}");
                0
            });
            tx.execute("DELETE FROM comments WHERE id = ?1", [id])?;
            tx.commit()?;
            Ok(queued)
        })
        .await?;
    if queued > 0 {
        st.hook_wake.notify_one();
    }
    ok()
}

pub async fn labels(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    Path((o, n)): Path<(String, String)>,
) -> J<serde_json::Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let rid = r.id;
    let ls = st
        .db
        .call(move |c| {
            let mut s = c.prepare(
                "SELECT l.id, l.name, l.color, (SELECT count(*) FROM issue_labels il JOIN issues i ON i.id = il.issue_id WHERE il.label_id = l.id AND i.state = 'open')
                 FROM labels l WHERE l.repo_id = ?1 ORDER BY l.name",
            )?;
            let v: Vec<serde_json::Value> = s
                .query_map([rid], |r| {
                    Ok(json!({ "id": r.get::<_, i64>(0)?, "name": r.get::<_, String>(1)?, "color": r.get::<_, String>(2)?, "open": r.get::<_, i64>(3)? }))
                })?
                .collect::<Result<_, _>>()?;
            Ok(v)
        })
        .await?;
    Ok(Json(json!({ "labels": ls })))
}

#[derive(Deserialize)]
pub struct LabelReq {
    name: String,
    color: String,
}

pub async fn create_label(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    Path((o, n)): Path<(String, String)>,
    Json(req): Json<LabelReq>,
) -> J<serde_json::Value> {
    let r = Repo::writable(&st, v.as_ref(), &o, &n).await?;
    let name = req.name.trim().to_string();
    let color = req.color.trim().to_string();
    if name.is_empty() || name.chars().count() > 50 {
        return Err(AppError::bad("Label names are 1 to 50 characters"));
    }
    let hex_ok = color.len() == 7
        && color.starts_with('#')
        && color[1..].bytes().all(|b| b.is_ascii_hexdigit());
    if !hex_ok {
        return Err(AppError::bad("Colors look like #3e9ef5"));
    }
    let rid = r.id;
    let id = st
        .db
        .call(move |c| {
            c.execute(
                "INSERT INTO labels(repo_id, name, color) VALUES (?1, ?2, ?3)",
                rusqlite::params![rid, name, color],
            )?;
            Ok(c.last_insert_rowid())
        })
        .await?;
    Ok(Json(json!({ "id": id })))
}

pub async fn delete_label(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    Path((o, n, id)): Path<(String, String, i64)>,
) -> J<serde_json::Value> {
    let r = Repo::writable(&st, v.as_ref(), &o, &n).await?;
    let rid = r.id;
    st.db
        .call(move |c| {
            Ok(c.execute(
                "DELETE FROM labels WHERE id = ?1 AND repo_id = ?2",
                [id, rid],
            )?)
        })
        .await?;
    ok()
}

#[derive(Deserialize)]
pub struct PullReq {
    title: String,
    #[serde(default)]
    body: String,
    head: String,
    base: String,
}

pub async fn create_pull(
    State(st): State<AppState>,
    Authed(u): Authed,
    Path((o, n)): Path<(String, String)>,
    Json(req): Json<PullReq>,
) -> J<serde_json::Value> {
    let r = Repo::readable(&st, Some(&u), &o, &n).await?;
    check_text(&req.title, &req.body)?;
    if !validate::ref_name(&req.head) || !validate::ref_name(&req.base) {
        return Err(AppError::bad("Pick two branches"));
    }
    if req.head == req.base {
        return Err(AppError::bad("Pick two different branches"));
    }
    let head = st
        .git
        .resolve(&r.path, &format!("refs/heads/{}", req.head))
        .await?
        .ok_or_else(|| AppError::bad("The source branch doesn't exist"))?;
    let base = st
        .git
        .resolve(&r.path, &format!("refs/heads/{}", req.base))
        .await?
        .ok_or_else(|| AppError::bad("The target branch doesn't exist"))?;
    if st.git.count(&r.path, &format!("{base}..{head}")).await? == 0 {
        return Err(AppError::bad(format!(
            "{} has nothing that {} doesn't already have",
            req.head, req.base
        )));
    }
    let (rid, uid) = (r.id, u.id);
    let agent = u.agent().map(|a| a.name.clone());
    let title = req.title.clone();
    let (head_branch, base_branch) = (req.head.clone(), req.base.clone());
    let num = st
        .db
        .call(move |c| {
            let tx = c.transaction()?;
            let dup: Option<i64> = tx
                .query_row(
                    "SELECT i.number FROM pulls p JOIN issues i ON i.id = p.issue_id WHERE i.repo_id = ?1 AND i.state = 'open' AND p.head_branch = ?2 AND p.base_branch = ?3",
                    rusqlite::params![rid, req.head, req.base],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(d) = dup {
                return Err(AppError::conflict(format!("Pull request #{d} is already open for these branches")));
            }
            let (id, num) = insert_issue(&tx, rid, uid, &req.title, &req.body, true, agent.as_deref())?;
            tx.execute("INSERT INTO pulls(issue_id, head_branch, base_branch) VALUES (?1, ?2, ?3)", rusqlite::params![id, req.head, req.base])?;
            tx.commit()?;
            Ok(num)
        })
        .await?;
    record_event(
        &st.db,
        u.id,
        Some(r.id),
        "pull",
        &title,
        &num.to_string(),
        1,
    )
    .await?;
    crate::webhooks::issue(&st, r.id, u.id, num, true, "opened").await;
    if let Err(e) = crate::ci::trigger::on_pull_opened(&st, &r, u.id, num, &head_branch, &base_branch).await {
        eprintln!("ci: {}#{num}: {}", r.full_name(), e.msg);
    }
    Ok(Json(json!({ "number": num })))
}

pub async fn pull_diff(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    Path((o, n, num)): Path<(String, String, i64)>,
    Query(fq): Query<super::code::FileQ>,
) -> J<serde_json::Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let mut issue = load_issue(&st, &r, num, v.as_ref()).await?;
    if !issue.is_pr {
        return Err(AppError::not_found());
    }
    let status = pull_status(&st, &r, &mut issue).await?;
    // After a merge these are the shas recorded at merge time, so the PR keeps showing what it brought in.
    let head = status["head_sha"].as_str().map(String::from);
    let base = status["base_sha"].as_str().map(String::from);
    let (Some(head), Some(base)) = (head, base) else {
        return Ok(Json(
            json!({ "commits": [], "diff": { "files": [], "additions": 0, "deletions": 0, "truncated": false } }),
        ));
    };
    let c = super::code::compare_shas(&st, &r.path, &base, &head, &fq).await?;
    Ok(Json(c.to_json()))
}

#[derive(Deserialize)]
pub struct MergeReq {
    #[serde(default = "default_method")]
    method: String,
    title: Option<String>,
    message: Option<String>,
    #[serde(default)]
    delete_branch: bool,
    /// A repository admin merging past branch rules not enforced on admins.
    #[serde(default)]
    bypass_rules: bool,
    /// Auto-merge: only if the head is still this commit.
    #[serde(skip)]
    expect_head: Option<String>,
    #[serde(skip)]
    ip: String,
}

fn default_method() -> String {
    "merge".into()
}

pub async fn merge_pull(
    State(st): State<AppState>,
    Authed(u): Authed,
    super::ClientIp(ip): super::ClientIp,
    Path((o, n, num)): Path<(String, String, i64)>,
    Json(mut req): Json<MergeReq>,
) -> J<serde_json::Value> {
    let r = Repo::writable(&st, Some(&u), &o, &n).await?;
    req.ip = ip;
    merge_with(&st, &r, &u, num, req).await.map(Json)
}

/// Auto-merge: as `u`, with the method they chose, of head `head` only, never past the branch rules.
pub(crate) async fn merge(st: &AppState, r: &Repo, u: &crate::auth::User, num: i64, method: &str, head: &str) -> R<serde_json::Value> {
    let req = MergeReq { method: method.into(), title: None, message: None, delete_branch: false, bypass_rules: false, expect_head: Some(head.into()), ip: String::new() };
    merge_with(st, r, u, num, req).await
}

async fn merge_with(st: &AppState, r: &Repo, u: &crate::auth::User, num: i64, req: MergeReq) -> R<serde_json::Value> {
    crate::mirrors::refuse_writes(r)?;
    let issue = load_issue(st, r, num, Some(u)).await?;
    if !issue.is_pr {
        return Err(AppError::not_found());
    }
    if issue.state != "open" {
        return Err(AppError::conflict("This pull request isn't open"));
    }
    let (head_b, base_b) = (
        issue.head.clone().unwrap_or_default(),
        issue.base.clone().unwrap_or_default(),
    );
    let head = st
        .git
        .resolve(&r.path, &format!("refs/heads/{head_b}"))
        .await?
        .ok_or_else(|| AppError::conflict("The source branch is gone"))?;
    let base = st
        .git
        .resolve(&r.path, &format!("refs/heads/{base_b}"))
        .await?
        .ok_or_else(|| AppError::conflict("The target branch is gone"))?;
    if req.expect_head.as_ref().is_some_and(|e| *e != head) {
        return Err(AppError::conflict("The source branch moved on"));
    }
    let gate = crate::protect::gate(st, r, issue.id, &base_b, &head).await?;
    if !gate.missing.is_empty() && !(gate.can_bypass && req.bypass_rules) {
        return Err(AppError::conflict(format!("Merging is blocked: {}", gate.missing.join("; "))));
    }
    if !gate.missing.is_empty() {
        let detail = format!("{} merged {}#{num} past its branch rules ({})", u.username, r.full_name(), gate.missing.join("; "));
        crate::security::record(&st.db, crate::security::Event { actor: Some(u), subject: Some(u.id), action: "branch_rule_bypass", detail, ip: &req.ip }).await;
    }
    let (an, ae) = u.signature();
    let title = req
        .title
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(&issue.title)
        .to_string();
    let extra = req.message.as_deref().map(str::trim).unwrap_or("");
    let new_sha = match req.method.as_str() {
        "ff" => {
            if st.git.merge_base(&r.path, &base, &head).await?.as_deref() != Some(base.as_str()) {
                return Err(AppError::conflict(
                    "Can't fast-forward: the target branch has new commits",
                ));
            }
            head.clone()
        }
        "merge" | "squash" => {
            let tree = match st.git.merge_tree(&r.path, &base, &head).await? {
                Ok(t) => t,
                Err(files) => {
                    return Err(AppError::conflict(format!(
                        "Merge conflicts in: {}",
                        files.join(", ")
                    )));
                }
            };
            if req.method == "merge" {
                let msg = format!(
                    "Merge pull request #{num} from {head_b}\n\n{title}\n{}",
                    if extra.is_empty() {
                        String::new()
                    } else {
                        format!("\n{extra}\n")
                    }
                );
                st.git
                    .commit_tree(&r.path, &tree, &[&base, &head], &msg, (&an, &ae))
                    .await?
            } else {
                let msg = format!(
                    "{title} (#{num})\n{}",
                    if extra.is_empty() {
                        String::new()
                    } else {
                        format!("\n{extra}\n")
                    }
                );
                st.git
                    .commit_tree(&r.path, &tree, &[&base], &msg, (&an, &ae))
                    .await?
            }
        }
        _ => return Err(AppError::bad("Unknown merge method")),
    };
    // Move the branch and record the merge in a task of its own, so a client
    // that disconnects halfway can't leave the branch moved and the PR open.
    let (st2, path, full) = (st.clone(), r.path.clone(), r.full_name());
    let (iid, uid) = (issue.id, u.id);
    let (hs, bs, ms) = (head.clone(), base.clone(), new_sha.clone());
    let base_ref = format!("refs/heads/{base_b}");
    tokio::spawn(async move {
        st2.git.update_ref(&path, &base_ref, &ms, Some(&bs)).await?;
        let (hs, bs2, ms2) = (hs, bs.clone(), ms.clone());
        let recorded = st2
            .db
            .call(move |c| {
                let tx = c.transaction()?;
                let t = now();
                tx.execute(
                    "UPDATE pulls SET merged_sha = ?1, merged_by = ?2, merged_at = ?3, head_sha = ?4, base_sha = ?5 WHERE issue_id = ?6",
                    rusqlite::params![ms2, uid, t, hs, bs2, iid],
                )?;
                tx.execute("UPDATE issues SET state = 'merged', closed_at = ?1, updated_at = ?1 WHERE id = ?2", rusqlite::params![t, iid])?;
                tx.execute(
                    "INSERT INTO comments(issue_id, author_id, kind, body, created_at, updated_at) VALUES (?1, ?2, 'merged', ?3, ?4, ?4)",
                    rusqlite::params![iid, uid, ms2, t],
                )?;
                tx.commit()?;
                Ok(())
            })
            .await;
        if let Err(e) = recorded {
            // Put the branch back so the PR stays open and nothing merged silently.
            // Expecting `ms` means a push that landed meanwhile is never undone.
            if st2.git.update_ref(&path, &base_ref, &bs, Some(&ms)).await.is_err() {
                eprintln!("merge of {full}#{num}: not recorded and {base_ref} moved on; left as is");
            }
            return Err(e);
        }
        Ok(())
    })
    .await
    .map_err(AppError::internal)??;
    touch(&st.db, r.id, true).await?;
    record_event(
        &st.db,
        u.id,
        Some(r.id),
        "merge",
        &issue.title,
        &num.to_string(),
        1,
    )
    .await?;
    let upd = RefUpdate {
        refname: format!("refs/heads/{base_b}"),
        old: Some(base),
        new: Some(new_sha.clone()),
    };
    crate::webhooks::issue(st, r.id, u.id, num, true, "merged").await;
    crate::hooks::on_push(st, r, u, &[upd]).await;
    if req.delete_branch && head_b != r.default_branch && crate::protect::direct(st, r, &head_b, true).await.is_ok() {
        let _ = st
            .git
            .delete_ref(&r.path, &format!("refs/heads/{head_b}"))
            .await;
        crate::hooks::forget_branches(st, r.id, vec![head_b.clone()]).await;
        crate::mirrors::changed(st, r.id).await;
    }
    Ok(json!({ "sha": new_sha }))
}
