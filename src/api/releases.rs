//! Releases: a tag (an existing one, or a new annotated tag at a branch or
//! commit) with a title, markdown notes and files to download. Drafts are for
//! writers only. Files live in GITTER_DATA/releases/<release id>/<asset id>,
//! stream in like Git LFS uploads and count toward the owner's storage.

use std::path::PathBuf;

use axum::Json;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::response::Response;
use rusqlite::{OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};

use super::issues::check_text;
use super::{ClientIp, J, ok};
use crate::AppState;
use crate::auth::{Authed, User, Viewer};
use crate::db::now;
use crate::error::{AppError, R};
use crate::repo::Repo;
use crate::security::{Event, record};
use crate::smart_http::Slot;
use crate::validate;

/// The folder holding release `id`'s files.
pub fn dir(st: &AppState, id: i64) -> PathBuf {
    st.cfg.data_dir.join("releases").join(id.to_string())
}

const COLS: &str = "rl.id, rl.tag, rl.title, rl.notes, rl.draft, rl.prerelease, u.username, rl.created_at, rl.published_at, rl.target
    FROM releases rl LEFT JOIN users u ON u.id = rl.author_id";
/// Drafts first, then the most recently published.
const ORDER: &str = "rl.draft DESC, coalesce(rl.published_at, rl.created_at) DESC, rl.id DESC";
/// The latest release of repository ?1: the newest published one that isn't a prerelease.
const LATEST: &str = "rl.repo_id = ?1 AND rl.draft = 0 AND rl.prerelease = 0 ORDER BY rl.published_at DESC, rl.id DESC LIMIT 1";
const ASSET_COLS: &str = "id, name, size, content_type, sha256, downloads, created_at FROM release_assets";
/// Files per release, so a release page stays a sensible size.
const MAX_ASSETS: i64 = 1000;

fn row(r: &rusqlite::Row) -> rusqlite::Result<Value> {
    Ok(json!({
        "id": r.get::<_, i64>(0)?, "tag": r.get::<_, String>(1)?, "title": r.get::<_, String>(2)?, "notes": r.get::<_, String>(3)?,
        "draft": r.get::<_, bool>(4)?, "prerelease": r.get::<_, bool>(5)?, "author": r.get::<_, Option<String>>(6)?,
        "created_at": r.get::<_, i64>(7)?, "published_at": r.get::<_, Option<i64>>(8)?, "target": r.get::<_, String>(9)?,
    }))
}

fn asset_row(r: &rusqlite::Row) -> rusqlite::Result<Value> {
    Ok(json!({
        "id": r.get::<_, i64>(0)?, "name": r.get::<_, String>(1)?, "size": r.get::<_, i64>(2)?, "content_type": r.get::<_, String>(3)?,
        "sha256": r.get::<_, String>(4)?, "downloads": r.get::<_, i64>(5)?, "created_at": r.get::<_, i64>(6)?,
    }))
}

fn with_assets(c: &rusqlite::Connection, mut rel: Value) -> rusqlite::Result<Value> {
    let mut q = c.prepare_cached(&format!("SELECT {ASSET_COLS} WHERE release_id = ?1 ORDER BY name"))?;
    let assets = q.query_map([rel["id"].as_i64()], asset_row)?.collect::<Result<Vec<_>, _>>()?;
    rel["assets"] = json!(assets);
    Ok(rel)
}

/// Each release's tag commit (null when the tag was deleted by a push).
async fn add_shas(st: &AppState, r: &Repo, list: &mut [Value]) -> R<()> {
    let refs = st.git.refs(&r.path).await?;
    for rel in list {
        let sha = refs.iter().find(|x| x.kind == "tag" && rel["tag"].as_str() == Some(x.name.as_str())).map(|x| x.sha.clone());
        rel["sha"] = json!(sha);
    }
    Ok(())
}

/// Release `id` of `r` with its files, if the viewer may see it.
async fn load(st: &AppState, r: &Repo, id: i64) -> R<Value> {
    let (rid, writer) = (r.id, r.can_write);
    let mut rel = st
        .db
        .call(move |c| {
            let rel = c
                .query_row(&format!("SELECT {COLS} WHERE rl.id = ?1 AND rl.repo_id = ?2 AND (?3 OR rl.draft = 0)"), params![id, rid, writer], row)
                .optional()?;
            Ok(rel.map(|x| with_assets(c, x)).transpose()?)
        })
        .await?
        .ok_or_else(AppError::not_found)?;
    add_shas(st, r, std::slice::from_mut(&mut rel)).await?;
    Ok(rel)
}

/// For the repository page: how many releases are published, and the latest.
pub fn summary(c: &rusqlite::Connection, repo_id: i64) -> rusqlite::Result<(i64, Option<Value>)> {
    let count = c.query_row("SELECT count(*) FROM releases WHERE repo_id = ?1 AND draft = 0", [repo_id], |r| r.get(0))?;
    let latest = c
        .query_row(&format!("SELECT rl.id, rl.tag, rl.title, rl.published_at FROM releases rl WHERE {LATEST}"), [repo_id], |r| {
            Ok(json!({ "id": r.get::<_, i64>(0)?, "tag": r.get::<_, String>(1)?, "title": r.get::<_, String>(2)?, "published_at": r.get::<_, Option<i64>>(3)? }))
        })
        .optional()?;
    Ok((count, latest))
}

/// The ids of repository `id`'s releases, or (`owner`) of every repository of
/// account `id`: read before deleting those, then `remove_files`.
pub fn ids_of(c: &rusqlite::Connection, owner: bool, id: i64) -> rusqlite::Result<Vec<i64>> {
    let sql = if owner {
        "SELECT rl.id FROM releases rl JOIN repos r ON r.id = rl.repo_id WHERE r.owner_id = ?1"
    } else {
        "SELECT id FROM releases WHERE repo_id = ?1"
    };
    let mut q = c.prepare(sql)?;
    q.query_map([id], |r| r.get(0))?.collect()
}

/// Remove the files of these deleted releases.
pub async fn remove_files(st: &AppState, ids: Vec<i64>) {
    for id in ids {
        let _ = tokio::fs::remove_dir_all(dir(st, id)).await;
    }
}

/// A unique-constraint failure as a 409 with `msg`.
fn taken(msg: &'static str) -> impl Fn(rusqlite::Error) -> AppError {
    move |e| match AppError::from(e) {
        e if e.status == StatusCode::CONFLICT => AppError::conflict(msg),
        e => e,
    }
}

#[derive(Deserialize)]
pub struct ListQ {
    page: Option<usize>,
}

pub async fn list(State(st): State<AppState>, Viewer(v): Viewer, Path((o, n)): Path<(String, String)>, Query(q): Query<ListQ>) -> J<Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let (rid, writer, page, per) = (r.id, r.can_write, super::page(q.page), 20);
    let (mut items, latest) = st
        .db
        .call(move |c| {
            let sql = format!("SELECT {COLS} WHERE rl.repo_id = ?1 AND (?2 OR rl.draft = 0) ORDER BY {ORDER} LIMIT {} OFFSET {}", per + 1, (page - 1) * per);
            let items = c.prepare(&sql)?.query_map(params![rid, writer], row)?.collect::<Result<Vec<_>, _>>()?;
            let items = items.into_iter().map(|x| with_assets(c, x)).collect::<Result<Vec<_>, _>>()?;
            Ok((items, summary(c, rid)?.1))
        })
        .await?;
    let more = items.len() > per;
    items.truncate(per);
    add_shas(&st, &r, &mut items).await?;
    Ok(Json(json!({ "releases": items, "has_more": more, "page": page, "latest_id": latest.map(|l| l["id"].clone()), "can_write": r.can_write })))
}

pub async fn show(State(st): State<AppState>, Viewer(v): Viewer, Path((o, n, id)): Path<(String, String, i64)>) -> J<Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let mut rel = load(&st, &r, id).await?;
    let rid = r.id;
    let latest = st.db.call(move |c| Ok(summary(c, rid)?.1)).await?;
    rel["latest"] = json!(latest.is_some_and(|l| l["id"] == id));
    rel["can_write"] = json!(r.can_write);
    Ok(Json(rel))
}

pub async fn latest(State(st): State<AppState>, Viewer(v): Viewer, Path((o, n)): Path<(String, String)>) -> J<Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let rid = r.id;
    let latest = st.db.call(move |c| Ok(summary(c, rid)?.1)).await?.ok_or_else(AppError::not_found)?;
    Ok(Json(load(&st, &r, latest["id"].as_i64().unwrap_or(0)).await?))
}

#[derive(Deserialize)]
pub struct CreateReq {
    tag: String,
    /// Where a new tag points: a branch or commit (the default branch if empty).
    #[serde(default)]
    target: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

pub async fn create(State(st): State<AppState>, Authed(u): Authed, Path((o, n)): Path<(String, String)>, Json(req): Json<CreateReq>) -> J<Value> {
    let r = Repo::writable(&st, Some(&u), &o, &n).await?;
    let tag = req.tag.trim().to_string();
    if !validate::ref_name(&tag) {
        return Err(AppError::bad("That isn't a valid tag name"));
    }
    let title = if req.title.trim().is_empty() { tag.clone() } else { req.title.trim().to_string() };
    check_text(&title, &req.notes)?;
    let (rid, t) = (r.id, tag.clone());
    let exists = st
        .db
        .call(move |c| Ok(c.query_row("SELECT 1 FROM releases WHERE repo_id = ?1 AND tag = ?2", params![rid, t], |_| Ok(())).optional()?))
        .await?;
    if exists.is_some() {
        return Err(AppError::conflict("There's already a release for that tag"));
    }
    // A draft's new tag waits until it's published (readers would see it in the tag list); its target must exist now.
    let mut target = String::new();
    if !st.git.refs(&r.path).await?.iter().any(|x| x.kind == "tag" && x.name == tag) {
        let (name, sha) = check_target(&st, &r, &req.target).await?;
        if req.draft {
            target = name;
        } else {
            make_tag(&st, &r, &u, &tag, &sha, &title).await?;
        }
    }
    let (uid, draft, pre, notes) = (u.id, req.draft, req.prerelease, req.notes);
    let id = st
        .db
        .call(move |c| {
            let t = now();
            c.execute(
                "INSERT INTO releases(repo_id, tag, target, title, notes, draft, prerelease, author_id, created_at, published_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                params![rid, tag, target, title, notes, draft, pre, uid, t, (!draft).then_some(t)],
            )
            .map_err(taken("There's already a release for that tag"))?;
            Ok(c.last_insert_rowid())
        })
        .await?;
    Ok(Json(json!({ "id": id })))
}

/// Where a new tag goes: the branch or commit `raw` names (the default branch
/// when empty), checked to exist. Returns the name and its commit.
async fn check_target(st: &AppState, r: &Repo, raw: &str) -> R<(String, String)> {
    let target = if raw.trim().is_empty() { r.default_branch.clone() } else { raw.trim().to_string() };
    let unknown = || AppError::bad("Unknown branch or commit to tag");
    if !validate::rev(&target) {
        return Err(unknown());
    }
    let sha = st.git.resolve(&r.path, &target).await?.ok_or_else(unknown)?;
    Ok((target, sha))
}

/// Point `refs/tags/<tag>` at a new annotated tag of commit `sha`, unless the tag appears meanwhile.
async fn make_tag(st: &AppState, r: &Repo, u: &User, tag: &str, sha: &str, title: &str) -> R<()> {
    let (name, email) = u.signature();
    // mktag checks the object strictly: no angle brackets or line breaks in the name, no NULs.
    let name: String = name.chars().filter(|c| !matches!(c, '<' | '>' | '\n' | '\r' | '\0')).collect();
    let name = if name.trim().is_empty() { u.username.clone() } else { name.trim().to_string() };
    let obj = format!("object {sha}\ntype commit\ntag {tag}\ntagger {name} <{email}> {} +0000\n\n{}\n", now(), title.replace('\0', ""));
    let oid = st.git.ok_env(&r.path, &["mktag"], Some(obj.as_bytes()), &[]).await?;
    st.git
        .update_ref(&r.path, &format!("refs/tags/{tag}"), &oid, None)
        .await
        .map_err(|_| AppError::conflict("That tag was just created by someone else. Try again."))
}

#[derive(Deserialize)]
pub struct UpdateReq {
    title: Option<String>,
    notes: Option<String>,
    draft: Option<bool>,
    prerelease: Option<bool>,
}

pub async fn update(
    State(st): State<AppState>,
    Authed(u): Authed,
    Path((o, n, id)): Path<(String, String, i64)>,
    Json(req): Json<UpdateReq>,
) -> J<Value> {
    let r = Repo::writable(&st, Some(&u), &o, &n).await?;
    let cur = load(&st, &r, id).await?;
    let text = |v: Option<String>, k: &str| v.unwrap_or_else(|| cur[k].as_str().unwrap_or("").to_string());
    let title = text(req.title, "title").trim().to_string();
    let title = if title.is_empty() { text(None, "tag") } else { title };
    let notes = text(req.notes, "notes");
    check_text(&title, &notes)?;
    let draft = req.draft.unwrap_or(cur["draft"] == true);
    let pre = req.prerelease.unwrap_or(cur["prerelease"] == true);
    // Publishing a draft makes its new tag now (unless someone pushed that tag meanwhile).
    if cur["draft"] == true && !draft && cur["sha"].is_null() {
        let (_, sha) = check_target(&st, &r, cur["target"].as_str().unwrap_or("")).await?;
        make_tag(&st, &r, &u, cur["tag"].as_str().unwrap_or(""), &sha, &title).await?;
    }
    st.db
        .call(move |c| {
            // A draft isn't published; publishing it (again) dates it now.
            c.execute(
                "UPDATE releases SET title = ?2, notes = ?3, draft = ?4, prerelease = ?5,
                 published_at = CASE WHEN ?4 THEN NULL ELSE coalesce(published_at, ?6) END,
                 target = CASE WHEN ?4 THEN target ELSE '' END WHERE id = ?1",
                params![id, title, notes, draft, pre, now()],
            )?;
            Ok(())
        })
        .await?;
    ok()
}

/// Delete a release and its files. Its tag stays.
pub async fn delete(State(st): State<AppState>, Authed(u): Authed, ip: ClientIp, Path((o, n, id)): Path<(String, String, i64)>) -> J<Value> {
    let r = Repo::writable(&st, Some(&u), &o, &n).await?;
    let rel = load(&st, &r, id).await?;
    st.db.call(move |c| Ok(c.execute("DELETE FROM releases WHERE id = ?1", [id])?)).await?;
    remove_files(&st, vec![id]).await;
    let detail = format!("{} {}", r.full_name(), rel["tag"].as_str().unwrap_or(""));
    record(&st.db, Event { actor: Some(&u), subject: Some(r.owner_id), action: "release_deleted", detail, ip: &ip.0 }).await;
    ok()
}

/// The name a file is listed and downloaded as: the last part of `raw`, with
/// anything but letters, digits and `.-_+` turned into dots, so it's always
/// safe in a Content-Disposition header.
fn asset_name(raw: &str) -> Option<String> {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or("");
    let name: String = base
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+') { c } else { '.' })
        .take(255)
        .collect();
    (!name.trim_matches('.').is_empty()).then_some(name)
}

/// The uploader's `type/subtype` (parameters dropped), if it's well formed.
fn media_type(h: &HeaderMap) -> String {
    let t = h.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("");
    let t = t.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    let token = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"!#$&^_.+-".contains(&b));
    if t.len() <= 100 && t.split_once('/').is_some_and(|(a, b)| token(a) && token(b)) {
        t
    } else {
        "application/octet-stream".into()
    }
}

fn busy() -> AppError {
    AppError::slow_down("Too many transfers running from your network. Try again when one finishes.", 10)
}

#[derive(Deserialize)]
pub struct AssetQ {
    #[serde(default)]
    name: String,
}

/// Upload a file to a release: the request body is the file, `?name=` its name.
pub async fn upload(
    State(st): State<AppState>,
    Authed(u): Authed,
    ip: ClientIp,
    headers: HeaderMap,
    Path((o, n, id)): Path<(String, String, i64)>,
    Query(q): Query<AssetQ>,
    body: Body,
) -> J<Value> {
    let r = Repo::writable(&st, Some(&u), &o, &n).await?;
    let full = || AppError::bad(format!("A release holds at most {MAX_ASSETS} files"));
    if load(&st, &r, id).await?["assets"].as_array().map_or(0, |a| a.len()) as i64 >= MAX_ASSETS {
        return Err(full());
    }
    let name = asset_name(&q.name).ok_or_else(|| AppError::bad("Give the file a name"))?;
    let ctype = media_type(&headers);
    let _slot = Slot::take(&st, &ip.0).ok_or_else(busy)?;
    let max = st.settings.get().max_lfs_bytes();
    let (limit, too_big) = match crate::repo::storage_left(&st, r.owner_id).await? {
        Some(left) if left < max => (left, format!("This account has only {} MB of storage left", left >> 20)),
        _ => (max, format!("Release files are limited to {} MB here", max >> 20)),
    };
    let declared = headers.get(header::CONTENT_LENGTH).and_then(|v| v.to_str().ok()?.parse::<u64>().ok());
    if declared.is_some_and(|len| len > limit) {
        return Err(AppError::new(StatusCode::PAYLOAD_TOO_LARGE, too_big));
    }
    let (tmp, size, sha) = crate::lfs::spool(&st.cfg.data_dir.join("releases/tmp"), limit, &too_big, body)
        .await
        .map_err(|(s, m)| AppError::new(s, m))?;
    // Uploads running side by side each saw the room left before any finished:
    // count again as this one is recorded, and take it back if it doesn't fit.
    let (owner, quota, folder) = (r.owner_id, st.settings.get().max_storage_mb * 1024 * 1024, dir(&st, id));
    let asset = st
        .db
        .call(move |c| {
            let tx = c.transaction()?;
            // The release may have been deleted while the file streamed in.
            tx.query_row("SELECT 1 FROM releases WHERE id = ?1", [id], |_| Ok(())).optional()?.ok_or_else(AppError::not_found)?;
            if tx.query_row("SELECT count(*) FROM release_assets WHERE release_id = ?1", [id], |r| r.get::<_, i64>(0))? >= MAX_ASSETS {
                return Err(full());
            }
            tx.execute(
                "INSERT INTO release_assets(release_id, name, size, content_type, sha256, created_at) VALUES (?1,?2,?3,?4,?5,?6)",
                params![id, name, size as i64, ctype, sha, now()],
            )
            .map_err(taken("This release already has a file with that name"))?;
            let aid = tx.last_insert_rowid();
            let (used, admin): (i64, bool) = tx.query_row(
                &format!("SELECT {}, is_admin FROM users WHERE id = ?1", crate::repo::STORAGE_USED),
                [owner],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            if quota > 0 && !admin && used.max(0) as u64 > quota {
                return Err(AppError::new(StatusCode::PAYLOAD_TOO_LARGE, "This account is out of storage"));
            }
            let asset = tx.query_row(&format!("SELECT {ASSET_COLS} WHERE id = ?1"), [aid], asset_row)?;
            // In place before the row commits, so every row has its file (backups rely on it).
            std::fs::create_dir_all(&folder)?;
            let dest = folder.join(aid.to_string());
            std::fs::rename(&tmp.0, &dest)?;
            if let Err(e) = tx.commit() {
                let _ = std::fs::remove_file(&dest);
                return Err(e.into());
            }
            Ok(asset)
        })
        .await?;
    Ok(Json(asset))
}

/// Download a release file (counted, unless it's a HEAD request).
pub async fn download(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    ip: ClientIp,
    method: Method,
    Path((o, n, id, aid)): Path<(String, String, i64, i64)>,
) -> R<Response> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let (rid, writer) = (r.id, r.can_write);
    let (name, draft): (String, bool) = st
        .db
        .call(move |c| {
            Ok(c.query_row(
                "SELECT a.name, rl.draft FROM release_assets a JOIN releases rl ON rl.id = a.release_id
                 WHERE a.id = ?1 AND rl.id = ?2 AND rl.repo_id = ?3 AND (?4 OR rl.draft = 0)",
                params![aid, id, rid, writer],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
        })
        .await?
        .ok_or_else(AppError::not_found)?;
    let slot = Slot::take(&st, &ip.0).ok_or_else(busy)?;
    let (len, body) = crate::lfs::file_body(&dir(&st, id).join(aid.to_string()), slot).await.map_err(|_| AppError::not_found())?;
    if method != Method::HEAD {
        st.db.call(move |c| Ok(c.execute("UPDATE release_assets SET downloads = downloads + 1 WHERE id = ?1", [aid])?)).await?;
    }
    let mut resp = super::code::inert(body, "application/octet-stream", Some(len), false, r.private || r.hidden || draft);
    if let Ok(v) = HeaderValue::from_str(&format!("attachment; filename=\"{name}\"")) {
        resp.headers_mut().insert(header::CONTENT_DISPOSITION, v);
    }
    Ok(resp)
}

pub async fn delete_asset(State(st): State<AppState>, Authed(u): Authed, Path((o, n, id, aid)): Path<(String, String, i64, i64)>) -> J<Value> {
    let r = Repo::writable(&st, Some(&u), &o, &n).await?;
    load(&st, &r, id).await?;
    let gone = st.db.call(move |c| Ok(c.execute("DELETE FROM release_assets WHERE id = ?1 AND release_id = ?2", [aid, id])?)).await?;
    if gone == 0 {
        return Err(AppError::not_found());
    }
    let _ = tokio::fs::remove_file(dir(&st, id).join(aid.to_string())).await;
    ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_names_are_safe_file_names() {
        assert_eq!(asset_name("dist/../my tool\".sh").as_deref(), Some("my.tool..sh"));
        assert_eq!(asset_name("C:\\builds\\app-1.0+x86_64.tar.gz").as_deref(), Some("app-1.0+x86_64.tar.gz"));
        assert_eq!(asset_name("a\r\nSet-Cookie: x").as_deref(), Some("a..Set-Cookie..x"));
        for bad in ["", "..", "a/", "../.."] {
            assert_eq!(asset_name(bad), None, "{bad:?}");
        }
        assert_eq!(asset_name(&"x".repeat(300)).unwrap().len(), 255);
        let h = |v: &str| HeaderMap::from_iter([(header::CONTENT_TYPE, HeaderValue::from_str(v).unwrap())]);
        assert_eq!(media_type(&h("Application/Zip; charset=x")), "application/zip");
        assert_eq!(media_type(&h("text/html\"><script>")), "application/octet-stream");
        assert_eq!(media_type(&HeaderMap::new()), "application/octet-stream");
    }
}
