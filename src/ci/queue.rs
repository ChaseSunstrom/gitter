//! The CI queue in SQLite. A runner claims a queued job and gets a lease it
//! renews with heartbeats, plus a job token that is new for every attempt. A job
//! whose lease runs out goes back to the queue; its old token stops working,
//! so a runner that lost the job can't report over the new attempt (fencing).
//!
//! Job states: waiting (on `needs`) → queued → running → success | failure
//! | cancelled | skipped | error (lost too many times).


use rusqlite::{Connection, OptionalExtension, Transaction, params};

use super::config::Job;
use crate::db::now;

/// True for an owner `u` that is a site admin, or an org a site admin created.
/// (Not "has an admin among its owners": any owner can add anyone as an owner.)
pub const ADMIN_OWNED: &str = "(u.is_admin = 1 OR EXISTS (SELECT 1 FROM orgs o JOIN users a ON a.id = o.created_by WHERE o.id = u.id AND a.is_admin = 1))";

/// Job tokens: one per attempt, good while it runs (also reads the repository).
pub const JOB_TOKEN_PREFIX: &str = "gcj_";

/// How long a claim lasts without a heartbeat.
pub const LEASE_SECS: i64 = 90;
/// Claims a job gets before it's failed as lost.
const MAX_ATTEMPTS: i64 = 3;

pub fn terminal(status: &str) -> bool {
    matches!(status, "success" | "failure" | "cancelled" | "skipped" | "error")
}

pub struct NewRun {
    pub repo_id: i64,
    pub event: String,
    pub refname: String,
    pub sha: String,
    pub pr_number: Option<i64>,
    pub actor_id: Option<i64>,
    pub title: String,
}

/// Record a run and its jobs. Jobs in `skip` (their `paths` didn't match) start
/// skipped; jobs with no `needs` start queued. Returns (run id, run number).
pub fn create_run(c: &mut Connection, r: &NewRun, jobs: &[Job], skip: &[bool]) -> rusqlite::Result<(i64, i64)> {
    let tx = c.transaction()?;
    let number: i64 = tx.query_row(
        "SELECT COALESCE(max(number), 0) + 1 FROM ci_runs WHERE repo_id = ?1",
        [r.repo_id],
        |x| x.get(0),
    )?;
    tx.execute(
        "INSERT INTO ci_runs(repo_id, number, event, ref, sha, pr_number, actor_id, title, status, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'queued', ?9)",
        params![r.repo_id, number, r.event, r.refname, r.sha, r.pr_number, r.actor_id, r.title, now()],
    )?;
    let run = tx.last_insert_rowid();
    for (i, j) in jobs.iter().enumerate() {
        let by_paths = skip.get(i).copied().unwrap_or(false);
        let status = match (by_paths, j.needs.is_empty()) {
            (true, _) => "skipped",
            (false, true) => "queued",
            (false, false) => "waiting",
        };
        let spec = serde_json::to_string(j).map_err(|e| rusqlite::Error::ToSqlConversionFailure(e.into()))?;
        tx.execute(
            "INSERT INTO ci_jobs(run_id, name, key, spec, status, reason, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![run, j.name, j.key, spec, status, if by_paths { "paths" } else { "" }, now()],
        )?;
    }
    advance(&tx, run)?;
    tx.commit()?;
    Ok((run, number))
}

/// Record a run that couldn't start (its config is broken). Returns (run id, run number);
/// it ended at once, so its follow-ups are due (`jobs::run_ended`).
pub fn create_failed_run(c: &mut Connection, r: &NewRun, error: &str) -> rusqlite::Result<(i64, i64)> {
    let (run, number) = create_run(c, r, &[], &[])?;
    c.execute(
        "UPDATE ci_runs SET status = 'failure', error = ?2, finished_at = ?3 WHERE id = ?1",
        params![run, error, now()],
    )?;
    Ok((run, number))
}

/// What a runner needs to run a claimed job.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Lease {
    pub job_id: i64,
    pub attempt: i64,
    /// Authenticates this attempt's heartbeats, logs and result; also reads the repository.
    pub token: String,
    pub run_id: i64,
    pub repo_id: i64,
    pub repo: String,
    pub sha: String,
    pub refname: String,
    pub event: String,
    pub run_number: i64,
    pub job: Job,
    /// The repository's CI secrets, decrypted (filled in by `jobs::claim`).
    #[serde(default)]
    pub secrets: Vec<(String, String)>,
    /// The site's limits for job containers (filled in by `jobs::claim`).
    #[serde(default)]
    pub limits: Limits,
}

/// What a job's container may use (0: no limit).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Limits {
    pub cpus: u32,
    pub memory_mb: u32,
    /// Off: no job gets a network, whatever it asks for.
    pub network: bool,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { cpus: 0, memory_mb: 0, network: true }
    }
}

/// Claim the oldest queued job this runner's labels can run. With
/// `admins_only`, only jobs of repositories owned by admins.
/// Runs waiting for approval are never handed out. `builtin`: the runner built
/// into the server is the server itself, so it takes only runs a site admin
/// started (pushed, merged, pressed Run, or last pushed the branch a schedule
/// runs), never one an agent's changes started.
pub fn claim(c: &mut Connection, runner_id: i64, labels: &[String], admins_only: bool, builtin: bool) -> rusqlite::Result<Option<Lease>> {
    let tx = c.transaction()?;
    let picked = {
        // ponytail: parses each queued job's labels; a runs_on column if queues get long.
        let mut q = tx.prepare_cached(&format!(
            "SELECT j.id, j.spec FROM ci_jobs j JOIN ci_runs r ON r.id = j.run_id JOIN repos p ON p.id = r.repo_id
             JOIN users u ON u.id = p.owner_id WHERE j.status = 'queued' AND p.deleted_at IS NULL AND (?1 = 0 OR {ADMIN_OWNED})
             AND (r.approval IS NULL OR r.approval = 'approved')
             AND (?2 = 0 OR (r.agent IS NULL AND EXISTS (SELECT 1 FROM users a WHERE a.id = r.actor_id AND a.is_admin = 1)))
             ORDER BY j.id LIMIT 10000"
        ))?;
        let rows = q.query_map([admins_only, builtin], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
        let mut found = None;
        for row in rows {
            let (id, spec) = row?;
            let Ok(job) = serde_json::from_str::<Job>(&spec) else { continue };
            if job.runs_on.iter().all(|l| labels.contains(l)) {
                found = Some((id, job));
                break;
            }
        }
        found
    };
    let Some((job_id, job)) = picked else { return Ok(None) };
    let (token, hash) = crate::auth::new_secret(JOB_TOKEN_PREFIX);
    let t = now();
    let steps: Vec<serde_json::Value> = job.steps.iter().map(|s| serde_json::json!({ "name": s.name, "status": "pending" })).collect();
    tx.execute(
        "UPDATE ci_jobs SET status = 'running', runner_id = ?2, attempt = attempt + 1, lease_until = ?3,
         token_hash = ?4, started_at = ?5, finished_at = NULL, exit_code = NULL, steps = ?6 WHERE id = ?1",
        params![job_id, runner_id, t + LEASE_SECS, hash, t, serde_json::Value::from(steps).to_string()],
    )?;
    let lease = tx.query_row(
        "SELECT j.attempt, r.id, r.repo_id, u.username || '/' || p.name, r.sha, r.ref, r.event, r.number
         FROM ci_jobs j JOIN ci_runs r ON r.id = j.run_id JOIN repos p ON p.id = r.repo_id JOIN users u ON u.id = p.owner_id
         WHERE j.id = ?1",
        [job_id],
        |r| {
            Ok(Lease {
                job_id,
                attempt: r.get(0)?,
                token: token.clone(),
                run_id: r.get(1)?,
                repo_id: r.get(2)?,
                repo: r.get(3)?,
                sha: r.get(4)?,
                refname: r.get(5)?,
                event: r.get(6)?,
                run_number: r.get(7)?,
                job: job.clone(),
                secrets: vec![],
                limits: Limits::default(),
            })
        },
    )?;
    tx.execute(
        "UPDATE ci_runs SET status = 'running', started_at = COALESCE(started_at, ?2) WHERE id = ?1",
        params![lease.run_id, t],
    )?;
    tx.execute("UPDATE ci_runners SET last_seen = ?2 WHERE id = ?1", params![runner_id, t])?;
    tx.commit()?;
    Ok(Some(lease))
}

/// The job a token's current attempt is running, if the attempt still holds it.
pub fn job_for_token(c: &Connection, token: &str) -> rusqlite::Result<Option<i64>> {
    let hash = crate::auth::sha256_hex(token);
    c.query_row(
        "SELECT id FROM ci_jobs WHERE token_hash = ?1 AND status = 'running'",
        [hash],
        |r| r.get(0),
    )
    .optional()
}

/// Renew the lease. False when the attempt no longer holds the job (requeued,
/// cancelled, or finished elsewhere): the runner must stop it.
pub fn heartbeat(c: &Connection, job_id: i64, token: &str) -> rusqlite::Result<bool> {
    let n = c.execute(
        "UPDATE ci_jobs SET lease_until = ?3 WHERE id = ?1 AND token_hash = ?2 AND status = 'running'",
        params![job_id, crate::auth::sha256_hex(token), now() + LEASE_SECS],
    )?;
    Ok(n == 1)
}

/// Record the attempt's result. False (and nothing changes) when it was fenced off.
pub fn finish(c: &mut Connection, job_id: i64, token: &str, success: bool, exit_code: Option<i32>) -> rusqlite::Result<bool> {
    let tx = c.transaction()?;
    let n = tx.execute(
        "UPDATE ci_jobs SET status = ?3, exit_code = ?4, finished_at = ?5, lease_until = NULL, token_hash = NULL
         WHERE id = ?1 AND token_hash = ?2 AND status = 'running'",
        params![job_id, crate::auth::sha256_hex(token), if success { "success" } else { "failure" }, exit_code, now()],
    )?;
    if n == 1 {
        let run: i64 = tx.query_row("SELECT run_id FROM ci_jobs WHERE id = ?1", [job_id], |r| r.get(0))?;
        advance(&tx, run)?;
    }
    tx.commit()?;
    Ok(n == 1)
}

/// What `requeue_expired` did: how many jobs went back in the queue, the jobs failed
/// as lost, and the runs that ended with them (their follow-ups: `jobs::after_lost`).
#[derive(Debug, Default)]
pub struct Lost {
    pub requeued: usize,
    pub failed: Vec<i64>,
    pub ended: Vec<i64>,
}

/// Put jobs whose runner stopped heartbeating back in the queue, or fail them
/// after MAX_ATTEMPTS.
pub fn requeue_expired(c: &mut Connection) -> rusqlite::Result<Lost> {
    let tx = c.transaction()?;
    let t = now();
    let runs: Vec<i64> = {
        let mut q = tx.prepare("SELECT DISTINCT run_id FROM ci_jobs WHERE status = 'running' AND lease_until < ?1")?;
        q.query_map([t], |r| r.get(0))?.collect::<Result<_, _>>()?
    };
    let failed: Vec<i64> = {
        let mut q = tx.prepare(
            "UPDATE ci_jobs SET status = 'error', finished_at = ?1, lease_until = NULL, token_hash = NULL
             WHERE status = 'running' AND lease_until < ?1 AND attempt >= ?2 RETURNING id",
        )?;
        q.query_map(params![t, MAX_ATTEMPTS], |r| r.get(0))?.collect::<Result<_, _>>()?
    };
    let requeued = tx.execute(
        "UPDATE ci_jobs SET status = 'queued', lease_until = NULL, token_hash = NULL, runner_id = NULL
         WHERE status = 'running' AND lease_until < ?1",
        [t],
    )?;
    // A run with a running job wasn't over, so each one that is now ended here.
    let mut ended = Vec::new();
    for run in runs {
        advance(&tx, run)?;
        let status: String = tx.query_row("SELECT status FROM ci_runs WHERE id = ?1", [run], |r| r.get(0))?;
        if terminal(&status) {
            ended.push(run);
        }
    }
    tx.commit()?;
    Ok(Lost { requeued, failed, ended })
}

/// A runner was removed: its running jobs go back to the queue now (or fail
/// if they've been lost too often), as if their leases had run out.
pub fn release_runner(c: &mut Connection, runner_id: i64) -> rusqlite::Result<Lost> {
    c.execute("UPDATE ci_jobs SET lease_until = 0 WHERE runner_id = ?1 AND status = 'running'", [runner_id])?;
    requeue_expired(c)
}

/// Cancel a run's unfinished jobs; a running one stops at its next heartbeat.
pub fn cancel(c: &mut Connection, run_id: i64) -> rusqlite::Result<()> {
    let tx = c.transaction()?;
    tx.execute(
        "UPDATE ci_jobs SET status = 'cancelled', finished_at = ?2, lease_until = NULL, token_hash = NULL
         WHERE run_id = ?1 AND status IN ('waiting', 'queued', 'running')",
        params![run_id, now()],
    )?;
    advance(&tx, run_id)?;
    tx.commit()
}

/// Queue the failed, cancelled and lost jobs of a finished run again (and the
/// jobs skipped because of them; jobs skipped by `paths` stay skipped), or all
/// of them. False if the run is still going.
pub fn rerun(c: &mut Connection, run_id: i64, all: bool) -> rusqlite::Result<bool> {
    let tx = c.transaction()?;
    let status: String = tx.query_row("SELECT status FROM ci_runs WHERE id = ?1", [run_id], |r| r.get(0))?;
    let jobs: i64 = tx.query_row("SELECT count(*) FROM ci_jobs WHERE run_id = ?1", [run_id], |r| r.get(0))?;
    // A run whose config was broken has nothing to run again: push a fix instead.
    if !terminal(&status) || jobs == 0 {
        return Ok(false);
    }
    let which = if all { "1" } else { "(status IN ('failure', 'cancelled', 'error') OR (status = 'skipped' AND reason = 'needs'))" };
    tx.execute(
        &format!(
            "UPDATE ci_jobs SET status = 'waiting', reason = '', attempt = 0, runner_id = NULL, exit_code = NULL, started_at = NULL,
             finished_at = NULL, steps = '[]' WHERE run_id = ?1 AND {which}"
        ),
        [run_id],
    )?;
    tx.execute(
        "UPDATE ci_runs SET status = 'queued', finished_at = NULL, error = '' WHERE id = ?1",
        [run_id],
    )?;
    advance(&tx, run_id)?;
    tx.commit()?;
    Ok(true)
}

/// Move waiting jobs on: queued once every job they need succeeded, skipped
/// once one of them ended any other way (as on GitHub). Then settle the run.
fn advance(tx: &Transaction, run_id: i64) -> rusqlite::Result<()> {
    let mut jobs: Vec<(i64, String, String, Vec<String>)> = {
        let mut q = tx.prepare("SELECT id, key, status, spec FROM ci_jobs WHERE run_id = ?1")?;
        q.query_map([run_id], |r| {
            let spec: String = r.get(3)?;
            let needs = serde_json::from_str::<Job>(&spec).map(|j| j.needs).unwrap_or_default();
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, needs))
        })?
        .collect::<Result<_, _>>()?
    };
    loop {
        // A key can stand for several matrix jobs; all of them count.
        let of = |k: &str| jobs.iter().filter(|j| j.1 == k).map(|j| j.2.as_str()).collect::<Vec<_>>();
        let next = jobs.iter().enumerate().find_map(|(i, (_, _, status, needs))| {
            if status != "waiting" {
                return None;
            }
            let states: Vec<&str> = needs.iter().flat_map(|n| of(n)).collect();
            if states.iter().any(|s| terminal(s) && *s != "success") {
                Some((i, "skipped"))
            } else if states.iter().all(|s| *s == "success") {
                Some((i, "queued"))
            } else {
                None
            }
        });
        let Some((i, to)) = next else { break };
        let reason = if to == "skipped" { "needs" } else { "" };
        tx.execute("UPDATE ci_jobs SET status = ?2, reason = ?3 WHERE id = ?1", params![jobs[i].0, to, reason])?;
        jobs[i].2 = to.to_string();
    }
    if jobs.iter().all(|j| terminal(&j.2)) {
        let any = |s: &str| jobs.iter().any(|j| j.2 == s);
        let run_status = if any("failure") || any("error") {
            "failure"
        } else if any("cancelled") {
            "cancelled"
        } else if !any("success") {
            "skipped"
        } else {
            "success"
        };
        tx.execute(
            "UPDATE ci_runs SET status = ?2, finished_at = COALESCE(finished_at, ?3) WHERE id = ?1",
            params![run_id, run_status, now()],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ci::config::parse;

    fn setup(c: &mut Connection) -> (i64, i64) {
        c.execute("INSERT INTO users(username, pass_hash, created_at) VALUES ('ann', 'x', 0)", []).unwrap();
        c.execute("INSERT INTO repos(owner_id, name, created_at, updated_at) VALUES (1, 'r', 0, 0)", []).unwrap();
        c.execute("INSERT INTO ci_runners(name, labels, created_at) VALUES ('a', '[]', 0)", []).unwrap();
        c.execute("INSERT INTO ci_runners(name, labels, created_at) VALUES ('b', '[\"gpu\"]', 0)", []).unwrap();
        (1, 2)
    }

    fn run(c: &mut Connection, yaml: &str) -> i64 {
        let p = parse(yaml).unwrap();
        let r = NewRun { repo_id: 1, event: "push".into(), refname: "refs/heads/main".into(), sha: "a".repeat(40), pr_number: None, actor_id: None, title: "t".into() };
        create_run(c, &r, &p.jobs, &[]).unwrap().0
    }

    fn status(c: &Connection, sql: &str) -> String {
        c.query_row(sql, [], |r| r.get(0)).unwrap()
    }

    fn jobs(c: &Connection) -> Vec<(String, String)> {
        let mut q = c.prepare("SELECT name, status FROM ci_jobs ORDER BY id").unwrap();
        q.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect()
    }

    const CHAIN: &str = "jobs:\n  a:\n    steps: [echo a]\n  b:\n    needs: [a]\n    steps: [echo b]\n  c:\n    needs: [b]\n    steps: [echo c]\n";

    #[tokio::test]
    async fn needs_run_in_order_and_failures_skip_what_follows() {
        let db = crate::db::Db::memory().unwrap();
        db.call(|c| {
            let (r1, _) = setup(c);
            let run_id = run(c, CHAIN);
            assert_eq!(jobs(c).iter().map(|j| j.1.as_str()).collect::<Vec<_>>(), ["queued", "waiting", "waiting"]);
            let a = claim(c, r1, &[], false, false).unwrap().unwrap();
            assert_eq!((a.job.key.as_str(), a.attempt, a.repo.as_str()), ("a", 1, "ann/r"));
            assert!(claim(c, r1, &[], false, false).unwrap().is_none(), "b waits for a");
            assert!(finish(c, a.job_id, &a.token, true, Some(0)).unwrap());
            let b = claim(c, r1, &[], false, false).unwrap().unwrap();
            assert_eq!(b.job.key, "b");
            assert!(finish(c, b.job_id, &b.token, false, Some(1)).unwrap());
            assert_eq!(jobs(c)[2].1, "skipped");
            assert_eq!(status(c, "SELECT status FROM ci_runs"), "failure");
            // Re-run failed jobs: b and what it skipped go again, a stays green.
            assert!(rerun(c, run_id, false).unwrap());
            assert_eq!(jobs(c).iter().map(|j| j.1.as_str()).collect::<Vec<_>>(), ["success", "queued", "waiting"]);
            assert_eq!(status(c, "SELECT status FROM ci_runs"), "queued");
            Ok(())
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn rerunning_failures_leaves_path_skipped_jobs_alone() {
        let db = crate::db::Db::memory().unwrap();
        db.call(|c| {
            let (r1, _) = setup(c);
            let p = parse("jobs:\n  a:\n    steps: [x]\n  docs:\n    paths: [docs/]\n    steps: [y]\n  b:\n    needs: [a]\n    steps: [z]\n").unwrap();
            let r = NewRun { repo_id: 1, event: "push".into(), refname: "refs/heads/main".into(), sha: "a".repeat(40), pr_number: None, actor_id: None, title: "t".into() };
            let (run_id, _) = create_run(c, &r, &p.jobs, &[false, true, false]).unwrap();
            let a = claim(c, r1, &[], false, false).unwrap().unwrap();
            finish(c, a.job_id, &a.token, false, Some(1)).unwrap();
            assert_eq!(jobs(c).iter().map(|j| j.1.as_str()).collect::<Vec<_>>(), ["failure", "skipped", "skipped"]);
            rerun(c, run_id, false).unwrap();
            assert_eq!(jobs(c).iter().map(|j| j.1.as_str()).collect::<Vec<_>>(), ["queued", "skipped", "waiting"], "docs had no matching change");
            // A removed runner's job goes straight back to the queue.
            let again = claim(c, r1, &[], false, false).unwrap().unwrap();
            assert_eq!(release_runner(c, r1).unwrap().requeued, 1);
            assert!(!heartbeat(c, again.job_id, &again.token).unwrap());
            assert_eq!(jobs(c)[0].1, "queued");
            Ok(())
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn lost_jobs_requeue_and_stale_attempts_are_fenced() {
        let db = crate::db::Db::memory().unwrap();
        db.call(|c| {
            let (r1, r2) = setup(c);
            run(c, "jobs:\n  a:\n    steps: [echo a]\n");
            let first = claim(c, r1, &[], false, false).unwrap().unwrap();
            assert!(heartbeat(c, first.job_id, &first.token).unwrap());
            c.execute("UPDATE ci_jobs SET lease_until = 0", []).unwrap();
            let lost = requeue_expired(c).unwrap();
            assert_eq!((lost.requeued, lost.failed.len(), lost.ended.len()), (1, 0, 0), "back in the queue, the run goes on");
            let second = claim(c, r2, &[], false, false).unwrap().unwrap();
            assert_eq!(second.attempt, 2);
            assert!(!heartbeat(c, first.job_id, &first.token).unwrap(), "the old attempt must stop");
            assert!(!finish(c, first.job_id, &first.token, false, Some(1)).unwrap(), "and can't report");
            assert_eq!(job_for_token(c, &second.token).unwrap(), Some(second.job_id));
            assert_eq!(job_for_token(c, &first.token).unwrap(), None);
            assert!(finish(c, second.job_id, &second.token, true, Some(0)).unwrap());
            assert_eq!(status(c, "SELECT status FROM ci_runs"), "success");
            // A job lost on every attempt fails instead of looping forever.
            run(c, "jobs:\n  z:\n    steps: [echo z]\n");
            let mut lost = Lost::default();
            for _ in 0..MAX_ATTEMPTS {
                claim(c, r1, &[], false, false).unwrap().unwrap();
                c.execute("UPDATE ci_jobs SET lease_until = 0 WHERE status = 'running'", []).unwrap();
                lost = requeue_expired(c).unwrap();
            }
            assert_eq!(jobs(c)[1].1, "error");
            let (job, run): (i64, i64) = c.query_row("SELECT id, run_id FROM ci_jobs ORDER BY id DESC", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
            assert_eq!((lost.requeued, lost.failed, lost.ended), (0, vec![job], vec![run]), "the lost job ended its run");
            assert_eq!(status(c, "SELECT status FROM ci_runs ORDER BY id DESC"), "failure");
            Ok(())
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn labels_route_jobs_and_cancel_stops_running_ones() {
        let db = crate::db::Db::memory().unwrap();
        db.call(|c| {
            let (r1, r2) = setup(c);
            let run_id = run(c, "jobs:\n  g:\n    runs-on: [gpu]\n    steps: [train]\n  h:\n    steps: [echo]\n");
            let plain = claim(c, r1, &[], false, false).unwrap().unwrap();
            assert_eq!(plain.job.key, "h", "a runner without the label skips g");
            assert!(claim(c, r1, &[], false, false).unwrap().is_none());
            let gpu = claim(c, r2, &["gpu".into()], false, false).unwrap().unwrap();
            assert_eq!(gpu.job.key, "g");
            cancel(c, run_id).unwrap();
            assert!(!heartbeat(c, gpu.job_id, &gpu.token).unwrap());
            assert!(!finish(c, plain.job_id, &plain.token, true, Some(0)).unwrap());
            assert_eq!(status(c, "SELECT status FROM ci_runs"), "cancelled");
            Ok(())
        })
        .await
        .unwrap();
    }
}
