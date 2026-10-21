//! `gitter bench`: boot a throwaway instance, seed a repository with real
//! history, and time the pages people open most. `--check` fails when a page
//! is slower than its budget, so speed is tested like any other behaviour.

use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::Request;
use tower::ServiceExt;

use crate::{AppState, Config, SignupMode, db::Db};

const COMMITS: usize = 5000;
const FILES: usize = 1000;
const RUNS: usize = 40;

/// Budgets in milliseconds for a release build on a small server:
/// (page, warm p95, cold = first request with an empty cache).
const BUDGETS: &[(&str, f64, f64)] = &[
    ("app shell", 5.0, 5.0),
    ("explore", 15.0, 20.0),
    ("repo home", 30.0, 40.0),
    ("tree", 40.0, 80.0),
    ("tree last commits (root)", 60.0, 120.0),
    ("tree last commits (dir)", 60.0, 120.0),
    ("file", 40.0, 80.0),
    ("history", 40.0, 80.0),
    ("file history", 60.0, 120.0),
    ("commit diff", 40.0, 80.0),
    ("big file diff", 150.0, 200.0),
    ("blame", 80.0, 150.0),
    ("languages", 40.0, 80.0),
];

fn rss_kb(field: &str) -> Option<u64> {
    let s = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = s.lines().find(|l| l.starts_with(field))?;
    line.split_whitespace().nth(1)?.parse().ok()
}

/// A deterministic history: one commit with every file, then small edits.
fn fast_import_stream() -> Vec<u8> {
    let mut s = Vec::new();
    let ext = ["rs", "js", "py", "go", "md"];
    let file = |n: usize| format!("src/d{:02}/f{n:04}.{}", n % 50, ext[n % ext.len()]);
    let body = |n: usize, v: usize| {
        let mut b = String::new();
        for l in 0..24 {
            b.push_str(&format!("// file {n} version {v} line {l}: the quick brown fox jumps over the lazy dog\n"));
        }
        b
    };
    let put = |s: &mut Vec<u8>, t: &str| s.extend_from_slice(t.as_bytes());
    for i in 1..=COMMITS {
        let t = 1_700_000_000 + i * 600;
        let msg = format!("Change {i}: tweak a few files");
        put(&mut s, &format!("commit refs/heads/main\nmark :{i}\nauthor Bench <bench@example.com> {t} +0000\ncommitter Bench <bench@example.com> {t} +0000\ndata {}\n{msg}\n", msg.len()));
        if i > 1 {
            put(&mut s, &format!("from :{}\n", i - 1));
        }
        let touched: Vec<usize> = if i == 1 { (0..FILES).collect() } else { vec![(i * 7) % FILES, (i * 13 + 5) % FILES] };
        for n in touched {
            let b = body(n, i);
            put(&mut s, &format!("M 100644 inline {}\ndata {}\n{b}\n", file(n), b.len()));
        }
    }
    // One huge change, too big for the full diff view.
    let mut big = String::new();
    for l in 0..6000 {
        big.push_str(&format!("row {l}, value {}\n", l * 31 % 977));
    }
    let t = 1_700_000_000 + (COMMITS + 1) * 600;
    put(&mut s, &format!("commit refs/heads/main\nmark :{}\nauthor Bench <bench@example.com> {t} +0000\ncommitter Bench <bench@example.com> {t} +0000\ndata 13\nAdd big table\nfrom :{COMMITS}\nM 100644 inline data/table.csv\ndata {}\n{big}\n", COMMITS + 1, big.len()));
    s
}

struct Row {
    name: &'static str,
    cold: f64,
    p50: f64,
    p95: f64,
}

async fn time(st: &AppState, app: &axum::Router, name: &'static str, path: &str) -> anyhow::Result<Row> {
    let mut samples = Vec::with_capacity(RUNS);
    let mut cold = 0.0;
    st.git.memo.clear();
    for i in 0..RUNS + 1 {
        let req = Request::get(path).header("host", "bench.local").body(Body::empty())?;
        let t = Instant::now();
        let resp = app.clone().oneshot(req).await?;
        let status = resp.status();
        axum::body::to_bytes(resp.into_body(), usize::MAX).await?;
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        if !status.is_success() {
            anyhow::bail!("{name}: GET {path} returned {status}");
        }
        if i == 0 {
            cold = ms;
        } else {
            samples.push(ms);
        }
    }
    samples.sort_by(f64::total_cmp);
    let pick = |q: f64| samples[((samples.len() as f64 - 1.0) * q).round() as usize];
    Ok(Row { name, cold, p50: pick(0.5), p95: pick(0.95) })
}

pub async fn run(check: bool) -> anyhow::Result<()> {
    let dir = std::env::temp_dir().join(format!("gitter-bench-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for d in ["repos", "home"] {
        std::fs::create_dir_all(dir.join(d))?;
    }
    let res = bench(&dir, check).await;
    let _ = std::fs::remove_dir_all(&dir);
    res
}

async fn bench(dir: &std::path::Path, check: bool) -> anyhow::Result<()> {
    let cfg = Config {
        data_dir: dir.to_path_buf(),
        addr: String::new(),
        url: "http://bench.local".into(),
        secure_cookies: false,
        trust_proxy: false,
        signup: SignupMode::Open,
        max_push_bytes: 1 << 30,
        max_repos_per_user: 100,
    };
    let st = AppState::new(cfg, Db::open(&dir.join("gitter.db"))?);
    let app = crate::router(st.clone());
    let idle = rss_kb("VmRSS:");

    // Seed: an account and a repository with real history.
    let seed = Instant::now();
    let signup = Request::post("/api/auth/signup")
        .header("host", "bench.local")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"username":"bench","email":"bench@example.com","password":"correct horse"}"#))?;
    let resp = app.clone().oneshot(signup).await?;
    let cookie = resp
        .headers()
        .get("set-cookie")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        .unwrap_or("")
        .to_string();
    let create = Request::post("/api/repos")
        .header("host", "bench.local")
        .header("origin", "http://bench.local")
        .header("cookie", cookie)
        .header("content-type", "application/json")
        .body(Body::from(r#"{"name":"big"}"#))?;
    let resp = app.clone().oneshot(create).await?;
    anyhow::ensure!(resp.status().is_success(), "creating the repo failed: {}", resp.status());
    let path = st.git.repo_path(1);
    let out = st
        .git
        .run_timeout(&path, &["fast-import", "--quiet"], Some(&fast_import_stream()), &[], Duration::from_secs(300))
        .await
        .map_err(|e| anyhow::anyhow!(e.msg))?;
    anyhow::ensure!(out.code == 0, "fast-import: {}", String::from_utf8_lossy(&out.stderr));
    crate::hooks::after_import(&st, 1).await;
    let seed = seed.elapsed();

    let head = st
        .git
        .resolve(&path, "refs/heads/main")
        .await
        .map_err(|e| anyhow::anyhow!(e.msg))?
        .unwrap_or_default();
    let base = "/api/repos/bench/big";
    let pages: Vec<(&'static str, String)> = vec![
        ("app shell", "/".into()),
        ("explore", "/api/explore".into()),
        ("repo home", base.into()),
        ("tree", format!("{base}/tree")),
        ("tree last commits (root)", format!("{base}/tree-commits")),
        ("tree last commits (dir)", format!("{base}/tree-commits?path=src/d07")),
        ("file", format!("{base}/blob?path=src/d07/f0007.py")),
        ("history", format!("{base}/commits")),
        ("file history", format!("{base}/commits?path=src/d07/f0007.py")),
        ("commit diff", format!("{base}/commit/{head}~1")),
        ("big file diff", format!("{base}/commit/{head}?path=data/table.csv")),
        ("blame", format!("{base}/blame?path=src/d07/f0007.py")),
        ("languages", format!("{base}/languages")),
    ];
    let mut rows = Vec::new();
    for (name, p) in &pages {
        rows.push(time(&st, &app, name, p).await?);
    }

    // A real clone over HTTP, through a socket like any git client.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>()).await;
    });
    let clone_dir = dir.join("clone.git");
    let t = Instant::now();
    let ok = tokio::process::Command::new("git")
        .args(["clone", "--bare", "-q"])
        .arg(format!("http://127.0.0.1:{port}/bench/big.git"))
        .arg(&clone_dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .status()
        .await?
        .success();
    let clone_ms = t.elapsed().as_secs_f64() * 1000.0;
    server.abort();
    anyhow::ensure!(ok, "git clone over HTTP failed");

    println!("gitter {} bench: {COMMITS} commits, {FILES} files, {RUNS} runs per page (seeded in {:.1}s)\n", env!("CARGO_PKG_VERSION"), seed.as_secs_f64());
    println!("{:<26} {:>9} {:>9} {:>9} {:>13}", "page", "cold ms", "p50 ms", "p95 ms", "budget c/p95");
    let mut over = Vec::new();
    for r in &rows {
        let budget = BUDGETS.iter().find(|b| b.0 == r.name);
        let flag = match budget {
            Some(b) if r.p95 > b.1 || r.cold > b.2 => {
                over.push(r.name);
                " ✗"
            }
            _ => "",
        };
        println!(
            "{:<26} {:>9.1} {:>9.1} {:>9.1} {:>13}{flag}",
            r.name,
            r.cold,
            r.p50,
            r.p95,
            budget.map(|b| format!("{:.0}/{:.0}", b.2, b.1)).unwrap_or_default()
        );
    }
    println!("{:<26} {:>9.0}", "git clone (full history)", clone_ms);
    let mb = |k: Option<u64>| k.map(|k| format!("{:.0} MB", k as f64 / 1024.0)).unwrap_or_else(|| "n/a".into());
    println!("\nmemory: {} after boot, {} after the run", mb(idle), mb(rss_kb("VmRSS:")));
    if check && !over.is_empty() {
        anyhow::bail!("over budget: {}", over.join(", "));
    }
    if check {
        println!("✓ every page is within its budget");
    }
    Ok(())
}
