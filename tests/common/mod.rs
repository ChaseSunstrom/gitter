//! The in-process test harness shared by the API and CI tests.
#![allow(dead_code)]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use gitter::{AppState, Config, SignupMode, db::Db};

pub const ORIGIN: &str = "http://test.local";

pub struct App {
    pub router: axum::Router,
    pub state: AppState,
    _dir: tempfile::TempDir,
}

/// A signed-in (or anonymous) client: just a session cookie.
#[derive(Clone, Default)]
pub struct Client(pub Option<String>);

impl App {
    pub fn new() -> App {
        App::with_url(ORIGIN)
    }

    /// A server whose public address is `url` (one that listens there, for other servers).
    pub fn with_url(url: &str) -> App {
        // Git runs the built binary as the pre-receive hook (branch rules).
        let _ = gitter::protect::HOOK_EXE.set(env!("CARGO_BIN_EXE_gitter").into());
        let dir = tempfile::tempdir().unwrap();
        for d in ["repos", "home"] {
            std::fs::create_dir_all(dir.path().join(d)).unwrap();
        }
        let cfg = Config {
            data_dir: dir.path().to_path_buf(),
            addr: String::new(),
            url: url.into(),
            secure_cookies: false,
            trust_proxy: false,
            signup: SignupMode::Open,
            max_push_bytes: 10 * 1024 * 1024,
            max_repos_per_user: 50,
        };
        let state = AppState::new(cfg, Db::memory().unwrap());
        App {
            router: gitter::router(state.clone()),
            state,
            _dir: dir,
        }
    }

    pub async fn call(
        &self,
        c: &Client,
        method: &str,
        path: &str,
        body: Option<Value>,
        origin: Option<&str>,
    ) -> (StatusCode, Value, Option<String>) {
        let mut b = Request::builder()
            .method(method)
            .uri(path)
            .header("host", "test.local");
        if let Some(o) = origin {
            b = b.header("origin", o);
        }
        if let Some(cookie) = &c.0 {
            b = b.header("cookie", format!("gitter_session={cookie}"));
        }
        let req = match body {
            Some(v) => b
                .header("content-type", "application/json")
                .body(Body::from(v.to_string()))
                .unwrap(),
            None => b.body(Body::empty()).unwrap(),
        };
        let resp = self.router.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let set_cookie = resp
            .headers()
            .get("set-cookie")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(';').next())
            .and_then(|kv| kv.strip_prefix("gitter_session="))
            .map(String::from);
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, json, set_cookie)
    }

    pub async fn req(
        &self,
        c: &Client,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let (s, v, _) = self.call(c, method, path, body, Some(ORIGIN)).await;
        (s, v)
    }

    pub async fn signup(&self, name: &str) -> Client {
        let body = json!({ "username": name, "email": format!("{name}@example.com"), "password": "correct horse" });
        let (s, _, cookie) = self
            .call(
                &Client::default(),
                "POST",
                "/api/auth/signup",
                Some(body),
                Some(ORIGIN),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "signup {name}");
        Client(cookie)
    }
}
