//! The server's side of a running job: claiming it with its secrets, and
//! keeping what the runner reports (log, step progress, artifacts, result).

use std::path::{Path, PathBuf};

use rusqlite::OptionalExtension;
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;

use super::queue::{self, Lease};
use crate::AppState;
use crate::db::now;
use crate::error::{AppError, R};

/// A job's log stops growing here (a runaway `yes` can't fill the disk).
pub const LOG_CAP: u64 = 16 << 20;
/// A job's artifacts archive is at most this big.
pub const ARTIFACT_CAP: u64 = 1 << 30;
/// Artifacts are deleted this long after their job finished.
pub const ARTIFACT_DAYS: i64 = 30;

pub fn log_path(data: &Path, job: i64) -> PathBuf {
    data.join("ci/logs").join(format!("{job}.log"))
}

pub fn artifact_path(data: &Path, job: i64) -> PathBuf {
    data.join("ci/artifacts").join(format!("{job}.tar.gz"))
}

/// Claim a job for a runner: the attempt starts with an empty log and no
/// artifacts, and carries the repository's secrets. Nothing is handed out
/// while CI is off; the built-in runner (the server itself) and the "admins"
/// setting only take jobs of repositories owned by admins, and the built-in
/// runner never takes a run started by an agent's changes.
pub async fn claim(st: &AppState, runner_id: i64, labels: Vec<String>, builtin: bool) -> R<Option<Lease>> {
    let s = st.settings.get();
    let mode = s.ci.clone();
    if mode == "off" {
        return Ok(None);
    }
    let admins_only = builtin || mode != "everyone";
    // A job its environment refuses ends at once, and the next one is claimed instead.
    loop {
        let labels = labels.clone();
        let Some(mut lease) = st.db.call(move |c| Ok(queue::claim(c, runner_id, &labels, admins_only, builtin)?)).await? else {
            return Ok(None);
        };
        lease.limits = queue::Limits { cpus: s.ci_cpus, memory_mb: s.ci_memory_mb, network: s.ci_network };
        let data = &st.cfg.data_dir;
        let _ = tokio::fs::remove_file(log_path(data, lease.job_id)).await;
        let _ = tokio::fs::remove_file(artifact_path(data, lease.job_id)).await;
        let (job, repo) = (lease.job_id, lease.repo_id);
        let (sealed, by_agent): (Vec<(String, Vec<u8>)>, bool) = st
            .db
            .call(move |c| {
                c.execute("DELETE FROM ci_artifacts WHERE job_id = ?1", [job])?;
                // Earlier attempts' test results stop showing (they stay for spotting flaky tests).
                c.execute("UPDATE ci_test_results SET current = 0 WHERE job_id = ?1", [job])?;
                c.execute("DELETE FROM ci_job_reports WHERE job_id = ?1", [job])?;
                // A run an agent started gets no secrets: its workflow could print them.
                let by_agent: bool = c.query_row(
                    "SELECT r.agent IS NOT NULL FROM ci_jobs j JOIN ci_runs r ON r.id = j.run_id WHERE j.id = ?1",
                    [job],
                    |r| r.get(0),
                )?;
                if by_agent {
                    return Ok((vec![], true));
                }
                let mut q = c.prepare("SELECT name, value FROM ci_secrets WHERE repo_id = ?1 ORDER BY name")?;
                Ok((q.query_map([repo], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?, false))
            })
            .await?;
        if by_agent {
            append_log(st, lease.job_id, b"\x1b[33mAn agent started this run, so it gets no CI secrets.\x1b[0m\n").await?;
        }
        let total = sealed.len();
        lease.secrets = sealed
            .into_iter()
            .filter_map(|(n, v)| Some((n, String::from_utf8(crate::security::open(&st.key, &v)?).ok()?)))
            .collect();
        if lease.secrets.len() < total {
            let note = format!(
                "\x1b[33mwarning: {} of this repository's secrets can't be decrypted (was GITTER_DATA/secret.key replaced?); set them again in Settings → CI secrets\x1b[0m\n",
                total - lease.secrets.len()
            );
            append_log(st, lease.job_id, note.as_bytes()).await?;
        }
        if lease.job.environment.is_some() || lease.job.preview.is_some() {
            match crate::api::deploy::job_started(st, &lease, by_agent).await? {
                Ok(theirs) => {
                    // The environment's own secrets win over the repository's of the same name.
                    lease.secrets.retain(|(n, _)| !theirs.iter().any(|(m, _)| m == n));
                    lease.secrets.extend(theirs);
                }
                Err(why) => {
                    append_log(st, lease.job_id, format!("\x1b[31m{why}\x1b[0m\n").as_bytes()).await?;
                    finish(st, lease.job_id, &lease.token, false, None).await?;
                    continue;
                }
            }
        }
        return Ok(Some(lease));
    }
}

/// Does `token` still hold job `job` (this attempt is the current one)?
pub async fn holds(st: &AppState, job: i64, token: &str) -> bool {
    let t = token.to_string();
    matches!(st.db.call(move |c| Ok(queue::job_for_token(c, &t)?)).await, Ok(Some(j)) if j == job)
}

/// Renew the lease (false: stop the job).
pub async fn heartbeat(st: &AppState, job: i64, token: &str) -> R<bool> {
    let t = token.to_string();
    st.db.call(move |c| Ok(queue::heartbeat(c, job, &t)?)).await
}

/// Record the attempt's result and wake runners for the jobs that needed it.
pub async fn finish(st: &AppState, job: i64, token: &str, success: bool, code: Option<i32>) -> R<bool> {
    let t = token.to_string();
    let done = st.db.call(move |c| Ok(queue::finish(c, job, &t, success, code)?)).await?;
    st.ci_wake.notify_waiters();
    if done {
        crate::api::deploy::job_finished(st, job, success).await;
        crate::api::notifications::ci_finished(st, job).await;
    }
    // A finished run may be the last check a pull request waited on to merge itself.
    let finished: Option<(i64, String)> = st
        .db
        .call(move |c| {
            Ok(c.query_row(
                "SELECT r.repo_id, r.sha FROM ci_jobs j JOIN ci_runs r ON r.id = j.run_id WHERE j.id = ?1 AND r.status IN ('success', 'failure', 'cancelled', 'skipped')",
                [job],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
        })
        .await?;
    if let Some((repo, sha)) = finished {
        let st = st.clone();
        tokio::spawn(async move {
            crate::protect::auto_merge(&st, repo, Some(sha)).await;
            crate::mergequeue::advance(&st, repo).await;
        });
    }
    Ok(done)
}

/// Hourly: delete logs and artifacts whose job is gone (its repository was
/// deleted), artifacts past ARTIFACT_DAYS, and artifact rows whose file is missing
/// (backups don't carry CI files).
pub async fn sweep(st: &AppState) -> R<usize> {
    crate::api::preview::sweep(st).await;
    let data = st.cfg.data_dir.clone();
    let cutoff = now() - ARTIFACT_DAYS * 86400;
    let old: Vec<i64> = st
        .db
        .call(move |c| {
            let mut q = c.prepare("SELECT a.job_id FROM ci_artifacts a JOIN ci_jobs j ON j.id = a.job_id WHERE coalesce(j.finished_at, a.created_at) < ?1")?;
            let ids: Vec<i64> = q.query_map([cutoff], |r| r.get(0))?.collect::<Result<_, _>>()?;
            c.execute("DELETE FROM ci_artifacts WHERE job_id IN (SELECT a.job_id FROM ci_artifacts a JOIN ci_jobs j ON j.id = a.job_id WHERE coalesce(j.finished_at, a.created_at) < ?1)", [cutoff])?;
            Ok(ids)
        })
        .await?;
    let mut gone = 0;
    for id in old {
        gone += tokio::fs::remove_file(artifact_path(&data, id)).await.is_ok() as usize;
    }
    // Files named after jobs that no longer exist, and rows whose file is missing.
    let (d2, mut files) = (data.clone(), Vec::new());
    for dir in ["ci/logs", "ci/artifacts"] {
        let Ok(rd) = std::fs::read_dir(d2.join(dir)) else { continue };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if let Some(id) = name.split('.').next().and_then(|n| n.parse::<i64>().ok()) {
                files.push((e.path(), id));
            }
        }
    }
    let ids: Vec<i64> = files.iter().map(|f| f.1).collect();
    let known: std::collections::HashSet<i64> = st
        .db
        .call(move |c| {
            let mut q = c.prepare_cached("SELECT 1 FROM ci_jobs WHERE id = ?1")?;
            let mut out = std::collections::HashSet::new();
            for id in ids {
                if q.exists([id])? {
                    out.insert(id);
                }
            }
            Ok(out)
        })
        .await?;
    for (p, id) in files {
        if !known.contains(&id) && tokio::fs::remove_file(&p).await.is_ok() {
            gone += 1;
        }
    }
    let missing: Vec<i64> = st
        .db
        .call(|c| {
            let mut q = c.prepare("SELECT job_id FROM ci_artifacts")?;
            Ok(q.query_map([], |r| r.get::<_, i64>(0))?.collect::<Result<Vec<_>, _>>()?)
        })
        .await?
        .into_iter()
        .filter(|id| !artifact_path(&data, *id).exists())
        .collect();
    if !missing.is_empty() {
        st.db
            .call(move |c| {
                for id in missing {
                    c.execute("DELETE FROM ci_artifacts WHERE job_id = ?1", [id])?;
                }
                Ok(())
            })
            .await?;
    }
    Ok(gone)
}

/// Append to a job's log, up to LOG_CAP.
pub async fn append_log(st: &AppState, job: i64, data: &[u8]) -> R<()> {
    let p = log_path(&st.cfg.data_dir, job);
    if let Some(d) = p.parent() {
        tokio::fs::create_dir_all(d).await?;
    }
    let mut f = tokio::fs::OpenOptions::new().create(true).append(true).open(&p).await?;
    let len = f.metadata().await?.len();
    if len >= LOG_CAP {
        return Ok(());
    }
    let room = (LOG_CAP - len) as usize;
    if data.len() <= room {
        f.write_all(data).await?;
    } else {
        f.write_all(&data[..room]).await?;
        f.write_all(b"\n\x1b[33m[the log is cut off here at 16 MB]\x1b[0m\n").await?;
    }
    Ok(())
}

/// A step started ("running") or ended ("success", "failure", "skipped").
/// A started step remembers where its output begins in the log.
pub async fn step(st: &AppState, job: i64, index: usize, status: &str) -> R<()> {
    if !matches!(status, "running" | "success" | "failure" | "skipped") {
        return Err(AppError::bad("Unknown step status"));
    }
    let offset = tokio::fs::metadata(log_path(&st.cfg.data_dir, job)).await.map(|m| m.len()).unwrap_or(0);
    let status = status.to_string();
    st.db
        .call(move |c| {
            let raw: String = c.query_row("SELECT steps FROM ci_jobs WHERE id = ?1", [job], |r| r.get(0))?;
            let mut steps: Vec<Value> = serde_json::from_str(&raw).unwrap_or_default();
            let Some(s) = steps.get_mut(index) else { return Err(AppError::bad("No such step")) };
            if status == "running" {
                s["offset"] = json!(offset);
                s["started_at"] = json!(now());
            } else {
                s["finished_at"] = json!(now());
            }
            s["status"] = json!(status);
            c.execute("UPDATE ci_jobs SET steps = ?2 WHERE id = ?1", rusqlite::params![job, Value::from(steps).to_string()])?;
            Ok(())
        })
        .await
}

/// Keep a job's artifacts archive (a .tar.gz the runner built), moving or copying `file` into place.
pub async fn store_artifact(st: &AppState, job: i64, file: &Path) -> R<()> {
    let dest = artifact_path(&st.cfg.data_dir, job);
    if let Some(d) = dest.parent() {
        tokio::fs::create_dir_all(d).await?;
    }
    if tokio::fs::rename(file, &dest).await.is_err() {
        tokio::fs::copy(file, &dest).await?;
    }
    let size = tokio::fs::metadata(&dest).await?.len() as i64;
    st.db
        .call(move |c| {
            c.execute(
                "INSERT OR REPLACE INTO ci_artifacts(job_id, name, size, created_at) VALUES (?1, 'artifacts.tar.gz', ?2, ?3)",
                rusqlite::params![job, size, now()],
            )?;
            Ok(())
        })
        .await
}
