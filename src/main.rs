use std::net::SocketAddr;
use std::path::PathBuf;

use gitter::{AppState, Config, SignupMode, db::Db};

fn env(k: &str, default: &str) -> String {
    std::env::var(k)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn config() -> Config {
    let url = env("GITTER_URL", "http://localhost:3000")
        .trim_end_matches('/')
        .to_string();
    Config {
        data_dir: PathBuf::from(env("GITTER_DATA", "./data")),
        addr: env("GITTER_ADDR", "0.0.0.0:3000"),
        secure_cookies: url.starts_with("https://"),
        trust_proxy: matches!(
            env("GITTER_TRUST_PROXY", "false").as_str(),
            "1" | "true" | "yes"
        ),
        signup: match env("GITTER_SIGNUP", "open").to_ascii_lowercase().as_str() {
            "open" => SignupMode::Open,
            "closed" => SignupMode::Closed,
            other => {
                eprintln!("GITTER_SIGNUP must be 'open' or 'closed', not '{other}'");
                std::process::exit(2);
            }
        },
        max_push_bytes: env("GITTER_MAX_PUSH_MB", "500")
            .parse::<u64>()
            .unwrap_or(500)
            * 1024
            * 1024,
        max_repos_per_user: env("GITTER_MAX_REPOS", "200").parse().unwrap_or(200),
        url,
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = config();
    for d in ["repos", "home"] {
        std::fs::create_dir_all(cfg.data_dir.join(d))?;
    }
    let db = Db::open(&cfg.data_dir.join("gitter.db"))?;
    let addr = cfg.addr.clone();
    let url = cfg.url.clone();
    let st = AppState::new(cfg, db);
    let app = gitter::router(st);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!(
        "gitter {} listening on {addr} — open {url}",
        env!("CARGO_PKG_VERSION")
    );
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown())
    .await?;
    Ok(())
}

async fn shutdown() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = term => {} }
    println!("shutting down");
}
