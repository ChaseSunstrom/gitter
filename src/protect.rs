//! Branch rules. The branches matching a rule's pattern (`main`, `release/*`)
//! can require pull requests, with approving reviews and passing checks, and
//! refuse force-pushes and deletion. Repository admins pass unless the rule is
//! enforced on them too.
//!
//! One rule set, three doors: git pushes are checked by a pre-receive hook
//! (git runs this binary as `pre-receive` with the rules that bind the pusher
//! in its environment), web commits and branch deletion call `direct`, and
//! merges call `gate`.

use std::io::Read;
use std::path::PathBuf;
use std::sync::OnceLock;

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::ci::config::glob;
use crate::error::{AppError, R};
use crate::repo::Repo;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Rule {
    #[serde(default)]
    pub id: i64,
    pub pattern: String,
    /// No direct pushes or web commits: changes arrive by merging a pull request.
    #[serde(default = "yes")]
    pub require_pr: bool,
    /// Approving reviews from people with write access a merge needs.
    #[serde(default)]
    pub approvals: i64,
    /// CI jobs or commit status contexts that must pass on the pull request's head.
    #[serde(default)]
    pub checks: Vec<String>,
    #[serde(default)]
    pub allow_force_push: bool,
    #[serde(default)]
    pub allow_deletion: bool,
    /// The rule binds repository admins too.
    #[serde(default)]
    pub enforce_admins: bool,
}

fn yes() -> bool {
    true
}

pub const MAX_RULES: i64 = 50;

impl Rule {
    pub fn validate(&self) -> R<()> {
        let p = &self.pattern;
        if p.is_empty() || p.len() > 200 || p.split('/').any(str::is_empty) || !p.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_./*?".contains(&b)) {
            return Err(AppError::bad("A branch pattern uses letters, digits, '-', '_', '.', '/' and wildcards (* within a part, ** across parts)"));
        }
        if !(0..=10).contains(&self.approvals) {
            return Err(AppError::bad("Required approvals go from 0 to 10"));
        }
        if self.checks.len() > 20 || self.checks.iter().any(|c| c.trim().is_empty() || c.len() > 100) {
            return Err(AppError::bad("Up to 20 required checks, each named in up to 100 characters"));
        }
        Ok(())
    }
}

pub fn load(c: &Connection, repo_id: i64) -> rusqlite::Result<Vec<Rule>> {
    let mut q = c.prepare_cached(
        "SELECT id, pattern, require_pr, approvals, checks, allow_force_push, allow_deletion, enforce_admins FROM branch_rules WHERE repo_id = ?1 ORDER BY pattern",
    )?;
    q.query_map([repo_id], |r| {
        Ok(Rule {
            id: r.get(0)?,
            pattern: r.get(1)?,
            require_pr: r.get(2)?,
            approvals: r.get(3)?,
            checks: serde_json::from_str(&r.get::<_, String>(4)?).unwrap_or_default(),
            allow_force_push: r.get(5)?,
            allow_deletion: r.get(6)?,
            enforce_admins: r.get(7)?,
        })
    })?
    .collect()
}

/// Every rule matching `branch` at once: the strictest of each setting.
pub fn effective(rules: &[Rule], branch: &str) -> Option<Rule> {
    let mut m = rules.iter().filter(|r| glob(&r.pattern, branch));
    let first = m.next()?.clone();
    Some(m.fold(first, |mut a, r| {
        a.pattern = format!("{}, {}", a.pattern, r.pattern);
        a.require_pr |= r.require_pr;
        a.approvals = a.approvals.max(r.approvals);
        for c in &r.checks {
            if !a.checks.contains(c) {
                a.checks.push(c.clone());
            }
        }
        a.allow_force_push &= r.allow_force_push;
        a.allow_deletion &= r.allow_deletion;
        a.enforce_admins |= r.enforce_admins;
        a
    }))
}

/// The rules that bind `repo`'s viewer (admins pass the ones not enforced on them).
async fn binding(st: &AppState, repo: &Repo) -> R<Vec<Rule>> {
    let id = repo.id;
    let rules = st.db.call(move |c| Ok(load(c, id)?)).await?;
    Ok(rules.into_iter().filter(|r| !repo.can_admin || r.enforce_admins).collect())
}

fn protected(msg: String) -> AppError {
    AppError::new(axum::http::StatusCode::FORBIDDEN, msg)
}

/// A web commit to `branch`, or deleting it, outside a merge.
pub async fn direct(st: &AppState, repo: &Repo, branch: &str, delete: bool) -> R<()> {
    let Some(rule) = effective(&binding(st, repo).await?, branch) else { return Ok(()) };
    if delete && !rule.allow_deletion {
        return Err(protected(format!("{branch} is protected: it can't be deleted")));
    }
    if !delete && rule.require_pr {
        return Err(protected(format!("{branch} is protected: changes go through a pull request")));
    }
    Ok(())
}

// ---------------------------------------------------------------- checks

#[derive(Clone, Debug, Serialize)]
pub struct Check {
    pub name: String,
    /// success, failure or pending.
    pub state: String,
    pub description: String,
    pub url: String,
    /// "ci" (a job here) or "status" (posted from outside).
    pub kind: &'static str,
}

/// What ran on commit `sha`: each CI job's latest attempt (across the runs of
/// that commit) and each commit status. A skipped job counts as passed.
pub fn checks(c: &Connection, repo_id: i64, sha: &str, repo_url: &str) -> rusqlite::Result<Vec<Check>> {
    let mut out: Vec<Check> = Vec::new();
    let mut q = c.prepare_cached(
        "SELECT j.name, j.status, r.number, r.approval FROM ci_jobs j JOIN ci_runs r ON r.id = j.run_id WHERE r.repo_id = ?1 AND r.sha = ?2 ORDER BY j.id",
    )?;
    for row in q.query_map(params![repo_id, sha], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, Option<String>>(3)?)))? {
        let (name, status, number, approval) = row?;
        let state = match status.as_str() {
            "success" | "skipped" => "success",
            "failure" | "error" | "cancelled" => "failure",
            _ => "pending",
        };
        let description = if approval.as_deref() == Some("needed") { "Waiting for approval".to_string() } else { status };
        let check = Check { name: name.clone(), state: state.into(), description, url: format!("{repo_url}/ci/{number}"), kind: "ci" };
        match out.iter_mut().find(|x| x.name == name) {
            Some(x) => *x = check,
            None => out.push(check),
        }
    }
    let mut q = c.prepare_cached("SELECT context, state, description, target_url FROM commit_statuses WHERE repo_id = ?1 AND sha = ?2 ORDER BY context")?;
    for row in q.query_map(params![repo_id, sha], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?)))? {
        let (name, state, description, url) = row?;
        let state = if state == "error" { "failure".to_string() } else { state };
        out.retain(|x| x.name != name);
        out.push(Check { name, state, description, url, kind: "status" });
    }
    Ok(out)
}

/// One state for a list of checks: failure, pending, success, or none.
pub fn combined(checks: &[Check]) -> &'static str {
    if checks.is_empty() {
        "none"
    } else if checks.iter().any(|c| c.state == "failure") {
        "failure"
    } else if checks.iter().any(|c| c.state == "pending") {
        "pending"
    } else {
        "success"
    }
}

// ---------------------------------------------------------------- merges

/// What stands between a pull request and its merge, for `repo`'s viewer.
#[derive(Debug, Serialize)]
pub struct Gate {
    /// The rules on the target branch, combined (shown to everyone).
    pub rule: Option<Rule>,
    /// Unmet requirements, in words. Empty: nothing in the way.
    pub missing: Vec<String>,
    /// The viewer is a repository admin the rule isn't enforced on: they may merge anyway.
    pub can_bypass: bool,
    pub checks: Vec<Check>,
}

pub async fn gate(st: &AppState, repo: &Repo, issue_id: i64, base_branch: &str, head_sha: &str) -> R<Gate> {
    let (rid, sha, url, r2) = (repo.id, head_sha.to_string(), format!("/{}/{}", repo.owner, repo.name), repo.clone());
    let base = base_branch.to_string();
    st.db
        .call(move |c| {
            let checks = checks(c, rid, &sha, &url)?;
            let rule = effective(&load(c, rid)?, &base);
            let mut missing = Vec::new();
            if let Some(rule) = &rule {
                for name in &rule.checks {
                    match checks.iter().find(|x| &x.name == name) {
                        Some(x) if x.state == "success" => {}
                        Some(x) => missing.push(format!("{name} hasn't passed ({})", x.description)),
                        None => missing.push(format!("{name} hasn't run")),
                    }
                }
                if rule.approvals > 0 {
                    let (approved, changes) = reviews(c, &r2, issue_id)?;
                    if approved < rule.approvals {
                        missing.push(format!("{} of {} approving reviews from people with write access", approved, rule.approvals));
                    }
                    for who in changes {
                        missing.push(format!("{who} requested changes"));
                    }
                }
            }
            let can_bypass = repo_admin_passes(&r2, rule.as_ref()) && !missing.is_empty();
            Ok(Gate { rule, missing, can_bypass, checks })
        })
        .await
}

fn repo_admin_passes(repo: &Repo, rule: Option<&Rule>) -> bool {
    repo.can_admin && rule.is_some_and(|r| !r.enforce_admins)
}

/// Approvals from people with write access (each reviewer's latest decision),
/// and the names of writers whose latest decision asks for changes.
fn reviews(c: &Connection, repo: &Repo, issue_id: i64) -> rusqlite::Result<(i64, Vec<String>)> {
    let mut q = c.prepare_cached(
        "SELECT v.author_id, u.username, v.state FROM reviews v JOIN users u ON u.id = v.author_id
         WHERE v.issue_id = ?1 AND v.state IN ('approved', 'changes_requested') AND NOT EXISTS (
             SELECT 1 FROM reviews x WHERE x.issue_id = v.issue_id AND x.author_id = v.author_id AND x.state IN ('approved', 'changes_requested')
             AND (x.submitted_at > v.submitted_at OR (x.submitted_at = v.submitted_at AND x.id > v.id)))
         ORDER BY u.username",
    )?;
    let rows: Vec<(i64, String, String)> = q.query_map([issue_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<Result<_, _>>()?;
    let (mut approved, mut changes) = (0, Vec::new());
    for (uid, name, state) in rows {
        if !crate::repo::writes(c, repo, uid)? {
            continue;
        }
        if state == "approved" {
            approved += 1;
        } else {
            changes.push(name);
        }
    }
    Ok((approved, changes))
}

// ---------------------------------------------------------------- the git hook

/// The binary git runs as the pre-receive hook (tests point it at the built `gitter`).
pub static HOOK_EXE: OnceLock<PathBuf> = OnceLock::new();

/// Git config and environment for a push by `repo`'s viewer: the hook folder
/// and the rules binding them, or None when no rule does (no hook at all).
pub async fn push_hook(st: &AppState, repo: &Repo) -> R<Option<(PathBuf, String)>> {
    let rules = binding(st, repo).await?;
    if rules.is_empty() {
        return Ok(None);
    }
    let dir = st.cfg.data_dir.join("hooks");
    let exe = HOOK_EXE.get().cloned().or_else(|| std::env::current_exe().ok()).ok_or_else(|| AppError::internal("can't find the server binary for the git hook"))?;
    let link = dir.join("pre-receive");
    // A link to this binary, not a script: nothing to quote, and it works on a noexec data volume.
    if std::fs::read_link(&link).ok() != Some(exe.clone()) {
        std::fs::create_dir_all(&dir)?;
        let tmp = dir.join(format!(".pre-receive.{}", std::process::id()));
        let _ = std::fs::remove_file(&tmp);
        std::os::unix::fs::symlink(&exe, &tmp)?;
        std::fs::rename(&tmp, &link)?;
    }
    Ok(Some((dir, serde_json::to_string(&rules).map_err(AppError::internal)?)))
}

/// Run as git's pre-receive hook: "old new ref" lines on stdin; refuse the
/// whole push (exit 1, the reasons go to the pusher) when one breaks a rule in
/// GITTER_RULES. A new branch is always allowed.
pub fn pre_receive() -> i32 {
    let Ok(raw) = std::env::var("GITTER_RULES") else { return 0 };
    let Ok(rules) = serde_json::from_str::<Vec<Rule>>(&raw) else {
        eprintln!("Gitter couldn't read this repository's branch rules, so it refuses the push.");
        return 1;
    };
    let mut input = String::new();
    if std::io::stdin().read_to_string(&mut input).is_err() {
        return 1;
    }
    let hex = |s: &str| (40..=64).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_hexdigit());
    let zero = |s: &str| s.bytes().all(|b| b == b'0');
    let mut refused = Vec::new();
    for line in input.lines() {
        let mut f = line.split_whitespace();
        let (Some(old), Some(new), Some(name)) = (f.next(), f.next(), f.next()) else { continue };
        let Some(branch) = name.strip_prefix("refs/heads/") else { continue };
        let Some(rule) = effective(&rules, branch) else { continue };
        if !hex(old) || !hex(new) || zero(old) {
            continue;
        }
        if zero(new) {
            if !rule.allow_deletion {
                refused.push(format!("{branch} is protected: it can't be deleted"));
            }
        } else if rule.require_pr {
            refused.push(format!("{branch} is protected: changes go through a pull request"));
        } else if !rule.allow_force_push && !ancestor(old, new) {
            refused.push(format!("{branch} is protected: force-pushes aren't allowed"));
        }
    }
    for m in &refused {
        eprintln!("{m}");
    }
    i32::from(!refused.is_empty())
}

/// Whether `old` is an ancestor of `new` (a fast-forward). Git sets up the
/// repository and the pushed objects in the hook's environment.
fn ancestor(old: &str, new: &str) -> bool {
    std::process::Command::new("git").args(["merge-base", "--is-ancestor", old, new]).status().is_ok_and(|s| s.success())
}

// ---------------------------------------------------------------- auto-merge

/// Merge the open pull requests in `repo_id` whose head is at `sha` and that
/// wait to merge themselves, once nothing stands in the way. Called when a run
/// finishes, a status arrives or a review is submitted.
pub async fn auto_merge(st: &AppState, repo_id: i64, sha: Option<String>) {
    if let Err(e) = try_auto_merge(st, repo_id, sha).await {
        eprintln!("auto-merge in repository {repo_id}: {}", e.msg);
    }
}

async fn try_auto_merge(st: &AppState, repo_id: i64, sha: Option<String>) -> R<()> {
    let waiting: Vec<(i64, String, String, i64, String)> = st
        .db
        .call(move |c| {
            let mut q = c.prepare(
                "SELECT i.number, p.head_branch, p.auto_merge, p.auto_merge_by, p.base_branch FROM pulls p JOIN issues i ON i.id = p.issue_id
                 WHERE i.repo_id = ?1 AND i.state = 'open' AND p.auto_merge IS NOT NULL AND p.auto_merge_by IS NOT NULL",
            )?;
            Ok(q.query_map([repo_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?.collect::<Result<_, _>>()?)
        })
        .await?;
    for (number, head, method, by, _base) in waiting {
        let Some(user) = crate::auth::user_by_id(&st.db, by).await? else { continue };
        let found = st.db.call(move |c| Ok(c.query_row("SELECT u.username, r.name FROM repos r JOIN users u ON u.id = r.owner_id WHERE r.id = ?1", [repo_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))).optional()?)).await?;
        let Some((owner, name)) = found else { return Ok(()) };
        let Ok(repo) = Repo::writable(st, Some(&user), &owner, &name).await else {
            // Whoever asked can't merge any more: the request lapses.
            clear(st, repo_id, number).await?;
            continue;
        };
        let Some(head_sha) = st.git.resolve(&repo.path, &format!("refs/heads/{head}")).await? else { continue };
        if sha.as_ref().is_some_and(|s| *s != head_sha) {
            continue;
        }
        match crate::api::issues::merge(st, &repo, &user, number, &method).await {
            Ok(_) => {}
            // Not yet (checks pending, approvals missing, conflicts): it stays armed.
            Err(e) if e.status == axum::http::StatusCode::CONFLICT => {}
            Err(e) => {
                clear(st, repo_id, number).await?;
                eprintln!("auto-merge of #{number} in {owner}/{name}: {}", e.msg);
            }
        }
    }
    Ok(())
}

async fn clear(st: &AppState, repo_id: i64, number: i64) -> R<()> {
    st.db
        .call(move |c| {
            Ok(c.execute(
                "UPDATE pulls SET auto_merge = NULL, auto_merge_by = NULL WHERE issue_id = (SELECT id FROM issues WHERE repo_id = ?1 AND number = ?2)",
                params![repo_id, number],
            )?)
        })
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(p: &str) -> Rule {
        Rule { id: 0, pattern: p.into(), require_pr: false, approvals: 0, checks: vec![], allow_force_push: true, allow_deletion: true, enforce_admins: false }
    }

    #[test]
    fn rules_combine_to_the_strictest() {
        let mut a = rule("release/*");
        a.checks = vec!["build".into()];
        a.allow_deletion = false;
        let mut b = rule("**");
        b.approvals = 2;
        b.checks = vec!["build".into(), "lint".into()];
        b.allow_force_push = false;
        let rules = [a, b];
        let e = effective(&rules, "release/1.0").unwrap();
        assert_eq!((e.approvals, e.checks.len(), e.allow_force_push, e.allow_deletion), (2, 2, false, false));
        assert_eq!(effective(&rules, "main").unwrap().checks, ["build", "lint"]);
        assert!(effective(&[rule("release/*")], "release/1/x").is_none(), "* stays within a part");
        assert!(rule("a b").validate().is_err() && rule("x//y").validate().is_err() && rule("feat/**").validate().is_ok());
    }
}
