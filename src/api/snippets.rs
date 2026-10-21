//! Snippets: a few files someone shares, public or secret (unlisted: only the
//! link finds it). Each is a bare git repository (GITTER_DATA/snippets/<id>.git),
//! so every edit is a revision.

use std::collections::HashSet;
use std::path::PathBuf;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::response::Response;
use serde::Deserialize;
use serde_json::{Value, json};
use sha1::Digest;

use super::{ClientIp, J, ok};
use crate::auth::{Authed, User, Viewer};
use crate::db::now;
use crate::error::{AppError, R};
use crate::git::{Change, CommitSpec, LogQuery};
use crate::security::{Event, record};
use crate::{AppState, validate};

const BRANCH: &str = "main";
const MAX_FILES: usize = 20;
const MAX_BYTES: usize = 1024 * 1024;

pub fn path(st: &AppState, id: &str) -> PathBuf {
    st.cfg.data_dir.join("snippets").join(format!("{id}.git"))
}

fn valid_id(s: &str) -> bool {
    s.len() == 32 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

struct Snippet {
    id: String,
    owner_id: i64,
    owner: String,
    title: String,
    secret: bool,
    created_at: i64,
    updated_at: i64,
}

async fn load(st: &AppState, id: &str) -> R<Snippet> {
    if !valid_id(id) {
        return Err(AppError::not_found());
    }
    let id = id.to_string();
    st.db
        .call(move |c| {
            Ok(c.query_row(
                "SELECT s.id, s.owner_id, u.username, s.title, s.is_secret, s.created_at, s.updated_at FROM snippets s JOIN users u ON u.id = s.owner_id WHERE s.id = ?1",
                [id],
                |r| {
                    Ok(Snippet {
                        id: r.get(0)?,
                        owner_id: r.get(1)?,
                        owner: r.get(2)?,
                        title: r.get(3)?,
                        secret: r.get::<_, i64>(4)? != 0,
                        created_at: r.get(5)?,
                        updated_at: r.get(6)?,
                    })
                },
            )?)
        })
        .await
}

/// A deploy token sees snippets as if signed out (it's for one repository).
fn person(v: Option<&User>) -> Option<&User> {
    v.filter(|u| u.limits.as_ref().is_none_or(|l| l.repo_id.is_none()))
}

fn may_edit(s: &Snippet, u: &User) -> bool {
    person(Some(u)).is_some_and(|u| u.id == s.owner_id || u.is_admin)
}

#[derive(Deserialize)]
pub struct FileIn {
    #[serde(default)]
    name: String,
    content: String,
}

#[derive(Deserialize)]
pub struct SaveReq {
    #[serde(default)]
    title: String,
    secret: Option<bool>,
    files: Vec<FileIn>,
    /// The revision the editor started from; the save fails if the snippet moved on.
    expected_sha: Option<String>,
}

/// (name, content) of each file.
type Files<'a> = Vec<(String, &'a [u8])>;

/// Flat file names (unnamed ones get `snippetfile<n>.txt`), each non-empty, 1 MB in all.
fn check(req: &SaveReq) -> R<(String, Files<'_>)> {
    let title = req.title.trim().to_string();
    if title.chars().count() > 256 {
        return Err(AppError::bad("Keep the title under 256 characters"));
    }
    if req.files.is_empty() || req.files.len() > MAX_FILES {
        return Err(AppError::bad(format!("A snippet has 1 to {MAX_FILES} files")));
    }
    let (mut out, mut seen, mut total) = (Vec::new(), HashSet::new(), 0);
    for (i, f) in req.files.iter().enumerate() {
        let name = match f.name.trim() {
            "" => format!("snippetfile{}.txt", i + 1),
            n => n.to_string(),
        };
        if name.contains('/') || name.chars().count() > 100 || !validate::write_path(&name) {
            return Err(AppError::bad(format!("'{name}' isn't a valid file name")));
        }
        if !seen.insert(name.clone()) {
            return Err(AppError::bad(format!("Two files are called {name}")));
        }
        if f.content.trim().is_empty() {
            return Err(AppError::bad(format!("{name} is empty")));
        }
        total += f.content.len();
        if total > MAX_BYTES {
            return Err(AppError::bad("A snippet can be up to 1 MB"));
        }
        out.push((name, f.content.as_bytes()));
    }
    Ok((title, out))
}

fn file_list(files: &[(String, &[u8])]) -> String {
    files.iter().map(|f| f.0.as_str()).collect::<Vec<_>>().join("\n")
}

pub async fn create(State(st): State<AppState>, Authed(u): Authed, Json(req): Json<SaveReq>) -> J<Value> {
    let (title, files) = check(&req)?;
    if !st.limiter.allow(&format!("snippet:{}", u.id), 30, 3600) {
        return Err(AppError::new(axum::http::StatusCode::TOO_MANY_REQUESTS, "You're making snippets very quickly. Try again later."));
    }
    let mut b = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut b);
    let id = hex::encode(b);
    let (id2, uid, secret, names) = (id.clone(), u.id, req.secret.unwrap_or(false), file_list(&files));
    st.db
        .call(move |c| {
            let t = now();
            c.execute(
                "INSERT INTO snippets(id, owner_id, title, is_secret, files, created_at, updated_at) VALUES (?1,?2,?3,?4,?5,?6,?6)",
                rusqlite::params![id2, uid, title, secret as i64, names, t],
            )?;
            Ok(())
        })
        .await?;
    let p = path(&st, &id);
    let res = async {
        tokio::fs::create_dir_all(p.parent().unwrap_or(&p)).await.map_err(AppError::internal)?;
        st.git.init_bare(&p, BRANCH, st.settings.get().max_push_bytes()).await?;
        let changes: Vec<Change> = files.iter().map(|(n, d)| Change::Write(n, d, false)).collect();
        let (an, ae) = u.signature();
        let spec = CommitSpec { branch: BRANCH, parent: None, expect_old: None, changes: &changes, message: "Create snippet", author: (&an, &ae) };
        st.git.commit_files(&p, &spec).await
    }
    .await;
    if let Err(e) = res {
        let id2 = id.clone();
        let _ = st.db.call(move |c| Ok(c.execute("DELETE FROM snippets WHERE id = ?1", [id2])?)).await;
        let _ = tokio::fs::remove_dir_all(&p).await;
        return Err(e);
    }
    Ok(Json(json!({ "id": id })))
}

#[derive(Deserialize)]
pub struct ListQ {
    user: Option<String>,
    page: Option<usize>,
}

/// Public snippets, newest first, or one person's (with their secret ones when it's you).
pub async fn list(State(st): State<AppState>, Viewer(v): Viewer, Query(q): Query<ListQ>) -> J<Value> {
    let me = person(v.as_ref()).map_or(-1, |u| u.id);
    let (page, per) = (super::page(q.page), 30);
    let user = q.user.filter(|s| !s.is_empty());
    let mut items: Vec<Value> = st
        .db
        .call(move |c| {
            let sql = format!(
                "SELECT s.id, u.username, s.title, s.is_secret, s.files, s.created_at, s.updated_at FROM snippets s JOIN users u ON u.id = s.owner_id
                 WHERE (?1 IS NULL OR u.username = ?1) AND (s.is_secret = 0 OR (?1 IS NOT NULL AND s.owner_id = ?2))
                 ORDER BY s.updated_at DESC, s.rowid DESC LIMIT {} OFFSET {}",
                per + 1,
                (page - 1) * per
            );
            let mut s = c.prepare(&sql)?;
            let rows = s.query_map(rusqlite::params![user, me], |r| {
                let files: String = r.get(4)?;
                Ok(json!({ "id": r.get::<_, String>(0)?, "owner": r.get::<_, String>(1)?, "title": r.get::<_, String>(2)?,
                    "secret": r.get::<_, i64>(3)? != 0, "files": files.split('\n').filter(|f| !f.is_empty()).collect::<Vec<_>>(),
                    "created_at": r.get::<_, i64>(5)?, "updated_at": r.get::<_, i64>(6)? }))
            })?;
            Ok(rows.collect::<Result<_, _>>()?)
        })
        .await?;
    let more = items.len() > per;
    items.truncate(per);
    Ok(Json(json!({ "items": items, "has_more": more, "page": page })))
}

#[derive(Deserialize)]
pub struct RevQ {
    rev: Option<String>,
    page: Option<usize>,
}

async fn head(st: &AppState, p: &std::path::Path) -> R<String> {
    st.git.resolve(p, &format!("refs/heads/{BRANCH}")).await?.ok_or_else(AppError::not_found)
}

/// A 40-hex commit of this snippet, or its latest one.
async fn revision(st: &AppState, p: &std::path::Path, rev: Option<&str>) -> R<String> {
    match rev.filter(|r| !r.is_empty() && *r != "latest") {
        Some(r) if r.len() == 40 && validate::sha(r) => st.git.resolve(p, r).await?.ok_or_else(AppError::not_found),
        Some(_) => Err(AppError::not_found()),
        None => head(st, p).await,
    }
}

pub async fn show(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    Path(id): Path<String>,
    Query(q): Query<RevQ>,
) -> J<Value> {
    let s = load(&st, &id).await?;
    let p = path(&st, &s.id);
    let latest = head(&st, &p).await?;
    let at = revision(&st, &p, q.rev.as_deref()).await?;
    let mut files = Vec::new();
    for e in st.git.ls_tree(&p, &at, "").await?.into_iter().filter(|e| e.kind == "blob") {
        let content = String::from_utf8_lossy(&st.git.blob(&p, &e.oid).await?).into_owned();
        let language = crate::langs::detect(&e.name).map(|l| l.0);
        files.push(json!({ "name": e.name, "size": e.size, "content": content, "language": language }));
    }
    let revisions = st.git.count(&p, &latest).await?;
    let can_edit = v.as_ref().is_some_and(|u| may_edit(&s, u));
    Ok(Json(json!({ "id": s.id, "owner": s.owner, "title": s.title, "secret": s.secret, "created_at": s.created_at,
        "updated_at": s.updated_at, "sha": at, "latest": at == latest, "files": files, "revisions": revisions, "can_edit": can_edit })))
}

/// Git's blob id for `data`, to spot files that didn't change.
fn blob_oid(data: &[u8]) -> String {
    let mut h = sha1::Sha1::new();
    h.update(format!("blob {}\0", data.len()));
    h.update(data);
    hex::encode(h.finalize())
}

/// Replace the snippet's files (a new revision, unless nothing in them changed) and its title or visibility.
pub async fn update(
    State(st): State<AppState>,
    Authed(u): Authed,
    Path(id): Path<String>,
    Json(req): Json<SaveReq>,
) -> J<Value> {
    let s = load(&st, &id).await?;
    if !may_edit(&s, &u) {
        return Err(AppError::forbidden());
    }
    let (title, files) = check(&req)?;
    let p = path(&st, &s.id);
    let h = head(&st, &p).await?;
    if req.expected_sha.as_ref().is_some_and(|e| *e != h) {
        return Err(AppError::conflict("This snippet changed since you opened it. Reload to see the new version."));
    }
    let current: Vec<(String, String)> = st
        .git
        .ls_tree(&p, &h, "")
        .await?
        .into_iter()
        .filter(|e| e.kind == "blob")
        .map(|e| (e.name, e.oid))
        .collect();
    let wanted: Vec<(String, String)> = files.iter().map(|(n, d)| (n.clone(), blob_oid(d))).collect();
    let mut sha = h.clone();
    if current.iter().collect::<HashSet<_>>() != wanted.iter().collect::<HashSet<_>>() {
        let keep: HashSet<&str> = files.iter().map(|f| f.0.as_str()).collect();
        let mut changes: Vec<Change> = current.iter().filter(|c| !keep.contains(c.0.as_str())).map(|c| Change::Delete(&c.0)).collect();
        changes.extend(files.iter().map(|(n, d)| Change::Write(n, d, false)));
        let (an, ae) = u.signature();
        let spec = CommitSpec { branch: BRANCH, parent: Some(&h), expect_old: Some(&h), changes: &changes, message: "Update snippet", author: (&an, &ae) };
        sha = st.git.commit_files(&p, &spec).await?;
    }
    let (sid, secret, names) = (s.id.clone(), req.secret.unwrap_or(s.secret), file_list(&files));
    st.db
        .call(move |c| {
            Ok(c.execute(
                "UPDATE snippets SET title = ?1, is_secret = ?2, files = ?3, updated_at = ?4 WHERE id = ?5",
                rusqlite::params![title, secret as i64, names, now(), sid],
            )?)
        })
        .await?;
    Ok(Json(json!({ "sha": sha })))
}

pub async fn delete(State(st): State<AppState>, Authed(u): Authed, ip: ClientIp, Path(id): Path<String>) -> J<Value> {
    let s = load(&st, &id).await?;
    if !may_edit(&s, &u) {
        return Err(AppError::forbidden());
    }
    let sid = s.id.clone();
    st.db.call(move |c| Ok(c.execute("DELETE FROM snippets WHERE id = ?1", [sid])?)).await?;
    let p = path(&st, &s.id);
    if let Err(e) = tokio::fs::remove_dir_all(&p).await {
        eprintln!("removing {}: {e}", p.display());
    }
    let detail = format!("{} ({})", if s.title.is_empty() { "untitled" } else { &s.title }, s.id);
    record(&st.db, Event { actor: Some(&u), subject: Some(s.owner_id), action: "snippet_deleted", detail, ip: &ip.0 }).await;
    ok()
}

pub async fn revisions(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<RevQ>,
) -> J<Value> {
    let s = load(&st, &id).await?;
    let p = path(&st, &s.id);
    let h = head(&st, &p).await?;
    let (page, per) = (super::page(q.page), 30);
    let lq = LogQuery { range: &h, path: None, skip: (page - 1) * per, limit: per + 1, grep: None, author: None, first_parent: false };
    let mut commits = st.git.log(&p, &lq).await?;
    let more = commits.len() > per;
    commits.truncate(per);
    Ok(Json(json!({ "commits": commits, "has_more": more, "page": page })))
}

/// `/raw/snippets/{id}/{rev or "latest"}/{file}`, served as inert text.
pub async fn raw(State(st): State<AppState>, Path((id, rest)): Path<(String, String)>) -> R<Response> {
    let s = load(&st, &id).await?;
    let (rev, file) = rest.split_once('/').ok_or_else(AppError::not_found)?;
    if file.contains('/') || !validate::path(file) {
        return Err(AppError::not_found());
    }
    let p = path(&st, &s.id);
    let at = revision(&st, &p, Some(rev)).await?;
    let (oid, kind, _) = st.git.stat(&p, &at, file).await?.ok_or_else(AppError::not_found)?;
    if kind != "blob" {
        return Err(AppError::not_found());
    }
    let data = st.git.blob(&p, &oid).await?;
    Ok(super::code::inert(axum::body::Body::from(data), "text/plain; charset=utf-8", None, rev.len() == 40, s.secret))
}

/// Snippet folders of one account, for removing them with it.
pub fn ids_of(c: &rusqlite::Connection, owner: i64) -> rusqlite::Result<Vec<String>> {
    let mut s = c.prepare("SELECT id FROM snippets WHERE owner_id = ?1")?;
    s.query_map([owner], |r| r.get(0))?.collect()
}
