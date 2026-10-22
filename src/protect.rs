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
    /// Pull requests into these branches merge through the merge queue (`crate::mergequeue`).
    #[serde(default)]
    pub merge_queue: bool,
}

fn yes() -> bool {
    true
}

pub const MAX_RULES: i64 = 20;

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
        "SELECT id, pattern, require_pr, approvals, checks, allow_force_push, allow_deletion, enforce_admins, merge_queue FROM branch_rules WHERE repo_id = ?1 ORDER BY pattern",
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
            merge_queue: r.get(8)?,
        })
    })?
    .collect()
}

/// Whether a rule's pattern covers `branch`: `*` within one part of the name,
/// `**` across parts, and a pattern without `/` only names without one (`main`
/// is not `fix/main`).
pub fn covers(pattern: &str, branch: &str) -> bool {
    glob(pattern, branch) && (pattern.contains('/') || pattern.contains("**") || !branch.contains('/'))
}

/// Every rule matching `branch` at once: the strictest of each setting.
pub fn effective(rules: &[Rule], branch: &str) -> Option<Rule> {
    let mut m = rules.iter().filter(|r| covers(&r.pattern, branch));
    let first = m.next()?.clone();
    let mut rule = m.fold(first, |mut a, r| {
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
        a.merge_queue |= r.merge_queue;
        a
    });
    // A queue means nothing if changes can go around it.
    rule.require_pr |= rule.merge_queue;
    Some(rule)
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
        "SELECT j.name, j.status, r.number, r.approval, j.reason FROM ci_jobs j JOIN ci_runs r ON r.id = j.run_id WHERE r.repo_id = ?1 AND r.sha = ?2 ORDER BY j.id",
    )?;
    for row in q.query_map(params![repo_id, sha], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, Option<String>>(3)?, r.get::<_, String>(4)?))
    })? {
        let (name, status, number, approval, reason) = row?;
        let state = match status.as_str() {
            // Skipped because a job it needs failed: that's a failure, not a pass.
            "skipped" if reason == "needs" => "failure",
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
        // Any writer can post a status, so one named like a CI job here can only
        // make that check worse, never pass a job that didn't.
        if let Some(ci) = out.iter_mut().find(|x| x.name == name) {
            let rank = |s: &str| match s {
                "failure" => 2,
                "pending" => 1,
                _ => 0,
            };
            if rank(&state) > rank(&ci.state) {
                *ci = Check { name, state, description, url, kind: "status" };
            }
            continue;
        }
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
    /// The ones of them about checks (the merge queue checks its own commit instead).
    #[serde(skip)]
    pub missing_checks: Vec<String>,
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
            let (mut missing, mut missing_checks) = (Vec::new(), Vec::new());
            if let Some(rule) = &rule {
                for name in &rule.checks {
                    match checks.iter().find(|x| &x.name == name) {
                        Some(x) if x.state == "success" => {}
                        Some(x) => missing_checks.push(format!("{name} hasn't passed ({})", x.description)),
                        None => missing_checks.push(format!("{name} hasn't run")),
                    }
                }
                missing.extend(missing_checks.iter().cloned());
                if rule.approvals > 0 {
                    let (approved, changes) = reviews(c, &r2, issue_id, &sha)?;
                    if approved < rule.approvals {
                        missing.push(format!("{} of {} approving reviews from people with write access", approved, rule.approvals));
                    }
                    for who in changes {
                        missing.push(format!("{who} requested changes"));
                    }
                }
            }
            let queued = rule.as_ref().is_some_and(|x| x.merge_queue);
            let can_bypass = repo_admin_passes(&r2, rule.as_ref()) && (!missing.is_empty() || queued);
            Ok(Gate { rule, missing, missing_checks, can_bypass, checks })
        })
        .await
}

fn repo_admin_passes(repo: &Repo, rule: Option<&Rule>) -> bool {
    repo.can_admin && rule.is_some_and(|r| !r.enforce_admins)
}

/// Approvals of the current head (`sha`) from people with write access (each
/// reviewer's latest decision; a push after an approval needs a new one), and
/// the names of writers whose latest decision asks for changes.
fn reviews(c: &Connection, repo: &Repo, issue_id: i64, sha: &str) -> rusqlite::Result<(i64, Vec<String>)> {
    let mut q = c.prepare_cached(
        "SELECT v.author_id, u.username, v.state, v.commit_sha FROM reviews v JOIN users u ON u.id = v.author_id
         WHERE v.issue_id = ?1 AND v.state IN ('approved', 'changes_requested') AND NOT EXISTS (
             SELECT 1 FROM reviews x WHERE x.issue_id = v.issue_id AND x.author_id = v.author_id AND x.state IN ('approved', 'changes_requested')
             AND (x.submitted_at > v.submitted_at OR (x.submitted_at = v.submitted_at AND x.id > v.id)))
         ORDER BY u.username",
    )?;
    let rows: Vec<(i64, String, String, Option<String>)> = q.query_map([issue_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<Result<_, _>>()?;
    let (mut approved, mut changes) = (0, Vec::new());
    for (uid, name, state, at) in rows {
        if !crate::repo::writes(c, repo, uid)? {
            continue;
        }
        if state == "approved" {
            approved += i64::from(at.as_deref() == Some(sha));
        } else {
            changes.push(name);
        }
    }
    Ok((approved, changes))
}

// ---------------------------------------------------------------- the git hook

/// The binary git runs as the pre-receive hook (tests point it at the built `gitter`).
pub static HOOK_EXE: OnceLock<PathBuf> = OnceLock::new();

/// The rules binding `repo`'s viewer, for the hook (GITTER_RULES), or None.
pub async fn rules_env(st: &AppState, repo: &Repo) -> R<Option<String>> {
    let rules = binding(st, repo).await?;
    if rules.is_empty() {
        return Ok(None);
    }
    let json = serde_json::to_string(&rules).map_err(AppError::internal)?;
    // An environment variable has room for this many (MAX_RULES keeps it so).
    if json.len() > 100_000 {
        return Err(AppError::internal("too many branch rules to check this push"));
    }
    Ok(Some(json))
}

/// GITTER_DATA/hooks, holding `pre-receive`: a link to this binary, which git
/// runs before any ref of a push moves (branch rules, then push protection).
pub fn hook_dir(st: &AppState) -> R<PathBuf> {
    let dir = st.cfg.data_dir.join("hooks");
    // Set at startup (`main`): once the binary is replaced in place, current_exe() names a deleted file.
    let exe = HOOK_EXE.get().cloned().or_else(|| std::env::current_exe().ok()).ok_or_else(|| AppError::internal("can't find the server binary for the git hook"))?;
    // Git skips a hook it can't run, so a missing binary refuses the push instead.
    if !exe.is_file() {
        return Err(AppError::internal("the server binary moved: restart it so pushes can be checked"));
    }
    let link = dir.join("pre-receive");
    // A link, not a script: nothing to quote, and it works on a noexec data volume.
    if std::fs::read_link(&link).ok() != Some(exe.clone()) {
        std::fs::create_dir_all(&dir)?;
        let tmp = dir.join(format!(".pre-receive.{}.{:x}", std::process::id(), rand::random::<u64>()));
        std::os::unix::fs::symlink(&exe, &tmp)?;
        std::fs::rename(&tmp, &link)?;
    }
    Ok(dir)
}

/// Run as git's pre-receive hook: "old new ref" lines on stdin. Refuses the
/// whole push (exit 1, the reasons go to the pusher) when one breaks a branch
/// rule in GITTER_RULES, and otherwise runs push protection (`scan::check`),
/// so a refused push never uses up an allowance.
pub fn pre_receive() -> i32 {
    let mut raw = Vec::new();
    let Ok(input) = std::io::stdin().read_to_end(&mut raw).map_err(drop).and_then(|_| String::from_utf8(raw).map_err(drop)) else {
        eprintln!("Gitter couldn't read the refs this push updates (names must be UTF-8), so it refuses the push.");
        return 1;
    };
    if let Ok(rules) = std::env::var("GITTER_RULES") {
        let code = check_rules(&rules, &input);
        if code != 0 {
            return code;
        }
    }
    crate::scan::check(&input)
}

/// The branch-rule half of the hook.
fn check_rules(raw: &str, input: &str) -> i32 {
    let Ok(rules) = serde_json::from_str::<Vec<Rule>>(raw) else {
        eprintln!("Gitter couldn't read this repository's branch rules, so it refuses the push.");
        return 1;
    };
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
    type Armed = (i64, String, String, i64, Option<String>, Option<i64>);
    let (waiting, names): (Vec<Armed>, Option<(String, String)>) = st
        .db
        .call(move |c| {
            let mut q = c.prepare(
                "SELECT i.number, p.head_branch, p.auto_merge, p.auto_merge_by, p.auto_merge_sha,
                        (SELECT b.user_id FROM branch_pushers b WHERE b.repo_id = i.repo_id AND b.branch = p.head_branch)
                 FROM pulls p JOIN issues i ON i.id = p.issue_id
                 WHERE i.repo_id = ?1 AND i.state = 'open' AND p.auto_merge IS NOT NULL AND p.auto_merge_by IS NOT NULL",
            )?;
            let armed = q.query_map([repo_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))?.collect::<Result<_, _>>()?;
            let names = c
                .query_row("SELECT u.username, r.name FROM repos r JOIN users u ON u.id = r.owner_id WHERE r.id = ?1", [repo_id], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()?;
            Ok((armed, names))
        })
        .await?;
    let Some((owner, name)) = names else { return Ok(()) };
    for (number, head, method, by, armed_at, pusher) in waiting {
        // Whoever asked was suspended, or can't merge any more: the request lapses.
        let Some(user) = crate::auth::user_by_id(&st.db, by).await? else {
            clear(st, repo_id, number).await?;
            continue;
        };
        let Ok(repo) = Repo::writable(st, Some(&user), &owner, &name).await else {
            clear(st, repo_id, number).await?;
            continue;
        };
        let Some(head_sha) = st.git.resolve(&repo.path, &format!("refs/heads/{head}")).await? else { continue };
        if sha.as_ref().is_some_and(|s| *s != head_sha) {
            continue;
        }
        // Armed for one commit: someone else's newer push needs a new go-ahead
        // (their code would otherwise merge as the person who armed it).
        if armed_at.as_deref() != Some(head_sha.as_str()) {
            if pusher != Some(by) {
                clear(st, repo_id, number).await?;
                continue;
            }
            let (h, n) = (head_sha.clone(), number);
            st.db
                .call(move |c| Ok(c.execute("UPDATE pulls SET auto_merge_sha = ?1 WHERE issue_id = (SELECT id FROM issues WHERE repo_id = ?2 AND number = ?3)", params![h, repo_id, n])?))
                .await?;
        }
        match crate::api::issues::merge(st, &repo, &user, number, &method, &head_sha).await {
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
                "UPDATE pulls SET auto_merge = NULL, auto_merge_by = NULL, auto_merge_sha = NULL WHERE issue_id = (SELECT id FROM issues WHERE repo_id = ?1 AND number = ?2)",
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
        Rule { id: 0, pattern: p.into(), require_pr: false, approvals: 0, checks: vec![], allow_force_push: true, allow_deletion: true, enforce_admins: false, merge_queue: false }
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
        assert!(!covers("main", "fix/main") && !covers("*", "fix/x") && covers("*", "main") && covers("**", "fix/x") && covers("fix/**", "fix/a/b"));
        assert!(rule("a b").validate().is_err() && rule("x//y").validate().is_err() && rule("feat/**").validate().is_ok());
    }
}
