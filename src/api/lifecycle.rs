//! What happens to repositories and accounts over their life: transfers to
//! another account (an offer the recipient accepts), the trash (a deleted
//! repository is kept for a week, then purged), the account data export and
//! deleting your own account.

use std::path::{Path as FsPath, PathBuf};
use std::time::Duration;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use rusqlite::types::ValueRef;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};

use super::admin::Admin;
use super::{ClientIp, J, ok};
use crate::AppState;
use crate::auth::{Authed, Sudo, User, Viewer};
use crate::db::now;
use crate::error::{AppError, R};
use crate::repo::{Repo, add_redirect, claim_name};
use crate::security::{Event, record};

/// How long a deleted repository stays restorable.
pub const TRASH_SECS: i64 = 7 * 86400;
/// How long a transfer offer waits for an answer.
pub const OFFER_SECS: i64 = 7 * 86400;
/// How long a finished data export can be downloaded.
pub const EXPORT_SECS: i64 = 86400;

/// Exports building at once across the instance (each holds git busy for a while).
static EXPORTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

pub fn trash_path(st: &AppState, id: i64) -> PathBuf {
    st.cfg.data_dir.join("trash").join(format!("{id}.git"))
}

fn export_path(st: &AppState, user_id: i64, id: i64) -> PathBuf {
    st.cfg.data_dir.join("exports").join(format!("{user_id}-{id}.tar.gz"))
}

/// Repositories counted against `payer`'s limit (its own and its organizations'), not counting the trash.
pub fn live_repos(c: &Connection, payer: i64) -> rusqlite::Result<i64> {
    c.query_row(
        "SELECT count(*) FROM repos WHERE owner_id IN (SELECT ?1 UNION ALL SELECT id FROM orgs WHERE created_by = ?1) AND deleted_at IS NULL",
        [payer],
        |r| r.get(0),
    )
}

/// May `u` give away or bring back a repository of `owner_id`: the owner, an
/// owner of the organization that owns it, or a site admin (not admin collaborators).
fn owns(c: &Connection, u: &User, owner_id: i64) -> rusqlite::Result<bool> {
    if u.is_admin || u.id == owner_id {
        return Ok(true);
    }
    let q = "SELECT 1 FROM org_members WHERE org_id = ?1 AND user_id = ?2 AND role = 'owner'";
    Ok(c.query_row(q, [owner_id, u.id], |_| Ok(())).optional()?.is_some())
}

async fn must_own(st: &AppState, u: &User, owner_id: i64) -> R<()> {
    let u2 = u.clone();
    if st.db.call(move |c| Ok(owns(c, &u2, owner_id)?)).await? {
        Ok(())
    } else {
        Err(AppError::new(axum::http::StatusCode::FORBIDDEN, "Only the repository's owner can do that"))
    }
}

fn name_taken(c: &Connection, owner_id: i64, name: &str) -> rusqlite::Result<bool> {
    c.query_row("SELECT 1 FROM repos WHERE owner_id = ?1 AND name = ?2", params![owner_id, name], |_| Ok(()))
        .optional()
        .map(|r| r.is_some())
}

/// A rename that treats "already gone" as done (a folder lost before the move).
async fn move_dir(from: &FsPath, to: &FsPath) -> std::io::Result<()> {
    if let Some(p) = to.parent() {
        tokio::fs::create_dir_all(p).await?;
    }
    match tokio::fs::rename(from, to).await {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        r => r,
    }
}

// ---------- transfers ----------

pub async fn offer(State(st): State<AppState>, Viewer(v): Viewer, Path((owner, name)): Path<(String, String)>) -> J<Value> {
    let r = Repo::administrable(&st, v.as_ref(), &owner, &name).await?;
    let id = r.id;
    let offer = st
        .db
        .call(move |c| {
            Ok(c.query_row(
                "SELECT u.username, t.expires_at FROM repo_transfers t JOIN users u ON u.id = t.to_id WHERE t.repo_id = ?1 AND t.expires_at > ?2",
                params![id, now()],
                |r| Ok(json!({ "to": r.get::<_, String>(0)?, "expires_at": r.get::<_, i64>(1)? })),
            )
            .optional()?)
        })
        .await?;
    Ok(Json(json!({ "offer": offer })))
}

#[derive(Deserialize)]
pub struct OfferReq {
    to: String,
    confirm: String,
}

/// Offer a repository to another account. Nothing moves until they accept.
pub async fn offer_transfer(
    State(st): State<AppState>,
    Sudo(u): Sudo,
    ip: ClientIp,
    Path((owner, name)): Path<(String, String)>,
    Json(req): Json<OfferReq>,
) -> J<Value> {
    let r = Repo::administrable(&st, Some(&u), &owner, &name).await?;
    must_own(&st, &u, r.owner_id).await?;
    if !req.confirm.trim().eq_ignore_ascii_case(&r.full_name()) {
        return Err(AppError::bad(format!("Type {} to confirm", r.full_name())));
    }
    let to = req.to.trim().to_string();
    let (rid, from) = (r.id, r.owner_id);
    let to_id = st
        .db
        .call(move |c| {
            let id: i64 = c
                .query_row("SELECT id FROM users WHERE username = ?1 AND suspended = 0", [&to], |r| r.get(0))
                .optional()?
                .ok_or_else(|| AppError::new(axum::http::StatusCode::NOT_FOUND, "There's no account with that name"))?;
            if id == from {
                return Err(AppError::bad("That account already owns this repository"));
            }
            let t = now();
            c.execute(
                "INSERT OR REPLACE INTO repo_transfers(repo_id, from_id, to_id, created_at, expires_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![rid, from, id, t, t + OFFER_SECS],
            )?;
            Ok(id)
        })
        .await?;
    let detail = format!("{} to {}", r.full_name(), req.to.trim());
    record(&st.db, Event { actor: Some(&u), subject: Some(to_id), action: "repo_transfer_offered", detail, ip: &ip.0 }).await;
    ok()
}

pub async fn cancel_transfer(State(st): State<AppState>, Authed(u): Authed, Path((owner, name)): Path<(String, String)>) -> J<Value> {
    let r = Repo::administrable(&st, Some(&u), &owner, &name).await?;
    must_own(&st, &u, r.owner_id).await?;
    let id = r.id;
    st.db.call(move |c| Ok(c.execute("DELETE FROM repo_transfers WHERE repo_id = ?1", [id])?)).await?;
    ok()
}

/// Offers to you (to accept or decline) and offers you made, still open.
pub async fn my_transfers(State(st): State<AppState>, Authed(u): Authed) -> J<Value> {
    let uid = u.id;
    let out = st
        .db
        .call(move |c| {
            let t = now();
            let incoming = rows(
                c,
                "SELECT t.repo_id AS id, fu.username || '/' || r.name AS repo, fu.username AS \"from\", r.description, r.is_private AS private,
                    t.created_at, t.expires_at
                 FROM repo_transfers t JOIN repos r ON r.id = t.repo_id JOIN users fu ON fu.id = r.owner_id
                 WHERE t.to_id IN (SELECT ?1 UNION ALL SELECT org_id FROM org_members WHERE user_id = ?1 AND role = 'owner')
                   AND t.expires_at > ?2 AND r.deleted_at IS NULL AND r.owner_id = t.from_id ORDER BY t.created_at DESC",
                params![uid, t],
            )?;
            let outgoing = rows(
                c,
                "SELECT t.repo_id AS id, ou.username || '/' || r.name AS repo, tu.username AS \"to\", t.created_at, t.expires_at
                 FROM repo_transfers t JOIN repos r ON r.id = t.repo_id JOIN users ou ON ou.id = r.owner_id JOIN users tu ON tu.id = t.to_id
                 WHERE r.owner_id IN (SELECT ?1 UNION ALL SELECT org_id FROM org_members WHERE user_id = ?1 AND role = 'owner')
                   AND t.expires_at > ?2 AND r.deleted_at IS NULL ORDER BY t.created_at DESC",
                params![uid, t],
            )?;
            Ok(json!({ "incoming": incoming, "outgoing": outgoing }))
        })
        .await?;
    Ok(Json(out))
}

/// Take a repository offered to you. It keeps its issues, stars and history;
/// its old address redirects here. It must fit your repository limit and storage.
pub async fn accept(State(st): State<AppState>, Authed(u): Authed, ip: ClientIp, Path(id): Path<i64>) -> J<Value> {
    let s = st.settings.get();
    let (max, quota) = (s.max_repos_per_user, (s.max_storage_mb * 1024 * 1024) as i64);
    let me = u.id;
    let (from_id, from, to, name) = st
        .db
        .call(move |c| {
            let tx = c.transaction()?;
            // Offered to you, or to an organization you own. Its size there: the folder,
            // the wiki, release files and the LFS files that account doesn't hold yet.
            let row: Option<(i64, String, i64, String, String, i64)> = tx
                .query_row(
                    "SELECT r.owner_id, fu.username, t.to_id, tu.username, r.name, r.size_bytes + r.wiki_bytes
                        + COALESCE((SELECT sum(size) FROM lfs_objects
                            WHERE repo_id = r.id AND oid NOT IN (SELECT l.oid FROM lfs_objects l JOIN repos x ON x.id = l.repo_id WHERE x.owner_id = t.to_id)), 0)
                        + COALESCE((SELECT sum(a.size) FROM release_assets a JOIN releases rl ON rl.id = a.release_id WHERE rl.repo_id = r.id), 0)
                        + COALESCE((SELECT sum(size) FROM registry_blobs
                            WHERE repo_id = r.id AND digest NOT IN (SELECT b.digest FROM registry_blobs b JOIN repos x ON x.id = b.repo_id WHERE x.owner_id = t.to_id)), 0)
                     FROM repo_transfers t JOIN repos r ON r.id = t.repo_id JOIN users fu ON fu.id = r.owner_id JOIN users tu ON tu.id = t.to_id
                     WHERE t.repo_id = ?1 AND t.expires_at > ?3 AND r.deleted_at IS NULL AND r.owner_id = t.from_id
                       AND (t.to_id = ?2 OR t.to_id IN (SELECT org_id FROM org_members WHERE user_id = ?2 AND role = 'owner'))",
                    params![id, me, now()],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
                )
                .optional()?;
            let (from_id, from, to_id, to, name, size) = row.ok_or_else(AppError::not_found)?;
            if name_taken(&tx, to_id, &name)? {
                return Err(AppError::conflict(format!("{to} already has a repository named {name}. Rename that one first.")));
            }
            let (payer, admin) = crate::repo::payer(&tx, to_id)?;
            if !admin && live_repos(&tx, payer)? >= max {
                return Err(AppError::bad(format!("That would go over the limit of {max} repositories")));
            }
            if !admin && quota > 0 {
                let (used, _) = crate::repo::usage(&tx, to_id)?;
                if used + size > quota {
                    return Err(AppError::bad("There isn't enough storage left to take this repository"));
                }
            }
            add_redirect(&tx, from_id, &name, id)?;
            claim_name(&tx, to_id, &name)?;
            tx.execute("UPDATE repos SET owner_id = ?2, updated_at = ?3 WHERE id = ?1", params![id, to_id, now()])?;
            // The old owner's keys to it and credentials in it: deploy tokens, CI secrets,
            // and webhooks (their URLs and secrets) don't move with the code.
            tx.execute("DELETE FROM tokens WHERE repo_id = ?1", [id])?;
            tx.execute("DELETE FROM ci_secrets WHERE repo_id = ?1", [id])?;
            tx.execute("DELETE FROM webhooks WHERE repo_id = ?1", [id])?;
            tx.execute("DELETE FROM repo_transfers WHERE repo_id = ?1", [id])?;
            tx.commit()?;
            Ok((from_id, from, to, name))
        })
        .await?;
    let detail = format!("{from}/{name} to {to}");
    record(&st.db, Event { actor: Some(&u), subject: Some(from_id), action: "repo_transferred", detail, ip: &ip.0 }).await;
    Ok(Json(json!({ "owner": to, "name": name })))
}

pub async fn decline(State(st): State<AppState>, Authed(u): Authed, ip: ClientIp, Path(id): Path<i64>) -> J<Value> {
    let uid = u.id;
    let row: Option<(i64, String)> = st
        .db
        .call(move |c| {
            let row = c
                .query_row(
                    "SELECT t.from_id, fu.username || '/' || r.name FROM repo_transfers t JOIN repos r ON r.id = t.repo_id
                     JOIN users fu ON fu.id = r.owner_id WHERE t.repo_id = ?1
                       AND t.to_id IN (SELECT ?2 UNION ALL SELECT org_id FROM org_members WHERE user_id = ?2 AND role = 'owner')",
                    [id, uid],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            if row.is_some() {
                c.execute("DELETE FROM repo_transfers WHERE repo_id = ?1", [id])?;
            }
            Ok(row)
        })
        .await?;
    let (from, full) = row.ok_or_else(AppError::not_found)?;
    record(&st.db, Event { actor: Some(&u), subject: Some(from), action: "repo_transfer_declined", detail: full, ip: &ip.0 }).await;
    ok()
}

// ---------- trash ----------

/// Move a repository to the trash: invisible everywhere, its name free for a
/// new one, its folder in GITTER_DATA/trash until it's restored or purged.
pub async fn trash(st: &AppState, r: &Repo) -> R<()> {
    let id = r.id;
    st.db
        .call(move |c| {
            let tx = c.transaction()?;
            tx.execute(
                "UPDATE repos SET deleted_at = ?2, deleted_name = name, name = '~' || id WHERE id = ?1 AND deleted_at IS NULL",
                params![id, now()],
            )?;
            tx.execute("DELETE FROM repo_transfers WHERE repo_id = ?1", [id])?;
            tx.commit()?;
            // Its CI stops: queued jobs never start, running ones stop at their next heartbeat.
            // Best effort: the claim query skips trashed repositories anyway, and a failure
            // here mustn't leave the row trashed with the folder still in place.
            let runs: Vec<(i64, String)> = {
                let mut q = c.prepare("SELECT id, status FROM ci_runs WHERE repo_id = ?1")?;
                q.query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?
            };
            for (run, _) in runs.iter().filter(|(_, s)| !crate::ci::queue::terminal(s)) {
                if let Err(e) = crate::ci::queue::cancel(c, *run) {
                    eprintln!("trash: cancelling CI run {run}: {e}");
                }
            }
            Ok(())
        })
        .await?;
    // A move that fails puts the row back, so nothing is left half-deleted.
    if let Err(e) = move_dir(&r.path, &trash_path(st, id)).await {
        st.db
            .call(move |c| Ok(c.execute("UPDATE repos SET name = deleted_name, deleted_name = NULL, deleted_at = NULL WHERE id = ?1", [id])?))
            .await?;
        return Err(AppError::internal(format!("moving repository {id} to the trash: {e}")));
    }
    Ok(())
}

/// Delete a trashed repository for good, as deleting did before the trash:
/// the row (with its issues, stars...), the folder, the wiki, and the Git LFS
/// files no other repository holds. False if there's no such repository in the trash.
pub async fn purge(st: &AppState, id: i64, actor: Option<&User>, ip: &str) -> R<bool> {
    let found = st
        .db
        .call(move |c| {
            let tx = c.transaction()?;
            let row: Option<(i64, String, String)> = tx
                .query_row(
                    "SELECT r.owner_id, u.username, COALESCE(r.deleted_name, '') FROM repos r JOIN users u ON u.id = r.owner_id
                     WHERE r.id = ?1 AND r.deleted_at IS NOT NULL",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?;
            let Some(row) = row else { return Ok(None) };
            let oids = crate::lfs::held_by(&tx, false, id)?;
            let releases = super::releases::ids_of(&tx, false, id)?;
            let blobs = crate::registry::held_by(&tx, false, id)?;
            tx.execute("DELETE FROM repos WHERE id = ?1", [id])?;
            tx.commit()?;
            Ok(Some((row, oids, releases, blobs)))
        })
        .await?;
    let Some(((owner_id, owner, name), oids, releases, blobs)) = found else { return Ok(false) };
    crate::lfs::collect(st, oids).await;
    super::releases::remove_files(st, releases).await;
    crate::registry::collect(st, blobs).await;
    // The live path too, in case moving it into the trash failed halfway.
    for dir in [trash_path(st, id), st.git.repo_path(id), super::wiki::path(st, id)] {
        if let Err(e) = tokio::fs::remove_dir_all(&dir).await
            && e.kind() != std::io::ErrorKind::NotFound
        {
            eprintln!("removing {}: {e}", dir.display());
        }
    }
    record(&st.db, Event { actor, subject: Some(owner_id), action: "repo_purged", detail: format!("{owner}/{name}"), ip }).await;
    Ok(true)
}

/// Trashed repositories matching `cond` (over `r`), newest first.
fn trashed(c: &Connection, cond: &str, p: impl rusqlite::Params) -> rusqlite::Result<Vec<Value>> {
    rows(
        c,
        &format!(
            "SELECT r.id, r.deleted_name AS name, u.username AS owner, r.description, r.is_private AS private, r.deleted_at,
                r.deleted_at + {TRASH_SECS} AS purge_at,
                EXISTS (SELECT 1 FROM repos x WHERE x.owner_id = r.owner_id AND x.name = r.deleted_name) AS name_taken
             FROM repos r JOIN users u ON u.id = r.owner_id WHERE r.deleted_at IS NOT NULL AND {cond} ORDER BY r.deleted_at DESC LIMIT 500"
        ),
        p,
    )
}

/// Your recently deleted repositories.
pub async fn my_trash(State(st): State<AppState>, Authed(u): Authed) -> J<Value> {
    let uid = u.id;
    let cond = "(r.owner_id = ?1 OR r.owner_id IN (SELECT org_id FROM org_members WHERE user_id = ?1 AND role = 'owner'))";
    let repos = st.db.call(move |c| Ok(trashed(c, cond, [uid])?)).await?;
    Ok(Json(json!({ "repos": repos })))
}

/// Every repository in the trash, for admins.
pub async fn all_trash(State(st): State<AppState>, _a: Admin) -> J<Value> {
    let repos = st.db.call(|c| Ok(trashed(c, "1 = 1", [])?)).await?;
    Ok(Json(json!({ "repos": repos })))
}

/// Bring a repository back from the trash, under its old name (which must be free).
pub async fn restore(State(st): State<AppState>, Authed(u): Authed, ip: ClientIp, Path(id): Path<i64>) -> J<Value> {
    let max = st.settings.get().max_repos_per_user;
    if !tokio::fs::try_exists(trash_path(&st, id)).await.unwrap_or(false) {
        return Err(AppError::internal(format!("repository {id}'s files are missing from the trash")));
    }
    let u2 = u.clone();
    let (owner, name) = st
        .db
        .call(move |c| {
            let tx = c.transaction()?;
            let (owner_id, owner, name): (i64, String, String) = tx
                .query_row(
                    "SELECT r.owner_id, u.username, r.deleted_name FROM repos r JOIN users u ON u.id = r.owner_id WHERE r.id = ?1 AND r.deleted_at IS NOT NULL",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?
                .ok_or_else(AppError::not_found)?;
            if !owns(&tx, &u2, owner_id)? {
                return Err(AppError::not_found());
            }
            if name_taken(&tx, owner_id, &name)? {
                return Err(AppError::conflict(format!("{owner} already has a repository named {name}. Rename or delete it, then restore this one.")));
            }
            let (payer, admin) = crate::repo::payer(&tx, owner_id)?;
            if !admin && live_repos(&tx, payer)? >= max {
                return Err(AppError::bad(format!("That would go over the limit of {max} repositories")));
            }
            tx.execute("UPDATE repos SET name = deleted_name, deleted_name = NULL, deleted_at = NULL WHERE id = ?1", [id])?;
            claim_name(&tx, owner_id, &name)?;
            tx.commit()?;
            Ok((owner, name))
        })
        .await?;
    if let Err(e) = move_dir(&trash_path(&st, id), &st.git.repo_path(id)).await {
        st.db
            .call(move |c| Ok(c.execute("UPDATE repos SET deleted_at = ?2, deleted_name = name, name = '~' || id WHERE id = ?1", params![id, now()])?))
            .await?;
        return Err(AppError::internal(format!("restoring repository {id}: {e}")));
    }
    record(&st.db, Event { actor: Some(&u), subject: Some(u.id), action: "repo_restored", detail: format!("{owner}/{name}"), ip: &ip.0 }).await;
    Ok(Json(json!({ "owner": owner, "name": name })))
}

/// Admins can empty a repository from the trash without waiting the week.
pub async fn purge_now(State(st): State<AppState>, Admin(me): Admin, _s: Sudo, ip: ClientIp, Path(id): Path<i64>) -> J<Value> {
    if !purge(&st, id, Some(&me), &ip.0).await? {
        return Err(AppError::not_found());
    }
    ok()
}

/// Hourly: purge repositories trashed over a week ago, and drop expired offers
/// and exports. Returns how many repositories were purged.
pub async fn sweep(st: &AppState) -> R<usize> {
    let t = now();
    let (due, gone) = st
        .db
        .call(move |c| {
            let due: Vec<i64> = {
                let mut q = c.prepare("SELECT id FROM repos WHERE deleted_at IS NOT NULL AND deleted_at < ?1")?;
                q.query_map([t - TRASH_SECS], |r| r.get(0))?.collect::<Result<_, _>>()?
            };
            c.execute("DELETE FROM repo_transfers WHERE expires_at < ?1", [t])?;
            let gone: Vec<(i64, i64)> = {
                let mut q = c.prepare("SELECT user_id, id FROM exports WHERE status = 'ready' AND expires_at < ?1")?;
                q.query_map([t], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?
            };
            c.execute("DELETE FROM exports WHERE status = 'ready' AND expires_at < ?1", [t])?;
            Ok((due, gone))
        })
        .await?;
    for (uid, id) in gone {
        let _ = tokio::fs::remove_file(export_path(st, uid, id)).await;
    }
    let mut n = 0;
    for id in due {
        n += purge(st, id, None, "").await? as usize;
    }
    Ok(n)
}

/// Background upkeep, from boot: the hourly `sweep`.
pub async fn upkeep(st: AppState) {
    // No export survives a restart: say so, and drop their half-written files.
    let failed = "UPDATE exports SET status = 'failed', error = 'The server restarted while it was being made. Start a new export.' WHERE status = 'running'";
    if let Err(e) = st.db.call(move |c| Ok(c.execute(failed, [])?)).await {
        eprintln!("exports: {}", e.msg);
    }
    if let Ok(rd) = std::fs::read_dir(st.cfg.data_dir.join("exports")) {
        for e in rd.flatten().filter(|e| e.file_name().to_string_lossy().ends_with(".tmp")) {
            let _ = std::fs::remove_dir_all(e.path()).or_else(|_| std::fs::remove_file(e.path()));
        }
    }
    loop {
        match sweep(&st).await {
            Ok(n) if n > 0 => println!("trash: purged {n} repositories deleted over a week ago"),
            Ok(_) => {}
            Err(e) => eprintln!("trash sweep: {}", e.msg),
        }
        tokio::time::sleep(Duration::from_secs(3600)).await;
    }
}

// ---------- accounts ----------

/// Delete account `name` and everything it owns. Its repositories, live or in
/// the trash, are purged the way the trash purges (LFS files collected, folders removed).
pub async fn delete_account(st: &AppState, actor: &User, name: String, ip: &str) -> R<()> {
    let (me, ip2) = (actor.clone(), ip.to_string());
    let (repo_ids, snippet_ids, lfs, releases, exports, blobs) = st
        .db
        .call(move |c| {
            let tx = c.transaction()?;
            let id: i64 = tx.query_row("SELECT id FROM users WHERE username = ?1", [&name], |r| r.get(0))?;
            super::admin::ensure_another_admin(&tx, id)?;
            let sole: Option<String> = tx
                .query_row(
                    "SELECT o.username FROM org_members m JOIN users o ON o.id = m.org_id WHERE m.user_id = ?1 AND m.role = 'owner'
                     AND NOT EXISTS (SELECT 1 FROM org_members x WHERE x.org_id = m.org_id AND x.role = 'owner' AND x.user_id != ?1) LIMIT 1",
                    [id],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(org) = sole {
                return Err(AppError::bad(format!("{name} is the only owner of {org}: make someone else an owner first, or delete it")));
            }
            let ids: Vec<i64> = {
                let mut s = tx.prepare("SELECT id FROM repos WHERE owner_id = ?1")?;
                s.query_map([id], |r| r.get(0))?.collect::<Result<_, _>>()?
            };
            let exports: Vec<i64> = {
                let mut s = tx.prepare("SELECT id FROM exports WHERE user_id = ?1")?;
                s.query_map([id], |r| r.get(0))?.collect::<Result<_, _>>()?
            };
            let snippets = super::snippets::ids_of(&tx, id)?;
            let lfs = crate::lfs::held_by(&tx, true, id)?;
            let releases = super::releases::ids_of(&tx, true, id)?;
            let blobs = crate::registry::held_by(&tx, true, id)?;
            tx.execute("DELETE FROM users WHERE id = ?1", [id])?;
            let e = Event { actor: Some(&me), subject: None, action: "user_deleted", detail: format!("{name} and {} repositories", ids.len()), ip: &ip2 };
            crate::security::append(&tx, &e)?;
            // After the entry above, so deleting yourself doesn't leave your id on it either.
            crate::security::forget_user(&tx, id)?;
            tx.commit()?;
            Ok((ids, snippets, lfs, releases, exports.into_iter().map(|e| (id, e)).collect::<Vec<_>>(), blobs))
        })
        .await?;
    crate::lfs::collect(st, lfs).await;
    super::releases::remove_files(st, releases).await;
    crate::registry::collect(st, blobs).await;
    for id in repo_ids {
        for dir in [st.git.repo_path(id), trash_path(st, id), super::wiki::path(st, id)] {
            let _ = tokio::fs::remove_dir_all(dir).await;
        }
    }
    for id in snippet_ids {
        let _ = tokio::fs::remove_dir_all(super::snippets::path(st, &id)).await;
    }
    for (uid, id) in exports {
        let _ = tokio::fs::remove_file(export_path(st, uid, id)).await;
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct DeleteMe {
    confirm: String,
}

/// Delete your own account (typed username, fresh password). Signs you out.
pub async fn delete_me(State(st): State<AppState>, Sudo(u): Sudo, ip: ClientIp, Json(d): Json<DeleteMe>) -> R<Response> {
    if !d.confirm.trim().eq_ignore_ascii_case(&u.username) {
        return Err(AppError::bad(format!("Type {} to confirm", u.username)));
    }
    delete_account(&st, &u, u.username.clone(), &ip.0).await?;
    Ok(([(header::SET_COOKIE, crate::auth::clear_cookie(st.cfg.secure_cookies))], Json(json!({ "ok": true }))).into_response())
}

// ---------- data export ----------

/// Start building your data export in the background (one at a time).
pub async fn start_export(State(st): State<AppState>, Sudo(u): Sudo) -> J<Value> {
    if let Err(wait) = st.limiter.check(&format!("export:{}", u.id), 10, 86400) {
        return Err(AppError::slow_down("You've made a lot of exports today. Try again tomorrow.", wait));
    }
    let uid = u.id;
    let (id, old) = st
        .db
        .call(move |c| {
            let tx = c.transaction()?;
            let old: Option<(i64, String, i64)> = tx
                .query_row("SELECT id, status, created_at FROM exports WHERE user_id = ?1", [uid], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .optional()?;
            if old.as_ref().is_some_and(|o| o.1 == "running" && o.2 > now() - 3600) {
                return Err(AppError::conflict("Your data export is still being prepared"));
            }
            tx.execute("DELETE FROM exports WHERE user_id = ?1", [uid])?;
            tx.execute("INSERT INTO exports(user_id, created_at) VALUES (?1, ?2)", [uid, now()])?;
            let id = tx.last_insert_rowid();
            tx.commit()?;
            Ok((id, old.map(|o| o.0)))
        })
        .await?;
    if let Some(old) = old {
        let _ = tokio::fs::remove_file(export_path(&st, uid, old)).await;
    }
    tokio::spawn(run_export(st.clone(), u, id));
    ok()
}

pub async fn export_status(State(st): State<AppState>, Authed(u): Authed) -> J<Value> {
    let uid = u.id;
    let e = st
        .db
        .call(move |c| {
            let mut v = rows(
                c,
                "SELECT status, error, size, created_at, finished_at, expires_at FROM exports
                 WHERE user_id = ?1 AND (status != 'ready' OR expires_at > ?2)",
                params![uid, now()],
            )?;
            Ok(v.pop())
        })
        .await?;
    Ok(Json(json!({ "export": e })))
}

pub async fn download_export(State(st): State<AppState>, Authed(u): Authed) -> R<Response> {
    let uid = u.id;
    let id: i64 = st
        .db
        .call(move |c| {
            Ok(c.query_row(
                "SELECT id FROM exports WHERE user_id = ?1 AND status = 'ready' AND expires_at > ?2",
                params![uid, now()],
                |r| r.get(0),
            )
            .optional()?)
        })
        .await?
        .ok_or_else(AppError::not_found)?;
    let f = tokio::fs::File::open(export_path(&st, uid, id)).await.map_err(|_| AppError::not_found())?;
    let len = f.metadata().await?.len();
    let body = axum::body::Body::from_stream(tokio_util::io::ReaderStream::new(f));
    let name = format!("attachment; filename=\"gitter-{}-export.tar.gz\"", u.username);
    Ok((
        [
            (header::CONTENT_TYPE, "application/gzip".to_string()),
            (header::CONTENT_LENGTH, len.to_string()),
            (header::CONTENT_DISPOSITION, name),
            (header::CACHE_CONTROL, "no-store".to_string()),
        ],
        body,
    )
        .into_response())
}

async fn run_export(st: AppState, u: User, id: i64) {
    let _permit = EXPORTS.acquire().await;
    let out = export_path(&st, u.id, id);
    let res = build_export(&st, &u, &out).await;
    let (status, error, size) = match &res {
        Ok(n) => ("ready", String::new(), *n as i64),
        Err(e) => ("failed", e.msg.clone(), 0),
    };
    let t = now();
    let marked = st
        .db
        .call(move |c| {
            Ok(c.execute(
                "UPDATE exports SET status = ?2, error = ?3, size = ?4, finished_at = ?5, expires_at = ?6 WHERE id = ?1 AND status = 'running'",
                params![id, status, error, size, t, t + EXPORT_SECS],
            )?)
        })
        .await
        .unwrap_or(0);
    // The account (or this export) went while it was being made.
    if res.is_err() || marked == 0 {
        let _ = tokio::fs::remove_file(&out).await;
    }
}

/// Write `out`: profile.json, the account's writing as JSON, and a git bundle
/// of each repository it owns (and its wiki) and each snippet. Returns its size.
async fn build_export(st: &AppState, u: &User, out: &FsPath) -> R<u64> {
    let uid = u.id;
    let (json_files, repos, snippets) = st.db.call(move |c| export_json(c, uid)).await?;
    let tmp = PathBuf::from(format!("{}.tmp", out.display()));
    tokio::fs::create_dir_all(&tmp).await?;
    let res = async {
        let mut bundles: Vec<(String, PathBuf)> = Vec::new();
        for (rid, name) in repos {
            // Wikis in their own folder: a repository may well be named "x.wiki".
            let both = [("repositories", st.git.repo_path(rid), "git"), ("wikis", super::wiki::path(st, rid), "wiki")];
            for (dir, src, kind) in both {
                let dst = tmp.join(format!("{rid}.{kind}.bundle"));
                if src.is_dir() && bundle(st, &src, &dst).await? {
                    bundles.push((format!("{dir}/{name}.bundle"), dst));
                }
            }
        }
        for sid in snippets {
            let (src, dst) = (super::snippets::path(st, &sid), tmp.join(format!("{sid}.bundle")));
            if src.is_dir() && bundle(st, &src, &dst).await? {
                bundles.push((format!("snippets/{sid}.bundle"), dst));
            }
        }
        let out = out.to_path_buf();
        tokio::task::spawn_blocking(move || write_tar(&out, &json_files, &bundles)).await.map_err(AppError::internal)?.map_err(AppError::internal)
    }
    .await;
    let _ = tokio::fs::remove_dir_all(&tmp).await;
    res
}

fn write_tar(out: &FsPath, json_files: &[(&str, Vec<u8>)], bundles: &[(String, PathBuf)]) -> std::io::Result<u64> {
    let part = PathBuf::from(format!("{}.tmp", out.display())).join("export.tar.gz");
    // Owner-only: it holds the account's private repositories and security log.
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
    let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(o.open(&part)?, flate2::Compression::default()));
    for (name, bytes) in json_files {
        let mut h = tar::Header::new_gnu();
        h.set_size(bytes.len() as u64);
        h.set_mode(0o644);
        h.set_mtime(now().max(0) as u64);
        h.set_cksum();
        tar.append_data(&mut h, name, &bytes[..])?;
    }
    for (name, path) in bundles {
        tar.append_path_with_name(path, name)?;
    }
    tar.into_inner()?.finish()?.sync_all()?;
    std::fs::rename(&part, out)?;
    Ok(std::fs::metadata(out)?.len())
}

/// `git bundle create` of every ref. False when there's nothing to bundle yet (an empty repository).
async fn bundle(st: &AppState, repo: &FsPath, out: &FsPath) -> R<bool> {
    let dst = out.to_string_lossy().into_owned();
    let o = st.git.run_timeout(repo, &["bundle", "create", "--quiet", &dst, "--all"], None, &[], Duration::from_secs(600)).await?;
    if o.code == 0 {
        return Ok(true);
    }
    let err = String::from_utf8_lossy(&o.stderr);
    if err.contains("empty bundle") {
        return Ok(false);
    }
    Err(AppError::internal(format!("git bundle {}: {}", repo.display(), err.trim())))
}

type ExportData = (Vec<(&'static str, Vec<u8>)>, Vec<(i64, String)>, Vec<String>);

// ponytail: each JSON file is built in memory; stream rows to the tar if accounts write gigabytes of issues.
fn export_json(c: &Connection, uid: i64) -> R<ExportData> {
    let repo_name = "ou.username || '/' || COALESCE(r.deleted_name, r.name)";
    let mut profile = rows(c, "SELECT username, display_name, email, bio, location, website, is_admin, created_at FROM users WHERE id = ?1", [uid])?
        .pop()
        .ok_or_else(AppError::not_found)?;
    profile["repositories"] = json!(rows(
        c,
        "SELECT name, description, website, is_private AS private, default_branch, created_at, pushed_at FROM repos
         WHERE owner_id = ?1 AND deleted_at IS NULL ORDER BY name",
        [uid]
    )?);
    profile["stars"] = json!(rows(
        c,
        "SELECT ou.username || '/' || r.name AS repo, s.created_at FROM stars s JOIN repos r ON r.id = s.repo_id
         JOIN users ou ON ou.id = r.owner_id WHERE s.user_id = ?1 AND r.deleted_at IS NULL ORDER BY s.created_at",
        [uid]
    )?);
    profile["tokens"] = json!(rows(c, "SELECT name, scope, created_at, last_used, expires_at FROM tokens WHERE user_id = ?1 ORDER BY id", [uid])?);
    let issues = rows(
        c,
        &format!(
            "SELECT {repo_name} AS repo, i.number, CASE WHEN i.is_pr = 1 THEN 'pull request' ELSE 'issue' END AS kind,
                i.title, i.body, i.state, i.created_at, i.updated_at, i.closed_at
             FROM issues i JOIN repos r ON r.id = i.repo_id JOIN users ou ON ou.id = r.owner_id WHERE i.author_id = ?1 ORDER BY i.id"
        ),
        [uid],
    )?;
    let comments = rows(
        c,
        &format!(
            "SELECT {repo_name} AS repo, i.number, cm.kind, cm.body, cm.created_at, cm.updated_at
             FROM comments cm JOIN issues i ON i.id = cm.issue_id JOIN repos r ON r.id = i.repo_id JOIN users ou ON ou.id = r.owner_id
             WHERE cm.author_id = ?1 ORDER BY cm.id"
        ),
        [uid],
    )?;
    let reviews = json!({
        "reviews": rows(c, &format!(
            "SELECT {repo_name} AS repo, i.number, v.state, v.body, v.commit_sha, v.submitted_at
             FROM reviews v JOIN issues i ON i.id = v.issue_id JOIN repos r ON r.id = i.repo_id JOIN users ou ON ou.id = r.owner_id
             WHERE v.author_id = ?1 AND v.state != 'pending' ORDER BY v.id"
        ), [uid])?,
        "line_comments": rows(c, &format!(
            "SELECT {repo_name} AS repo, i.number, rc.path, rc.side, rc.line, rc.start_line, rc.commit_sha, rc.body, rc.created_at, rc.updated_at
             FROM review_comments rc JOIN issues i ON i.id = rc.issue_id JOIN repos r ON r.id = i.repo_id JOIN users ou ON ou.id = r.owner_id
             WHERE rc.author_id = ?1 ORDER BY rc.id"
        ), [uid])?,
    });
    let discussions = json!({
        "discussions": rows(c, &format!(
            "SELECT {repo_name} AS repo, d.number, d.category, d.title, d.body, d.created_at, d.updated_at
             FROM discussions d JOIN repos r ON r.id = d.repo_id JOIN users ou ON ou.id = r.owner_id WHERE d.author_id = ?1 ORDER BY d.id"
        ), [uid])?,
        "replies": rows(c, &format!(
            "SELECT {repo_name} AS repo, d.number, dc.body, dc.created_at, dc.updated_at
             FROM discussion_comments dc JOIN discussions d ON d.id = dc.discussion_id JOIN repos r ON r.id = d.repo_id
             JOIN users ou ON ou.id = r.owner_id WHERE dc.author_id = ?1 ORDER BY dc.id"
        ), [uid])?,
    });
    let snippets = rows(c, "SELECT id, title, is_secret AS secret, files, created_at, updated_at FROM snippets WHERE owner_id = ?1 ORDER BY created_at", [uid])?;
    // Like the security page: someone else's address (an admin acting on the account) is left out.
    let log = rows(
        c,
        "SELECT created_at, action, actor, subject, detail, CASE WHEN actor_id = ?1 THEN ip ELSE '' END AS ip
         FROM audit WHERE subject_id = ?1 OR actor_id = ?1 ORDER BY id",
        [uid],
    )?;
    let repos: Vec<(i64, String)> = {
        let mut q = c.prepare("SELECT id, name FROM repos WHERE owner_id = ?1 AND deleted_at IS NULL")?;
        q.query_map([uid], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?
    };
    // Names and ids become file names: only the characters they're made of anyway.
    let safe = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.')) && !s.contains("..");
    let repos = repos.into_iter().filter(|(_, n)| safe(n)).collect();
    let snippet_ids = super::snippets::ids_of(c, uid)?.into_iter().filter(|s| safe(s)).collect();
    let pretty = |v: &Value| serde_json::to_vec_pretty(v).unwrap_or_default();
    let files = vec![
        ("profile.json", pretty(&profile)),
        ("issues.json", pretty(&json!(issues))),
        ("comments.json", pretty(&json!(comments))),
        ("discussions.json", pretty(&discussions)),
        ("reviews.json", pretty(&reviews)),
        ("snippets.json", pretty(&json!(snippets))),
        ("security-log.json", pretty(&json!(log))),
    ];
    Ok((files, repos, snippet_ids))
}

/// Rows as JSON objects keyed by column name.
fn rows(c: &Connection, sql: &str, p: impl rusqlite::Params) -> rusqlite::Result<Vec<Value>> {
    let mut s = c.prepare(sql)?;
    let names: Vec<String> = s.column_names().into_iter().map(String::from).collect();
    s.query_map(p, |r| {
        let mut m = serde_json::Map::new();
        for (i, n) in names.iter().enumerate() {
            let v = match r.get_ref(i)? {
                ValueRef::Null => Value::Null,
                ValueRef::Integer(x) => json!(x),
                ValueRef::Real(x) => json!(x),
                ValueRef::Text(t) => json!(String::from_utf8_lossy(t)),
                ValueRef::Blob(b) => json!(hex::encode(b)),
            };
            m.insert(n.clone(), v);
        }
        Ok(Value::Object(m))
    })?
    .collect()
}
