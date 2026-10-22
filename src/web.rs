//! Serves the embedded web app, plus security headers and CSRF checks.

use std::sync::OnceLock;

use axum::body::Body;
use axum::extract::{FromRequestParts, Path, Request, State};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;
use sha2::{Digest, Sha256};

use crate::AppState;
use crate::auth::{SESSION_COOKIE, cookie_value};

#[derive(RustEmbed)]
#[folder = "static/"]
struct Assets;

/// A short hash of every embedded file. Asset URLs carry it (`/assets/<v>/...`)
/// so they can be cached forever and still change on upgrade.
pub fn version() -> &'static str {
    static V: OnceLock<String> = OnceLock::new();
    V.get_or_init(|| {
        let mut names: Vec<_> = Assets::iter().collect();
        names.sort();
        let mut h = Sha256::new();
        for n in names {
            if let Some(f) = Assets::get(&n) {
                h.update(n.as_bytes());
                h.update(f.metadata.sha256_hash());
            }
        }
        hex::encode(&h.finalize()[..6])
    })
}

fn index_html() -> Vec<u8> {
    let raw = Assets::get("index.html")
        .map(|f| f.data.into_owned())
        .unwrap_or_default();
    String::from_utf8_lossy(&raw)
        .replace("__V__", version())
        .into_bytes()
}

pub async fn index(State(st): State<AppState>, req: Request) -> Response {
    if let Some(to) = moved_page(&st, req).await {
        return redirect(StatusCode::MOVED_PERMANENTLY, &to);
    }
    let mut r = Response::new(Body::from(index_html()));
    r.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    r.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    r
}

/// A page of a renamed or transferred repository (`/owner/repo/...`): where it
/// lives now, if the viewer may see it there.
async fn moved_page(st: &AppState, req: Request) -> Option<String> {
    let path = req.uri().path().to_string();
    let mut segs = path.trim_start_matches('/').splitn(3, '/');
    let (o, n) = (segs.next()?.to_string(), segs.next()?.to_string());
    let rest = segs.next().map(|r| format!("/{r}")).unwrap_or_default();
    if !crate::validate::username(&o) || !crate::validate::repo_name(&n) {
        return None;
    }
    let (repo, moved) = st.db.call(move |c| Ok(crate::repo::lookup(c, &o, &n)?)).await.ok()??;
    if !moved {
        return None;
    }
    let (mut parts, _) = req.into_parts();
    let viewer = crate::auth::Viewer::from_request_parts(&mut parts, st).await.ok()?.0;
    let repo = crate::repo::Repo::with_access(st, repo, viewer.as_ref());
    let query = parts.uri.query().map(|q| format!("?{q}")).unwrap_or_default();
    repo.can_read.then(|| format!("/{}/{}{rest}{query}", repo.owner, repo.name))
}

fn redirect(status: StatusCode, to: &str) -> Response {
    match HeaderValue::from_str(to) {
        Ok(v) => {
            let body = axum::Json(serde_json::json!({ "error": "This repository moved", "location": to }));
            (status, [(header::LOCATION, v)], body).into_response()
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Turn the `Moved` mark a handler's 404 carries into a redirect to the same
/// path at the repository's new place: 301 for reads, 308 for writes (so a
/// client that follows it repeats the POST instead of turning it into a GET).
/// Only paths that name the repository (`/api/repos/o/r/…`, `/raw/o/r/…`,
/// `/archive/o/r/…`) are rewritten; anything else stays a 404.
pub async fn follow_moved(req: Request, next: Next) -> Response {
    let (read, uri) = (matches!(*req.method(), Method::GET | Method::HEAD), req.uri().clone());
    let r = next.run(req).await;
    let Some(m) = r.extensions().get::<crate::error::Moved>() else { return r };
    match moved_path(uri.path(), m) {
        Some(p) => {
            let to = format!("{p}{}", uri.query().map(|q| format!("?{q}")).unwrap_or_default());
            redirect(if read { StatusCode::MOVED_PERMANENTLY } else { StatusCode::PERMANENT_REDIRECT }, &to)
        }
        None => r,
    }
}

fn moved_path(path: &str, m: &crate::error::Moved) -> Option<String> {
    let mut segs: Vec<&str> = path.split('/').collect();
    let at = match (segs.get(1), segs.get(2)) {
        (Some(&"api"), Some(&"repos")) => 3,
        (Some(&"raw" | &"archive"), _) => 2,
        _ => return None,
    };
    let (o, n) = (*segs.get(at)?, *segs.get(at + 1)?);
    if !o.eq_ignore_ascii_case(&m.from.0) || !n.eq_ignore_ascii_case(&m.from.1) {
        return None;
    }
    segs[at] = &m.to.0;
    segs[at + 1] = &m.to.1;
    Some(segs.join("/"))
}

pub async fn asset(Path((_v, path)): Path<(String, String)>, req: Request) -> Response {
    serve_file(&path, &req, true)
}

pub async fn favicon(req: Request) -> Response {
    serve_file("favicon.svg", &req, false)
}

pub async fn robots() -> Response {
    (
        [(header::CONTENT_TYPE, "text/plain")],
        "User-agent: *\nDisallow: /api/\nDisallow: /raw/\nDisallow: /archive/\n",
    )
        .into_response()
}

fn serve_file(path: &str, req: &Request, versioned: bool) -> Response {
    let Some(f) = Assets::get(path) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let etag = format!("\"{}\"", hex::encode(&f.metadata.sha256_hash()[..12]));
    let cache = if versioned && !cfg!(debug_assertions) {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    if req
        .headers()
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        == Some(etag.as_str())
    {
        return (
            StatusCode::NOT_MODIFIED,
            [
                (header::ETAG, etag),
                (header::CACHE_CONTROL, cache.to_string()),
            ],
        )
            .into_response();
    }
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let mut r = Response::new(Body::from(f.data.into_owned()));
    let h = r.headers_mut();
    if let Ok(v) = HeaderValue::from_str(mime.as_ref()) {
        h.insert(header::CONTENT_TYPE, v);
    }
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    if let Ok(v) = HeaderValue::from_str(&etag) {
        h.insert(header::ETAG, v);
    }
    r
}

const CSP: &str = "default-src 'self'; img-src 'self' data: https:; style-src 'self' 'unsafe-inline'; script-src 'self'; \
    connect-src 'self'; font-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'";

pub async fn security_headers(req: Request, next: Next) -> Response {
    let mut r = next.run(req).await;
    let h = r.headers_mut();
    if !h.contains_key(header::CONTENT_SECURITY_POLICY) {
        h.insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(CSP),
        );
    }
    h.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    h.insert("x-frame-options", HeaderValue::from_static("DENY"));
    h.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    h.insert(
        "permissions-policy",
        HeaderValue::from_static("camera=(), microphone=(), geolocation=(), interest-cohort=()"),
    );
    r
}

/// Reject cross-site writes made with the session cookie: a mutating API call
/// that carries the cookie must come from this site's own pages. "This site" is
/// the configured public URL, the Host header, or (behind a trusted proxy)
/// X-Forwarded-Host, so it works directly and behind nginx/Caddy alike.
pub async fn csrf(State(st): State<AppState>, req: Request, next: Next) -> Response {
    let mutating = !matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS);
    if mutating && cookie_value(req.headers(), SESSION_COOKIE).is_some() && !same_origin(&req, &st)
    {
        return (
            StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({ "error": "Cross-site request blocked" })),
        )
            .into_response();
    }
    next.run(req).await
}

fn host_of(url: &str) -> &str {
    let rest = url.split_once("://").map(|x| x.1).unwrap_or("");
    rest.split('/').next().unwrap_or("")
}

fn same_origin(req: &Request, st: &AppState) -> bool {
    let h = req.headers();
    let origin = h
        .get(header::ORIGIN)
        .or_else(|| h.get(header::REFERER))
        .and_then(|v| v.to_str().ok());
    let Some(origin) = origin else { return false };
    let o = host_of(origin);
    if o.is_empty() {
        return false;
    }
    let mut allowed = vec![host_of(&st.cfg.url)];
    if let Some(v) = h.get(header::HOST).and_then(|v| v.to_str().ok()) {
        allowed.push(v);
    }
    if st.cfg.trust_proxy
        && let Some(v) = h.get("x-forwarded-host").and_then(|v| v.to_str().ok())
    {
        allowed.push(v);
    }
    allowed
        .iter()
        .any(|a| !a.is_empty() && a.eq_ignore_ascii_case(o))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(url: &str) -> AppState {
        let cfg = crate::Config {
            data_dir: std::env::temp_dir(),
            addr: String::new(),
            url: url.into(),
            secure_cookies: false,
            trust_proxy: false,
            signup: crate::SignupMode::Open,
            max_push_bytes: 1,
            max_repos_per_user: 1,
        };
        AppState::new(cfg, crate::db::Db::memory().unwrap())
    }

    fn req(origin: Option<&str>, host: &str) -> Request {
        let mut b = Request::builder()
            .method("POST")
            .uri("/api/x")
            .header("host", host);
        if let Some(o) = origin {
            b = b.header("origin", o);
        }
        b.body(Body::empty()).unwrap()
    }

    #[test]
    fn origin_checks() {
        let st = state("https://git.example.com");
        assert!(same_origin(
            &req(Some("http://localhost:3000"), "localhost:3000"),
            &st
        ));
        assert!(!same_origin(
            &req(Some("https://evil.com"), "localhost:3000"),
            &st
        ));
        assert!(!same_origin(
            &req(Some("http://localhost:3000.evil.com"), "localhost:3000"),
            &st
        ));
        assert!(!same_origin(&req(None, "localhost:3000"), &st));
        // Behind a proxy that rewrites Host, the configured URL still matches.
        assert!(same_origin(
            &req(Some("https://git.example.com"), "127.0.0.1:3000"),
            &st
        ));
    }

    #[test]
    fn index_has_version() {
        let v = version();
        assert_eq!(v.len(), 12);
    }
}
