use axum::Json;
use axum::extract::{Path, Query, State};
use futures_util::future::join_all;
use rusqlite::OptionalExtension;
use serde::Deserialize;
use serde_json::json;

use super::{J, ok};
use crate::auth::{Authed, Fresh, Sudo, User, Viewer};
use super::ClientIp;
use crate::security::{Event, record};
use crate::db::now;
use crate::error::{AppError, R};
use crate::git::Change;
use crate::repo::{Repo, record_event, touch};
use crate::smart_http::RefUpdate;
use crate::{AppState, validate};

pub const DEFAULT_LABELS: &[(&str, &str)] = &[
    ("bug", "#e5484d"),
    ("enhancement", "#3e9ef5"),
    ("documentation", "#0e9384"),
    ("question", "#a35bd6"),
    ("good first issue", "#46a758"),
    ("help wanted", "#e5932b"),
];

#[derive(Deserialize)]
pub struct CreateReq {
    name: String,
    /// An organization to create it in (default: yourself).
    owner: Option<String>,
    #[serde(default)]
    description: String,
    #[serde(default)]
    private: bool,
    #[serde(default)]
    readme: bool,
}

async fn insert_repo(
    st: &AppState,
    owner: &User,
    name: &str,
    description: &str,
    private: bool,
    forked_from: Option<i64>,
) -> R<i64> {
    insert_repo_limited(st, owner, name, description, private, forked_from, true).await
}

async fn insert_repo_limited(
    st: &AppState,
    owner: &User,
    name: &str,
    description: &str,
    private: bool,
    forked_from: Option<i64>,
    limited: bool,
) -> R<i64> {
    let max = st.settings.get().max_repos_per_user;
    // An organization's repositories count toward its creator's limits.
    let oid = owner.id;
    let (payer, admin) = st.db.call(move |c| Ok(crate::repo::payer(c, oid)?)).await?;
    if !admin
        && limited
        && let Err(wait) = st.limiter.check(&format!("create:{payer}"), 30, 3600)
    {
        return Err(AppError::slow_down("You've created a lot of repositories in the last hour. Try again later.", wait));
    }
    let (n, d) = (name.to_string(), description.to_string());
    st.db
        .call(move |c| {
            let tx = c.transaction()?;
            if !admin && super::lifecycle::live_repos(&tx, payer)? >= max {
                return Err(AppError::bad(format!("You've reached the limit of {max} repositories")));
            }
            let exists = tx.query_row("SELECT 1 FROM repos WHERE owner_id = ?1 AND name = ?2", rusqlite::params![oid, n], |_| Ok(())).optional()?;
            if exists.is_some() {
                return Err(AppError::conflict("You already have a repository with that name"));
            }
            let t = now();
            tx.execute(
                "INSERT INTO repos(owner_id, name, description, is_private, forked_from, created_at, updated_at) VALUES (?1,?2,?3,?4,?5,?6,?6)",
                rusqlite::params![oid, n, d, private as i64, forked_from, t],
            )?;
            let id = tx.last_insert_rowid();
            crate::repo::claim_name(&tx, oid, &n)?;
            for (name, color) in DEFAULT_LABELS {
                tx.execute("INSERT INTO labels(repo_id, name, color) VALUES (?1, ?2, ?3)", rusqlite::params![id, name, color])?;
            }
            tx.commit()?;
            Ok(id)
        })
        .await
}

async fn drop_row(st: &AppState, id: i64) {
    let _ = st
        .db
        .call(move |c| Ok(c.execute("DELETE FROM repos WHERE id = ?1", [id])?))
        .await;
}

/// Who a new repository belongs to: `u`, or the organization `owner` when `u` may create in it.
pub async fn owner_for(st: &AppState, u: &User, owner: Option<&str>) -> R<User> {
    // In an organization, the org owns it: its quota, its name.
    Ok(match owner.filter(|o| !o.eq_ignore_ascii_case(&u.username)) {
        None => u.clone(),
        Some(org) => {
            let o = org.to_string();
            let found: Option<User> = st
                .db
                .call(move |c| {
                    Ok(c.query_row(
                        "SELECT id, username, display_name, email, is_admin FROM users WHERE username = ?1 AND is_org = 1",
                        [&o],
                        User::from_row,
                    )
                    .optional()?)
                })
                .await?;
            let org = found.ok_or_else(|| AppError::bad("No organization with that name"))?;
            if !crate::api::orgs::can_create_in(&st, org.id, &u).await? {
                return Err(AppError::new(axum::http::StatusCode::FORBIDDEN, "You can't create repositories in that organization"));
            }
            org
        }
    })
}

/// An empty repository for an import: no README, no create-rate limit (the import has its own limits).
pub async fn create_empty(st: &AppState, u: &User, owner: &User, name: &str, description: &str, private: bool) -> R<i64> {
    let id = insert_repo_limited(st, owner, name, description, private, None, false).await?;
    if let Err(e) = init_repo(st, u, id, name, description, false).await {
        drop_row(st, id).await;
        let _ = tokio::fs::remove_dir_all(st.git.repo_path(id)).await;
        return Err(e);
    }
    record_event(&st.db, u.id, Some(id), "create", name, "", 1).await?;
    Ok(id)
}

pub async fn create(
    State(st): State<AppState>,
    Authed(u): Authed,
    Json(req): Json<CreateReq>,
) -> J<serde_json::Value> {
    let name = req.name.trim().to_string();
    if !validate::repo_name(&name) {
        return Err(AppError::bad(
            "Repository names use letters, numbers, '-', '_' or '.', and can't end in .git",
        ));
    }
    let description = req.description.trim().to_string();
    if description.chars().count() > 350 {
        return Err(AppError::bad("Keep the description under 350 characters"));
    }
    let owner = owner_for(&st, &u, req.owner.as_deref()).await?;
    let id = insert_repo(&st, &owner, &name, &description, req.private, None).await?;
    let path = st.git.repo_path(id);
    if let Err(e) = init_repo(&st, &u, id, &name, &description, req.readme).await {
        drop_row(&st, id).await;
        let _ = tokio::fs::remove_dir_all(&path).await;
        return Err(e);
    }
    record_event(&st.db, u.id, Some(id), "create", &name, "", 1).await?;
    Ok(Json(json!({ "owner": owner.username, "name": name })))
}

async fn init_repo(
    st: &AppState,
    u: &User,
    id: i64,
    name: &str,
    description: &str,
    readme: bool,
) -> R<()> {
    let path = st.git.repo_path(id);
    st.git
        .init_bare(&path, "main", st.settings.get().max_push_bytes())
        .await?;
    if readme {
        let body = if description.is_empty() {
            format!("# {name}\n")
        } else {
            format!("# {name}\n\n{description}\n")
        };
        let (an, ae) = u.signature();
        let changes = [Change::Write("README.md", body.as_bytes(), false)];
        let spec = crate::git::CommitSpec {
            branch: "main",
            parent: None,
            expect_old: None,
            changes: &changes,
            message: "Initial commit",
            author: (&an, &ae),
        };
        st.git.commit_files(&path, &spec).await?;
        crate::langs::refresh(st, id, &path).await;
    }
    Ok(())
}

pub async fn show(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    Path((owner, name)): Path<(String, String)>,
) -> J<serde_json::Value> {
    let r = Repo::readable(&st, v.as_ref(), &owner, &name).await?;
    let refs = st.git.refs(&r.path).await?;
    let branches: Vec<&str> = refs
        .iter()
        .filter(|x| x.kind == "branch")
        .map(|x| x.name.as_str())
        .collect();
    let tags: Vec<&str> = refs
        .iter()
        .filter(|x| x.kind == "tag")
        .map(|x| x.name.as_str())
        .collect();
    let (rid, vid) = (r.id, v.as_ref().map(|u| u.id).unwrap_or(-1));
    let fork_of = r.forked_from;
    let counts = st
        .db
        .call(move |c| {
            let q = |sql: &str| -> rusqlite::Result<i64> { c.query_row(sql, [rid], |r| r.get(0)) };
            let stars = q("SELECT count(*) FROM stars WHERE repo_id = ?1")?;
            let starred = c.query_row("SELECT count(*) FROM stars WHERE repo_id = ?1 AND user_id = ?2", [rid, vid], |r| r.get::<_, i64>(0))? > 0;
            let forks = q("SELECT count(*) FROM repos WHERE forked_from = ?1 AND deleted_at IS NULL")?;
            let issues = q("SELECT count(*) FROM issues WHERE repo_id = ?1 AND is_pr = 0 AND state = 'open'")?;
            let pulls = q("SELECT count(*) FROM issues WHERE repo_id = ?1 AND is_pr = 1 AND state = 'open'")?;
            let language: String = c.query_row("SELECT language FROM repos WHERE id = ?1", [rid], |r| r.get(0))?;
            let parent: Option<(String, String, bool)> = match fork_of {
                Some(pid) => c
                    .query_row(
                        "SELECT u.username, p.name, p.is_private OR p.hidden OR p.deleted_at IS NOT NULL FROM repos p JOIN users u ON u.id = p.owner_id WHERE p.id = ?1",
                        [pid],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2)? != 0)),
                    )
                    .optional()?,
                None => None,
            };
            let releases = super::releases::summary(c, rid)?;
            Ok((stars, starred, forks, issues, pulls, language, parent, releases))
        })
        .await?;
    let (stars, starred, forks, issues, pulls, language, parent, (releases, latest_release)) = counts;
    let parent = parent
        .filter(|p| !p.2)
        .map(|p| json!({ "owner": p.0, "name": p.1 }));
    let mirror = if r.mirror { crate::mirrors::badge(&st, r.id, r.can_admin).await? } else { serde_json::Value::Null };
    // Who may give it away: the owner, an owner of the owning organization, or a site admin.
    let can_transfer = match v.as_ref().filter(|_| r.can_admin) {
        Some(u) => {
            let (u, oid) = (u.clone(), r.owner_id);
            st.db.call(move |c| Ok(super::lifecycle::owns(c, &u, oid)?)).await?
        }
        None => false,
    };
    Ok(Json(json!({
        "owner": r.owner, "name": r.name, "description": r.description, "website": r.website,
        "private": r.private, "default_branch": r.default_branch, "created_at": r.created_at,
        "updated_at": r.updated_at, "pushed_at": r.pushed_at, "language": language,
        "empty": branches.is_empty(), "branches": branches, "tags": tags,
        "stars": stars, "starred": starred, "forks": forks, "open_issues": issues, "open_pulls": pulls,
        "releases": releases, "latest_release": latest_release,
        "fork_of": parent, "mirror": mirror, "can_write": r.can_write, "can_admin": r.can_admin, "is_owner": r.is_owner, "can_transfer": can_transfer, "hidden": r.hidden,
        "clone_url": format!("{}/{}/{}.git", st.cfg.url.trim_end_matches('/'), r.owner, r.name),
        "ssh_url": st.ssh_url.get().map(|b| format!("{b}{}/{}.git", r.owner, r.name)),
    })))
}

#[derive(Deserialize)]
pub struct UpdateReq {
    name: Option<String>,
    description: Option<String>,
    website: Option<String>,
    private: Option<bool>,
    default_branch: Option<String>,
}

pub async fn update(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    Fresh(fresh): Fresh,
    ip: ClientIp,
    Path((owner, name)): Path<(String, String)>,
    Json(req): Json<UpdateReq>,
) -> J<serde_json::Value> {
    let r = Repo::administrable(&st, v.as_ref(), &owner, &name).await?;
    let visibility = req.private.filter(|p| *p != r.private);
    // Publishing a private repo can't be undone (people may clone it), so it needs a fresh password.
    if visibility.is_some() && !fresh {
        return Err(AppError::reauth());
    }
    if let Some(n) = &req.name
        && !validate::repo_name(n)
    {
        return Err(AppError::bad("That isn't a valid repository name"));
    }
    if let Some(d) = &req.description
        && d.chars().count() > 350
    {
        return Err(AppError::bad("Keep the description under 350 characters"));
    }
    if let Some(w) = &req.website
        && !w.is_empty()
        && !(w.starts_with("https://") || w.starts_with("http://"))
    {
        return Err(AppError::bad("Website must start with https:// or http://"));
    }
    if req.default_branch.is_some() {
        // A pull mirror's default branch follows its remote.
        crate::mirrors::refuse_writes(&r)?;
    }
    if let Some(b) = &req.default_branch
        && (!validate::ref_name(b)
            || st
                .git
                .resolve(&r.path, &format!("refs/heads/{b}"))
                .await?
                .is_none())
    {
        return Err(AppError::bad("That branch doesn't exist"));
    }
    let new_head = req.default_branch.clone();
    let (id, oid, old_name) = (r.id, r.owner_id, r.name.clone());
    st.db
        .call(move |c| {
            let tx = c.transaction()?;
            if let Some(n) = req.name.as_deref().map(str::trim) {
                let clash = tx
                    .query_row(
                        "SELECT 1 FROM repos WHERE owner_id = ?1 AND name = ?2 AND id != ?3",
                        rusqlite::params![oid, n, id],
                        |_| Ok(()),
                    )
                    .optional()?;
                if clash.is_some() {
                    return Err(AppError::conflict(
                        "You already have a repository with that name",
                    ));
                }
                tx.execute(
                    "UPDATE repos SET name = ?1 WHERE id = ?2",
                    rusqlite::params![n, id],
                )?;
                // The old name keeps working (web and API redirect, git is served there).
                crate::repo::add_redirect(&tx, oid, &old_name, id)?;
                crate::repo::claim_name(&tx, oid, n)?;
            }
            if let Some(d) = req.description {
                tx.execute(
                    "UPDATE repos SET description = ?1 WHERE id = ?2",
                    rusqlite::params![d.trim(), id],
                )?;
            }
            if let Some(w) = req.website {
                tx.execute(
                    "UPDATE repos SET website = ?1 WHERE id = ?2",
                    rusqlite::params![w.trim(), id],
                )?;
            }
            if let Some(p) = req.private {
                tx.execute(
                    "UPDATE repos SET is_private = ?1 WHERE id = ?2",
                    rusqlite::params![p as i64, id],
                )?;
            }
            if let Some(b) = req.default_branch {
                tx.execute(
                    "UPDATE repos SET default_branch = ?1 WHERE id = ?2",
                    rusqlite::params![b, id],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
        .await?;
    if let Some(b) = new_head {
        st.git.set_head(&r.path, &b).await?;
    }
    if let Some(p) = visibility {
        let detail = format!("{} made {}", r.full_name(), if p { "private" } else { "public" });
        record(&st.db, Event { actor: v.as_ref(), subject: Some(r.owner_id), action: "repo_visibility", detail, ip: &ip.0 }).await;
    }
    touch(&st.db, id, false).await?;
    let n2: String = st
        .db
        .call(move |c| Ok(c.query_row("SELECT name FROM repos WHERE id = ?1", [id], |r| r.get(0))?))
        .await?;
    Ok(Json(json!({ "owner": r.owner, "name": n2 })))
}

#[derive(Deserialize)]
pub struct DeleteReq {
    confirm: String,
}

pub async fn remove(
    State(st): State<AppState>,
    Sudo(u): Sudo,
    ip: ClientIp,
    Path((owner, name)): Path<(String, String)>,
    Json(req): Json<DeleteReq>,
) -> J<serde_json::Value> {
    let r = Repo::administrable(&st, Some(&u), &owner, &name).await?;
    if !req.confirm.eq_ignore_ascii_case(&r.full_name()) {
        return Err(AppError::bad(format!("Type {} to confirm", r.full_name())));
    }
    // Into the trash for the retention window (restorable); the upkeep loop purges it after.
    super::lifecycle::trash(&st, &r).await?;
    record(&st.db, Event { actor: Some(&u), subject: Some(r.owner_id), action: "repo_deleted", detail: r.full_name(), ip: &ip.0 }).await;
    ok()
}

pub async fn star(
    State(st): State<AppState>,
    Authed(u): Authed,
    Path((owner, name)): Path<(String, String)>,
) -> J<serde_json::Value> {
    let r = Repo::readable(&st, Some(&u), &owner, &name).await?;
    let (rid, uid) = (r.id, u.id);
    let added = st
        .db
        .call(move |c| {
            Ok(c.execute(
                "INSERT OR IGNORE INTO stars(user_id, repo_id, created_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![uid, rid, now()],
            )?)
        })
        .await?;
    if added > 0 {
        record_event(&st.db, u.id, Some(r.id), "star", &r.name, "", 1).await?;
    }
    ok()
}

pub async fn unstar(
    State(st): State<AppState>,
    Authed(u): Authed,
    Path((owner, name)): Path<(String, String)>,
) -> J<serde_json::Value> {
    let r = Repo::readable(&st, Some(&u), &owner, &name).await?;
    let (rid, uid) = (r.id, u.id);
    st.db
        .call(move |c| {
            Ok(c.execute(
                "DELETE FROM stars WHERE user_id = ?1 AND repo_id = ?2",
                [uid, rid],
            )?)
        })
        .await?;
    ok()
}

#[derive(Deserialize)]
pub struct ForkReq {
    name: Option<String>,
}

pub async fn fork(
    State(st): State<AppState>,
    Authed(u): Authed,
    Path((owner, name)): Path<(String, String)>,
    Json(req): Json<ForkReq>,
) -> J<serde_json::Value> {
    let src = Repo::readable(&st, Some(&u), &owner, &name).await?;
    if src.hidden {
        return Err(AppError::bad("A moderator hid this repository, so it can't be forked"));
    }
    let new_name = req.name.unwrap_or_else(|| src.name.clone());
    if !validate::repo_name(&new_name) {
        return Err(AppError::bad("That isn't a valid repository name"));
    }
    let (sid, me) = (src.id, u.id);
    // The fork shares the parent's LFS files, so the ones this account doesn't hold yet count too.
    let size: i64 = st
        .db
        .call(move |c| {
            Ok(c.query_row(
                "SELECT size_bytes + COALESCE((SELECT sum(size) FROM lfs_objects WHERE repo_id = ?1 AND oid NOT IN
                    (SELECT l.oid FROM lfs_objects l JOIN repos r ON r.id = l.repo_id WHERE r.owner_id = ?2)), 0)
                 FROM repos WHERE id = ?1",
                [sid, me],
                |r| r.get(0),
            )?)
        })
        .await?;
    if let Some(left) = crate::repo::storage_left(&st, u.id).await?
        && size.max(0) as u64 > left
    {
        return Err(AppError::bad("You don't have enough storage left to fork this repository"));
    }
    let id = insert_repo(
        &st,
        &u,
        &new_name,
        &src.description,
        src.private,
        Some(src.id),
    )
    .await?;
    let dst = st.git.repo_path(id);
    if let Err(e) = st
        .git
        .clone_bare(&src.path, &dst, st.settings.get().max_push_bytes())
        .await
    {
        drop_row(&st, id).await;
        let _ = tokio::fs::remove_dir_all(&dst).await;
        return Err(e);
    }
    let b = src.default_branch.clone();
    st.db
        .call(move |c| {
            Ok(c.execute(
                "UPDATE repos SET default_branch = ?1 WHERE id = ?2",
                rusqlite::params![b, id],
            )?)
        })
        .await?;
    let lang_src = src.id;
    st.db
        .call(move |c| {
            c.execute("UPDATE repos SET language = (SELECT language FROM repos WHERE id = ?1) WHERE id = ?2", [lang_src, id])?;
            // The fork shares the parent's Git LFS files (stored once by content).
            Ok(c.execute(
                "INSERT OR IGNORE INTO lfs_objects(repo_id, oid, size, created_at) SELECT ?2, oid, size, created_at FROM lfs_objects WHERE repo_id = ?1",
                [lang_src, id],
            )?)
        })
        .await?;
    crate::hooks::refresh_size(&st, id).await;
    record_event(&st.db, u.id, Some(id), "fork", &src.full_name(), "", 1).await?;
    Ok(Json(json!({ "owner": u.username, "name": new_name })))
}

pub async fn branches(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    Path((owner, name)): Path<(String, String)>,
) -> J<serde_json::Value> {
    let r = Repo::readable(&st, v.as_ref(), &owner, &name).await?;
    let refs = st.git.refs(&r.path).await?;
    let def = r.default_branch.clone();
    let list: Vec<_> = refs
        .into_iter()
        .filter(|x| x.kind == "branch")
        .take(200)
        .collect();
    // Ahead/behind costs one git process per branch; only the 60 most recent get it.
    let counts = join_all(list.iter().take(60).map(|b| {
        let (st, path, def, name) = (st.clone(), r.path.clone(), def.clone(), b.name.clone());
        async move {
            if name == def {
                return (0, 0);
            }
            let range = format!("refs/heads/{def}...refs/heads/{name}");
            match st
                .git
                .run(
                    &path,
                    &["rev-list", "--left-right", "--count", &range, "--"],
                    None,
                    &[],
                )
                .await
            {
                Ok(o) if o.code == 0 => {
                    let t = o.text();
                    let mut it = t.split_whitespace().map(|n| n.parse::<i64>().unwrap_or(0));
                    let behind = it.next().unwrap_or(0);
                    let ahead = it.next().unwrap_or(0);
                    (ahead, behind)
                }
                _ => (0, 0),
            }
        }
    }))
    .await;
    let rid = r.id;
    let open_prs: Vec<(String, i64)> = st
        .db
        .call(move |c| {
            let mut s = c.prepare("SELECT p.head_branch, i.number FROM pulls p JOIN issues i ON i.id = p.issue_id WHERE i.repo_id = ?1 AND i.state = 'open' AND p.from_fork = 0")?;
            let v = s.query_map([rid], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
            Ok(v)
        })
        .await?;
    let counts = counts.into_iter().chain(std::iter::repeat((0, 0)));
    let out: Vec<_> = list
        .iter()
        .zip(counts)
        .map(|(b, (ahead, behind))| {
            let pr = open_prs.iter().find(|(h, _)| *h == b.name).map(|(_, n)| *n);
            json!({ "name": b.name, "sha": b.sha, "date": b.date, "subject": b.subject, "author": b.author,
                    "ahead": ahead, "behind": behind, "default": b.name == def, "pull": pr })
        })
        .collect();
    Ok(Json(json!({ "branches": out, "default_branch": def })))
}

#[derive(Deserialize)]
pub struct BranchReq {
    name: String,
    from: String,
}

pub async fn create_branch(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    Path((owner, name)): Path<(String, String)>,
    Json(req): Json<BranchReq>,
) -> J<serde_json::Value> {
    let r = Repo::writable(&st, v.as_ref(), &owner, &name).await?;
    let u = v.ok_or_else(AppError::unauthorized)?;
    crate::mirrors::refuse_writes(&r)?;
    if !validate::ref_name(&req.name) {
        return Err(AppError::bad("That isn't a valid branch name"));
    }
    if !validate::rev(&req.from) {
        return Err(AppError::bad("Unknown starting point"));
    }
    let sha = st
        .git
        .resolve(&r.path, &req.from)
        .await?
        .ok_or_else(|| AppError::bad("Unknown starting point"))?;
    let refname = format!("refs/heads/{}", req.name);
    if let Some(x) = st.git.create_ref(&r.path, &refname, &sha).await? {
        return Err(crate::git::in_way(&refname, &x));
    }
    touch(&st.db, r.id, false).await?;
    let upd = RefUpdate { refname, old: None, new: Some(sha.clone()) };
    crate::hooks::on_push(&st, &r, &u, &[upd]).await;
    Ok(Json(json!({ "name": req.name, "sha": sha })))
}

#[derive(Deserialize)]
pub struct NameQ {
    name: String,
}

pub async fn delete_branch(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    Path((owner, name)): Path<(String, String)>,
    Query(q): Query<NameQ>,
) -> J<serde_json::Value> {
    let r = Repo::writable(&st, v.as_ref(), &owner, &name).await?;
    let u = v.ok_or_else(AppError::unauthorized)?;
    crate::mirrors::refuse_writes(&r)?;
    if !validate::ref_name(&q.name) {
        return Err(AppError::bad("That isn't a valid branch name"));
    }
    if q.name == r.default_branch {
        return Err(AppError::bad("You can't delete the default branch"));
    }
    crate::protect::direct(&st, &r, &q.name, true).await?;
    let refname = format!("refs/heads/{}", q.name);
    // Already gone: nothing to delete or announce.
    let Some(old) = st.git.resolve(&r.path, &refname).await? else { return ok() };
    // Only the commit looked at, so the deletion webhook names what was deleted.
    st.git.ok(&r.path, &["update-ref", "-d", &refname, &old]).await?;
    touch(&st.db, r.id, false).await?;
    let upd = RefUpdate { refname, old: Some(old), new: None };
    crate::hooks::on_push(&st, &r, &u, &[upd]).await;
    ok()
}

pub async fn tags(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    Path((owner, name)): Path<(String, String)>,
) -> J<serde_json::Value> {
    let r = Repo::readable(&st, v.as_ref(), &owner, &name).await?;
    let refs = st.git.refs(&r.path).await?;
    let tags: Vec<_> = refs.into_iter().filter(|x| x.kind == "tag").collect();
    Ok(Json(json!({ "tags": tags })))
}

// ---------- deploy tokens: access to this one repository, for CI and servers ----------

pub async fn deploy_tokens(
    State(st): State<AppState>,
    Authed(u): Authed,
    Path((owner, name)): Path<(String, String)>,
) -> J<serde_json::Value> {
    let r = Repo::administrable(&st, Some(&u), &owner, &name).await?;
    let rid = r.id;
    let rows = st
        .db
        .call(move |c| super::account::token_rows(c, "t.repo_id = ?1", rid))
        .await?;
    Ok(Json(json!({ "tokens": rows })))
}

#[derive(Deserialize)]
pub struct DeployReq {
    name: String,
    /// Allow pushing too (read-write), not just cloning.
    #[serde(default)]
    write: bool,
    expires_days: Option<i64>,
}

pub async fn create_deploy_token(
    State(st): State<AppState>,
    Sudo(u): Sudo,
    ip: ClientIp,
    Path((owner, name)): Path<(String, String)>,
    Json(req): Json<DeployReq>,
) -> J<serde_json::Value> {
    let r = Repo::administrable(&st, Some(&u), &owner, &name).await?;
    let t = super::account::TokenReq::deploy(req.name, req.write, req.expires_days);
    let raw = super::account::issue_token(&st, &u, &ip.0, t, Some((r.id, r.full_name()))).await?;
    Ok(Json(json!({ "token": raw })))
}

pub async fn delete_deploy_token(
    State(st): State<AppState>,
    Sudo(u): Sudo,
    ip: ClientIp,
    Path((owner, name, id)): Path<(String, String, i64)>,
) -> J<serde_json::Value> {
    let r = Repo::administrable(&st, Some(&u), &owner, &name).await?;
    let rid = r.id;
    let gone: Option<String> = st
        .db
        .call(move |c| {
            let n = c
                .query_row("SELECT name FROM tokens WHERE id = ?1 AND repo_id = ?2", [id, rid], |r| r.get(0))
                .optional()?;
            c.execute("DELETE FROM tokens WHERE id = ?1 AND repo_id = ?2", [id, rid])?;
            Ok(n)
        })
        .await?;
    let Some(n) = gone else { return Err(AppError::not_found()) };
    let detail = format!("{n} (deploy token for {})", r.full_name());
    record(&st.db, Event { actor: Some(&u), subject: Some(r.owner_id), action: "token_deleted", detail, ip: &ip.0 }).await;
    ok()
}
