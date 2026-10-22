//! Issue tracking depth: milestones with progress, who can be assigned, and
//! issue / pull request templates read from `.gitter/` on the default branch.
//! Setting an issue's milestone and assignees is `issues::update`.

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{J, ok};
use crate::AppState;
use crate::auth::Viewer;
use crate::db::now;
use crate::error::{AppError, R};
use crate::repo::Repo;

const MAX_MILESTONES: i64 = 500;

/// SQL that's true when user `u` (a column or `?`) may write to repository `r`
/// right now: its owner (not an organization), collaborators with write or admin,
/// and members of the owning organization with write. The same rule as
/// `repo::access`, so assignments of someone who loses access stop showing.
pub fn writer_sql(u: &str) -> String {
    format!(
        "(({u} = r.owner_id AND NOT EXISTS (SELECT 1 FROM orgs WHERE id = {u}))
        OR EXISTS (SELECT 1 FROM repo_collaborators rc WHERE rc.repo_id = r.id AND rc.user_id = {u}
            AND (rc.role = 'admin' OR (rc.role = 'write' AND r.hidden = 0)))
        OR EXISTS (SELECT 1 FROM org_members om JOIN orgs o ON o.id = om.org_id WHERE om.org_id = r.owner_id AND om.user_id = {u}
            AND (om.role = 'owner' OR (o.base_role = 'write' AND r.hidden = 0))))"
    )
}

/// The assignees of each issue who can still write, by username.
pub fn assignees_for(c: &rusqlite::Connection, issue_ids: &[i64]) -> rusqlite::Result<std::collections::HashMap<i64, Vec<String>>> {
    let mut m: std::collections::HashMap<i64, Vec<String>> = Default::default();
    if issue_ids.is_empty() {
        return Ok(m);
    }
    let marks = vec!["?"; issue_ids.len()].join(",");
    let mut s = c.prepare(&format!(
        "SELECT ia.issue_id, u.username FROM issue_assignees ia JOIN users u ON u.id = ia.user_id
         JOIN issues i ON i.id = ia.issue_id JOIN repos r ON r.id = i.repo_id
         WHERE ia.issue_id IN ({marks}) AND {} ORDER BY u.username",
        writer_sql("ia.user_id")
    ))?;
    for row in s.query_map(rusqlite::params_from_iter(issue_ids), |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
        let (iid, name) = row?;
        m.entry(iid).or_default().push(name);
    }
    Ok(m)
}

// ---------------------------------------------------------------- milestones

const MILESTONE_SELECT: &str = "SELECT m.number, m.title, m.description, m.due_on, m.state, m.created_at, m.updated_at, m.closed_at,
    (SELECT count(*) FROM issues i WHERE i.milestone_id = m.id AND i.hidden = 0 AND i.state = 'open'),
    (SELECT count(*) FROM issues i WHERE i.milestone_id = m.id AND i.hidden = 0 AND i.state != 'open')
    FROM milestones m";

fn milestone_row(r: &rusqlite::Row) -> rusqlite::Result<Value> {
    Ok(json!({
        "number": r.get::<_, i64>(0)?, "title": r.get::<_, String>(1)?, "description": r.get::<_, String>(2)?,
        "due_on": r.get::<_, Option<String>>(3)?, "state": r.get::<_, String>(4)?, "created_at": r.get::<_, i64>(5)?,
        "updated_at": r.get::<_, i64>(6)?, "closed_at": r.get::<_, Option<i64>>(7)?,
        "open": r.get::<_, i64>(8)?, "closed": r.get::<_, i64>(9)?,
    }))
}

#[derive(Deserialize)]
pub struct ListQ {
    state: Option<String>,
}

pub async fn list(State(st): State<AppState>, Viewer(v): Viewer, Path((o, n)): Path<(String, String)>, Query(q): Query<ListQ>) -> J<Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let rid = r.id;
    let out = st
        .db
        .call(move |c| {
            let cond = match q.state.as_deref() {
                Some("closed") => " AND m.state = 'closed'",
                Some("all") => "",
                _ => " AND m.state = 'open'",
            };
            // Soonest due first, then the ones without a date, newest last.
            let mut s = c.prepare(&format!("{MILESTONE_SELECT} WHERE m.repo_id = ?1{cond} ORDER BY m.due_on IS NULL, m.due_on, m.number"))?;
            let items: Vec<Value> = s.query_map([rid], milestone_row)?.collect::<Result<_, _>>()?;
            let count = |state: &str| -> rusqlite::Result<i64> {
                c.query_row("SELECT count(*) FROM milestones WHERE repo_id = ?1 AND state = ?2", rusqlite::params![rid, state], |r| r.get(0))
            };
            Ok(json!({ "milestones": items, "counts": { "open": count("open")?, "closed": count("closed")? } }))
        })
        .await?;
    Ok(Json(out))
}

pub async fn show(State(st): State<AppState>, Viewer(v): Viewer, Path((o, n, num)): Path<(String, String, i64)>) -> J<Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let rid = r.id;
    let m = st
        .db
        .call(move |c| Ok(c.query_row(&format!("{MILESTONE_SELECT} WHERE m.repo_id = ?1 AND m.number = ?2"), [rid, num], milestone_row)?))
        .await?;
    Ok(Json(m))
}

#[derive(Deserialize)]
pub struct MilestoneReq {
    title: Option<String>,
    description: Option<String>,
    /// YYYY-MM-DD, or "" for none.
    due_on: Option<String>,
    state: Option<String>,
}

fn check_title(t: &str) -> R<String> {
    let t = t.trim();
    if t.is_empty() || t.chars().count() > 100 {
        return Err(AppError::bad("Milestone titles are 1 to 100 characters"));
    }
    Ok(t.to_string())
}

fn title_taken(e: rusqlite::Error) -> AppError {
    match AppError::from(e) {
        c if c.status == axum::http::StatusCode::CONFLICT => AppError::conflict("There's already a milestone with that title"),
        other => other,
    }
}

fn check_description(d: &str) -> R<()> {
    if d.len() > 10_000 {
        return Err(AppError::bad("Keep the description under 10,000 characters"));
    }
    Ok(())
}

/// "" clears it; anything else must be a real-looking YYYY-MM-DD date.
fn check_due(d: &str) -> R<Option<String>> {
    let d = d.trim();
    if d.is_empty() {
        return Ok(None);
    }
    let b = d.as_bytes();
    let digits = |r: std::ops::Range<usize>| b[r].iter().all(u8::is_ascii_digit);
    let ok = b.len() == 10 && b[4] == b'-' && b[7] == b'-' && digits(0..4) && digits(5..7) && digits(8..10) && {
        let (m, day): (u32, u32) = (d[5..7].parse().unwrap_or(0), d[8..10].parse().unwrap_or(0));
        (1..=12).contains(&m) && (1..=31).contains(&day)
    };
    if !ok {
        return Err(AppError::bad("Due dates look like 2027-01-31"));
    }
    Ok(Some(d.to_string()))
}

pub async fn create(State(st): State<AppState>, Viewer(v): Viewer, Path((o, n)): Path<(String, String)>, Json(req): Json<MilestoneReq>) -> J<Value> {
    let r = Repo::writable(&st, v.as_ref(), &o, &n).await?;
    let title = check_title(req.title.as_deref().unwrap_or(""))?;
    let description = req.description.unwrap_or_default();
    check_description(&description)?;
    let due = check_due(req.due_on.as_deref().unwrap_or(""))?;
    let rid = r.id;
    let num = st
        .db
        .call(move |c| {
            let tx = c.transaction()?;
            let (count, num): (i64, i64) =
                tx.query_row("SELECT count(*), coalesce(max(number), 0) + 1 FROM milestones WHERE repo_id = ?1", [rid], |r| Ok((r.get(0)?, r.get(1)?)))?;
            if count >= MAX_MILESTONES {
                return Err(AppError::bad(format!("A repository can have up to {MAX_MILESTONES} milestones")));
            }
            let t = now();
            tx.execute(
                "INSERT INTO milestones(repo_id, number, title, description, due_on, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
                rusqlite::params![rid, num, title, description, due, t],
            )
            .map_err(title_taken)?;
            tx.commit()?;
            Ok(num)
        })
        .await?;
    Ok(Json(json!({ "number": num })))
}

pub async fn update(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    Path((o, n, num)): Path<(String, String, i64)>,
    Json(req): Json<MilestoneReq>,
) -> J<Value> {
    let r = Repo::writable(&st, v.as_ref(), &o, &n).await?;
    let title = req.title.as_deref().map(check_title).transpose()?;
    if let Some(d) = &req.description {
        check_description(d)?;
    }
    let due = req.due_on.as_deref().map(check_due).transpose()?;
    let state = match req.state.as_deref() {
        None => None,
        Some(s @ ("open" | "closed")) => Some(s.to_string()),
        Some(_) => return Err(AppError::bad("Milestones are open or closed")),
    };
    let rid = r.id;
    st.db
        .call(move |c| {
            let tx = c.transaction()?;
            let (id, old_state): (i64, String) =
                tx.query_row("SELECT id, state FROM milestones WHERE repo_id = ?1 AND number = ?2", [rid, num], |r| Ok((r.get(0)?, r.get(1)?)))?;
            let t = now();
            if let Some(title) = title {
                tx.execute("UPDATE milestones SET title = ?1 WHERE id = ?2", rusqlite::params![title, id]).map_err(title_taken)?;
            }
            if let Some(d) = req.description {
                tx.execute("UPDATE milestones SET description = ?1 WHERE id = ?2", rusqlite::params![d, id])?;
            }
            if let Some(d) = due {
                tx.execute("UPDATE milestones SET due_on = ?1 WHERE id = ?2", rusqlite::params![d, id])?;
            }
            if let Some(s) = state.filter(|s| *s != old_state) {
                let closed = (s == "closed").then_some(t);
                tx.execute("UPDATE milestones SET state = ?1, closed_at = ?2 WHERE id = ?3", rusqlite::params![s, closed, id])?;
            }
            tx.execute("UPDATE milestones SET updated_at = ?1 WHERE id = ?2", rusqlite::params![t, id])?;
            tx.commit()?;
            Ok(())
        })
        .await?;
    ok()
}

/// Deleting a milestone leaves its issues without one.
pub async fn delete(State(st): State<AppState>, Viewer(v): Viewer, Path((o, n, num)): Path<(String, String, i64)>) -> J<Value> {
    let r = Repo::writable(&st, v.as_ref(), &o, &n).await?;
    let rid = r.id;
    let gone = st
        .db
        .call(move |c| Ok(c.execute("DELETE FROM milestones WHERE repo_id = ?1 AND number = ?2", [rid, num])?))
        .await?;
    if gone == 0 {
        return Err(AppError::not_found());
    }
    ok()
}

// ---------------------------------------------------------------- assignees

/// Who can be assigned here (people who can write), for the sidebar picker.
pub async fn assignable(State(st): State<AppState>, Viewer(v): Viewer, Path((o, n)): Path<(String, String)>) -> J<Value> {
    let r = Repo::writable(&st, v.as_ref(), &o, &n).await?;
    let rid = r.id;
    let users = st
        .db
        .call(move |c| {
            // ponytail: the first 500 by name; search the list server-side if orgs get bigger.
            let mut s = c.prepare(&format!(
                "SELECT u.username FROM users u JOIN repos r ON r.id = ?1 WHERE {} ORDER BY u.username LIMIT 500",
                writer_sql("u.id")
            ))?;
            let v: Vec<String> = s.query_map([rid], |r| r.get(0))?.collect::<Result<_, _>>()?;
            Ok(v)
        })
        .await?;
    Ok(Json(json!({ "users": users })))
}

// ---------------------------------------------------------------- templates

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Template {
    pub file: String,
    pub name: String,
    pub about: String,
    pub title: String,
    pub labels: Vec<String>,
    pub body: String,
}

const TEMPLATE_DIR: &str = ".gitter/ISSUE_TEMPLATE";
const PR_TEMPLATE: &str = ".gitter/pull_request_template.md";
const MAX_TEMPLATES: usize = 20;
const MAX_TEMPLATE_BYTES: u64 = 64 * 1024;

fn unquote(s: &str) -> String {
    let s = s.trim();
    for q in ['"', '\''] {
        if s.len() >= 2 && s.starts_with(q) && s.ends_with(q) {
            return s[1..s.len() - 1].to_string();
        }
    }
    s.to_string()
}

/// A template file: optional `---` front matter (name, about, title, and labels as
/// `[a, b]`, `a, b` or a `- a` list), then the body. Other keys are ignored.
pub fn parse_template(file: &str, text: &str) -> Template {
    let stem = file.strip_suffix(".md").unwrap_or(file);
    let mut t = Template { file: file.to_string(), name: stem.to_string(), body: text.to_string(), ..Default::default() };
    let Some(rest) = text.strip_prefix("---\n").or_else(|| text.strip_prefix("---\r\n")) else { return t };
    let mut front = vec![];
    let mut body = None;
    let mut at = 0;
    for line in rest.split_inclusive('\n') {
        at += line.len();
        if line.trim_end() == "---" {
            body = Some(&rest[at..]);
            break;
        }
        front.push(line.trim_end());
    }
    // No closing line: it's all body.
    let Some(body) = body else { return t };
    t.body = body.to_string();
    let mut key = "";
    for line in front {
        if let Some(item) = line.trim_start().strip_prefix("- ") {
            if key == "labels" {
                t.labels.push(unquote(item));
            }
            continue;
        }
        let Some((k, v)) = line.split_once(':') else { continue };
        key = k.trim();
        let v = v.trim();
        match key {
            "name" if !v.is_empty() => t.name = unquote(v),
            "about" => t.about = unquote(v),
            "title" => t.title = unquote(v),
            "labels" => {
                let v = v.strip_prefix('[').and_then(|v| v.strip_suffix(']')).unwrap_or(v);
                t.labels.extend(v.split(',').map(unquote));
            }
            _ => {}
        }
    }
    t.labels.retain(|l| !l.is_empty() && l.chars().count() <= 50);
    t.labels.truncate(10);
    t.name = t.name.chars().take(100).collect();
    t.about = t.about.chars().take(300).collect();
    t.title = t.title.chars().take(256).collect();
    t
}

/// The issue templates and the pull request template on the default branch,
/// memoised by its commit (so a push shows the new ones).
pub async fn load_templates(st: &AppState, r: &Repo) -> R<(Vec<Template>, Option<String>)> {
    let Some(sha) = st.git.resolve(&r.path, &format!("refs/heads/{}", r.default_branch)).await? else {
        return Ok((vec![], None));
    };
    let key = format!("templates:{}:{sha}", r.id);
    if let Some(hit) = st.git.memo.get(&key) {
        return Ok(hit);
    }
    let mut files: Vec<_> = st
        .git
        .ls_tree(&r.path, &sha, TEMPLATE_DIR)
        .await?
        .into_iter()
        .filter(|e| e.kind == "blob" && e.name.to_ascii_lowercase().ends_with(".md") && e.size.is_some_and(|s| s <= MAX_TEMPLATE_BYTES))
        .collect();
    files.sort_by(|a, b| a.name.cmp(&b.name));
    files.truncate(MAX_TEMPLATES);
    let mut issue = vec![];
    for f in files {
        let bytes = st.git.blob(&r.path, &f.oid).await?;
        issue.push(parse_template(&f.name, &String::from_utf8_lossy(&bytes)));
    }
    let pull = match st.git.stat(&r.path, &sha, PR_TEMPLATE).await? {
        Some((oid, kind, size)) if kind == "blob" && size <= MAX_TEMPLATE_BYTES => {
            Some(String::from_utf8_lossy(&st.git.blob(&r.path, &oid).await?).into_owned())
        }
        _ => None,
    };
    let out = (issue, pull);
    st.git.memo.put(key, &out);
    Ok(out)
}

pub async fn templates(State(st): State<AppState>, Viewer(v): Viewer, Path((o, n)): Path<(String, String)>) -> J<Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let (issue, pull) = load_templates(&st, &r).await?;
    Ok(Json(json!({ "issue": issue, "pull": pull })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn front_matter() {
        let t = parse_template("bug.md", "---\nname: Bug\nlabels: a, 'b'\n---\nbody\n");
        assert_eq!((t.name.as_str(), t.labels.clone(), t.body.as_str()), ("Bug", vec!["a".to_string(), "b".to_string()], "body\n"));
        let t = parse_template("x.md", "---\nname: unclosed\n");
        assert_eq!((t.name.as_str(), t.body.as_str()), ("x", "---\nname: unclosed\n"), "no closing line: all body");
        let t = parse_template("x.md", "---\r\ntitle: \"[x] \"\r\n---\r\nhi");
        assert_eq!((t.title.as_str(), t.body.as_str()), ("[x] ", "hi"));
        assert!(check_due("2027-13-01").is_err() && check_due("2027-1-01").is_err() && check_due("").unwrap().is_none());
    }
}
