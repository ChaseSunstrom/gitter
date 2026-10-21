//! Repository wikis: markdown pages in their own bare git repository
//! (GITTER_DATA/wikis/<repo id>.git, made on the first edit), so every change
//! is a commit. Readable like the repo, editable by people who can write it.

use std::path::{Path as FsPath, PathBuf};

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{ClientIp, J, ok};
use crate::auth::{Authed, User, Viewer};
use crate::error::{AppError, R};
use crate::git::{Change, CommitSpec, LogQuery};
use crate::repo::Repo;
use crate::security::{Event, record};
use crate::{AppState, validate};

const BRANCH: &str = "main";
const PAGE_CAP: usize = 512 * 1024;

pub fn path(st: &AppState, repo_id: i64) -> PathBuf {
    st.cfg.data_dir.join("wikis").join(format!("{repo_id}.git"))
}

/// Page names become file names (`<name>.md`): letters, digits, '-', '_' and
/// '.', with spaces turned into '-'. Titles show the dashes as spaces.
fn page_name(s: &str) -> R<String> {
    let n = s.trim().replace(' ', "-");
    let ok = !n.is_empty()
        && n.chars().count() <= 100
        // A leading '_' is kept for the app's own paths (/wiki/_new).
        && !n.starts_with(['.', '-', '_'])
        && !n.contains("..")
        && n.chars().all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if !ok {
        return Err(AppError::bad("Page names use letters, numbers, spaces, '-', '_' or '.'"));
    }
    Ok(n)
}

fn title(name: &str) -> String {
    name.replace('-', " ")
}

async fn head(st: &AppState, p: &FsPath) -> R<Option<String>> {
    if !tokio::fs::try_exists(p).await.unwrap_or(false) {
        return Ok(None);
    }
    st.git.resolve(p, &format!("refs/heads/{BRANCH}")).await
}

/// Blob id of `<name>.md` at `sha`.
async fn page_oid(st: &AppState, p: &FsPath, sha: Option<&str>, name: &str) -> R<Option<String>> {
    let Some(sha) = sha else { return Ok(None) };
    let found = st.git.stat(p, sha, &format!("{name}.md")).await?;
    Ok(found.filter(|(_, kind, _)| kind == "blob").map(|(oid, _, _)| oid))
}

async fn page_list(st: &AppState, p: &FsPath, head: Option<&str>) -> R<Vec<Value>> {
    let Some(h) = head else { return Ok(vec![]) };
    Ok(st
        .git
        .ls_tree(p, h, "")
        .await?
        .into_iter()
        .filter(|e| e.kind == "blob")
        .filter_map(|e| e.name.strip_suffix(".md").map(|n| json!({ "name": n, "title": title(n) })))
        .collect())
}

pub async fn pages(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    Path((o, n)): Path<(String, String)>,
) -> J<Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let p = path(&st, r.id);
    let h = head(&st, &p).await?;
    Ok(Json(json!({ "pages": page_list(&st, &p, h.as_deref()).await?, "can_write": r.can_write })))
}

#[derive(Deserialize)]
pub struct RevQ {
    rev: Option<String>,
    page: Option<usize>,
}

/// One page (null when it doesn't exist yet, so the UI can offer to create it) and the page list.
pub async fn show(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    Path((o, n, name)): Path<(String, String, String)>,
    Query(q): Query<RevQ>,
) -> J<Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let name = page_name(&name)?;
    let p = path(&st, r.id);
    let h = head(&st, &p).await?;
    let at = match q.rev.as_deref().filter(|s| !s.is_empty()) {
        Some(rev) if validate::sha(rev) => Some(st.git.resolve(&p, rev).await?.ok_or_else(AppError::not_found)?),
        Some(_) => return Err(AppError::bad("That isn't a valid revision")),
        None => h.clone(),
    };
    let mut page = Value::Null;
    if let Some(oid) = page_oid(&st, &p, at.as_deref(), &name).await? {
        let sha = at.clone().unwrap_or_default();
        let content = String::from_utf8_lossy(&st.git.blob(&p, &oid).await?).into_owned();
        let file = format!("{name}.md");
        let q = LogQuery { range: &sha, path: Some(&file), skip: 0, limit: 1, grep: None, author: None, first_parent: false };
        let last = st.git.log(&p, &q).await?.into_iter().next();
        page = json!({ "name": name, "title": title(&name), "content": content, "oid": oid,
            "sha": sha, "latest": at == h, "last_commit": last });
    }
    Ok(Json(json!({ "page": page, "pages": page_list(&st, &p, h.as_deref()).await?, "can_write": r.can_write })))
}

#[derive(Deserialize)]
pub struct SaveReq {
    name: String,
    content: String,
    #[serde(default)]
    message: String,
    /// Blob id of the page the editor started from; a page changed since then is a conflict.
    expected_oid: Option<String>,
}

pub async fn create(
    State(st): State<AppState>,
    Authed(u): Authed,
    Path((o, n)): Path<(String, String)>,
    Json(req): Json<SaveReq>,
) -> J<Value> {
    save(&st, &u, &o, &n, None, req).await
}

pub async fn update(
    State(st): State<AppState>,
    Authed(u): Authed,
    Path((o, n, name)): Path<(String, String, String)>,
    Json(req): Json<SaveReq>,
) -> J<Value> {
    save(&st, &u, &o, &n, Some(&name), req).await
}

/// Create a page (`old` None) or edit one, renaming it when `req.name` differs.
async fn save(st: &AppState, u: &User, o: &str, n: &str, old: Option<&str>, req: SaveReq) -> J<Value> {
    let r = Repo::writable(st, Some(u), o, n).await?;
    let name = page_name(&req.name)?;
    let old = old.map(page_name).transpose()?;
    if req.content.len() > PAGE_CAP {
        return Err(AppError::bad("A page can be up to 512 KB"));
    }
    let message = req.message.trim();
    if message.len() > 10_000 {
        return Err(AppError::bad("Keep the edit message under 10,000 characters"));
    }
    let p = path(st, r.id);
    if !tokio::fs::try_exists(&p).await.unwrap_or(false) {
        tokio::fs::create_dir_all(p.parent().unwrap_or(&p)).await.map_err(AppError::internal)?;
        // Two first edits at once: the loser's init fails, but the wiki exists, so carry on.
        if let Err(e) = st.git.init_bare(&p, BRANCH, st.settings.get().max_push_bytes()).await
            && !tokio::fs::try_exists(&p).await.unwrap_or(false)
        {
            return Err(e);
        }
    }
    let h = head(st, &p).await?;
    let taken = AppError::conflict("A page with that name already exists");
    let (old_file, new_file) = (old.as_ref().map(|o| format!("{o}.md")), format!("{name}.md"));
    let mut changes = Vec::new();
    match &old {
        Some(old) => {
            let cur = page_oid(st, &p, h.as_deref(), old).await?.ok_or_else(AppError::not_found)?;
            if req.expected_oid.as_ref().is_some_and(|e| *e != cur) {
                return Err(AppError::conflict("Someone else changed this page while you were editing. Reload to see their version."));
            }
            if *old != name {
                if page_oid(st, &p, h.as_deref(), &name).await?.is_some() {
                    return Err(taken);
                }
                changes.push(Change::Delete(old_file.as_deref().unwrap_or_default()));
            }
        }
        None if page_oid(st, &p, h.as_deref(), &name).await?.is_some() => return Err(taken),
        None => {}
    }
    changes.push(Change::Write(&new_file, req.content.as_bytes(), false));
    let msg = match (message.is_empty(), &old) {
        (false, _) => message.to_string(),
        (true, Some(_)) => format!("Update {}", title(&name)),
        (true, None) => format!("Create {}", title(&name)),
    };
    let (an, ae) = u.signature();
    let spec = CommitSpec { branch: BRANCH, parent: h.as_deref(), expect_old: h.as_deref(), changes: &changes, message: &msg, author: (&an, &ae) };
    let sha = st.git.commit_files(&p, &spec).await?;
    Ok(Json(json!({ "name": name, "sha": sha })))
}

pub async fn delete(
    State(st): State<AppState>,
    Authed(u): Authed,
    ip: ClientIp,
    Path((o, n, name)): Path<(String, String, String)>,
) -> J<Value> {
    let r = Repo::writable(&st, Some(&u), &o, &n).await?;
    let name = page_name(&name)?;
    let p = path(&st, r.id);
    let h = head(&st, &p).await?;
    page_oid(&st, &p, h.as_deref(), &name).await?.ok_or_else(AppError::not_found)?;
    let file = format!("{name}.md");
    let changes = [Change::Delete(&file)];
    let msg = format!("Delete {}", title(&name));
    let (an, ae) = u.signature();
    let spec = CommitSpec { branch: BRANCH, parent: h.as_deref(), expect_old: h.as_deref(), changes: &changes, message: &msg, author: (&an, &ae) };
    st.git.commit_files(&p, &spec).await?;
    let detail = format!("{} wiki: {}", r.full_name(), title(&name));
    record(&st.db, Event { actor: Some(&u), subject: Some(r.owner_id), action: "wiki_page_deleted", detail, ip: &ip.0 }).await;
    ok()
}

pub async fn history(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    Path((o, n, name)): Path<(String, String, String)>,
    Query(q): Query<RevQ>,
) -> J<Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let name = page_name(&name)?;
    let p = path(&st, r.id);
    let h = head(&st, &p).await?.ok_or_else(AppError::not_found)?;
    let (page, per) = (super::page(q.page), 30);
    let file = format!("{name}.md");
    let lq = LogQuery { range: &h, path: Some(&file), skip: (page - 1) * per, limit: per + 1, grep: None, author: None, first_parent: false };
    let mut commits = st.git.log(&p, &lq).await?;
    let more = commits.len() > per;
    commits.truncate(per);
    Ok(Json(json!({ "name": name, "title": title(&name), "commits": commits, "has_more": more, "page": page })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_names() {
        assert_eq!(page_name(" Getting started ").unwrap(), "Getting-started");
        assert_eq!(page_name("Über_v1.2").unwrap(), "Über_v1.2");
        for bad in ["", "../x", "a/b", ".git", "-x", "_new", "a..b", "a\\b", "a\nb", &"x".repeat(101)] {
            assert!(page_name(bad).is_err(), "{bad:?}");
        }
    }
}
