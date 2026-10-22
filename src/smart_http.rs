//! Git smart HTTP (clone, fetch, push) by running `git http-backend` as CGI.

use std::collections::HashMap;
use std::process::Stdio;
use std::time::Duration;

use axum::body::Body;
use axum::extract::{Path, Request, State};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use futures_util::StreamExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::AppState;
use crate::auth::{User, user_by_basic};
use crate::error::AppError;
use crate::repo::{Repo, record_event, touch};

const FETCH_BODY_CAP: u64 = 64 * 1024 * 1024;
const SEND_TIMEOUT: Duration = Duration::from_secs(60);
/// A client that sends nothing for this long while uploading is dropped (slowloris).
const RECV_IDLE: Duration = Duration::from_secs(60);
/// Clones/fetches/pushes running at once from one address, and started per minute.
const TRANSFERS_PER_IP: u32 = 8;
const TRANSFER_STARTS_PER_MIN: u32 = 120;

/// One running transfer for an address; frees its slot when dropped.
pub struct Slot(AppState, String);

impl Slot {
    pub fn take(st: &AppState, ip: &str) -> Option<Slot> {
        let ip = &crate::auth::ip_bucket(ip);
        let mut m = st.transfers.lock().unwrap_or_else(|p| p.into_inner());
        let n = m.entry(ip.to_string()).or_insert(0);
        if *n >= TRANSFERS_PER_IP {
            return None;
        }
        *n += 1;
        Some(Slot(st.clone(), ip.to_string()))
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        let mut m = self.0.transfers.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(n) = m.get_mut(&self.1) {
            *n = n.saturating_sub(1);
            if *n == 0 {
                m.remove(&self.1);
            }
        }
    }
}

fn too_many(msg: &str, wait: i64) -> Response {
    let mut r = plain(StatusCode::TOO_MANY_REQUESTS, msg);
    if let Ok(v) = HeaderValue::from_str(&wait.max(1).to_string()) {
        r.headers_mut().insert(header::RETRY_AFTER, v);
    }
    r
}

fn plain(status: StatusCode, msg: &str) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        format!("{msg}\n"),
    )
        .into_response()
}

fn challenge() -> Response {
    let mut r = plain(StatusCode::UNAUTHORIZED, "Authentication required");
    r.headers_mut().insert(
        header::WWW_AUTHENTICATE,
        HeaderValue::from_static("Basic realm=\"Gitter\", charset=\"UTF-8\""),
    );
    r
}

pub async fn info_refs(
    State(st): State<AppState>,
    Path((owner, repo)): Path<(String, String)>,
    req: Request,
) -> Response {
    let service = req
        .uri()
        .query()
        .and_then(|q| q.split('&').find_map(|kv| kv.strip_prefix("service=")))
        .unwrap_or("")
        .to_string();
    if service != "git-upload-pack" && service != "git-receive-pack" {
        return plain(
            StatusCode::FORBIDDEN,
            "Dumb HTTP is not supported; use a recent git client",
        );
    }
    serve(
        st,
        owner,
        repo,
        "info/refs",
        service == "git-receive-pack",
        req,
    )
    .await
}

pub async fn upload_pack(
    State(st): State<AppState>,
    Path((owner, repo)): Path<(String, String)>,
    req: Request,
) -> Response {
    serve(st, owner, repo, "git-upload-pack", false, req).await
}

pub async fn receive_pack(
    State(st): State<AppState>,
    Path((owner, repo)): Path<(String, String)>,
    req: Request,
) -> Response {
    serve(st, owner, repo, "git-receive-pack", true, req).await
}

async fn serve(
    st: AppState,
    owner: String,
    name: String,
    service_path: &str,
    push: bool,
    req: Request,
) -> Response {
    match serve_inner(st, owner, name, service_path, push, req).await {
        Ok(r) => r,
        Err(e) => plain(e.status, &e.msg),
    }
}

async fn serve_inner(
    st: AppState,
    owner: String,
    name: String,
    service_path: &str,
    push: bool,
    req: Request,
) -> Result<Response, AppError> {
    let headers = req.headers().clone();
    let ip = crate::api::client_ip(&st, &headers, req.extensions());
    if let Err(wait) = st.limiter.check(&format!("git:{}", crate::auth::ip_bucket(&ip)), TRANSFER_STARTS_PER_MIN, 60) {
        return Ok(too_many("Too many git requests from your network. Try again shortly.", wait));
    }
    let Some(slot) = Slot::take(&st, &ip) else {
        return Ok(too_many("Too many git transfers running from your network. Try again when one finishes.", 10));
    };
    let user = user_by_basic(&st, &headers, &ip).await?;
    // `find`: a renamed or transferred repository is served at its old URL too.
    let repo = match Repo::find(&st, user.as_ref(), &owner, &name).await {
        Ok(r) => r,
        Err(e) if e.status == StatusCode::NOT_FOUND => {
            return Ok(if user.is_none() {
                challenge()
            } else {
                plain(StatusCode::NOT_FOUND, "Repository not found")
            });
        }
        Err(e) => return Err(e),
    };
    if !repo.can_read {
        return Ok(if user.is_none() {
            challenge()
        } else {
            plain(StatusCode::NOT_FOUND, "Repository not found")
        });
    }
    if push && !repo.can_write {
        return Ok(if user.is_none() {
            challenge()
        } else {
            plain(
                StatusCode::FORBIDDEN,
                "You don't have push access to this repository",
            )
        });
    }

    let is_post = req.method() == Method::POST;
    // A clone or fetch starts with this advertisement; CI runners fetching a job don't count.
    if service_path == "info/refs"
        && !push
        && !crate::auth::basic_secret(&headers).is_some_and(|s| s.starts_with(crate::ci::queue::JOB_TOKEN_PREFIX))
    {
        st.traffic.record(repo.id, "clone", &ip, None, None);
    }
    // A push is a change like any other for the per-account rate limit.
    let rate = st.settings.get().write_rate_per_min;
    if push
        && is_post
        && rate > 0
        && let Some(u) = user.as_ref().filter(|u| !u.is_admin)
        && let Err(wait) = st.limiter.check(&format!("write:{}", u.id), rate, 60)
    {
        return Ok(too_many("You're pushing very often. Wait a moment and try again.", wait));
    }
    let before = if push && is_post {
        Some(ref_map(&st, &repo).await?)
    } else {
        None
    };

    let permit = st.git.permit_stream().await?;
    let mut max_push = st.settings.get().max_push_bytes();
    if push {
        // git itself refuses a pack bigger than what the owner has left, so nothing is half-written.
        if let Some(left) = crate::repo::storage_left(&st, repo.owner_id).await? {
            if left == 0 {
                return Ok(plain(
                    StatusCode::FORBIDDEN,
                    "This account is out of storage. Delete something, or ask the site admin for more space.",
                ));
            }
            max_push = max_push.min(left);
        }
    }
    // Push protection: a pre-receive hook scans what the push adds, before any ref moves.
    let guard = match (&user, push && is_post) {
        (Some(u), true) => Some(crate::scan::PushGuard::start(&st, &repo, u).await?),
        _ => None,
    };
    let mut cmd = st.git.command(None);
    // The current push limit overrides whatever was stored when the repo was created.
    cmd.arg("-c").arg(format!("receive.maxInputSize={max_push}"));
    if let Some(g) = &guard {
        g.apply(&mut cmd);
    }
    if crate::auth::basic_secret(&headers).is_some_and(|s| s.starts_with(crate::ci::queue::JOB_TOKEN_PREFIX)) {
        // A CI runner fetches the one commit it builds (only commits a ref can reach).
        cmd.arg("-c").arg("uploadpack.allowReachableSHA1InWant=true");
    }
    cmd.arg("http-backend")
        .env("GIT_PROJECT_ROOT", &st.git.root)
        .env("GIT_HTTP_EXPORT_ALL", "1")
        .env("PATH_INFO", format!("/{}.git/{}", repo.id, service_path))
        .env("REQUEST_METHOD", req.method().as_str())
        .env("QUERY_STRING", req.uri().query().unwrap_or(""))
        .env("REMOTE_ADDR", "127.0.0.1")
        .stdin(Stdio::piped());
    if let Some(u) = &user {
        cmd.env("REMOTE_USER", &u.username);
    }
    for (h, var) in [
        (header::CONTENT_TYPE.as_str(), "CONTENT_TYPE"),
        ("content-encoding", "HTTP_CONTENT_ENCODING"),
        ("git-protocol", "HTTP_GIT_PROTOCOL"),
        (header::CONTENT_LENGTH.as_str(), "CONTENT_LENGTH"),
    ] {
        if let Some(v) = headers.get(h).and_then(|v| v.to_str().ok()) {
            cmd.env(var, v);
        }
    }
    let mut child = cmd.spawn().map_err(AppError::internal)?;

    // Feed the request body to git, enforcing a size cap.
    let mut stdin = child.stdin.take().expect("piped stdin");
    let cap = if push {
        max_push + 1024 * 1024
    } else {
        FETCH_BODY_CAP
    };
    let mut body_stream = req.into_body().into_data_stream();
    let feeder = tokio::spawn(async move {
        let mut total: u64 = 0;
        loop {
            let Ok(next) = tokio::time::timeout(RECV_IDLE, body_stream.next()).await else {
                return false;
            };
            let Some(chunk) = next else { break };
            let Ok(chunk) = chunk else { return false };
            total += chunk.len() as u64;
            if total > cap || stdin.write_all(&chunk).await.is_err() {
                return false;
            }
        }
        let _ = stdin.shutdown().await;
        true
    });

    // Parse the CGI header block, then stream the rest back.
    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut buf = Vec::with_capacity(8192);
    let mut chunk = vec![0u8; 16 * 1024];
    let header_end = loop {
        if let Some(i) = find_header_end(&buf) {
            break i;
        }
        let n = tokio::time::timeout(Duration::from_secs(300), stdout.read(&mut chunk))
            .await
            .map_err(|_| AppError::new(StatusCode::GATEWAY_TIMEOUT, "git took too long"))?
            .map_err(AppError::internal)?;
        if n == 0 {
            if !feeder.await.unwrap_or(false) {
                return Ok(plain(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "Push is larger than this server allows",
                ));
            }
            return Err(AppError::internal("http-backend produced no headers"));
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.len() > 64 * 1024 {
            return Err(AppError::internal("http-backend header block too large"));
        }
    };
    let (status, out_headers) = parse_cgi_headers(&buf[..header_end.0]);
    let rest = buf[header_end.1..].to_vec();

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Vec<u8>, std::io::Error>>(16);
    let st2 = st.clone();
    tokio::spawn(async move {
        let _permit = permit;
        let _slot = slot;
        // A client that stops reading for a minute (or disconnects) aborts the transfer.
        let send = |data: Result<Vec<u8>, std::io::Error>| {
            let tx = tx.clone();
            async move {
                matches!(
                    tokio::time::timeout(SEND_TIMEOUT, tx.send(data)).await,
                    Ok(Ok(()))
                )
            }
        };
        let mut aborted = !rest.is_empty() && !send(Ok(rest)).await;
        let mut chunk = vec![0u8; 64 * 1024];
        while !aborted {
            match tokio::time::timeout(Duration::from_secs(3600), stdout.read(&mut chunk)).await {
                Ok(Ok(0)) => break,
                Ok(Ok(n)) => aborted = !send(Ok(chunk[..n].to_vec())).await,
                Ok(Err(e)) => {
                    send(Err(e)).await;
                    break;
                }
                Err(_) => aborted = true,
            }
        }
        drop(stdout);
        if aborted {
            let _ = child.start_kill();
            feeder.abort();
        }
        let _ = feeder.await;
        let _ = tokio::time::timeout(Duration::from_secs(30), child.wait()).await;
        // What push protection refused is on record before the client hears the push is over.
        if let Some(g) = guard {
            g.finish(&st2).await;
        }
        drop(tx);
        if let (Some(before), Some(user)) = (before, user)
            && let Err(e) = after_push(&st2, &repo, &user, before).await
        {
            eprintln!("post-push for {}: {}", repo.full_name(), e.msg);
        }
    });

    let stream = tokio_stream_from(rx);
    let mut resp = Response::new(Body::from_stream(stream));
    *resp.status_mut() = status;
    for (k, v) in out_headers {
        if let (Ok(k), Ok(v)) = (header::HeaderName::try_from(k), HeaderValue::try_from(v)) {
            resp.headers_mut().insert(k, v);
        }
    }
    Ok(resp)
}

fn tokio_stream_from(
    mut rx: tokio::sync::mpsc::Receiver<Result<Vec<u8>, std::io::Error>>,
) -> impl futures_util::Stream<Item = Result<Vec<u8>, std::io::Error>> {
    futures_util::stream::poll_fn(move |cx| rx.poll_recv(cx))
}

/// Returns (end of headers, start of body).
fn find_header_end(b: &[u8]) -> Option<(usize, usize)> {
    if let Some(i) = b.windows(4).position(|w| w == b"\r\n\r\n") {
        return Some((i, i + 4));
    }
    b.windows(2).position(|w| w == b"\n\n").map(|i| (i, i + 2))
}

fn parse_cgi_headers(b: &[u8]) -> (StatusCode, Vec<(String, String)>) {
    let mut status = StatusCode::OK;
    let mut out = Vec::new();
    for line in String::from_utf8_lossy(b).lines() {
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let (k, v) = (k.trim(), v.trim());
        if k.eq_ignore_ascii_case("Status") {
            if let Some(code) = v
                .split_whitespace()
                .next()
                .and_then(|c| c.parse::<u16>().ok())
            {
                status = StatusCode::from_u16(code).unwrap_or(StatusCode::OK);
            }
        } else {
            out.push((k.to_string(), v.to_string()));
        }
    }
    (status, out)
}

async fn ref_map(st: &AppState, repo: &Repo) -> Result<HashMap<String, String>, AppError> {
    let out = st
        .git
        .ok(
            &repo.path,
            &["for-each-ref", "--format=%(refname) %(objectname)"],
        )
        .await?;
    Ok(String::from_utf8_lossy(&out)
        .lines()
        .filter_map(|l| {
            l.split_once(' ')
                .map(|(a, b)| (a.to_string(), b.to_string()))
        })
        .collect())
}

/// What changed in one push.
#[derive(Debug, Clone)]
pub struct RefUpdate {
    pub refname: String,
    pub old: Option<String>,
    pub new: Option<String>,
}

async fn after_push(
    st: &AppState,
    repo: &Repo,
    user: &User,
    before: HashMap<String, String>,
) -> Result<(), AppError> {
    let after = ref_map(st, repo).await?;
    let mut updates = Vec::new();
    for (r, new) in &after {
        if before.get(r) != Some(new) {
            updates.push(RefUpdate {
                refname: r.clone(),
                old: before.get(r).cloned(),
                new: Some(new.clone()),
            });
        }
    }
    for (r, old) in &before {
        if !after.contains_key(r) {
            updates.push(RefUpdate {
                refname: r.clone(),
                old: Some(old.clone()),
                new: None,
            });
        }
    }
    if updates.is_empty() {
        return Ok(());
    }
    fix_head(st, repo, &after).await?;
    touch(&st.db, repo.id, true).await?;
    for u in &updates {
        let Some(branch) = u.refname.strip_prefix("refs/heads/") else {
            continue;
        };
        let Some(new) = &u.new else { continue };
        let range = match &u.old {
            Some(old) => format!("{old}..{new}"),
            None => new.clone(),
        };
        let count = st.git.count(&repo.path, &range).await.unwrap_or(1).max(1) as i64;
        record_event(&st.db, user.id, Some(repo.id), "push", branch, new, count).await?;
    }
    crate::hooks::on_push(st, repo, user, &updates).await;
    Ok(())
}

/// Make HEAD point at a branch that exists (first push to an empty repo, or
/// the default branch was deleted).
pub async fn fix_head(
    st: &AppState,
    repo: &Repo,
    refs: &HashMap<String, String>,
) -> Result<(), AppError> {
    let head = st.git.head_branch(&repo.path).await?;
    let has = |b: &str| refs.contains_key(&format!("refs/heads/{b}"));
    if head.as_deref().is_some_and(has) {
        if head.as_deref() != Some(repo.default_branch.as_str()) {
            let (id, b) = (repo.id, head.unwrap_or_default());
            st.db
                .call(move |c| {
                    Ok(c.execute(
                        "UPDATE repos SET default_branch=?1 WHERE id=?2",
                        rusqlite::params![b, id],
                    )?)
                })
                .await?;
        }
        return Ok(());
    }
    let mut branches: Vec<&str> = refs
        .keys()
        .filter_map(|r| r.strip_prefix("refs/heads/"))
        .collect();
    branches.sort();
    let pick = [
        repo.default_branch.as_str(),
        "main",
        "master",
        "trunk",
        "develop",
    ]
    .into_iter()
    .find(|b| has(b))
    .or_else(|| branches.first().copied());
    if let Some(b) = pick {
        st.git.set_head(&repo.path, b).await?;
        let (id, b) = (repo.id, b.to_string());
        st.db
            .call(move |c| {
                Ok(c.execute(
                    "UPDATE repos SET default_branch=?1 WHERE id=?2",
                    rusqlite::params![b, id],
                )?)
            })
            .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cgi_headers() {
        let raw = b"Status: 403 Forbidden\r\nContent-Type: text/plain\r\nExpires: x\r\n\r\nbody";
        let (end, start) = find_header_end(raw).unwrap();
        let (s, h) = parse_cgi_headers(&raw[..end]);
        assert_eq!(s, StatusCode::FORBIDDEN);
        assert_eq!(h[0], ("Content-Type".into(), "text/plain".into()));
        assert_eq!(&raw[start..], b"body");
        assert_eq!(find_header_end(b"A: b\n\nrest"), Some((4, 6)));
    }
}
