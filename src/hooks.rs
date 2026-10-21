//! Things that happen after a branch moves (git push, web edits, merges).
//! CI (T-0006) plugs in here.

use crate::AppState;
use crate::auth::User;
use crate::repo::Repo;
use crate::smart_http::RefUpdate;

pub async fn on_push(st: &AppState, repo: &Repo, _user: &User, updates: &[RefUpdate]) {
    let default_ref = format!("refs/heads/{}", repo.default_branch);
    let empty_before = updates.iter().all(|u| u.old.is_none());
    if empty_before || updates.iter().any(|u| u.refname == default_ref) {
        crate::langs::refresh(st, repo.id, &repo.path).await;
    }
    st.git.maintain_soon(repo.path.clone());
}

/// After history arrived outside a push (an import, the bench seed): refresh what pushes refresh.
pub async fn after_import(st: &AppState, repo_id: i64) {
    let path = st.git.repo_path(repo_id);
    let _ = st.git.maintain(&path).await;
    crate::langs::refresh(st, repo_id, &path).await;
    let _ = crate::repo::touch(&st.db, repo_id, true).await;
}
