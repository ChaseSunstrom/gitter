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
            "invite" => SignupMode::Invite,
            "closed" => SignupMode::Closed,
            other => {
                eprintln!("GITTER_SIGNUP must be 'open', 'invite' or 'closed', not '{other}'");
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
  gitter backup <file.tar.gz>  back up accounts, issues, repositories, wikis, snippets and release files (safe while running; - for stdout)
  gitter verify <file>         check a backup end to end without restoring it (- for stdin)
  gitter restore <file> [--force]
                               restore a backup into GITTER_DATA (- for stdin; --force moves existing data aside)
  gitter doctor [--fsck]       check git, storage, database and repositories
  gitter bench [--check]       time the main pages on a seeded throwaway instance (--check enforces budgets)
  gitter demo                  seed an empty GITTER_DATA with sample people, repositories, issues, pull requests,
                               CI runs and a wiki, then start the server
  gitter runner --url URL --token TOKEN [--labels a,b] [--jobs N] [--work DIR]
                               run CI jobs for a Gitter server on this machine (token from Site administration → Runners;
                               or set GITTER_RUNNER_TOKEN)
  gitter --version

client (like gh; `gitter <command> --help` for each):
  gitter login URL [--token T]  save a server and an access token (or GITTER_TOKEN) to ~/.config/gitter/config.json
  gitter repo create|clone|list|view
  gitter issue list|create|view|close|comment
  gitter pr create|list|view|checkout|merge
  gitter ci status|logs|rerun   runs on the server
  gitter ci run [--here]        run .gitter/ci.yml on this machine, no server needed

Settings come from environment variables; see the README.";

fn main() -> anyhow::Result<()> {
    // Git runs this binary as a repository's pre-receive hook (a link named so).
    if std::env::args_os().next().is_some_and(|a| std::path::Path::new(&a).file_name().is_some_and(|f| f == "pre-receive")) {
        std::process::exit(gitter::protect::pre_receive());
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("serve");
    if matches!(cmd, "login" | "repo" | "issue" | "pr" | "ci") {
        return gitter::cli::main(&args);
    }
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
        "runner" => {
            let known = ["--url", "--token", "--labels", "--jobs", "--work"];
            for pair in args[1..].chunks(2) {
                if !known.contains(&pair[0].as_str()) || pair.len() < 2 {
                    anyhow::bail!("unknown or incomplete option {:?}\n\n{HELP}", pair[0]);
                }
            }
            let flag = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
            let url = flag("--url").ok_or_else(|| anyhow::anyhow!("usage: gitter runner --url https://git.example.com --token grn_…"))?;
            let token = flag("--token")
                .or_else(|| std::env::var("GITTER_RUNNER_TOKEN").ok())
                .ok_or_else(|| anyhow::anyhow!("pass --token or set GITTER_RUNNER_TOKEN"))?;
            let labels: Vec<String> = flag("--labels").map(|l| l.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()).unwrap_or_default();
            let jobs: usize = match flag("--jobs") {
                None => 1,
                Some(n) => n.parse().ok().filter(|n| (1..=256).contains(n)).ok_or_else(|| anyhow::anyhow!("--jobs takes a number from 1 to 256"))?,
            };
            let work = PathBuf::from(flag("--work").unwrap_or_else(|| "gitter-runner-work".into()));
            runner(url, token, labels, jobs, work)
        }
        "bench" => tokio::runtime::Runtime::new()?
            .block_on(gitter::bench::run(args.iter().any(|a| a == "--check"))),
        "demo" => {
            let mut cfg = cfg;
            // A demo has a known admin: only this machine reaches it unless GITTER_ADDR says otherwise.
            if std::env::var_os("GITTER_ADDR").is_none() {
                cfg.addr = "127.0.0.1:3000".into();
            }
            tokio::runtime::Runtime::new()?.block_on(gitter::demo::run(&cfg))?;
            serve(cfg)
        }
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
async fn runner(url: String, token: String, extra: Vec<String>, jobs: usize, work: PathBuf) -> anyhow::Result<()> {
    use gitter::ci::runner::{Backend, Runner, default_labels, serve};
    std::fs::create_dir_all(&work)?;
    let mut labels = default_labels().await;
    labels.extend(extra);
    println!("gitter runner: {} job(s) at once for {url}, labels {}", jobs, labels.join(", "));
    let r = Runner { backend: std::sync::Arc::new(Backend::remote(&url, &token)), labels, work: work.canonicalize()? };
    tokio::select! {
        _ = serve(r, move || jobs, None) => {}
        _ = stop_signal() => {
            println!("stopping: killing running steps (their jobs go back to the queue)");
            gitter::ci::runner::shutdown(std::time::Duration::from_secs(30)).await;
        }
    }
    Ok(())
}

#[tokio::main]
async fn serve(mut cfg: gitter::Config) -> anyhow::Result<()> {
    // The git hook links to this binary: take its path now, while it still names this file.
    let _ = gitter::protect::HOOK_EXE.set(std::env::current_exe()?);
    for d in ["repos", "home"] {
        std::fs::create_dir_all(cfg.data_dir.join(d))?;
    }
    // Absolute, so paths handed to git and CI jobs (file:// URLs, docker mounts) don't depend on the working directory.
    cfg.data_dir = cfg.data_dir.canonicalize()?;
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
    match gitter::security::seal_totp_secrets(&st.db, st.key.clone()).await {
        Ok(0) => {}
        Ok(n) => println!("sealed {n} two-factor secret(s) with the server key"),
        Err(e) => eprintln!("sealing two-factor secrets: {}", e.msg),
    }
    // Git over SSH, when asked for. GITTER_SSH_PORT is the port clone URLs show, if it's mapped to another.
    if let Ok(ssh_addr) = std::env::var("GITTER_SSH_ADDR")
        && !ssh_addr.trim().is_empty()
    {
        let port = match env("GITTER_SSH_PORT", "").as_str() {
            "" => None,
            p => Some(p.parse::<u16>().map_err(|_| anyhow::anyhow!("GITTER_SSH_PORT must be a port number, not {p:?}"))?),
        };
        let bound = gitter::ssh::start(&st, ssh_addr.trim(), port).await?;
        println!("git over SSH on {bound}");
    }
    tokio::spawn(gitter::ci::upkeep(st.clone()));
    tokio::spawn(gitter::api::lifecycle::upkeep(st.clone()));
    tokio::spawn(gitter::ci::runner::builtin(st.clone()));
    tokio::spawn(gitter::webhooks::run(st.clone()));
    tokio::spawn(gitter::mirrors::run(st.clone()));
    tokio::spawn(gitter::api::insights::flush_loop(st.clone()));
    let (traffic, db) = (st.traffic.clone(), st.db.clone());
    let st2 = st.clone();
    tokio::spawn(async move {
        st2.git.sweep().await;
        gitter::hooks::backfill_sizes(&st2).await;
        loop {
            gitter::lfs::sweep(&st2).await;
            gitter::registry::sweep(&st2).await;
            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
        }
    });
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
    // Requests have drained: save the last minute of traffic counts.
    traffic.flush(&db).await;
    Ok(())
}

async fn shutdown() {
    stop_signal().await;
    println!("shutting down");
    // Kill running CI steps (they're in their own process groups) before exiting.
    gitter::ci::runner::shutdown(std::time::Duration::from_secs(10)).await;
}

/// Ctrl-C or SIGTERM.
async fn stop_signal() {
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
}
