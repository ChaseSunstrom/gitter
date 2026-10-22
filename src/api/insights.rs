//! Repository insights beyond contributors: the commit graph (lanes for a page
//! of history across every branch), a file's history through renames, and
//! traffic (views and clones per day, for the repository's admins).

use std::collections::{HashMap, HashSet};

use axum::Json;
use axum::extract::{Path, Query, State};
use rand::RngCore;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{ClientIp, J};
use crate::auth::Viewer;
use crate::db::{Db, now};
use crate::error::{AppError, R};
use crate::git::{COMMIT_FMT, Commit, parse_log};
use crate::repo::Repo;
use crate::{AppState, validate};

#[derive(Deserialize)]
pub struct PageQ {
    page: Option<usize>,
    per: Option<usize>,
}

// ---------- commit graph

/// The graph only reaches this far back: laying out page N means walking every row before it.
const GRAPH_DEPTH: usize = 10_000;

/// Where one commit sits in the graph. Lanes are columns; each row's lines run
/// from the top edge to the bottom edge of the row: `pass` lanes go straight
/// through, `up` lanes come down into the dot, `down` lanes leave it for the
/// parents (first parent first).
#[derive(Serialize, Debug, PartialEq)]
pub struct Row {
    pub lane: usize,
    pub pass: Vec<usize>,
    pub up: Vec<usize>,
    pub down: Vec<usize>,
}

fn free_lane(open: &mut Vec<Option<&str>>) -> usize {
    open.iter().position(Option::is_none).unwrap_or_else(|| {
        open.push(None);
        open.len() - 1
    })
}

/// Lanes past this one aren't sent (a repository with thousands of unmerged
/// branches would otherwise send thousands of lines per row).
const GRAPH_LANES: usize = 64;

/// Lay out commits given newest first in topological order, as (sha, parents),
/// returning the rows from `from` on (the earlier ones only set up the lanes).
pub fn layout(rows: &[(String, Vec<String>)], from: usize) -> Vec<Row> {
    // Each open lane waits for the commit it leads to.
    let mut open: Vec<Option<&str>> = Vec::new();
    let mut out = Vec::new();
    for (n, (sha, parents)) in rows.iter().enumerate() {
        let me = Some(sha.as_str());
        let up: Vec<usize> = (0..open.len()).filter(|&i| open[i] == me).collect();
        let pass: Vec<usize> = if n < from {
            Vec::new()
        } else {
            (0..open.len().min(GRAPH_LANES)).filter(|&i| open[i].is_some() && open[i] != me).collect()
        };
        let lane = up.first().copied().unwrap_or_else(|| free_lane(&mut open));
        for &i in &up {
            open[i] = None;
        }
        let mut down = Vec::new();
        for (k, p) in parents.iter().enumerate() {
            // The first parent carries straight on (lanes that share it meet at
            // the parent); a merged parent joins a lane already heading there.
            let j = match open.iter().position(|l| *l == Some(p.as_str())).filter(|_| k > 0) {
                Some(j) => j,
                None => {
                    let j = if k == 0 { lane } else { free_lane(&mut open) };
                    open[j] = Some(p);
                    j
                }
            };
            if !down.contains(&j) {
                down.push(j);
            }
        }
        while open.last() == Some(&None) {
            open.pop();
        }
        if n >= from {
            let keep = |v: Vec<usize>| v.into_iter().filter(|&j| j < GRAPH_LANES).collect();
            out.push(Row { lane, pass, up: keep(up), down: keep(down) });
        }
    }
    out
}

/// A page of history across every branch, each commit with its graph row.
pub async fn graph(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    Path((o, n)): Path<(String, String)>,
    Query(q): Query<PageQ>,
) -> J<Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let per = q.per.unwrap_or(35).clamp(1, 100);
    let page = super::page(q.page);
    let start = (page - 1) * per;
    if start >= GRAPH_DEPTH {
        return Ok(Json(json!({ "commits": [], "has_more": false, "page": page, "width": 0 })));
    }
    let end = (start + per).min(GRAPH_DEPTH);
    let n = format!("-n{}", end + 1);
    let out = st
        .git
        .run_records(&r.path, &["rev-list", "--parents", "--topo-order", "--branches", &n, "--"], 8 << 20, b'\n')
        .await?;
    let mut rows: Vec<(String, Vec<String>)> = String::from_utf8_lossy(&out)
        .lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace().map(String::from);
            Some((it.next()?, it.collect()))
        })
        .collect();
    let has_more = rows.len() > end && end < GRAPH_DEPTH;
    rows.truncate(end);
    let lanes = layout(&rows, start);
    let shas: Vec<&str> = rows.iter().skip(start).map(|(s, _)| s.as_str()).collect();
    let mut list = if shas.is_empty() { Vec::new() } else { commits_of(&st, &r, &shas).await? };
    crate::signing::annotate(&st, &r, &mut list).await;
    let by_sha: HashMap<&str, &Row> = rows.iter().skip(start).zip(&lanes).map(|((s, _), l)| (s.as_str(), l)).collect();
    let mut width = 0;
    let commits: Vec<Value> = list
        .iter()
        .filter_map(|c| {
            let row = by_sha.get(c.sha.as_str())?;
            let widest = row.pass.iter().chain(&row.up).chain(&row.down).copied().chain([row.lane]).max().unwrap_or(0);
            width = width.max(widest + 1).min(GRAPH_LANES);
            let mut v = serde_json::to_value(c).ok()?;
            let g = serde_json::to_value(row).ok()?;
            v.as_object_mut()?.extend(g.as_object()?.clone());
            Some(v)
        })
        .collect();
    Ok(Json(json!({ "commits": commits, "has_more": has_more, "page": page, "width": width })))
}

/// Full details of these commits, in this order.
async fn commits_of(st: &AppState, r: &Repo, shas: &[&str]) -> R<Vec<Commit>> {
    let fmt = format!("--format={COMMIT_FMT}");
    let mut args = vec!["show", "-s", "-z", fmt.as_str()];
    args.extend(shas);
    args.push("--");
    let out = st.git.run_records(&r.path, &args, 8 << 20, 0).await?;
    Ok(parse_log(&String::from_utf8_lossy(&out)))
}

// ---------- file history

#[derive(Deserialize)]
pub struct HistoryQ {
    rev: Option<String>,
    path: Option<String>,
    page: Option<usize>,
    per: Option<usize>,
}

#[derive(Serialize, Debug)]
pub struct Version {
    pub commit: Commit,
    /// added, modified, deleted, renamed or copied
    pub status: &'static str,
    /// The file's name in this commit.
    pub path: String,
    /// Its name before, when this commit renamed or copied it.
    pub old_path: Option<String>,
}

fn status_word(s: &str) -> Option<&'static str> {
    let mut ch = s.chars();
    let word = match ch.next()? {
        'A' => "added",
        'D' => "deleted",
        'R' => "renamed",
        'C' => "copied",
        'M' | 'T' | 'U' | 'X' => "modified",
        _ => return None,
    };
    (s.len() <= 4 && ch.all(|c| c.is_ascii_digit())).then_some(word)
}

/// `git log -z --name-status --format=COMMIT_FMT` for one path: NUL-separated
/// tokens, each commit followed by a status and its path (two for renames).
pub fn parse_history(s: &str, path: &str) -> Vec<Version> {
    let mut out: Vec<Version> = Vec::new();
    let mut toks = s.split('\0');
    // A commit without a status line (a merge) keeps the name the file had in the newer one.
    let mut current = path.to_string();
    while let Some(t) = toks.next() {
        let bare = t.trim_start_matches('\n');
        if let Some(word) = status_word(bare) {
            let first = toks.next().unwrap_or("").to_string();
            let Some(last) = out.last_mut() else { continue };
            last.status = word;
            if matches!(word, "renamed" | "copied") {
                last.path = toks.next().unwrap_or("").to_string();
                last.old_path = Some(first.clone());
            } else {
                last.path = first.clone();
            }
            // Older commits know the file by its name before this one.
            current = first;
        } else if let Some(commit) = parse_log(t).into_iter().next() {
            out.push(Version { commit, status: "modified", path: current.clone(), old_path: None });
        }
    }
    out
}

/// The commits that touched one file, newest first, following it through renames.
pub async fn history(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    Path((o, n)): Path<(String, String)>,
    Query(q): Query<HistoryQ>,
) -> J<Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let path = q.path.as_deref().unwrap_or("").trim_end_matches('/');
    if path.is_empty() || !validate::path(path) {
        return Err(AppError::bad("Pick a file to see its history"));
    }
    let sha = super::code::resolve_rev(&st, &r, q.rev.as_deref()).await?;
    let per = q.per.unwrap_or(50).clamp(1, 100);
    let page = super::page(q.page);
    // No --skip: git only notices a rename on a commit it shows, so skipping past one
    // loses the file. Pages are cut here instead, as deep as the graph goes.
    let start = (page - 1) * per;
    if start >= GRAPH_DEPTH {
        return Ok(Json(json!({ "sha": sha, "path": path, "versions": [], "has_more": false, "page": page })));
    }
    let end = (start + per).min(GRAPH_DEPTH);
    let (fmt, lim) = (format!("--format={COMMIT_FMT}"), format!("-n{}", end + 1));
    let out = st
        .git
        .run_records(&r.path, &["log", "--follow", "-M", "-z", "--name-status", &fmt, &lim, &sha, "--", path], 16 << 20, 0)
        .await?;
    let mut versions = parse_history(&String::from_utf8_lossy(&out), path);
    // A record cut off at the output cap has no name; drop it.
    versions.retain(|v| !v.path.is_empty());
    let has_more = versions.len() > end && end < GRAPH_DEPTH;
    versions.truncate(end);
    versions.drain(..start.min(versions.len()));
    let mut commits: Vec<Commit> = versions.iter().map(|x| x.commit.clone()).collect();
    crate::signing::annotate(&st, &r, &mut commits).await;
    for (x, c) in versions.iter_mut().zip(commits) {
        x.commit = c;
    }
    Ok(Json(json!({ "sha": sha, "path": path, "versions": versions, "has_more": has_more, "page": page })))
}

// ---------- traffic

/// Days of traffic kept and shown.
const KEEP_DAYS: i64 = 14;
/// Distinct referring sites, and distinct pages, kept per repository per day.
const MAX_NAMES: i64 = 100;
/// Distinct visitors counted per repository per day (views and clones each).
const MAX_VISITORS: i64 = 50_000;
/// Entries held in memory between flushes.
const MAX_PENDING: usize = 100_000;

type Key = (i64, i64, &'static str, String);

/// Views and clones counted in memory and written once a minute (and at
/// shutdown), so a page view never costs a database write.
#[derive(Default)]
pub struct Traffic(std::sync::Mutex<Pending>);

#[derive(Default, Clone)]
struct Pending {
    /// The UTC day `salt` belongs to.
    day: i64,
    /// Random, per day, and never stored: a visitor hash can't be traced back to an
    /// address, nor matched across days.
    // ponytail: a restart mid-day draws a new salt, so that day's returning visitors count twice.
    salt: [u8; 16],
    pruned: i64,
    counts: HashMap<Key, i64>,
    visitors: HashSet<Key>,
}

impl Traffic {
    fn lock(&self) -> std::sync::MutexGuard<'_, Pending> {
        self.0.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Count one view (kind "view") or clone ("clone") of repository `repo` from `ip`,
    /// with the page and referring site of a view.
    pub fn record(&self, repo: i64, kind: &'static str, ip: &str, path: Option<String>, referrer: Option<String>) {
        let day = now() / 86400;
        let mut p = self.lock();
        if p.day != day {
            p.day = day;
            rand::rngs::OsRng.fill_bytes(&mut p.salt);
        }
        // ponytail: when full (a flood inside one minute), counts are dropped until the next flush.
        if p.counts.len() + p.visitors.len() + 4 > MAX_PENDING {
            return;
        }
        let mut h = crate::auth::sha256_hex(&format!("{}:{}", hex::encode(p.salt), crate::auth::ip_bucket(ip)));
        h.truncate(32);
        p.visitors.insert((repo, day, kind, h));
        *p.counts.entry((repo, day, kind, String::new())).or_default() += 1;
        for (k, name) in [("path", path), ("ref", referrer)] {
            if let Some(name) = name {
                *p.counts.entry((repo, day, k, name)).or_default() += 1;
            }
        }
    }

    /// Write what's pending. A failed write keeps it for the next try.
    pub async fn flush(&self, db: &Db) {
        let (counts, visitors, prune) = {
            let mut p = self.lock();
            let today = now() / 86400;
            let prune = (p.pruned != today).then_some(today - KEEP_DAYS + 1);
            if p.counts.is_empty() && p.visitors.is_empty() && prune.is_none() {
                return;
            }
            p.pruned = today;
            (std::mem::take(&mut p.counts), std::mem::take(&mut p.visitors), prune)
        };
        let (c2, v2) = (counts.clone(), visitors.clone());
        let res = db.call(move |c| Ok(write_traffic(c, &c2, &v2, prune)?)).await;
        if let Err(e) = res {
            eprintln!("saving traffic counts: {}", e.msg);
            let mut p = self.lock();
            p.pruned = 0;
            if p.counts.len() + p.visitors.len() + counts.len() + visitors.len() <= MAX_PENDING {
                for (k, n) in counts {
                    *p.counts.entry(k).or_default() += n;
                }
                p.visitors.extend(visitors);
            }
        }
    }
}

fn write_traffic(c: &mut rusqlite::Connection, counts: &HashMap<Key, i64>, visitors: &HashSet<Key>, prune: Option<i64>) -> rusqlite::Result<()> {
    let tx = c.transaction()?;
    {
        // Repositories purged since are skipped; new names stop at MAX_NAMES a day.
        let mut add = tx.prepare(
            "INSERT INTO traffic(repo_id, day, kind, name, count) SELECT ?1, ?2, ?3, ?4, ?5
             WHERE EXISTS (SELECT 1 FROM repos WHERE id = ?1)
               AND (?4 = '' OR EXISTS (SELECT 1 FROM traffic WHERE repo_id = ?1 AND day = ?2 AND kind = ?3 AND name = ?4)
                    OR (SELECT count(*) FROM traffic WHERE repo_id = ?1 AND day = ?2 AND kind = ?3) < ?6)
             ON CONFLICT(repo_id, day, kind, name) DO UPDATE SET count = count + excluded.count",
        )?;
        for ((repo, day, kind, name), n) in counts {
            add.execute(params![repo, day, kind, name, n, MAX_NAMES])?;
        }
        let mut groups: HashMap<(i64, i64, &str), Vec<&str>> = HashMap::new();
        for (repo, day, kind, h) in visitors {
            groups.entry((*repo, *day, kind)).or_default().push(h);
        }
        let mut have = tx.prepare("SELECT count(*) FROM traffic_visitors WHERE repo_id = ?1 AND day = ?2 AND kind = ?3")?;
        let mut seen = tx.prepare(
            "INSERT OR IGNORE INTO traffic_visitors(repo_id, day, kind, hash) SELECT ?1, ?2, ?3, ?4 WHERE EXISTS (SELECT 1 FROM repos WHERE id = ?1)",
        )?;
        for ((repo, day, kind), hashes) in groups {
            let mut room: i64 = MAX_VISITORS - have.query_row(params![repo, day, kind], |r| r.get::<_, i64>(0))?;
            for h in hashes {
                if room <= 0 {
                    break;
                }
                room -= seen.execute(params![repo, day, kind, h])? as i64;
            }
        }
    }
    if let Some(from) = prune {
        tx.execute("DELETE FROM traffic WHERE day < ?1", [from])?;
        tx.execute("DELETE FROM traffic_visitors WHERE day < ?1", [from])?;
    }
    tx.commit()
}

/// Write pending traffic every minute.
pub async fn flush_loop(st: AppState) {
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        st.traffic.flush(&st.db).await;
    }
}

#[derive(Deserialize)]
pub struct ViewReq {
    #[serde(default)]
    path: String,
    #[serde(default)]
    referrer: String,
}

/// The page inside the repository ("/", "/blob/main/README.md"), without query or fragment.
fn view_path(p: &str) -> Option<String> {
    let p = p.split(['?', '#']).next().unwrap_or("");
    (p.starts_with('/') && p.len() <= 200 && !p.chars().any(char::is_control)).then(|| p.to_string())
}

/// The referring site's host, unless it's this site or isn't a web address.
fn referrer_host(r: &str, own_url: &str) -> Option<String> {
    let host = |u: &str| -> Option<String> {
        let rest = u.strip_prefix("https://").or_else(|| u.strip_prefix("http://"))?;
        let auth = rest.split(['/', '?', '#']).next()?;
        let h = auth.rsplit('@').next()?.split(':').next()?.to_ascii_lowercase();
        (!h.is_empty() && h.len() <= 100 && h.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')).then_some(h)
    };
    let h = host(r)?;
    (host(own_url).as_deref() != Some(h.as_str())).then_some(h)
}

/// Count a page view (the web app sends one per repository page it shows).
pub async fn view(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    ip: ClientIp,
    Path((o, n)): Path<(String, String)>,
    Json(req): Json<ViewReq>,
) -> J<Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    st.traffic.record(r.id, "view", &ip.0, view_path(&req.path), referrer_host(&req.referrer, &st.cfg.url));
    super::ok()
}

/// Views and clones per day for the last two weeks, top referrers and pages: admins only.
pub async fn traffic(State(st): State<AppState>, Viewer(v): Viewer, Path((o, n)): Path<(String, String)>) -> J<Value> {
    let r = Repo::administrable(&st, v.as_ref(), &o, &n).await?;
    st.traffic.flush(&st.db).await;
    let (id, today) = (r.id, now() / 86400);
    let from = today - KEEP_DAYS + 1;
    st.db
        .call(move |c| {
            let mut days: HashMap<i64, [i64; 4]> = HashMap::new();
            let mut tops: HashMap<&str, HashMap<String, i64>> = HashMap::new();
            let mut s = c.prepare("SELECT day, kind, name, count FROM traffic WHERE repo_id = ?1 AND day >= ?2")?;
            let mut rows = s.query(params![id, from])?;
            while let Some(row) = rows.next()? {
                let (day, kind, name, n): (i64, String, String, i64) = (row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?);
                match (kind.as_str(), name.is_empty()) {
                    ("view", true) => days.entry(day).or_default()[0] += n,
                    ("clone", true) => days.entry(day).or_default()[2] += n,
                    ("path", false) => *tops.entry("paths").or_default().entry(name).or_default() += n,
                    ("ref", false) => *tops.entry("referrers").or_default().entry(name).or_default() += n,
                    _ => {}
                }
            }
            let mut s = c.prepare("SELECT day, kind, count(*) FROM traffic_visitors WHERE repo_id = ?1 AND day >= ?2 GROUP BY day, kind")?;
            let mut rows = s.query(params![id, from])?;
            while let Some(row) = rows.next()? {
                let (day, kind, n): (i64, String, i64) = (row.get(0)?, row.get(1)?, row.get(2)?);
                let slot = if kind == "view" { 1 } else { 3 };
                days.entry(day).or_default()[slot] = n;
            }
            let list: Vec<Value> = (from..=today)
                .map(|d| {
                    let x = days.get(&d).copied().unwrap_or_default();
                    json!({ "time": d * 86400, "views": x[0], "unique_views": x[1], "clones": x[2], "unique_clones": x[3] })
                })
                .collect();
            let top = |k: &str| -> Vec<Value> {
                let mut v: Vec<(String, i64)> = tops.get(k).map(|m| m.clone().into_iter().collect()).unwrap_or_default();
                v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
                v.into_iter().take(10).map(|(name, count)| json!({ "name": name, "count": count })).collect()
            };
            let total = |i: usize| days.values().map(|x| x[i]).sum::<i64>();
            Ok(Json(json!({
                "days": list, "views": total(0), "clones": total(2),
                "referrers": top("referrers"), "paths": top("paths"),
            })))
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(spec: &[(&str, &[&str])]) -> Vec<(String, Vec<String>)> {
        spec.iter().map(|(s, p)| (s.to_string(), p.iter().map(|x| x.to_string()).collect())).collect()
    }

    #[test]
    fn a_merge_opens_and_closes_a_lane() {
        // M merges S into B; both come from A.
        let r = rows(&[("M", &["B", "S"]), ("S", &["A"]), ("B", &["A"]), ("A", &[])]);
        let l = layout(&r, 0);
        assert_eq!(layout(&r, 2), l[2..], "a later page lays out the same");
        assert_eq!(l[0], Row { lane: 0, pass: vec![], up: vec![], down: vec![0, 1] });
        assert_eq!(l[1], Row { lane: 1, pass: vec![0], up: vec![1], down: vec![1] });
        assert_eq!(l[2], Row { lane: 0, pass: vec![1], up: vec![0], down: vec![0] });
        assert_eq!(l[3], Row { lane: 0, pass: vec![], up: vec![0, 1], down: vec![] });
    }

    #[test]
    fn history_tokens_pair_statuses_with_commits() {
        let c = |sha: &str, msg: &str| format!("{sha}\x1f\x1fa\x1fa@b\x1f1\x1fa\x1fa@b\x1f1\x1f{msg}\n");
        let s = format!("{}\0\nM\0b.txt\0{}\0\nR100\0a.txt\0b.txt\0{}\0\nA\0a.txt\0", c("3", "edit"), c("2", "move"), c("1", "add"));
        let v = parse_history(&s, "b.txt");
        let got: Vec<_> = v.iter().map(|x| (x.commit.sha.as_str(), x.status, x.path.as_str(), x.old_path.as_deref())).collect();
        assert_eq!(got, [("3", "modified", "b.txt", None), ("2", "renamed", "b.txt", Some("a.txt")), ("1", "added", "a.txt", None)]);
    }

    #[test]
    fn referrers_are_other_sites_only() {
        assert_eq!(referrer_host("https://News.example.com:443/x?y", "https://git.example.com").as_deref(), Some("news.example.com"));
        assert_eq!(referrer_host("https://git.example.com/a/b", "https://git.example.com"), None);
        assert_eq!(referrer_host("javascript:alert(1)", "https://git.example.com"), None);
        assert_eq!(referrer_host("https://u:p@evil<x>/", "https://git.example.com"), None);
        assert_eq!(view_path("/blob/main/a.rs?plain=1#L3").as_deref(), Some("/blob/main/a.rs"));
        assert_eq!(view_path("blob"), None);
    }
}
