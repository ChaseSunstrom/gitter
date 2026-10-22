//! Organizations (accounts that own repositories but can't sign in) and
//! repository collaborators. Who may do what is decided in one place,
//! `repo::access`; this module only edits the memberships it reads.

use axum::Json;
use axum::extract::{Path, State};
use rusqlite::OptionalExtension;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{ClientIp, J, ok};
use crate::AppState;
use crate::auth::{Authed, Sudo, User, Viewer};
use crate::db::now;
use crate::error::{AppError, R};
use crate::repo::Repo;
use crate::security::{Event, record};
use crate::validate;

const MAX_ORGS_PER_USER: i64 = 10;

/// What each role means, in the words the settings page shows.
fn role_text(role: &str) -> &'static str {
    match role {
        "admin" => "can do everything, including settings, access and deleting",
        "write" => "can push (pushes run CI with this repository's secrets), merge pull requests and manage issues",
        _ => "can see and clone it, and comment",
    }
}

fn valid_role(role: &str) -> R<()> {
    if matches!(role, "read" | "write" | "admin") {
        Ok(())
    } else {
        Err(AppError::bad("A role is read, write or admin"))
    }
}

/// An account by name: (id, is_org).
async fn account(st: &AppState, name: &str) -> R<(i64, bool)> {
    let n = name.to_string();
    st.db
        .call(move |c| {
            Ok(c.query_row("SELECT id, is_org FROM users WHERE username = ?1", [&n], |r| Ok((r.get(0)?, r.get::<_, bool>(1)?))).optional()?)
        })
        .await?
        .ok_or_else(|| AppError::new(axum::http::StatusCode::NOT_FOUND, "No account with that name"))
}

// ---------------------------------------------------------------- collaborators

/// Who can reach the repository and how, in plain words, plus the editable collaborator list.
pub async fn collaborators(State(st): State<AppState>, Authed(u): Authed, Path((o, n)): Path<(String, String)>) -> J<Value> {
    let r = Repo::readable(&st, Some(&u), &o, &n).await?;
    let (id, owner_id) = (r.id, r.owner_id);
    let (list, org): (Vec<Value>, Option<(String, String, Vec<(String, String)>)>) = st
        .db
        .call(move |c| {
            let mut q = c.prepare(
                "SELECT u.username, rc.role, rc.created_at, a.username FROM repo_collaborators rc JOIN users u ON u.id = rc.user_id
                 LEFT JOIN users a ON a.id = rc.added_by WHERE rc.repo_id = ?1 ORDER BY u.username",
            )?;
            let list: Vec<Value> = q
                .query_map([id], |r| {
                    let role: String = r.get(1)?;
                    Ok(json!({ "username": r.get::<_, String>(0)?, "role": role, "text": role_text(&role), "created_at": r.get::<_, i64>(2)?, "added_by": r.get::<_, Option<String>>(3)? }))
                })?
                .collect::<Result<_, _>>()?;
            let org = c
                .query_row("SELECT u.username, o.base_role FROM orgs o JOIN users u ON u.id = o.id WHERE o.id = ?1", [owner_id], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()?
                .map(|(name, base): (String, String)| -> rusqlite::Result<_> {
                    let mut q = c.prepare("SELECT u.username, m.role FROM org_members m JOIN users u ON u.id = m.user_id WHERE m.org_id = ?1 AND m.role = 'owner' ORDER BY u.username")?;
                    let owners = q.query_map([owner_id], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
                    Ok((name, base, owners))
                })
                .transpose()?;
            Ok((list, org))
        })
        .await?;
    // The summary everyone with access can read: who reaches it, and what they may do.
    let mut access = Vec::new();
    if !r.private && !r.hidden {
        access.push(json!({ "who": "Anyone on the internet", "text": "can see and clone it" }));
    }
    match &org {
        Some((name, base, owners)) => {
            let names: Vec<&str> = owners.iter().map(|o| o.0.as_str()).collect();
            access.push(json!({ "who": format!("Owners of {name}"), "detail": names.join(", "), "text": role_text("admin") }));
            match base.as_str() {
                "none" => {}
                b => access.push(json!({ "who": format!("Members of {name}"), "text": role_text(b) })),
            }
        }
        None => access.push(json!({ "who": r.owner.clone(), "detail": "owner", "text": role_text("admin") })),
    }
    for c in &list {
        access.push(json!({ "who": c["username"], "detail": c["role"], "text": c["text"] }));
    }
    access.push(json!({ "who": "Site admins", "text": role_text("admin") }));
    Ok(Json(json!({ "access": access, "collaborators": list, "can_admin": r.can_admin })))
}

#[derive(Deserialize)]
pub struct RoleReq {
    role: String,
}

/// Add a collaborator or change their role (the repository's admins).
pub async fn set_collaborator(
    State(st): State<AppState>,
    Sudo(u): Sudo,
    ip: ClientIp,
    Path((o, n, who)): Path<(String, String, String)>,
    Json(req): Json<RoleReq>,
) -> J<Value> {
    let r = Repo::administrable(&st, Some(&u), &o, &n).await?;
    valid_role(&req.role)?;
    let (uid, is_org) = account(&st, &who).await?;
    if is_org {
        return Err(AppError::bad("Add people, not organizations (give an organization's members access through the organization)"));
    }
    if uid == r.owner_id {
        return Err(AppError::bad("That's the owner: they already have full access"));
    }
    let (rid, role, me) = (r.id, req.role.clone(), u.id);
    let added = st
        .db
        .call(move |c| {
            let before: Option<String> = c.query_row("SELECT role FROM repo_collaborators WHERE repo_id = ?1 AND user_id = ?2", [rid, uid], |r| r.get(0)).optional()?;
            c.execute(
                "INSERT INTO repo_collaborators(repo_id, user_id, role, added_by, created_at) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(repo_id, user_id) DO UPDATE SET role = ?3",
                rusqlite::params![rid, uid, role, me, now()],
            )?;
            Ok(before.is_none())
        })
        .await?;
    let action = if added { "collaborator_added" } else { "collaborator_changed" };
    record(&st.db, Event { actor: Some(&u), subject: Some(uid), action, detail: format!("{who} as {} on {}", req.role, r.full_name()), ip: &ip.0 }).await;
    ok()
}

/// Remove a collaborator (the repository's admins, or the collaborator leaving).
pub async fn remove_collaborator(State(st): State<AppState>, Authed(u): Authed, ip: ClientIp, Path((o, n, who)): Path<(String, String, String)>) -> J<Value> {
    let r = Repo::readable(&st, Some(&u), &o, &n).await?;
    let (uid, _) = account(&st, &who).await?;
    // Leaving needs a signed-in session: a token can't drop its owner's access.
    if !r.can_admin && !(uid == u.id && u.limits.is_none()) {
        return Err(AppError::new(axum::http::StatusCode::FORBIDDEN, "Only this repository's admins can do that"));
    }
    let rid = r.id;
    let gone = st.db.call(move |c| Ok(c.execute("DELETE FROM repo_collaborators WHERE repo_id = ?1 AND user_id = ?2", [rid, uid])?)).await?;
    if gone == 0 {
        return Err(AppError::not_found());
    }
    record(&st.db, Event { actor: Some(&u), subject: Some(uid), action: "collaborator_removed", detail: format!("{who} from {}", r.full_name()), ip: &ip.0 }).await;
    ok()
}

// ---------------------------------------------------------------- organizations

/// The viewer's role in org `org_id`: "owner", "member" or None.
async fn my_role(st: &AppState, org_id: i64, user: Option<&User>) -> R<Option<String>> {
    let Some(uid) = user.map(|u| u.id) else { return Ok(None) };
    st.db
        .call(move |c| Ok(c.query_row("SELECT role FROM org_members WHERE org_id = ?1 AND user_id = ?2", [org_id, uid], |r| r.get(0)).optional()?))
        .await
}

/// An org by name, for its owners (site admins count as owners).
async fn owned_org(st: &AppState, u: &User, name: &str) -> R<i64> {
    let (id, is_org) = account(st, name).await?;
    if !is_org {
        return Err(AppError::not_found());
    }
    if !u.is_admin && my_role(st, id, Some(u)).await?.as_deref() != Some("owner") {
        return Err(AppError::new(axum::http::StatusCode::FORBIDDEN, "Only the organization's owners can do that"));
    }
    Ok(id)
}

/// May `u` create repositories in org `org_id`? Those who'd have write on all of its repos.
pub async fn can_create_in(st: &AppState, org_id: i64, u: &User) -> R<bool> {
    let uid = u.id;
    Ok(u.is_admin
        || st
            .db
            .call(move |c| {
                Ok(c.query_row(
                    "SELECT m.role = 'owner' OR o.base_role = 'write' FROM org_members m JOIN orgs o ON o.id = m.org_id WHERE m.org_id = ?1 AND m.user_id = ?2",
                    [org_id, uid],
                    |r| r.get::<_, bool>(0),
                )
                .optional()?
                .unwrap_or(false))
            })
            .await?)
}

#[derive(Deserialize)]
pub struct NewOrg {
    name: String,
    #[serde(default)]
    display_name: String,
}

pub async fn create(State(st): State<AppState>, Authed(u): Authed, ip: ClientIp, Json(req): Json<NewOrg>) -> J<Value> {
    let name = req.name.trim().to_string();
    if !validate::username(&name) {
        return Err(AppError::bad("Organization names use letters, numbers, '-', '_' or '.', up to 39 characters"));
    }
    let display: String = req.display_name.trim().chars().take(80).collect();
    let (uid, n2) = (u.id, name.clone());
    let id = st
        .db
        .call(move |c| {
            let tx = c.transaction()?;
            let made: i64 = tx.query_row("SELECT count(*) FROM orgs WHERE created_by = ?1", [uid], |r| r.get(0))?;
            if made >= MAX_ORGS_PER_USER {
                return Err(AppError::bad(format!("You can create up to {MAX_ORGS_PER_USER} organizations")));
            }
            if tx.query_row("SELECT 1 FROM users WHERE username = ?1", [&n2], |_| Ok(())).optional()?.is_some() {
                return Err(AppError::conflict("That name is taken"));
            }
            let t = now();
            // No password can match '!', and login refuses organizations anyway.
            tx.execute(
                "INSERT INTO users(username, email, pass_hash, display_name, is_org, created_at) VALUES (?1, '', '!', ?2, 1, ?3)",
                rusqlite::params![n2, display, t],
            )?;
            let id = tx.last_insert_rowid();
            tx.execute("INSERT INTO orgs(id, created_by, created_at) VALUES (?1, ?2, ?3)", rusqlite::params![id, uid, t])?;
            tx.execute("INSERT INTO org_members(org_id, user_id, role, created_at) VALUES (?1, ?2, 'owner', ?3)", rusqlite::params![id, uid, t])?;
            tx.commit()?;
            Ok(id)
        })
        .await?;
    record(&st.db, Event { actor: Some(&u), subject: Some(u.id), action: "org_created", detail: name.clone(), ip: &ip.0 }).await;
    Ok(Json(json!({ "id": id, "name": name })))
}

/// An organization's profile: its settings, members, and the viewer's role.
pub async fn show(State(st): State<AppState>, Viewer(v): Viewer, Path(name): Path<String>) -> J<Value> {
    let (id, is_org) = account(&st, &name).await?;
    if !is_org {
        return Err(AppError::not_found());
    }
    let (org, members): (Value, Vec<Value>) = st
        .db
        .call(move |c| {
            let org = c.query_row(
                "SELECT u.username, u.display_name, u.bio, u.website, o.base_role, o.created_at FROM orgs o JOIN users u ON u.id = o.id WHERE o.id = ?1",
                [id],
                |r| {
                    Ok(json!({ "name": r.get::<_, String>(0)?, "display_name": r.get::<_, String>(1)?, "bio": r.get::<_, String>(2)?,
                        "website": r.get::<_, String>(3)?, "base_role": r.get::<_, String>(4)?, "created_at": r.get::<_, i64>(5)? }))
                },
            )?;
            let mut q = c.prepare(
                "SELECT u.username, u.display_name, m.role FROM org_members m JOIN users u ON u.id = m.user_id WHERE m.org_id = ?1 ORDER BY m.role = 'owner' DESC, u.username",
            )?;
            let members = q
                .query_map([id], |r| Ok(json!({ "username": r.get::<_, String>(0)?, "display_name": r.get::<_, String>(1)?, "role": r.get::<_, String>(2)? })))?
                .collect::<Result<_, _>>()?;
            Ok((org, members))
        })
        .await?;
    let role = my_role(&st, id, v.as_ref()).await?;
    let can_create = match &v {
        Some(u) => can_create_in(&st, id, u).await?,
        None => false,
    };
    let is_admin = v.as_ref().is_some_and(|u| u.is_admin);
    let is_owner = role.as_deref() == Some("owner") || is_admin;
    // Who belongs to an organization is for its members (and site admins) to see.
    let members = if role.is_some() || is_admin { members } else { vec![] };
    Ok(Json(json!({ "org": org, "members": members, "my_role": role, "is_owner": is_owner, "can_create": can_create })))
}

#[derive(Deserialize)]
pub struct OrgSettings {
    display_name: Option<String>,
    base_role: Option<String>,
    bio: Option<String>,
    website: Option<String>,
}

pub async fn update(State(st): State<AppState>, Authed(u): Authed, ip: ClientIp, Path(name): Path<String>, Json(req): Json<OrgSettings>) -> J<Value> {
    let id = owned_org(&st, &u, &name).await?;
    if let Some(b) = &req.base_role
        && !matches!(b.as_str(), "none" | "read" | "write")
    {
        return Err(AppError::bad("Members get none, read or write"));
    }
    let base = req.base_role.clone();
    st.db
        .call(move |c| {
            if let Some(d) = req.display_name {
                c.execute("UPDATE users SET display_name = ?2 WHERE id = ?1", rusqlite::params![id, d.trim().chars().take(80).collect::<String>()])?;
            }
            if let Some(b) = req.bio {
                c.execute("UPDATE users SET bio = ?2 WHERE id = ?1", rusqlite::params![id, b.trim().chars().take(300).collect::<String>()])?;
            }
            if let Some(w) = req.website {
                c.execute("UPDATE users SET website = ?2 WHERE id = ?1", rusqlite::params![id, w.trim().chars().take(200).collect::<String>()])?;
            }
            if let Some(b) = &req.base_role {
                c.execute("UPDATE orgs SET base_role = ?2 WHERE id = ?1", rusqlite::params![id, b])?;
            }
            Ok(())
        })
        .await?;
    if let Some(b) = base {
        record(&st.db, Event { actor: Some(&u), subject: None, action: "org_changed", detail: format!("{name}: members get {b}"), ip: &ip.0 }).await;
    }
    ok()
}

/// Add a member or change their role (owners). The last owner can't step down.
pub async fn set_member(
    State(st): State<AppState>,
    Authed(u): Authed,
    ip: ClientIp,
    Path((name, who)): Path<(String, String)>,
    Json(req): Json<RoleReq>,
) -> J<Value> {
    let id = owned_org(&st, &u, &name).await?;
    if !matches!(req.role.as_str(), "owner" | "member") {
        return Err(AppError::bad("Members are an owner or a member"));
    }
    let (uid, is_org) = account(&st, &who).await?;
    if is_org {
        return Err(AppError::bad("Organizations can't be members"));
    }
    let role = req.role.clone();
    let added = st
        .db
        .call(move |c| {
            let tx = c.transaction()?;
            let before: Option<String> = tx.query_row("SELECT role FROM org_members WHERE org_id = ?1 AND user_id = ?2", [id, uid], |r| r.get(0)).optional()?;
            tx.execute(
                "INSERT INTO org_members(org_id, user_id, role, created_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(org_id, user_id) DO UPDATE SET role = ?3",
                rusqlite::params![id, uid, role, now()],
            )?;
            last_owner_check(&tx, id)?;
            tx.commit()?;
            Ok(before.is_none())
        })
        .await?;
    let action = if added { "org_member_added" } else { "org_member_changed" };
    record(&st.db, Event { actor: Some(&u), subject: Some(uid), action, detail: format!("{who} as {} of {name}", req.role), ip: &ip.0 }).await;
    ok()
}

fn last_owner_check(tx: &rusqlite::Transaction, org: i64) -> R<()> {
    let owners: i64 = tx.query_row("SELECT count(*) FROM org_members WHERE org_id = ?1 AND role = 'owner'", [org], |r| r.get(0))?;
    if owners == 0 {
        return Err(AppError::bad("An organization needs at least one owner: make someone else an owner first"));
    }
    Ok(())
}

/// Remove a member (owners), or leave.
pub async fn remove_member(State(st): State<AppState>, Authed(u): Authed, ip: ClientIp, Path((name, who)): Path<(String, String)>) -> J<Value> {
    let (id, is_org) = account(&st, &name).await?;
    let (uid, _) = account(&st, &who).await?;
    if !is_org {
        return Err(AppError::not_found());
    }
    if uid != u.id {
        owned_org(&st, &u, &name).await?;
    }
    st.db
        .call(move |c| {
            let tx = c.transaction()?;
            if tx.execute("DELETE FROM org_members WHERE org_id = ?1 AND user_id = ?2", [id, uid])? == 0 {
                return Err(AppError::not_found());
            }
            last_owner_check(&tx, id)?;
            tx.commit()?;
            Ok(())
        })
        .await?;
    record(&st.db, Event { actor: Some(&u), subject: Some(uid), action: "org_member_removed", detail: format!("{who} from {name}"), ip: &ip.0 }).await;
    ok()
}

#[derive(Deserialize)]
pub struct ConfirmReq {
    confirm: String,
}

/// Delete an organization that owns no repositories (owners, after a password check).
pub async fn delete(State(st): State<AppState>, Sudo(u): Sudo, ip: ClientIp, Path(name): Path<String>, Json(req): Json<ConfirmReq>) -> J<Value> {
    let id = owned_org(&st, &u, &name).await?;
    if !req.confirm.eq_ignore_ascii_case(&name) {
        return Err(AppError::bad(format!("Type {name} to confirm")));
    }
    let live: i64 = st.db.call(move |c| Ok(c.query_row("SELECT count(*) FROM repos WHERE owner_id = ?1 AND deleted_at IS NULL", [id], |r| r.get(0))?)).await?;
    if live > 0 {
        return Err(AppError::bad("Delete or transfer this organization's repositories first"));
    }
    // Purges what's left in its trash the way deleting an account does.
    super::lifecycle::delete_account(&st, &u, name.clone(), &ip.0).await?;
    record(&st.db, Event { actor: Some(&u), subject: None, action: "org_deleted", detail: name, ip: &ip.0 }).await;
    ok()
}

/// The organizations the signed-in user belongs to.
pub async fn mine(State(st): State<AppState>, Authed(u): Authed) -> J<Value> {
    let uid = u.id;
    let list: Vec<Value> = st
        .db
        .call(move |c| {
            let mut q = c.prepare(
                "SELECT u.username, u.display_name, m.role, m.role = 'owner' OR o.base_role = 'write' FROM org_members m
                 JOIN users u ON u.id = m.org_id JOIN orgs o ON o.id = m.org_id WHERE m.user_id = ?1 ORDER BY u.username",
            )?;
            Ok(q.query_map([uid], |r| {
                Ok(json!({ "name": r.get::<_, String>(0)?, "display_name": r.get::<_, String>(1)?, "role": r.get::<_, String>(2)?, "can_create": r.get::<_, bool>(3)? }))
            })?
            .collect::<Result<_, _>>()?)
        })
        .await?;
    Ok(Json(json!({ "orgs": list })))
}
