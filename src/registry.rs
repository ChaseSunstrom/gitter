//! A container registry (the OCI distribution API at /v2/, for docker, podman
//! and skopeo) and generic packages, both kept under a repository and following
//! its access: whoever can read it pulls, its writers push. Images are named
//! `host/owner/repo[/name]:tag`. Blobs are stored once by content under
//! GITTER_DATA/registry/sha256/; `registry_blobs` records which repositories
//! hold each one, the way `lfs_objects` does for Git LFS, and a manifest's own
//! bytes are a blob too. Auth is HTTP Basic (a password or an access token) on
//! every request, which every client supports; there's no token service.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use axum::Json;
use axum::body::Body;
use axum::extract::{Path as UrlPath, Query, Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::AppState;
use crate::api::{ClientIp, J, ok};
use crate::auth::{Authed, User, Viewer, ip_bucket, user_by_basic};
use crate::db::now;
use crate::error::{AppError, R};
use crate::repo::Repo;
use crate::smart_http::Slot;

const MAX_MANIFEST: usize = 4 << 20;
/// Image names (`owner/repo/sub`) and tags one repository may hold.
const MAX_IMAGES: i64 = 100;
const MAX_TAGS: i64 = 1000;
/// Registry requests per minute from one address (a push is a few per layer).
const REQUESTS_PER_MIN: u32 = 1200;
/// Uploads one account may have open at once.
const MAX_UPLOADS: i64 = 32;
/// A blob no manifest or package names is dropped after this long (a push
/// uploads its layers before the manifest that names them).
const GRACE: i64 = 24 * 3600;
/// An upload nothing was added to for this long is dropped.
const STALE: Duration = Duration::from_secs(3600);

// ---------- errors in the OCI format ----------

/// An error answered as `{"errors":[{"code","message"}]}`.
pub struct Oci {
    status: StatusCode,
    code: &'static str,
    msg: String,
    retry: Option<i64>,
}

type O<T> = Result<T, Oci>;

fn oci(status: StatusCode, code: &'static str, msg: impl Into<String>) -> Oci {
    Oci { status, code, msg: msg.into(), retry: None }
}

fn challenge() -> Oci {
    oci(StatusCode::UNAUTHORIZED, "UNAUTHORIZED", "Sign in with your username and a password or access token")
}

impl From<AppError> for Oci {
    fn from(e: AppError) -> Oci {
        let code = match e.status.as_u16() {
            401 => "UNAUTHORIZED",
            403 | 413 => "DENIED",
            404 => "NAME_UNKNOWN",
            429 => "TOOMANYREQUESTS",
            _ => "UNKNOWN",
        };
        Oci { status: e.status, code, msg: e.msg, retry: e.retry_after }
    }
}

impl From<std::io::Error> for Oci {
    fn from(e: std::io::Error) -> Oci {
        AppError::internal(format!("registry: {e}")).into()
    }
}

/// A refused upload body (too big, stalled, too slow, cut off).
fn upload_err((status, msg): (StatusCode, String)) -> Oci {
    let code = if status == StatusCode::PAYLOAD_TOO_LARGE { "DENIED" } else { "BLOB_UPLOAD_INVALID" };
    oci(status, code, msg)
}

impl IntoResponse for Oci {
    fn into_response(self) -> Response {
        let body = json!({ "errors": [{ "code": self.code, "message": self.msg, "detail": {} }] });
        let mut r = (self.status, Json(body)).into_response();
        if self.status == StatusCode::UNAUTHORIZED {
            r.headers_mut().insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Basic realm=\"Gitter\", charset=\"UTF-8\""));
        }
        if let Some(s) = self.retry {
            r.headers_mut().insert(header::RETRY_AFTER, HeaderValue::from(s.max(1)));
        }
        r
    }
}

fn reply(r: O<Response>) -> Response {
    let mut r = r.unwrap_or_else(IntoResponse::into_response);
    r.headers_mut().insert("docker-distribution-api-version", HeaderValue::from_static("registry/2.0"));
    r
}

// ---------- names, digests and the blob store ----------

/// `sha256:` and 64 lowercase hex digits (the only algorithm accepted).
pub fn valid_digest(d: &str) -> bool {
    d.strip_prefix("sha256:").is_some_and(crate::lfs::valid_oid)
}

/// `GITTER_DATA/registry/sha256/ab/abcd…` (the digest must already be valid).
pub fn blob_path(data: &Path, digest: &str) -> PathBuf {
    let hex = &digest[7..];
    data.join("registry/sha256").join(&hex[..2]).join(hex)
}

fn tmp_dir(st: &AppState) -> PathBuf {
    st.cfg.data_dir.join("registry/tmp")
}

/// One path component of an image name: lowercase alphanumerics, separated by
/// `.`, `_`, `__` or dashes (the OCI rule).
fn component(s: &str) -> bool {
    let b = s.as_bytes();
    let alnum = |c: u8| c.is_ascii_lowercase() || c.is_ascii_digit();
    !b.is_empty()
        && alnum(b[0])
        && alnum(b[b.len() - 1])
        && s.split(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
            .all(|sep| matches!(sep, "" | "." | "_" | "__") || sep.bytes().all(|c| c == b'-'))
}

fn valid_tag(t: &str) -> bool {
    let b = t.as_bytes();
    !b.is_empty()
        && b.len() <= 128
        && (b[0].is_ascii_alphanumeric() || b[0] == b'_')
        && b.iter().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.' | b'-'))
}

/// An image name split into the repository's owner and name and the image's
/// path under it ('' for the repository itself).
struct Image {
    full: String,
    owner: String,
    repo: String,
    sub: String,
}

fn image(name: &str) -> Option<Image> {
    let segs: Vec<&str> = name.split('/').collect();
    if name.len() > 255 || segs.len() < 2 || !segs.iter().all(|s| component(s)) {
        return None;
    }
    Some(Image { full: name.to_string(), owner: segs[0].into(), repo: segs[1].into(), sub: segs[2..].join("/") })
}

/// Why the registry can't serve a repository: its name (lowercased, as images are
/// named) isn't an OCI path component, or ends in `.git`, which lookups strip.
fn unusable(owner: &str, repo: &str) -> Option<String> {
    let (o, r) = (owner.to_ascii_lowercase(), repo.to_ascii_lowercase());
    if r.ends_with(".git") {
        return Some(format!("The registry doesn't take '{repo}': a name ending in .git is git's form of the repository name"));
    }
    (!component(&o) || !component(&r)).then(|| {
        format!("The registry can't serve '{owner}/{repo}': image names allow only lowercase letters and digits, separated by single '.', '_', '__' or dashes (no '..' and no separator before a '.')")
    })
}

enum Op {
    Blob(String),
    StartUpload,
    Upload(String),
    Manifest(String),
    Tags,
}

/// The image name and the endpoint in a path after `/v2/`.
fn parse(path: &str) -> Option<(String, Op)> {
    let s: Vec<&str> = path.split('/').collect();
    let n = s.len();
    if n < 4 {
        return None;
    }
    let name = |k: usize| s[..n - k].join("/");
    Some(match (s[n - 3], s[n - 2], s[n - 1]) {
        (_, "tags", "list") => (name(2), Op::Tags),
        ("blobs", "uploads", "") => (name(3), Op::StartUpload),
        ("blobs", "uploads", id) => (name(3), Op::Upload(id.into())),
        (_, "blobs", "uploads") => (name(2), Op::StartUpload),
        (_, "blobs", d) => (name(2), Op::Blob(d.into())),
        (_, "manifests", r) => (name(2), Op::Manifest(r.into())),
        _ => return None,
    })
}

/// The repository behind an image if the caller may read it. Credentials that
/// don't work are refused rather than treated as anonymous, so `docker login`
/// fails on a wrong password; repositories the caller can't see are a challenge
/// (anonymous) or unknown.
async fn open(st: &AppState, headers: &HeaderMap, ip: &str, owner: &str, name: &str) -> O<(Repo, Option<User>)> {
    let user = user_by_basic(st, headers, ip).await?;
    if user.is_none() && headers.contains_key(header::AUTHORIZATION) {
        return Err(challenge());
    }
    let hidden = || if user.is_none() { challenge() } else { oci(StatusCode::NOT_FOUND, "NAME_UNKNOWN", "Repository not found") };
    match Repo::find(st, user.as_ref(), owner, name).await {
        Ok(r) if r.can_read => Ok((r, user)),
        Ok(_) => Err(hidden()),
        Err(e) if e.status == StatusCode::NOT_FOUND => Err(hidden()),
        Err(e) => Err(e.into()),
    }
}

/// The pusher's account id.
fn need_write(repo: &Repo, user: &Option<User>) -> O<i64> {
    match user {
        Some(u) if repo.can_write => Ok(u.id),
        Some(_) => Err(oci(StatusCode::FORBIDDEN, "DENIED", "You don't have push access to this repository")),
        None => Err(challenge()),
    }
}

/// One of the address's registry transfer slots (apart from git's and Git LFS's).
fn slot(st: &AppState, ip: &str) -> Option<Slot> {
    Slot::take(st, &format!("registry {}", ip_bucket(ip)))
}

fn busy() -> AppError {
    AppError::slow_down("Too many transfers running from your network", 5)
}

/// The size of blob `digest` if repository `repo_id` holds it, with its file on disk.
async fn held(st: &AppState, repo_id: i64, digest: &str) -> R<Option<u64>> {
    let (d, path) = (digest.to_string(), blob_path(&st.cfg.data_dir, digest));
    st.db
        .call(move |c| {
            let size: Option<i64> = c
                .query_row("SELECT size FROM registry_blobs WHERE repo_id = ?1 AND digest = ?2", params![repo_id, d], |r| r.get(0))
                .optional()?;
            Ok(size.filter(|_| path.is_file()).map(|s| s.max(0) as u64))
        })
        .await
}

/// The largest blob (or chunk of one, `already` bytes in) `repo` may take now,
/// and the message when one is bigger: the per-file limit, or what its owner
/// has left of the storage quota after the uploads in progress.
async fn upload_limit(st: &AppState, repo: &Repo, already: u64) -> R<(u64, String)> {
    let max = st.settings.get().max_lfs_bytes();
    let per_blob = (max.saturating_sub(already), format!("Blobs are limited to {} MB here", max >> 20));
    let Some(left) = crate::repo::storage_left(st, repo.owner_id).await? else { return Ok(per_blob) };
    let owner = repo.owner_id;
    let pending: i64 = st
        .db
        .call(move |c| {
            let (payer, _) = crate::repo::payer(c, owner)?;
            Ok(c.query_row(
                "SELECT COALESCE(sum(u.size), 0) FROM registry_uploads u JOIN repos r ON r.id = u.repo_id
                 WHERE r.owner_id IN (SELECT ?1 UNION ALL SELECT id FROM orgs WHERE created_by = ?1)",
                [payer],
                |r| r.get(0),
            )?)
        })
        .await?;
    let left = left.saturating_sub(pending.max(0) as u64);
    Ok(if left < per_blob.0 { (left, "This account is out of storage. Delete something, or ask the site admin for more space.".into()) } else { per_blob })
}

/// Record that `repo` holds blob `digest` (its file already in place). Counted
/// again as it's recorded, since uploads running side by side each saw the room
/// left before any finished; one that doesn't fit is taken back.
async fn link(st: &AppState, repo: &Repo, digest: &str, size: u64) -> R<()> {
    let (id, owner, d, quota) = (repo.id, repo.owner_id, digest.to_string(), st.settings.get().max_storage_mb * 1024 * 1024);
    let over = st
        .db
        .call(move |c| {
            let tx = c.transaction()?;
            let added = tx.execute(
                "INSERT OR IGNORE INTO registry_blobs(repo_id, digest, size, created_at) VALUES (?1, ?2, ?3, ?4)",
                params![id, d, size as i64, now()],
            )? == 1;
            let (used, admin) = crate::repo::usage(&tx, owner)?;
            let over = added && quota > 0 && !admin && used.max(0) as u64 > quota;
            if over {
                tx.execute("DELETE FROM registry_blobs WHERE repo_id = ?1 AND digest = ?2", params![id, d])?;
            }
            tx.commit()?;
            Ok(over)
        })
        .await?;
    if over {
        collect(st, vec![digest.to_string()]).await;
        return Err(AppError::new(StatusCode::PAYLOAD_TOO_LARGE, "This account is out of storage"));
    }
    Ok(())
}

/// Move a verified file into the store as blob `digest`, held by `repo`.
async fn place(st: &AppState, repo: &Repo, src: &Path, digest: &str, size: u64) -> R<()> {
    let dest = blob_path(&st.cfg.data_dir, digest);
    if let Some(dir) = dest.parent() {
        tokio::fs::create_dir_all(dir).await?;
    }
    // The same digest is the same bytes, so replacing a file another upload put there is harmless.
    tokio::fs::rename(src, &dest).await?;
    link(st, repo, digest, size).await
}

/// Remove the files of these blobs that no repository holds any more.
pub async fn collect(st: &AppState, digests: Vec<String>) {
    let data = st.cfg.data_dir.clone();
    let unused = st
        .db
        .call(move |c| {
            let mut q = c.prepare_cached("SELECT 1 FROM registry_blobs WHERE digest = ?1")?;
            let mut out = Vec::new();
            for d in digests {
                if valid_digest(&d) && !q.exists([&d])? {
                    out.push(d);
                }
            }
            Ok(out)
        })
        .await;
    let Ok(unused) = unused else { return };
    let _ = tokio::task::spawn_blocking(move || {
        for d in unused {
            let _ = std::fs::remove_file(blob_path(&data, &d));
        }
    })
    .await;
}

/// The blobs held by repository `id`, or (`owner`) by every repository of
/// account `id`: read before deleting those, then `collect` them.
pub fn held_by(c: &rusqlite::Connection, owner: bool, id: i64) -> rusqlite::Result<Vec<String>> {
    let sql = if owner {
        "SELECT DISTINCT b.digest FROM registry_blobs b JOIN repos r ON r.id = b.repo_id WHERE r.owner_id = ?1"
    } else {
        "SELECT digest FROM registry_blobs WHERE repo_id = ?1"
    };
    let mut q = c.prepare(sql)?;
    q.query_map([id], |r| r.get(0))?.collect()
}

fn sha256_file(p: &Path) -> std::io::Result<String> {
    let mut h = Sha256::new();
    std::io::copy(&mut std::fs::File::open(p)?, &mut h)?;
    Ok(format!("sha256:{}", hex::encode(h.finalize())))
}

// ---------- the /v2/ API ----------

fn limited(st: &AppState, ip: &str) -> O<()> {
    st.limiter.check(&format!("registry:{}", ip_bucket(ip)), REQUESTS_PER_MIN, 60).map_err(|wait| {
        let mut e = oci(StatusCode::TOO_MANY_REQUESTS, "TOOMANYREQUESTS", "Too many registry requests from your network");
        e.retry = Some(wait);
        e
    })
}

/// `GET /v2/`: clients check it to learn how to authenticate, and `docker login` to check a password.
pub async fn ping(State(st): State<AppState>, req: Request) -> Response {
    let ip = crate::api::client_ip(&st, req.headers(), req.extensions());
    reply(ping_inner(&st, &ip, req.headers().clone()).await)
}

async fn ping_inner(st: &AppState, ip: &str, headers: HeaderMap) -> O<Response> {
    limited(st, ip)?;
    match user_by_basic(st, &headers, ip).await? {
        Some(_) => Ok(Json(json!({})).into_response()),
        None => Err(challenge()),
    }
}

pub async fn v2(State(st): State<AppState>, req: Request) -> Response {
    reply(route(st, req).await)
}

async fn route(st: AppState, req: Request) -> O<Response> {
    let (parts, body) = req.into_parts();
    let (method, headers) = (parts.method, parts.headers);
    let ip = crate::api::client_ip(&st, &headers, &parts.extensions);
    limited(&st, &ip)?;
    let path = parts.uri.path().strip_prefix("/v2/").unwrap_or_default().to_string();
    let q: HashMap<String, String> = Query::try_from_uri(&parts.uri).map(|Query(q)| q).unwrap_or_default();
    let Some((name, op)) = parse(&path) else {
        return Err(oci(StatusCode::NOT_FOUND, "UNSUPPORTED", "There's no such registry endpoint"));
    };
    let Some(img) = image(&name) else {
        let why = match name.split('/').collect::<Vec<_>>()[..] {
            [o, r, ..] => unusable(o, r),
            _ => None,
        };
        return Err(oci(StatusCode::BAD_REQUEST, "NAME_INVALID", why.unwrap_or_else(|| "Image names are owner/repository[/name], in lowercase, each part following the OCI name rules".into())));
    };
    if let Some(why) = unusable(&img.owner, &img.repo) {
        return Err(oci(StatusCode::BAD_REQUEST, "NAME_INVALID", why));
    }
    let (repo, user) = open(&st, &headers, &ip, &img.owner, &img.repo).await?;
    match (method, op) {
        (m @ (Method::GET | Method::HEAD), Op::Blob(d)) => get_blob(&st, &repo, &d, m == Method::HEAD, &ip).await,
        (Method::POST, Op::StartUpload) => start_upload(&st, &repo, &user, &img, &q, body, &ip).await,
        (Method::GET, Op::Upload(id)) => {
            let uid = need_write(&repo, &user)?;
            let size = session(&st, &repo, uid, &id).await?;
            Ok(accepted(&img, &id, size, StatusCode::NO_CONTENT))
        }
        (Method::PATCH, Op::Upload(id)) => {
            let uid = need_write(&repo, &user)?;
            let _slot = slot(&st, &ip).ok_or_else(busy)?;
            let _busy = Busy::take(&id)?;
            let size = append(&st, &repo, uid, &id, &headers, body).await?;
            Ok(accepted(&img, &id, size, StatusCode::ACCEPTED))
        }
        (Method::PUT, Op::Upload(id)) => {
            let uid = need_write(&repo, &user)?;
            let _slot = slot(&st, &ip).ok_or_else(busy)?;
            finish_upload(&st, &repo, uid, &img, &id, q.get("digest").map(String::as_str).unwrap_or(""), &headers, body).await
        }
        (Method::DELETE, Op::Upload(id)) => {
            let uid = need_write(&repo, &user)?;
            session(&st, &repo, uid, &id).await?;
            end_session(&st, &id).await;
            Ok(StatusCode::NO_CONTENT.into_response())
        }
        (m @ (Method::GET | Method::HEAD), Op::Manifest(r)) => get_manifest(&st, &repo, &img, &r, m == Method::HEAD).await,
        (Method::PUT, Op::Manifest(r)) => put_manifest(&st, &repo, &user, &img, &r, &headers, body).await,
        (Method::DELETE, Op::Manifest(r)) => delete_manifest(&st, &repo, &user, &img, &r).await,
        (Method::GET, Op::Tags) => tags(&st, &repo, &img, &q).await,
        _ => Err(oci(StatusCode::METHOD_NOT_ALLOWED, "UNSUPPORTED", "That method isn't supported here")),
    }
}

fn hv(s: &str) -> HeaderValue {
    HeaderValue::from_str(s).unwrap_or_else(|_| HeaderValue::from_static(""))
}

/// The manifest kinds accepted (and so the only Content-Types a manifest is served with).
const MANIFEST_TYPES: &[&str] = &[
    "application/vnd.oci.image.manifest.v1+json",
    "application/vnd.oci.image.index.v1+json",
    "application/vnd.docker.distribution.manifest.v2+json",
    "application/vnd.docker.distribution.manifest.list.v2+json",
];

/// Stored bytes are pushers' content served from this origin: never sniffed,
/// never run as a page.
fn stored(h: &mut HeaderMap) {
    h.insert("x-content-type-options", HeaderValue::from_static("nosniff"));
    h.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static("sandbox; default-src 'none'"));
}

fn bad_digest() -> Oci {
    oci(StatusCode::BAD_REQUEST, "DIGEST_INVALID", "The digest is invalid, or the content doesn't match it (only sha256 is supported)")
}

async fn get_blob(st: &AppState, repo: &Repo, d: &str, head: bool, ip: &str) -> O<Response> {
    if !valid_digest(d) {
        return Err(bad_digest());
    }
    let Some(size) = held(st, repo.id, d).await? else {
        return Err(oci(StatusCode::NOT_FOUND, "BLOB_UNKNOWN", "This repository has no such blob"));
    };
    let (len, body) = if head {
        (size, Body::empty())
    } else {
        let s = slot(st, ip).ok_or_else(busy)?;
        crate::lfs::file_body(&blob_path(&st.cfg.data_dir, d), s).await?
    };
    let mut r = body.into_response();
    let h = r.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/octet-stream"));
    h.insert(header::CONTENT_DISPOSITION, HeaderValue::from_static("attachment"));
    h.insert(header::CONTENT_LENGTH, HeaderValue::from(len));
    h.insert("docker-content-digest", hv(d));
    h.insert(header::ETAG, hv(&format!("\"{d}\"")));
    stored(h);
    Ok(r)
}

/// 201 for a blob now in the repository.
fn created(img: &Image, kind: &str, d: &str) -> Response {
    let mut r = StatusCode::CREATED.into_response();
    r.headers_mut().insert(header::LOCATION, hv(&format!("/v2/{}/{kind}/{d}", img.full)));
    r.headers_mut().insert("docker-content-digest", hv(d));
    r
}

/// An upload in progress: where to send more, and the bytes received so far.
fn accepted(img: &Image, id: &str, size: u64, status: StatusCode) -> Response {
    let mut r = status.into_response();
    let h = r.headers_mut();
    h.insert(header::LOCATION, hv(&format!("/v2/{}/blobs/uploads/{id}", img.full)));
    h.insert(header::RANGE, hv(&format!("0-{}", size.saturating_sub(1))));
    h.insert("docker-upload-uuid", hv(id));
    r
}

fn upload_path(st: &AppState, id: &str) -> PathBuf {
    tmp_dir(st).join(id)
}

/// Uploads a request is adding to right now, so two can't interleave their bytes.
static BUSY: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Default::default);

struct Busy(String);

impl Busy {
    fn take(id: &str) -> O<Busy> {
        let mut b = BUSY.lock().unwrap_or_else(|p| p.into_inner());
        if !b.insert(id.to_string()) {
            return Err(oci(StatusCode::CONFLICT, "BLOB_UPLOAD_INVALID", "Another request is adding to this upload"));
        }
        Ok(Busy(id.to_string()))
    }
}

impl Drop for Busy {
    fn drop(&mut self) {
        BUSY.lock().unwrap_or_else(|p| p.into_inner()).remove(&self.0);
    }
}

/// `POST …/blobs/uploads/`: mount a blob from another repository the caller can
/// read (`?mount=&from=`), take a whole blob at once (`?digest=`), or start an upload.
async fn start_upload(st: &AppState, repo: &Repo, user: &Option<User>, img: &Image, q: &HashMap<String, String>, body: Body, ip: &str) -> O<Response> {
    let uid = need_write(repo, user)?;
    if let (Some(d), Some(from)) = (q.get("mount"), q.get("from"))
        && valid_digest(d)
        && let Some(src) = image(from)
        && unusable(&src.owner, &src.repo).is_none()
        && let Ok(src) = Repo::find(st, user.as_ref(), &src.owner, &src.repo).await
        && src.can_read
        && let Some(size) = held(st, src.id, d).await?
    {
        link(st, repo, d, size).await?;
        return Ok(created(img, "blobs", d));
    }
    if let Some(d) = q.get("digest") {
        if !valid_digest(d) {
            return Err(bad_digest());
        }
        let _slot = slot(st, ip).ok_or_else(busy)?;
        let (limit, too_big) = upload_limit(st, repo, 0).await?;
        let (tmp, n, hash) = crate::lfs::spool(&tmp_dir(st), limit, &too_big, body).await.map_err(upload_err)?;
        if format!("sha256:{hash}") != *d {
            return Err(bad_digest());
        }
        place(st, repo, &tmp.0, d, n).await?;
        return Ok(created(img, "blobs", d));
    }
    let mut rnd = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut rnd);
    let id = hex::encode(rnd);
    tokio::fs::create_dir_all(tmp_dir(st)).await?;
    tokio::fs::File::create(upload_path(st, &id)).await?;
    let (rid, i) = (repo.id, id.clone());
    let opened = st
        .db
        .call(move |c| {
            let open: i64 = c.query_row("SELECT count(*) FROM registry_uploads WHERE user_id = ?1", [uid], |r| r.get(0))?;
            if open >= MAX_UPLOADS {
                return Ok(false);
            }
            c.execute("INSERT INTO registry_uploads(id, repo_id, user_id, created_at) VALUES (?1, ?2, ?3, ?4)", params![i, rid, uid, now()])?;
            Ok(true)
        })
        .await?;
    if !opened {
        let _ = tokio::fs::remove_file(upload_path(st, &id)).await;
        return Err(oci(StatusCode::TOO_MANY_REQUESTS, "TOOMANYREQUESTS", "Too many uploads in progress: finish or cancel some first"));
    }
    Ok(accepted(img, &id, 0, StatusCode::ACCEPTED))
}

/// The bytes received so far by upload `id`, if it's this pusher's, to this repository.
async fn session(st: &AppState, repo: &Repo, uid: i64, id: &str) -> O<u64> {
    let unknown = || oci(StatusCode::NOT_FOUND, "BLOB_UPLOAD_UNKNOWN", "There's no such upload (it may have finished, or been idle too long)");
    if id.len() != 32 || !id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        return Err(unknown());
    }
    let (rid, i) = (repo.id, id.to_string());
    let size: Option<i64> = st
        .db
        .call(move |c| {
            Ok(c.query_row("SELECT size FROM registry_uploads WHERE id = ?1 AND repo_id = ?2 AND user_id = ?3", params![i, rid, uid], |r| r.get(0))
                .optional()?)
        })
        .await?;
    let size = size.ok_or_else(unknown)?;
    // A restart clears unfinished uploads' files.
    if !tokio::fs::try_exists(upload_path(st, id)).await.unwrap_or(false) {
        end_session(st, id).await;
        return Err(unknown());
    }
    Ok(size.max(0) as u64)
}

async fn end_session(st: &AppState, id: &str) {
    let i = id.to_string();
    let _ = st.db.call(move |c| Ok(c.execute("DELETE FROM registry_uploads WHERE id = ?1", [i])?)).await;
    let _ = tokio::fs::remove_file(upload_path(st, id)).await;
}

/// Add a request's body to upload `id`, where its Content-Range (if any) says it
/// goes. Returns the upload's size after it. The caller holds its `Busy`.
async fn append(st: &AppState, repo: &Repo, uid: i64, id: &str, headers: &HeaderMap, body: Body) -> O<u64> {
    let size = session(st, repo, uid, id).await?;
    if let Some(range) = headers.get(header::CONTENT_RANGE) {
        let start = range.to_str().ok().map(|r| r.trim_start_matches("bytes ").split(['-', '/']).next().unwrap_or("").trim().to_string());
        if start.and_then(|s| s.parse::<u64>().ok()) != Some(size) {
            return Err(oci(StatusCode::RANGE_NOT_SATISFIABLE, "BLOB_UPLOAD_INVALID", format!("This upload has {size} bytes: send the next chunk from there")));
        }
    }
    let (limit, too_big) = upload_limit(st, repo, size).await?;
    let mut f = tokio::fs::OpenOptions::new().append(true).open(upload_path(st, id)).await?;
    match crate::lfs::pipe(&mut f, limit, &too_big, body, &mut Sha256::new()).await {
        Ok(n) => {
            let (i, total) = (id.to_string(), (size + n) as i64);
            st.db.call(move |c| Ok(c.execute("UPDATE registry_uploads SET size = ?2 WHERE id = ?1", params![i, total])?)).await?;
            Ok(size + n)
        }
        Err(e) => {
            // Back to what was there, so the pusher can retry the chunk.
            let _ = f.set_len(size).await;
            Err(upload_err(e))
        }
    }
}

/// `PUT` on an upload: its last bytes (if any), then check the whole against
/// `digest` and keep it.
#[allow(clippy::too_many_arguments)]
async fn finish_upload(st: &AppState, repo: &Repo, uid: i64, img: &Image, id: &str, d: &str, headers: &HeaderMap, body: Body) -> O<Response> {
    if !valid_digest(d) {
        return Err(bad_digest());
    }
    // Held from the last bytes through the hash and the move, so nothing is added in between.
    let _busy = Busy::take(id)?;
    let size = append(st, repo, uid, id, headers, body).await?;
    let path = upload_path(st, id);
    let p = path.clone();
    let hash = tokio::task::spawn_blocking(move || sha256_file(&p)).await.map_err(AppError::internal)?;
    let kept = match hash {
        Ok(h) if h == d => place(st, repo, &path, d, size).await.map_err(Oci::from),
        Ok(_) => Err(bad_digest()),
        Err(e) => Err(AppError::internal(format!("hashing upload {id}: {e}")).into()),
    };
    end_session(st, id).await;
    kept.map(|_| created(img, "blobs", d))
}

/// A manifest by tag or digest: (digest, media type).
async fn resolve(st: &AppState, repo: &Repo, img: &Image, reference: &str) -> O<(String, String)> {
    let unknown = || oci(StatusCode::NOT_FOUND, "MANIFEST_UNKNOWN", "This image has no such manifest");
    let by_digest = reference.starts_with("sha256:");
    if by_digest && !valid_digest(reference) {
        return Err(bad_digest());
    }
    if !by_digest && !valid_tag(reference) {
        return Err(unknown());
    }
    let (rid, sub, r) = (repo.id, img.sub.clone(), reference.to_string());
    let sql = if by_digest {
        "SELECT digest, media_type FROM registry_manifests WHERE repo_id = ?1 AND name = ?2 AND digest = ?3"
    } else {
        "SELECT m.digest, m.media_type FROM registry_tags t JOIN registry_manifests m ON m.repo_id = t.repo_id AND m.name = t.name AND m.digest = t.digest
         WHERE t.repo_id = ?1 AND t.name = ?2 AND t.tag = ?3"
    };
    let row = st.db.call(move |c| Ok(c.query_row(sql, params![rid, sub, r], |x| Ok((x.get(0)?, x.get(1)?))).optional()?)).await?;
    row.ok_or_else(unknown)
}

async fn get_manifest(st: &AppState, repo: &Repo, img: &Image, reference: &str, head: bool) -> O<Response> {
    let (d, media) = resolve(st, repo, img, reference).await?;
    let bytes = tokio::fs::read(blob_path(&st.cfg.data_dir, &d))
        .await
        .map_err(|_| oci(StatusCode::NOT_FOUND, "MANIFEST_UNKNOWN", "This manifest's file is missing"))?;
    let len = bytes.len();
    let mut r = if head { Body::empty() } else { Body::from(bytes) }.into_response();
    let h = r.headers_mut();
    // Rows hold only MANIFEST_TYPES; anything else is served as plain bytes.
    let media = if MANIFEST_TYPES.contains(&media.as_str()) { media } else { "application/octet-stream".into() };
    h.insert(header::CONTENT_TYPE, hv(&media));
    h.insert(header::CONTENT_LENGTH, HeaderValue::from(len));
    h.insert("docker-content-digest", hv(&d));
    h.insert(header::ETAG, hv(&format!("\"{d}\"")));
    stored(h);
    Ok(r)
}

/// The digests a manifest names: child manifests (an index), or its config and layers.
fn references(v: &Value) -> Option<(Vec<String>, Vec<String>)> {
    let digests = |items: &[&Value]| -> Option<Vec<String>> {
        let mut out: Vec<String> = Vec::new();
        for i in items {
            let d = i["digest"].as_str().filter(|d| valid_digest(d))?;
            if !out.iter().any(|o| o == d) {
                out.push(d.to_string());
            }
        }
        Some(out)
    };
    if let Some(ms) = v["manifests"].as_array() {
        return Some((digests(&ms.iter().collect::<Vec<_>>())?, vec![]));
    }
    let mut blobs: Vec<&Value> = v["layers"].as_array().map(|l| l.iter().collect()).unwrap_or_default();
    if !v["config"].is_null() {
        blobs.push(&v["config"]);
    }
    if blobs.is_empty() {
        return None;
    }
    Some((vec![], digests(&blobs)?))
}

async fn put_manifest(st: &AppState, repo: &Repo, user: &Option<User>, img: &Image, reference: &str, headers: &HeaderMap, body: Body) -> O<Response> {
    need_write(repo, user)?;
    let invalid = |m: &str| oci(StatusCode::BAD_REQUEST, "MANIFEST_INVALID", m);
    // A client that stalls mid-manifest doesn't hold the request open for good.
    let bytes = tokio::time::timeout(Duration::from_secs(60), axum::body::to_bytes(body, MAX_MANIFEST))
        .await
        .map_err(|_| oci(StatusCode::REQUEST_TIMEOUT, "MANIFEST_INVALID", "The manifest took too long to arrive"))?
        .map_err(|_| oci(StatusCode::PAYLOAD_TOO_LARGE, "MANIFEST_INVALID", "Manifests are limited to 4 MB"))?;
    let v: Value = serde_json::from_slice(&bytes).ok().filter(Value::is_object).ok_or_else(|| invalid("That isn't a valid manifest"))?;
    if v["schemaVersion"].as_i64() != Some(2) {
        return Err(invalid("Only schema version 2 manifests are supported"));
    }
    let declared = headers.get(header::CONTENT_TYPE).and_then(|t| t.to_str().ok()).map(|t| t.split(';').next().unwrap_or("").trim().to_ascii_lowercase());
    let media = declared
        .filter(|t| !t.is_empty())
        .or_else(|| v["mediaType"].as_str().map(String::from))
        .ok_or_else(|| invalid("Say what kind of manifest this is (Content-Type)"))?;
    // Served back with this Content-Type from this origin, so only real manifest kinds.
    if !MANIFEST_TYPES.contains(&media.as_str()) {
        return Err(invalid("Only OCI and Docker image manifests and indexes are supported"));
    }
    let (children, blobs) = references(&v).ok_or_else(|| invalid("The manifest must name its layers, config or manifests by sha256 digest"))?;
    let d = format!("sha256:{}", hex::encode(Sha256::digest(&bytes)));
    let tag = if reference.starts_with("sha256:") {
        if reference != d {
            return Err(bad_digest());
        }
        None
    } else if valid_tag(reference) {
        Some(reference.to_string())
    } else {
        return Err(oci(StatusCode::BAD_REQUEST, "TAG_INVALID", "Tags are up to 128 letters, digits, '_', '.' and '-'"));
    };
    // The manifest's bytes are a blob, in place before the rows that name it:
    // always written afresh (a rename), so a sweep removing an old copy meanwhile
    // can't leave the rows without their file. Storage is checked with the rows.
    let dest = blob_path(&st.cfg.data_dir, &d);
    let mut rnd = [0u8; 8];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut rnd);
    tokio::fs::create_dir_all(tmp_dir(st)).await?;
    let tmp = crate::lfs::Tmp(tmp_dir(st).join(hex::encode(rnd)));
    tokio::fs::write(&tmp.0, &bytes).await?;
    tokio::fs::create_dir_all(dest.parent().unwrap_or(&dest)).await?;
    tokio::fs::rename(&tmp.0, &dest).await?;
    let (rid, owner, sub, dg, len) = (repo.id, repo.owner_id, img.sub.clone(), d.clone(), bytes.len() as i64);
    let quota = st.settings.get().max_storage_mb * 1024 * 1024;
    let res: O<()> = st
        .db
        .call(move |c| {
            let tx = c.transaction()?;
            // Checked in the same transaction as the rows that keep them, so the sweep can't take one meanwhile.
            let mut size = len;
            for b in &blobs {
                let s: Option<i64> = tx.query_row("SELECT size FROM registry_blobs WHERE repo_id = ?1 AND digest = ?2", params![rid, b], |r| r.get(0)).optional()?;
                match s {
                    Some(s) => size += s,
                    None => return Ok(Err(oci(StatusCode::BAD_REQUEST, "MANIFEST_BLOB_UNKNOWN", format!("Push blob {b} before the manifest that names it")))),
                }
            }
            for m in &children {
                let s: Option<i64> = tx
                    .query_row("SELECT size FROM registry_manifests WHERE repo_id = ?1 AND name = ?2 AND digest = ?3", params![rid, sub, m], |r| r.get(0))
                    .optional()?;
                match s {
                    Some(s) => size += s,
                    None => return Ok(Err(oci(StatusCode::BAD_REQUEST, "MANIFEST_UNKNOWN", format!("Push manifest {m} before the index that names it")))),
                }
            }
            let one = |sql: &str, p: &[&dyn rusqlite::ToSql]| tx.query_row(sql, p, |r| r.get::<_, i64>(0));
            if one("SELECT EXISTS (SELECT 1 FROM registry_manifests WHERE repo_id = ?1 AND name = ?2)", &[&rid, &sub])? == 0
                && one("SELECT COUNT(DISTINCT name) FROM registry_manifests WHERE repo_id = ?1", &[&rid])? >= MAX_IMAGES
            {
                return Ok(Err(oci(StatusCode::FORBIDDEN, "DENIED", format!("A repository holds at most {MAX_IMAGES} images: delete one first"))));
            }
            if let Some(t) = &tag
                && one("SELECT EXISTS (SELECT 1 FROM registry_tags WHERE repo_id = ?1 AND name = ?2 AND tag = ?3)", &[&rid, &sub, t])? == 0
                && one("SELECT COUNT(*) FROM registry_tags WHERE repo_id = ?1", &[&rid])? >= MAX_TAGS
            {
                return Ok(Err(oci(StatusCode::FORBIDDEN, "DENIED", format!("A repository holds at most {MAX_TAGS} tags: delete some first"))));
            }
            let added = tx.execute("INSERT OR IGNORE INTO registry_blobs(repo_id, digest, size, created_at) VALUES (?1, ?2, ?3, ?4)", params![rid, dg, len, now()])? == 1;
            tx.execute(
                "INSERT OR IGNORE INTO registry_manifests(repo_id, name, digest, media_type, size, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![rid, sub, dg, media, size, now()],
            )?;
            let mid: i64 = tx.query_row("SELECT id FROM registry_manifests WHERE repo_id = ?1 AND name = ?2 AND digest = ?3", params![rid, sub, dg], |r| r.get(0))?;
            for r in blobs.iter().chain(&children) {
                tx.execute("INSERT OR IGNORE INTO registry_refs(manifest_id, digest) VALUES (?1, ?2)", params![mid, r])?;
            }
            if let Some(t) = &tag {
                tx.execute("INSERT OR REPLACE INTO registry_tags(repo_id, name, tag, digest, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)", params![rid, sub, t, dg, now()])?;
            }
            if added && quota > 0 {
                let (used, admin) = crate::repo::usage(&tx, owner)?;
                if !admin && used.max(0) as u64 > quota {
                    return Ok(Err(oci(StatusCode::PAYLOAD_TOO_LARGE, "DENIED", "This account is out of storage")));
                }
            }
            tx.commit()?;
            Ok(Ok(()))
        })
        .await?;
    if let Err(e) = res {
        collect(st, vec![d]).await;
        return Err(e);
    }
    // A sweep that found the old copy unheld may have removed it just before the rows committed.
    if !tokio::fs::try_exists(&dest).await.unwrap_or(false) {
        tokio::fs::write(&dest, &bytes).await?;
    }
    Ok(created(img, "manifests", &d))
}

/// By digest, the manifest and its tags; by tag, just the tag.
async fn delete_manifest(st: &AppState, repo: &Repo, user: &Option<User>, img: &Image, reference: &str) -> O<Response> {
    need_write(repo, user)?;
    if reference.starts_with("sha256:") && !valid_digest(reference) {
        return Err(bad_digest());
    }
    let (rid, sub, r) = (repo.id, img.sub.clone(), reference.to_string());
    let gone = st
        .db
        .call(move |c| {
            let tx = c.transaction()?;
            let named: bool = tx.query_row(
                "SELECT EXISTS (SELECT 1 FROM registry_refs f JOIN registry_manifests m ON m.id = f.manifest_id WHERE m.repo_id = ?1 AND m.name = ?3 AND f.digest = ?2)",
                params![rid, r, sub],
                |x| x.get(0),
            )?;
            if named {
                return Ok(Err(oci(StatusCode::CONFLICT, "DENIED", "An index names this manifest: delete the index first")));
            }
            let n = if r.starts_with("sha256:") {
                tx.execute("DELETE FROM registry_tags WHERE repo_id = ?1 AND name = ?2 AND digest = ?3", params![rid, sub, r])?;
                tx.execute("DELETE FROM registry_manifests WHERE repo_id = ?1 AND name = ?2 AND digest = ?3", params![rid, sub, r])?
            } else {
                tx.execute("DELETE FROM registry_tags WHERE repo_id = ?1 AND name = ?2 AND tag = ?3", params![rid, sub, r])?
            };
            tx.commit()?;
            Ok(Ok(n > 0))
        })
        .await??;
    if !gone {
        return Err(oci(StatusCode::NOT_FOUND, "MANIFEST_UNKNOWN", "This image has no such manifest"));
    }
    // The blobs no manifest names any more go with the next sweeps.
    Ok(StatusCode::ACCEPTED.into_response())
}

/// `GET …/tags/list[?n=&last=]`, in order, with a Link to the next page.
async fn tags(st: &AppState, repo: &Repo, img: &Image, q: &HashMap<String, String>) -> O<Response> {
    let n = q.get("n").and_then(|n| n.parse::<i64>().ok()).unwrap_or(1000).clamp(1, 1000);
    let last = q.get("last").cloned().unwrap_or_default();
    let (rid, sub) = (repo.id, img.sub.clone());
    let tags: Vec<String> = st
        .db
        .call(move |c| {
            let mut s = c.prepare("SELECT tag FROM registry_tags WHERE repo_id = ?1 AND name = ?2 AND tag > ?3 ORDER BY tag LIMIT ?4")?;
            Ok(s.query_map(params![rid, sub, last, n], |r| r.get(0))?.collect::<Result<_, _>>()?)
        })
        .await?;
    let mut r = Json(json!({ "name": img.full, "tags": tags })).into_response();
    if tags.len() as i64 == n
        && let Some(l) = tags.last()
    {
        r.headers_mut().insert(header::LINK, hv(&format!("</v2/{}/tags/list?n={n}&last={l}>; rel=\"next\"", img.full)));
    }
    Ok(r)
}

// ---------- the Packages tab and generic packages ----------

/// Where images are pulled from: the site's host (and port).
fn registry_host(st: &AppState) -> String {
    let u = &st.cfg.url;
    u.split_once("://").map_or(u.as_str(), |x| x.1).trim_end_matches('/').to_string()
}

/// A repository's images with their tags (newest first) and its generic packages.
pub async fn list(State(st): State<AppState>, Viewer(v): Viewer, UrlPath((o, n)): UrlPath<(String, String)>) -> J<Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let host = registry_host(&st);
    let base = format!("{}/{}", r.owner.to_ascii_lowercase(), r.name.to_ascii_lowercase());
    let rid = r.id;
    let (tags, untagged, files) = st
        .db
        .call(move |c| {
            let mut s = c.prepare(
                "SELECT t.name, t.tag, t.digest, m.size, t.updated_at FROM registry_tags t
                 JOIN registry_manifests m ON m.repo_id = t.repo_id AND m.name = t.name AND m.digest = t.digest
                 WHERE t.repo_id = ?1 ORDER BY t.name, t.updated_at DESC, t.tag LIMIT 2000",
            )?;
            let tags: Vec<(String, Value)> = s
                .query_map([rid], |x| {
                    Ok((x.get(0)?, json!({ "tag": x.get::<_, String>(1)?, "digest": x.get::<_, String>(2)?, "size": x.get::<_, i64>(3)?, "updated_at": x.get::<_, i64>(4)? })))
                })?
                .collect::<Result<_, _>>()?;
            // Manifests no tag or index names (a tag moved on): still pullable by digest,
            // still using storage, so listed to be deleted.
            let mut s = c.prepare(
                "SELECT m.name, m.digest, m.size, m.created_at FROM registry_manifests m WHERE m.repo_id = ?1
                 AND NOT EXISTS (SELECT 1 FROM registry_tags t WHERE t.repo_id = m.repo_id AND t.name = m.name AND t.digest = m.digest)
                 AND NOT EXISTS (SELECT 1 FROM registry_refs f JOIN registry_manifests p ON p.id = f.manifest_id WHERE p.repo_id = m.repo_id AND f.digest = m.digest)
                 ORDER BY m.name, m.created_at DESC LIMIT 2000",
            )?;
            let untagged: Vec<(String, Value)> = s
                .query_map([rid], |x| Ok((x.get(0)?, json!({ "digest": x.get::<_, String>(1)?, "size": x.get::<_, i64>(2)?, "created_at": x.get::<_, i64>(3)? }))))?
                .collect::<Result<_, _>>()?;
            let mut s = c.prepare("SELECT name, version, file, size, digest, created_at FROM packages WHERE repo_id = ?1 ORDER BY name, created_at DESC, version, file LIMIT 2000")?;
            let files: Vec<(String, String, Value)> = s
                .query_map([rid], |x| {
                    let d: String = x.get(4)?;
                    Ok((x.get(0)?, x.get(1)?, json!({ "file": x.get::<_, String>(2)?, "size": x.get::<_, i64>(3)?, "sha256": d.get(7..).unwrap_or(""), "created_at": x.get::<_, i64>(5)? })))
                })?
                .collect::<Result<_, _>>()?;
            Ok((tags, untagged, files))
        })
        .await?;
    let mut images: std::collections::BTreeMap<String, Value> = Default::default();
    for (key, (sub, t)) in tags.into_iter().map(|x| ("tags", x)).chain(untagged.into_iter().map(|x| ("untagged", x))) {
        let name = if sub.is_empty() { base.clone() } else { format!("{base}/{sub}") };
        let pull = match t["tag"].as_str() {
            Some(tag) => format!("{host}/{name}:{tag}"),
            None => format!("{host}/{name}@{}", t["digest"].as_str().unwrap_or("")),
        };
        let img = images.entry(sub.clone()).or_insert_with(|| json!({ "name": name, "sub": sub, "pull": pull, "tags": [], "untagged": [] }));
        if let Some(Value::Array(a)) = img.get_mut(key) {
            a.push(t);
        }
    }
    let unavailable = unusable(&r.owner, &r.name);
    let images: Vec<Value> = if unavailable.is_some() { vec![] } else { images.into_values().collect() };
    let mut packages: Vec<Value> = Vec::new();
    for (name, version, f) in files {
        if packages.last().is_none_or(|p| p["name"] != name.as_str() || p["version"] != version.as_str()) {
            packages.push(json!({ "name": name, "version": version, "files": [] }));
        }
        if let Some(Value::Array(a)) = packages.last_mut().map(|p| &mut p["files"]) {
            a.push(f);
        }
    }
    Ok(Json(json!({ "registry": host, "image": if unavailable.is_some() { String::new() } else { format!("{host}/{base}") }, "registry_unavailable": unavailable, "can_write": r.can_write, "images": images, "packages": packages })))
}

#[derive(serde::Deserialize)]
pub struct ImageRef {
    #[serde(default)]
    image: String,
    reference: String,
}

/// From the Packages tab: delete a tag, or a manifest by digest (its tags too).
pub async fn image_delete(State(st): State<AppState>, Authed(u): Authed, UrlPath((o, n)): UrlPath<(String, String)>, Query(q): Query<ImageRef>) -> J<Value> {
    let r = Repo::writable(&st, Some(&u), &o, &n).await?;
    let img = Image { full: String::new(), owner: r.owner.clone(), repo: r.name.clone(), sub: q.image };
    delete_manifest(&st, &r, &Some(u), &img, &q.reference).await.map_err(|e| AppError::new(e.status, e.msg))?;
    Ok(Json(json!({ "ok": true })))
}

/// A package name, version or file name: letters, digits, `.`, `-`, `_`, `+`.
fn part(s: &str, max: usize) -> bool {
    !s.is_empty()
        && s.len() <= max
        && s.as_bytes()[0].is_ascii_alphanumeric()
        && s.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'-' | b'_' | b'+'))
        && !s.contains("..")
}

type PkgPath = UrlPath<(String, String, String, String, String)>;

fn check_parts(name: &str, version: &str, file: &str) -> R<()> {
    if part(name, 100) && part(version, 128) && part(file, 255) {
        return Ok(());
    }
    Err(AppError::bad("Package names, versions and file names are letters, digits, '.', '-', '_' and '+'"))
}

/// `PUT /api/repos/{owner}/{repo}/packages/generic/{name}/{version}/{file}`:
/// store a file (replacing one of the same name) for that package version.
pub async fn generic_put(State(st): State<AppState>, Authed(u): Authed, ip: ClientIp, headers: HeaderMap, UrlPath((o, n, name, version, file)): PkgPath, body: Body) -> J<Value> {
    check_parts(&name, &version, &file)?;
    let r = Repo::writable(&st, Some(&u), &o, &n).await?;
    let _slot = slot(&st, &ip.0).ok_or_else(busy)?;
    let (limit, too_big) = upload_limit(&st, &r, 0).await?;
    let declared = headers.get(header::CONTENT_LENGTH).and_then(|v| v.to_str().ok()?.parse::<u64>().ok());
    if declared.is_some_and(|len| len > limit) {
        return Err(AppError::new(StatusCode::PAYLOAD_TOO_LARGE, too_big));
    }
    let (tmp, size, hash) = crate::lfs::spool(&tmp_dir(&st), limit, &too_big, body).await.map_err(|(s, m)| AppError::new(s, m))?;
    let d = format!("sha256:{hash}");
    place(&st, &r, &tmp.0, &d, size).await?;
    let (rid, nm, ver, f, dg) = (r.id, name.clone(), version.clone(), file.clone(), d.clone());
    st.db
        .call(move |c| {
            c.execute(
                "INSERT OR REPLACE INTO packages(repo_id, name, version, file, digest, size, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![rid, nm, ver, f, dg, size as i64, now()],
            )?;
            Ok(())
        })
        .await?;
    Ok(Json(json!({ "name": name, "version": version, "file": file, "size": size, "sha256": hash })))
}

pub async fn generic_get(State(st): State<AppState>, Viewer(v): Viewer, ip: ClientIp, UrlPath((o, n, name, version, file)): PkgPath) -> R<Response> {
    check_parts(&name, &version, &file).map_err(|_| AppError::not_found())?;
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let (rid, f) = (r.id, file.clone());
    let d: Option<String> = st
        .db
        .call(move |c| {
            Ok(c.query_row("SELECT digest FROM packages WHERE repo_id = ?1 AND name = ?2 AND version = ?3 AND file = ?4", params![rid, name, version, f], |x| x.get(0))
                .optional()?)
        })
        .await?;
    let d = d.filter(|d| valid_digest(d)).ok_or_else(AppError::not_found)?;
    let s = slot(&st, &ip.0).ok_or_else(busy)?;
    let (len, body) = crate::lfs::file_body(&blob_path(&st.cfg.data_dir, &d), s).await.map_err(|_| AppError::not_found())?;
    let mut resp = body.into_response();
    let h = resp.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/octet-stream"));
    h.insert(header::CONTENT_LENGTH, HeaderValue::from(len));
    // `file` is letters, digits and .-_+ only, so it's safe in the header as is.
    h.insert(header::CONTENT_DISPOSITION, hv(&format!("attachment; filename=\"{file}\"")));
    stored(h);
    Ok(resp)
}

pub async fn generic_delete(State(st): State<AppState>, Authed(u): Authed, UrlPath((o, n, name, version, file)): PkgPath) -> J<Value> {
    check_parts(&name, &version, &file).map_err(|_| AppError::not_found())?;
    let r = Repo::writable(&st, Some(&u), &o, &n).await?;
    let rid = r.id;
    let n = st
        .db
        .call(move |c| Ok(c.execute("DELETE FROM packages WHERE repo_id = ?1 AND name = ?2 AND version = ?3 AND file = ?4", params![rid, name, version, file])?))
        .await?;
    if n == 0 {
        return Err(AppError::not_found());
    }
    ok()
}

// ---------- upkeep ----------

/// Hourly, from boot: end uploads idle for an hour, drop blobs no manifest or
/// package has named for a day, and remove files nothing holds: temp files and
/// blobs over an hour old (so one between its rename and its row is never caught).
pub async fn sweep(st: &AppState) {
    let (tmp, t, stale) = (tmp_dir(st), now(), STALE.as_secs() as i64);
    let old = |p: &Path| p.metadata().and_then(|m| m.modified()).map_or(true, |m| m.elapsed().is_ok_and(|e| e > STALE));
    let unheld = "b.created_at < ?1
        AND NOT EXISTS (SELECT 1 FROM registry_manifests m WHERE m.repo_id = b.repo_id AND m.digest = b.digest)
        AND NOT EXISTS (SELECT 1 FROM registry_refs f JOIN registry_manifests m ON m.id = f.manifest_id WHERE m.repo_id = b.repo_id AND f.digest = b.digest)
        AND NOT EXISTS (SELECT 1 FROM packages p WHERE p.repo_id = b.repo_id AND p.digest = b.digest)";
    let dropped = st
        .db
        .call(move |c| {
            let ids: Vec<String> = {
                let mut q = c.prepare("SELECT id FROM registry_uploads WHERE created_at < ?1")?;
                q.query_map([t - stale], |r| r.get(0))?.collect::<Result<_, _>>()?
            };
            for id in ids.iter().filter(|id| old(&tmp.join(id))) {
                c.execute("DELETE FROM registry_uploads WHERE id = ?1", [id])?;
                let _ = std::fs::remove_file(tmp.join(id));
            }
            // Spooled bodies and manifests cut off mid-request, which no upload owns.
            let mut q = c.prepare("SELECT 1 FROM registry_uploads WHERE id = ?1")?;
            for e in std::fs::read_dir(&tmp).into_iter().flatten().flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if old(&e.path()) && !q.exists([&name])? {
                    let _ = std::fs::remove_file(e.path());
                }
            }
            let dropped: Vec<String> = {
                let mut q = c.prepare(&format!("SELECT DISTINCT b.digest FROM registry_blobs b WHERE {unheld}"))?;
                q.query_map([t - GRACE], |r| r.get(0))?.collect::<Result<_, _>>()?
            };
            c.execute(&format!("DELETE FROM registry_blobs AS b WHERE {unheld}"), [t - GRACE])?;
            Ok(dropped)
        })
        .await;
    match dropped {
        Ok(d) => collect(st, d).await,
        Err(e) => eprintln!("registry sweep: {}", e.msg),
    }
    let data = st.cfg.data_dir.clone();
    let found = tokio::task::spawn_blocking(move || {
        let files = |p: PathBuf| std::fs::read_dir(p).into_iter().flatten().flatten().map(|e| e.path());
        files(data.join("registry/sha256"))
            .flat_map(files)
            .filter(|f| old(f))
            .filter_map(|f| f.file_name()?.to_str().map(|h| format!("sha256:{h}")))
            .collect::<Vec<_>>()
    })
    .await;
    if let Ok(digests) = found {
        collect(st, digests).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_digests_and_paths() {
        for ok in ["alice", "a.b", "a_b", "a__b", "a---b", "x1"] {
            assert!(component(ok), "{ok}");
        }
        for bad in ["", "Alice", "a..b", "a___b", "-a", "a-", "a/b", "a b", ".."] {
            assert!(!component(bad), "{bad}");
        }
        assert!(image("alice/app").is_some_and(|i| i.sub.is_empty()));
        assert!(image("alice/app/web/x").is_some_and(|i| i.sub == "web/x"));
        assert!(image("alice").is_none() && image("alice//x").is_none());
        let p = |s: &str| parse(s).map(|(n, op)| (n, std::mem::discriminant(&op)));
        let d = std::mem::discriminant;
        assert_eq!(p("a/b/blobs/uploads/"), Some(("a/b".into(), d(&Op::StartUpload))));
        assert_eq!(p("a/b/blobs/uploads"), Some(("a/b".into(), d(&Op::StartUpload))));
        assert_eq!(p("a/b/c/blobs/uploads/123"), Some(("a/b/c".into(), d(&Op::Upload(String::new())))));
        assert_eq!(p("a/b/blobs/sha256:x"), Some(("a/b".into(), d(&Op::Blob(String::new())))));
        assert_eq!(p("a/b/manifests/v1"), Some(("a/b".into(), d(&Op::Manifest(String::new())))));
        assert_eq!(p("a/b/tags/list"), Some(("a/b".into(), d(&Op::Tags))));
        assert!(p("a/manifests/v1").is_none() && p("a/b/c/d").is_none());
        assert!(valid_tag("v1.0-rc_1") && !valid_tag(".x") && !valid_tag("a/b") && !valid_tag(&"x".repeat(129)));
        assert!(!valid_digest("sha256:../../etc") && !valid_digest("sha512:00"));
        let dg = format!("sha256:{}", "ab".repeat(32));
        assert!(valid_digest(&dg));
        assert_eq!(blob_path(Path::new("/d"), &dg), Path::new("/d/registry/sha256/ab").join("ab".repeat(32)));
        assert!(part("1.0.0+build", 128) && !part("..", 10) && !part("a/b", 10) && !part(".x", 10));
    }

    #[test]
    fn manifest_references() {
        let d = |c: &str| format!("sha256:{}", c.repeat(64));
        let image = json!({ "schemaVersion": 2, "config": { "digest": d("a") }, "layers": [{ "digest": d("b") }, { "digest": d("b") }] });
        assert_eq!(references(&image), Some((vec![], vec![d("b"), d("a")])));
        let index = json!({ "schemaVersion": 2, "manifests": [{ "digest": d("c") }] });
        assert_eq!(references(&index), Some((vec![d("c")], vec![])));
        assert_eq!(references(&json!({ "schemaVersion": 2 })), None);
        assert_eq!(references(&json!({ "layers": [{ "digest": "sha256:../x" }] })), None);
    }
}
