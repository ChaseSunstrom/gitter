//! Pull request previews: a folder from a CI job's artifacts, served by Gitter at
//! /preview/<token>/. The token is the only key (no cookies reach a preview, so a private
//! repository's preview works for whoever has the link), and the pages are sandboxed: their
//! scripts run in an origin of their own and can never act as the person viewing them.

use std::path::{Component, Path as FsPath, PathBuf};

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Redirect, Response};
use rusqlite::OptionalExtension;

use crate::AppState;
use crate::error::{AppError, R};

/// Where deployment `id`'s preview files are.
pub fn dir(data: &FsPath, id: i64) -> PathBuf {
    data.join("ci/previews").join(id.to_string())
}

/// Delete the files of these deployments' previews.
pub async fn remove(st: &AppState, ids: Vec<i64>) {
    for id in ids {
        let _ = tokio::fs::remove_dir_all(dir(&st.cfg.data_dir, id)).await;
    }
}

/// Publish `folder` of job `job`'s artifacts as deployment `id`'s preview, within the space its
/// repository's owner has left: its new token and how many bytes its files take.
pub async fn publish(st: &AppState, job: i64, id: i64, folder: &str) -> Result<(String, i64), String> {
    // The space left, plus what the environment's live previews take: this one retires them.
    let quota = st.settings.get().max_storage_mb * 1024 * 1024;
    let (used, admin) = st
        .db
        .call(move |c| {
            let (owner, earlier): (i64, i64) = c.query_row(
                "SELECT r.owner_id, (SELECT coalesce(sum(e.preview_bytes), 0) FROM deployments e
                     WHERE e.environment_id = d.environment_id AND e.id < d.id AND e.preview IS NOT NULL)
                 FROM deployments d JOIN repos r ON r.id = d.repo_id WHERE d.id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            let (used, admin) = crate::repo::usage(c, owner)?;
            Ok((used - earlier, admin))
        })
        .await
        .map_err(|e| e.msg)?;
    let room = if quota == 0 || admin { u64::MAX } else { quota.saturating_sub(used.max(0) as u64) };
    let tgz = crate::ci::jobs::artifact_path(&st.cfg.data_dir, job);
    let dest = dir(&st.cfg.data_dir, id);
    let tmp = dest.with_extension("new");
    let f = folder.to_string();
    let (n, bytes) = tokio::task::spawn_blocking(move || -> std::io::Result<(usize, u64)> {
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp)?;
        let n = extract(&tgz, &f, &tmp, room);
        if !matches!(n, Ok((1.., _))) {
            let _ = std::fs::remove_dir_all(&tmp);
            return n;
        }
        let _ = std::fs::remove_dir_all(&dest);
        std::fs::rename(&tmp, &dest)?;
        n
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("The preview couldn't be published: {e}"))?;
    if n == 0 {
        return Err(format!("{folder} had no files to preview"));
    }
    Ok((crate::auth::new_secret("").0, bytes as i64))
}

/// Copy the regular files under `folder/` in the .tar.gz at `tgz` into `dest`, up to the
/// artifacts' own size cap and `room` bytes. How many, and their bytes.
fn extract(tgz: &FsPath, folder: &str, dest: &FsPath, room: u64) -> std::io::Result<(usize, u64)> {
    let mut ar = tar::Archive::new(flate2::read::GzDecoder::new(std::fs::File::open(tgz)?));
    let (mut n, mut total) = (0, 0u64);
    for e in ar.entries()? {
        let mut e = e?;
        if e.header().entry_type() != tar::EntryType::Regular {
            continue;
        }
        let path = e.path()?.into_owned();
        let Ok(rel) = path.strip_prefix(folder) else { continue };
        if rel.as_os_str().is_empty() || !rel.components().all(|c| matches!(c, Component::Normal(_))) {
            continue;
        }
        total += e.size();
        if total > crate::ci::jobs::ARTIFACT_CAP {
            return Err(std::io::Error::other("the folder is over 1 GB"));
        }
        if total > room {
            return Err(std::io::Error::other("this account is out of storage"));
        }
        let out = dest.join(rel);
        if let Some(p) = out.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::io::copy(&mut e, &mut std::fs::File::create(&out)?)?;
        n += 1;
    }
    Ok((n, total))
}

/// `/preview/<token>`: the folder's address ends with a slash, so its relative links work.
pub async fn bare(Path(token): Path<String>) -> Response {
    Redirect::permanent(&format!("/preview/{}/", crate::net::enc(&token))).into_response()
}

pub async fn index(st: State<AppState>, Path(token): Path<String>) -> R<Response> {
    serve(st, Path((token, String::new()))).await
}

/// A file of a preview: the folder's index.html for the folder itself.
pub async fn serve(State(st): State<AppState>, Path((token, rest)): Path<(String, String)>) -> R<Response> {
    if token.len() > 100 || !token.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
        return Err(AppError::not_found());
    }
    let id: i64 = st
        .db
        .call(move |c| {
            Ok(c.query_row("SELECT d.id FROM deployments d JOIN repos r ON r.id = d.repo_id WHERE d.preview = ?1 AND r.deleted_at IS NULL AND r.hidden = 0", [token], |r| r.get(0))
                .optional()?)
        })
        .await?
        .ok_or_else(AppError::not_found)?;
    let mut path = dir(&st.cfg.data_dir, id);
    for part in rest.split('/').filter(|p| !p.is_empty()) {
        if part == "." || part == ".." || part.contains(['\\', '\0']) {
            return Err(AppError::not_found());
        }
        path.push(part);
    }
    let meta = tokio::fs::symlink_metadata(&path).await.map_err(|_| AppError::not_found())?;
    if meta.is_dir() {
        if !rest.is_empty() && !rest.ends_with('/') {
            let last = rest.rsplit('/').next().unwrap_or_default();
            return Ok(Redirect::permanent(&format!("{}/", crate::net::enc(last))).into_response());
        }
        path.push("index.html");
    } else if !meta.is_file() {
        return Err(AppError::not_found());
    }
    let f = tokio::fs::File::open(&path).await.map_err(|_| AppError::not_found())?;
    let meta = f.metadata().await?;
    if !meta.is_file() {
        return Err(AppError::not_found());
    }
    let mime = mime_guess::from_path(&path).first_or_octet_stream();
    let mut resp = Response::new(Body::from_stream(tokio_util::io::ReaderStream::new(f)));
    let h = resp.headers_mut();
    if let Ok(v) = HeaderValue::from_str(mime.essence_str()) {
        h.insert(header::CONTENT_TYPE, v);
    }
    h.insert(header::CONTENT_LENGTH, HeaderValue::from(meta.len()));
    // Its own opaque origin: scripts run, but without Gitter's cookies, storage or API.
    h.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static("sandbox allow-scripts allow-forms allow-popups allow-modals allow-downloads"));
    // The page's own fetches come from that opaque origin.
    h.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    h.insert("x-robots-tag", HeaderValue::from_static("noindex"));
    Ok(resp)
}

/// Hourly: preview folders whose deployment is gone or no longer has a preview, and previews
/// whose folder is gone (so their bytes stop counting).
pub async fn sweep(st: &AppState) {
    let data = st.cfg.data_dir.clone();
    let _ = st
        .db
        .call(move |c| {
            let live: Vec<i64> = c.prepare("SELECT id FROM deployments WHERE preview IS NOT NULL")?.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;
            for id in live.into_iter().filter(|&id| !dir(&data, id).exists()) {
                c.execute("UPDATE deployments SET preview = NULL WHERE id = ?1", [id])?;
            }
            Ok(())
        })
        .await;
    let root = st.cfg.data_dir.join("ci/previews");
    let Ok(mut rd) = tokio::fs::read_dir(&root).await else { return };
    let mut ids = vec![];
    while let Ok(Some(e)) = rd.next_entry().await {
        // Not one being written or settled right now.
        let young = e.metadata().await.ok().and_then(|m| m.modified().ok()).and_then(|t| t.elapsed().ok()).is_none_or(|a| a.as_secs() < 600);
        if young {
            continue;
        }
        match e.file_name().to_str().and_then(|n| n.parse::<i64>().ok()) {
            Some(id) => ids.push(id),
            // A publish that stopped halfway.
            None => {
                let _ = tokio::fs::remove_dir_all(e.path()).await;
            }
        }
    }
    let cutoff = crate::db::now() - crate::ci::jobs::ARTIFACT_DAYS * 86400;
    let gone = st
        .db
        .call(move |c| {
            let mut gone = vec![];
            for id in ids {
                let live: bool = c
                    .query_row("SELECT count(*) FROM deployments WHERE id = ?1 AND preview IS NOT NULL AND updated_at >= ?2", rusqlite::params![id, cutoff], |r| r.get(0))?;
                if !live {
                    c.execute("UPDATE deployments SET preview = NULL WHERE id = ?1", [id])?;
                    gone.push(id);
                }
            }
            Ok(gone)
        })
        .await
        .unwrap_or_default();
    remove(st, gone).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_regular_files_inside_the_folder_come_out() {
        let tmp = tempfile::tempdir().unwrap();
        let tgz = tmp.path().join("a.tar.gz");
        let mut b = tar::Builder::new(flate2::write::GzEncoder::new(std::fs::File::create(&tgz).unwrap(), flate2::Compression::fast()));
        let mut h = tar::Header::new_gnu();
        h.set_size(2);
        h.set_mode(0o644);
        b.append_data(&mut h.clone(), "dist/index.html", &b"hi"[..]).unwrap();
        b.append_data(&mut h.clone(), "dist/css/a.css", &b"h1"[..]).unwrap();
        b.append_data(&mut h.clone(), "distant.txt", &b"no"[..]).unwrap();
        let mut l = tar::Header::new_gnu();
        l.set_entry_type(tar::EntryType::Symlink);
        l.set_size(0);
        b.append_link(&mut l, "dist/link", "/etc/passwd").unwrap();
        b.into_inner().unwrap().finish().unwrap();
        let out = tmp.path().join("out");
        std::fs::create_dir(&out).unwrap();
        assert!(extract(&tgz, "dist", &out, 3).unwrap_err().to_string().contains("out of storage"), "4 bytes don't fit in 3");
        std::fs::remove_dir_all(&out).unwrap();
        std::fs::create_dir(&out).unwrap();
        assert_eq!(extract(&tgz, "dist", &out, 4).unwrap(), (2, 4));
        assert!(out.join("index.html").is_file() && out.join("css/a.css").is_file());
        assert!(std::fs::symlink_metadata(out.join("link")).is_err(), "no symlinks");
        assert!(!tmp.path().join("out/../distant.txt").exists() && std::fs::read_dir(&out).unwrap().count() == 2);
    }
}
