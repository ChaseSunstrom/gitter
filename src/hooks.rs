//! Things that happen after a branch moves (git push, web edits, merges):
//! CI runs, webhooks and push mirrors start here.

use crate::AppState;
use crate::auth::User;
use crate::repo::Repo;
use crate::smart_http::RefUpdate;

pub async fn on_push(st: &AppState, repo: &Repo, user: &User, updates: &[RefUpdate]) {
    let default_ref = format!("refs/heads/{}", repo.default_branch);
    let empty_before = updates.iter().all(|u| u.old.is_none());
    if empty_before || updates.iter().any(|u| u.refname == default_ref) {
        crate::langs::refresh(st, repo.id, &repo.path).await;
    }
    st.git.maintain_soon(repo.path.clone());
    refresh_size(st, repo.id).await;
    record_pushers(st, repo.id, user.id, updates).await;
    // A maintainer's push to a fork's pull request branch doesn't run the fork's CI (with its secrets).
    if repo.push_only.is_empty() {
        crate::ci::trigger::on_push(st, repo, user.id, updates).await;
    }
    crate::webhooks::push(st, repo, user.id, updates).await;
    crate::mirrors::changed(st, repo.id).await;
    hand_merged(st, repo, user, updates).await;
    // A queued pull request's branch, or the branch it's tested against, may have moved.
    crate::mergequeue::kick(st, repo.id);
    // Pull requests from this repository's branches into the one it was forked from (or its forks).
    crate::api::issues::sync_fork_pulls(st, repo, updates).await;
    // And into the one it was forked from on another server.
    crate::api::federation::pushed(st, repo, updates).await;
}

/// Open pull requests into a moved branch whose head it now holds (merged by hand:
/// a push, a fast-forward) are recorded as merged by the pusher.
async fn hand_merged(st: &AppState, repo: &Repo, user: &User, updates: &[RefUpdate]) {
    for u in updates {
        let (Some(base), Some(new)) = (u.refname.strip_prefix("refs/heads/"), u.new.as_deref()) else { continue };
        let (rid, b) = (repo.id, base.to_string());
        let open: Vec<(i64, i64, String, String)> = match st
            .db
            .call(move |c| {
                let mut q = c.prepare(
                    "SELECT i.id, i.number, p.head_branch, coalesce(p.from_fork, 0), i.title FROM pulls p JOIN issues i ON i.id = p.issue_id
                     WHERE i.repo_id = ?1 AND i.state = 'open' AND p.base_branch = ?2",
                )?;
                let rows = q.query_map(rusqlite::params![rid, b], |r| {
                    let (n, branch): (i64, String) = (r.get(1)?, r.get(2)?);
                    Ok((r.get(0)?, n, crate::api::issues::head_ref(n, &branch, r.get::<_, i64>(3)? != 0), r.get(4)?))
                })?;
                Ok(rows.collect::<Result<_, _>>()?)
            })
            .await
        {
            Ok(o) => o,
            Err(e) => {
                eprintln!("hand merges into {}:{base}: {}", repo.full_name(), e.msg);
                continue;
            }
        };
        if open.is_empty() {
            continue;
        }
        // One listing for all of them: which of their heads the branch now holds.
        let mut args = vec!["for-each-ref", "--format=%(refname) %(objectname)", "--merged", new];
        args.extend(open.iter().map(|p| p.2.as_str()));
        let out = match st.git.run(&repo.path, &args, None, &[]).await {
            Ok(o) if o.code == 0 => String::from_utf8_lossy(&o.stdout).into_owned(),
            Ok(o) => {
                eprintln!("hand merges into {}:{base}: {}", repo.full_name(), String::from_utf8_lossy(&o.stderr).trim());
                continue;
            }
            Err(e) => {
                eprintln!("hand merges into {}:{base}: {}", repo.full_name(), e.msg);
                continue;
            }
        };
        let held: std::collections::HashMap<&str, &str> = out.lines().filter_map(|l| l.split_once(' ')).collect();
        for (iid, number, head_ref, title) in open {
            let Some(head) = held.get(head_ref.as_str()).map(|s| s.to_string()) else { continue };
            let (uid, base_sha) = (user.id, new.to_string());
            // Still open: a second push or a merge landing meanwhile records nothing more.
            let marked = st
                .db
                .call(move |c| {
                    let tx = c.transaction()?;
                    let marked = crate::api::issues::mark_merged(&tx, iid, uid, &head, &head, &base_sha)?;
                    tx.commit()?;
                    Ok(marked)
                })
                .await;
            let done = match marked {
                Ok(true) => crate::api::issues::announce_merge(st, repo.id, uid, iid, number, &title).await,
                Ok(false) => Ok(()),
                Err(e) => Err(e),
            };
            if let Err(e) = done {
                eprintln!("hand merge of {}#{number}: {}", repo.full_name(), e.msg);
            }
        }
    }
}

/// Who moved each branch last (a scheduled run counts as theirs). A deleted
/// branch is forgotten, so one made later under its name starts fresh.
async fn record_pushers(st: &AppState, repo_id: i64, user_id: i64, updates: &[RefUpdate]) {
    let moved: Vec<(String, bool)> = updates.iter().filter_map(|u| Some((u.refname.strip_prefix("refs/heads/")?.to_string(), u.new.is_some()))).collect();
    let gone: Vec<String> = moved.iter().filter(|m| !m.1).map(|m| m.0.clone()).collect();
    let _ = st
        .db
        .call(move |c| {
            for (b, _) in moved.iter().filter(|m| m.1) {
                c.execute(
                    "INSERT OR REPLACE INTO branch_pushers (repo_id, branch, user_id, at) VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![repo_id, b, user_id, crate::db::now()],
                )?;
            }
            Ok(())
        })
        .await;
    if !gone.is_empty() {
        forget_branches(st, repo_id, gone).await;
    }
}

/// A deleted branch stops being an agent's, and its last pusher is forgotten.
pub async fn forget_branches(st: &AppState, repo_id: i64, branches: Vec<String>) {
    let _ = st
        .db
        .call(move |c| {
            for b in &branches {
                c.execute("DELETE FROM agent_branches WHERE repo_id = ?1 AND branch = ?2", rusqlite::params![repo_id, b])?;
                c.execute("DELETE FROM branch_pushers WHERE repo_id = ?1 AND branch = ?2", rusqlite::params![repo_id, b])?;
            }
            Ok(())
        })
        .await;
}

/// Record how much disk a repository uses (for storage quotas and the admin page).
pub async fn refresh_size(st: &AppState, repo_id: i64) {
    let path = st.git.repo_path(repo_id);
    // Moved to the trash meanwhile: its size stays what it was (a restore brings it back).
    if !path.is_dir() {
        return;
    }
    let Ok(size) = tokio::task::spawn_blocking(move || crate::api::admin::dir_size(&path)).await else { return };
    let _ = st
        .db
        .call(move |c| Ok(c.execute("UPDATE repos SET size_bytes = ?1 WHERE id = ?2 AND deleted_at IS NULL", rusqlite::params![size as i64, repo_id])?))
        .await;
}

/// After history arrived outside a push (an import, the bench seed): refresh what pushes refresh.
pub async fn after_import(st: &AppState, repo_id: i64) {
    let path = st.git.repo_path(repo_id);
    let _ = st.git.maintain(&path).await;
    crate::langs::refresh(st, repo_id, &path).await;
    refresh_size(st, repo_id).await;
    let _ = crate::repo::touch(&st.db, repo_id, true).await;
}

/// At boot: measure repositories that have no recorded size yet (from before
/// quotas, or a restore), one at a time.
// ponytail: quota checks read recorded sizes, so pushes racing each other can overshoot by one push each; reserve bytes at check time if that matters.
pub async fn backfill_sizes(st: &AppState) {
    let ids: Vec<i64> = st
        .db
        .call(|c| {
            let mut s = c.prepare("SELECT id FROM repos WHERE size_bytes = 0 AND deleted_at IS NULL")?;
            Ok(s.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?)
        })
        .await
        .unwrap_or_default();
    for id in ids {
        refresh_size(st, id).await;
    }
}
