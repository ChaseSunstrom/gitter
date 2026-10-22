//! Things that happen after a branch moves (git push, web edits, merges).
//! CI (T-0006) plugs in here.

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
    crate::ci::trigger::on_push(st, repo, user.id, updates).await;
}

/// Record how much disk a repository uses (for storage quotas and the admin page).
pub async fn refresh_size(st: &AppState, repo_id: i64) {
    let path = st.git.repo_path(repo_id);
    let Ok(size) = tokio::task::spawn_blocking(move || crate::api::admin::dir_size(&path)).await else { return };
    let _ = st
        .db
        .call(move |c| Ok(c.execute("UPDATE repos SET size_bytes = ?1 WHERE id = ?2", rusqlite::params![size as i64, repo_id])?))
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
            let mut s = c.prepare("SELECT id FROM repos WHERE size_bytes = 0")?;
            Ok(s.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?)
        })
        .await
        .unwrap_or_default();
    for id in ids {
        refresh_size(st, id).await;
    }
}
