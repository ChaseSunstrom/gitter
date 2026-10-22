//! Notifications: one inbox row per thread (issue, pull request, discussion, CI
//! run) per person, made or refreshed when something happens there. Who hears of
//! it: the thread's participants (its author, commenters, reviewers, assignees,
//! people mentioned in it), people subscribed to it by hand, and the repository's
//! watchers (its owner, unless they opted out), minus whoever did it, people who
//! unsubscribed from the thread (a new @mention still reaches them) and people
//! ignoring the repository. Only people who can read the repository right now
//! (`super::VISIBLE`, the same rule as `repo::access`) ever get one, and the
//! inbox applies the same rule again when it's read, so losing access hides them.
//! Handlers call `notify` (or `tell`, for an event meant for given people).

use std::collections::{HashMap, HashSet};

use axum::Json;
use axum::extract::{Path, Query, State};
use rusqlite::{OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{J, ok};
use crate::AppState;
use crate::auth::{Authed, User};
use crate::db::now;
use crate::error::{AppError, R};
use crate::repo::Repo;

#[derive(Clone, Copy, Debug)]
pub enum Thread {
    /// An issue or pull request, by its row id.
    Issue(i64),
    Discussion(i64),
    Run(i64),
}

impl Thread {
    fn key(self) -> String {
        match self {
            Thread::Issue(i) => format!("issue:{i}"),
            Thread::Discussion(i) => format!("discussion:{i}"),
            Thread::Run(i) => format!("run:{i}"),
        }
    }
    /// (issue_id, discussion_id, run_id)
    fn cols(self) -> (Option<i64>, Option<i64>, Option<i64>) {
        match self {
            Thread::Issue(i) => (Some(i), None, None),
            Thread::Discussion(i) => (None, Some(i), None),
            Thread::Run(i) => (None, None, Some(i)),
        }
    }
}

/// Why someone is notified, weakest first. A thread keeps its strongest reason
/// until it's marked done.
const REASONS: &[&str] = &["watching", "subscribed", "ci", "comment", "author", "assign", "mention"];
/// Reasons that aren't taking part yourself (the "participating" filter leaves them out).
const BYSTANDER: &str = "('watching', 'subscribed')";
/// @mentions read from one body, and notified per author per hour.
pub const MAX_MENTIONS: usize = 50;
const MENTIONS_PER_HOUR: u32 = 200;

fn rank(reason: &str) -> usize {
    REASONS.iter().position(|r| *r == reason).unwrap_or(0)
}

fn rank_sql(col: &str) -> String {
    let arms: String = REASONS.iter().enumerate().map(|(i, r)| format!(" WHEN '{r}' THEN {i}")).collect();
    format!("(CASE {col}{arms} ELSE 0 END)")
}

/// SQL that's true when user `u` (a column) may read repository `r` and, when the
/// thread is an issue `i` (LEFT JOINed, so possibly NULL), see it: moderator-hidden
/// issues are their author's and admins' only.
fn reader_sql(u: &str) -> String {
    format!(
        "{} AND (i.id IS NULL OR i.hidden = 0 OR i.author_id = {u} OR EXISTS (SELECT 1 FROM users ha WHERE ha.id = {u} AND ha.is_admin = 1))",
        super::VISIBLE.replace('?', u)
    )
}

/// The usernames @mentioned in markdown `text`, outside code blocks and code
/// spans, each once (case-insensitively), at most MAX_MENTIONS.
pub fn mentions(text: &str) -> Vec<String> {
    // Drop fenced and indented code blocks, keep the rest. An indented line is code
    // only where a code block can start or go on: not right after paragraph text
    // (it continues the paragraph), and not in a list (it's the item's content).
    // ponytail: no list-item content offsets, so code indented inside a list item counts as prose.
    let mut prose = String::new();
    let mut fence: Option<(char, usize)> = None;
    let (mut para, mut code, mut list, mut blank) = (false, false, false, true);
    for line in text.lines() {
        let t = line.trim_start();
        let c = t.chars().next().unwrap_or(' ');
        let run = t.chars().take_while(|x| *x == c).count();
        if let Some((fc, fl)) = fence {
            if c == fc && run >= fl && t.trim_end().chars().all(|x| x == fc) {
                fence = None;
            }
            continue;
        }
        if t.is_empty() {
            (para, blank) = (false, true);
            continue;
        }
        let was_blank = std::mem::replace(&mut blank, false);
        if (c == '`' || c == '~') && run >= 3 {
            fence = Some((c, run));
            (para, code) = (false, false);
            continue;
        }
        if line.starts_with("    ") || line.starts_with('\t') {
            code = code || (!para && !list);
            if code {
                continue;
            }
        } else {
            // A list item starts with "-", "*", "+", "1." or "1)" and a space (or nothing).
            let digits = t.bytes().take_while(u8::is_ascii_digit).count();
            let marker = if matches!(c, '-' | '*' | '+') {
                1
            } else if (1..10).contains(&digits) && matches!(t.as_bytes().get(digits), Some(b'.' | b')')) {
                digits + 1
            } else {
                0
            };
            let item = marker > 0 && matches!(t.as_bytes().get(marker), None | Some(b' ' | b'\t'));
            code = false;
            list = item || (list && !was_blank);
        }
        para = true;
        prose.push_str(line);
        prose.push('\n');
    }
    let b = prose.as_bytes();
    let (mut out, mut seen) = (Vec::new(), HashSet::new());
    let mut i = 0;
    while i < b.len() && out.len() < MAX_MENTIONS {
        if b[i] == b'`' {
            // A code span runs to the next backtick run of the same length.
            let n = b[i..].iter().take_while(|x| **x == b'`').count();
            let mut j = i + n;
            let mut end = None;
            while j < b.len() {
                if b[j] == b'`' {
                    let m = b[j..].iter().take_while(|x| **x == b'`').count();
                    if m == n {
                        end = Some(j + m);
                        break;
                    }
                    j += m;
                } else {
                    j += 1;
                }
            }
            i = end.unwrap_or(i + n);
            continue;
        }
        let word_before = i > 0 && (b[i - 1].is_ascii_alphanumeric() || matches!(b[i - 1], b'_' | b'-' | b'.' | b'@' | b'/' | b'`') || b[i - 1] >= 0x80);
        if b[i] == b'@' && !word_before {
            let start = i + 1;
            let mut j = start;
            while j < b.len() && (b[j].is_ascii_alphanumeric() || matches!(b[j], b'-' | b'_' | b'.')) {
                j += 1;
            }
            // Only ASCII bytes were passed, so these are char boundaries.
            let name = prose[start..j].trim_end_matches('.');
            if b.get(j) != Some(&b'/') && crate::validate::username(name) && seen.insert(name.to_ascii_lowercase()) {
                out.push(name.to_string());
            }
            i = j.max(start);
            continue;
        }
        i += 1;
    }
    out
}

fn ids(c: &rusqlite::Connection, sql: &str, p: impl rusqlite::Params) -> rusqlite::Result<Vec<i64>> {
    c.prepare_cached(sql)?.query_map(p, |r| r.get(0))?.collect()
}

/// Everyone with a standing stake in `thread`, with their strongest reason's rank.
fn stakeholders(c: &rusqlite::Connection, repo_id: i64, thread: Thread) -> rusqlite::Result<HashMap<i64, usize>> {
    let mut want: HashMap<i64, usize> = HashMap::new();
    let mut add = |who: Vec<i64>, reason: &str| {
        for u in who {
            let e = want.entry(u).or_insert(0);
            *e = (*e).max(rank(reason));
        }
    };
    add(
        ids(
            c,
            "SELECT user_id FROM repo_watches WHERE repo_id = ?1 AND level = 'watching'
             UNION SELECT r.owner_id FROM repos r WHERE r.id = ?1 AND NOT EXISTS (SELECT 1 FROM repo_watches w WHERE w.repo_id = r.id AND w.user_id = r.owner_id)",
            [repo_id],
        )?,
        "watching",
    );
    add(ids(c, "SELECT user_id FROM thread_subscriptions WHERE thread = ?1 AND subscribed = 1", [thread.key()])?, "subscribed");
    match thread {
        Thread::Issue(i) => {
            add(ids(c, "SELECT author_id FROM issues WHERE id = ?1", [i])?, "author");
            add(
                ids(
                    c,
                    "SELECT author_id FROM comments WHERE issue_id = ?1 AND kind = 'comment'
                     UNION SELECT author_id FROM reviews WHERE issue_id = ?1 AND state != 'pending'
                     UNION SELECT c.author_id FROM review_comments c LEFT JOIN reviews v ON v.id = c.review_id
                         WHERE c.issue_id = ?1 AND (c.review_id IS NULL OR v.state != 'pending')",
                    [i],
                )?,
                "comment",
            );
            add(ids(c, "SELECT user_id FROM issue_assignees WHERE issue_id = ?1", [i])?, "assign");
        }
        Thread::Discussion(d) => {
            add(ids(c, "SELECT author_id FROM discussions WHERE id = ?1", [d])?, "author");
            add(ids(c, "SELECT author_id FROM discussion_comments WHERE discussion_id = ?1", [d])?, "comment");
        }
        Thread::Run(_) => {}
    }
    Ok(want)
}

/// (unsubscribed from the thread, ignoring the repository)
fn opted_out(c: &rusqlite::Connection, repo_id: i64, thread: Thread) -> rusqlite::Result<(HashSet<i64>, HashSet<i64>)> {
    let unsub = ids(c, "SELECT user_id FROM thread_subscriptions WHERE thread = ?1 AND subscribed = 0", [thread.key()])?;
    let ignoring = ids(c, "SELECT user_id FROM repo_watches WHERE repo_id = ?1 AND level = 'ignoring'", [repo_id])?;
    Ok((unsub.into_iter().collect(), ignoring.into_iter().collect()))
}

/// Who should hear of something done on `thread`: everyone with a stake (`direct` empty),
/// or just the people in `direct` (an assignment, a failed run), plus the @mentioned
/// `names`. Writes their notifications; returns how many.
fn deliver(
    c: &mut rusqlite::Connection,
    repo_id: i64,
    thread: Thread,
    actor: Option<i64>,
    names: &[String],
    direct: &[i64],
    reason: &str,
) -> R<usize> {
    let tx = c.transaction()?;
    let mut want = if direct.is_empty() { stakeholders(&tx, repo_id, thread)? } else { HashMap::new() };
    // Asked for by this event: past an unsubscribe (not past ignoring the repository).
    let mut asked: HashSet<i64> = direct.iter().copied().collect();
    for u in direct {
        want.insert(*u, rank(reason));
    }
    let mut mentioned = HashSet::new();
    for n in names {
        if let Some(u) = tx.prepare_cached("SELECT id FROM users WHERE username = ?1")?.query_row([n], |r| r.get::<_, i64>(0)).optional()? {
            want.insert(u, rank("mention"));
            asked.insert(u);
            mentioned.insert(u);
        }
    }
    if let Some(a) = actor {
        want.remove(&a);
    }
    let (unsub, ignoring) = opted_out(&tx, repo_id, thread)?;
    want.retain(|u, _| !ignoring.contains(u) && (asked.contains(u) || !unsub.contains(u)));
    if want.is_empty() {
        return Ok(0);
    }
    // Only people who can read the repository (and the thread) now; never organizations.
    let candidates = serde_json::to_string(&want.keys().collect::<Vec<_>>()).map_err(AppError::internal)?;
    let (issue, discussion, run) = thread.cols();
    let readers = ids(
        &tx,
        &format!(
            "SELECT u.id FROM users u JOIN repos r ON r.id = ?1 LEFT JOIN issues i ON i.id = ?3
             WHERE u.id IN (SELECT value FROM json_each(?2)) AND u.suspended = 0 AND NOT EXISTS (SELECT 1 FROM orgs o WHERE o.id = u.id)
             AND {}",
            reader_sql("u.id")
        ),
        params![repo_id, candidates, issue.unwrap_or(-1)],
    )?;
    let key = thread.key();
    let t = now();
    // ponytail: one upsert per recipient inside the request, holding the connection; a repository
    // with thousands of watchers makes each comment that slow. Move to a background queue (like
    // webhook deliveries) if that ever shows up.
    for u in &readers {
        tx.prepare_cached(&format!(
            "INSERT INTO notifications(user_id, repo_id, thread, issue_id, discussion_id, run_id, reason, actor_id, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(user_id, thread) DO UPDATE SET unread = 1, done = 0, actor_id = excluded.actor_id, updated_at = excluded.updated_at,
                 reason = CASE WHEN done = 1 OR {} > {} THEN excluded.reason ELSE reason END",
            rank_sql("excluded.reason"),
            rank_sql("reason")
        ))?
        .execute(params![u, repo_id, key, issue, discussion, run, REASONS[want[u]], actor, t])?;
        // Being mentioned subscribes you to the thread (unless you said otherwise before).
        if mentioned.contains(u) && run.is_none() {
            tx.execute(
                "INSERT OR IGNORE INTO thread_subscriptions(user_id, thread, issue_id, discussion_id, subscribed) VALUES (?1, ?2, ?3, ?4, 1)",
                params![u, key, issue, discussion],
            )?;
        }
    }
    tx.commit()?;
    Ok(readers.len())
}

async fn fan_out(st: &AppState, repo_id: i64, thread: Thread, actor: Option<i64>, text: &str, direct: Vec<i64>, reason: &'static str) {
    let mut names = mentions(text);
    // Per author, so mentions can't be used to spam people.
    if let Some(a) = actor {
        names.retain(|_| st.limiter.allow(&format!("mention:{a}"), MENTIONS_PER_HOUR, 3600));
    }
    if let Err(e) = st.db.call(move |c| deliver(c, repo_id, thread, actor, &names, &direct, reason)).await {
        eprintln!("notifications: {}", e.msg);
    }
}

/// `actor` did something on `thread` (opened it, commented, closed, merged...);
/// `text` is what they wrote, for @mentions. Tells everyone with a stake in it.
/// An agent acts as its owner, so its owner isn't told of what it did.
pub async fn notify(st: &AppState, repo_id: i64, thread: Thread, actor: i64, text: &str) {
    fan_out(st, repo_id, thread, Some(actor), text, vec![], "comment").await;
}

/// Tell only `users` (an assignment; a run that failed), with `reason`.
pub async fn tell(st: &AppState, repo_id: i64, thread: Thread, actor: Option<i64>, users: Vec<i64>, reason: &'static str) {
    if !users.is_empty() {
        fan_out(st, repo_id, thread, actor, "", users, reason).await;
    }
}

/// A submitted review: its summary and its line comments may mention people.
pub async fn review(st: &AppState, repo_id: i64, issue_id: i64, review_id: i64, actor: i64, body: &str) {
    let summary = body.to_string();
    let text = st
        .db
        .call(move |c| Ok(review_mentions(c, issue_id, review_id, &summary)?))
        .await
        .unwrap_or_else(|_| body.to_string());
    notify(st, repo_id, Thread::Issue(issue_id), actor, &text).await;
}

/// The people a review's summary and line comments mention (up to MAX_MENTIONS), as
/// "@a @b". Each comment is read on its own, so one's unclosed fence can't hide the next's.
fn review_mentions(c: &rusqlite::Connection, issue_id: i64, review_id: i64, summary: &str) -> rusqlite::Result<String> {
    let mut names = mentions(summary);
    let mut seen: HashSet<String> = names.iter().map(|n| n.to_ascii_lowercase()).collect();
    let mut q = c.prepare("SELECT body FROM review_comments WHERE issue_id = ?1 AND review_id = ?2 AND instr(body, '@') > 0 ORDER BY id")?;
    let mut rows = q.query([issue_id, review_id])?;
    while names.len() < MAX_MENTIONS
        && let Some(r) = rows.next()?
    {
        names.extend(mentions(&r.get::<_, String>(0)?).into_iter().filter(|n| seen.insert(n.to_ascii_lowercase())));
    }
    names.truncate(MAX_MENTIONS);
    Ok(names.iter().map(|n| format!("@{n}")).collect::<Vec<_>>().join(" "))
}

/// A CI job finished: if that made its run fail, tell whoever started the run.
pub async fn ci_finished(st: &AppState, job: i64) {
    let run: Option<(i64, i64, Option<i64>)> = st
        .db
        .call(move |c| {
            Ok(c.query_row(
                "SELECT r.id, r.repo_id, r.actor_id FROM ci_jobs j JOIN ci_runs r ON r.id = j.run_id WHERE j.id = ?1 AND r.status = 'failure'",
                [job],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?)
        })
        .await
        .unwrap_or(None);
    if let Some((run, repo, Some(actor))) = run {
        tell(st, repo, Thread::Run(run), None, vec![actor], "ci").await;
    }
}

/// Forget done notifications 90 days after their thread's last news and read ones
/// after 180 (news makes a new one). Hourly; 10,000 ids at a time, so requests get
/// the connection in between. Returns how many went.
pub async fn sweep(st: &AppState) -> R<usize> {
    let (low, high): (i64, i64) = st
        .db
        .call(|c| Ok(c.query_row("SELECT coalesce(min(id), 1) - 1, coalesce(max(id), 0) FROM notifications", [], |r| Ok((r.get(0)?, r.get(1)?)))?))
        .await?;
    let (t, mut total) = (now(), 0);
    for from in (low..high).step_by(10_000) {
        total += st
            .db
            .call(move |c| {
                Ok(c.execute(
                    "DELETE FROM notifications WHERE id > ?1 AND id <= ?1 + 10000
                     AND unread = 0 AND updated_at < ?2 - (CASE WHEN done = 1 THEN 90 ELSE 180 END) * 86400",
                    [from, t],
                )?)
            })
            .await?;
    }
    Ok(total)
}

// ------------------------------------------------------------------ the inbox

/// The inbox spans every repository: not for deploy or agent tokens, which act in one.
fn person(u: &User) -> R<()> {
    if u.limits.as_ref().is_some_and(|l| l.repo_id.is_some() || l.agent.is_some()) {
        return Err(AppError::forbidden());
    }
    Ok(())
}

const FROM: &str = "FROM notifications n JOIN repos r ON r.id = n.repo_id JOIN users ru ON ru.id = r.owner_id
    LEFT JOIN issues i ON i.id = n.issue_id LEFT JOIN discussions d ON d.id = n.discussion_id LEFT JOIN ci_runs cr ON cr.id = n.run_id
    LEFT JOIN users au ON au.id = n.actor_id";

#[derive(Deserialize)]
pub struct ListQ {
    /// all (not done; the default), unread, participating, mentions, done
    filter: Option<String>,
    /// owner/name
    repo: Option<String>,
    page: Option<usize>,
}

fn split_repo(s: Option<&str>) -> (Option<String>, Option<String>) {
    match s.and_then(|s| s.split_once('/')) {
        Some((o, n)) => (Some(o.to_string()), Some(n.to_string())),
        None => (None, None),
    }
}

pub async fn list(State(st): State<AppState>, Authed(u): Authed, Query(q): Query<ListQ>) -> J<Value> {
    person(&u)?;
    let cond = match q.filter.as_deref().unwrap_or("all") {
        "all" | "" => "n.done = 0".to_string(),
        "unread" => "n.done = 0 AND n.unread = 1".to_string(),
        "participating" => format!("n.done = 0 AND n.reason NOT IN {BYSTANDER}"),
        "mentions" => "n.done = 0 AND n.reason = 'mention'".to_string(),
        "done" => "n.done = 1".to_string(),
        _ => return Err(AppError::bad("Unknown filter")),
    };
    if q.repo.as_deref().is_some_and(|r| !r.is_empty() && !r.contains('/')) {
        return Err(AppError::bad("Name a repository as owner/name"));
    }
    let (ro, rn) = split_repo(q.repo.as_deref());
    let (uid, page, per) = (u.id, super::page(q.page), 50);
    let (items, repos, unread) = st
        .db
        .call(move |c| {
            let readable = reader_sql("n.user_id");
            let sql = format!(
                "SELECT n.id, n.reason, n.unread, n.updated_at, ru.username, r.name, au.username,
                    i.number, i.title, i.is_pr, i.state, d.number, d.title, cr.number, cr.title, cr.status
                 {FROM} WHERE n.user_id = ?1 AND {cond} AND (?2 IS NULL OR (ru.username = ?2 AND r.name = ?3)) AND {readable}
                 ORDER BY n.updated_at DESC, n.id DESC LIMIT {} OFFSET {}",
                per + 1,
                (page - 1) * per
            );
            let items: Vec<Value> = c
                .prepare(&sql)?
                .query_map(params![uid, ro, rn], |r| {
                    let (owner, name): (String, String) = (r.get(4)?, r.get(5)?);
                    let base = format!("/{owner}/{name}");
                    let (kind, number, title, state, url) = if let Some(num) = r.get::<_, Option<i64>>(7)? {
                        let pr = r.get::<_, bool>(9)?;
                        let url = format!("{base}/{}/{num}", if pr { "pull" } else { "issues" });
                        (if pr { "pr" } else { "issue" }, num, r.get::<_, String>(8)?, r.get::<_, String>(10)?, url)
                    } else if let Some(num) = r.get::<_, Option<i64>>(11)? {
                        ("discussion", num, r.get::<_, String>(12)?, String::new(), format!("{base}/discussions/{num}"))
                    } else {
                        let num = r.get::<_, Option<i64>>(13)?.unwrap_or(0);
                        ("ci", num, r.get::<_, Option<String>>(14)?.unwrap_or_default(), r.get::<_, Option<String>>(15)?.unwrap_or_default(), format!("{base}/ci/{num}"))
                    };
                    Ok(json!({ "id": r.get::<_, i64>(0)?, "reason": r.get::<_, String>(1)?, "unread": r.get::<_, bool>(2)?,
                        "updated_at": r.get::<_, i64>(3)?, "repo": format!("{owner}/{name}"), "actor": r.get::<_, Option<String>>(6)?,
                        "kind": kind, "number": number, "title": title, "state": state, "url": url }))
                })?
                .collect::<Result<_, _>>()?;
            // The repository facet counts the newest 1000 open ones (as the badge does), so a
            // huge inbox doesn't visibility-check every row on each load.
            let mut s = c.prepare(&format!(
                "SELECT ru.username || '/' || r.name, count(*), sum(n.unread) {FROM}
                 WHERE n.id IN (SELECT id FROM notifications WHERE user_id = ?1 AND done = 0 ORDER BY updated_at DESC LIMIT 1000) AND {readable}
                 GROUP BY r.id ORDER BY 1 LIMIT 100"
            ))?;
            let repos: Vec<Value> = s
                .query_map([uid], |r| Ok(json!({ "repo": r.get::<_, String>(0)?, "count": r.get::<_, i64>(1)?, "unread": r.get::<_, i64>(2)? })))?
                .collect::<Result<_, _>>()?;
            Ok((items, repos, unread_count(c, uid)?))
        })
        .await?;
    let mut items = items;
    let more = items.len() > per;
    items.truncate(per);
    Ok(Json(json!({ "items": items, "has_more": more, "page": page, "repos": repos, "unread": unread })))
}

/// Unread, not done, still readable; counted up to 1000 (the badge says 99+ long before).
fn unread_count(c: &rusqlite::Connection, uid: i64) -> rusqlite::Result<i64> {
    c.prepare_cached(&format!(
        "SELECT count(*) FROM (SELECT 1 {FROM} WHERE n.user_id = ?1 AND n.done = 0 AND n.unread = 1 AND {} LIMIT 1000)",
        reader_sql("n.user_id")
    ))?
    .query_row([uid], |r| r.get(0))
}

pub async fn count(State(st): State<AppState>, Authed(u): Authed) -> J<Value> {
    person(&u)?;
    let uid = u.id;
    let n = st.db.call(move |c| Ok(unread_count(c, uid)?)).await?;
    Ok(Json(json!({ "unread": n })))
}

/// Mark one read, done (gone from the inbox until there's news), or unsubscribe
/// from its thread (and done).
async fn mark(st: &AppState, u: &User, id: i64, action: &'static str) -> J<Value> {
    person(u)?;
    let uid = u.id;
    let found = st
        .db
        .call(move |c| {
            let tx = c.transaction()?;
            let row: Option<(String, Option<i64>, Option<i64>)> = tx
                .query_row("SELECT thread, issue_id, discussion_id FROM notifications WHERE id = ?1 AND user_id = ?2", [id, uid], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?))
                })
                .optional()?;
            let Some((thread, issue, discussion)) = row else { return Ok(false) };
            let sql = if action == "read" { "UPDATE notifications SET unread = 0 WHERE id = ?1" } else { "UPDATE notifications SET unread = 0, done = 1 WHERE id = ?1" };
            tx.execute(sql, [id])?;
            if action == "unsubscribe" && (issue.is_some() || discussion.is_some()) {
                tx.execute(
                    "INSERT OR REPLACE INTO thread_subscriptions(user_id, thread, issue_id, discussion_id, subscribed) VALUES (?1, ?2, ?3, ?4, 0)",
                    params![uid, thread, issue, discussion],
                )?;
            }
            tx.commit()?;
            Ok(true)
        })
        .await?;
    if !found {
        return Err(AppError::not_found());
    }
    ok()
}

pub async fn read(State(st): State<AppState>, Authed(u): Authed, Path(id): Path<i64>) -> J<Value> {
    mark(&st, &u, id, "read").await
}

pub async fn done(State(st): State<AppState>, Authed(u): Authed, Path(id): Path<i64>) -> J<Value> {
    mark(&st, &u, id, "done").await
}

pub async fn unsubscribe(State(st): State<AppState>, Authed(u): Authed, Path(id): Path<i64>) -> J<Value> {
    mark(&st, &u, id, "unsubscribe").await
}

#[derive(Deserialize)]
pub struct ReadAllReq {
    /// owner/name: only that repository's.
    repo: Option<String>,
}

pub async fn read_all(State(st): State<AppState>, Authed(u): Authed, Json(req): Json<ReadAllReq>) -> J<Value> {
    person(&u)?;
    let named = req.repo.as_deref().filter(|s| !s.is_empty());
    let (ro, rn) = split_repo(named);
    // A repository named wrong must not mean "all of them".
    if named.is_some() && ro.is_none() {
        return Err(AppError::bad("repo is owner/name"));
    }
    let uid = u.id;
    st.db
        .call(move |c| {
            Ok(c.execute(
                "UPDATE notifications SET unread = 0 WHERE user_id = ?1 AND unread = 1
                 AND (?2 IS NULL OR repo_id IN (SELECT r.id FROM repos r JOIN users ru ON ru.id = r.owner_id WHERE ru.username = ?2 AND r.name = ?3))",
                params![uid, ro, rn],
            )?)
        })
        .await?;
    ok()
}

// ------------------------------------------------ watching and subscriptions

const LEVELS: &[&str] = &["participating", "watching", "ignoring"];

/// Your watch level for a repository: participating (threads you take part in),
/// watching (all of them) or ignoring (none, mentions included).
pub async fn watch(State(st): State<AppState>, Authed(u): Authed, Path((o, n)): Path<(String, String)>) -> J<Value> {
    let r = Repo::readable(&st, Some(&u), &o, &n).await?;
    let (rid, uid, owner) = (r.id, u.id, r.owner_id);
    let level: Option<String> = st
        .db
        .call(move |c| Ok(c.query_row("SELECT level FROM repo_watches WHERE user_id = ?1 AND repo_id = ?2", [uid, rid], |r| r.get(0)).optional()?))
        .await?;
    let level = level.unwrap_or_else(|| if uid == owner { "watching" } else { "participating" }.into());
    Ok(Json(json!({ "level": level })))
}

#[derive(Deserialize)]
pub struct WatchReq {
    level: String,
}

pub async fn set_watch(State(st): State<AppState>, Authed(u): Authed, Path((o, n)): Path<(String, String)>, Json(req): Json<WatchReq>) -> J<Value> {
    let r = Repo::readable(&st, Some(&u), &o, &n).await?;
    let Some(level) = LEVELS.iter().find(|l| **l == req.level) else {
        return Err(AppError::bad("Watch, participate or ignore"));
    };
    let (rid, uid) = (r.id, u.id);
    st.db
        .call(move |c| {
            Ok(c.execute(
                "INSERT INTO repo_watches(user_id, repo_id, level, created_at) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(user_id, repo_id) DO UPDATE SET level = excluded.level",
                params![uid, rid, level, now()],
            )?)
        })
        .await?;
    Ok(Json(json!({ "level": level })))
}

/// The thread `/issues/{n}` (DISCUSSION false) or `/discussions/{n}` names, as `u` may see it.
async fn thread_of<const DISCUSSION: bool>(st: &AppState, u: &User, o: &str, n: &str, num: i64) -> R<(Repo, Thread)> {
    let r = Repo::readable(st, Some(u), o, n).await?;
    let t = if !DISCUSSION {
        Thread::Issue(super::issues::load_issue(st, &r, num, Some(u)).await?.id)
    } else {
        let rid = r.id;
        let id: Option<i64> = st
            .db
            .call(move |c| Ok(c.query_row("SELECT id FROM discussions WHERE repo_id = ?1 AND number = ?2", [rid, num], |r| r.get(0)).optional()?))
            .await?;
        Thread::Discussion(id.ok_or_else(AppError::not_found)?)
    };
    Ok((r, t))
}

async fn subscription_state(st: &AppState, r: &Repo, t: Thread, uid: i64) -> R<Value> {
    let rid = r.id;
    st.db
        .call(move |c| {
            let explicit: Option<bool> = c
                .query_row("SELECT subscribed FROM thread_subscriptions WHERE user_id = ?1 AND thread = ?2", params![uid, t.key()], |r| r.get(0))
                .optional()?;
            let (_, ignoring) = opted_out(c, rid, t)?;
            let subscribed = !ignoring.contains(&uid) && explicit.map_or_else(|| stakeholders(c, rid, t).map(|s| s.contains_key(&uid)), Ok)?;
            Ok(json!({ "subscribed": subscribed, "ignoring": ignoring.contains(&uid) }))
        })
        .await
}

/// Whether you hear of a thread's news: subscribed by hand, or by taking part
/// in it or watching its repository (and not unsubscribed or ignoring it).
pub async fn subscription<const DISCUSSION: bool>(State(st): State<AppState>, Authed(u): Authed, Path((o, n, num)): Path<(String, String, i64)>) -> J<Value> {
    let (r, t) = thread_of::<DISCUSSION>(&st, &u, &o, &n, num).await?;
    Ok(Json(subscription_state(&st, &r, t, u.id).await?))
}

#[derive(Deserialize)]
pub struct SubscribeReq {
    subscribed: bool,
}

pub async fn set_subscription<const DISCUSSION: bool>(
    State(st): State<AppState>,
    Authed(u): Authed,
    Path((o, n, num)): Path<(String, String, i64)>,
    Json(req): Json<SubscribeReq>,
) -> J<Value> {
    let (r, t) = thread_of::<DISCUSSION>(&st, &u, &o, &n, num).await?;
    let (uid, (issue, discussion, _)) = (u.id, t.cols());
    st.db
        .call(move |c| {
            Ok(c.execute(
                "INSERT OR REPLACE INTO thread_subscriptions(user_id, thread, issue_id, discussion_id, subscribed) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![uid, t.key(), issue, discussion, req.subscribed],
            )?)
        })
        .await?;
    Ok(Json(subscription_state(&st, &r, t, u.id).await?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mentions_skip_code_emails_and_repeats() {
        let m = |s: &str| mentions(s);
        assert_eq!(m("hi @alice and @Bob, @alice again"), ["alice", "Bob"]);
        assert_eq!(m("@ALICE @alice"), ["ALICE"], "case-insensitive repeats");
        assert_eq!(m("mail bob@example.com or @carol."), ["carol"]);
        assert_eq!(m("`@a` ``x ` @b `` @c"), ["c"]);
        assert_eq!(m("```\n@a\n```\n~~~~\n@b\n~~~\n@c\n~~~~\n@d"), ["d"]);
        assert_eq!(m("text\n\n    @indented\n\t@tab\n@e"), ["e"]);
        assert_eq!(m("    @first\n\n    @blank-inside"), Vec::<String>::new(), "code from the start, past a blank line");
        assert_eq!(m("text\n    @lazy"), ["lazy"], "an indented line can't interrupt a paragraph");
        assert_eq!(m("- a\n    - cc @bob\n1. b\n\n    @carol\n\t@tab"), ["bob", "carol", "tab"], "list items' nested lists and paragraphs");
        assert_eq!(m("- a\n\nout of the list\n\n    @code"), Vec::<String>::new(), "a paragraph after a blank line ends the list");
        assert_eq!(m("```\nx\n```\n    @after-fence"), Vec::<String>::new());
        assert_eq!(m("1 apple\n\n    @code\n2) b\n    @item"), ["item"], "a number alone isn't a list item");
        assert_eq!(m("@org/team @admin @x..y @-no"), Vec::<String>::new(), "teams, reserved and invalid names");
        assert_eq!(m("unclosed ` @f"), ["f"]);
        assert_eq!(m("é@g @h"), ["h"]);
        let many: String = (0..80).map(|i| format!("@user{i} ")).collect();
        assert_eq!(m(&many).len(), MAX_MENTIONS);
    }

    #[test]
    fn reasons_rank_mentions_highest() {
        assert!(rank("mention") > rank("assign") && rank("assign") > rank("author") && rank("author") > rank("comment"));
        assert!(rank("comment") > rank("subscribed") && rank("subscribed") > rank("watching"));
        assert!(rank_sql("x").contains("WHEN 'mention' THEN 6"));
    }
}
