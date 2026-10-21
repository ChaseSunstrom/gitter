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
}
