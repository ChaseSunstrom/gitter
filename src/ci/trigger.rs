//! When pipelines run: pushes, pull requests, the Run button and schedules.

use std::path::Path;

use super::config::{self, Filter, Pipeline, any_glob, glob};
use super::queue::{self, NewRun};
use crate::AppState;
use crate::error::R;
use crate::repo::{REPO_SELECT, Repo, row_to_repo};
use rusqlite::OptionalExtension;
use crate::smart_http::RefUpdate;

/// More changed files than this and path filters treat the push as touching everything.
const MAX_CHANGED: usize = 5000;

/// May this repository run CI under the site's setting?
pub async fn enabled(st: &AppState, owner_id: i64) -> bool {
    match st.settings.get().ci.as_str() {
        "everyone" => true,
        "admins" => st
            .db
            .call(move |c| Ok(c.query_row(&format!("SELECT {} FROM users u WHERE u.id = ?1", super::queue::ADMIN_OWNED), [owner_id], |r| r.get::<_, bool>(0))?))
            .await
            .unwrap_or(false),
        _ => false,
    }
}

/// The pipeline at `sha`: `.gitter/ci.yml`, else `.gitter/ci.sh`, else none.
/// Err is a broken config, shown on the run it would have started.
pub async fn pipeline_at(st: &AppState, repo: &Path, sha: &str) -> R<Option<Result<Pipeline, String>>> {
    if let Some((oid, kind, size)) = st.git.stat(repo, sha, config::CONFIG_PATH).await?
        && kind == "blob"
    {
        if size > config::MAX_CONFIG_BYTES {
            return Ok(Some(Err(format!("{} is larger than {} KB", config::CONFIG_PATH, config::MAX_CONFIG_BYTES / 1024))));
        }
        let data = st.git.blob(repo, &oid).await?;
        return Ok(Some(match std::str::from_utf8(&data) {
            Ok(text) => config::parse(text),
            Err(_) => Err(format!("{} isn't UTF-8 text", config::CONFIG_PATH)),
        }));
    }
    if let Some((_, kind, _)) = st.git.stat(repo, sha, config::SCRIPT_PATH).await?
        && kind == "blob"
    {
        return Ok(Some(Ok(config::script_pipeline())));
    }
    Ok(None)
}

/// Files changed between `old` and `new`; None when every file counts (a new
/// branch, or too many to list).
async fn changed(st: &AppState, repo: &Path, old: Option<&str>, new: &str) -> Option<Vec<String>> {
    let old = old?;
    let out = st.git.ok(repo, &["diff", "--name-only", "-z", "--no-renames", old, new, "--"]).await.ok()?;
    let files: Vec<String> = out
        .split(|b| *b == 0)
        .filter(|f| !f.is_empty())
        .map(|f| String::from_utf8_lossy(f).into_owned())
        .collect();
    (files.len() <= MAX_CHANGED).then_some(files)
}

fn wants(f: &Filter, branch: &str, files: &Option<Vec<String>>) -> bool {
    let branch_ok = f.branches.is_empty() || f.branches.iter().any(|g| glob(g, branch));
    let paths_ok = f.paths.is_empty() || files.as_ref().is_none_or(|c| any_glob(&f.paths, c));
    branch_ok && paths_ok
}

pub struct Start<'a> {
    pub event: &'a str,
    pub refname: String,
    pub sha: String,
    pub pr_number: Option<i64>,
    pub actor_id: Option<i64>,
    pub files: Option<Vec<String>>,
}

/// Queue a run of `pipeline` (or record why it couldn't run) and wake the runners.
pub async fn start(st: &AppState, repo: &Repo, pipeline: Result<Pipeline, String>, s: Start<'_>) -> R<i64> {
    let title = st.git.commit_info(&repo.path, &s.sha).await?.map(|c| c.subject).unwrap_or_default();
    let run = NewRun {
        repo_id: repo.id,
        event: s.event.to_string(),
        refname: s.refname,
        sha: s.sha,
        pr_number: s.pr_number,
        actor_id: s.actor_id,
        title,
    };
    // A job whose own `paths` match none of the changed files is skipped
    // (matched here, not on the database thread).
    let skip: Vec<bool> = match &pipeline {
        Ok(p) => p.jobs.iter().map(|j| !j.paths.is_empty() && s.files.as_ref().is_some_and(|f| !any_glob(&j.paths, f))).collect(),
        Err(_) => vec![],
    };
    let number = st
        .db
        .call(move |c| {
            Ok(match pipeline {
                Ok(p) => queue::create_run(c, &run, &p.jobs, &skip)?.1,
                Err(e) => queue::create_failed_run(c, &run, &e)?,
            })
        })
        .await?;
    st.ci_wake.notify_waiters();
    Ok(number)
}

/// After a push, a web commit or a merge: runs for the pushed branches, for
/// open pull requests from them, and the schedule from the default branch.
pub async fn on_push(st: &AppState, repo: &Repo, actor_id: i64, updates: &[RefUpdate]) {
    if !enabled(st, repo.owner_id).await {
        return;
    }
    for u in updates {
        let (Some(new), Some(branch)) = (u.new.as_deref(), u.refname.strip_prefix("refs/heads/")) else { continue };
        if let Err(e) = on_branch(st, repo, actor_id, u, branch, new).await {
            eprintln!("ci: {} {}: {}", repo.full_name(), u.refname, e.msg);
        }
    }
}

async fn on_branch(st: &AppState, repo: &Repo, actor_id: i64, u: &RefUpdate, branch: &str, new: &str) -> R<()> {
    let found = pipeline_at(st, &repo.path, new).await?;
    if branch == repo.default_branch {
        let crons = match &found {
            Some(Ok(p)) => p.triggers.schedule.clone(),
            _ => vec![],
        };
        set_schedule(st, repo.id, crons).await?;
    }
    let Some(pipeline) = found else { return Ok(()) };
    let files = changed(st, &repo.path, u.old.as_deref(), new).await;
    let triggers = pipeline.as_ref().map(|p| p.triggers.clone()).unwrap_or_default();
    // A broken file shows up on the push that brought it.
    if pipeline.is_err() || triggers.push.as_ref().is_some_and(|f| wants(f, branch, &files)) {
        let s = Start { event: "push", refname: u.refname.clone(), sha: new.into(), pr_number: None, actor_id: Some(actor_id), files: files.clone() };
        start(st, repo, pipeline.clone(), s).await?;
    }
    let Some(f) = triggers.pull_request else { return Ok(()) };
    let (id, b) = (repo.id, branch.to_string());
    let prs: Vec<(i64, String)> = st
        .db
        .call(move |c| {
            let mut q = c.prepare(
                "SELECT i.number, p.base_branch FROM pulls p JOIN issues i ON i.id = p.issue_id
                 WHERE i.repo_id = ?1 AND i.state = 'open' AND p.head_branch = ?2",
            )?;
            Ok(q.query_map(rusqlite::params![id, b], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?)
        })
        .await?;
    for (number, base) in prs {
        if wants(&f, &base, &files) {
            let s = Start { event: "pull_request", refname: u.refname.clone(), sha: new.into(), pr_number: Some(number), actor_id: Some(actor_id), files: files.clone() };
            start(st, repo, pipeline.clone(), s).await?;
        }
    }
    Ok(())
}

/// A pull request was opened: run its pipeline for the files it changes.
/// `repo` carries the opener's access: only people who can push start runs
/// (a run gets the repository's secrets).
pub async fn on_pull_opened(st: &AppState, repo: &Repo, actor_id: i64, number: i64, head: &str, base: &str) -> R<()> {
    if !repo.can_write || !enabled(st, repo.owner_id).await {
        return Ok(());
    }
    let Some(head_sha) = st.git.resolve(&repo.path, &format!("refs/heads/{head}")).await? else { return Ok(()) };
    let Some(pipeline) = pipeline_at(st, &repo.path, &head_sha).await? else { return Ok(()) };
    let wanted = match &pipeline {
        Ok(p) => p.triggers.pull_request.clone(),
        Err(_) => Some(Filter::default()),
    };
    let Some(f) = wanted else { return Ok(()) };
    let base_sha = st.git.resolve(&repo.path, &format!("refs/heads/{base}")).await?;
    let from = match base_sha {
        Some(b) => st.git.merge_base(&repo.path, &b, &head_sha).await?,
        None => None,
    };
    let files = changed(st, &repo.path, from.as_deref(), &head_sha).await;
    if wants(&f, base, &files) {
        let s = Start { event: "pull_request", refname: format!("refs/heads/{head}"), sha: head_sha, pr_number: Some(number), actor_id: Some(actor_id), files };
        start(st, repo, pipeline, s).await?;
    }
    Ok(())
}

async fn set_schedule(st: &AppState, repo_id: i64, crons: Vec<String>) -> R<()> {
    st.db
        .call(move |c| {
            if crons.is_empty() {
                c.execute("DELETE FROM ci_schedules WHERE repo_id = ?1", [repo_id])?;
            } else {
                let json = serde_json::to_string(&crons).unwrap_or_default();
                c.execute(
                    "INSERT INTO ci_schedules(repo_id, crons) VALUES (?1, ?2) ON CONFLICT(repo_id) DO UPDATE SET crons = ?2",
                    rusqlite::params![repo_id, json],
                )?;
            }
            Ok(())
        })
        .await
}

/// Every minute: start the scheduled runs due this minute (once each, even
/// across restarts within the minute).
pub async fn tick(st: &AppState, now: i64) -> R<()> {
    let minute = now / 60;
    let due: Vec<i64> = st
        .db
        .call(move |c| {
            let mut q = c.prepare("SELECT repo_id, crons FROM ci_schedules WHERE last_minute < ?1")?;
            let rows: Vec<(i64, String)> = q.query_map([minute], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
            let mut due = Vec::new();
            for (id, crons) in rows {
                let crons: Vec<String> = serde_json::from_str(&crons).unwrap_or_default();
                if crons.iter().filter_map(|s| config::Cron::parse(s).ok()).any(|cr| cr.matches(now)) {
                    c.execute("UPDATE ci_schedules SET last_minute = ?2 WHERE repo_id = ?1", [id, minute])?;
                    due.push(id);
                }
            }
            Ok(due)
        })
        .await?;
    for id in due {
        let found = st
            .db
            .call(move |c| Ok(c.query_row(&format!("{REPO_SELECT} WHERE r.id = ?1"), [id], row_to_repo).optional()?))
            .await?;
        let Some(repo) = found.map(|r| Repo::with_access(st, r, None)) else { continue };
        if repo.hidden || !enabled(st, repo.owner_id).await {
            continue;
        }
        if let Err(e) = scheduled(st, &repo).await {
            eprintln!("ci schedule {}: {}", repo.full_name(), e.msg);
        }
    }
    Ok(())
}

async fn scheduled(st: &AppState, repo: &Repo) -> R<()> {
    let refname = format!("refs/heads/{}", repo.default_branch);
    let Some(sha) = st.git.resolve(&repo.path, &refname).await? else { return Ok(()) };
    if let Some(p) = pipeline_at(st, &repo.path, &sha).await? {
        let s = Start { event: "schedule", refname, sha, pr_number: None, actor_id: None, files: None };
        start(st, repo, p, s).await?;
    }
    Ok(())
}
