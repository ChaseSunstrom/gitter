//! Runs CI jobs. The runner built into the server and `gitter runner` on other
//! machines share this code; they differ only in how they reach the queue
//! (`Backend`).
//!
//! A job: check out the commit into a fresh workspace, run each step with
//! `sh -e -c` (inside `docker run` when the job names an image), stream the
//! output with secrets masked, pack the artifacts, report the result. A
//! heartbeat keeps the lease; when it's refused (cancelled, or the job went to
//! another runner) the job is killed.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use base64::Engine;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::{Notify, mpsc};

use super::cache::{self, walk};
use super::config::{any_glob, glob};
use super::queue::Lease;
use crate::AppState;

const HEARTBEAT: Duration = Duration::from_secs(20);
/// Output is sent in batches this often.
const FLUSH: Duration = Duration::from_millis(400);
/// Set when the process is shutting down: running steps are killed, no new jobs start.
pub static SHUTDOWN: AtomicBool = AtomicBool::new(false);
/// Jobs running in this process.
static RUNNING: AtomicUsize = AtomicUsize::new(0);

/// How a runner reaches the queue.
pub enum Backend {
    /// Inside the server: straight to the database and the data folder.
    Local { st: AppState, runner_id: i64 },
    /// `gitter runner`: the server's runner API over HTTP.
    Remote { base: String, token: String, agent: ureq::Agent },
}

type Res<T> = Result<T, String>;

impl Backend {
    pub fn remote(base: &str, token: &str) -> Backend {
        let cfg = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(120)))
            .http_status_as_error(false)
            .build();
        Backend::Remote { base: base.trim_end_matches('/').to_string(), token: token.into(), agent: ureq::Agent::new_with_config(cfg) }
    }

    /// Run a blocking HTTP call against `{base}/ci-runner/{path}` off the async threads.
    async fn http<T: Send + 'static>(
        &self,
        f: impl FnOnce(&ureq::Agent, String) -> Result<T, ureq::Error> + Send + 'static,
        path: String,
    ) -> Res<T> {
        let Backend::Remote { base, agent, .. } = self else { return Err("not a remote runner".into()) };
        let (agent, url) = (agent.clone(), format!("{base}/ci-runner/{path}"));
        tokio::task::spawn_blocking(move || f(&agent, url)).await.map_err(|e| e.to_string())?.map_err(|e| e.to_string())
    }

    async fn claim(&self, labels: &[String]) -> Res<Option<Lease>> {
        match self {
            Backend::Local { st, runner_id } => super::jobs::claim(st, *runner_id, labels.to_vec(), true).await.map_err(|e| e.msg),
            Backend::Remote { token, .. } => {
                let (auth, body) = (format!("Bearer {token}"), serde_json::json!({ "labels": labels }));
                self.http(
                    move |a, url| {
                        let mut r = a.post(url).header("authorization", auth).send_json(body)?;
                        Ok(match r.status().as_u16() {
                            200 => Ok(Some(r.body_mut().read_json::<Lease>()?)),
                            204 => Ok(None),
                            s => Err(format!("the server answered {s}: {}", r.body_mut().read_to_string().unwrap_or_default())),
                        })
                    },
                    "claim".into(),
                )
                .await?
            }
        }
    }

    /// Some(false): stop the job. None: couldn't ask (keep going; the lease decides).
    async fn heartbeat(&self, l: &Lease) -> Option<bool> {
        match self {
            Backend::Local { st, .. } => super::jobs::heartbeat(st, l.job_id, &l.token).await.ok(),
            Backend::Remote { .. } => {
                let auth = format!("Bearer {}", l.token);
                let r = self
                    .http(
                        move |a, url| {
                            let mut r = a.post(url).header("authorization", auth).send_empty()?;
                            Ok((r.status().as_u16(), r.body_mut().read_json::<serde_json::Value>().ok()))
                        },
                        format!("jobs/{}/heartbeat", l.job_id),
                    )
                    .await
                    .ok()?;
                match r {
                    (200, Some(v)) => v["ok"].as_bool(),
                    (403 | 404, _) => Some(false),
                    _ => None,
                }
            }
        }
    }

    async fn send(&self, l: &Lease, path: &str, body: Vec<u8>) -> Res<()> {
        let auth = format!("Bearer {}", l.token);
        let status = self
            .http(move |a, url| Ok(a.post(url).header("authorization", auth).send(body)?.status().as_u16()), format!("jobs/{}/{path}", l.job_id))
            .await?;
        if status == 200 { Ok(()) } else { Err(format!("the server answered {status}")) }
    }

    async fn log(&self, l: &Lease, data: Vec<u8>) {
        let res = match self {
            // A stale attempt (cancelled, or the job went elsewhere) must not write over the new one.
            Backend::Local { st, .. } if !super::jobs::holds(st, l.job_id, &l.token).await => return,
            Backend::Local { st, .. } => super::jobs::append_log(st, l.job_id, &data).await.map_err(|e| e.msg),
            Backend::Remote { .. } => self.send(l, "log", data).await,
        };
        if let Err(e) = res {
            eprintln!("runner: job {} log: {e}", l.job_id);
        }
    }

    async fn step(&self, l: &Lease, index: usize, status: &str) {
        let res = match self {
            Backend::Local { st, .. } if !super::jobs::holds(st, l.job_id, &l.token).await => return,
            Backend::Local { st, .. } => super::jobs::step(st, l.job_id, index, status).await.map_err(|e| e.msg),
            Backend::Remote { .. } => self.send(l, &format!("steps/{index}/{status}"), vec![]).await,
        };
        if let Err(e) = res {
            eprintln!("runner: job {} step {index}: {e}", l.job_id);
        }
    }

    /// PUT a file to `jobs/{id}/{path}` of the runner API.
    async fn upload(&self, l: &Lease, path: String, file: &Path) -> Res<()> {
        let (auth, file) = (format!("Bearer {}", l.token), file.to_path_buf());
        let status = self
            .http(
                move |a, url| {
                    let f = std::fs::File::open(&file).map_err(ureq::Error::Io)?;
                    // Up to 1 GB over a slow link: no overall deadline, only per-read/write ones.
                    let req = a.put(url).header("authorization", auth).config().timeout_global(None).timeout_send_body(Some(Duration::from_secs(120))).build();
                    Ok(req.send(f)?.status().as_u16())
                },
                format!("jobs/{}/{path}", l.job_id),
            )
            .await?;
        if status == 200 { Ok(()) } else { Err(format!("the server answered {status}")) }
    }

    async fn artifact(&self, l: &Lease, file: &Path) -> Res<()> {
        match self {
            Backend::Local { st, .. } if !super::jobs::holds(st, l.job_id, &l.token).await => Err("the job moved on".into()),
            Backend::Local { st, .. } => super::jobs::store_artifact(st, l.job_id, file).await.map_err(|e| e.msg),
            Backend::Remote { .. } => self.upload(l, "artifacts".into(), file).await,
        }
    }

    async fn reports(&self, l: &Lease, file: &Path) -> Res<()> {
        match self {
            Backend::Local { st, .. } if !super::jobs::holds(st, l.job_id, &l.token).await => Err("the job moved on".into()),
            Backend::Local { st, .. } => super::reports::store(st, l.job_id, file).await.map_err(|e| e.msg),
            Backend::Remote { .. } => self.upload(l, "reports".into(), file).await,
        }
    }

    /// Save the job's cache under `key` (an expanded, plain key).
    async fn cache_put(&self, l: &Lease, key: &str, file: &Path) -> Res<()> {
        match self {
            Backend::Local { st, .. } if !super::jobs::holds(st, l.job_id, &l.token).await => Err("the job moved on".into()),
            Backend::Local { st, .. } => super::cache::store(st, l.job_id, key.into(), file).await.map(|_| ()).map_err(|e| e.msg),
            Backend::Remote { .. } => self.upload(l, format!("cache?key={key}"), file).await,
        }
    }

    /// The cache to restore: (its key, the archive, whether it's a download to delete after).
    async fn cache_get(&self, l: &Lease, key: &str, restore: &[String], work: &Path) -> Res<Option<(String, PathBuf, bool)>> {
        match self {
            Backend::Local { st, .. } => {
                let hit = super::cache::find(st, l.job_id, key.into(), restore.to_vec()).await.map_err(|e| e.msg)?;
                Ok(hit.map(|(k, p)| (k, p, false)))
            }
            Backend::Remote { .. } => {
                let (auth, out) = (format!("Bearer {}", l.token), work.join(format!("job-{}-cache.tar.gz", l.job_id)));
                let out2 = out.clone();
                let got = self
                    .http(
                        move |a, url| {
                            let mut r = a.get(url).header("authorization", auth).config().timeout_global(None).timeout_recv_body(Some(Duration::from_secs(120))).build().call()?;
                            if r.status().as_u16() != 200 {
                                return Ok(Err(r.status().as_u16()));
                            }
                            let key = r.headers().get("x-cache-key").and_then(|v| v.to_str().ok()).unwrap_or_default().to_string();
                            let mut f = std::fs::File::create(&out2).map_err(ureq::Error::Io)?;
                            let mut body = r.body_mut().with_config().limit(super::cache::ENTRY_CAP).reader();
                            std::io::copy(&mut body, &mut f).map_err(ureq::Error::Io)?;
                            Ok(Ok(key))
                        },
                        format!("jobs/{}/cache?key={key}&restore={}", l.job_id, restore.join(",")),
                    )
                    .await?;
                match got {
                    Ok(k) if !k.is_empty() => Ok(Some((k, out, true))),
                    Ok(_) => Err("the server sent no key".into()),
                    Err(204) => Ok(None),
                    Err(s) => Err(format!("the server answered {s}")),
                }
            }
        }
    }

    async fn finish(&self, l: &Lease, success: bool, code: Option<i32>) {
        let res = match self {
            Backend::Local { st, .. } => super::jobs::finish(st, l.job_id, &l.token, success, code).await.map(|_| ()).map_err(|e| e.msg),
            Backend::Remote { .. } => {
                let body = serde_json::json!({ "success": success, "exit_code": code }).to_string().into_bytes();
                self.send(l, "finish", body).await
            }
        };
        if let Err(e) = res {
            eprintln!("runner: job {} finish: {e}", l.job_id);
        }
    }

    /// Where to fetch the commit from, and the one git config setting that lets it.
    fn source(&self, l: &Lease) -> (String, &'static str, String) {
        match self {
            Backend::Local { st, .. } => {
                (format!("file://{}", st.git.repo_path(l.repo_id).display()), "uploadpack.allowReachableSHA1InWant", "true".into())
            }
            Backend::Remote { base, .. } => {
                let basic = base64::engine::general_purpose::STANDARD.encode(format!("x-access-token:{}", l.token));
                (format!("{base}/{}.git", l.repo), "http.extraHeader", format!("Authorization: Basic {basic}"))
            }
        }
    }
}

/// Labels every runner has: its OS and CPU, and `docker` when docker works.
pub async fn default_labels() -> Vec<String> {
    let mut l = vec![std::env::consts::OS.to_string(), std::env::consts::ARCH.to_string()];
    let docker = Command::new("docker")
        .args(["version", "--format", "{{.Server.Version}}"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;
    if docker.is_ok_and(|s| s.success()) {
        l.push("docker".into());
    }
    l
}

pub struct Runner {
    pub backend: Arc<Backend>,
    pub labels: Vec<String>,
    /// Workspaces go here, one folder per running job.
    pub work: PathBuf,
}

/// Claim and run jobs forever, up to `slots()` at once (re-read each time, so
/// the built-in runner follows its setting). `wake` is notified when jobs are queued.
pub async fn serve(r: Runner, slots: impl Fn() -> usize, wake: Option<Arc<Notify>>) {
    let r = Arc::new(r);
    let running = Arc::new(AtomicUsize::new(0));
    let done = Arc::new(Notify::new());
    loop {
        if SHUTDOWN.load(Ordering::SeqCst) {
            return;
        }
        while running.load(Ordering::SeqCst) < slots() && !SHUTDOWN.load(Ordering::SeqCst) {
            match r.backend.claim(&r.labels).await {
                Ok(Some(lease)) => {
                    running.fetch_add(1, Ordering::SeqCst);
                    RUNNING.fetch_add(1, Ordering::SeqCst);
                    let (r, running, done) = (r.clone(), running.clone(), done.clone());
                    tokio::spawn(async move {
                        run_job(&r, lease).await;
                        running.fetch_sub(1, Ordering::SeqCst);
                        RUNNING.fetch_sub(1, Ordering::SeqCst);
                        done.notify_one();
                    });
                }
                Ok(None) => break,
                Err(e) => {
                    eprintln!("runner: can't get jobs: {e}");
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    break;
                }
            }
        }
        // A remote claim already waits on the server; locally, wait for news.
        let pause = if wake.is_some() || running.load(Ordering::SeqCst) >= slots() { 3 } else { 0 };
        let pause = if SHUTDOWN.load(Ordering::SeqCst) { 0 } else { pause };
        let notified = async {
            match &wake {
                Some(w) => w.notified().await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            _ = notified => {}
            _ = done.notified() => {}
            _ = tokio::time::sleep(Duration::from_secs(pause)) => {}
        }
    }
}

/// The job must stop: cancelled, moved to another runner, or the process is shutting down.
fn halted(stop: &AtomicBool) -> bool {
    stop.load(Ordering::SeqCst) || SHUTDOWN.load(Ordering::SeqCst)
}

/// What the log pump does, in order.
enum Msg {
    Out(Vec<u8>),
    Step(usize, &'static str),
    Note(String),
}

/// Send output on in order, batched, with every secret value replaced by ***.
/// The values a job's output never shows: each secret whole, and each line of
/// multi-line ones (output arrives line by line), longest first so a value
/// containing another is masked whole.
fn secret_values(l: &Lease) -> Vec<String> {
    let mut secrets: Vec<String> = l.secrets.iter().flat_map(|s| std::iter::once(s.1.clone()).chain(s.1.lines().map(String::from))).filter(|v| v.trim().len() >= 3).collect();
    secrets.sort_by_key(|v| std::cmp::Reverse(v.len()));
    secrets.dedup();
    secrets
}

async fn pump(b: Arc<Backend>, l: Lease, mut rx: mpsc::Receiver<Msg>) {
    let secrets = secret_values(&l);
    let mask = |data: &[u8]| -> Vec<u8> {
        let mut s = String::from_utf8_lossy(data).into_owned();
        for v in &secrets {
            s = s.replace(v.as_str(), "***");
        }
        s.into_bytes()
    };
    let (mut buf, mut flushed): (Vec<u8>, Instant) = (Vec::new(), Instant::now());
    loop {
        let msg = tokio::time::timeout(FLUSH, rx.recv()).await;
        // Only whole lines go out (so a secret is never split across sends), unless a line is huge.
        let flush_to = |buf: &Vec<u8>, all: bool| {
            if all || buf.len() > 64 * 1024 {
                buf.len()
            } else {
                buf.iter().rposition(|c| *c == b'\n' || *c == b'\r').map_or(0, |i| i + 1)
            }
        };
        let all = !matches!(msg, Ok(Some(Msg::Out(_))) | Err(_));
        if let Ok(Some(Msg::Out(d))) = &msg {
            buf.extend_from_slice(d);
            // Steady output still goes out every FLUSH, not only once 16 KB piles up.
            if buf.len() < 16 * 1024 && flushed.elapsed() < FLUSH {
                continue;
            }
        }
        let n = flush_to(&buf, all);
        if n > 0 {
            let out: Vec<u8> = buf.drain(..n).collect();
            b.log(&l, mask(&out)).await;
            flushed = Instant::now();
        }
        match msg {
            Ok(Some(Msg::Step(i, s))) => b.step(&l, i, s).await,
            Ok(Some(Msg::Note(t))) => b.log(&l, mask(t.as_bytes())).await,
            Ok(None) => return,
            _ => {}
        }
    }
}

async fn note(tx: &mpsc::Sender<Msg>, color: &str, text: &str) {
    let _ = tx.send(Msg::Note(format!("\x1b[{color}m{text}\x1b[0m\n"))).await;
}

/// Why a command stopped early.
enum Stopped {
    Cancelled,
    TimedOut,
}

/// Run `cmd` (in its own process group) to the end, streaming its output, unless
/// the job is stopped or out of time. `container` is killed too when set.
/// Process groups a job started: killed when the job ends, so a step's
/// background process (`server &`) can't outlive it.
type Groups = std::sync::Mutex<Vec<u32>>;

async fn run(mut cmd: Command, tx: &mpsc::Sender<Msg>, stop: &AtomicBool, deadline: Instant, container: Option<&str>, groups: &Groups) -> Result<i32, Stopped> {
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).process_group(0).kill_on_drop(true);
    let mut child = match cmd.spawn() {
        Ok(c) => {
            if let Some(pid) = c.id() {
                groups.lock().unwrap_or_else(|p| p.into_inner()).push(pid);
            }
            c
        }
        Err(e) => {
            note(tx, "31", &format!("couldn't start: {e}")).await;
            return Ok(127);
        }
    };
    let readers: Vec<_> = [child.stdout.take().map(|o| Box::new(o) as Box<dyn tokio::io::AsyncRead + Unpin + Send>), child.stderr.take().map(|e| Box::new(e) as _)]
        .into_iter()
        .flatten()
        .map(|mut r| {
            let tx = tx.clone();
            tokio::spawn(async move {
                let mut chunk = vec![0u8; 16 * 1024];
                while let Ok(n) = r.read(&mut chunk).await {
                    if n == 0 || tx.send(Msg::Out(chunk[..n].to_vec())).await.is_err() {
                        break;
                    }
                }
            })
        })
        .collect();
    let outcome = loop {
        tokio::select! {
            s = child.wait() => break Ok(s.ok().and_then(|s| s.code()).unwrap_or(-1)),
            _ = tokio::time::sleep(Duration::from_millis(250)) => {
                if stop.load(Ordering::SeqCst) || SHUTDOWN.load(Ordering::SeqCst) {
                    break Err(Stopped::Cancelled);
                }
                if Instant::now() > deadline {
                    break Err(Stopped::TimedOut);
                }
            }
        }
    };
    if outcome.is_err() {
        if let Some(pid) = child.id() {
            let _ = Command::new("kill").args(["-KILL", &format!("-{pid}")]).status().await;
        }
        if let Some(name) = container {
            let _ = Command::new("docker").args(["rm", "-f", name]).stdout(Stdio::null()).stderr(Stdio::null()).status().await;
        }
        let _ = child.kill().await;
    }
    // A background process may still hold the pipes: read what's there, then let go.
    for mut r in readers {
        if tokio::time::timeout(Duration::from_secs(2), &mut r).await.is_err() {
            r.abort();
        }
    }
    outcome
}

async fn kill_groups(groups: &Groups) {
    let pids = std::mem::take(&mut *groups.lock().unwrap_or_else(|p| p.into_inner()));
    for pid in pids {
        let _ = Command::new("kill").args(["-KILL", &format!("-{pid}")]).stdout(Stdio::null()).stderr(Stdio::null()).status().await;
    }
}

/// The environment every step gets: a clean one, plus CI facts, the job's env and secrets.
fn job_env(l: &Lease, ws: &Path, home: &Path) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = ["PATH", "LANG", "TZ", "DOCKER_HOST", "DOCKER_CONFIG", "DOCKER_CERT_PATH", "DOCKER_TLS_VERIFY"]
        .iter()
        .filter_map(|k| std::env::var(k).ok().map(|v| (k.to_string(), v)))
        .collect();
    let branch = l.refname.strip_prefix("refs/heads/").unwrap_or(&l.refname);
    for (k, v) in [
        ("HOME", home.display().to_string()),
        ("CI", "true".into()),
        ("GITTER", "true".into()),
        ("GITTER_REPOSITORY", l.repo.clone()),
        ("GITTER_SHA", l.sha.clone()),
        ("GITTER_REF", l.refname.clone()),
        ("GITTER_BRANCH", branch.to_string()),
        ("GITTER_EVENT", l.event.clone()),
        ("GITTER_RUN_NUMBER", l.run_number.to_string()),
        ("GITTER_JOB", l.job.name.clone()),
        ("GITTER_WORKSPACE", ws.display().to_string()),
    ] {
        env.push((k.into(), v));
    }
    env.extend(l.job.env.iter().map(|(k, v)| (k.clone(), v.clone())));
    env.extend(l.secrets.iter().cloned());
    env
}

async fn run_job(r: &Runner, l: Lease) {
    let b = r.backend.clone();
    let (ws, home) = (r.work.join(format!("job-{}", l.job_id)), r.work.join(format!("job-{}-home", l.job_id)));
    for d in [&ws, &home] {
        let _ = tokio::fs::remove_dir_all(d).await;
        let _ = tokio::fs::create_dir_all(d).await;
    }
    let (tx, rx) = mpsc::channel::<Msg>(256);
    let logger = tokio::spawn(pump(b.clone(), l.clone(), rx));
    let stop = Arc::new(AtomicBool::new(false));
    let beat = {
        let (b, l, stop) = (b.clone(), l.clone(), stop.clone());
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(HEARTBEAT).await;
                if b.heartbeat(&l).await == Some(false) {
                    stop.store(true, Ordering::SeqCst);
                    return;
                }
            }
        })
    };
    let deadline = Instant::now() + Duration::from_secs(l.job.timeout_secs.max(1));
    let groups = Groups::default();
    let (success, code) = match checkout(b.source(&l), &l, &ws, &home, &tx, &stop, deadline, &groups).await {
        Err(failed) => failed,
        Ok(()) => {
            let cache = restore_cache(&b, &l, &ws, &r.work, &tx).await;
            let (success, code) = steps(None, &l, &ws, &home, &tx, &stop, deadline, &groups).await;
            kill_groups(&groups).await;
            if !halted(&stop) && !(l.job.reports.is_empty() && l.job.coverage.is_empty()) {
                send_reports(&b, &l, &ws, &r.work, &tx).await;
            }
            if success && !l.job.artifacts.is_empty() && !halted(&stop) {
                pack_artifacts(&b, &l, &ws, &r.work, &tx).await;
            }
            if let Some((key, false)) = cache.filter(|_| success && !halted(&stop)) {
                save_cache(&b, &l, &ws, &r.work, &tx, &key).await;
            }
            (success, code)
        }
    };
    kill_groups(&groups).await;
    match (success, halted(&stop)) {
        (_, true) => note(&tx, "33", "The job was stopped.").await,
        (true, _) => note(&tx, "32", "✓ Job succeeded").await,
        (false, _) => note(&tx, "31", &format!("✗ Job failed (exit code {})", code.unwrap_or(-1))).await,
    }
    drop(tx);
    // The pump ends once every sender is gone; don't let a stuck one hold the job.
    let _ = tokio::time::timeout(Duration::from_secs(30), logger).await;
    beat.abort();
    if !halted(&stop) {
        b.finish(&l, success, code).await;
    }
    for d in [&ws, &home] {
        let _ = tokio::fs::remove_dir_all(d).await;
    }
}

/// Check out the commit from `source` (`Backend::source`; None: the workspace
/// is ready), then run the steps until one fails.
#[allow(clippy::too_many_arguments)]
async fn steps(source: Option<(String, &'static str, String)>, l: &Lease, ws: &Path, home: &Path, tx: &mpsc::Sender<Msg>, stop: &AtomicBool, deadline: Instant, groups: &Groups) -> (bool, Option<i32>) {
    if let Some(source) = source
        && let Err(failed) = checkout(source, l, ws, home, tx, stop, deadline, groups).await
    {
        return failed;
    }
    run_steps(l, ws, home, tx, stop, deadline, groups).await
}

/// Fetch the commit into the empty workspace. Err: the job's result (its steps marked skipped).
#[allow(clippy::too_many_arguments)]
async fn checkout(source: (String, &'static str, String), l: &Lease, ws: &Path, home: &Path, tx: &mpsc::Sender<Msg>, stop: &AtomicBool, deadline: Instant, groups: &Groups) -> Result<(), (bool, Option<i32>)> {
    let (src, config_key, config_value) = source;
    {
        note(tx, "90", &format!("Checking out {} at {}", l.repo, &l.sha[..l.sha.len().min(12)])).await;
        let git = |args: &[&str]| {
            let mut c = Command::new("git");
            c.args(args).current_dir(ws).env_clear();
            for k in ["PATH", "LANG"] {
                if let Ok(v) = std::env::var(k) {
                    c.env(k, v);
                }
            }
            c.env("HOME", home)
                .env("GIT_TERMINAL_PROMPT", "0")
                .env("GIT_LFS_SKIP_SMUDGE", "1")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_COUNT", "1")
                .env("GIT_CONFIG_KEY_0", config_key)
                .env("GIT_CONFIG_VALUE_0", &config_value);
            c
        };
        for args in [
            vec!["init", "-q"],
            vec!["fetch", "-q", "--no-tags", "--depth=1", &src, &l.sha],
            vec!["checkout", "-q", "--detach", "FETCH_HEAD"],
        ] {
            match run(git(&args), tx, stop, deadline, None, groups).await {
                Ok(0) => {}
                Ok(c) => {
                    note(tx, "31", &format!("git {} failed", args[0])).await;
                    for i in 0..l.job.steps.len() {
                        let _ = tx.send(Msg::Step(i, "skipped")).await;
                    }
                    return Err((false, Some(c)));
                }
                Err(Stopped::TimedOut) => {
                    note(tx, "31", "The job ran out of time while checking out.").await;
                    return Err((false, Some(-1)));
                }
                Err(Stopped::Cancelled) => return Err((false, None)),
            }
        }
    }
    Ok(())
}

/// Run the steps in the ready workspace until one fails.
async fn run_steps(l: &Lease, ws: &Path, home: &Path, tx: &mpsc::Sender<Msg>, stop: &AtomicBool, deadline: Instant, groups: &Groups) -> (bool, Option<i32>) {
    let env = job_env(l, ws, home);
    let docker_ok = l.job.image.is_none() || Command::new("docker").arg("version").stdout(Stdio::null()).stderr(Stdio::null()).status().await.is_ok_and(|s| s.success());
    if l.job.image.is_none() && !(l.limits.network && l.job.network) {
        note(tx, "33", "This job names no image, so it runs as a plain process on the runner: turning the network off only applies to containers.").await;
    }
    let mut failed: Option<i32> = None;
    for (i, step) in l.job.steps.iter().enumerate() {
        if failed.is_some() || halted(stop) {
            let _ = tx.send(Msg::Step(i, "skipped")).await;
            continue;
        }
        let _ = tx.send(Msg::Step(i, "running")).await;
        note(tx, "1", &format!("▶ {}", step.name)).await;
        // Unique per process too: two runners (or servers) on one docker host have overlapping job ids.
        let container = format!("gitter-job-{}-{}-{i}-{:08x}", l.job_id, l.attempt, rand::random::<u32>());
        let cmd = match &l.job.image {
            None => {
                let mut c = Command::new("sh");
                c.args(["-e", "-c", &step.run]).current_dir(ws).env_clear().envs(env.iter().cloned());
                c
            }
            Some(_) if !docker_ok => {
                note(tx, "31", "This job names an image, but this runner can't use docker.").await;
                failed = Some(127);
                let _ = tx.send(Msg::Step(i, "failure")).await;
                continue;
            }
            Some(image) => {
                let mut c = Command::new("docker");
                c.args(["run", "--rm", "--init", "--name", &container, "-v", &format!("{}:/workspace", ws.display()), "-w", "/workspace"]);
                c.args(sandbox(l));
                // Values go through the environment, never the command line.
                for (k, _) in env.iter().filter(|(k, _)| !matches!(k.as_str(), "PATH" | "HOME" | "LANG" | "TZ") && !k.starts_with("DOCKER_")) {
                    c.args(["-e", k]);
                }
                c.args(["-e", "GITTER_WORKSPACE=/workspace", "-e", "HOME=/tmp"]);
                c.args([image.as_str(), "sh", "-e", "-c", &step.run]).current_dir(ws).env_clear().envs(env.iter().cloned());
                c
            }
        };
        let res = run(cmd, tx, stop, deadline, l.job.image.as_ref().map(|_| container.as_str()), groups).await;
        let status = match res {
            Ok(0) => "success",
            Ok(c) => {
                failed = Some(c);
                "failure"
            }
            Err(Stopped::TimedOut) => {
                let t = l.job.timeout_secs;
                let limit = if t < 120 { format!("{t} seconds") } else { format!("{} minutes", t / 60) };
                note(tx, "31", &format!("The job ran out of time ({limit}).")).await;
                failed = Some(-1);
                "failure"
            }
            Err(Stopped::Cancelled) => "failure",
        };
        let _ = tx.send(Msg::Step(i, status)).await;
    }
    if let Some(image) = &l.job.image {
        // Containers run as root: hand the workspace back so it can be cleaned up.
        use std::os::unix::fs::MetadataExt;
        if let Ok(m) = std::fs::metadata(ws) {
            let owner = format!("{}:{}", m.uid(), m.gid());
            let mount = format!("{}:/workspace", ws.display());
            let _ = Command::new("docker")
                .args(["run", "--rm", "--network", "none", "-v", &mount, "--entrypoint", "chown", image, "-R", &owner, "/workspace"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .await;
        }
    }
    (failed.is_none() && !halted(stop), Some(failed.unwrap_or(0)))
}

/// `docker run` flags that keep a job inside its container: no new privileges
/// (setuid binaries can't escalate), the capabilities a build never needs
/// dropped, a process limit, the site's CPU and memory limits, and no network
/// when the site or the job turns it off. Nothing from the host is mounted
/// but the workspace (never the docker socket).
fn sandbox(l: &Lease) -> Vec<String> {
    let mut a: Vec<String> = ["--security-opt", "no-new-privileges", "--pids-limit", "4096"].map(String::from).into();
    for cap in ["NET_RAW", "MKNOD", "SYS_CHROOT", "SETFCAP", "AUDIT_WRITE"] {
        a.extend(["--cap-drop".into(), cap.into()]);
    }
    if l.limits.cpus > 0 {
        a.extend(["--cpus".into(), l.limits.cpus.to_string()]);
    }
    if l.limits.memory_mb > 0 {
        let m = format!("{}m", l.limits.memory_mb);
        a.extend(["--memory".into(), m.clone(), "--memory-swap".into(), m]);
    }
    if !(l.limits.network && l.job.network) {
        a.extend(["--network".into(), "none".into()]);
    }
    a
}

/// Pack the files matching the job's `artifacts` globs into one .tar.gz and send it.
async fn pack_artifacts(b: &Backend, l: &Lease, ws: &Path, work: &Path, tx: &mpsc::Sender<Msg>) {
    let out = work.join(format!("job-{}.tar.gz", l.job_id));
    let (ws2, out2, globs) = (ws.to_path_buf(), out.clone(), l.job.artifacts.clone());
    let packed = tokio::task::spawn_blocking(move || {
        let files = walk(&ws2, |rel| any_glob(&globs, &[rel.to_string()]), usize::MAX)?;
        pack(files, &out2, super::jobs::ARTIFACT_CAP, &[])
    })
    .await
    .map_err(std::io::Error::other)
    .and_then(|r| r);
    match packed {
        Ok((0, _)) => note(tx, "33", "No files matched the artifacts patterns.").await,
        Ok((n, cut)) => {
            if cut {
                note(tx, "33", "Artifacts are limited to 1 GB; the rest were left out.").await;
            }
            match b.artifact(l, &out).await {
                Ok(()) => note(tx, "90", &format!("Saved {n} file(s) as artifacts.")).await,
                Err(e) => note(tx, "31", &format!("Couldn't save the artifacts: {e}")).await,
            }
        }
        Err(e) => note(tx, "31", &format!("Couldn't pack the artifacts: {e}")).await,
    }
    let _ = tokio::fs::remove_file(&out).await;
}

/// Write `files` (path, name in the archive) to a .tar.gz at `out`, stopping
/// before their sizes pass `cap`. (files written, whether some were left out)
/// Pack `files` into a .tar.gz at `out`, up to `cap` bytes. With `mask`, each
/// file's text has those values replaced by *** (reports are shown like logs).
fn pack(files: Vec<(PathBuf, String)>, out: &Path, cap: u64, mask: &[String]) -> std::io::Result<(usize, bool)> {
    let f = std::fs::File::create(out)?;
    let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(f, flate2::Compression::fast()));
    tar.follow_symlinks(false);
    let (mut total, mut cut, mut n) = (0u64, false, 0);
    for (p, name) in files {
        let meta = std::fs::symlink_metadata(&p)?;
        total += meta.len();
        if total > cap {
            cut = true;
            break;
        }
        if mask.is_empty() || !meta.is_file() {
            tar.append_path_with_name(&p, name)?;
        } else {
            let mut text = String::from_utf8_lossy(&std::fs::read(&p)?).into_owned();
            for v in mask {
                text = text.replace(v.as_str(), "***");
            }
            let mut h = tar::Header::new_gnu();
            h.set_size(text.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            tar.append_data(&mut h, name, text.as_bytes())?;
        }
        n += 1;
    }
    tar.into_inner()?.finish()?;
    Ok((n, cut))
}

/// Send the files the job's `reports` and `coverage` patterns match, whether
/// or not the steps passed (failed tests are the point).
async fn send_reports(b: &Backend, l: &Lease, ws: &Path, work: &Path, tx: &mpsc::Sender<Msg>) {
    let out = work.join(format!("job-{}-reports.tar.gz", l.job_id));
    let (ws2, out2, junit, cov, secrets) = (ws.to_path_buf(), out.clone(), l.job.reports.clone(), l.job.coverage.clone(), secret_values(l));
    let packed = tokio::task::spawn_blocking(move || {
        let matches = |globs: &[String], rel: &str| globs.iter().any(|g| glob(g, rel));
        let files = walk(&ws2, |rel| matches(&junit, rel) || matches(&cov, rel), 1_000_000)?;
        let named = files.into_iter().take(100).map(|(p, rel)| {
            let kind = if matches(&junit, &rel) { "junit" } else { "coverage" };
            (p, format!("{kind}/{rel}"))
        });
        pack(named.collect(), &out2, super::reports::UPLOAD_CAP, &secrets)
    })
    .await
    .map_err(std::io::Error::other)
    .and_then(|r| r);
    match packed {
        Ok((0, _)) => note(tx, "33", "No files matched the reports or coverage patterns.").await,
        Ok((n, _)) => match b.reports(l, &out).await {
            Ok(()) => note(tx, "90", &format!("Sent {n} report file(s).")).await,
            Err(e) => note(tx, "31", &format!("Couldn't send the reports: {e}")).await,
        },
        Err(e) => note(tx, "31", &format!("Couldn't pack the reports: {e}")).await,
    }
    let _ = tokio::fs::remove_file(&out).await;
}

/// Is workspace path `rel` one of the cache's `paths` (a file, or inside a folder)?
fn in_cache(paths: &[String], rel: &str) -> bool {
    paths.iter().any(|p| rel == p || rel.strip_prefix(p.as_str()).is_some_and(|r| r.starts_with('/')))
}

/// Restore the job's cache into the fresh checkout. Returns its expanded key and
/// whether that exact key was restored (then there's nothing to save).
async fn restore_cache(b: &Backend, l: &Lease, ws: &Path, work: &Path, tx: &mpsc::Sender<Msg>) -> Option<(String, bool)> {
    let c = l.job.cache.clone()?;
    let ws2 = ws.to_path_buf();
    let (key, restore) = tokio::task::spawn_blocking(move || (cache::expand_key(&c.key, &ws2), c.restore_keys.iter().map(|r| cache::expand_key(r, &ws2)).collect::<Vec<_>>()))
        .await
        .ok()?;
    let mut exact = false;
    match b.cache_get(l, &key, &restore, work).await {
        Ok(None) => note(tx, "90", &format!("No cache for {key} yet.")).await,
        Ok(Some((hit, file, _))) => {
            let (ws2, file2, paths) = (ws.to_path_buf(), file.clone(), l.job.cache.as_ref().map(|c| c.paths.clone()).unwrap_or_default());
            let res = tokio::task::spawn_blocking(move || unpack_cache(&file2, &ws2, &paths)).await.map_err(std::io::Error::other).and_then(|r| r);
            match res {
                Ok(n) => {
                    exact = hit == key;
                    note(tx, "90", &format!("Restored {n} file(s) from the cache {hit}.")).await;
                }
                Err(e) => note(tx, "33", &format!("Couldn't restore the cache {hit}: {e}")).await,
            }
        }
        Err(e) => note(tx, "33", &format!("Couldn't get the cache: {e}")).await,
    }
    // A download (remote runners); the built-in runner reads the stored file in place.
    let _ = tokio::fs::remove_file(work.join(format!("job-{}-cache.tar.gz", l.job_id))).await;
    Some((key, exact))
}

/// Unpack a cache archive into the workspace: only files and folders under the
/// cache's paths, never outside the workspace (tar's checks: no `..`, no
/// absolute paths, no writing through symlinks), at most ENTRY_CAP bytes.
fn unpack_cache(file: &Path, ws: &Path, paths: &[String]) -> std::io::Result<usize> {
    use std::io::Read;
    let f = std::fs::File::open(file)?;
    // The whole stream is capped too: tar reads long-name and PAX headers into memory.
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(f).take(cache::ENTRY_CAP + (64 << 20)));
    tar.set_preserve_permissions(false);
    let (mut n, mut total) = (0, 0u64);
    for e in tar.entries()? {
        let mut e = e?;
        // Every entry counts: moving past one decompresses it too.
        total = total.saturating_add(e.header().size()?);
        if total > cache::ENTRY_CAP {
            return Err(std::io::Error::other("the cache is over 1 GB"));
        }
        let kind = e.header().entry_type();
        if !matches!(kind, tar::EntryType::Regular | tar::EntryType::Directory) {
            continue;
        }
        let rel = e.path()?.to_string_lossy().trim_end_matches('/').to_string();
        if !in_cache(paths, &rel) && !paths.iter().any(|p| p.starts_with(&format!("{rel}/"))) {
            continue;
        }
        if e.unpack_in(ws)? && kind == tar::EntryType::Regular {
            n += 1;
        }
    }
    Ok(n)
}

/// Save the cache's paths under `key` after a successful job (all or nothing).
async fn save_cache(b: &Backend, l: &Lease, ws: &Path, work: &Path, tx: &mpsc::Sender<Msg>, key: &str) {
    let Some(c) = &l.job.cache else { return };
    let out = work.join(format!("job-{}-cache-save.tar.gz", l.job_id));
    let (ws2, out2, paths) = (ws.to_path_buf(), out.clone(), c.paths.clone());
    let packed = tokio::task::spawn_blocking(move || {
        let files = walk(&ws2, |rel| in_cache(&paths, rel), 1_000_000)?;
        pack(files, &out2, cache::ENTRY_CAP, &[])
    })
    .await
    .map_err(std::io::Error::other)
    .and_then(|r| r);
    match packed {
        Ok((0, _)) => note(tx, "33", &format!("Nothing to cache: no files in {}.", c.paths.join(", "))).await,
        Ok((_, true)) => note(tx, "33", "The cache is over 1 GB, so it wasn't saved.").await,
        Ok((n, false)) => match b.cache_put(l, key, &out).await {
            Ok(()) => note(tx, "90", &format!("Saved {n} file(s) as the cache {key}.")).await,
            Err(e) => note(tx, "33", &format!("Couldn't save the cache: {e}")).await,
        },
        Err(e) => note(tx, "33", &format!("Couldn't pack the cache: {e}")).await,
    }
    let _ = tokio::fs::remove_file(&out).await;
}

/// `gitter ci run`: one job's steps in `ws` (already holding the files), with
/// the output on stdout. Stops when `SHUTDOWN` is set. (success, exit code)
pub async fn run_here(l: &Lease, ws: &Path, home: &Path) -> (bool, Option<i32>) {
    let (tx, mut rx) = mpsc::channel::<Msg>(256);
    let printer = tokio::spawn(async move {
        use std::io::Write;
        while let Some(m) = rx.recv().await {
            let mut out = std::io::stdout().lock();
            let _ = match m {
                Msg::Out(d) => out.write_all(&d),
                Msg::Note(t) => out.write_all(t.as_bytes()),
                Msg::Step(..) => Ok(()),
            };
            let _ = out.flush();
        }
    });
    let groups = Groups::default();
    let deadline = Instant::now() + Duration::from_secs(l.job.timeout_secs.max(1));
    let res = steps(None, l, ws, home, &tx, &AtomicBool::new(false), deadline, &groups).await;
    kill_groups(&groups).await;
    drop(tx);
    let _ = printer.await;
    res
}

/// Ask every running job to stop and wait (up to `limit`) until they have.
pub async fn shutdown(limit: Duration) {
    SHUTDOWN.store(true, Ordering::SeqCst);
    let until = Instant::now() + limit;
    while RUNNING.load(Ordering::SeqCst) > 0 && Instant::now() < until {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// The runner inside the server: registered once, runs `builtin_runner_jobs` at once.
pub async fn builtin(st: AppState) {
    let id = st
        .db
        .call(|c| {
            let found: Option<i64> = rusqlite::OptionalExtension::optional(c.query_row("SELECT id FROM ci_runners WHERE builtin = 1", [], |r| r.get(0)))?;
            Ok(match found {
                Some(id) => id,
                None => {
                    c.execute("INSERT INTO ci_runners(name, builtin, created_at) VALUES ('built-in', 1, ?1)", [crate::db::now()])?;
                    c.last_insert_rowid()
                }
            })
        })
        .await;
    let id = match id {
        Ok(id) => id,
        Err(e) => return eprintln!("ci: can't register the built-in runner: {}", e.msg),
    };
    // Workspaces left by a crash: nothing runs yet, so all of them are stale.
    let _ = tokio::fs::remove_dir_all(st.cfg.data_dir.join("ci/work")).await;
    let mut labels = default_labels().await;
    labels.push("builtin".into());
    let labels_json = serde_json::to_string(&labels).unwrap_or_default();
    let _ = st.db.call(move |c| Ok(c.execute("UPDATE ci_runners SET labels = ?2 WHERE id = ?1", rusqlite::params![id, labels_json])?)).await;
    let runner = Runner { backend: Arc::new(Backend::Local { st: st.clone(), runner_id: id }), labels, work: st.cfg.data_dir.join("ci/work") };
    let (s, wake) = (st.clone(), st.ci_wake.clone());
    serve(runner, move || s.settings.get().builtin_runner_jobs as usize, Some(wake)).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tar entry with any name (the tar crate refuses to write unsafe ones itself).
    fn raw(b: &mut tar::Builder<flate2::write::GzEncoder<std::fs::File>>, name: &str, kind: tar::EntryType, link: Option<&str>, data: &[u8]) {
        let mut h = tar::Header::new_gnu();
        h.as_gnu_mut().unwrap().name[..name.len()].copy_from_slice(name.as_bytes());
        h.set_entry_type(kind);
        h.set_size(data.len() as u64);
        h.set_mode(0o644);
        if let Some(l) = link {
            h.as_gnu_mut().unwrap().linkname[..l.len()].copy_from_slice(l.as_bytes());
        }
        h.set_cksum();
        b.append(&h, data).unwrap();
    }

    #[test]
    fn a_cache_archive_only_unpacks_plain_files_inside_its_paths() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().join("ws");
        std::fs::create_dir_all(&ws).unwrap();
        let file = dir.path().join("c.tar.gz");
        let mut b = tar::Builder::new(flate2::write::GzEncoder::new(std::fs::File::create(&file).unwrap(), flate2::Compression::fast()));
        raw(&mut b, "keep/../../evil", tar::EntryType::Regular, None, b"x");
        raw(&mut b, "/keep/abs", tar::EntryType::Regular, None, b"x");
        raw(&mut b, "keep/link", tar::EntryType::Symlink, Some("/etc"), b"");
        raw(&mut b, "keep/hard", tar::EntryType::Link, Some("/etc/passwd"), b"");
        raw(&mut b, "other/x", tar::EntryType::Regular, None, b"x");
        raw(&mut b, "keep/ok", tar::EntryType::Regular, None, b"fine");
        b.into_inner().unwrap().finish().unwrap();
        assert_eq!(unpack_cache(&file, &ws, &["keep".to_string()]).unwrap(), 1);
        assert_eq!(std::fs::read_to_string(ws.join("keep/ok")).unwrap(), "fine");
        for gone in [dir.path().join("evil"), ws.join("keep/link"), ws.join("keep/hard"), ws.join("other/x"), ws.join("keep/abs")] {
            assert!(std::fs::symlink_metadata(&gone).is_err(), "{} was written", gone.display());
        }
    }

    #[test]
    fn reports_are_packed_with_secrets_masked() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("r.xml");
        std::fs::write(&src, "<failure message=\"token s3cr3t-value here\"/>").unwrap();
        let out = dir.path().join("r.tar.gz");
        assert_eq!(pack(vec![(src, "junit/r.xml".into())], &out, 1 << 20, &["s3cr3t-value".to_string()]).unwrap(), (1, false));
        let mut a = tar::Archive::new(flate2::read::GzDecoder::new(std::fs::File::open(&out).unwrap()));
        let mut text = String::new();
        std::io::Read::read_to_string(&mut a.entries().unwrap().next().unwrap().unwrap(), &mut text).unwrap();
        assert_eq!(text, "<failure message=\"token *** here\"/>");
    }
}
