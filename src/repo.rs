//! Loading repositories and deciding who may read or write them. Every API
//! route goes through `Repo::load`, and the git HTTP endpoints through `Repo::find`.

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
    /// Settings, collaborators, tokens, secrets, delete.
    pub can_admin: bool,
    pub is_owner: bool,
    /// The viewer's role from collaborators and org membership (ROLE_* levels), before token limits.
    #[serde(skip)]
    pub role: u8,
}

/// Role levels: what a collaborator or org member may do.
pub const ROLE_READ: u8 = 1;
pub const ROLE_WRITE: u8 = 2;
pub const ROLE_ADMIN: u8 = 3;

/// The highest level user ?2 has on repository ?1 (owned by ?3) through
/// collaborators or membership of the owning org.
const ROLE_SQL: &str = "SELECT max(level) FROM (
    SELECT CASE role WHEN 'admin' THEN 3 WHEN 'write' THEN 2 WHEN 'read' THEN 1 ELSE 0 END AS level
        FROM repo_collaborators WHERE repo_id = ?1 AND user_id = ?2
    UNION ALL
    SELECT CASE WHEN m.role = 'owner' THEN 3 WHEN o.base_role = 'write' THEN 2 WHEN o.base_role = 'read' THEN 1 ELSE 0 END
        FROM org_members m JOIN orgs o ON o.id = m.org_id WHERE m.org_id = ?3 AND m.user_id = ?2)";

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
        can_admin: false,
        is_owner: false,
        role: 0,
    })
}

/// The live repository at `owner/name`, or (true) the one that was renamed or
/// transferred away from there. Trashed repositories are never found.
pub fn lookup(c: &rusqlite::Connection, owner: &str, name: &str) -> rusqlite::Result<Option<(Repo, bool)>> {
    let name = name.strip_suffix(".git").unwrap_or(name);
    if !crate::validate::name(owner, 39) || !crate::validate::name(name, 100) {
        return Ok(None);
    }
    let here = c
        .query_row(&format!("{REPO_SELECT} WHERE u.username = ?1 AND r.name = ?2 AND r.deleted_at IS NULL"), [owner, name], row_to_repo)
        .optional()?;
    if let Some(r) = here {
        return Ok(Some((r, false)));
    }
    let moved = c
        .query_row(
            &format!(
                "{REPO_SELECT} WHERE r.deleted_at IS NULL AND r.id = (SELECT d.repo_id FROM repo_redirects d
                 JOIN users du ON du.id = d.owner_id WHERE du.username = ?1 AND d.name = ?2)"
            ),
            [owner, name],
            row_to_repo,
        )
        .optional()?;
    Ok(moved.map(|r| (r, true)))
}

/// A live repository now uses `name`: drop any redirect from there, so the
/// name never points elsewhere again (even after this repository goes).
pub fn claim_name(c: &rusqlite::Connection, owner_id: i64, name: &str) -> rusqlite::Result<()> {
    c.execute("DELETE FROM repo_redirects WHERE owner_id = ?1 AND name = ?2", rusqlite::params![owner_id, name])?;
    Ok(())
}

/// Remember that `owner_id/name` now means repository `repo_id` (a rename or a transfer away).
pub fn add_redirect(c: &rusqlite::Connection, owner_id: i64, name: &str, repo_id: i64) -> rusqlite::Result<()> {
    c.execute(
        "INSERT OR REPLACE INTO repo_redirects(owner_id, name, repo_id, created_at) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![owner_id, name, repo_id, now()],
    )?;
    // Bounded, so renaming in a loop can't grow the table without end.
    c.execute(
        "DELETE FROM repo_redirects WHERE repo_id = ?1 AND rowid NOT IN
         (SELECT rowid FROM repo_redirects WHERE repo_id = ?1 ORDER BY created_at DESC, rowid DESC LIMIT 20)",
        [repo_id],
    )?;
    Ok(())
}

impl Repo {
    /// Look up `owner/name` and work out the viewer's access. Use `readable`
    /// or `writable` in handlers; they turn "no access" into an error. A
    /// repository that moved is `AppError::moved` (the web and API redirect),
    /// but only for viewers who may see where it went.
    pub async fn load(st: &AppState, viewer: Option<&User>, owner: &str, name: &str) -> R<Repo> {
        let (repo, moved) = Self::locate(st, viewer, owner, name).await?.ok_or_else(AppError::not_found)?;
        let repo = Self::with_access(st, repo, viewer);
        if !moved {
            return Ok(repo);
        }
        if !repo.can_read {
            return Err(AppError::not_found());
        }
        Err(AppError::moved((owner, name.strip_suffix(".git").unwrap_or(name)), (&repo.owner, &repo.name)))
    }

    /// Like `load`, but a moved repository is served at its old name: git and
    /// Git LFS clients don't follow redirects when pushing.
    pub async fn find(st: &AppState, viewer: Option<&User>, owner: &str, name: &str) -> R<Repo> {
        let (repo, _) = Self::locate(st, viewer, owner, name).await?.ok_or_else(AppError::not_found)?;
        Ok(Self::with_access(st, repo, viewer))
    }

    /// The repository (and whether it moved), with the viewer's collaborator or org role, in one call.
    pub async fn locate(st: &AppState, viewer: Option<&User>, owner: &str, name: &str) -> R<Option<(Repo, bool)>> {
        let (o, n) = (owner.to_string(), name.to_string());
        let vid = viewer.map(|u| u.id).unwrap_or(-1);
        st.db
            .call(move |c| {
                let Some((mut r, moved)) = lookup(c, &o, &n)? else { return Ok(None) };
                if vid >= 0 {
                    let level: Option<i64> = c.query_row(ROLE_SQL, rusqlite::params![r.id, vid, r.owner_id], |x| x.get(0))?;
                    r.role = level.unwrap_or(0) as u8;
                }
                Ok(Some((r, moved)))
            })
            .await
    }

    pub fn with_access(st: &AppState, mut repo: Repo, viewer: Option<&User>) -> Repo {
        repo.path = st.git.repo_path(repo.id);
        let a = access(&repo, viewer);
        (repo.can_read, repo.can_write, repo.can_admin) = (a.read, a.write, a.admin);
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

    /// For settings, collaborators, tokens and secrets: the repository's admins.
    pub async fn administrable(st: &AppState, viewer: Option<&User>, owner: &str, name: &str) -> R<Repo> {
        let r = Self::writable(st, viewer, owner, name).await?;
        if !r.can_admin {
            return Err(AppError::new(axum::http::StatusCode::FORBIDDEN, "Only this repository's admins can do that"));
        }
        Ok(r)
    }

    pub fn full_name(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Access {
    pub read: bool,
    pub write: bool,
    pub admin: bool,
}

/// What `viewer` may do with `repo`. The owner and site admins have admin;
/// collaborators and org members their role (`repo.role`); everyone reads
/// public repos. A moderator-hidden repo is for its admins only. A limited
/// token narrows that: read-only tokens never write, only full-access tokens
/// administer, and a deploy token sees other repositories as if signed out.
pub fn access(repo: &Repo, viewer: Option<&User>) -> Access {
    let level = match viewer {
        Some(u) if u.id == repo.owner_id || u.is_admin => ROLE_ADMIN,
        Some(_) => repo.role,
        None => 0,
    };
    let public = !(repo.private || repo.hidden);
    let usable = !repo.hidden || level >= ROLE_ADMIN;
    let mut a = Access {
        read: public || (usable && level >= ROLE_READ),
        write: usable && level >= ROLE_WRITE,
        admin: level >= ROLE_ADMIN,
    };
    if let Some(l) = viewer.and_then(|u| u.limits.as_ref()) {
        if l.repo_id.is_some_and(|id| id != repo.id) {
            a = Access { read: public, write: false, admin: false };
        }
        if l.scope == crate::auth::Scope::Read {
            a.write = false;
        }
        if l.scope != crate::auth::Scope::All {
            a.admin = false;
        }
    }
    a
}

/// Bytes account ?1 stores: its repositories (and its organizations') with their
/// wikis, LFS objects (each once) and release files, plus its snippets.
pub const STORAGE_USED: &str = "COALESCE((SELECT sum(size_bytes + wiki_bytes) FROM repos WHERE owner_id IN (SELECT ?1 UNION ALL SELECT id FROM orgs WHERE created_by = ?1)), 0)
    + COALESCE((SELECT sum(size_bytes) FROM snippets WHERE owner_id = ?1), 0)
    + COALESCE((SELECT sum(size) FROM (SELECT DISTINCT l.oid, l.size FROM lfs_objects l JOIN repos r ON r.id = l.repo_id
        WHERE r.owner_id IN (SELECT ?1 UNION ALL SELECT id FROM orgs WHERE created_by = ?1))), 0)
    + COALESCE((SELECT sum(a.size) FROM release_assets a JOIN releases rl ON rl.id = a.release_id JOIN repos r ON r.id = rl.repo_id
        WHERE r.owner_id IN (SELECT ?1 UNION ALL SELECT id FROM orgs WHERE created_by = ?1)), 0)";

/// The account whose limits `owner` uses, and whether it's a site admin. An
/// organization shares its creator's quota and repository limit, so making
/// organizations doesn't multiply them.
pub fn payer(c: &rusqlite::Connection, owner: i64) -> rusqlite::Result<(i64, bool)> {
    c.query_row(
        "SELECT p.id, p.is_admin FROM users p WHERE p.id = coalesce((SELECT o.created_by FROM orgs o JOIN users x ON x.id = o.created_by WHERE o.id = ?1), ?1)",
        [owner],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
}

/// Bytes used against `owner`'s quota (its payer's, for an organization), and whether it's unlimited.
pub fn usage(c: &rusqlite::Connection, owner: i64) -> rusqlite::Result<(i64, bool)> {
    let (id, admin) = payer(c, owner)?;
    Ok((c.query_row(&format!("SELECT {STORAGE_USED}"), [id], |r| r.get(0))?, admin))
}

/// Bytes an account may still store across its repositories (None = no limit).
pub async fn storage_left(st: &AppState, owner_id: i64) -> R<Option<u64>> {
    let quota = st.settings.get().max_storage_mb * 1024 * 1024;
    let (used, admin) = st.db.call(move |c| Ok(usage(c, owner_id)?)).await?;
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
            can_admin: false,
            is_owner: false,
            role: 0,
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
        let rw = |a: Access| (a.read, a.write, a.admin);
        assert_eq!(rw(access(&repo(1, false), None)), (true, false, false));
        assert_eq!(rw(access(&repo(1, true), None)), (false, false, false));
        assert_eq!(rw(access(&repo(1, true), Some(&user(2, false)))), (false, false, false));
        assert_eq!(rw(access(&repo(1, true), Some(&user(1, false)))), (true, true, true));
        assert_eq!(rw(access(&repo(1, true), Some(&user(9, true)))), (true, true, true));
        assert_eq!(rw(access(&repo(1, false), Some(&user(2, false)))), (true, false, false));
        // Collaborator and org roles.
        let with = |role, hidden| Repo { role, hidden, ..repo(1, true) };
        assert_eq!(rw(access(&with(ROLE_READ, false), Some(&user(2, false)))), (true, false, false));
        assert_eq!(rw(access(&with(ROLE_WRITE, false), Some(&user(2, false)))), (true, true, false));
        assert_eq!(rw(access(&with(ROLE_ADMIN, false), Some(&user(2, false)))), (true, true, true));
        assert_eq!(rw(access(&with(ROLE_WRITE, true), Some(&user(2, false)))), (false, false, false), "hidden: admins only");
        assert_eq!(rw(access(&with(ROLE_ADMIN, true), Some(&user(2, false)))), (true, true, true));
        // Limited tokens.
        let lim = |scope, repo_id| User { limits: Some(crate::auth::Limits { scope, repo_id }), ..user(1, false) };
        use crate::auth::Scope;
        assert_eq!(rw(access(&repo(1, true), Some(&lim(Scope::Read, None)))), (true, false, false));
        assert_eq!(rw(access(&repo(1, true), Some(&lim(Scope::Write, Some(1))))), (true, true, false), "only full tokens administer");
        assert_eq!(rw(access(&repo(1, true), Some(&lim(Scope::Write, Some(7))))), (false, false, false));
        assert_eq!(rw(access(&repo(1, false), Some(&lim(Scope::All, Some(7))))), (true, false, false));
    }
}
