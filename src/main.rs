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

const HELP: &str = "gitter — fast, simple git hosting

usage:
  gitter                       start the server (same as `gitter serve`)
  gitter backup <file.tar.gz>  back up accounts, issues and repositories (safe while running; - for stdout)
  gitter verify <file>         check a backup end to end without restoring it (- for stdin)
  gitter restore <file> [--force]
                               restore a backup into GITTER_DATA (- for stdin; --force moves existing data aside)
  gitter doctor [--fsck]       check git, storage, database and repositories
  gitter bench [--check]       time the main pages on a seeded throwaway instance (--check enforces budgets)
  gitter --version

Settings come from environment variables; see the README.";

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("serve");
    let cfg = config();
    match cmd {
        "serve" => serve(cfg),
        "backup" => {
            let out = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("usage: gitter backup <file.tar.gz>"))?;
            gitter::ops::backup(&cfg, std::path::Path::new(out))
        }
        "restore" => {
            let file = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("usage: gitter restore <file.tar.gz> [--force]"))?;
            gitter::ops::restore(
                &cfg,
                std::path::Path::new(file),
                args.iter().any(|a| a == "--force"),
            )
        }
        "verify" => {
            let file = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("usage: gitter verify <file.tar.gz>"))?;
            gitter::ops::verify(&cfg, std::path::Path::new(file))
        }
        "doctor" => {
            let problems = gitter::ops::check(&cfg, args.iter().any(|a| a == "--fsck"))?;
            if problems > 0 {
                std::process::exit(1);
            }
            println!("All good.");
            Ok(())
        }
        "bench" => tokio::runtime::Runtime::new()?
            .block_on(gitter::bench::run(args.iter().any(|a| a == "--check"))),
        "--version" | "-V" | "version" => {
            println!("gitter {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        "--help" | "-h" | "help" => {
            println!("{HELP}");
            Ok(())
        }
        other => {
            eprintln!("unknown command {other:?}\n\n{HELP}");
            std::process::exit(2);
        }
    }
}

#[tokio::main]
async fn serve(cfg: gitter::Config) -> anyhow::Result<()> {
    for d in ["repos", "home"] {
        std::fs::create_dir_all(cfg.data_dir.join(d))?;
    }
    gitter::ops::reconcile(&cfg)?;
    let db = Db::open(&cfg.data_dir.join("gitter.db"))?;
    let addr = cfg.addr.clone();
    let url = cfg.url.clone();
    let st = AppState::new(cfg, db);
    st.settings
        .load(&st.db)
        .await
        .map_err(|e| anyhow::anyhow!(e.msg))?;
    let users: i64 = st
        .db
        .call(|c| Ok(c.query_row("SELECT count(*) FROM users", [], |r| r.get(0))?))
        .await
        .map_err(|e| anyhow::anyhow!(e.msg))?;
    if users == 0 {
        let (code, _) = gitter::auth::new_secret("");
        let code = code[..16].to_string();
        println!(
            "\n  Welcome to Gitter. Create the admin account here (the link works once):\n\n    {}/signup?setup={code}\n",
            url
        );
        *st.setup_code.lock().unwrap_or_else(|p| p.into_inner()) = Some(code);
    }
    let git = st.git.clone();
    tokio::spawn(async move { git.sweep().await });
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
