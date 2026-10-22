//! Bulk import from GitHub (or GitHub Enterprise) and GitLab (or a self-managed
//! one): repositories with their labels, milestones, issues, pull or merge
//! requests, comments and releases.
//!
//! - A job holds the person's access token, sealed, until it ends (done, failed
//!   or cancelled), when it's wiped; the hourly sweep ends jobs idle for a day.
//! - Each repository is an item that moves through the stages one page at a
//!   time; the cursor is the next page's URL, so a restart picks up where it
//!   left off, and import_map makes a page done twice add nothing twice.
//! - Every API URL is on the job's own host and passes net.rs's checks (public
//!   addresses only unless a site admin allowed private ones, no redirects);
//!   the clone goes through mirrors' pinned git with the token in the environment.
//! - A rate limit reschedules the item for when it resets, without holding the worker.
//! - Remote text is kept as written with its author named in inline code, so
//!   nobody here is mentioned or notified; GitHub numbers are kept, and GitLab
//!   merge requests are numbered after the highest issue.

use axum::Json;
use axum::extract::{Path, State};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::api::{ClientIp, J};
use crate::auth::{Authed, User};
use crate::db::now;
use crate::error::{AppError, R};
use crate::mirrors::Remote;
use crate::security::{Event, open_text, record, seal_text};
use crate::{AppState, net, validate};

const STAGES: [&str; 9] = ["create", "clone", "labels", "milestones", "issues", "pulls", "comments", "releases", "done"];
const MAX_REPOS: usize = 200;
const LEASE: i64 = 900;
/// The most of one API answer that's read (100 items with long bodies).
const MAX_JSON: u64 = 8 << 20;
/// A job nobody has moved for this long ends (and its token goes).
const IDLE_SECS: i64 = 86400;
/// Pages one stage of one repository may take (100 items each).
const MAX_PAGES: i64 = 2000;
/// Server errors and timeouts are tried this many times, waiting longer each time.
const MAX_TRIES: i64 = 5;
/// Finished jobs are kept this long for the page to show.
const KEEP_SECS: i64 = 30 * 86400;
/// The longest a job may run, however it's going (a provider always rate limiting, say).
const MAX_AGE_SECS: i64 = 7 * 86400;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    GitHub,
    GitLab,
}

impl Kind {
    fn parse(s: &str) -> Option<Kind> {
        match s {
            "github" => Some(Kind::GitHub),
            "gitlab" => Some(Kind::GitLab),
            _ => None,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Kind::GitHub => "GitHub",
            Kind::GitLab => "GitLab",
        }
    }
}

/// Where a job reads from, and as whom.
struct Source {
    kind: Kind,
    /// The site people browse (clone URLs are built from it).
    web: String,
    /// The API root every request must be under.
    api: String,
    token: String,
    allow_private: bool,
}

impl Source {
    fn new(kind: Kind, web: &str, token: String, allow_private: bool) -> Source {
        let web = web.trim_end_matches('/').to_string();
        let api = match kind {
            Kind::GitHub if web.eq_ignore_ascii_case("https://github.com") => "https://api.github.com".to_string(),
            Kind::GitHub => format!("{web}/api/v3"),
            Kind::GitLab => format!("{web}/api/v4"),
        };
        Source { kind, web, api, token, allow_private }
    }
    fn clone_url(&self, path: &str) -> String {
        format!("{}/{path}.git", self.web)
    }
    fn remote(&self, path: &str) -> Remote {
        let username = if self.kind == Kind::GitHub { "x-access-token" } else { "oauth2" };
        Remote { url: self.clone_url(path), username: username.into(), password: self.token.clone(), allow_private: self.allow_private }
    }
    /// Text from the provider with the token blanked out, short.
    fn redact(&self, s: &str) -> String {
        let s = if self.token.is_empty() { s.to_string() } else { s.replace(&self.token, "•••") };
        s.chars().filter(|c| !c.is_control()).take(300).collect()
    }
}

/// scheme://host:port of an http(s) URL.
fn origin(url: &str) -> Option<String> {
    let u = net::check_url(url).ok()?;
    Some(format!("{}://{}", u.scheme_str()?, u.authority()?.as_str().to_ascii_lowercase()))
}

/// http(s) with a host, and https unless private addresses are allowed.
fn check_scheme(url: &str, allow_private: bool) -> Result<(), String> {
    let u = net::check_url(url)?;
    if u.scheme_str() != Some("https") && !allow_private {
        return Err("Use an https:// address (plain http only with private addresses allowed)".into());
    }
    Ok(())
}

/// The address someone typed for the provider.
fn check_base(url: &str, allow_private: bool) -> Result<(), String> {
    check_scheme(url, allow_private)?;
    if url.contains(['?', '#']) {
        return Err("Leave the query and fragment out of the address".into());
    }
    Ok(())
}

struct Page {
    body: Value,
    next: Option<String>,
}

enum Got {
    Page(Page),
    /// Rate limited until this time.
    Wait(i64),
    /// A server error or no answer: worth trying again later.
    Retry(String),
}

/// The `rel="next"` URL of a Link header.
fn next_link(h: &str) -> Option<String> {
    h.split(',').find(|p| p.contains("rel=\"next\"")).and_then(|p| Some(p.split(';').next()?.trim().strip_prefix('<')?.strip_suffix('>')?.to_string()))
}

async fn get(src: &Source, url: &str) -> Result<Got, String> {
    if origin(url).is_none() || origin(url) != origin(&src.api) {
        return Err("a link pointed at another host, so the import stopped there".into());
    }
    check_scheme(url, src.allow_private)?;
    let (u, token, ap) = (url.to_string(), src.token.clone(), src.allow_private);
    let r = tokio::task::spawn_blocking(move || {
        let h = [("Accept", "application/json".to_string()), ("Authorization", format!("Bearer {token}"))];
        net::fetch(&u, &h, None, ap, MAX_JSON)
    })
    .await
    .map_err(|e| e.to_string())?;
    let r = match r {
        Ok(r) => r,
        // Refused addresses won't change; anything else (no answer, a timeout) may.
        Err(e) if e.starts_with("Not contacted") || e.starts_with("Use an") => return Err(src.redact(&e)),
        Err(e) => return Ok(Got::Retry(src.redact(&e))),
    };
    let limited = r.status == 429 || (r.status == 403 && (r.header("x-ratelimit-remaining") == Some("0") || r.header("retry-after").is_some()));
    if limited {
        let t = now();
        let reset = r.header("x-ratelimit-reset").or(r.header("ratelimit-reset")).and_then(|v| v.parse::<i64>().ok());
        let until = r.header("retry-after").and_then(|v| v.parse::<i64>().ok()).map(|s| t + s).or(reset).unwrap_or(t + 60);
        return Ok(Got::Wait(until.clamp(t + 5, t + 3600)));
    }
    let body: Option<Value> = serde_json::from_slice(&r.body).ok();
    if r.status >= 500 {
        return Ok(Got::Retry(format!("{} answered {}", src.kind.name(), r.status)));
    }
    if !(200..300).contains(&r.status) {
        let why = body.as_ref().and_then(|b| b["message"].as_str().or(b["error"].as_str())).map(|m| format!(": {}", src.redact(m))).unwrap_or_default();
        return Err(format!("{} answered {}{why}", src.kind.name(), r.status));
    }
    // Cut short (a slow answer past the time limit) or not JSON: maybe better next time.
    let Some(body) = body else { return Ok(Got::Retry(format!("{} sent an answer that isn't JSON or was cut short", src.kind.name()))) };
    Ok(Got::Page(Page { body, next: r.header("link").and_then(next_link) }))
}

/// GET that waits out nothing: for the job form, where a person is waiting.
async fn get_now(src: &Source, url: &str) -> Result<Page, String> {
    match get(src, url).await? {
        Got::Page(p) => Ok(p),
        Got::Wait(t) => Err(format!("{} says to wait {} seconds (rate limit); try again then", src.kind.name(), t - now())),
        Got::Retry(e) => Err(e),
    }
}

/// A repository the source has.
struct Listed {
    path: String,
    name: String,
    private: bool,
    description: String,
    remote_id: String,
}

/// The source's repositories (at most MAX_REPOS, and whether there were more): a user's own
/// (with private ones when it's the token's owner), an organization's or a GitLab group's.
async fn list(src: &Source, source: &str) -> Result<(Vec<Listed>, bool), String> {
    let api = &src.api;
    let mut url = match src.kind {
        Kind::GitHub => {
            let me = get_now(src, &format!("{api}/user")).await?.body;
            if me["login"].as_str().is_some_and(|l| l.eq_ignore_ascii_case(source)) {
                format!("{api}/user/repos?affiliation=owner&per_page=100")
            } else {
                let org = format!("{api}/orgs/{}/repos?type=all&per_page=100", net::enc(source));
                match get_now(src, &org).await {
                    Err(e) if e.contains(" 404") => format!("{api}/users/{}/repos?per_page=100", net::enc(source)),
                    _ => org,
                }
            }
        }
        Kind::GitLab => {
            let group = format!("{api}/groups/{}/projects?include_subgroups=true&per_page=100", net::enc(source));
            match get_now(src, &group).await {
                Err(e) if e.contains(" 404") => format!("{api}/users/{}/projects?per_page=100", net::enc(source)),
                _ => group,
            }
        }
    };
    let mut out = vec![];
    for _ in 0..20 {
        let p = get_now(src, &url).await?;
        for r in p.body.as_array().ok_or("the repository list isn't a list")? {
            let l = match src.kind {
                Kind::GitHub => Listed {
                    path: text(&r["full_name"]),
                    name: text(&r["name"]),
                    private: r["private"] == true,
                    description: text(&r["description"]),
                    remote_id: String::new(),
                },
                Kind::GitLab => Listed {
                    path: text(&r["path_with_namespace"]),
                    name: text(&r["path"]),
                    private: r["visibility"] != "public",
                    description: text(&r["description"]),
                    remote_id: r["id"].as_i64().map(|n| n.to_string()).unwrap_or_default(),
                },
            };
            if !l.path.is_empty() && out.len() < MAX_REPOS {
                out.push(l);
            }
        }
        match p.next {
            Some(n) if n == url => return Err("its pages lead back to the same page".into()),
            Some(_) if out.len() >= MAX_REPOS => return Ok((out, true)),
            Some(n) => url = n,
            None => return Ok((out, false)),
        }
    }
    Err("its list of repositories goes on for too many pages".into())
}

// ---------------------------------------------------------------- text and time

fn text(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}

fn short(s: &str, max: usize) -> String {
    s.chars().filter(|c| !c.is_control() || *c == '\n' || *c == '\t').take(max).collect()
}

/// Seconds since 1970 for an ISO 8601 time like 2020-01-02T03:04:05.000+02:00.
fn epoch(s: &str) -> Option<i64> {
    let n = |a: usize, b: usize| s.get(a..b)?.parse::<i64>().ok();
    let (y, mo, d, h, mi, se) = (n(0, 4)?, n(5, 7)?, n(8, 10)?, n(11, 13)?, n(14, 16)?, n(17, 19)?);
    let mut rest = s.get(19..)?;
    if let Some(r) = rest.strip_prefix('.') {
        rest = r.trim_start_matches(|c: char| c.is_ascii_digit());
    }
    let off = match rest {
        "" | "Z" => 0,
        r if r.len() == 6 && (r.starts_with('+') || r.starts_with('-')) => {
            let v = r.get(1..3)?.parse::<i64>().ok()? * 3600 + r.get(4..6)?.parse::<i64>().ok()? * 60;
            if r.starts_with('-') { -v } else { v }
        }
        _ => return None,
    };
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) {
        return None;
    }
    // Days from the civil date (H. Hinnant's algorithm).
    let y2 = if mo <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let doy = (153 * ((mo + 9) % 12) + 2) / 5 + d - 1;
    let days = era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468;
    Some(days * 86400 + h * 3600 + mi * 60 + se - off)
}

/// A remote time, never in the future.
fn when(v: &Value) -> Option<i64> {
    v.as_str().and_then(epoch).map(|t| t.min(now()))
}

fn day(t: i64) -> String {
    let (y, m, d, ..) = crate::ci::config::civil(t);
    format!("{y:04}-{m:02}-{d:02}")
}

/// "*Imported from GitHub: by `octo` on 2020-01-02.*" and the text as written.
fn attributed(kind: Kind, login: &str, at: i64, note: &str, body: &str) -> String {
    let who: String = login.chars().filter(|c| c.is_ascii_alphanumeric() || "-_.".contains(*c)).take(64).collect();
    let who = if who.is_empty() { "someone".to_string() } else { format!("`{who}`") };
    let head = format!("*Imported from {}{note}: by {who} on {}.*", kind.name(), day(at));
    if body.trim().is_empty() { head } else { format!("{head}\n\n{}", short(body, 65_000)) }
}

// ---------------------------------------------------------------- the worker

struct Item {
    id: i64,
    job_id: i64,
    user_id: i64,
    path: String,
    remote_id: String,
    name: String,
    private: bool,
    description: String,
    owner_id: i64,
    repo_id: Option<i64>,
    stage: String,
    cursor: String,
    offset: i64,
    pages: i64,
    tries: i64,
    kind: Kind,
    base_url: String,
    token: Option<String>,
    allow_private: bool,
}

enum Flow {
    /// More pages in this stage, from this URL (or cursor).
    Stay(String),
    /// On in this stage to a new list (GitLab's next issue), whose pages count afresh.
    Moved(String),
    Next,
    /// Straight on to this stage.
    Jump(&'static str),
    Wait(i64),
    Retry(String),
}

/// Take the next due item, leased. Its job must be running.
fn claim(c: &mut rusqlite::Connection) -> rusqlite::Result<Option<Item>> {
    let tx = c.transaction()?;
    let t = now();
    let item = tx
        .query_row(
            "SELECT i.id, i.job_id, j.user_id, i.source, i.remote_id, i.name, i.private, i.description, j.owner_id, i.repo_id, i.stage, i.cursor,
                    i.number_offset, j.kind, j.base_url, j.token, j.allow_private, i.pages, i.tries
             FROM import_items i JOIN import_jobs j ON j.id = i.job_id
             WHERE j.state = 'running' AND i.stage NOT IN ('done', 'failed') AND i.next_at <= ?1 AND i.running_until < ?1
             ORDER BY j.updated_at, i.next_at, i.id LIMIT 1",
            [t],
            |r| {
                Ok(Item {
                    id: r.get(0)?,
                    job_id: r.get(1)?,
                    user_id: r.get(2)?,
                    path: r.get(3)?,
                    remote_id: r.get(4)?,
                    name: r.get(5)?,
                    private: r.get(6)?,
                    description: r.get(7)?,
                    owner_id: r.get(8)?,
                    repo_id: r.get(9)?,
                    stage: r.get(10)?,
                    cursor: r.get(11)?,
                    offset: r.get(12)?,
                    kind: Kind::parse(&r.get::<_, String>(13)?).unwrap_or(Kind::GitHub),
                    base_url: r.get(14)?,
                    token: r.get(15)?,
                    allow_private: r.get(16)?,
                    pages: r.get(17)?,
                    tries: r.get(18)?,
                })
            },
        )
        .optional()?;
    if let Some(i) = &item {
        tx.execute("UPDATE import_items SET running_until = ?2 WHERE id = ?1", params![i.id, t + LEASE])?;
    }
    tx.commit()?;
    Ok(item)
}

/// Do one step of one due item (one page, or a clone). False when nothing was due.
pub async fn step(st: &AppState) -> bool {
    let item = match st.db.call(|c| Ok(claim(c)?)).await {
        Ok(Some(i)) => i,
        Ok(None) => return false,
        Err(e) => {
            eprintln!("imports: {}", e.msg);
            return false;
        }
    };
    let Some(token) = item.token.as_deref().and_then(|t| open_text(&st.key, t)) else {
        save(st, &item, Err("The access token can't be read (was the server key replaced?). Start the import again.".into())).await;
        return true;
    };
    let src = Source::new(item.kind, &item.base_url, token, item.allow_private);
    let result = advance(st, &item, &src).await.map_err(|e| src.redact(&e));
    save(st, &item, result).await;
    true
}

async fn save(st: &AppState, item: &Item, result: Result<Flow, String>) {
    let (id, job, stage, tries) = (item.id, item.job_id, item.stage.clone(), item.tries);
    let r = st
        .db
        .call(move |c| {
            let t = now();
            let next_stage = |to: &str| {
                // Due again now: with next_at as the order, every due repository gets its turn.
                c.execute(
                    "UPDATE import_items SET stage = ?2, cursor = '', pages = 0, tries = 0, next_at = ?3, running_until = 0,
                     error = CASE WHEN ?2 = 'done' AND skipped > 0 THEN skipped || ' skipped: their numbers are already used here' ELSE '' END WHERE id = ?1",
                    params![id, to, t],
                )
            };
            match result {
                Ok(Flow::Stay(cursor)) => c.execute(
                    "UPDATE import_items SET cursor = ?2, pages = pages + 1, tries = 0, next_at = ?3, running_until = 0, error = '' WHERE id = ?1",
                    params![id, cursor, t],
                )?,
                Ok(Flow::Moved(cursor)) => c.execute(
                    "UPDATE import_items SET cursor = ?2, pages = 0, tries = 0, next_at = ?3, running_until = 0, error = '' WHERE id = ?1",
                    params![id, cursor, t],
                )?,
                Ok(Flow::Next) => next_stage(STAGES.iter().position(|s| *s == stage).map(|i| STAGES[(i + 1).min(STAGES.len() - 1)]).unwrap_or("done"))?,
                Ok(Flow::Jump(to)) => next_stage(to)?,
                Ok(Flow::Wait(until)) => c.execute("UPDATE import_items SET next_at = ?2, running_until = 0 WHERE id = ?1", params![id, until])?,
                Ok(Flow::Retry(e)) if tries + 1 < MAX_TRIES => c.execute(
                    "UPDATE import_items SET tries = tries + 1, next_at = ?2, running_until = 0, error = ?3 WHERE id = ?1",
                    params![id, t + (30 << tries), format!("Trying again soon: {e}")],
                )?,
                Ok(Flow::Retry(e)) | Err(e) => c.execute("UPDATE import_items SET stage = 'failed', error = ?2, running_until = 0 WHERE id = ?1", params![id, e])?,
            };
            c.execute("UPDATE import_jobs SET updated_at = ?2 WHERE id = ?1", params![job, t])?;
            // The last item settled: the job ends, and its token goes.
            let open: i64 = c.query_row("SELECT count(*) FROM import_items WHERE job_id = ?1 AND stage NOT IN ('done', 'failed')", [job], |r| r.get(0))?;
            if open == 0 {
                c.execute(
                    "UPDATE import_jobs SET token = NULL, state = CASE WHEN EXISTS (SELECT 1 FROM import_items WHERE job_id = ?1 AND stage = 'done') THEN 'done' ELSE 'failed' END
                     WHERE id = ?1 AND state = 'running'",
                    [job],
                )?;
            }
            Ok(())
        })
        .await;
    if let Err(e) = r {
        eprintln!("imports: item {id}: {}", e.msg);
    }
}

/// Release what a step left leased when the server stopped (one worker runs, in this process).
pub async fn recover(st: &AppState) {
    if let Err(e) = st.db.call(|c| Ok(c.execute("UPDATE import_items SET running_until = 0", [])?)).await {
        eprintln!("imports: {}", e.msg);
    }
}

/// Every second or so, the next due step; the hourly sweep is in main.
// ponytail: one worker for every import, a step (a page or a clone) at a time, jobs taking turns;
// a slow clone (up to 30 minutes) holds everyone's meanwhile. A small pool of workers if that bites.
pub async fn run(st: AppState) {
    recover(&st).await;
    loop {
        while step(&st).await {}
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}

/// End jobs nobody has moved for a day, wiping their tokens; forget old finished ones.
pub async fn sweep(st: &AppState) {
    let t = now();
    let r = st
        .db
        .call(move |c| {
            c.execute(
                "UPDATE import_jobs SET state = 'failed', token = NULL, error = 'Stopped after a day without progress', updated_at = ?2
                 WHERE state = 'running' AND updated_at < ?1",
                [t - IDLE_SECS, t],
            )?;
            c.execute(
                "UPDATE import_jobs SET state = 'failed', token = NULL, error = 'Stopped after a week', updated_at = ?2
                 WHERE state = 'running' AND created_at < ?1",
                [t - MAX_AGE_SECS, t],
            )?;
            Ok(c.execute("DELETE FROM import_jobs WHERE state != 'running' AND updated_at < ?1", [t - KEEP_SECS])?)
        })
        .await;
    if let Err(e) = r {
        eprintln!("imports: sweep: {}", e.msg);
    }
}

fn first_url(item: &Item, src: &Source) -> String {
    let api = &src.api;
    let gh = |p: &str| format!("{api}/repos/{}/{p}", item.path);
    let gl = |p: &str| format!("{api}/projects/{}/{p}", item.remote_id);
    match (src.kind, item.stage.as_str()) {
        (Kind::GitHub, "labels") => gh("labels?per_page=100"),
        (Kind::GitHub, "milestones") => gh("milestones?state=all&per_page=100"),
        (Kind::GitHub, "issues") => gh("issues?state=all&per_page=100&sort=created&direction=asc"),
        (Kind::GitHub, "pulls") => gh("pulls?state=all&per_page=100&sort=created&direction=asc"),
        (Kind::GitHub, "comments") => gh("issues/comments?per_page=100&sort=created&direction=asc"),
        (Kind::GitHub, _) => gh("releases?per_page=100"),
        (Kind::GitLab, "labels") => gl("labels?per_page=100"),
        (Kind::GitLab, "milestones") => gl("milestones?per_page=100"),
        (Kind::GitLab, "issues") => gl("issues?scope=all&per_page=100&order_by=created_at&sort=asc"),
        (Kind::GitLab, "pulls") => gl("merge_requests?scope=all&state=all&per_page=100&order_by=created_at&sort=asc"),
        (Kind::GitLab, _) => gl("releases?per_page=100"),
    }
}

async fn advance(st: &AppState, item: &Item, src: &Source) -> Result<Flow, String> {
    match item.stage.as_str() {
        "create" => create(st, item, src).await,
        "clone" => {
            let repo = item.repo_id.ok_or("the repository is gone")?;
            let extra: &[&str] = match src.kind {
                Kind::GitHub => &["+refs/pull/*/head:refs/pull/*/head"],
                Kind::GitLab => &["+refs/merge-requests/*/head:refs/merge-requests/*/head"],
            };
            let remote = src.remote(&item.path);
            match crate::mirrors::pull(st, repo, &remote, extra).await {
                Ok(()) => Ok(Flow::Next),
                // A dropped connection or a stall may go next time; mirrors' errors don't say which.
                Err(e) => Ok(Flow::Retry(crate::mirrors::clean(&e, &remote))),
            }
        }
        "comments" if src.kind == Kind::GitLab => gitlab_notes(st, item, src).await,
        stage => {
            let url = if item.cursor.is_empty() { first_url(item, src) } else { item.cursor.clone() };
            let p = match get(src, &url).await? {
                Got::Page(p) => p,
                Got::Wait(t) => return Ok(Flow::Wait(t)),
                Got::Retry(e) => return Ok(Flow::Retry(e)),
            };
            let rows = p.body.as_array().cloned().ok_or("the answer isn't a list")?;
            write(st, item, src, stage, rows).await?;
            match p.next {
                Some(n) if n == url => Err("its pages lead back to the same page".into()),
                Some(_) if item.pages + 1 >= MAX_PAGES => Err(format!("its {stage} go on past {MAX_PAGES} pages")),
                Some(n) => Ok(Flow::Stay(n)),
                None => Ok(Flow::Next),
            }
        }
    }
}

/// The repository to import into: made empty, or the one an earlier import of the same source made.
async fn create(st: &AppState, item: &Item, src: &Source) -> Result<Flow, String> {
    if item.repo_id.is_some() {
        return Ok(Flow::Next);
    }
    let mut fresh = true;
    let key = format!("{}/{}", src.web, item.path);
    let (owner_id, name, id) = (item.owner_id, item.name.clone(), item.id);
    let found = st
        .db
        .call(move |c| {
            let mine: Option<i64> = c
                .query_row(
                    "SELECT r.id FROM repos r JOIN import_map m ON m.repo_id = r.id AND m.kind = 'repo' AND m.remote_id = ?3
                     WHERE r.owner_id = ?1 AND r.name = ?2 AND r.deleted_at IS NULL",
                    params![owner_id, name, key],
                    |r| r.get(0),
                )
                .optional()?;
            let taken = c.query_row("SELECT 1 FROM repos WHERE owner_id = ?1 AND name = ?2", params![owner_id, name], |_| Ok(())).optional()?.is_some();
            Ok((mine, taken))
        })
        .await
        .map_err(|e| e.msg)?;
    let repo = match found {
        (Some(r), _) => {
            // Doing it again fetches over the repository, so only for someone who may administer it
            // (in an organization, not every member who may create repositories).
            let (u, owner) = load_users(st, item.user_id, item.owner_id).await?;
            let here = crate::repo::Repo::readable(st, Some(&u), &owner.username, &item.name).await.ok();
            if !here.is_some_and(|h| h.id == r && h.can_admin) {
                return Err(format!("{}/{} came from an earlier import, and only its admins can import into it again", owner.username, item.name));
            }
            // Skip the copy only when there's code (the first one may never have landed).
            fresh = st.git.refs(&st.git.repo_path(r)).await.map(|refs| refs.is_empty()).unwrap_or(true);
            r
        }
        (None, true) => return Err(format!("A repository named {} is already there", item.name)),
        (None, false) => {
            if !validate::repo_name(&item.name) {
                return Err(format!("{} isn't a repository name this server takes", item.name));
            }
            let (u, owner) = load_users(st, item.user_id, item.owner_id).await?;
            // Still allowed to create there (an organization's roles may have changed since the start).
            let owner = crate::api::repos::owner_for(st, &u, Some(&owner.username)).await.map_err(|e| e.msg)?;
            let description: String = item.description.chars().take(350).collect();
            let r = crate::api::repos::create_empty(st, &u, &owner, &item.name, &description, item.private).await.map_err(|e| e.msg)?;
            let key = format!("{}/{}", src.web, item.path);
            st.db.call(move |c| Ok(c.execute("INSERT OR IGNORE INTO import_map(repo_id, kind, remote_id, local_id) VALUES (?1, 'repo', ?2, ?1)", params![r, key])?)).await.map_err(|e| e.msg)?;
            r
        }
    };
    // Importing again fills in issues and the rest; the code may have moved on here, so it's left alone.
    // The next stage is saved with the repository, so a crash in between can't send it to the copy.
    let to = if fresh { "clone" } else { "labels" };
    st.db.call(move |c| Ok(c.execute("UPDATE import_items SET repo_id = ?2, stage = ?3 WHERE id = ?1", params![id, repo, to])?)).await.map_err(|e| e.msg)?;
    Ok(Flow::Jump(to))
}

async fn load_users(st: &AppState, uid: i64, owner: i64) -> Result<(User, User), String> {
    let sql = format!("SELECT {} FROM users WHERE id = ?1 AND suspended = 0", crate::auth::USER_COLS);
    st.db
        .call(move |c| {
            let u = c.query_row(&sql, [uid], User::from_row).optional()?;
            let o = c.query_row(&sql, [owner], User::from_row).optional()?;
            Ok(u.zip(o))
        })
        .await
        .map_err(|e| e.msg)?
        .ok_or_else(|| "the account importing (or the owner) is gone or suspended".to_string())
}

fn mapped(tx: &Transaction, repo: i64, kind: &str, rid: &str) -> rusqlite::Result<Option<i64>> {
    tx.query_row("SELECT local_id FROM import_map WHERE repo_id = ?1 AND kind = ?2 AND remote_id = ?3", params![repo, kind, rid], |r| r.get(0)).optional()
}

fn map(tx: &Transaction, repo: i64, kind: &str, rid: &str, local: i64) -> rusqlite::Result<()> {
    tx.execute("INSERT OR IGNORE INTO import_map(repo_id, kind, remote_id, local_id) VALUES (?1, ?2, ?3, ?4)", params![repo, kind, rid, local])?;
    Ok(())
}

/// The number, or GitLab's iid, as a string key.
fn num(v: &Value, gl: bool) -> Option<i64> {
    v[if gl { "iid" } else { "number" }].as_i64()
}

fn color(c: &str) -> String {
    let h = c.trim_start_matches('#');
    if h.len() == 6 && h.chars().all(|c| c.is_ascii_hexdigit()) { format!("#{}", h.to_ascii_lowercase()) } else { "#8b949e".into() }
}

struct NewIssue {
    key: String,
    number: i64,
    pr: bool,
    title: String,
    body: String,
    open: bool,
    created: i64,
    closed: Option<i64>,
    milestone: Option<String>,
    labels: Vec<String>,
    pull: Option<Pull>,
}

struct Pull {
    head: String,
    base: String,
    head_sha: Option<String>,
    base_sha: Option<String>,
    merged: Option<i64>,
}

/// False when its number is taken here by something that wasn't imported (it's left out).
fn put_issue(tx: &Transaction, repo: i64, uid: i64, n: &NewIssue) -> rusqlite::Result<bool> {
    let kind = if n.pr { "pull" } else { "issue" };
    if mapped(tx, repo, kind, &n.key)?.is_some() {
        return Ok(true);
    }
    let milestone = match &n.milestone {
        Some(k) => mapped(tx, repo, "milestone", k)?,
        None => None,
    };
    let title: String = short(&n.title, 256).replace(['\n', '\t'], " ");
    let added = tx.execute(
        "INSERT OR IGNORE INTO issues(repo_id, number, author_id, title, body, is_pr, state, created_at, updated_at, closed_at, milestone_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?9, ?10)",
        params![repo, n.number, uid, if title.trim().is_empty() { "(untitled)" } else { title.trim() }, n.body, n.pr, if n.open { "open" } else { "closed" }, n.created, n.closed, milestone],
    )?;
    if added == 0 {
        return Ok(false);
    }
    let id = tx.last_insert_rowid();
    for l in &n.labels {
        tx.execute(
            "INSERT OR IGNORE INTO issue_labels(issue_id, label_id) SELECT ?1, id FROM labels WHERE repo_id = ?2 AND name = ?3",
            params![id, repo, l],
        )?;
    }
    if let Some(p) = &n.pull {
        tx.execute(
            "INSERT INTO pulls(issue_id, head_branch, base_branch, head_sha, base_sha, merged_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, p.head, p.base, p.head_sha, p.base_sha, p.merged],
        )?;
    }
    map(tx, repo, kind, &n.key, id)?;
    Ok(true)
}

/// Commits the repository has (pull request heads may be gone at the source), in one git call.
async fn present(st: &AppState, repo: i64, shas: Vec<String>) -> std::collections::HashSet<String> {
    let input: String = shas.iter().filter(|s| validate::sha(s)).map(|s| format!("{s}\n")).collect();
    if input.is_empty() {
        return Default::default();
    }
    let out = st.git.run(&st.git.repo_path(repo), &["cat-file", "--batch-check"], Some(input.as_bytes()), &[]).await;
    // "<sha> commit <size>" for each that's there, "<sha>^{commit} missing" for the rest.
    out.map(|o| o.text().lines().filter_map(|l| l.split_once(" commit ").map(|(sha, _)| sha.to_string())).collect()).unwrap_or_default()
}

/// Write one page of `stage`.
async fn write(st: &AppState, item: &Item, src: &Source, stage: &str, rows: Vec<Value>) -> Result<(), String> {
    let repo = item.repo_id.ok_or("the repository is gone")?;
    let (uid, kind, gl, id) = (item.user_id, src.kind, src.kind == Kind::GitLab, item.id);
    let stage = stage.to_string();
    // Pull requests: which head and base commits came with the clone.
    let have = if stage == "pulls" {
        let shas = rows.iter().flat_map(|r| [text(&r["head"]["sha"]), text(&r["base"]["sha"]), text(&r["sha"]), text(&r["diff_refs"]["base_sha"])]).filter(|s| !s.is_empty()).collect();
        present(st, repo, shas).await
    } else {
        Default::default()
    };
    let tags: std::collections::HashSet<String> = if stage == "releases" {
        st.git.refs(&st.git.repo_path(repo)).await.map_err(|e| e.msg)?.into_iter().filter(|r| r.kind == "tag").map(|r| r.name).collect()
    } else {
        Default::default()
    };
    let mut offset = item.offset;
    st.db
        .call(move |c| {
            let tx = c.transaction()?;
            let t = now();
            let mut skipped = 0;
            match stage.as_str() {
                "labels" => {
                    for r in &rows {
                        let name: String = short(&text(&r["name"]), 50).trim().to_string();
                        if name.is_empty() {
                            continue;
                        }
                        tx.execute("INSERT OR IGNORE INTO labels(repo_id, name, color) VALUES (?1, ?2, ?3)", params![repo, name, color(&text(&r["color"]))])?;
                    }
                }
                "milestones" => {
                    for r in &rows {
                        let Some(n) = num(r, gl) else { continue };
                        if mapped(&tx, repo, "milestone", &n.to_string())?.is_some() {
                            continue;
                        }
                        let due = text(if gl { &r["due_date"] } else { &r["due_on"] }).chars().take(10).collect::<String>();
                        let due = (due.len() == 10 && epoch(&format!("{due}T00:00:00Z")).is_some()).then_some(due);
                        let open = matches!(r["state"].as_str(), Some("open" | "active"));
                        let created = when(&r["created_at"]).unwrap_or(t);
                        let closed = if open { None } else { when(&r["closed_at"]).or(when(&r["updated_at"])).or(Some(t)) };
                        let added = tx.execute(
                            "INSERT OR IGNORE INTO milestones(repo_id, number, title, description, due_on, state, created_at, updated_at, closed_at)
                             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, ?8)",
                            params![repo, n, short(&text(&r["title"]), 200), short(&text(&r["description"]), 10_000), due, if open { "open" } else { "closed" }, created, closed],
                        )?;
                        if added > 0 {
                            map(&tx, repo, "milestone", &n.to_string(), tx.last_insert_rowid())?;
                        }
                    }
                }
                "issues" | "pulls" => {
                    let pr = stage == "pulls";
                    if pr && gl && offset < 0 {
                        // Merge requests go after the highest issue number, fixed once.
                        offset = tx.query_row("SELECT coalesce(max(number), 0) FROM issues WHERE repo_id = ?1 AND is_pr = 0", [repo], |r| r.get(0))?;
                        tx.execute("UPDATE import_items SET number_offset = ?2 WHERE id = ?1", params![id, offset])?;
                    }
                    for r in &rows {
                        // GitHub lists pull requests among issues too; they come with their branches next.
                        if !pr && !r["pull_request"].is_null() {
                            continue;
                        }
                        let Some(n) = num(r, gl) else { continue };
                        let login = text(if gl { &r["author"]["username"] } else { &r["user"]["login"] });
                        let created = when(&r["created_at"]).unwrap_or(t);
                        let open = matches!(r["state"].as_str(), Some("open" | "opened"));
                        let merged = when(&r["merged_at"]);
                        let note = if pr && gl { format!(" (merge request !{n})") } else { String::new() };
                        let labels: Vec<String> = r["labels"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .map(|l| if gl { text(l) } else { text(&l["name"]) })
                            .collect();
                        let sha = |v: &Value| Some(text(v)).filter(|s| have.contains(s));
                        let pull = pr.then(|| {
                            if gl {
                                Pull { head: text(&r["source_branch"]), base: text(&r["target_branch"]), head_sha: sha(&r["sha"]), base_sha: sha(&r["diff_refs"]["base_sha"]), merged }
                            } else {
                                Pull { head: text(&r["head"]["ref"]), base: text(&r["base"]["ref"]), head_sha: sha(&r["head"]["sha"]), base_sha: sha(&r["base"]["sha"]), merged }
                            }
                        });
                        let issue = NewIssue {
                            key: n.to_string(),
                            number: if pr && gl { offset + n } else { n },
                            pr,
                            title: text(&r["title"]),
                            body: attributed(kind, &login, created, &note, &text(if gl { &r["description"] } else { &r["body"] })),
                            open,
                            created,
                            closed: if open { None } else { when(&r["closed_at"]).or(merged).or(Some(t)) },
                            milestone: num(&r["milestone"], gl).map(|m| m.to_string()),
                            labels,
                            pull,
                        };
                        if !put_issue(&tx, repo, uid, &issue)? {
                            skipped += 1;
                        }
                    }
                }
                "comments" => {
                    // GitHub: one list for the whole repository, each naming its issue by URL.
                    for r in &rows {
                        let Some(rid) = r["id"].as_i64().map(|n| n.to_string()) else { continue };
                        let Some(n) = text(&r["issue_url"]).rsplit('/').next().and_then(|n| n.parse::<i64>().ok()) else { continue };
                        // Through what was imported: a number taken here by something else isn't that issue.
                        let issue = match mapped(&tx, repo, "issue", &n.to_string())? {
                            Some(i) => Some(i),
                            None => mapped(&tx, repo, "pull", &n.to_string())?,
                        };
                        if let Some(issue) = issue {
                            put_comment(&tx, repo, uid, issue, &rid, kind, &text(&r["user"]["login"]), &r["created_at"], &text(&r["body"]))?;
                        }
                    }
                }
                _ => {
                    for r in &rows {
                        let tag = text(&r["tag_name"]);
                        let rid = r["id"].as_i64().map(|n| n.to_string()).unwrap_or_else(|| tag.clone());
                        if !validate::ref_name(&tag) || mapped(&tx, repo, "release", &rid)?.is_some() {
                            continue;
                        }
                        let created = when(&r["created_at"]).unwrap_or(t);
                        // Without its tag here (or still upcoming) it can only be a draft.
                        let draft = r["draft"] == true || r["upcoming_release"] == true || !tags.contains(&tag);
                        let published = if draft { None } else { when(if gl { &r["released_at"] } else { &r["published_at"] }).or(Some(created)) };
                        let notes = text(if gl { &r["description"] } else { &r["body"] });
                        let added = tx.execute(
                            "INSERT OR IGNORE INTO releases(repo_id, tag, title, notes, draft, prerelease, author_id, created_at, published_at)
                             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                            params![repo, tag, short(&text(&r["name"]), 200), short(&notes, 65_000), draft, r["prerelease"] == true, uid, created, published],
                        )?;
                        if added > 0 {
                            map(&tx, repo, "release", &rid, tx.last_insert_rowid())?;
                        }
                    }
                }
            }
            if skipped > 0 {
                tx.execute("UPDATE import_items SET skipped = skipped + ?2 WHERE id = ?1", params![id, skipped])?;
            }
            tx.commit()?;
            Ok(())
        })
        .await
        .map_err(|e| e.msg)
}

#[allow(clippy::too_many_arguments)]
fn put_comment(tx: &Transaction, repo: i64, uid: i64, issue: i64, rid: &str, kind: Kind, login: &str, at: &Value, body: &str) -> rusqlite::Result<()> {
    if mapped(tx, repo, "comment", rid)?.is_some() {
        return Ok(());
    }
    let t = when(at).unwrap_or_else(now);
    tx.execute(
        "INSERT INTO comments(issue_id, author_id, kind, body, created_at, updated_at) VALUES (?1, ?2, 'comment', ?3, ?4, ?4)",
        params![issue, uid, attributed(kind, login, t, "", body), t],
    )?;
    map(tx, repo, "comment", rid, tx.last_insert_rowid())
}

/// GitLab keeps comments per issue and merge request: the cursor is the local issue
/// done so far, and while one has more pages, its next page's URL after a space.
async fn gitlab_notes(st: &AppState, item: &Item, src: &Source) -> Result<Flow, String> {
    let repo = item.repo_id.ok_or("the repository is gone")?;
    let (done, next) = match item.cursor.split_once(' ') {
        Some((d, n)) => (d.parse::<i64>().unwrap_or(0), Some(n.to_string())),
        None => (item.cursor.parse::<i64>().unwrap_or(0), None),
    };
    // The issue whose notes are being read: the one with a page pending, or the next one.
    let after = if next.is_some() { done - 1 } else { done };
    let target: Option<(String, String, i64)> = st
        .db
        .call(move |c| {
            Ok(c.query_row(
                "SELECT kind, remote_id, local_id FROM import_map WHERE repo_id = ?1 AND kind IN ('issue', 'pull') AND local_id > ?2 ORDER BY local_id LIMIT 1",
                params![repo, after],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?)
        })
        .await
        .map_err(|e| e.msg)?;
    let Some((kind, iid, local)) = target else { return Ok(Flow::Next) };
    let what = if kind == "pull" { "merge_requests" } else { "issues" };
    let url = next.unwrap_or_else(|| format!("{}/projects/{}/{what}/{iid}/notes?per_page=100&sort=asc", src.api, item.remote_id));
    let p = match get(src, &url).await? {
        Got::Page(p) => p,
        Got::Wait(t) => return Ok(Flow::Wait(t)),
        Got::Retry(e) => return Ok(Flow::Retry(e)),
    };
    if p.next.as_deref() == Some(url.as_str()) {
        return Err("its pages lead back to the same page".into());
    }
    if p.next.is_some() && item.pages + 1 >= MAX_PAGES {
        return Err(format!("the comments of one of its issues go on past {MAX_PAGES} pages"));
    }
    let rows = p.body.as_array().cloned().ok_or("the answer isn't a list")?;
    let uid = item.user_id;
    st.db
        .call(move |c| {
            let tx = c.transaction()?;
            for r in rows.iter().filter(|r| r["system"] != true) {
                let Some(rid) = r["id"].as_i64().map(|n| n.to_string()) else { continue };
                put_comment(&tx, repo, uid, local, &rid, Kind::GitLab, &text(&r["author"]["username"]), &r["created_at"], &text(&r["body"]))?;
            }
            tx.commit()?;
            Ok(())
        })
        .await
        .map_err(|e| e.msg)?;
    Ok(match p.next {
        Some(n) => Flow::Stay(format!("{local} {n}")),
        None => Flow::Moved(local.to_string()),
    })
}

// ---------------------------------------------------------------- the API

#[derive(Deserialize)]
pub struct JobReq {
    kind: String,
    #[serde(default)]
    base_url: String,
    #[serde(default)]
    token: String,
    #[serde(default)]
    source: String,
    #[serde(default)]
    owner: String,
    #[serde(default)]
    repos: Vec<String>,
    #[serde(default)]
    allow_private: bool,
}

fn source_of(req: &JobReq, u: &User) -> R<(Source, String)> {
    let kind = Kind::parse(req.kind.trim()).ok_or_else(|| AppError::bad("Import from github or gitlab"))?;
    let base = match req.base_url.trim() {
        "" if kind == Kind::GitHub => "https://github.com",
        "" => "https://gitlab.com",
        b => b,
    };
    if req.allow_private && !u.is_admin {
        return Err(AppError::forbidden());
    }
    check_base(base, req.allow_private).map_err(AppError::bad)?;
    let token = req.token.trim();
    if token.is_empty() || token.len() > 500 || token.chars().any(|c| c.is_control() || c == ' ') {
        return Err(AppError::bad("Paste an access token from the provider (it needs read access to the repositories)"));
    }
    let source = req.source.trim().trim_matches('/');
    let ok = !source.is_empty() && source.len() <= 200 && source.chars().all(|c| c.is_ascii_alphanumeric() || "-_./".contains(c)) && !source.contains("..");
    if !ok {
        return Err(AppError::bad("Name the user, organization or group to import from"));
    }
    Ok((Source::new(kind, base, token.to_string(), req.allow_private), source.to_string()))
}

fn listing_error(src: &Source, e: String) -> AppError {
    AppError::new(axum::http::StatusCode::BAD_GATEWAY, format!("Couldn't list the repositories at {}: {}", src.web, src.redact(&e)))
}

/// The source's repositories, for picking.
pub async fn preview(State(st): State<AppState>, Authed(u): Authed, Json(req): Json<JobReq>) -> J<Value> {
    if !st.limiter.allow(&format!("import-list:{}", u.id), 30, 600) {
        return Err(AppError::new(axum::http::StatusCode::TOO_MANY_REQUESTS, "Too many lookups. Wait a few minutes."));
    }
    let (src, source) = source_of(&req, &u)?;
    let (repos, truncated) = list(&src, &source).await.map_err(|e| listing_error(&src, e))?;
    let repos: Vec<Value> = repos.iter().map(|r| json!({ "source": r.path, "name": r.name, "private": r.private, "description": r.description })).collect();
    Ok(Json(json!({ "repos": repos, "truncated": truncated, "max": MAX_REPOS })))
}

pub async fn start(State(st): State<AppState>, Authed(u): Authed, ip: ClientIp, Json(req): Json<JobReq>) -> J<Value> {
    // Starting lists the source again, so it shares the lookups' budget.
    if !st.limiter.allow(&format!("import-list:{}", u.id), 30, 600) {
        return Err(AppError::new(axum::http::StatusCode::TOO_MANY_REQUESTS, "Too many lookups. Wait a few minutes."));
    }
    let (src, source) = source_of(&req, &u)?;
    if req.repos.is_empty() || req.repos.len() > MAX_REPOS {
        return Err(AppError::bad(format!("Pick 1 to {MAX_REPOS} repositories")));
    }
    let owner = crate::api::repos::owner_for(&st, &u, Some(req.owner.trim()).filter(|o| !o.is_empty())).await?;
    let uid = u.id;
    let busy = st.db.call(move |c| Ok(c.query_row("SELECT 1 FROM import_jobs WHERE user_id = ?1 AND state = 'running'", [uid], |_| Ok(())).optional()?)).await?;
    if busy.is_some() {
        return Err(AppError::conflict("You have an import running. Wait for it to finish, or cancel it."));
    }
    let (listed, _) = list(&src, &source).await.map_err(|e| listing_error(&src, e))?;
    let mut picked: Vec<(String, String, String, bool, String)> = vec![];
    for want in &req.repos {
        let l = listed.iter().find(|l| l.path.eq_ignore_ascii_case(want.trim())).ok_or_else(|| AppError::bad(format!("{} isn't among {source}'s repositories", short(want, 100))))?;
        if !picked.iter().any(|p| p.0 == l.path) {
            picked.push((l.path.clone(), l.remote_id.clone(), l.name.clone(), l.private, l.description.clone()));
        }
    }
    let sealed = seal_text(&st.key, &src.token);
    let (kind, base, ap, oid, n) = (req.kind.trim().to_string(), src.web.clone(), src.allow_private, owner.id, picked.len());
    let id = st
        .db
        .call(move |c| {
            let tx = c.transaction()?;
            let t = now();
            // The unique index on running jobs settles two starts at once.
            tx.execute(
                "INSERT INTO import_jobs(user_id, owner_id, kind, base_url, token, allow_private, state, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'running', ?7, ?7)",
                params![uid, oid, kind, base, sealed, ap, t],
            )
            .map_err(|e| match e.sqlite_error_code() {
                Some(rusqlite::ErrorCode::ConstraintViolation) => AppError::conflict("You have an import running. Wait for it to finish, or cancel it."),
                _ => e.into(),
            })?;
            let job = tx.last_insert_rowid();
            for (path, rid, name, private, description) in picked {
                tx.execute(
                    "INSERT INTO import_items(job_id, source, remote_id, name, private, description) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![job, path, rid, name, private, description],
                )?;
            }
            tx.commit()?;
            Ok(job)
        })
        .await?;
    record(&st.db, Event { actor: Some(&u), subject: Some(u.id), action: "import_started", detail: format!("{n} from {} {source}", src.web), ip: &ip.0 }).await;
    Ok(Json(json!({ "id": id })))
}

/// Your imports, newest first, with each repository's stage.
pub async fn mine(State(st): State<AppState>, Authed(u): Authed) -> J<Value> {
    let uid = u.id;
    let jobs = st
        .db
        .call(move |c| {
            let mut q = c.prepare(
                "SELECT j.id, j.kind, j.base_url, j.state, j.error, j.created_at, j.updated_at, o.username FROM import_jobs j JOIN users o ON o.id = j.owner_id
                 WHERE j.user_id = ?1 ORDER BY j.id DESC LIMIT 20",
            )?;
            let jobs: Vec<(i64, Value)> = q
                .query_map([uid], |r| {
                    Ok((r.get(0)?, json!({ "id": r.get::<_, i64>(0)?, "kind": r.get::<_, String>(1)?, "base_url": r.get::<_, String>(2)?, "state": r.get::<_, String>(3)?,
                        "error": r.get::<_, String>(4)?, "created_at": r.get::<_, i64>(5)?, "updated_at": r.get::<_, i64>(6)?, "owner": r.get::<_, String>(7)? })))
                })?
                .collect::<Result<_, _>>()?;
            let mut items = c.prepare(
                "SELECT i.source, i.name, i.stage, i.error, i.next_at, r.name FROM import_items i LEFT JOIN repos r ON r.id = i.repo_id AND r.deleted_at IS NULL
                 WHERE i.job_id = ?1 ORDER BY i.id",
            )?;
            let mut out = vec![];
            for (id, mut j) in jobs {
                let list: Vec<Value> = items
                    .query_map([id], |r| {
                        Ok(json!({ "source": r.get::<_, String>(0)?, "name": r.get::<_, String>(1)?, "stage": r.get::<_, String>(2)?, "error": r.get::<_, String>(3)?,
                            "waiting_until": r.get::<_, i64>(4)?, "repo": r.get::<_, Option<String>>(5)? }))
                    })?
                    .collect::<Result<_, _>>()?;
                j["items"] = json!(list);
                out.push(j);
            }
            Ok(out)
        })
        .await?;
    Ok(Json(json!({ "jobs": jobs, "stages": STAGES })))
}

pub async fn cancel(State(st): State<AppState>, Authed(u): Authed, Path(id): Path<i64>) -> J<Value> {
    let (uid, admin) = (u.id, u.is_admin);
    let n = st
        .db
        .call(move |c| {
            Ok(c.execute(
                "UPDATE import_jobs SET state = 'cancelled', token = NULL, updated_at = ?3 WHERE id = ?1 AND (user_id = ?2 OR ?4) AND state = 'running'",
                params![id, uid, now(), admin],
            )?)
        })
        .await?;
    if n == 0 {
        return Err(AppError::not_found());
    }
    Ok(Json(json!({ "ok": true })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_links_and_colors() {
        assert_eq!(epoch("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(epoch("2020-01-04T00:00:00.000+02:00"), Some(1_578_088_800));
        assert_eq!(epoch("2020-01-04T00:00:00Z"), Some(1_578_096_000));
        assert_eq!(epoch("2020-13-04T00:00:00Z"), None);
        assert_eq!(epoch("nope"), None);
        assert_eq!(day(1_578_096_000), "2020-01-04");
        let h = r#"<https://api.github.com/x?page=2>; rel="next", <https://api.github.com/x?page=5>; rel="last""#;
        assert_eq!(next_link(h).as_deref(), Some("https://api.github.com/x?page=2"));
        assert_eq!(next_link(r#"<https://a/x?page=1>; rel="prev""#), None);
        assert_eq!((color("D73A4A"), color("#123456"), color("red")), ("#d73a4a".into(), "#123456".into(), "#8b949e".into()));
        assert_eq!(origin("https://API.github.com/repos/a"), Some("https://api.github.com".into()));
        assert_ne!(origin("https://api.github.com.evil.example/x"), origin("https://api.github.com/x"));
        let a = attributed(Kind::GitHub, "oc`to @x", 0, "", "hi @alice");
        assert!(a.starts_with("*Imported from GitHub: by `octox` on 1970-01-01.*") && a.ends_with("hi @alice"), "{a}");
    }
}
