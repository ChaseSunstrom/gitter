//! Loading repositories and deciding who may read or write them. Every API
//! route and the git HTTP endpoints go through `Repo::load`.

use std::path::PathBuf;

use rusqlite::OptionalExtension;
use serde::Serialize;

use crate::AppState;
use crate::auth::User;
use crate::db::{Db, now};
use crate::error::{AppError, R};

#[derive(Clone, Debug, Serialize)]
pub struct Repo {
    pub id: i64,
    pub owner_id: i64,
    pub owner: String,
    pub name: String,
    pub description: String,
    pub website: String,
    pub private: bool,
    pub default_branch: String,
    pub forked_from: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    pub pushed_at: Option<i64>,
    /// Hidden by a moderator: only the owner and admins can see it.
    pub hidden: bool,
    #[serde(skip)]
    pub path: PathBuf,
    #[serde(skip)]
    pub can_read: bool,
    pub can_write: bool,
    pub is_owner: bool,
}

pub const REPO_SELECT: &str = "SELECT r.id, r.owner_id, u.username, r.name, r.description, r.website, r.is_private,
    r.default_branch, r.forked_from, r.created_at, r.updated_at, r.pushed_at, r.hidden FROM repos r JOIN users u ON u.id = r.owner_id";

pub fn row_to_repo(r: &rusqlite::Row) -> rusqlite::Result<Repo> {
    Ok(Repo {
        id: r.get(0)?,
        owner_id: r.get(1)?,
        owner: r.get(2)?,
        name: r.get(3)?,
        description: r.get(4)?,
        website: r.get(5)?,
        private: r.get::<_, i64>(6)? != 0,
        default_branch: r.get(7)?,
        forked_from: r.get(8)?,
        created_at: r.get(9)?,
        updated_at: r.get(10)?,
        pushed_at: r.get(11)?,
        hidden: r.get::<_, i64>(12)? != 0,
        path: PathBuf::new(),
        can_read: false,
        can_write: false,
        is_owner: false,
    })
}

impl Repo {
    /// Look up `owner/name` and work out the viewer's access. Use `readable`
    /// or `writable` in handlers; they turn "no access" into an error.
    pub async fn load(st: &AppState, viewer: Option<&User>, owner: &str, name: &str) -> R<Repo> {
        let name = name.strip_suffix(".git").unwrap_or(name);
        if !crate::validate::name(owner, 39) || !crate::validate::name(name, 100) {
            return Err(AppError::not_found());
        }
        let (o, n) = (owner.to_string(), name.to_string());
        let repo = st
            .db
            .call(move |c| {
                Ok(c.query_row(
                    &format!("{REPO_SELECT} WHERE u.username = ?1 AND r.name = ?2"),
                    [&o, &n],
                    row_to_repo,
                )
                .optional()?)
            })
            .await?
            .ok_or_else(AppError::not_found)?;
        Ok(Self::with_access(st, repo, viewer))
    }

    pub fn with_access(st: &AppState, mut repo: Repo, viewer: Option<&User>) -> Repo {
        repo.path = st.git.repo_path(repo.id);
        let (read, write) = access(&repo, viewer);
        repo.can_read = read;
        repo.can_write = write;
        repo.is_owner = viewer.is_some_and(|u| u.id == repo.owner_id);
        repo
    }

    pub async fn readable(
        st: &AppState,
        viewer: Option<&User>,
        owner: &str,
        name: &str,
    ) -> R<Repo> {
        let r = Self::load(st, viewer, owner, name).await?;
        if !r.can_read {
            return Err(AppError::not_found());
        }
        Ok(r)
    }

    pub async fn writable(
        st: &AppState,
        viewer: Option<&User>,
        owner: &str,
        name: &str,
    ) -> R<Repo> {
        let r = Self::readable(st, viewer, owner, name).await?;
        if viewer.is_none() {
            return Err(AppError::unauthorized());
        }
        if !r.can_write {
            return Err(AppError::forbidden());
        }
        Ok(r)
    }

    pub fn full_name(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
}

/// (can read, can write). Owners and admins write; everyone reads public repos.
/// A limited token narrows that: read-only tokens never write, and a deploy
/// token sees other repositories as if signed out.
pub fn access(repo: &Repo, viewer: Option<&User>) -> (bool, bool) {
    let write = viewer.is_some_and(|u| u.id == repo.owner_id || u.is_admin);
    let (mut read, mut write) = (!(repo.private || repo.hidden) || write, write);
    if let Some(l) = viewer.and_then(|u| u.limits.as_ref()) {
        if l.repo_id.is_some_and(|id| id != repo.id) {
            (read, write) = (!(repo.private || repo.hidden), false);
        }
        if l.scope == crate::auth::Scope::Read {
            write = false;
        }
    }
    (read, write)
}

/// Bytes account ?1 stores: its repositories and their wikis, its snippets, and
/// the LFS objects its repositories hold (each once).
pub const STORAGE_USED: &str = "COALESCE((SELECT sum(size_bytes + wiki_bytes) FROM repos WHERE owner_id = ?1), 0)
    + COALESCE((SELECT sum(size_bytes) FROM snippets WHERE owner_id = ?1), 0)
    + COALESCE((SELECT sum(size) FROM (SELECT DISTINCT l.oid, l.size FROM lfs_objects l JOIN repos r ON r.id = l.repo_id WHERE r.owner_id = ?1)), 0)";

/// Bytes an account may still store across its repositories (None = no limit).
pub async fn storage_left(st: &AppState, owner_id: i64) -> R<Option<u64>> {
    let quota = st.settings.get().max_storage_mb * 1024 * 1024;
    let (used, admin): (i64, bool) = st
        .db
        .call(move |c| {
            Ok(c.query_row(
                &format!("SELECT {STORAGE_USED}, is_admin FROM users WHERE id = ?1"),
                [owner_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?)
        })
        .await?;
    if quota == 0 || admin {
        return Ok(None);
    }
    Ok(Some(quota.saturating_sub(used.max(0) as u64)))
}

/// Refuse a save of `bytes` more than account `owner_id` has room for.
pub async fn need_room(st: &AppState, owner_id: i64, bytes: usize) -> R<()> {
    match storage_left(st, owner_id).await? {
        Some(left) if bytes as u64 > left => Err(AppError::bad(
            "This account is out of storage. Delete something, or ask the site admin for more space.",
        )),
        _ => Ok(()),
    }
}

/// The disk a bare repository folder uses now (wikis and snippets record theirs after each save).
pub async fn measure(dir: PathBuf) -> i64 {
    tokio::task::spawn_blocking(move || crate::api::admin::dir_size(&dir) as i64).await.unwrap_or(0)
}

pub async fn record_event(
    db: &Db,
    user_id: i64,
    repo_id: Option<i64>,
    kind: &str,
    title: &str,
    target: &str,
    count: i64,
) -> R<()> {
    let (kind, title, target) = (kind.to_string(), title.to_string(), target.to_string());
    db.call(move |c| {
        c.execute(
            "INSERT INTO events(user_id, repo_id, kind, title, target, count, created_at) VALUES (?1,?2,?3,?4,?5,?6,?7)",
            rusqlite::params![user_id, repo_id, kind, title, target, count, now()],
        )?;
        Ok(())
    })
    .await
}

pub async fn touch(db: &Db, repo_id: i64, pushed: bool) -> R<()> {
    db.call(move |c| {
        let t = now();
        if pushed {
            c.execute(
                "UPDATE repos SET updated_at = ?1, pushed_at = ?1 WHERE id = ?2",
                rusqlite::params![t, repo_id],
            )?;
        } else {
            c.execute(
                "UPDATE repos SET updated_at = ?1 WHERE id = ?2",
                rusqlite::params![t, repo_id],
            )?;
        }
        Ok(())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(owner: i64, private: bool) -> Repo {
        Repo {
            id: 1,
            owner_id: owner,
            owner: "o".into(),
            name: "r".into(),
            description: String::new(),
            website: String::new(),
            private,
            default_branch: "main".into(),
            forked_from: None,
            created_at: 0,
            updated_at: 0,
            pushed_at: None,
            hidden: false,
            path: PathBuf::new(),
            can_read: false,
            can_write: false,
            is_owner: false,
        }
    }
    fn user(id: i64, admin: bool) -> User {
        User {
            id,
            username: "u".into(),
            display_name: String::new(),
            email: String::new(),
            is_admin: admin,
            limits: None,
        }
    }

    #[test]
    fn access_rules() {
        assert_eq!(access(&repo(1, false), None), (true, false));
        assert_eq!(access(&repo(1, true), None), (false, false));
        assert_eq!(
            access(&repo(1, true), Some(&user(2, false))),
            (false, false)
        );
        assert_eq!(access(&repo(1, true), Some(&user(1, false))), (true, true));
        assert_eq!(access(&repo(1, true), Some(&user(9, true))), (true, true));
        assert_eq!(
            access(&repo(1, false), Some(&user(2, false))),
            (true, false)
        );
        // Limited tokens.
        let lim = |scope, repo_id| User { limits: Some(crate::auth::Limits { scope, repo_id }), ..user(1, false) };
        use crate::auth::Scope;
        assert_eq!(access(&repo(1, true), Some(&lim(Scope::Read, None))), (true, false));
        assert_eq!(access(&repo(1, true), Some(&lim(Scope::Write, Some(1)))), (true, true));
        assert_eq!(access(&repo(1, true), Some(&lim(Scope::Write, Some(7)))), (false, false));
        assert_eq!(access(&repo(1, false), Some(&lim(Scope::All, Some(7)))), (true, false));
    }
}
