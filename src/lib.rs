pub mod api;
pub mod auth;
pub mod bench;
pub mod db;
pub mod diff;
pub mod error;
pub mod git;
pub mod hooks;
pub mod langs;
pub mod ops;
pub mod repo;
pub mod security;
pub mod settings;
pub mod smart_http;
pub mod validate;
pub mod web;

use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use tower_http::compression::CompressionLayer;

/// Settings read once at startup.
#[derive(Clone, Debug)]
pub struct Config {
    pub data_dir: PathBuf,
    pub addr: String,
    /// Public base URL, e.g. https://git.example.com (used for clone URLs and cookies).
    pub url: String,
    pub secure_cookies: bool,
    pub trust_proxy: bool,
    pub signup: SignupMode,
    pub max_push_bytes: u64,
    pub max_repos_per_user: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignupMode {
    Open,
    Closed,
}

#[derive(Clone)]
pub struct AppState {
    pub db: db::Db,
    pub git: git::Git,
    pub cfg: Arc<Config>,
    pub limiter: Arc<auth::Limiter>,
    pub settings: Arc<settings::Live>,
    /// Unix time the server started.
    pub started: i64,
    /// One-time code required to create the first (admin) account, printed at boot.
    /// None means no code is needed (tests, or an instance that already has users).
    pub setup_code: Arc<std::sync::Mutex<Option<String>>>,
    /// Sign-ins waiting for a two-factor code.
    pub challenges: Arc<security::Challenges>,
}

impl AppState {
    pub fn new(cfg: Config, db: db::Db) -> AppState {
        let git = git::Git::new(cfg.data_dir.join("repos"), cfg.data_dir.join("home"), 64);
        let settings = Arc::new(settings::Live::new(settings::Settings::from_config(&cfg)));
        AppState {
            db,
            git,
            cfg: Arc::new(cfg),
            limiter: Arc::new(auth::Limiter::default()),
            settings,
            started: db::now(),
            setup_code: Arc::default(),
            challenges: Arc::default(),
        }
    }
}

pub fn router(st: AppState) -> Router {
    let git_http = Router::new()
        .route("/{owner}/{repo}/info/refs", get(smart_http::info_refs))
        .route(
            "/{owner}/{repo}/git-upload-pack",
            post(smart_http::upload_pack),
        )
        .route(
            "/{owner}/{repo}/git-receive-pack",
            post(smart_http::receive_pack),
        )
        .layer(DefaultBodyLimit::disable());

    let app = Router::new()
        .nest(
            "/api",
            api::router()
                .layer(DefaultBodyLimit::max(40 * 1024 * 1024))
                .layer(axum::middleware::from_fn_with_state(st.clone(), api::scope_guard))
                .layer(axum::middleware::from_fn_with_state(st.clone(), web::csrf)),
        )
        .route("/raw/{owner}/{repo}/{*rest}", get(api::code::raw))
        .route("/assets/{v}/{*path}", get(web::asset))
        .route("/favicon.svg", get(web::favicon))
        .route("/favicon.ico", get(web::favicon))
        .route("/robots.txt", get(web::robots))
        .route("/healthz", get(api::admin::healthz))
        .fallback(get(web::index))
        .layer(CompressionLayer::new());

    Router::new()
        .route("/archive/{owner}/{repo}/{*spec}", get(api::code::archive))
        .merge(git_http)
        .merge(app)
        .layer(axum::middleware::from_fn(web::security_headers))
        .with_state(st)
}
