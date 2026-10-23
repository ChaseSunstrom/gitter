//! When pipelines run: pushes, pull requests, the Run button and schedules.

use std::path::Path;

use super::compat;
use super::config::{self, Filter, Pipeline, any_glob, glob};
use super::queue::{self, NewRun};
use crate::AppState;
use crate::error::R;
use crate::repo::{REPO_SELECT, Repo, row_to_repo};
use rusqlite::OptionalExtension;
use crate::smart_http::RefUpdate;

/// More changed files than this and path filters treat the push as touching everything.
const MAX_CHANGED: usize = 5000;
/// A pull request run for the same commit started this recently is the same run, finished or not.
const SAME_RUN_SECS: i64 = 30;

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

/// The pipeline at `sha`: `.gitter/ci.yml`, else `.gitter/ci.sh`, else the
/// GitHub workflows, else `.gitlab-ci.yml` (translated), else none.
/// Err is a broken config, shown on the run it would have started.
pub async fn pipeline_at(st: &AppState, repo: &Path, sha: &str, default_branch: &str) -> R<Option<Result<Pipeline, String>>> {
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
    // Other hosts' files.
    let mut workflows = Vec::new();
    for e in st.git.ls_tree(repo, sha, compat::GITHUB_DIR).await? {
        if e.kind != "blob" || !(e.name.ends_with(".yml") || e.name.ends_with(".yaml")) {
            continue;
        }
        if workflows.len() == compat::MAX_WORKFLOWS {
            return Ok(Some(Err(format!("{}: at most {} workflow files", compat::GITHUB_DIR, compat::MAX_WORKFLOWS))));
        }
        match text_at(st, repo, &e.path, &e.oid, e.size).await? {
            Ok(t) => workflows.push((e.path.clone(), t)),
            Err(e) => return Ok(Some(Err(e))),
        }
    }
    // Workflows that never run here (tags, releases, reusable ones) are no pipeline.
    if !workflows.is_empty()
        && let Some(p) = compat::github(&workflows).transpose()
    {
        return Ok(Some(p));
    }
    if let Some((oid, kind, size)) = st.git.stat(repo, sha, compat::GITLAB_FILE).await?
        && kind == "blob"
    {
        return Ok(Some(match text_at(st, repo, compat::GITLAB_FILE, &oid, Some(size)).await? {
            Ok(t) => compat::gitlab(&t, default_branch),
            Err(e) => Err(e),
        }));
    }
    Ok(None)
}

/// A pipeline file's text, within the size limit.
async fn text_at(st: &AppState, repo: &Path, path: &str, oid: &str, size: Option<u64>) -> R<Result<String, String>> {
    if size.unwrap_or(0) > config::MAX_CONFIG_BYTES {
        return Ok(Err(format!("{path} is larger than {} KB", config::MAX_CONFIG_BYTES / 1024)));
    }
    let data = st.git.blob(repo, oid).await?;
    Ok(String::from_utf8(data).map_err(|_| format!("{path} isn't UTF-8 text")))
}

/// The jobs that run for this event. Jobs from GitHub workflows or GitLab rules
/// carry their own triggers: only those wanting this event stay (and `needs` on
/// jobs left out are dropped, as GitLab does). None: nothing runs.
pub fn pick(p: Result<Pipeline, String>, event: &str, branch: &str, files: &Option<Vec<String>>) -> Option<Result<Pipeline, String>> {
    let Ok(p) = p else { return Some(p) };
    let wanted = |t: &config::Triggers| match event {
        "push" => t.push.as_ref().is_some_and(|f| wants(f, branch, files)),
        "pull_request" | "merge_queue" => t.pull_request.as_ref().is_some_and(|f| wants(f, branch, files)),
        "manual" => t.manual,
        // ponytail: a schedule runs every workflow that has one, not only the one whose cron fired.
        "schedule" => !t.schedule.is_empty(),
        _ => true,
    };
    // A job left out hands its own needs on, so order holds across it (GitLab stages).
    let needs_of: std::collections::HashMap<String, Vec<String>> = p.jobs.iter().map(|j| (j.key.clone(), j.needs.clone())).collect();
    let mut jobs: Vec<config::Job> = p.jobs.into_iter().filter(|j| j.when.as_ref().is_none_or(wanted)).collect();
    if jobs.len() == needs_of.len() {
        return Some(Ok(Pipeline { triggers: p.triggers, jobs }));
    }
    let keys: Vec<String> = jobs.iter().map(|j| j.key.clone()).collect();
    for j in &mut jobs {
        let (mut out, mut todo, mut seen) = (Vec::new(), j.needs.clone(), std::collections::HashSet::new());
        while let Some(n) = todo.pop() {
            if !seen.insert(n.clone()) {
                continue;
            }
            if keys.contains(&n) {
                out.push(n);
            } else if let Some(more) = needs_of.get(&n) {
                todo.extend(more.iter().cloned());
            }
        }
        out.sort();
        out.dedup();
        j.needs = out;
    }
    (!jobs.is_empty()).then_some(Ok(Pipeline { triggers: p.triggers, jobs }))
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
    // A run an agent started, or any run of a branch an agent created (whoever
    // pushed to it), is marked: no secrets, never the built-in runner (`jobs::claim`).
    let (agent, repo_id) = (crate::auth::acting_agent(), repo.id);
    let branch = run.refname.strip_prefix("refs/heads/").unwrap_or_default().to_string();
    let (number, failed) = st
        .db
        .call(move |c| {
            // The push hook and the opening of a pull request can both start its run for the same
            // commit (one database call, so nothing slips in between): the second keeps the first,
            // even one that already finished moments ago. A run that broke on its config ended at
            // once, and would break the same way again.
            if run.event == "pull_request" {
                let same: Option<i64> = c
                    .query_row(
                        "SELECT number FROM ci_runs WHERE repo_id = ?1 AND event = 'pull_request' AND pr_number = ?2 AND sha = ?3
                         AND (status IN ('queued', 'running') OR error != '' OR created_at >= ?4) ORDER BY id DESC LIMIT 1",
                        rusqlite::params![run.repo_id, run.pr_number, run.sha, crate::db::now() - SAME_RUN_SECS],
                        |r| r.get(0),
                    )
                    .optional()?;
                if let Some(n) = same {
                    return Ok((n, None));
                }
            }
            let (n, failed) = match pipeline {
                Ok(p) => (queue::create_run(c, &run, &p.jobs, &skip)?.1, None),
                Err(e) => {
                    let (id, n) = queue::create_failed_run(c, &run, &e)?;
                    (n, Some(id))
                }
            };
            // An agent's run also waits for a person to approve it (they read what it will run).
            c.execute(
                "UPDATE ci_runs SET agent = COALESCE(?1, (SELECT agent FROM agent_branches WHERE repo_id = ?2 AND branch = ?4),
                     (SELECT a.agent FROM agent_branches a JOIN pulls p ON p.head_branch = a.branch AND p.from_fork = 0 JOIN issues i ON i.id = p.issue_id
                      WHERE a.repo_id = ?2 AND i.repo_id = ?2 AND i.number = ci_runs.pr_number))
                 WHERE repo_id = ?2 AND number = ?3",
                rusqlite::params![agent, repo_id, n, branch],
            )?;
            c.execute("UPDATE ci_runs SET approval = 'needed' WHERE repo_id = ?1 AND number = ?2 AND agent IS NOT NULL AND status = 'queued'", rusqlite::params![repo_id, n])?;
            Ok((n, failed))
        })
        .await?;
    st.ci_wake.notify_waiters();
    // A run whose config broke ended without a job to finish it.
    // The run is recorded either way: a follow-up's error mustn't fail whoever started it.
    if let Some(run) = failed
        && let Err(e) = super::jobs::run_ended(st, run).await
    {
        eprintln!("ci: {} run {number}: follow-ups: {}", repo.full_name(), e.msg);
    }
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
    let found = pipeline_at(st, &repo.path, new, &repo.default_branch).await?;
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
    if (pipeline.is_err() || triggers.push.as_ref().is_some_and(|f| wants(f, branch, &files)))
        && let Some(p) = pick(pipeline.clone(), "push", branch, &files)
    {
        let s = Start { event: "push", refname: u.refname.clone(), sha: new.into(), pr_number: None, actor_id: Some(actor_id), files: files.clone() };
        start(st, repo, p, s).await?;
    }
    let Some(f) = triggers.pull_request else { return Ok(()) };
    let (id, b) = (repo.id, branch.to_string());
    let prs: Vec<(i64, String)> = st
        .db
        .call(move |c| {
            let mut q = c.prepare(
                "SELECT i.number, p.base_branch FROM pulls p JOIN issues i ON i.id = p.issue_id
                 WHERE i.repo_id = ?1 AND i.state = 'open' AND p.head_branch = ?2 AND p.from_fork = 0",
            )?;
            Ok(q.query_map(rusqlite::params![id, b], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?)
        })
        .await?;
    for (number, base) in prs {
        let files = pull_files(st, &repo.path, &base, new).await?;
        if wants(&f, &base, &files)
            && let Some(p) = pick(pipeline.clone(), "pull_request", &base, &files)
        {
            let s = Start { event: "pull_request", refname: u.refname.clone(), sha: new.into(), pr_number: Some(number), actor_id: Some(actor_id), files };
            start(st, repo, p, s).await?;
        }
    }
    Ok(())
}

/// A pull request was opened: run its pipeline for the files it changes.
/// `repo` carries the opener's access: only people who can push start runs
/// (a run gets the repository's secrets).
pub async fn on_pull_opened(st: &AppState, repo: &Repo, _opener: i64, number: i64, head: &str, base: &str) -> R<()> {
    if !repo.can_write || !enabled(st, repo.owner_id).await {
        return Ok(());
    }
    let Some(head_sha) = st.git.resolve(&repo.path, &format!("refs/heads/{head}")).await? else { return Ok(()) };
    let Some(pipeline) = pipeline_at(st, &repo.path, &head_sha, &repo.default_branch).await? else { return Ok(()) };
    let wanted = match &pipeline {
        Ok(p) => p.triggers.pull_request.clone(),
        Err(_) => Some(Filter::default()),
    };
    let Some(f) = wanted else { return Ok(()) };
    let files = pull_files(st, &repo.path, base, &head_sha).await?;
    if wants(&f, base, &files)
        && let Some(pipeline) = pick(pipeline, "pull_request", base, &files)
    {
        // The run is the head branch's code, so it's on behalf of whoever last pushed it,
        // not whoever opened the pull request (an admin opening one doesn't vouch for it).
        let s = Start { event: "pull_request", refname: format!("refs/heads/{head}"), sha: head_sha, pr_number: Some(number), actor_id: pusher(st, repo.id, head).await?, files };
        start(st, repo, pipeline, s).await?;
    }
    Ok(())
}

/// The files a pull request onto `base` changes at `head`: since where it left the
/// base branch, not since the last push (both starters must build the same run).
async fn pull_files(st: &AppState, repo: &Path, base: &str, head: &str) -> R<Option<Vec<String>>> {
    let from = match st.git.resolve(repo, &format!("refs/heads/{base}")).await? {
        Some(b) => st.git.merge_base(repo, &b, head).await?,
        None => None,
    };
    Ok(changed(st, repo, from.as_deref(), head).await)
}

/// Who last pushed `branch` (its runs are on their behalf).
async fn pusher(st: &AppState, repo_id: i64, branch: &str) -> R<Option<i64>> {
    let b = branch.to_string();
    st.db
        .call(move |c| Ok(c.query_row("SELECT user_id FROM branch_pushers WHERE repo_id = ?1 AND branch = ?2", rusqlite::params![repo_id, b], |r| r.get(0)).optional()?))
        .await
}

/// A merge queue's commit: a pull request merged onto its target branch's tip.
pub struct Queued<'a> {
    pub entry: i64,
    pub number: i64,
    pub head: &'a str,
    pub base: &'a str,
    pub base_sha: &'a str,
    pub sha: &'a str,
}

/// Run the pipeline on a merge queue's commit, as the pull request's own runs
/// would be (on behalf of whoever last pushed its branch).
pub async fn on_queue(st: &AppState, repo: &Repo, q: Queued<'_>) -> R<()> {
    if !enabled(st, repo.owner_id).await {
        return Ok(());
    }
    let Some(pipeline) = pipeline_at(st, &repo.path, q.sha, &repo.default_branch).await? else { return Ok(()) };
    let wanted = match &pipeline {
        Ok(p) => p.triggers.pull_request.clone(),
        Err(_) => Some(Filter::default()),
    };
    let Some(f) = wanted else { return Ok(()) };
    let files = changed(st, &repo.path, Some(q.base_sha), q.sha).await;
    if wants(&f, q.base, &files)
        && let Some(pipeline) = pick(pipeline, "merge_queue", q.base, &files)
    {
        let refname = format!("{}/{}", crate::mergequeue::REF_PREFIX, q.entry);
        let s = Start { event: "merge_queue", refname, sha: q.sha.into(), pr_number: Some(q.number), actor_id: pusher(st, repo.id, q.head).await?, files };
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
            .call(move |c| Ok(c.query_row(&format!("{REPO_SELECT} WHERE r.id = ?1 AND r.deleted_at IS NULL"), [id], row_to_repo).optional()?))
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
    if let Some(p) = pipeline_at(st, &repo.path, &sha, &repo.default_branch).await?.and_then(|p| pick(p, "schedule", &repo.default_branch, &None)) {
        // A scheduled run is on behalf of whoever last moved the branch.
        let (id, b) = (repo.id, repo.default_branch.clone());
        let actor_id: Option<i64> = st
            .db
            .call(move |c| Ok(c.query_row("SELECT user_id FROM branch_pushers WHERE repo_id = ?1 AND branch = ?2", rusqlite::params![id, b], |r| r.get(0)).optional()?))
            .await?;
        let s = Start { event: "schedule", refname, sha, pr_number: None, actor_id, files: None };
        start(st, repo, p, s).await?;
    }
    Ok(())
}
