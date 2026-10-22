//! The merge queue. Pull requests into a branch whose rule asks for one merge one
//! at a time, each only after its checks pass on the very commit it would land as:
//! the first in line is merged onto the branch's tip (kept at refs/gitter-queue/<id>,
//! hidden from clients), CI runs there, and the branch moves to that commit when
//! it passes. A push to the pull request takes it out of the line; the branch
//! moving under it means testing again on the new tip.

use axum::Json;
use axum::extract::{Path, State};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::AppState;
use crate::api::issues::{MergePlan, MergeReq, land_merge, prepare_merge};
use crate::api::{J, ok};
use crate::auth::{Authed, Viewer};
use crate::db::now;
use crate::error::{AppError, R};
use crate::protect;
use crate::repo::Repo;

pub const REF_PREFIX: &str = "refs/gitter-queue";
/// Checks still unfinished this long after testing began take the pull request out.
const TEST_TIMEOUT: i64 = 3600;

// ponytail: one lock for every queue on the site; per-repository locks if queues get busy.
static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

struct Entry {
    id: i64,
    issue_id: i64,
    number: i64,
    title: String,
    open: bool,
    head_branch: String,
    base: String,
    method: String,
    added_by: i64,
    head_sha: String,
    state: String,
    test_sha: Option<String>,
    tested_base: Option<String>,
    started_at: Option<i64>,
}

#[derive(Deserialize)]
pub struct QueueReq {
    #[serde(default = "merge")]
    method: String,
}

fn merge() -> String {
    "merge".into()
}

/// Add a pull request to its target branch's queue (people who can merge it).
pub async fn enqueue(State(st): State<AppState>, Authed(u): Authed, Path((o, n, num)): Path<(String, String, i64)>, Json(req): Json<QueueReq>) -> J<Value> {
    let r = Repo::writable(&st, Some(&u), &o, &n).await?;
    // Fast-forward can't work behind another entry: the branch has moved by then.
    if !matches!(req.method.as_str(), "merge" | "squash") {
        return Err(AppError::bad("The merge queue merges with a merge commit or a squash"));
    }
    let (rid, n2) = (r.id, num);
    let pull: Option<(String, String, bool)> = st
        .db
        .call(move |c| {
            Ok(c.query_row(
                "SELECT p.head_branch, p.base_branch, p.from_fork FROM pulls p JOIN issues i ON i.id = p.issue_id WHERE i.repo_id = ?1 AND i.number = ?2 AND i.state = 'open'",
                params![rid, n2],
                |x| Ok((x.get(0)?, x.get(1)?, x.get(2)?)),
            )
            .optional()?)
        })
        .await?;
    let Some((head_b, base_b, from_fork)) = pull else { return Err(AppError::conflict("That pull request isn't open")) };
    if from_fork {
        // The queue tests with CI, which would run the fork's code with this repository's secrets.
        return Err(AppError::conflict("Pull requests from forks can't go through the merge queue"));
    }
    let b2 = base_b.clone();
    let queued = st.db.call(move |c| Ok(protect::effective(&protect::load(c, rid)?, &b2).is_some_and(|x| x.merge_queue))).await?;
    if !queued {
        return Err(AppError::conflict(format!("{base_b} doesn't use a merge queue")));
    }
    let head = st.git.resolve(&r.path, &format!("refs/heads/{head_b}")).await?.ok_or_else(|| AppError::conflict("The source branch is gone"))?;
    // Everything a merge needs but the checks (those run on the queue's own commit).
    let plan = prepare_merge(&st, &r, &u, num, MergeReq::queued(&req.method, &head), true).await?;
    let (iid, uid, m, h) = (plan.issue_id, u.id, req.method.clone(), plan.head.clone());
    let added = st
        .db
        .call(move |c| {
            Ok(c.execute(
                "INSERT INTO merge_queue (repo_id, issue_id, base, method, added_by, head_sha, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) ON CONFLICT (issue_id) DO NOTHING",
                params![rid, iid, base_b, m, uid, h, now()],
            )?)
        })
        .await?;
    if added == 0 {
        return Err(AppError::conflict("This pull request is already in the queue"));
    }
    advance(&st, r.id).await;
    let place = st.db.call(move |c| Ok(position(c, iid)?)).await?;
    Ok(Json(json!({ "queue": place })))
}

/// Take a pull request out of the queue.
pub async fn dequeue(State(st): State<AppState>, Authed(u): Authed, Path((o, n, num)): Path<(String, String, i64)>) -> J<Value> {
    let r = Repo::writable(&st, Some(&u), &o, &n).await?;
    let rid = r.id;
    // Not in the middle of a step that may be merging it.
    let _one = LOCK.lock().await;
    let id: Option<i64> = st
        .db
        .call(move |c| {
            Ok(c.query_row(
                "DELETE FROM merge_queue WHERE issue_id = (SELECT id FROM issues WHERE repo_id = ?1 AND number = ?2) RETURNING id",
                params![rid, num],
                |x| x.get(0),
            )
            .optional()?)
        })
        .await?;
    let Some(id) = id else { return Err(AppError::not_found()) };
    let _ = st.git.delete_ref(&r.path, &format!("{REF_PREFIX}/{id}")).await;
    kick(&st, r.id);
    ok()
}

/// Every queue in the repository, in order.
pub async fn list(State(st): State<AppState>, Viewer(v): Viewer, Path((o, n)): Path<(String, String)>) -> J<Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let rid = r.id;
    let rows = st
        .db
        .call(move |c| {
            let mut q = c.prepare(
                "SELECT i.number, i.title, q.base, q.state, q.test_sha, u.username, q.created_at FROM merge_queue q
                 JOIN issues i ON i.id = q.issue_id JOIN users u ON u.id = q.added_by
                 WHERE q.repo_id = ?1 AND i.state = 'open' ORDER BY q.base, q.id",
            )?;
            let mut out: Vec<Value> = Vec::new();
            let mut last = (String::new(), 0);
            for row in q.query_map([rid], |x| {
                Ok((x.get::<_, i64>(0)?, x.get::<_, String>(1)?, x.get::<_, String>(2)?, x.get::<_, String>(3)?, x.get::<_, Option<String>>(4)?, x.get::<_, String>(5)?, x.get::<_, i64>(6)?))
            })? {
                let (number, title, base, state, test_sha, by, at) = row?;
                last = if last.0 == base { (base.clone(), last.1 + 1) } else { (base.clone(), 1) };
                out.push(json!({ "number": number, "title": title, "base": base, "position": last.1, "state": state, "test_sha": test_sha, "added_by": by, "added_at": at }));
            }
            Ok(out)
        })
        .await?;
    Ok(Json(json!({ "entries": rows })))
}

/// Where a pull request is in its queue (1 is being tested or next), if it's in one.
pub fn position(c: &Connection, issue_id: i64) -> rusqlite::Result<Option<Value>> {
    c.query_row(
        "SELECT q.state, q.test_sha, (SELECT COUNT(*) FROM merge_queue o JOIN issues oi ON oi.id = o.issue_id
                WHERE o.repo_id = q.repo_id AND o.base = q.base AND o.id <= q.id AND oi.state = 'open')
         FROM merge_queue q WHERE q.issue_id = ?1",
        [issue_id],
        |x| Ok(json!({ "state": x.get::<_, String>(0)?, "test_sha": x.get::<_, Option<String>>(1)?, "position": x.get::<_, i64>(2)? })),
    )
    .optional()
}

/// Move the repository's queues along in the background (for callers that may
/// be inside a queue's own merge, like the push hooks).
pub fn kick(st: &AppState, repo_id: i64) {
    let st = st.clone();
    tokio::spawn(async move { advance(&st, repo_id).await });
}

/// Move the repository's queues along: start testing, land what passed, take out
/// what failed, until nothing changes.
pub async fn advance(st: &AppState, repo_id: i64) {
    let any = st.db.call(move |c| Ok(c.query_row("SELECT EXISTS (SELECT 1 FROM merge_queue WHERE repo_id = ?1)", [repo_id], |x| x.get::<_, bool>(0))?)).await;
    if !matches!(any, Ok(true)) {
        return;
    }
    let _one = LOCK.lock().await;
    for _ in 0..100 {
        match step_all(st, repo_id).await {
            Ok(true) => {}
            Ok(false) => break,
            Err(e) => {
                eprintln!("merge queue in repository {repo_id}: {}", e.msg);
                break;
            }
        }
    }
}

/// Every minute: queues whose checks may have timed out, or that nothing else woke.
pub async fn tick(st: &AppState) -> R<()> {
    let repos: Vec<i64> = st
        .db
        .call(|c| Ok(c.prepare("SELECT DISTINCT repo_id FROM merge_queue")?.query_map([], |x| x.get(0))?.collect::<Result<_, _>>()?))
        .await?;
    for id in repos {
        advance(st, id).await;
    }
    Ok(())
}

async fn step_all(st: &AppState, repo_id: i64) -> R<bool> {
    let firsts: Vec<Entry> = st
        .db
        .call(move |c| {
            let mut q = c.prepare(
                "SELECT q.id, q.issue_id, i.number, i.title, i.state = 'open', p.head_branch, q.base, q.method, q.added_by, q.head_sha, q.state, q.test_sha, q.tested_base, q.started_at
                 FROM merge_queue q JOIN issues i ON i.id = q.issue_id JOIN pulls p ON p.issue_id = q.issue_id
                 WHERE q.repo_id = ?1 AND q.id = (SELECT MIN(o.id) FROM merge_queue o WHERE o.repo_id = q.repo_id AND o.base = q.base)",
            )?;
            let rows = q.query_map([repo_id], |x| {
                Ok(Entry {
                    id: x.get(0)?,
                    issue_id: x.get(1)?,
                    number: x.get(2)?,
                    title: x.get(3)?,
                    open: x.get(4)?,
                    head_branch: x.get(5)?,
                    base: x.get(6)?,
                    method: x.get(7)?,
                    added_by: x.get(8)?,
                    head_sha: x.get(9)?,
                    state: x.get(10)?,
                    test_sha: x.get(11)?,
                    tested_base: x.get(12)?,
                    started_at: x.get(13)?,
                })
            })?;
            Ok(rows.collect::<Result<_, _>>()?)
        })
        .await?;
    let mut moved = false;
    for e in firsts {
        moved |= match step(st, repo_id, &e).await {
            Ok(m) => m,
            // An error that would repeat on every pass mustn't hold up the line behind it.
            Err(err) => {
                eject(st, repo_id, &e, &format!("the merge queue hit an error: {}", err.msg)).await?;
                true
            }
        };
    }
    Ok(moved)
}

/// One step for the first pull request in a queue. True: something changed.
async fn step(st: &AppState, repo_id: i64, e: &Entry) -> R<bool> {
    let names: Option<(String, String)> = st
        .db
        .call(move |c| {
            Ok(c.query_row("SELECT u.username, r.name FROM repos r JOIN users u ON u.id = r.owner_id WHERE r.id = ?1", [repo_id], |x| Ok((x.get(0)?, x.get(1)?)))
                .optional()?)
        })
        .await?;
    let Some((owner, name)) = names else { return Ok(false) };
    if !e.open {
        remove(st, repo_id, e.id).await?;
        return Ok(true);
    }
    // It merges as whoever queued it: they must still be able to.
    let who = match crate::auth::user_by_id(&st.db, e.added_by).await? {
        Some(u) => Repo::writable(st, Some(&u), &owner, &name).await.ok().map(|r| (u, r)),
        None => None,
    };
    let Some((user, repo)) = who else {
        eject(st, repo_id, e, "whoever added it can no longer merge it").await?;
        return Ok(true);
    };
    let head = st.git.resolve(&repo.path, &format!("refs/heads/{}", e.head_branch)).await?;
    if head.as_deref() != Some(e.head_sha.as_str()) {
        eject(st, repo_id, e, "the source branch changed; add it again to merge the new commits").await?;
        return Ok(true);
    }
    let Some(base) = st.git.resolve(&repo.path, &format!("refs/heads/{}", e.base)).await? else {
        eject(st, repo_id, e, "the target branch is gone").await?;
        return Ok(true);
    };
    if e.state == "queued" || e.tested_base.as_deref() != Some(base.as_str()) {
        return start_test(st, &repo, &user, e).await;
    }
    let test = e.test_sha.clone().unwrap_or_default();
    match verdict(st, &repo, &e.base, &test).await? {
        Verdict::Wait if now() - e.started_at.unwrap_or(0) > TEST_TIMEOUT => {
            eject(st, repo_id, e, "its checks didn't finish within an hour").await?;
            Ok(true)
        }
        Verdict::Wait => Ok(false),
        Verdict::Fail(why) => {
            eject(st, repo_id, e, &why).await?;
            Ok(true)
        }
        Verdict::Pass => {
            // Reviews may have changed while it waited.
            let gate = protect::gate(st, &repo, e.issue_id, &e.base, &e.head_sha).await?;
            let blockers: Vec<&String> = gate.missing.iter().filter(|m| !gate.missing_checks.contains(m)).collect();
            if !blockers.is_empty() {
                let why = format!("merging is blocked: {}", blockers.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("; "));
                eject(st, repo_id, e, &why).await?;
                return Ok(true);
            }
            let plan = MergePlan { issue_id: e.issue_id, num: e.number, title: e.title.clone(), head_b: e.head_branch.clone(), base_b: e.base.clone(), head: e.head_sha.clone(), base, new_sha: test, from_fork: false };
            match land_merge(st, &repo, &user, plan, false).await {
                Ok(()) => remove(st, repo_id, e.id).await?,
                // The branch moved just now: the next pass tests on its new tip.
                Err(_) if st.git.resolve(&repo.path, &format!("refs/heads/{}", e.base)).await?.as_deref() != e.tested_base.as_deref() => {}
                Err(err) => eject(st, repo_id, e, &err.msg).await?,
            }
            Ok(true)
        }
    }
}

/// Merge the pull request onto the branch's tip and run its checks there.
async fn start_test(st: &AppState, repo: &Repo, user: &crate::auth::User, e: &Entry) -> R<bool> {
    let plan = match prepare_merge(st, repo, user, e.number, MergeReq::queued(&e.method, &e.head_sha), true).await {
        Ok(p) => p,
        Err(err) => {
            eject(st, repo.id, e, &err.msg).await?;
            return Ok(true);
        }
    };
    st.git.ok(&repo.path, &["update-ref", &format!("{REF_PREFIX}/{}", e.id), &plan.new_sha]).await?;
    let (id, sha, base) = (e.id, plan.new_sha.clone(), plan.base.clone());
    st.db
        .call(move |c| {
            c.execute("UPDATE merge_queue SET state = 'testing', test_sha = ?2, tested_base = ?3, started_at = ?4 WHERE id = ?1", params![id, sha, base, now()])?;
            Ok(())
        })
        .await?;
    let s = crate::ci::trigger::Queued { entry: e.id, number: e.number, head: &e.head_branch, base: &e.base, base_sha: &plan.base, sha: &plan.new_sha };
    // Without its runs the checks would look done: never land on that.
    if let Err(err) = crate::ci::trigger::on_queue(st, repo, s).await {
        eject(st, repo.id, e, &format!("its CI run couldn't start: {}", err.msg)).await?;
    }
    Ok(true)
}

enum Verdict {
    Pass,
    Wait,
    Fail(String),
}

/// The rule's required checks on the queue's commit, or every check there if it names none.
async fn verdict(st: &AppState, repo: &Repo, base: &str, sha: &str) -> R<Verdict> {
    let (rid, sha, base, url) = (repo.id, sha.to_string(), base.to_string(), format!("/{}/{}", repo.owner, repo.name));
    st.db
        .call(move |c| {
            let checks = protect::checks(c, rid, &sha, &url)?;
            let required = protect::effective(&protect::load(c, rid)?, &base).map(|r| r.checks).unwrap_or_default();
            let wanted: Vec<&protect::Check> = if required.is_empty() { checks.iter().collect() } else { checks.iter().filter(|x| required.contains(&x.name)).collect() };
            if let Some(f) = wanted.iter().find(|x| x.state == "failure") {
                return Ok(Verdict::Fail(format!("{} failed on the merged commit", f.name)));
            }
            let all_in = required.iter().all(|n| checks.iter().any(|x| &x.name == n));
            Ok(if all_in && wanted.iter().all(|x| x.state == "success") { Verdict::Pass } else { Verdict::Wait })
        })
        .await
}

async fn remove(st: &AppState, repo_id: i64, id: i64) -> R<()> {
    st.db.call(move |c| Ok(c.execute("DELETE FROM merge_queue WHERE id = ?1", [id])?)).await?;
    let _ = st.git.delete_ref(&st.git.repo_path(repo_id), &format!("{REF_PREFIX}/{id}")).await;
    Ok(())
}

/// Take a pull request out of the queue and say why on its timeline.
async fn eject(st: &AppState, repo_id: i64, e: &Entry, why: &str) -> R<()> {
    let (id, iid, uid, why) = (e.id, e.issue_id, e.added_by, why.to_string());
    st.db
        .call(move |c| {
            let tx = c.transaction()?;
            let t = now();
            tx.execute(
                "INSERT INTO comments (issue_id, author_id, kind, body, created_at, updated_at) VALUES (?1, ?2, 'dequeued', ?3, ?4, ?4)",
                params![iid, uid, why, t],
            )?;
            tx.execute("DELETE FROM merge_queue WHERE id = ?1", [id])?;
            tx.commit()?;
            Ok(())
        })
        .await?;
    let _ = st.git.delete_ref(&st.git.repo_path(repo_id), &format!("{REF_PREFIX}/{id}")).await;
    Ok(())
}
