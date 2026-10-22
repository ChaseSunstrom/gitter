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

use super::config::any_glob;
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

    async fn artifact(&self, l: &Lease, file: &Path) -> Res<()> {
        match self {
            Backend::Local { st, .. } if !super::jobs::holds(st, l.job_id, &l.token).await => Err("the job moved on".into()),
            Backend::Local { st, .. } => super::jobs::store_artifact(st, l.job_id, file).await.map_err(|e| e.msg),
            Backend::Remote { .. } => {
                let (auth, file) = (format!("Bearer {}", l.token), file.to_path_buf());
                let status = self
                    .http(
                        move |a, url| {
                            let f = std::fs::File::open(&file).map_err(ureq::Error::Io)?;
                            // Up to 1 GB over a slow link: no overall deadline, only per-read/write ones.
                            let req = a.put(url).header("authorization", auth).config().timeout_global(None).timeout_send_body(Some(Duration::from_secs(120))).build();
                            Ok(req.send(f)?.status().as_u16())
                        },
                        format!("jobs/{}/artifacts", l.job_id),
                    )
                    .await?;
                if status == 200 { Ok(()) } else { Err(format!("the server answered {status}")) }
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
async fn pump(b: Arc<Backend>, l: Lease, mut rx: mpsc::Receiver<Msg>) {
    // Whole values, and each line of multi-line ones (output arrives line by line).
    let mut secrets: Vec<String> = l.secrets.iter().flat_map(|s| std::iter::once(s.1.clone()).chain(s.1.lines().map(String::from))).filter(|v| v.trim().len() >= 3).collect();
    // Longest first, so a value containing another is masked whole.
    secrets.sort_by_key(|v| std::cmp::Reverse(v.len()));
    secrets.dedup();
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
    let (success, code) = steps(&b, &l, &ws, &home, &tx, &stop, deadline, &groups).await;
    kill_groups(&groups).await;
    if success && !l.job.artifacts.is_empty() && !halted(&stop) {
        pack_artifacts(&b, &l, &ws, &r.work, &tx).await;
    }
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

/// Check out the commit, then run the steps until one fails.
#[allow(clippy::too_many_arguments)]
async fn steps(b: &Backend, l: &Lease, ws: &Path, home: &Path, tx: &mpsc::Sender<Msg>, stop: &AtomicBool, deadline: Instant, groups: &Groups) -> (bool, Option<i32>) {
    let (src, config_key, config_value) = b.source(l);
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
                return (false, Some(c));
            }
            Err(Stopped::TimedOut) => {
                note(tx, "31", "The job ran out of time while checking out.").await;
                return (false, Some(-1));
            }
            Err(Stopped::Cancelled) => return (false, None),
        }
    }
    let env = job_env(l, ws, home);
    let docker_ok = l.job.image.is_none() || Command::new("docker").arg("version").stdout(Stdio::null()).stderr(Stdio::null()).status().await.is_ok_and(|s| s.success());
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
                .args(["run", "--rm", "-v", &mount, "--entrypoint", "chown", image, "-R", &owner, "/workspace"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .await;
        }
    }
    (failed.is_none() && !halted(stop), Some(failed.unwrap_or(0)))
}

/// Pack the files matching the job's `artifacts` globs into one .tar.gz and send it.
async fn pack_artifacts(b: &Backend, l: &Lease, ws: &Path, work: &Path, tx: &mpsc::Sender<Msg>) {
    let out = work.join(format!("job-{}.tar.gz", l.job_id));
    let (ws2, out2, globs) = (ws.to_path_buf(), out.clone(), l.job.artifacts.clone());
    let packed = tokio::task::spawn_blocking(move || -> std::io::Result<(usize, bool)> {
        let mut files = Vec::new();
        let mut stack = vec![ws2.clone()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d)?.flatten() {
                let p = e.path();
                let ft = e.file_type()?;
                if ft.is_dir() && e.file_name() != ".git" {
                    stack.push(p);
                } else if ft.is_file()
                    && let Ok(rel) = p.strip_prefix(&ws2)
                {
                    let rel = rel.to_string_lossy().into_owned();
                    if any_glob(&globs, std::slice::from_ref(&rel)) {
                        files.push((p, rel));
                    }
                }
            }
        }
        files.sort_by(|a, b| a.1.cmp(&b.1));
        let f = std::fs::File::create(&out2)?;
        let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(f, flate2::Compression::default()));
        let (mut total, mut cut, mut n) = (0u64, false, 0);
        for (p, rel) in files {
            total += std::fs::metadata(&p)?.len();
            if total > super::jobs::ARTIFACT_CAP {
                cut = true;
                break;
            }
            tar.append_path_with_name(&p, rel)?;
            n += 1;
        }
        tar.into_inner()?.finish()?;
        Ok((n, cut))
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
