//! Code review on pull requests: comments on lines of the diff (threads with
//! replies, resolved or not, outdated once their lines change), reviews that
//! gather pending comments and end as approved, changes requested or a plain
//! comment, and suggestions a writer applies as a commit on the head branch.

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use rusqlite::{OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};

use super::issues::{IssueRow, load_issue, pull_shas};
use super::{J, ok};
use crate::AppState;
use crate::auth::{Authed, User, Viewer};
use crate::db::now;
use crate::error::{AppError, R};
use crate::git::{Change, CommitSpec};
use crate::repo::{Repo, record_event, touch};
use crate::smart_http::RefUpdate;
use crate::validate;

/// Files bigger than this aren't read for anchors (comments on them are refused).
const MAX_FILE: usize = 4 << 20;
const MAX_BODY: usize = 65_536;

/// A pull request with the shas its diff is between: (issue, head branch, head sha, merge base).
async fn pull(st: &AppState, r: &Repo, num: i64, viewer: Option<&User>) -> R<(IssueRow, String, String, String)> {
    let issue = load_issue(st, r, num, viewer).await?;
    if !issue.is_pr {
        return Err(AppError::not_found());
    }
    let (branch, head, base) = pull_shas(st, r, &issue).await?;
    let (Some(head), Some(base)) = (head, base) else {
        return Err(AppError::bad("This pull request's branches are gone"));
    };
    let mb = st.git.merge_base(&r.path, &base, &head).await?.unwrap_or(base);
    Ok((issue, branch, head, mb))
}

/// The lines of `path` at commit `sha`, or None if it isn't there (or is too big to read).
async fn lines_at(st: &AppState, r: &Repo, sha: &str, path: &str) -> R<Option<Vec<String>>> {
    let spec = format!("{sha}:{path}");
    let o = st.git.run(&r.path, &["cat-file", "blob", &spec], None, &[]).await?;
    if o.code != 0 || o.stdout.len() > MAX_FILE || o.stdout.contains(&0) {
        return Ok(None);
    }
    Ok(Some(String::from_utf8_lossy(&o.stdout).lines().map(String::from).collect()))
}

fn slice(lines: &[String], start: i64, end: i64) -> Option<String> {
    let (a, b) = (usize::try_from(start - 1).ok()?, usize::try_from(end).ok()?);
    (a < b && b <= lines.len()).then(|| lines[a..b].join("\n"))
}

/// Replace lines `start..=end` of `file` with `text`, if they still read as
/// `original`. Only those lines change: every other byte, and the file's line
/// endings (CRLF or LF), stay as they were.
fn splice(file: &[u8], start: i64, end: i64, original: &str, new: &[String]) -> Option<Vec<u8>> {
    let lines: Vec<&[u8]> = file.split_inclusive(|b| *b == b'\n').collect();
    let (a, b) = (usize::try_from(start - 1).ok()?, usize::try_from(end).ok()?);
    if a >= b || b > lines.len() {
        return None;
    }
    let bare = |l: &[u8]| -> String {
        let l = l.strip_suffix(b"\n").unwrap_or(l);
        String::from_utf8_lossy(l.strip_suffix(b"\r").unwrap_or(l)).into_owned()
    };
    if lines[a..b].iter().map(|l| bare(l)).collect::<Vec<_>>().join("\n") != original {
        return None;
    }
    let eol: &[u8] = if lines[a].ends_with(b"\r\n") { b"\r\n" } else { b"\n" };
    let last_has_eol = lines[b - 1].ends_with(b"\n");
    let mut out: Vec<u8> = lines[..a].concat();
    for (i, l) in new.iter().enumerate() {
        out.extend_from_slice(l.as_bytes());
        if i + 1 < new.len() || last_has_eol {
            out.extend_from_slice(eol);
        }
    }
    out.extend(lines[b..].concat());
    Some(out)
}

/// The first ```suggestion block of a comment: the lines it proposes (none:
/// delete them). Like markdown, it closes at a fence at least as long as the
/// one that opened it, so a suggestion can itself hold a ``` block.
fn suggestion(body: &str) -> Option<Vec<String>> {
    let mut open: Option<(usize, Vec<String>)> = None;
    for l in body.lines() {
        let t = l.trim_start();
        let ticks = t.bytes().take_while(|b| *b == b'`').count();
        match &mut open {
            None if ticks >= 3 && t[ticks..].trim() == "suggestion" => open = Some((ticks, vec![])),
            Some((n, v)) if ticks >= *n && t[ticks..].trim().is_empty() => return Some(std::mem::take(v)),
            Some((_, v)) => v.push(l.to_string()),
            None => {}
        }
    }
    None
}

#[derive(Deserialize)]
pub struct CommentReq {
    path: Option<String>,
    side: Option<String>,
    line: Option<i64>,
    start_line: Option<i64>,
    in_reply_to: Option<i64>,
    body: String,
    #[serde(default)]
    pending: bool,
}

/// Comment on a line or a range of lines (or reply to a thread). Alone it's
/// public at once; `pending` adds it to your review, which nobody else sees
/// until you submit it.
pub async fn comment(State(st): State<AppState>, Authed(u): Authed, Path((o, n, num)): Path<(String, String, i64)>, Json(req): Json<CommentReq>) -> J<Value> {
    let r = Repo::readable(&st, Some(&u), &o, &n).await?;
    let (issue, _, head, mb) = pull(&st, &r, num, Some(&u)).await?;
    let body = req.body.trim().to_string();
    if body.is_empty() || body.len() > MAX_BODY {
        return Err(AppError::bad("Write a comment (up to 64 KB)"));
    }
    if !st.limiter.allow(&format!("comment:{}", u.id), 60, 600) {
        return Err(AppError::new(StatusCode::TOO_MANY_REQUESTS, "You're commenting very quickly. Try again in a few minutes."));
    }
    let (iid, uid) = (issue.id, u.id);
    // A reply takes its thread's place in the diff.
    let anchor = match req.in_reply_to {
        Some(root) => {
            let a = st
                .db
                .call(move |c| {
                    Ok(c.query_row(
                        &format!(
                            "SELECT c.path, c.side, c.line, c.start_line, c.commit_sha, c.original FROM review_comments c LEFT JOIN reviews v ON v.id = c.review_id
                             WHERE c.id = ?1 AND c.issue_id = ?3 AND c.in_reply_to IS NULL AND {VISIBLE}"
                        ),
                        params![root, uid, iid],
                        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, Option<i64>>(3)?, r.get::<_, String>(4)?, r.get::<_, String>(5)?)),
                    )
                    .optional()?)
                })
                .await?;
            a.ok_or_else(|| AppError::bad("There's no such thread to reply to"))?
        }
        None => {
            let path = req.path.as_deref().unwrap_or("").trim_matches('/').to_string();
            let side = req.side.as_deref().unwrap_or("");
            let line = req.line.unwrap_or(0);
            let sha = match side {
                "new" => head.clone(),
                "old" => mb.clone(),
                _ => return Err(AppError::bad("A comment goes on the new or the old side of the diff")),
            };
            if !validate::write_path(&path) {
                return Err(AppError::bad("That isn't a file path"));
            }
            let start = req.start_line.filter(|s| *s != line);
            if start.is_some_and(|s| s > line) {
                return Err(AppError::bad("A range starts before it ends"));
            }
            // Only files the pull request changes (a comment elsewhere would never show in its diff).
            let changed = st.git.run(&r.path, &["diff", "--name-only", "--no-renames", &mb, &head], None, &[]).await?;
            if !String::from_utf8_lossy(&changed.stdout).lines().any(|l| l == path) {
                return Err(AppError::bad("That file isn't in this pull request"));
            }
            let lines = lines_at(&st, &r, &sha, &path).await?.ok_or_else(|| AppError::bad("That file isn't in this pull request"))?;
            let original = slice(&lines, start.unwrap_or(line), line).ok_or_else(|| AppError::bad("Those lines aren't in the file"))?;
            (path, side.to_string(), line, start, sha, original)
        }
    };
    let (path, side, line, start, sha, original) = anchor;
    let (reply, pending) = (req.in_reply_to, req.pending);
    let id = st
        .db
        .call(move |c| {
            let tx = c.transaction()?;
            let t = now();
            let review: Option<i64> = if pending {
                let open: Option<i64> = tx
                    .query_row("SELECT id FROM reviews WHERE issue_id = ?1 AND author_id = ?2 AND state = 'pending'", [iid, uid], |r| r.get(0))
                    .optional()?;
                Some(match open {
                    Some(id) => id,
                    None => {
                        tx.execute("INSERT INTO reviews(issue_id, author_id, commit_sha, created_at) VALUES (?1, ?2, ?3, ?4)", params![iid, uid, sha, t])?;
                        tx.last_insert_rowid()
                    }
                })
            } else {
                None
            };
            tx.execute(
                "INSERT INTO review_comments(issue_id, review_id, author_id, path, side, line, start_line, commit_sha, original, in_reply_to, body, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)",
                params![iid, review, uid, path, side, line, start, sha, original, reply, body, t],
            )?;
            let id = tx.last_insert_rowid();
            if !pending {
                tx.execute("UPDATE issues SET updated_at = ?1 WHERE id = ?2", params![t, iid])?;
            }
            tx.commit()?;
            Ok(id)
        })
        .await?;
    if !pending {
        record_event(&st.db, u.id, Some(r.id), "comment", &issue.title, &num.to_string(), 1).await?;
    }
    Ok(Json(json!({ "id": id })))
}

/// Who may see review comments: submitted ones, and your own pending ones.
const VISIBLE: &str = "(c.review_id IS NULL OR v.state != 'pending' OR v.author_id = ?2)";

/// Threads (with replies, outdated or not), submitted reviews, your pending
/// review, and who approved or asked for changes.
pub async fn list(State(st): State<AppState>, Viewer(v): Viewer, Path((o, n, num)): Path<(String, String, i64)>) -> J<Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let (issue, _, head, mb) = pull(&st, &r, num, v.as_ref()).await?;
    let (iid, vid) = (issue.id, v.as_ref().map_or(-1, |u| u.id));
    let (rows, reviews, pending, summary) = st
        .db
        .call(move |c| {
            let mut q = c.prepare(&format!(
                "SELECT c.id, c.in_reply_to, u.username, c.author_id, c.path, c.side, c.line, c.start_line, c.commit_sha, c.original, c.body,
                    c.created_at, c.updated_at, ru.username, c.review_id, v.state
                 FROM review_comments c JOIN users u ON u.id = c.author_id LEFT JOIN reviews v ON v.id = c.review_id
                 LEFT JOIN users ru ON ru.id = c.resolved_by
                 WHERE c.issue_id = ?1 AND {VISIBLE} ORDER BY c.id"
            ))?;
            let rows: Vec<Value> = q
                .query_map(params![iid, vid], |r| {
                    Ok(json!({ "id": r.get::<_, i64>(0)?, "in_reply_to": r.get::<_, Option<i64>>(1)?, "author": r.get::<_, String>(2)?,
                        "author_id": r.get::<_, i64>(3)?, "path": r.get::<_, String>(4)?, "side": r.get::<_, String>(5)?, "line": r.get::<_, i64>(6)?,
                        "start_line": r.get::<_, Option<i64>>(7)?, "commit_sha": r.get::<_, String>(8)?, "original": r.get::<_, String>(9)?,
                        "body": r.get::<_, String>(10)?, "created_at": r.get::<_, i64>(11)?, "updated_at": r.get::<_, i64>(12)?,
                        "resolved_by": r.get::<_, Option<String>>(13)?, "review_id": r.get::<_, Option<i64>>(14)?,
                        "pending": r.get::<_, Option<String>>(15)?.as_deref() == Some("pending") }))
                })?
                .collect::<Result<_, _>>()?;
            let mut q = c.prepare(
                "SELECT v.id, u.username, v.state, v.body, v.commit_sha, v.submitted_at, (SELECT count(*) FROM review_comments c WHERE c.review_id = v.id)
                 FROM reviews v JOIN users u ON u.id = v.author_id WHERE v.issue_id = ?1 AND v.state != 'pending' ORDER BY v.submitted_at, v.id",
            )?;
            let reviews: Vec<Value> = q
                .query_map([iid], |r| {
                    Ok(json!({ "id": r.get::<_, i64>(0)?, "author": r.get::<_, String>(1)?, "state": r.get::<_, String>(2)?, "body": r.get::<_, String>(3)?,
                        "commit_sha": r.get::<_, String>(4)?, "submitted_at": r.get::<_, Option<i64>>(5)?, "comments": r.get::<_, i64>(6)? }))
                })?
                .collect::<Result<_, _>>()?;
            let pending = c
                .query_row(
                    "SELECT v.id, (SELECT count(*) FROM review_comments c WHERE c.review_id = v.id) FROM reviews v WHERE v.issue_id = ?1 AND v.author_id = ?2 AND v.state = 'pending'",
                    [iid, vid],
                    |r| Ok(json!({ "id": r.get::<_, i64>(0)?, "comments": r.get::<_, i64>(1)? })),
                )
                .optional()?;
            Ok((rows, reviews, pending, decisions(c, iid)?))
        })
        .await?;
    // Threads: a first comment with its replies, outdated when its lines read differently now.
    let mut files: HashMap<(String, String), Option<Vec<String>>> = HashMap::new();
    let mut threads: Vec<Value> = Vec::new();
    let mut index: HashMap<i64, usize> = HashMap::new();
    let editor = |c: &Value| c["author_id"].as_i64() == Some(vid);
    for mut c in rows {
        let mine = editor(&c);
        c["can_edit"] = json!(mine);
        c["can_delete"] = json!(mine || r.can_write);
        if let Some(root) = c["in_reply_to"].as_i64() {
            if let Some(&i) = index.get(&root) {
                threads[i]["comments"].as_array_mut().unwrap().push(c);
            }
            continue;
        }
        let side_sha = if c["side"] == "old" { mb.clone() } else { head.clone() };
        let outdated = if c["commit_sha"].as_str() == Some(side_sha.as_str()) {
            false
        } else {
            let path = c["path"].as_str().unwrap_or_default().to_string();
            let key = (side_sha.clone(), path.clone());
            if !files.contains_key(&key) {
                let l = lines_at(&st, &r, &side_sha, &path).await?;
                files.insert(key.clone(), l);
            }
            let (line, start) = (c["line"].as_i64().unwrap_or(0), c["start_line"].as_i64());
            files[&key].as_deref().and_then(|l| slice(l, start.unwrap_or(line), line)).as_deref() != c["original"].as_str()
        };
        let resolver = c["resolved_by"].clone();
        let can_resolve = vid >= 0 && (r.can_write || issue.author_id == vid || mine);
        let has_suggestion = c["body"].as_str().and_then(suggestion).is_some();
        let thread = json!({ "id": c["id"], "path": c["path"], "side": c["side"], "line": c["line"], "start_line": c["start_line"],
            "original": c["original"], "outdated": outdated, "resolved_by": resolver, "review_id": c["review_id"], "pending": c["pending"],
            "can_resolve": can_resolve, "can_apply": has_suggestion && r.can_write && issue.state == "open" && c["side"] == "new" && !outdated && resolver.is_null(),
            "comments": [c] });
        index.insert(thread["id"].as_i64().unwrap_or(0), threads.len());
        threads.push(thread);
    }
    Ok(Json(json!({ "threads": threads, "reviews": reviews, "pending": pending, "summary": summary, "head_sha": head })))
}

/// Each reviewer's latest decision: who approved, who asked for changes.
fn decisions(c: &rusqlite::Connection, iid: i64) -> rusqlite::Result<Value> {
    let mut q = c.prepare(
        "SELECT u.username, v.state FROM reviews v JOIN users u ON u.id = v.author_id
         WHERE v.issue_id = ?1 AND v.state IN ('approved', 'changes_requested') AND NOT EXISTS (
             SELECT 1 FROM reviews x WHERE x.issue_id = v.issue_id AND x.author_id = v.author_id AND x.state IN ('approved', 'changes_requested')
             AND (x.submitted_at > v.submitted_at OR (x.submitted_at = v.submitted_at AND x.id > v.id)))
         ORDER BY u.username",
    )?;
    let (mut approved, mut changes) = (Vec::new(), Vec::new());
    for row in q.query_map([iid], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (who, state) = row?;
        if state == "approved" { approved.push(who) } else { changes.push(who) }
    }
    Ok(json!({ "approved": approved, "changes_requested": changes }))
}

#[derive(Deserialize)]
pub struct SubmitReq {
    event: String,
    #[serde(default)]
    body: String,
}

/// Submit your review: approve, request changes, or comment. Your pending
/// comments go with it and become visible.
pub async fn submit(State(st): State<AppState>, Authed(u): Authed, Path((o, n, num)): Path<(String, String, i64)>, Json(req): Json<SubmitReq>) -> J<Value> {
    let r = Repo::readable(&st, Some(&u), &o, &n).await?;
    let (issue, _, head, _) = pull(&st, &r, num, Some(&u)).await?;
    let state = match req.event.as_str() {
        "approve" => "approved",
        "request_changes" => "changes_requested",
        "comment" => "commented",
        _ => return Err(AppError::bad("A review approves, requests changes or comments")),
    };
    if state != "commented" && issue.author_id == u.id {
        return Err(AppError::bad("You can't approve or request changes on your own pull request"));
    }
    let body = req.body.trim().to_string();
    if body.len() > MAX_BODY {
        return Err(AppError::bad("That's too long (64 KB max)"));
    }
    if !st.limiter.allow(&format!("comment:{}", u.id), 60, 600) {
        return Err(AppError::new(StatusCode::TOO_MANY_REQUESTS, "You're reviewing very quickly. Try again in a few minutes."));
    }
    let (iid, uid) = (issue.id, u.id);
    let id = st
        .db
        .call(move |c| {
            let tx = c.transaction()?;
            let t = now();
            let open: Option<(i64, i64)> = tx
                .query_row(
                    "SELECT v.id, (SELECT count(*) FROM review_comments c WHERE c.review_id = v.id) FROM reviews v WHERE v.issue_id = ?1 AND v.author_id = ?2 AND v.state = 'pending'",
                    [iid, uid],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            let has_comments = open.is_some_and(|o| o.1 > 0);
            if state != "approved" && body.is_empty() && !has_comments {
                return Err(AppError::bad(if state == "changes_requested" { "Say what should change" } else { "Write a comment, or add some on lines first" }));
            }
            let id = match open {
                Some((id, _)) => {
                    tx.execute("UPDATE reviews SET state = ?2, body = ?3, commit_sha = ?4, submitted_at = ?5 WHERE id = ?1", params![id, state, body, head, t])?;
                    id
                }
                None => {
                    tx.execute(
                        "INSERT INTO reviews(issue_id, author_id, state, body, commit_sha, created_at, submitted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
                        params![iid, uid, state, body, head, t],
                    )?;
                    tx.last_insert_rowid()
                }
            };
            tx.execute("UPDATE issues SET updated_at = ?1 WHERE id = ?2", params![t, iid])?;
            tx.commit()?;
            Ok(id)
        })
        .await?;
    record_event(&st.db, u.id, Some(r.id), "comment", &issue.title, &num.to_string(), 1).await?;
    Ok(Json(json!({ "id": id, "state": state })))
}

/// Throw away your pending review and its comments.
pub async fn discard(State(st): State<AppState>, Authed(u): Authed, Path((o, n, num)): Path<(String, String, i64)>) -> J<Value> {
    let r = Repo::readable(&st, Some(&u), &o, &n).await?;
    let issue = load_issue(&st, &r, num, Some(&u)).await?;
    let (iid, uid) = (issue.id, u.id);
    st.db.call(move |c| Ok(c.execute("DELETE FROM reviews WHERE issue_id = ?1 AND author_id = ?2 AND state = 'pending'", [iid, uid])?)).await?;
    ok()
}

/// A review comment of this pull request that `u` may see: (author, root id or self, resolved?, side, body, path, line, start, original).
type Found = (i64, Option<i64>, bool, String, String, String, i64, Option<i64>, String);

async fn find(st: &AppState, iid: i64, id: i64, u: &User) -> R<Found> {
    let uid = u.id;
    st.db
        .call(move |c| {
            Ok(c.query_row(
                &format!(
                    "SELECT c.author_id, c.in_reply_to, c.resolved_by IS NOT NULL, c.side, c.body, c.path, c.line, c.start_line, c.original
                     FROM review_comments c LEFT JOIN reviews v ON v.id = c.review_id WHERE c.id = ?1 AND c.issue_id = ?3 AND {VISIBLE}"
                ),
                params![id, uid, iid],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?)),
            )
            .optional()?)
        })
        .await?
        .ok_or_else(AppError::not_found)
}

#[derive(Deserialize)]
pub struct EditReq {
    body: String,
}

pub async fn edit(State(st): State<AppState>, Authed(u): Authed, Path((o, n, num, id)): Path<(String, String, i64, i64)>, Json(req): Json<EditReq>) -> J<Value> {
    let r = Repo::readable(&st, Some(&u), &o, &n).await?;
    let issue = load_issue(&st, &r, num, Some(&u)).await?;
    let (author, ..) = find(&st, issue.id, id, &u).await?;
    if author != u.id {
        return Err(AppError::new(StatusCode::FORBIDDEN, "Only its author can edit a comment"));
    }
    let body = req.body.trim().to_string();
    if body.is_empty() || body.len() > MAX_BODY {
        return Err(AppError::bad("Write a comment (up to 64 KB)"));
    }
    st.db.call(move |c| Ok(c.execute("UPDATE review_comments SET body = ?2, updated_at = ?3 WHERE id = ?1", params![id, body, now()])?)).await?;
    ok()
}

/// Delete a comment (its author or a writer). The first comment takes its thread with it.
pub async fn delete(State(st): State<AppState>, Authed(u): Authed, Path((o, n, num, id)): Path<(String, String, i64, i64)>) -> J<Value> {
    let r = Repo::readable(&st, Some(&u), &o, &n).await?;
    let issue = load_issue(&st, &r, num, Some(&u)).await?;
    let (author, ..) = find(&st, issue.id, id, &u).await?;
    if author != u.id && !r.can_write {
        return Err(AppError::new(StatusCode::FORBIDDEN, "Only its author or people with write access can delete a comment"));
    }
    st.db
        .call(move |c| {
            let review: Option<i64> = c.query_row("SELECT review_id FROM review_comments WHERE id = ?1", [id], |r| r.get(0))?;
            c.execute("DELETE FROM review_comments WHERE id = ?1", [id])?;
            // A pending review left with nothing in it goes too.
            if let Some(rv) = review {
                c.execute("DELETE FROM reviews WHERE id = ?1 AND state = 'pending' AND NOT EXISTS (SELECT 1 FROM review_comments WHERE review_id = ?1)", [rv])?;
            }
            Ok(())
        })
        .await?;
    ok()
}

#[derive(Deserialize)]
pub struct ResolveReq {
    resolved: bool,
}

/// Mark a thread resolved (or not): the pull request's author, the thread's author, or a writer.
pub async fn resolve(State(st): State<AppState>, Authed(u): Authed, Path((o, n, num, id)): Path<(String, String, i64, i64)>, Json(req): Json<ResolveReq>) -> J<Value> {
    let r = Repo::readable(&st, Some(&u), &o, &n).await?;
    let issue = load_issue(&st, &r, num, Some(&u)).await?;
    let (author, root, ..) = find(&st, issue.id, id, &u).await?;
    if root.is_some() {
        return Err(AppError::bad("Resolve the thread from its first comment"));
    }
    if !(r.can_write || issue.author_id == u.id || author == u.id) {
        return Err(AppError::new(StatusCode::FORBIDDEN, "Only the pull request's author, the thread's author or writers can resolve it"));
    }
    let by = req.resolved.then_some(u.id);
    st.db
        .call(move |c| Ok(c.execute("UPDATE review_comments SET resolved_by = ?2, resolved_at = ?3 WHERE id = ?1", params![id, by, by.map(|_| now())])?))
        .await?;
    ok()
}

#[derive(Deserialize)]
pub struct ApplyReq {
    /// The comment's updated_at as shown: an edit since then is refused, so
    /// what's committed is the suggestion the writer read.
    updated_at: i64,
}

/// Apply a comment's suggestion as a commit on the head branch, if its lines
/// still read as they did when it was written. Resolves the thread.
pub async fn apply(
    State(st): State<AppState>,
    Authed(u): Authed,
    Path((o, n, num, id)): Path<(String, String, i64, i64)>,
    Json(req): Json<ApplyReq>,
) -> J<Value> {
    let r = Repo::writable(&st, Some(&u), &o, &n).await?;
    let (issue, branch, head, _) = pull(&st, &r, num, Some(&u)).await?;
    if issue.state != "open" {
        return Err(AppError::bad("This pull request isn't open"));
    }
    let (_, root, resolved, side, body, path, line, start, original) = find(&st, issue.id, id, &u).await?;
    let text = suggestion(&body).ok_or_else(|| AppError::bad("That comment has no suggestion to apply"))?;
    let seen: i64 = st.db.call(move |c| Ok(c.query_row("SELECT updated_at FROM review_comments WHERE id = ?1", [id], |r| r.get(0))?)).await?;
    if seen != req.updated_at {
        return Err(AppError::conflict("The suggestion was edited since you loaded it. Reload to see what it says now."));
    }
    if root.is_some() || side != "new" {
        return Err(AppError::bad("Only suggestions on the new side of a thread's first comment can be applied"));
    }
    if resolved {
        return Err(AppError::conflict("That thread is resolved already"));
    }
    let o = st.git.run(&r.path, &["cat-file", "blob", &format!("{head}:{path}")], None, &[]).await?;
    if o.code != 0 || o.stdout.len() > MAX_FILE {
        return Err(AppError::conflict("That file isn't on the branch any more"));
    }
    let content = splice(&o.stdout, start.unwrap_or(line), line, &original, &text)
        .ok_or_else(|| AppError::conflict("Those lines changed since the suggestion was made. Make the change by hand, or suggest it again."))?;
    let mode = st.git.mode_of(&r.path, &head, &path).await?;
    if mode.as_deref() == Some("120000") {
        return Err(AppError::bad("That file is a symbolic link"));
    }
    crate::repo::need_room(&st, r.owner_id, content.len()).await?;
    let changes = [Change::Write(&path, &content, mode.as_deref() == Some("100755"))];
    crate::scan::guard_commit(&st, &r, &u, Some(&head), &changes).await?;
    let (an, ae) = u.signature();
    let message = format!("Apply suggestion from review\n\nOn {path} line {line} of #{num}.");
    let spec = CommitSpec { branch: &branch, parent: Some(&head), expect_old: Some(&head), changes: &changes, message: &message, author: (&an, &ae) };
    let sha = st.git.commit_files(&r.path, &spec).await?;
    let uid = u.id;
    st.db.call(move |c| Ok(c.execute("UPDATE review_comments SET resolved_by = ?2, resolved_at = ?3 WHERE id = ?1", params![id, uid, now()])?)).await?;
    touch(&st.db, r.id, true).await?;
    record_event(&st.db, u.id, Some(r.id), "push", &branch, &sha, 1).await?;
    let upd = RefUpdate { refname: format!("refs/heads/{branch}"), old: Some(head), new: Some(sha.clone()) };
    crate::hooks::on_push(&st, &r, &u, &[upd]).await;
    Ok(Json(json!({ "sha": sha })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splicing_keeps_line_endings_and_other_bytes() {
        let v = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(splice(b"a\r\nb\r\nc\r\n", 2, 2, "b", &v(&["B", "B2"])).unwrap(), b"a\r\nB\r\nB2\r\nc\r\n");
        assert_eq!(splice(b"a\nb", 2, 2, "b", &v(&["x"])).unwrap(), b"a\nx", "no final newline stays that way");
        assert_eq!(splice(b"\xe9t\xe9\nb\n", 2, 2, "b", &v(&["c"])).unwrap(), b"\xe9t\xe9\nc\n", "other bytes untouched");
        assert_eq!(splice(b"a\nb\nc\n", 1, 2, "a\nb", &[]).unwrap(), b"c\n", "an empty suggestion deletes the lines");
        assert_eq!(splice(b"a\nb\n", 1, 1, "a", &v(&[""])).unwrap(), b"\nb\n", "a blank line stays a blank line");
        assert!(splice(b"a\nb\n", 2, 2, "x", &v(&["y"])).is_none(), "lines changed");
        assert!(splice(b"a\n", 2, 2, "", &v(&["y"])).is_none());
    }

    #[test]
    fn suggestions_and_slices() {
        assert_eq!(suggestion("Try:\n```suggestion\nTwo\nlines\n```\nthanks").unwrap(), ["Two", "lines"]);
        assert_eq!(suggestion("```suggestion\n```").unwrap(), Vec::<String>::new(), "an empty suggestion deletes the lines");
        assert_eq!(suggestion("```suggestion\n\n```").unwrap(), [""], "one blank line");
        assert_eq!(suggestion("````suggestion\n```sh\nls\n```\n````").unwrap(), ["```sh", "ls", "```"], "a longer fence holds a block");
        assert_eq!(suggestion("```rust\nx\n```"), None);
        assert_eq!(suggestion("```suggestion\nunclosed"), None);
        let l: Vec<String> = ["a", "b", "c"].map(String::from).to_vec();
        assert_eq!((slice(&l, 2, 3).as_deref(), slice(&l, 1, 1).as_deref()), (Some("b\nc"), Some("a")));
        assert_eq!((slice(&l, 0, 1), slice(&l, 3, 4), slice(&l, 3, 2)), (None, None, None));
    }
}
