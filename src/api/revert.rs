//! Revert a commit or a merged pull request from the web: the inverse change
//! goes on a new branch (`revert-<sha>` / `revert-pr-<n>`) with a pull request
//! into the target branch, so nothing lands without the usual review.

use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use serde_json::{Value, json};

use super::J;
use crate::auth::Authed;
use crate::error::{AppError, R};
use crate::repo::{Repo, record_event, touch};
use crate::smart_http::RefUpdate;
use crate::{AppState, validate};

#[derive(Deserialize)]
pub struct RevertReq {
    /// A commit to revert...
    sha: Option<String>,
    /// ...or a merged pull request.
    pull: Option<i64>,
    /// Where the revert should land (default: the PR's base, or the default branch).
    branch: Option<String>,
}

/// What to undo: the change from `before` to `after`.
struct Undo {
    before: String,
    after: String,
    target: String,
    branch: String,
    title: String,
    body: String,
    message: String,
}

fn clip(s: &str) -> String {
    let t: String = s.chars().take(200).collect();
    if t.len() < s.len() { format!("{t}…") } else { t }
}

async fn of_commit(st: &AppState, r: &Repo, sha: &str) -> R<Undo> {
    if !validate::sha(sha) {
        return Err(AppError::bad("That isn't a commit"));
    }
    let full = super::code::resolve_rev(st, r, Some(sha)).await?;
    let c = st.git.commit_info(&r.path, &full).await?.ok_or_else(AppError::not_found)?;
    let Some(parent) = c.parents.first().cloned() else {
        return Err(AppError::bad("That's the first commit: there's nothing before it to go back to"));
    };
    let subject = clip(&c.subject);
    Ok(Undo {
        before: parent,
        after: full.clone(),
        target: r.default_branch.clone(),
        branch: format!("revert-{}", &full[..12]),
        title: format!("Revert \"{subject}\""),
        body: format!("Reverts {full}."),
        message: format!("Revert \"{subject}\"\n\nThis reverts commit {full}.\n"),
    })
}

async fn of_pull(st: &AppState, r: &Repo, u: &crate::auth::User, n: i64) -> R<Undo> {
    let issue = super::issues::load_issue(st, r, n, Some(u)).await?;
    if !issue.is_pr {
        return Err(AppError::not_found());
    }
    if issue.state != "merged" {
        return Err(AppError::conflict("Only a merged pull request can be reverted"));
    }
    let iid = issue.id;
    let (base_branch, head_sha, base_sha): (String, Option<String>, Option<String>) = st
        .db
        .call(move |c| Ok(c.query_row("SELECT base_branch, head_sha, base_sha FROM pulls WHERE issue_id = ?1", [iid], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?))
        .await?;
    let (Some(head), Some(base)) = (head_sha, base_sha) else {
        return Err(AppError::conflict("This pull request's merge wasn't recorded, so there's nothing to revert"));
    };
    // What the pull request showed as its change: from where its branch left the base to its head.
    let from = st.git.merge_base(&r.path, &base, &head).await?.unwrap_or(base);
    if from == head {
        return Err(AppError::conflict("This pull request was merged outside Gitter, so its change isn't known: revert its commits instead"));
    }
    let title = clip(&issue.title);
    Ok(Undo {
        before: from,
        after: head,
        target: base_branch,
        branch: format!("revert-pr-{n}"),
        title: format!("Revert \"{title}\""),
        body: format!("Reverts #{n}."),
        message: format!("Revert \"{title}\"\n\nThis reverts pull request #{n}.\n"),
    })
}

pub async fn revert(
    State(st): State<AppState>,
    Authed(u): Authed,
    Path((o, n)): Path<(String, String)>,
    Json(req): Json<RevertReq>,
) -> J<Value> {
    let r = Repo::writable(&st, Some(&u), &o, &n).await?;
    crate::mirrors::refuse_writes(&r)?;
    let mut undo = match (req.sha.as_deref(), req.pull) {
        (Some(sha), None) => of_commit(&st, &r, sha).await?,
        (None, Some(num)) => of_pull(&st, &r, &u, num).await?,
        _ => return Err(AppError::bad("Name a commit (sha) or a pull request (pull) to revert")),
    };
    if let Some(b) = req.branch {
        undo.target = b;
    }
    if !validate::ref_name(&undo.target) {
        return Err(AppError::bad("That isn't a valid branch name"));
    }
    let tip = st
        .git
        .resolve(&r.path, &format!("refs/heads/{}", undo.target))
        .await?
        .ok_or_else(|| AppError::bad(format!("The branch {} doesn't exist", undo.target)))?;
    let branch_ref = format!("refs/heads/{}", undo.branch);
    if st.git.resolve(&r.path, &branch_ref).await?.is_some() {
        return Err(AppError::conflict(format!(
            "The branch {} already exists: open its pull request, or delete the branch to revert again",
            undo.branch
        )));
    }
    // The inverse change on top of the target, merged like any branch (no work tree).
    let tree = match st.git.merge_tree_at(&r.path, Some(&undo.after), &tip, &undo.before).await? {
        Ok(t) => t,
        Err(files) => {
            return Err(AppError::conflict(format!(
                "This can't be reverted automatically: later changes conflict in {}. Revert it locally with git revert.",
                files.join(", ")
            )));
        }
    };
    let tip_tree = st.git.ok(&r.path, &["rev-parse", &format!("{tip}^{{tree}}")]).await?;
    if String::from_utf8_lossy(&tip_tree).trim() == tree {
        return Err(AppError::conflict(format!("{} already has none of this change: nothing to revert", undo.target)));
    }
    let (an, ae) = u.signature();
    let sha = st.git.commit_tree(&r.path, &tree, &[&tip], &undo.message, (&an, &ae)).await?;
    st.git.update_ref(&r.path, &branch_ref, &sha, None).await?;
    let noted = async {
        touch(&st.db, r.id, true).await?;
        record_event(&st.db, u.id, Some(r.id), "push", &undo.branch, &sha, 1).await
    };
    if let Err(e) = noted.await {
        // Nothing half-done: no branch without its pull request.
        let _ = st.git.delete_ref(&r.path, &branch_ref).await;
        return Err(e);
    }
    let upd = RefUpdate { refname: branch_ref.clone(), old: None, new: Some(sha.clone()) };
    crate::hooks::on_push(&st, &r, &u, &[upd]).await;
    // Opened through the same path as any pull request (checks, events, webhooks, CI).
    let pr: super::issues::PullReq = serde_json::from_value(json!({
        "title": undo.title, "body": undo.body, "head": undo.branch, "base": undo.target,
    }))
    .map_err(AppError::internal)?;
    match super::issues::create_pull(State(st.clone()), Authed(u), Path((o, n)), Json(pr)).await {
        Ok(Json(v)) => Ok(Json(json!({ "number": v["number"], "branch": undo.branch, "sha": sha }))),
        Err(e) => {
            // No pull request: take the branch away again, so trying once more works.
            let _ = st.git.delete_ref(&r.path, &branch_ref).await;
            Err(e)
        }
    }
}
