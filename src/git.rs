//! Runs the git CLI safely: argument vectors only, a clean environment, no
//! hooks, a timeout and a global concurrency cap.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::Semaphore;

use crate::error::{AppError, R};

pub const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
pub const DIFF_CAP: usize = 8 * 1024 * 1024;

#[derive(Clone)]
pub struct Git {
    pub root: PathBuf,
    /// Short git calls behind API requests.
    sem: Arc<Semaphore>,
    /// Long transfers (clone, fetch, push, archives), kept apart so slow clients can't starve the UI.
    stream_sem: Arc<Semaphore>,
    home: PathBuf,
}

pub struct Out {
    pub code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl Out {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }
}

/// Extra environment for a git call (author identity, temp index...).
pub type Env<'a> = &'a [(&'a str, &'a str)];

impl Git {
    pub fn new(root: PathBuf, home: PathBuf, max_procs: usize) -> Git {
        Git {
            root,
            sem: Arc::new(Semaphore::new(max_procs)),
            stream_sem: Arc::new(Semaphore::new(max_procs / 2)),
            home,
        }
    }

    pub fn repo_path(&self, id: i64) -> PathBuf {
        self.root.join(format!("{id}.git"))
    }

    /// A `git` command with a scrubbed environment. Callers add args.
    pub fn command(&self, repo: Option<&Path>) -> Command {
        let mut c = Command::new("git");
        c.env_clear()
            .env(
                "PATH",
                std::env::var_os("PATH").unwrap_or_else(|| "/usr/bin:/bin".into()),
            )
            .env("HOME", &self.home)
            .env("LANG", "C.UTF-8")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_NO_REPLACE_OBJECTS", "1")
            .args([
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "core.quotePath=false",
                "-c",
                "protocol.file.allow=user",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some(r) = repo {
            c.arg(format!("--git-dir={}", r.display()));
        }
        c
    }

    pub async fn permit(&self) -> R<tokio::sync::OwnedSemaphorePermit> {
        Self::acquire(&self.sem, 15).await
    }

    pub async fn permit_stream(&self) -> R<tokio::sync::OwnedSemaphorePermit> {
        Self::acquire(&self.stream_sem, 10).await
    }

    async fn acquire(sem: &Arc<Semaphore>, secs: u64) -> R<tokio::sync::OwnedSemaphorePermit> {
        match tokio::time::timeout(Duration::from_secs(secs), sem.clone().acquire_owned()).await {
            Ok(Ok(p)) => Ok(p),
            Ok(Err(e)) => Err(AppError::internal(e)),
            Err(_) => Err(AppError::new(
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                "The server is busy. Try again in a moment.",
            )),
        }
    }

    /// Run git and collect its output. Never fails on a non-zero exit; check `code`.
    pub async fn run(
        &self,
        repo: &Path,
        args: &[&str],
        stdin: Option<&[u8]>,
        env: Env<'_>,
    ) -> R<Out> {
        self.run_timeout(repo, args, stdin, env, Duration::from_secs(30))
            .await
    }

    pub async fn run_timeout(
        &self,
        repo: &Path,
        args: &[&str],
        stdin: Option<&[u8]>,
        env: Env<'_>,
        timeout: Duration,
    ) -> R<Out> {
        let _permit = self.permit().await?;
        let mut cmd = self.command(Some(repo));
        cmd.args(args);
        for (k, v) in env {
            cmd.env(k, v);
        }
        if stdin.is_some() {
            cmd.stdin(Stdio::piped());
        }
        let mut child = cmd.spawn().map_err(AppError::internal)?;
        if let Some(input) = stdin {
            let mut si = child.stdin.take().expect("piped stdin");
            let data = input.to_vec();
            tokio::spawn(async move {
                let _ = si.write_all(&data).await;
            });
        }
        let out = tokio::time::timeout(timeout, child.wait_with_output())
            .await
            .map_err(|_| {
                AppError::new(axum::http::StatusCode::GATEWAY_TIMEOUT, "git took too long")
            })?
            .map_err(AppError::internal)?;
        Ok(Out {
            code: out.status.code().unwrap_or(-1),
            stdout: out.stdout,
            stderr: out.stderr,
        })
    }

    /// Run git and return stdout, turning a non-zero exit into an error.
    pub async fn ok(&self, repo: &Path, args: &[&str]) -> R<Vec<u8>> {
        let o = self.run(repo, args, None, &[]).await?;
        if o.code != 0 {
            return Err(AppError::internal(format!(
                "git {:?}: {}",
                args,
                String::from_utf8_lossy(&o.stderr).trim()
            )));
        }
        Ok(o.stdout)
    }

    pub async fn ok_env(
        &self,
        repo: &Path,
        args: &[&str],
        stdin: Option<&[u8]>,
        env: Env<'_>,
    ) -> R<String> {
        let o = self.run(repo, args, stdin, env).await?;
        if o.code != 0 {
            return Err(AppError::internal(format!(
                "git {:?}: {}",
                args,
                String::from_utf8_lossy(&o.stderr).trim()
            )));
        }
        Ok(o.text().trim().to_string())
    }

    // ---------- repository lifecycle ----------

    pub async fn init_bare(&self, path: &Path, branch: &str, max_push: u64) -> R<()> {
        if tokio::fs::try_exists(path).await.unwrap_or(true) {
            return Err(AppError::internal(format!(
                "{} already exists",
                path.display()
            )));
        }
        let _permit = self.permit().await?;
        let out = self
            .command(None)
            .args(["init", "--bare", "-q", "-b", branch])
            .arg(path)
            .output()
            .await
            .map_err(AppError::internal)?;
        if !out.status.success() {
            return Err(AppError::internal(String::from_utf8_lossy(&out.stderr)));
        }
        drop(_permit);
        self.configure(path, max_push).await
    }

    pub async fn configure(&self, path: &Path, max_push: u64) -> R<()> {
        for (k, v) in [
            ("core.hooksPath", "/dev/null"),
            ("receive.fsckObjects", "true"),
            ("receive.maxInputSize", &max_push.to_string()),
            ("uploadpack.allowFilter", "true"),
            ("uploadpack.allowAnySHA1InWant", "false"),
            ("gc.auto", "6700"),
        ] {
            self.ok(path, &["config", k, v]).await?;
        }
        Ok(())
    }

    pub async fn clone_bare(&self, src: &Path, dst: &Path, max_push: u64) -> R<()> {
        let _permit = self.permit().await?;
        let out = self
            .command(None)
            .args(["clone", "--bare", "-q", "--no-hardlinks", "--"])
            .arg(src)
            .arg(dst)
            .output()
            .await
            .map_err(AppError::internal)?;
        if !out.status.success() {
            return Err(AppError::internal(String::from_utf8_lossy(&out.stderr)));
        }
        drop(_permit);
        self.configure(dst, max_push).await
    }

    // ---------- reading ----------

    /// Resolve a user revision to a commit sha. `rev` must already be validated.
    pub async fn resolve(&self, repo: &Path, rev: &str) -> R<Option<String>> {
        let spec = format!("{rev}^{{commit}}");
        let o = self
            .run(
                repo,
                &[
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    "--end-of-options",
                    &spec,
                ],
                None,
                &[],
            )
            .await?;
        Ok((o.code == 0)
            .then(|| o.text().trim().to_string())
            .filter(|s| !s.is_empty()))
    }

    pub async fn head_branch(&self, repo: &Path) -> R<Option<String>> {
        let o = self
            .run(repo, &["symbolic-ref", "--short", "-q", "HEAD"], None, &[])
            .await?;
        Ok((o.code == 0).then(|| o.text().trim().to_string()))
    }

    pub async fn set_head(&self, repo: &Path, branch: &str) -> R<()> {
        self.ok(
            repo,
            &["symbolic-ref", "HEAD", &format!("refs/heads/{branch}")],
        )
        .await?;
        Ok(())
    }

    pub async fn refs(&self, repo: &Path) -> R<Vec<Ref>> {
        let fmt = "%(refname)%00%(objectname)%00%(*objectname)%00%(creatordate:unix)%00%(contents:subject)%00%(authorname)%00%(*authorname)";
        let args = [
            "for-each-ref",
            "--sort=-creatordate",
            &format!("--format={fmt}"),
            "refs/heads",
            "refs/tags",
        ];
        // ponytail: a repo with refs past 8 MB lists only the newest ones; paginate if that bites.
        let out = self.run_records(repo, &args, 8 << 20, b'\n').await?;
        Ok(parse_refs(&String::from_utf8_lossy(&out)))
    }

    pub async fn ls_tree(&self, repo: &Path, sha: &str, path: &str) -> R<Vec<TreeEntry>> {
        let mut args = vec!["ls-tree", "-z", "-l", sha];
        let p;
        if !path.is_empty() {
            p = format!("{path}/");
            args.push("--");
            args.push(&p);
        }
        let out = self.ok(repo, &args).await?;
        Ok(parse_ls_tree(&out, path))
    }

    /// Object id, type and size of `sha:path`, or None if missing.
    pub async fn stat(
        &self,
        repo: &Path,
        sha: &str,
        path: &str,
    ) -> R<Option<(String, String, u64)>> {
        let input = format!("{sha}:{path}\n");
        let o = self
            .run(
                repo,
                &["cat-file", "--batch-check"],
                Some(input.as_bytes()),
                &[],
            )
            .await?;
        let t = o.text();
        let parts: Vec<&str> = t.trim().split(' ').collect();
        if parts.len() == 3
            && parts[2] != "missing"
            && let Ok(size) = parts[2].parse()
        {
            return Ok(Some((parts[0].to_string(), parts[1].to_string(), size)));
        }
        Ok(None)
    }

    /// The tree-entry mode of `path` at `sha` (100644, 100755, 120000...), if it exists.
    pub async fn mode_of(&self, repo: &Path, sha: &str, path: &str) -> R<Option<String>> {
        let out = self.ok(repo, &["ls-tree", sha, "--", path]).await?;
        let t = String::from_utf8_lossy(&out);
        Ok(t.lines()
            .find(|l| l.split_once('\t').is_some_and(|(_, p)| p == path))
            .and_then(|l| l.split_whitespace().next())
            .map(String::from))
    }

    pub async fn blob(&self, repo: &Path, oid: &str) -> R<Vec<u8>> {
        self.ok(repo, &["cat-file", "blob", oid]).await
    }

    pub async fn log(&self, repo: &Path, q: &LogQuery<'_>) -> R<Vec<Commit>> {
        let fmt = format!("--format={COMMIT_FMT}");
        let skip = format!("--skip={}", q.skip);
        let n = format!("-n{}", q.limit);
        let mut args: Vec<String> = vec!["log".into(), "-z".into(), fmt, skip, n];
        if let Some(g) = q.grep {
            args.push(format!("--grep={g}"));
            args.push("-i".into());
            args.push("--fixed-strings".into());
        }
        if let Some(a) = q.author {
            args.push(format!("--author={a}"));
        }
        if q.first_parent {
            args.push("--first-parent".into());
        }
        args.push(q.range.to_string());
        args.push("--".into());
        if let Some(p) = q.path
            && !p.is_empty()
        {
            args.push(p.to_string());
        }
        let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let out = self.ok(repo, &refs).await?;
        Ok(parse_log(&String::from_utf8_lossy(&out)))
    }

    pub async fn commit_info(&self, repo: &Path, sha: &str) -> R<Option<Commit>> {
        let o = self
            .run(
                repo,
                &[
                    "show",
                    "-s",
                    "-z",
                    &format!("--format={COMMIT_FMT}"),
                    sha,
                    "--",
                ],
                None,
                &[],
            )
            .await?;
        if o.code != 0 {
            return Ok(None);
        }
        Ok(parse_log(&o.text()).into_iter().next())
    }

    pub async fn count(&self, repo: &Path, range: &str) -> R<u64> {
        let out = self.ok(repo, &["rev-list", "--count", range, "--"]).await?;
        Ok(String::from_utf8_lossy(&out).trim().parse().unwrap_or(0))
    }

    pub async fn merge_base(&self, repo: &Path, a: &str, b: &str) -> R<Option<String>> {
        let o = self.run(repo, &["merge-base", a, b], None, &[]).await?;
        Ok((o.code == 0).then(|| o.text().trim().to_string()))
    }

    /// Unified diff between two commits (`from` None = root commit), capped
    /// at `DIFF_CAP` bytes. Returns the text and whether it was cut short.
    pub async fn diff(
        &self,
        repo: &Path,
        from: Option<&str>,
        to: &str,
        path: Option<&str>,
    ) -> R<(String, bool)> {
        let from = from.unwrap_or(EMPTY_TREE);
        let mut args = vec![
            "diff",
            "--no-color",
            "--no-ext-diff",
            "-M",
            "-p",
            from,
            to,
            "--",
        ];
        if let Some(p) = path.filter(|p| !p.is_empty()) {
            args.push(p);
        }
        let (out, cut) = self.run_capped(repo, &args, DIFF_CAP).await?;
        Ok((String::from_utf8_lossy(&out).into_owned(), cut))
    }

    /// Run git, keep at most `cap` bytes of stdout, kill it if it goes over.
    pub async fn run_capped(&self, repo: &Path, args: &[&str], cap: usize) -> R<(Vec<u8>, bool)> {
        use tokio::io::AsyncReadExt;
        let _permit = self.permit().await?;
        let mut cmd = self.command(Some(repo));
        cmd.args(args).stderr(Stdio::null());
        let mut child = cmd.spawn().map_err(AppError::internal)?;
        let mut stdout = child.stdout.take().expect("piped stdout");
        let mut buf = Vec::new();
        let read = async {
            let mut chunk = vec![0u8; 64 * 1024];
            loop {
                let n = stdout.read(&mut chunk).await?;
                if n == 0 {
                    return Ok::<bool, std::io::Error>(false);
                }
                buf.extend_from_slice(&chunk[..n]);
                if buf.len() >= cap {
                    buf.truncate(cap);
                    return Ok(true);
                }
            }
        };
        let cut = tokio::time::timeout(Duration::from_secs(30), read)
            .await
            .map_err(|_| {
                AppError::new(axum::http::StatusCode::GATEWAY_TIMEOUT, "git took too long")
            })?
            .map_err(AppError::internal)?;
        if cut {
            let _ = child.kill().await;
        } else if !child.wait().await.map_err(AppError::internal)?.success() {
            return Err(AppError::internal(format!("git {args:?} failed")));
        }
        Ok((buf, cut))
    }

    /// `run_capped`, cut back to the last whole record when the cap is hit.
    pub async fn run_records(&self, repo: &Path, args: &[&str], cap: usize, sep: u8) -> R<Vec<u8>> {
        let (mut out, cut) = self.run_capped(repo, args, cap).await?;
        if cut {
            out.truncate(out.iter().rposition(|b| *b == sep).map_or(0, |i| i + 1));
        }
        Ok(out)
    }

    pub async fn blame(&self, repo: &Path, sha: &str, path: &str) -> R<Vec<u8>> {
        self.ok(repo, &["blame", "--porcelain", "-w", sha, "--", path])
            .await
    }

    // ---------- writing ----------

    /// Create a commit from `parent` with the given file changes and move
    /// `branch` to it, failing if the branch moved meanwhile.
    pub async fn commit_files(&self, repo: &Path, c: &CommitSpec<'_>) -> R<String> {
        let tmp = tempfile_path(repo);
        let idx = tmp.to_string_lossy().to_string();
        let res = async {
            let env_idx = [("GIT_INDEX_FILE", idx.as_str())];
            match c.parent {
                Some(p) => {
                    self.ok_env(repo, &["read-tree", p], None, &env_idx).await?;
                }
                None => {
                    self.ok_env(repo, &["read-tree", "--empty"], None, &env_idx)
                        .await?;
                }
            }
            for ch in c.changes {
                match ch {
                    Change::Delete(path) => {
                        self.ok_env(
                            repo,
                            &["update-index", "--force-remove", "--", path],
                            None,
                            &env_idx,
                        )
                        .await?;
                    }
                    Change::Write(path, data, exec) => {
                        let oid = self
                            .ok_env(repo, &["hash-object", "-w", "--stdin"], Some(data), &[])
                            .await?;
                        let info = format!(
                            "{},{},{}",
                            if *exec { "100755" } else { "100644" },
                            oid,
                            path
                        );
                        let o = self
                            .run(
                                repo,
                                &["update-index", "--add", "--cacheinfo", &info],
                                None,
                                &env_idx,
                            )
                            .await?;
                        if o.code != 0 {
                            return Err(AppError::bad(format!(
                                "Can't write {path}: {}",
                                String::from_utf8_lossy(&o.stderr).trim()
                            )));
                        }
                    }
                }
            }
            let tree = self.ok_env(repo, &["write-tree"], None, &env_idx).await?;
            self.commit_tree(repo, &tree, &c.parents(), c.message, c.author)
                .await
        }
        .await;
        let _ = tokio::fs::remove_file(&tmp).await;
        let sha = res?;
        self.update_ref(
            repo,
            &format!("refs/heads/{}", c.branch),
            &sha,
            c.expect_old,
        )
        .await?;
        Ok(sha)
    }

    pub async fn commit_tree(
        &self,
        repo: &Path,
        tree: &str,
        parents: &[&str],
        msg: &str,
        who: (&str, &str),
    ) -> R<String> {
        let mut args = vec!["commit-tree", tree];
        for p in parents {
            args.push("-p");
            args.push(p);
        }
        let env = [
            ("GIT_AUTHOR_NAME", who.0),
            ("GIT_AUTHOR_EMAIL", who.1),
            ("GIT_COMMITTER_NAME", who.0),
            ("GIT_COMMITTER_EMAIL", who.1),
        ];
        self.ok_env(repo, &args, Some(msg.as_bytes()), &env).await
    }

    /// Move a ref, failing with 409 if it no longer points at `old` (None = must not exist).
    pub async fn update_ref(
        &self,
        repo: &Path,
        refname: &str,
        new: &str,
        old: Option<&str>,
    ) -> R<()> {
        let zero = "0000000000000000000000000000000000000000";
        let o = self
            .run(
                repo,
                &["update-ref", refname, new, old.unwrap_or(zero)],
                None,
                &[],
            )
            .await?;
        if o.code != 0 {
            return Err(AppError::conflict(
                "The branch changed while you were working. Reload and try again.",
            ));
        }
        Ok(())
    }

    pub async fn delete_ref(&self, repo: &Path, refname: &str) -> R<()> {
        self.ok(repo, &["update-ref", "-d", refname]).await?;
        Ok(())
    }

    /// Three-way merge of two commits into a tree, without a worktree.
    /// Returns Ok(tree) when clean, Err(conflicting paths) otherwise.
    pub async fn merge_tree(
        &self,
        repo: &Path,
        base: &str,
        head: &str,
    ) -> R<Result<String, Vec<String>>> {
        let o = self
            .run(
                repo,
                &[
                    "merge-tree",
                    "--write-tree",
                    "--name-only",
                    "--no-messages",
                    "-z",
                    base,
                    head,
                ],
                None,
                &[],
            )
            .await?;
        let text = o.text();
        let mut parts = text.split('\0').filter(|s| !s.is_empty());
        let tree = parts.next().unwrap_or("").trim().to_string();
        match o.code {
            0 => Ok(Ok(tree)),
            1 => Ok(Err(parts.map(|s| s.to_string()).collect())),
            _ => Err(AppError::internal(String::from_utf8_lossy(&o.stderr))),
        }
    }
}

fn tempfile_path(repo: &Path) -> PathBuf {
    let mut b = [0u8; 8];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut b);
    repo.join(format!("gitter-index-{}", hex::encode(b)))
}

pub enum Change<'a> {
    Write(&'a str, &'a [u8], bool),
    Delete(&'a str),
}

pub struct CommitSpec<'a> {
    pub branch: &'a str,
    pub parent: Option<&'a str>,
    pub expect_old: Option<&'a str>,
    pub changes: &'a [Change<'a>],
    pub message: &'a str,
    pub author: (&'a str, &'a str),
}

impl CommitSpec<'_> {
    fn parents(&self) -> Vec<&str> {
        self.parent.into_iter().collect()
    }
}

pub struct LogQuery<'a> {
    pub range: &'a str,
    pub path: Option<&'a str>,
    pub skip: usize,
    pub limit: usize,
    pub grep: Option<&'a str>,
    pub author: Option<&'a str>,
    pub first_parent: bool,
}

// ---------- parsing ----------

/// Fields separated by 0x1f; the free-text message comes last and records end
/// with NUL (`-z`), which git never allows inside a commit message.
const COMMIT_FMT: &str = "%H%x1f%P%x1f%an%x1f%ae%x1f%at%x1f%cn%x1f%ce%x1f%ct%x1f%B";

#[derive(Serialize, Clone, Debug)]
pub struct Commit {
    pub sha: String,
    pub parents: Vec<String>,
    pub author_name: String,
    pub author_email: String,
    pub author_time: i64,
    pub committer_name: String,
    pub committer_email: String,
    pub commit_time: i64,
    pub subject: String,
    pub body: String,
}

pub fn parse_log(s: &str) -> Vec<Commit> {
    s.split('\0')
        .filter_map(|rec| {
            let rec = rec.trim_start_matches('\n');
            let f: Vec<&str> = rec.splitn(9, '\x1f').collect();
            if f.len() < 9 {
                return None;
            }
            let msg = f[8].trim_end();
            let (subject, body) = match msg.split_once("\n\n") {
                Some((s, b)) => (s.replace('\n', " "), b.trim().to_string()),
                None => (msg.replace('\n', " "), String::new()),
            };
            Some(Commit {
                sha: f[0].to_string(),
                parents: f[1].split_whitespace().map(String::from).collect(),
                author_name: f[2].to_string(),
                author_email: f[3].to_string(),
                author_time: f[4].parse().unwrap_or(0),
                committer_name: f[5].to_string(),
                committer_email: f[6].to_string(),
                commit_time: f[7].parse().unwrap_or(0),
                subject,
                body,
            })
        })
        .collect()
}

#[derive(Serialize, Clone, Debug)]
pub struct Ref {
    pub name: String,
    pub kind: &'static str,
    pub sha: String,
    pub date: i64,
    pub subject: String,
    pub author: String,
}

pub fn parse_refs(s: &str) -> Vec<Ref> {
    s.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\0').collect();
            if f.len() < 7 {
                return None;
            }
            let (kind, name) = if let Some(n) = f[0].strip_prefix("refs/heads/") {
                ("branch", n)
            } else {
                let n = f[0].strip_prefix("refs/tags/")?;
                ("tag", n)
            };
            let sha = if f[2].is_empty() { f[1] } else { f[2] };
            let author = if f[5].is_empty() { f[6] } else { f[5] };
            Some(Ref {
                name: name.to_string(),
                kind,
                sha: sha.to_string(),
                date: f[3].parse().unwrap_or(0),
                subject: f[4].to_string(),
                author: author.to_string(),
            })
        })
        .collect()
}

#[derive(Serialize, Clone, Debug)]
pub struct TreeEntry {
    pub name: String,
    pub path: String,
    pub kind: String,
    pub mode: String,
    pub oid: String,
    pub size: Option<u64>,
}

pub fn parse_ls_tree(out: &[u8], dir: &str) -> Vec<TreeEntry> {
    let mut v: Vec<TreeEntry> = out
        .split(|b| *b == 0)
        .filter(|r| !r.is_empty())
        .filter_map(|rec| {
            let rec = String::from_utf8_lossy(rec);
            let (meta, path) = rec.split_once('\t')?;
            let mut m = meta.split_whitespace();
            let mode = m.next()?.to_string();
            let kind = m.next()?.to_string();
            let oid = m.next()?.to_string();
            let size = m.next().and_then(|s| s.parse().ok());
            let name = path
                .strip_prefix(dir)
                .map(|s| s.trim_start_matches('/'))
                .unwrap_or(path)
                .to_string();
            let kind = match (kind.as_str(), mode.as_str()) {
                ("blob", "120000") => "symlink".to_string(),
                ("commit", _) => "submodule".to_string(),
                _ => kind,
            };
            Some(TreeEntry {
                name,
                path: path.to_string(),
                kind,
                mode,
                oid,
                size,
            })
        })
        .collect();
    v.sort_by(|a, b| {
        let da = a.kind == "tree";
        let db = b.kind == "tree";
        db.cmp(&da)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    v
}

#[derive(Serialize, Debug)]
pub struct BlameChunk {
    pub sha: String,
    pub author: String,
    pub time: i64,
    pub summary: String,
    pub start: usize,
    pub lines: Vec<String>,
}

pub fn parse_blame(out: &str) -> Vec<BlameChunk> {
    use std::collections::HashMap;
    let mut meta: HashMap<String, (String, i64, String)> = HashMap::new();
    let mut chunks: Vec<BlameChunk> = Vec::new();
    let mut cur_sha = String::new();
    let mut cur_line = 0usize;
    let mut pending: (String, i64, String) = Default::default();
    for line in out.lines() {
        if let Some(code) = line.strip_prefix('\t') {
            let m = meta
                .entry(cur_sha.clone())
                .or_insert_with(|| pending.clone())
                .clone();
            match chunks.last_mut() {
                Some(c) if c.sha == cur_sha && c.start + c.lines.len() == cur_line => {
                    c.lines.push(code.to_string())
                }
                _ => chunks.push(BlameChunk {
                    sha: cur_sha.clone(),
                    author: m.0,
                    time: m.1,
                    summary: m.2,
                    start: cur_line,
                    lines: vec![code.to_string()],
                }),
            }
            continue;
        }
        let mut it = line.splitn(2, ' ');
        let key = it.next().unwrap_or("");
        let val = it.next().unwrap_or("");
        if key.len() >= 40 && key.bytes().all(|b| b.is_ascii_hexdigit()) {
            cur_sha = key.to_string();
            cur_line = val
                .split(' ')
                .nth(1)
                .and_then(|n| n.parse().ok())
                .unwrap_or(0);
            if let Some(m) = meta.get(&cur_sha) {
                pending = m.clone();
            } else {
                pending = Default::default();
            }
        } else {
            match key {
                "author" => pending.0 = val.to_string(),
                "author-time" => pending.1 = val.parse().unwrap_or(0),
                "summary" => pending.2 = val.to_string(),
                _ => {}
            }
        }
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn capped_reads_keep_whole_records() {
        let dir = tempfile::tempdir().unwrap();
        let g = Git::new(dir.path().join("r"), dir.path().to_path_buf(), 4);
        let repo = g.root.join("1.git");
        std::fs::create_dir_all(&g.root).unwrap();
        g.init_bare(&repo, "main", 1 << 20).await.unwrap();
        let c = g.commit_tree(&repo, EMPTY_TREE, &[], "init", ("a", "a@b.c")).await.unwrap();
        for b in ["aaaa", "bbbb", "cccc"] {
            g.update_ref(&repo, &format!("refs/heads/{b}"), &c, None).await.unwrap();
        }
        assert_eq!(g.refs(&repo).await.unwrap().len(), 3);
        let args = ["for-each-ref", "--format=%(refname)"];
        let full = g.run_records(&repo, &args, 1 << 20, b'\n').await.unwrap();
        assert_eq!(full, b"refs/heads/aaaa\nrefs/heads/bbbb\nrefs/heads/cccc\n");
        // A cap in the middle of the second line keeps only the first.
        let cut = g.run_records(&repo, &args, 20, b'\n').await.unwrap();
        assert_eq!(cut, b"refs/heads/aaaa\n");
        // A git failure is still an error, not an empty list.
        assert!(g.refs(&dir.path().join("missing.git")).await.is_err());
    }

    #[test]
    fn log_parsing() {
        let s = "abc\x1fp1 p2\x1fAnn\x1fa@x.io\x1f100\x1fCo\x1fc@x.io\x1f200\x1fFix it\n\nbody \x1e\x1f line\n\0def\x1f\x1fB\x1fb@x\x1f1\x1fB\x1fb@x\x1f2\x1fInit\n\0";
        let v = parse_log(s);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].parents, vec!["p1", "p2"]);
        assert_eq!(v[0].subject, "Fix it");
        // Separator bytes inside a message can't forge extra commits.
        assert_eq!(v[0].body, "body \x1e\x1f line");
        assert_eq!(v[1].subject, "Init");
        assert!(v[1].parents.is_empty());
    }

    #[test]
    fn tree_parsing() {
        let out = b"100644 blob aaa      12\tsrc/z.rs\x00040000 tree bbb       -\tsrc/lib\x00100644 blob ccc       3\tsrc/A.md\x00160000 commit ddd       -\tsrc/sub\0";
        let v = parse_ls_tree(out, "src");
        assert_eq!(v[0].name, "lib");
        assert_eq!(v[0].kind, "tree");
        assert_eq!(v[1].name, "A.md");
        assert_eq!(v[1].size, Some(3));
        assert!(v.iter().any(|e| e.kind == "submodule"));
    }

    #[test]
    fn refs_parsing() {
        let s = "refs/heads/main\0aaa\0\x00100\0subj\0Ann\0\nrefs/tags/v1\0ttt\0ccc\x00200\0tagmsg\0\0Bob\n";
        let v = parse_refs(s);
        assert_eq!(v[0].name, "main");
        assert_eq!(v[0].kind, "branch");
        assert_eq!(v[1].sha, "ccc");
        assert_eq!(v[1].author, "Bob");
    }

    #[test]
    fn blame_parsing() {
        let sha1 = "a".repeat(40);
        let sha2 = "b".repeat(40);
        let out = format!(
            "{sha1} 1 1 2\nauthor Ann\nauthor-time 10\nsummary first\nfilename f\n\tline1\n{sha1} 2 2\n\tline2\n{sha2} 3 3 1\nauthor Bob\nauthor-time 20\nsummary second\nfilename f\n\tline3\n"
        );
        let c = parse_blame(&out);
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].lines, vec!["line1", "line2"]);
        assert_eq!(c[0].author, "Ann");
        assert_eq!(c[1].start, 3);
        assert_eq!(c[1].summary, "second");
    }
}
