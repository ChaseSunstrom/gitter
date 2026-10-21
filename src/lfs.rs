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

const MEDIA: &str = "application/vnd.git-lfs+json";
const BATCH_CAP: usize = 1024 * 1024;
const MAX_OBJECTS: usize = 1000;
/// An upload that sends nothing for this long is dropped.
const STALL: Duration = Duration::from_secs(60);

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
    let limit = st.settings.get().max_lfs_bytes();
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
            } else if upload && o.size > limit {
                fail(o, 422, format!("Files stored with Git LFS are limited to {} MB here", limit >> 20))
            } else if upload && have {
                json!({ "oid": o.oid, "size": o.size })
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
    Ok(([(header::CONTENT_TYPE, MEDIA)], out.to_string()).into_response())
}

pub async fn download(
    State(st): State<AppState>,
    UrlPath((owner, name, oid)): UrlPath<(String, String, String)>,
    req: Request,
) -> Response {
    reply(download_inner(st, owner, name, oid, req.headers()).await)
}

async fn download_inner(st: AppState, owner: String, name: String, oid: String, headers: &HeaderMap) -> R<Response> {
    let (repo, _) = open(&st, headers, &owner, &name).await?;
    if !valid_oid(&oid) || !held(&st, repo.id, vec![oid.clone()]).await?[0] {
        return Ok(error(StatusCode::NOT_FOUND, "Object does not exist"));
    }
    let (len, body) = object_body(&st.cfg.data_dir, &oid).await?;
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
pub async fn object_body(data: &Path, oid: &str) -> R<(u64, Body)> {
    let f = tokio::fs::File::open(object_path(data, oid)).await?;
    let len = f.metadata().await?.len();
    Ok((len, Body::from_stream(tokio_util::io::ReaderStream::new(f))))
}

pub async fn upload(
    State(st): State<AppState>,
    UrlPath((owner, name, oid)): UrlPath<(String, String, String)>,
    req: Request,
) -> Response {
    reply(upload_inner(st, owner, name, oid, req).await)
}

async fn upload_inner(st: AppState, owner: String, name: String, oid: String, req: Request) -> R<Response> {
    let (repo, anonymous) = open(&st, req.headers(), &owner, &name).await?;
    need_write(&repo, anonymous)?;
    if !valid_oid(&oid) {
        return Ok(error(StatusCode::UNPROCESSABLE_ENTITY, "Invalid object id"));
    }
    let limit = st.settings.get().max_lfs_bytes();
    let size = match receive(&st.cfg.data_dir, &oid, limit, req.into_body()).await {
        Ok(n) => n,
        Err((status, msg)) => return Ok(error(status, &msg)),
    };
    let id = repo.id;
    st.db
        .call(move |c| {
            c.execute(
                "INSERT OR IGNORE INTO lfs_objects(repo_id, oid, size, created_at) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![id, oid, size as i64, crate::db::now()],
            )?;
            Ok(())
        })
        .await?;
    Ok(StatusCode::OK.into_response())
}

/// Stream an upload into a temp file while hashing it; it's moved into place
/// only when it's within `limit` and its sha256 is `oid`. Returns its size.
async fn receive(data: &Path, oid: &str, limit: u64, body: Body) -> Result<u64, (StatusCode, String)> {
    let mut rnd = [0u8; 8];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut rnd);
    let tmp = data.join("lfs/tmp").join(format!("{oid}.{}", hex::encode(rnd)));
    let internal = |e: std::io::Error| {
        let e = AppError::internal(format!("lfs upload {oid}: {e}"));
        (e.status, e.msg)
    };
    let res = async {
        tokio::fs::create_dir_all(data.join("lfs/tmp")).await.map_err(internal)?;
        let mut f = tokio::fs::File::create(&tmp).await.map_err(internal)?;
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
                let msg = format!("Files stored with Git LFS are limited to {} MB here", limit >> 20);
                return Err((StatusCode::PAYLOAD_TOO_LARGE, msg));
            }
            hash.update(&chunk);
            f.write_all(&chunk).await.map_err(internal)?;
        }
        if hex::encode(hash.finalize()) != oid {
            return Err((StatusCode::UNPROCESSABLE_ENTITY, "The content doesn't match its object id (sha256)".into()));
        }
        f.sync_all().await.map_err(internal)?;
        drop(f);
        let dest = object_path(data, oid);
        if let Some(dir) = dest.parent() {
            tokio::fs::create_dir_all(dir).await.map_err(internal)?;
        }
        tokio::fs::rename(&tmp, &dest).await.map_err(internal)?;
        Ok(n)
    }
    .await;
    if res.is_err() {
        let _ = tokio::fs::remove_file(&tmp).await;
    }
    res
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
}
