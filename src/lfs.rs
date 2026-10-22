//! Git LFS: the batch API and basic transfers at the URLs git-lfs derives from
//! a remote (`/{owner}/{repo}.git/info/lfs/...`). Objects are stored once by
//! content under GITTER_DATA/lfs; `lfs_objects` records which repositories hold
//! each one, and access follows the repository, exactly as for git over HTTP.

use std::path::{Path, PathBuf};
use std::time::Duration;

use axum::body::Body;
use axum::extract::{Path as UrlPath, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::AppState;
use crate::auth::user_by_basic;
use crate::error::{AppError, R};
use crate::repo::Repo;
use crate::smart_http::Slot;

const MEDIA: &str = "application/vnd.git-lfs+json";
const BATCH_CAP: usize = 1024 * 1024;
const MAX_OBJECTS: usize = 1000;
/// An upload that sends nothing for this long is dropped.
const STALL: Duration = Duration::from_secs(60);
/// After its first minute, an upload averaging less than this (bytes/s) is dropped (slowloris).
const MIN_RATE: u64 = 4096;
/// Batch requests per minute from one address (each names up to 1000 objects).
const BATCHES_PER_MIN: u32 = 120;

/// `GITTER_DATA/lfs/ab/cd/abcd…` (the oid must already be valid).
pub fn object_path(data: &Path, oid: &str) -> PathBuf {
    data.join("lfs").join(&oid[..2]).join(&oid[2..4]).join(oid)
}

pub fn valid_oid(oid: &str) -> bool {
    oid.len() == 64 && oid.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// The (oid, size) a Git LFS pointer file points at.
pub fn parse_pointer(data: &[u8]) -> Option<(String, u64)> {
    if data.len() > 1024 {
        return None;
    }
    let mut lines = std::str::from_utf8(data).ok()?.lines();
    if lines.next()? != "version https://git-lfs.github.com/spec/v1" {
        return None;
    }
    let (mut oid, mut size) = (None, None);
    for l in lines {
        if let Some(o) = l.strip_prefix("oid sha256:") {
            oid = Some(o.to_string());
        } else if let Some(s) = l.strip_prefix("size ") {
            size = s.parse().ok();
        }
    }
    Some((oid.filter(|o| valid_oid(o))?, size?))
}

/// The largest upload `repo` may take now, and the message when one is bigger:
/// the per-file limit, or what the owner has left of their storage quota.
async fn upload_limit(st: &AppState, repo: &Repo) -> R<(u64, String)> {
    let max = st.settings.get().max_lfs_bytes();
    let per_file = (max, format!("Files stored with Git LFS are limited to {} MB here", max >> 20));
    Ok(match crate::repo::storage_left(st, repo.owner_id).await? {
        Some(left) if left < max => (left, format!("This account has only {} MB of storage left", left >> 20)),
        _ => per_file,
    })
}

/// One of the address's LFS transfer slots (apart from clones' and pushes', so
/// git-lfs's 8 parallel transfers don't block a clone from the same network).
pub fn slot(st: &AppState, ip: &str) -> Option<Slot> {
    Slot::take(st, &format!("lfs {}", crate::auth::ip_bucket(ip)))
}

fn take_slot(st: &AppState, req: &Request) -> Result<Slot, Response> {
    slot(st, &crate::api::client_ip(st, req.headers(), req.extensions())).ok_or_else(|| {
        let mut r = error(StatusCode::TOO_MANY_REQUESTS, "Too many transfers running from your network");
        r.headers_mut().insert(header::RETRY_AFTER, HeaderValue::from_static("5"));
        r
    })
}

/// For each oid: does this repository hold it, with its file on disk?
pub async fn held(st: &AppState, repo_id: i64, oids: Vec<String>) -> R<Vec<bool>> {
    let data = st.cfg.data_dir.clone();
    st.db
        .call(move |c| {
            let mut q = c.prepare_cached("SELECT 1 FROM lfs_objects WHERE repo_id = ?1 AND oid = ?2")?;
            let mut out = Vec::with_capacity(oids.len());
            for o in &oids {
                out.push(valid_oid(o) && q.exists(rusqlite::params![repo_id, o])? && object_path(&data, o).is_file());
            }
            Ok(out)
        })
        .await
}

fn error(status: StatusCode, msg: &str) -> Response {
    let mut r = (status, [(header::CONTENT_TYPE, MEDIA)], json!({ "message": msg }).to_string()).into_response();
    if status == StatusCode::UNAUTHORIZED {
        // git-lfs only sends credentials after a challenge.
        r.headers_mut().insert(
            header::WWW_AUTHENTICATE,
            HeaderValue::from_static("Basic realm=\"Gitter\", charset=\"UTF-8\""),
        );
    }
    r
}

fn reply(r: R<Response>) -> Response {
    r.unwrap_or_else(|e| error(e.status, &e.msg))
}

/// The repository behind an LFS URL if the caller may read it, and whether the
/// caller is anonymous. Repositories they can't see are a challenge (anonymous) or a 404.
async fn open(st: &AppState, headers: &HeaderMap, owner: &str, name: &str) -> R<(Repo, bool)> {
    let user = user_by_basic(st, headers).await?;
    let hidden = || {
        if user.is_none() {
            AppError::unauthorized()
        } else {
            AppError::new(StatusCode::NOT_FOUND, "Repository not found")
        }
    };
    match Repo::load(st, user.as_ref(), owner, name).await {
        Ok(r) if r.can_read => Ok((r, user.is_none())),
        Ok(_) => Err(hidden()),
        Err(e) if e.status == StatusCode::NOT_FOUND => Err(hidden()),
        Err(e) => Err(e),
    }
}

fn need_write(repo: &Repo, anonymous: bool) -> R<()> {
    match (repo.can_write, anonymous) {
        (true, _) => Ok(()),
        (false, true) => Err(AppError::unauthorized()),
        (false, false) => Err(AppError::new(
            StatusCode::FORBIDDEN,
            "You don't have push access to this repository",
        )),
    }
}

#[derive(Deserialize)]
struct BatchReq {
    operation: String,
    #[serde(default)]
    objects: Vec<Obj>,
    hash_algo: Option<String>,
}

#[derive(Deserialize)]
struct Obj {
    oid: String,
    size: u64,
}

pub async fn batch(
    State(st): State<AppState>,
    UrlPath((owner, name)): UrlPath<(String, String)>,
    req: Request,
) -> Response {
    reply(batch_inner(st, owner, name, req).await)
}

async fn batch_inner(st: AppState, owner: String, name: String, req: Request) -> R<Response> {
    let headers = req.headers().clone();
    let ip = crate::api::client_ip(&st, &headers, req.extensions());
    if let Err(wait) = st.limiter.check(&format!("lfs:{}", crate::auth::ip_bucket(&ip)), BATCHES_PER_MIN, 60) {
        let mut r = error(StatusCode::TOO_MANY_REQUESTS, "Too many Git LFS requests from your network");
        r.headers_mut().insert(header::RETRY_AFTER, HeaderValue::from(wait.max(1)));
        return Ok(r);
    }
    let (repo, anonymous) = open(&st, &headers, &owner, &name).await?;
    let Ok(body) = axum::body::to_bytes(req.into_body(), BATCH_CAP).await else {
        return Ok(error(StatusCode::PAYLOAD_TOO_LARGE, "The batch request is too large"));
    };
    let invalid = |m: &str| Ok(error(StatusCode::UNPROCESSABLE_ENTITY, m));
    let Ok(b) = serde_json::from_slice::<BatchReq>(&body) else {
        return invalid("That isn't a valid batch request");
    };
    if b.hash_algo.as_deref().is_some_and(|h| h != "sha256") {
        return Ok(error(StatusCode::CONFLICT, "Only sha256 objects are supported"));
    }
    let upload = match b.operation.as_str() {
        "upload" => true,
        "download" => false,
        _ => return invalid("The operation must be upload or download"),
    };
    if upload {
        need_write(&repo, anonymous)?;
    }
    if b.objects.len() > MAX_OBJECTS {
        return invalid("Send at most 1000 objects per batch");
    }
    let (limit, too_big) = if upload { upload_limit(&st, &repo).await? } else { (0, String::new()) };
    let have = held(&st, repo.id, b.objects.iter().map(|o| o.oid.clone()).collect()).await?;
    let base = format!("{}/{}/{}.git/info/lfs/objects", st.cfg.url, repo.owner, repo.name);
    // The transfer requests carry the same credentials as this one.
    let auth = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok());
    let action = |oid: &str| match auth {
        Some(a) => json!({ "href": format!("{base}/{oid}"), "header": { "Authorization": a } }),
        None => json!({ "href": format!("{base}/{oid}") }),
    };
    let fail = |o: &Obj, code: u16, msg: String| json!({ "oid": o.oid, "size": o.size, "error": { "code": code, "message": msg } });
    let objects: Vec<Value> = b
        .objects
        .iter()
        .zip(have)
        .map(|(o, have)| {
            let ok = |kind: &str| {
                let actions = serde_json::Map::from_iter([(kind.to_string(), action(&o.oid))]);
                json!({ "oid": o.oid, "size": o.size, "authenticated": auth.is_some(), "actions": actions })
            };
            if !valid_oid(&o.oid) {
                fail(o, 422, "Invalid object id".into())
            } else if upload && have {
                json!({ "oid": o.oid, "size": o.size })
            } else if upload && o.size > limit {
                fail(o, 422, too_big.clone())
            } else if upload {
                ok("upload")
            } else if have {
                ok("download")
            } else {
                fail(o, 404, "Object does not exist".into())
            }
        })
        .collect();
    let out = json!({ "transfer": "basic", "objects": objects, "hash_algo": "sha256" });
    // The actions carry the caller's credentials: never cache this.
    Ok(([(header::CONTENT_TYPE, MEDIA), (header::CACHE_CONTROL, "no-store")], out.to_string()).into_response())
}

pub async fn download(
    State(st): State<AppState>,
    UrlPath((owner, name, oid)): UrlPath<(String, String, String)>,
    req: Request,
) -> Response {
    let slot = match take_slot(&st, &req) {
        Ok(s) => s,
        Err(r) => return r,
    };
    reply(download_inner(st, owner, name, oid, req.headers(), slot).await)
}

async fn download_inner(st: AppState, owner: String, name: String, oid: String, headers: &HeaderMap, slot: Slot) -> R<Response> {
    let (repo, _) = open(&st, headers, &owner, &name).await?;
    if !valid_oid(&oid) || !held(&st, repo.id, vec![oid.clone()]).await?[0] {
        return Ok(error(StatusCode::NOT_FOUND, "Object does not exist"));
    }
    let (len, body) = object_body(&st.cfg.data_dir, &oid, slot).await?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/octet-stream".to_string()),
            (header::CONTENT_LENGTH, len.to_string()),
        ],
        body,
    )
        .into_response())
}

/// A stored object's length and a body streaming it.
pub async fn object_body(data: &Path, oid: &str, slot: Slot) -> R<(u64, Body)> {
    file_body(&object_path(data, oid), slot).await
}

/// A file's length and a body streaming it. The body holds `slot` until it's
/// sent, and stops if the client reads nothing for a minute.
pub async fn file_body(path: &Path, slot: Slot) -> R<(u64, Body)> {
    let f = tokio::fs::File::open(path).await?;
    let len = f.metadata().await?.len();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<std::io::Result<axum::body::Bytes>>(4);
    tokio::spawn(async move {
        let _slot = slot;
        let mut chunks = tokio_util::io::ReaderStream::new(f);
        while let Some(c) = chunks.next().await {
            if !matches!(tokio::time::timeout(STALL, tx.send(c)).await, Ok(Ok(()))) {
                break;
            }
        }
    });
    Ok((len, Body::from_stream(futures_util::stream::poll_fn(move |cx| rx.poll_recv(cx)))))
}

pub async fn upload(
    State(st): State<AppState>,
    UrlPath((owner, name, oid)): UrlPath<(String, String, String)>,
    req: Request,
) -> Response {
    let _slot = match take_slot(&st, &req) {
        Ok(s) => s,
        Err(r) => return r,
    };
    reply(upload_inner(st, owner, name, oid, req).await)
}

async fn upload_inner(st: AppState, owner: String, name: String, oid: String, req: Request) -> R<Response> {
    let (repo, anonymous) = open(&st, req.headers(), &owner, &name).await?;
    need_write(&repo, anonymous)?;
    if !valid_oid(&oid) {
        return Ok(error(StatusCode::UNPROCESSABLE_ENTITY, "Invalid object id"));
    }
    let (limit, too_big) = upload_limit(&st, &repo).await?;
    let size = match receive(&st.cfg.data_dir, &oid, limit, &too_big, req.into_body()).await {
        Ok(n) => n,
        Err((status, msg)) => return Ok(error(status, &msg)),
    };
    // Uploads running side by side each saw the room left before any finished:
    // count again as this one is recorded, and take it back if it doesn't fit.
    let (id, owner, quota) = (repo.id, repo.owner_id, st.settings.get().max_storage_mb * 1024 * 1024);
    let o = oid.clone();
    let over = st
        .db
        .call(move |c| {
            let tx = c.transaction()?;
            let added = tx.execute(
                "INSERT OR IGNORE INTO lfs_objects(repo_id, oid, size, created_at) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![id, o, size as i64, crate::db::now()],
            )? == 1;
            let (used, admin): (i64, bool) = tx.query_row(
                &format!("SELECT {}, is_admin FROM users WHERE id = ?1", crate::repo::STORAGE_USED),
                [owner],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            let over = added && quota > 0 && !admin && used.max(0) as u64 > quota;
            if over {
                tx.execute("DELETE FROM lfs_objects WHERE repo_id = ?1 AND oid = ?2", rusqlite::params![id, o])?;
            }
            tx.commit()?;
            Ok(over)
        })
        .await?;
    if over {
        collect(&st, vec![oid]).await;
        return Ok(error(StatusCode::PAYLOAD_TOO_LARGE, "This account is out of storage"));
    }
    Ok(StatusCode::OK.into_response())
}

/// A temp file removed when dropped, so a cancelled upload leaves nothing behind.
pub struct Tmp(pub PathBuf);

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Too slow: under MIN_RATE on average over a window longer than STALL.
fn too_slow(bytes: u64, window: Duration) -> bool {
    window > STALL && bytes < window.as_secs() * MIN_RATE
}

fn internal(e: std::io::Error) -> (StatusCode, String) {
    let e = AppError::internal(format!("upload: {e}"));
    (e.status, e.msg)
}

/// Stream an upload into a temp file while hashing it; it's moved into place
/// only when it's within `limit` and its sha256 is `oid`. Returns its size.
async fn receive(data: &Path, oid: &str, limit: u64, too_big: &str, body: Body) -> Result<u64, (StatusCode, String)> {
    let (tmp, n, hash) = spool(&data.join("lfs/tmp"), limit, too_big, body).await?;
    if hash != oid {
        return Err((StatusCode::UNPROCESSABLE_ENTITY, "The content doesn't match its object id (sha256)".into()));
    }
    let dest = object_path(data, oid);
    if let Some(dir) = dest.parent() {
        tokio::fs::create_dir_all(dir).await.map_err(internal)?;
    }
    tokio::fs::rename(&tmp.0, &dest).await.map_err(internal)?;
    Ok(n)
}

/// Stream an upload into a new temp file under `dir`, refusing it past `limit`
/// bytes, when it stalls, or when it's too slow. Returns the file (gone when
/// dropped, unless renamed first), its size and its sha256.
pub async fn spool(dir: &Path, limit: u64, too_big: &str, body: Body) -> Result<(Tmp, u64, String), (StatusCode, String)> {
    let mut rnd = [0u8; 8];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut rnd);
    let tmp = Tmp(dir.join(hex::encode(rnd)));
    // Bytes received since `window` began; checked, then restarted, about every minute.
    let (mut window, mut recent) = (std::time::Instant::now(), 0u64);
    tokio::fs::create_dir_all(dir).await.map_err(internal)?;
    let mut f = tokio::fs::File::create(&tmp.0).await.map_err(internal)?;
    let (mut hash, mut n) = (Sha256::new(), 0u64);
    let mut stream = body.into_data_stream();
    loop {
        let chunk = match tokio::time::timeout(STALL, stream.next()).await {
            Ok(None) => break,
            Ok(Some(Ok(c))) => c,
            Ok(Some(Err(_))) => return Err((StatusCode::BAD_REQUEST, "The upload was interrupted".into())),
            Err(_) => return Err((StatusCode::REQUEST_TIMEOUT, "The upload stalled".into())),
        };
        n += chunk.len() as u64;
        if n > limit {
            return Err((StatusCode::PAYLOAD_TOO_LARGE, too_big.to_string()));
        }
        recent += chunk.len() as u64;
        if window.elapsed() > STALL {
            if too_slow(recent, window.elapsed()) {
                return Err((StatusCode::REQUEST_TIMEOUT, "The upload is too slow".into()));
            }
            (window, recent) = (std::time::Instant::now(), 0);
        }
        hash.update(&chunk);
        f.write_all(&chunk).await.map_err(internal)?;
    }
    f.sync_all().await.map_err(internal)?;
    Ok((tmp, n, hex::encode(hash.finalize())))
}

/// Remove the files of these objects that no repository holds any more.
pub async fn collect(st: &AppState, oids: Vec<String>) {
    let data = st.cfg.data_dir.clone();
    let unused = st
        .db
        .call(move |c| {
            let mut q = c.prepare_cached("SELECT 1 FROM lfs_objects WHERE oid = ?1")?;
            let mut out = Vec::new();
            for o in oids {
                if valid_oid(&o) && !q.exists([&o])? {
                    out.push(o);
                }
            }
            Ok(out)
        })
        .await;
    let Ok(unused) = unused else { return };
    let _ = tokio::task::spawn_blocking(move || {
        for o in unused {
            let _ = std::fs::remove_file(object_path(&data, &o));
        }
    })
    .await;
}

/// The oids held by repository `id`, or (`owner`) by every repository of
/// account `id`: read before deleting those, then `collect` them.
pub fn held_by(c: &rusqlite::Connection, owner: bool, id: i64) -> rusqlite::Result<Vec<String>> {
    let sql = if owner {
        "SELECT DISTINCT l.oid FROM lfs_objects l JOIN repos r ON r.id = l.repo_id WHERE r.owner_id = ?1"
    } else {
        "SELECT oid FROM lfs_objects WHERE repo_id = ?1"
    };
    let mut q = c.prepare(sql)?;
    q.query_map([id], |r| r.get(0))?.collect()
}

/// Hourly, from boot: drop temp files of uploads cut off by a crash, and
/// objects no repository holds any more. Only files over an hour old, so an
/// upload between its rename and its row is never caught.
pub async fn sweep(st: &AppState) {
    let data = st.cfg.data_dir.clone();
    let found = tokio::task::spawn_blocking(move || {
        let old = |p: &Path| {
            p.metadata()
                .and_then(|m| m.modified())
                .is_ok_and(|t| t.elapsed().is_ok_and(|e| e > Duration::from_secs(3600)))
        };
        let files = |p: PathBuf| std::fs::read_dir(p).into_iter().flatten().flatten().map(|e| e.path());
        for t in files(data.join("lfs/tmp")).filter(|p| old(p)) {
            let _ = std::fs::remove_file(t);
        }
        files(data.join("lfs"))
            .filter(|p| p.file_name().is_some_and(|n| n.len() == 2))
            .flat_map(files)
            .flat_map(files)
            .filter(|f| old(f))
            .filter_map(|f| f.file_name()?.to_str().map(String::from))
            .collect::<Vec<_>>()
    })
    .await;
    if let Ok(oids) = found {
        collect(st, oids).await;
    }
}

/// No locking API: git-lfs treats a 404 as "not supported" and skips lock checks.
pub async fn no_locks() -> Response {
    error(StatusCode::NOT_FOUND, "File locking is not supported")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointers() {
        let oid = "4d7a214614ab2935c943f9e0ff69d22eadbb8f32b1258daaa5e2ca24d17e2393";
        let p = format!("version https://git-lfs.github.com/spec/v1\noid sha256:{oid}\nsize 12345\n");
        assert_eq!(parse_pointer(p.as_bytes()), Some((oid.to_string(), 12345)));
        assert_eq!(parse_pointer(p.replace("sha256:4d", "sha256:../").as_bytes()), None);
        assert_eq!(parse_pointer(p.replace("size 12345", "size -1").as_bytes()), None);
        assert_eq!(parse_pointer(b"hello"), None);
        assert!(!valid_oid(&oid.to_uppercase()) && !valid_oid(&oid[1..]));
    }

    #[test]
    fn slow_uploads_and_cancelled_temp_files() {
        assert!(!too_slow(0, Duration::from_secs(59)), "a quiet first minute is fine");
        assert!(too_slow(100, Duration::from_secs(120)));
        assert!(!too_slow(10 << 20, Duration::from_secs(120)));
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t");
        std::fs::write(&p, b"x").unwrap();
        drop(Tmp(p.clone()));
        assert!(!p.exists());
    }
}
