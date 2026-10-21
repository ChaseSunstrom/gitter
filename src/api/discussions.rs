//! Repository discussions: a forum next to issues, with categories, markdown
//! replies, one marked answer for Q&A threads, and upvotes. Anyone who can
//! read the repo and is signed in takes part; maintainers moderate.

use axum::Json;
use axum::extract::{Path, Query, State};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::issues::check_text;
use super::{ClientIp, J, ok};
use crate::AppState;
use crate::auth::{Authed, Viewer};
use crate::db::now;
use crate::error::{AppError, R};
use crate::repo::Repo;
use crate::security::{Event, record};

pub const CATEGORIES: &[(&str, &str)] =
    &[("general", "General"), ("qa", "Q&A"), ("ideas", "Ideas"), ("announcements", "Announcements")];

fn category(s: &str) -> R<&'static str> {
    CATEGORIES.iter().find(|c| c.0 == s).map(|c| c.0).ok_or_else(|| AppError::bad("Unknown category"))
}

#[derive(Serialize)]
struct Row {
    id: i64,
    number: i64,
    category: String,
    title: String,
    body: String,
    author: String,
    author_id: i64,
    answer_id: Option<i64>,
    answered: bool,
    created_at: i64,
    updated_at: i64,
    comments: i64,
    votes: i64,
}

const SELECT: &str = "SELECT d.id, d.number, d.category, d.title, d.body, u.username, d.author_id, d.answer_id, d.created_at, d.updated_at,
    (SELECT count(*) FROM discussion_comments c WHERE c.discussion_id = d.id),
    (SELECT count(*) FROM discussion_votes v WHERE v.discussion_id = d.id AND v.comment_id IS NULL) AS votes
    FROM discussions d JOIN users u ON u.id = d.author_id";

fn row(r: &rusqlite::Row) -> rusqlite::Result<Row> {
    let answer_id: Option<i64> = r.get(7)?;
    Ok(Row {
        id: r.get(0)?,
        number: r.get(1)?,
        category: r.get(2)?,
        title: r.get(3)?,
        body: r.get(4)?,
        author: r.get(5)?,
        author_id: r.get(6)?,
        answer_id,
        answered: answer_id.is_some(),
        created_at: r.get(8)?,
        updated_at: r.get(9)?,
        comments: r.get(10)?,
        votes: r.get(11)?,
    })
}

async fn load(st: &AppState, r: &Repo, number: i64) -> R<Row> {
    let rid = r.id;
    st.db
        .call(move |c| Ok(c.query_row(&format!("{SELECT} WHERE d.repo_id = ?1 AND d.number = ?2"), [rid, number], row)?))
        .await
}

#[derive(Deserialize)]
pub struct ListQ {
    category: Option<String>,
    sort: Option<String>,
    page: Option<usize>,
}

pub async fn list(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    Path((o, n)): Path<(String, String)>,
    Query(q): Query<ListQ>,
) -> J<Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let cat = q.category.as_deref().filter(|s| !s.is_empty()).map(category).transpose()?;
    let (rid, page, per) = (r.id, super::page(q.page), 30);
    let order = if q.sort.as_deref() == Some("top") { "votes DESC, d.id DESC" } else { "d.updated_at DESC, d.id DESC" };
    let (mut items, counts) = st
        .db
        .call(move |c| {
            let sql = format!(
                "{SELECT} WHERE d.repo_id = ?1 AND (?2 IS NULL OR d.category = ?2) ORDER BY {order} LIMIT {} OFFSET {}",
                per + 1,
                (page - 1) * per
            );
            let items: Vec<Row> = c.prepare(&sql)?.query_map(rusqlite::params![rid, cat], row)?.collect::<Result<_, _>>()?;
            let mut s = c.prepare("SELECT category, count(*) FROM discussions WHERE repo_id = ?1 GROUP BY category")?;
            let counts: Vec<(String, i64)> = s.query_map([rid], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
            Ok((items, counts))
        })
        .await?;
    let more = items.len() > per;
    items.truncate(per);
    items.iter_mut().for_each(|i| i.body.clear());
    let categories: Vec<Value> = CATEGORIES
        .iter()
        .map(|(id, name)| {
            let count = counts.iter().find(|c| c.0 == *id).map_or(0, |c| c.1);
            json!({ "id": id, "name": name, "count": count })
        })
        .collect();
    Ok(Json(json!({ "items": items, "has_more": more, "page": page, "categories": categories,
        "can_write": r.can_write, "signed_in": v.is_some() })))
}

#[derive(Deserialize)]
pub struct CreateReq {
    title: String,
    #[serde(default)]
    body: String,
    category: String,
}

pub async fn create(
    State(st): State<AppState>,
    Authed(u): Authed,
    Path((o, n)): Path<(String, String)>,
    Json(req): Json<CreateReq>,
) -> J<Value> {
    let r = Repo::readable(&st, Some(&u), &o, &n).await?;
    let cat = category(&req.category)?;
    if cat == "announcements" && !r.can_write {
        return Err(AppError::new(axum::http::StatusCode::FORBIDDEN, "Only maintainers post announcements"));
    }
    check_text(&req.title, &req.body)?;
    if !st.limiter.allow(&format!("discussion:{}", u.id), 30, 3600) {
        return Err(AppError::new(axum::http::StatusCode::TOO_MANY_REQUESTS, "You're starting discussions very quickly. Try again later."));
    }
    let (rid, uid) = (r.id, u.id);
    let number = st
        .db
        .call(move |c| {
            let tx = c.transaction()?;
            let num: i64 = tx.query_row("SELECT coalesce(max(number), 0) + 1 FROM discussions WHERE repo_id = ?1", [rid], |r| r.get(0))?;
            tx.execute(
                "INSERT INTO discussions(repo_id, number, category, author_id, title, body, created_at, updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?7)",
                rusqlite::params![rid, num, cat, uid, req.title.trim(), req.body, now()],
            )?;
            tx.commit()?;
            Ok(num)
        })
        .await?;
    Ok(Json(json!({ "number": number })))
}

pub async fn show(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    Path((o, n, num)): Path<(String, String, i64)>,
) -> J<Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let d = load(&st, &r, num).await?;
    let (did, vid, writer) = (d.id, v.as_ref().map_or(-1, |u| u.id), r.can_write);
    let answer = d.answer_id;
    let (voted, comments) = st
        .db
        .call(move |c| {
            let voted = c
                .query_row("SELECT 1 FROM discussion_votes WHERE discussion_id = ?1 AND comment_id IS NULL AND user_id = ?2", [did, vid], |_| Ok(()))
                .optional()?
                .is_some();
            let mut s = c.prepare(
                "SELECT c.id, u.username, c.author_id, c.body, c.created_at, c.updated_at,
                 (SELECT count(*) FROM discussion_votes v WHERE v.comment_id = c.id),
                 EXISTS (SELECT 1 FROM discussion_votes v WHERE v.comment_id = c.id AND v.user_id = ?2)
                 FROM discussion_comments c JOIN users u ON u.id = c.author_id WHERE c.discussion_id = ?1 ORDER BY c.id",
            )?;
            let comments: Vec<Value> = s
                .query_map([did, vid], |r| {
                    let id: i64 = r.get(0)?;
                    Ok(json!({ "id": id, "author": r.get::<_, String>(1)?, "body": r.get::<_, String>(3)?,
                        "created_at": r.get::<_, i64>(4)?, "updated_at": r.get::<_, i64>(5)?, "votes": r.get::<_, i64>(6)?,
                        "voted": r.get::<_, bool>(7)?, "is_answer": answer == Some(id), "can_edit": r.get::<_, i64>(2)? == vid || writer }))
                })?
                .collect::<Result<_, _>>()?;
            Ok((voted, comments))
        })
        .await?;
    let is_author = d.author_id == vid;
    let can_answer = d.category == "qa" && (is_author || writer);
    let mut disc = serde_json::to_value(&d).map_err(AppError::internal)?;
    disc["voted"] = json!(voted);
    Ok(Json(json!({ "discussion": disc, "comments": comments, "can_write": writer, "can_edit": is_author || writer,
        "can_answer": can_answer, "signed_in": v.is_some(), "categories": CATEGORIES.iter().map(|(id, name)| json!({ "id": id, "name": name })).collect::<Vec<_>>() })))
}

#[derive(Deserialize)]
pub struct UpdateReq {
    title: Option<String>,
    body: Option<String>,
    category: Option<String>,
}

pub async fn update(
    State(st): State<AppState>,
    Authed(u): Authed,
    Path((o, n, num)): Path<(String, String, i64)>,
    Json(req): Json<UpdateReq>,
) -> J<Value> {
    let r = Repo::readable(&st, Some(&u), &o, &n).await?;
    let d = load(&st, &r, num).await?;
    if d.author_id != u.id && !r.can_write {
        return Err(AppError::forbidden());
    }
    let cat = req.category.as_deref().map(category).transpose()?;
    if (cat == Some("announcements") || (cat.is_some() && d.category == "announcements")) && !r.can_write {
        return Err(AppError::new(axum::http::StatusCode::FORBIDDEN, "Only maintainers move announcements"));
    }
    check_text(req.title.as_deref().unwrap_or("x"), req.body.as_deref().unwrap_or(""))?;
    let did = d.id;
    st.db
        .call(move |c| {
            let tx = c.transaction()?;
            if let Some(t) = req.title {
                tx.execute("UPDATE discussions SET title = ?1 WHERE id = ?2", rusqlite::params![t.trim(), did])?;
            }
            if let Some(b) = req.body {
                tx.execute("UPDATE discussions SET body = ?1 WHERE id = ?2", rusqlite::params![b, did])?;
            }
            if let Some(cat) = cat {
                // Only Q&A threads have answers.
                tx.execute(
                    "UPDATE discussions SET category = ?1, answer_id = CASE WHEN ?1 = 'qa' THEN answer_id END WHERE id = ?2",
                    rusqlite::params![cat, did],
                )?;
            }
            tx.execute("UPDATE discussions SET updated_at = ?1 WHERE id = ?2", rusqlite::params![now(), did])?;
            tx.commit()?;
            Ok(())
        })
        .await?;
    ok()
}

pub async fn delete(
    State(st): State<AppState>,
    Authed(u): Authed,
    ip: ClientIp,
    Path((o, n, num)): Path<(String, String, i64)>,
) -> J<Value> {
    let r = Repo::writable(&st, Some(&u), &o, &n).await?;
    let d = load(&st, &r, num).await?;
    let did = d.id;
    st.db.call(move |c| Ok(c.execute("DELETE FROM discussions WHERE id = ?1", [did])?)).await?;
    let detail = format!("{} discussion #{num}: {}", r.full_name(), d.title);
    record(&st.db, Event { actor: Some(&u), subject: Some(r.owner_id), action: "discussion_deleted", detail, ip: &ip.0 }).await;
    ok()
}

#[derive(Deserialize)]
pub struct CommentReq {
    body: String,
}

fn check_body(body: &str) -> R<String> {
    let body = body.trim();
    if body.is_empty() || body.len() > 65_536 {
        return Err(AppError::bad("Write a reply (64 KB max)"));
    }
    Ok(body.to_string())
}

pub async fn comment(
    State(st): State<AppState>,
    Authed(u): Authed,
    Path((o, n, num)): Path<(String, String, i64)>,
    Json(req): Json<CommentReq>,
) -> J<Value> {
    let r = Repo::readable(&st, Some(&u), &o, &n).await?;
    let d = load(&st, &r, num).await?;
    let body = check_body(&req.body)?;
    if !st.limiter.allow(&format!("comment:{}", u.id), 60, 600) {
        return Err(AppError::new(axum::http::StatusCode::TOO_MANY_REQUESTS, "You're commenting very quickly. Try again in a few minutes."));
    }
    let (did, uid) = (d.id, u.id);
    let id = st
        .db
        .call(move |c| {
            let tx = c.transaction()?;
            let t = now();
            tx.execute(
                "INSERT INTO discussion_comments(discussion_id, author_id, body, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)",
                rusqlite::params![did, uid, body, t],
            )?;
            let id = tx.last_insert_rowid();
            tx.execute("UPDATE discussions SET updated_at = ?1 WHERE id = ?2", rusqlite::params![t, did])?;
            tx.commit()?;
            Ok(id)
        })
        .await?;
    Ok(Json(json!({ "id": id })))
}

/// A reply of this thread, and whether `uid` may change it.
async fn own_comment(st: &AppState, r: &Repo, d: &Row, id: i64, uid: i64) -> R<()> {
    let did = d.id;
    let author: i64 = st
        .db
        .call(move |c| Ok(c.query_row("SELECT author_id FROM discussion_comments WHERE id = ?1 AND discussion_id = ?2", [id, did], |r| r.get(0))?))
        .await?;
    if author != uid && !r.can_write {
        return Err(AppError::forbidden());
    }
    Ok(())
}

pub async fn edit_comment(
    State(st): State<AppState>,
    Authed(u): Authed,
    Path((o, n, num, id)): Path<(String, String, i64, i64)>,
    Json(req): Json<CommentReq>,
) -> J<Value> {
    let r = Repo::readable(&st, Some(&u), &o, &n).await?;
    let d = load(&st, &r, num).await?;
    own_comment(&st, &r, &d, id, u.id).await?;
    let body = check_body(&req.body)?;
    st.db
        .call(move |c| Ok(c.execute("UPDATE discussion_comments SET body = ?1, updated_at = ?2 WHERE id = ?3", rusqlite::params![body, now(), id])?))
        .await?;
    ok()
}

pub async fn delete_comment(
    State(st): State<AppState>,
    Authed(u): Authed,
    Path((o, n, num, id)): Path<(String, String, i64, i64)>,
) -> J<Value> {
    let r = Repo::readable(&st, Some(&u), &o, &n).await?;
    let d = load(&st, &r, num).await?;
    own_comment(&st, &r, &d, id, u.id).await?;
    st.db.call(move |c| Ok(c.execute("DELETE FROM discussion_comments WHERE id = ?1", [id])?)).await?;
    ok()
}

#[derive(Deserialize)]
pub struct AnswerReq {
    /// The reply that answers the question; null unmarks it.
    comment_id: Option<i64>,
}

pub async fn answer(
    State(st): State<AppState>,
    Authed(u): Authed,
    Path((o, n, num)): Path<(String, String, i64)>,
    Json(req): Json<AnswerReq>,
) -> J<Value> {
    let r = Repo::readable(&st, Some(&u), &o, &n).await?;
    let d = load(&st, &r, num).await?;
    if d.author_id != u.id && !r.can_write {
        return Err(AppError::forbidden());
    }
    if d.category != "qa" {
        return Err(AppError::bad("Only Q&A discussions have answers"));
    }
    let did = d.id;
    st.db
        .call(move |c| {
            if let Some(cid) = req.comment_id {
                c.query_row("SELECT 1 FROM discussion_comments WHERE id = ?1 AND discussion_id = ?2", [cid, did], |_| Ok(()))?;
            }
            Ok(c.execute("UPDATE discussions SET answer_id = ?1 WHERE id = ?2", rusqlite::params![req.comment_id, did])?)
        })
        .await?;
    ok()
}

#[derive(Deserialize)]
pub struct VoteReq {
    up: bool,
    /// A reply to vote for; absent means the thread itself.
    comment_id: Option<i64>,
}

pub async fn vote(
    State(st): State<AppState>,
    Authed(u): Authed,
    Path((o, n, num)): Path<(String, String, i64)>,
    Json(req): Json<VoteReq>,
) -> J<Value> {
    let r = Repo::readable(&st, Some(&u), &o, &n).await?;
    let d = load(&st, &r, num).await?;
    let (did, uid) = (d.id, u.id);
    let votes = st
        .db
        .call(move |c| {
            if let Some(cid) = req.comment_id {
                c.query_row("SELECT 1 FROM discussion_comments WHERE id = ?1 AND discussion_id = ?2", [cid, did], |_| Ok(()))?;
            }
            let p = rusqlite::params![uid, did, req.comment_id];
            if req.up {
                c.execute("INSERT OR IGNORE INTO discussion_votes(user_id, discussion_id, comment_id) VALUES (?1, ?2, ?3)", p)?;
            } else {
                c.execute("DELETE FROM discussion_votes WHERE user_id = ?1 AND discussion_id = ?2 AND comment_id IS ?3", p)?;
            }
            Ok(c.query_row(
                "SELECT count(*) FROM discussion_votes WHERE discussion_id = ?1 AND comment_id IS ?2",
                rusqlite::params![did, req.comment_id],
                |r| r.get::<_, i64>(0),
            )?)
        })
        .await?;
    Ok(Json(json!({ "votes": votes, "voted": req.up })))
}
