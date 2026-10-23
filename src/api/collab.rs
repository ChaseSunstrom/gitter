//! Editing one file together in the web editor. Everyone in a session sends their changes as
//! operations on the text; the server puts them in one order, transforming each over the ones its
//! sender hadn't seen yet, and streams them (and where everyone's cursor is) to the others.
//! Sessions live in memory: a restart ends them, and each browser still has its text.
//! Positions count UTF-16 code units, as a textarea does.

use std::collections::{HashMap, VecDeque};
use std::convert::Infallible;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{Path, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::broadcast;

use super::J;
use crate::AppState;
use crate::auth::{Authed, User};
use crate::error::{AppError, R};
use crate::repo::Repo;
use crate::validate;

/// The longest text a session holds (code units), how many people can be in one, how many
/// sessions the server keeps, how long an idle one lasts, and how far back a change can start.
const MAX_TEXT: usize = 1 << 20;
const MAX_PEOPLE: usize = 20;
const MAX_SESSIONS: usize = 200;
const IDLE: Duration = Duration::from_secs(3600);
const HISTORY: usize = 1000;
/// The history's size limit (parts plus inserted units), and how many sessions one person starts.
const HISTORY_SIZE: usize = 4 << 20;
const MAX_STARTED: usize = 10;
/// How long someone who joined has to connect, and how often a stream re-checks its person may write.
const GRACE: Duration = Duration::from_secs(30);
const RECHECK: Duration = Duration::from_secs(2);

// ---------------------------------------------------------------- operations

/// One part of an operation: keep, insert or delete this many code units, left to right.
#[derive(Clone, Debug, PartialEq)]
pub enum Part {
    Retain(usize),
    Insert(Vec<u16>),
    Delete(usize),
}
pub type Op = Vec<Part>;

/// Builds an operation, merging neighbours of a kind and dropping empty parts.
#[derive(Default)]
struct Build(Op);

impl Build {
    fn retain(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        match self.0.last_mut() {
            Some(Part::Retain(m)) => *m += n,
            _ => self.0.push(Part::Retain(n)),
        }
    }
    fn insert(&mut self, s: &[u16]) {
        if s.is_empty() {
            return;
        }
        match self.0.last_mut() {
            Some(Part::Insert(t)) => t.extend_from_slice(s),
            _ => self.0.push(Part::Insert(s.to_vec())),
        }
    }
    fn delete(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        match self.0.last_mut() {
            Some(Part::Delete(m)) => *m += n,
            _ => self.0.push(Part::Delete(n)),
        }
    }
}

fn len(p: &Part) -> usize {
    match p {
        Part::Retain(n) | Part::Delete(n) => *n,
        Part::Insert(s) => s.len(),
    }
}

/// The length of text an operation applies to.
fn base_len(op: &Op) -> usize {
    op.iter().map(|p| if matches!(p, Part::Insert(_)) { 0 } else { len(p) }).sum()
}

/// `[3, "ab", -2]`: keep 3, insert "ab", delete 2.
pub fn parse(v: &Value) -> Result<Op, String> {
    let Value::Array(a) = v else { return Err("A change is a list like [3, \"ab\", -2]".into()) };
    let mut b = Build::default();
    for x in a {
        match x {
            Value::String(s) => b.insert(&s.encode_utf16().collect::<Vec<_>>()),
            Value::Number(n) => match n.as_i64() {
                // No count is longer than a text can be (nor adds up past what a number holds).
                Some(k) if k.unsigned_abs() as usize > MAX_TEXT => return Err("That change is for a longer text than can be edited together".into()),
                Some(k) if k > 0 => b.retain(k as usize),
                Some(k) if k < 0 => b.delete(k.unsigned_abs() as usize),
                _ => return Err("Each count is a whole number other than 0".into()),
            },
            _ => return Err("A change is a list of counts and text".into()),
        }
    }
    Ok(b.0)
}

pub fn to_json(op: &Op) -> Value {
    Value::Array(
        op.iter()
            .map(|p| match p {
                Part::Retain(n) => json!(n),
                Part::Delete(n) => json!(-(*n as i64)),
                Part::Insert(s) => json!(String::from_utf16_lossy(s)),
            })
            .collect(),
    )
}

pub fn apply(doc: &[u16], op: &Op) -> Result<Vec<u16>, String> {
    if base_len(op) != doc.len() {
        return Err(format!("That change is for a text of {} characters; this one has {}", base_len(op), doc.len()));
    }
    let (mut out, mut at) = (Vec::with_capacity(doc.len()), 0);
    for p in op {
        match p {
            Part::Retain(n) => {
                out.extend_from_slice(&doc[at..at + n]);
                at += n;
            }
            Part::Insert(s) => out.extend_from_slice(s),
            Part::Delete(n) => at += n,
        }
    }
    Ok(out)
}

/// The rest of `p` after its first `m` units, or the next part.
fn rest<'a>(p: &Part, m: usize, it: &mut impl Iterator<Item = &'a Part>) -> Option<Part> {
    match p {
        Part::Retain(n) if *n > m => Some(Part::Retain(n - m)),
        Part::Delete(n) if *n > m => Some(Part::Delete(n - m)),
        _ => it.next().cloned(),
    }
}

/// `a` and `b` were made on the same text: (a', b') with b' applying after a and a' after b,
/// both ending at the same text. Where both insert at one place, `a`'s insert comes first.
pub fn transform(a: &Op, b: &Op) -> Result<(Op, Op), String> {
    if base_len(a) != base_len(b) {
        return Err("Those two changes are for different texts".into());
    }
    let (mut a2, mut b2) = (Build::default(), Build::default());
    let (mut ia, mut ib) = (a.iter(), b.iter());
    let (mut x, mut y) = (ia.next().cloned(), ib.next().cloned());
    loop {
        match (&x, &y) {
            (None, None) => break,
            (Some(Part::Insert(s)), _) => {
                a2.insert(s);
                b2.retain(s.len());
                x = ia.next().cloned();
            }
            (_, Some(Part::Insert(s))) => {
                a2.retain(s.len());
                b2.insert(s);
                y = ib.next().cloned();
            }
            (Some(p), Some(q)) => {
                let m = len(p).min(len(q));
                match (p, q) {
                    (Part::Retain(_), Part::Retain(_)) => {
                        a2.retain(m);
                        b2.retain(m);
                    }
                    (Part::Delete(_), Part::Retain(_)) => a2.delete(m),
                    (Part::Retain(_), Part::Delete(_)) => b2.delete(m),
                    // Both deleted the same text.
                    _ => {}
                }
                let (p, q) = (p.clone(), q.clone());
                x = rest(&p, m, &mut ia);
                y = rest(&q, m, &mut ib);
            }
            _ => return Err("Those two changes are for different texts".into()),
        }
    }
    Ok((a2.0, b2.0))
}

/// Where position `at` of the text before `op` is after it (inserts there push it along).
pub fn moved(at: usize, op: &Op) -> usize {
    let (mut old, mut new) = (0, 0);
    for p in op {
        match p {
            Part::Retain(n) => {
                if at < old + n {
                    return new + (at - old);
                }
                old += n;
                new += n;
            }
            Part::Insert(s) => new += s.len(),
            Part::Delete(n) => {
                if at < old + n {
                    return new;
                }
                old += n;
            }
        }
    }
    new + at.saturating_sub(old)
}

// ---------------------------------------------------------------- sessions

struct Person {
    name: String,
    color: usize,
    cursor: (usize, usize),
    streams: usize,
    /// When they joined or last had a stream: someone who joined and never connected leaves after `GRACE`.
    seen: Instant,
}

struct Session {
    /// The server's data folder: one process can serve several (tests do).
    home: std::path::PathBuf,
    repo: i64,
    branch: String,
    path: String,
    /// Who started it (each person starts a few at most).
    starter: i64,
    /// The commit the text started from, and the file's blob there.
    base: Option<String>,
    base_blob: Option<String>,
    text: Vec<u16>,
    version: u64,
    /// The last changes, the newest at the back: change `version - history.len() + i + 1`.
    history: VecDeque<Op>,
    /// The history's size (parts plus inserted units), kept under `HISTORY_SIZE`.
    weight: usize,
    people: HashMap<i64, Person>,
    /// Everyone who typed, by user id: their commit signature (a no-reply address, as web
    /// commits use), credited even after they've left.
    authors: HashMap<i64, String>,
    events: broadcast::Sender<String>,
    touched: Instant,
}

fn weight(op: &Op) -> usize {
    op.iter().map(|p| if let Part::Insert(s) = p { s.len() + 1 } else { 1 }).sum()
}

/// The id git gives a file with this text.
fn blob_id(text: &[u16]) -> String {
    use sha1::Digest;
    let bytes = String::from_utf16_lossy(text).into_bytes();
    let mut h = sha1::Sha1::new();
    h.update(format!("blob {}\0", bytes.len()));
    h.update(&bytes);
    hex::encode(h.finalize())
}

impl Session {
    fn people_json(&self) -> Value {
        let mut v: Vec<&Person> = self.people.values().collect();
        v.sort_by_key(|p| p.color);
        Value::Array(v.iter().map(|p| json!({ "user": p.name, "color": p.color, "cursor": [p.cursor.0, p.cursor.1] })).collect())
    }

    fn state(&self, id: &str, me: i64) -> Value {
        let mut co: Vec<String> = self.authors.iter().filter(|(uid, _)| **uid != me).map(|(_, s)| format!("Co-authored-by: {s}")).collect();
        co.sort();
        json!({ "id": id, "branch": self.branch, "path": self.path, "base": self.base, "version": self.version,
            "text": String::from_utf16_lossy(&self.text), "people": self.people_json(), "co_authors": co })
    }

    fn send(&self, ev: Value) {
        let _ = self.events.send(ev.to_string());
    }

    /// Apply `op` (made on the current text) as the next version and move everyone's cursor with it.
    fn record(&mut self, op: Op, text: Vec<u16>) {
        self.text = text;
        self.version += 1;
        for p in self.people.values_mut() {
            p.cursor = (moved(p.cursor.0, &op), moved(p.cursor.1, &op));
        }
        self.weight += weight(&op);
        self.history.push_back(op);
        while self.history.len() > HISTORY || (self.weight > HISTORY_SIZE && self.history.len() > 1) {
            let old = self.history.pop_front().expect("not empty");
            self.weight -= weight(&old);
        }
        self.touched = Instant::now();
    }

    /// Untouched for `IDLE`, with nobody watching: it ends.
    fn idle(&self) -> bool {
        self.touched.elapsed() >= IDLE && self.people.values().all(|p| p.streams == 0)
    }

    /// People who joined but never opened a stream (or a cursor that came in after they left) go.
    fn prune(&mut self) {
        let gone: Vec<i64> = self.people.iter().filter(|(_, p)| p.streams == 0 && p.seen.elapsed() > GRACE).map(|(id, _)| *id).collect();
        for id in gone {
            if let Some(p) = self.people.remove(&id) {
                self.send(json!({ "type": "leave", "user": p.name }));
            }
        }
    }
}

type Shared = Arc<Mutex<Session>>;
static SESSIONS: LazyLock<Mutex<HashMap<String, Shared>>> = LazyLock::new(Default::default);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Session `id`, if it's repository `r`'s (callers check the person can write to it).
fn session(st: &AppState, r: &Repo, id: &str) -> R<Shared> {
    let s = lock(&SESSIONS).get(id).cloned().ok_or_else(AppError::not_found)?;
    if lock(&s).repo != r.id || lock(&s).home != st.cfg.data_dir {
        return Err(AppError::not_found());
    }
    Ok(s)
}

fn enter(s: &mut Session, u: &User) -> R<()> {
    s.prune();
    if let Some(p) = s.people.get_mut(&u.id) {
        p.seen = Instant::now();
    } else {
        if s.people.len() >= MAX_PEOPLE {
            return Err(AppError::conflict(format!("{MAX_PEOPLE} people are editing this file already")));
        }
        let color = (0..).find(|c| !s.people.values().any(|p| p.color == *c)).unwrap_or(0);
        s.people.insert(u.id, Person { name: u.username.clone(), color, cursor: (0, 0), streams: 0, seen: Instant::now() });
        s.send(json!({ "type": "presence", "user": u.username, "color": color, "cursor": [0, 0] }));
    }
    s.touched = Instant::now();
    Ok(())
}

/// The blob id of `path` at commit `rev`, if it's a regular file there.
async fn file_at(st: &AppState, r: &Repo, rev: &str, path: &str) -> R<Option<String>> {
    let out = st.git.ok(&r.path, &["ls-tree", "-z", rev, "--", path]).await?;
    // Entries are "mode type oid\tpath", each ended by a NUL.
    for e in out.split(|b| *b == 0) {
        let e = String::from_utf8_lossy(e);
        if let Some((meta, p)) = e.split_once('\t')
            && p == path
        {
            let f: Vec<&str> = meta.split_whitespace().collect();
            if f.len() != 3 || f[1] != "blob" || f[0] == "120000" {
                return Err(AppError::bad(format!("{path} isn't a file that can be edited here")));
            }
            return Ok(Some(f[2].to_string()));
        }
    }
    Ok(None)
}

/// The text of blob `oid`, if it can be edited together.
async fn text_of(st: &AppState, r: &Repo, oid: &str, path: &str) -> R<Vec<u16>> {
    let bytes = st.git.blob(&r.path, oid).await?;
    let s = String::from_utf8(bytes).map_err(|_| AppError::bad(format!("{path} isn't text")))?;
    let text: Vec<u16> = s.encode_utf16().collect();
    if text.len() > MAX_TEXT {
        return Err(AppError::bad(format!("{path} is too big to edit together")));
    }
    Ok(text)
}

/// The branch moved on since the session's base (someone pushed): carry on from its head when
/// this file is there as it was, or as the session has it; take the new text when nobody has
/// typed since. Otherwise the file was changed outside, and a commit from the session is refused.
async fn refresh(st: &AppState, r: &Repo, shared: &Shared) -> R<()> {
    let (branch, path, base) = {
        let s = lock(shared);
        (s.branch.clone(), s.path.clone(), s.base.clone())
    };
    let Some(head) = st.git.resolve(&r.path, &format!("refs/heads/{branch}")).await? else { return Ok(()) };
    if base.as_deref() == Some(head.as_str()) {
        return Ok(());
    }
    let Some(blob) = file_at(st, r, &head, &path).await.ok().flatten() else { return Ok(()) };
    // Nobody typed since the base, the session has just what's there, the file is untouched there.
    let (untyped, same, untouched) = {
        let s = lock(shared);
        let now = blob_id(&s.text);
        (s.base_blob.as_deref() == Some(now.as_str()), now == blob, s.base_blob.as_deref() == Some(blob.as_str()))
    };
    let new = match untyped && !same && !untouched {
        true => match text_of(st, r, &blob, &path).await {
            Ok(t) => Some(t),
            Err(_) => return Ok(()),
        },
        false => None,
    };
    let mut s = lock(shared);
    if s.base != base || (untyped && s.base_blob.as_deref() != Some(blob_id(&s.text).as_str())) {
        return Ok(()); // moved, or typed in, meanwhile
    }
    if let Some(text) = new {
        let mut b = Build::default();
        b.delete(s.text.len());
        b.insert(&text);
        let op = b.0;
        s.record(op.clone(), text);
        let version = s.version;
        s.send(json!({ "type": "op", "version": version, "op": to_json(&op), "client": "", "user": "" }));
    } else if !same && !untouched {
        return Ok(());
    }
    s.base = Some(head.clone());
    s.base_blob = Some(blob);
    s.send(json!({ "type": "base", "sha": head, "user": "" }));
    Ok(())
}

#[derive(Deserialize)]
pub struct JoinReq {
    branch: String,
    path: String,
}

fn find(all: &HashMap<String, Shared>, st: &AppState, repo: i64, branch: &str, path: &str) -> Option<(String, Shared)> {
    all.iter()
        .find(|(_, s)| {
            let s = lock(s);
            s.home == st.cfg.data_dir && s.repo == repo && s.branch == branch && s.path == path
        })
        .map(|(id, s)| (id.clone(), s.clone()))
}

/// Join the session for a file on a branch, or start one from the branch's version of it.
pub async fn join(State(st): State<AppState>, Authed(u): Authed, Path((o, n)): Path<(String, String)>, Json(req): Json<JoinReq>) -> J<Value> {
    let r = Repo::writable(&st, Some(&u), &o, &n).await?;
    crate::mirrors::refuse_writes(&r)?;
    let path = req.path.trim_matches('/').to_string();
    if !validate::ref_name(&req.branch) || !validate::write_path(&path) {
        return Err(AppError::bad("Pick a branch and a file"));
    }
    let found = {
        let mut all = lock(&SESSIONS);
        // Idle sessions nobody is watching end.
        all.retain(|_, s| !lock(s).idle());
        find(&all, &st, r.id, &req.branch, &path)
    };
    let (id, s) = match found {
        Some((id, s)) => {
            refresh(&st, &r, &s).await?;
            (id, s)
        }
        None => {
            let none = || AppError::new(axum::http::StatusCode::NOT_FOUND, format!("There's no {} on {}", path, req.branch));
            let base = st.git.resolve(&r.path, &format!("refs/heads/{}", req.branch)).await?.ok_or_else(none)?;
            let blob = file_at(&st, &r, &base, &path).await?.ok_or_else(none)?;
            let text = text_of(&st, &r, &blob, &path).await?;
            let mut all = lock(&SESSIONS);
            // Someone else may have started it meanwhile.
            if let Some(f) = find(&all, &st, r.id, &req.branch, &path) {
                f
            } else {
                if all.len() >= MAX_SESSIONS {
                    return Err(AppError::conflict("Too many files are being edited together right now. Try again later."));
                }
                if all.values().filter(|s| lock(s).starter == u.id && lock(s).home == st.cfg.data_dir).count() >= MAX_STARTED {
                    return Err(AppError::conflict(format!("You have {MAX_STARTED} files open to edit together already; close one first")));
                }
                let id = crate::auth::new_secret("").0;
                let s = Arc::new(Mutex::new(Session {
                    home: st.cfg.data_dir.clone(),
                    repo: r.id,
                    branch: req.branch.clone(),
                    path: path.clone(),
                    starter: u.id,
                    base: Some(base),
                    base_blob: Some(blob),
                    text,
                    version: 0,
                    history: VecDeque::new(),
                    weight: 0,
                    people: HashMap::new(),
                    authors: HashMap::new(),
                    events: broadcast::channel(256).0,
                    touched: Instant::now(),
                }));
                all.insert(id.clone(), s.clone());
                (id, s)
            }
        }
    };
    let mut s = lock(&s);
    enter(&mut s, &u)?;
    Ok(Json(s.state(&id, u.id)))
}

/// The whole state, carried on to the branch's head first if that can be done without losing anything.
pub async fn show(State(st): State<AppState>, Authed(u): Authed, Path((o, n, id)): Path<(String, String, String)>) -> J<Value> {
    let r = Repo::writable(&st, Some(&u), &o, &n).await?;
    let s = session(&st, &r, &id)?;
    refresh(&st, &r, &s).await?;
    let mut s = lock(&s);
    s.prune();
    Ok(Json(s.state(&id, u.id)))
}

#[derive(Deserialize)]
pub struct ChangeReq {
    /// This browser tab, so it knows its own change when it comes back.
    client: String,
    /// The version the change was made on.
    revision: u64,
    op: Value,
}

/// A change made on version `revision`: transformed over the ones since, applied, and sent to everyone.
pub async fn change(State(st): State<AppState>, Authed(u): Authed, Path((o, n, id)): Path<(String, String, String)>, Json(req): Json<ChangeReq>) -> J<Value> {
    let r = Repo::writable(&st, Some(&u), &o, &n).await?;
    // Typing sends many of these: a budget of their own, not the write allowance.
    if let Err(wait) = st.limiter.check(&format!("collab:{}", u.id), 900, 60) {
        return Err(AppError::slow_down("You're typing faster than the server keeps up with. Wait a moment.", wait));
    }
    let s = session(&st, &r, &id)?;
    let mut op = parse(&req.op).map_err(AppError::bad)?;
    let client: String = req.client.chars().take(64).collect();
    let mut s = lock(&s);
    if req.revision > s.version {
        return Err(AppError::bad("That change is for a version this session hasn't reached"));
    }
    let behind = (s.version - req.revision) as usize;
    if behind > s.history.len() {
        return Err(AppError::conflict("This session moved on too far since; reload to catch up"));
    }
    let from = s.history.len() - behind;
    for h in s.history.range(from..) {
        op = transform(&op, h).map_err(AppError::bad)?.0;
    }
    let text = apply(&s.text, &op).map_err(AppError::bad)?;
    if text.len() > MAX_TEXT {
        return Err(AppError::bad("The text would be too big to edit together"));
    }
    s.record(op.clone(), text);
    let (name, email) = u.signature();
    s.authors.insert(u.id, format!("{name} <{email}>"));
    let version = s.version;
    s.send(json!({ "type": "op", "version": version, "op": to_json(&op), "client": client, "user": u.username }));
    Ok(Json(json!({ "version": version })))
}

#[derive(Deserialize)]
pub struct PresenceReq {
    cursor: (usize, usize),
}

/// Where your cursor (or selection) is, on the current version.
pub async fn presence(State(st): State<AppState>, Authed(u): Authed, Path((o, n, id)): Path<(String, String, String)>, Json(req): Json<PresenceReq>) -> J<Value> {
    let r = Repo::writable(&st, Some(&u), &o, &n).await?;
    if let Err(wait) = st.limiter.check(&format!("collab-cursor:{}", u.id), 600, 60) {
        return Err(AppError::slow_down("Your cursor moves faster than the server keeps up with. Wait a moment.", wait));
    }
    let s = session(&st, &r, &id)?;
    let mut s = lock(&s);
    let max = s.text.len();
    let cursor = (req.cursor.0.min(max), req.cursor.1.min(max));
    let version = s.version;
    // Only someone in the session has a cursor in it.
    let p = s.people.get_mut(&u.id).ok_or_else(AppError::not_found)?;
    p.cursor = cursor;
    let color = p.color;
    s.send(json!({ "type": "presence", "user": u.username, "color": color, "cursor": [cursor.0, cursor.1], "version": version }));
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct CommittedReq {
    sha: String,
}

/// Someone committed the text on top of the session's base: the session carries on from that commit.
pub async fn committed(State(st): State<AppState>, Authed(u): Authed, Path((o, n, id)): Path<(String, String, String)>, Json(req): Json<CommittedReq>) -> J<Value> {
    let r = Repo::writable(&st, Some(&u), &o, &n).await?;
    let s = session(&st, &r, &id)?;
    if !validate::sha(&req.sha) {
        return Err(AppError::bad("That isn't a commit"));
    }
    let (branch, path, base) = {
        let s = lock(&s);
        (s.branch.clone(), s.path.clone(), s.base.clone())
    };
    let head = st.git.resolve(&r.path, &format!("refs/heads/{branch}")).await?;
    let parent = st.git.resolve(&r.path, &format!("{}^", req.sha)).await?;
    if head.as_deref() != Some(req.sha.as_str()) || parent != base {
        return Err(AppError::conflict("That isn't the branch's latest commit, made on top of this session's"));
    }
    let blob = file_at(&st, &r, &req.sha, &path).await?;
    let mut s = lock(&s);
    if s.base != base {
        return Err(AppError::conflict("Someone else's commit got there first"));
    }
    s.base = head;
    s.base_blob = blob;
    s.send(json!({ "type": "base", "sha": req.sha, "user": u.username }));
    Ok(Json(json!({ "ok": true })))
}

/// Counts a browser's stream while it's open; the person leaves when their last one closes.
struct Watching(Shared, i64, String);

impl Drop for Watching {
    fn drop(&mut self) {
        let mut s = lock(&self.0);
        let gone = s.people.get_mut(&self.1).is_some_and(|p| {
            p.streams = p.streams.saturating_sub(1);
            p.seen = Instant::now();
            p.streams == 0
        });
        if gone {
            s.people.remove(&self.1);
            s.send(json!({ "type": "leave", "user": self.2 }));
        }
    }
}

/// The session as it happens: `init` with the whole state, then `op`, `presence`, `base` and
/// `leave` events; `resync` when this stream fell too far behind (load it again). The stream
/// ends once its person can't write to the repository any more (checked every `RECHECK`).
pub async fn events(
    State(st): State<AppState>,
    Authed(u): Authed,
    Path((o, n, id)): Path<(String, String, String)>,
) -> R<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>> {
    let r = Repo::writable(&st, Some(&u), &o, &n).await?;
    let shared = session(&st, &r, &id)?;
    let (init, rx) = {
        let mut s = lock(&shared);
        enter(&mut s, &u)?;
        if let Some(p) = s.people.get_mut(&u.id) {
            p.streams += 1;
        }
        (s.state(&id, u.id).to_string(), s.events.subscribe())
    };
    let guard = Watching(shared, u.id, u.username.clone());
    let first = futures_util::stream::once(async move { Ok(Event::default().event("init").data(init)) });
    let who = (st, u, o, n, Instant::now());
    let rest = futures_util::stream::unfold((rx, guard, who, false), |(mut rx, guard, mut who, done)| async move {
        if done {
            return None;
        }
        let ev = match rx.recv().await {
            Ok(ev) => ev,
            Err(broadcast::error::RecvError::Lagged(_)) => return Some((Ok(Event::default().event("resync").data("{}")), (rx, guard, who, true))),
            Err(broadcast::error::RecvError::Closed) => return None,
        };
        if who.4.elapsed() >= RECHECK {
            let (st, u, o, n, _) = &who;
            Repo::writable(st, Some(u), o, n).await.ok()?;
            who.4 = Instant::now();
        }
        Some((Ok(Event::default().event("change").data(ev)), (rx, guard, who, false)))
    });
    Ok(Sse::new(futures_util::StreamExt::chain(first, rest)).keep_alive(KeepAlive::default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    /// A small deterministic random source.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self, n: usize) -> usize {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 % n.max(1) as u64) as usize
        }
    }

    fn random_op(doc: &[u16], r: &mut Rng) -> Op {
        let mut b = Build::default();
        let mut at = 0;
        while at < doc.len() {
            let n = 1 + r.next((doc.len() - at).min(4));
            match r.next(3) {
                0 => b.retain(n),
                1 => b.delete(n),
                _ => {
                    b.insert(&u(["x", "yz", "é", "😀"][r.next(4)]));
                    b.retain(n);
                }
            }
            at += n;
        }
        if r.next(2) == 0 {
            b.insert(&u("!"));
        }
        b.0
    }

    #[test]
    fn transformed_changes_meet_at_the_same_text() {
        let mut r = Rng(0x9e3779b97f4a7c15);
        for _ in 0..3000 {
            let doc = u(&"abcdefghij"[..r.next(11)]);
            let (a, b) = (random_op(&doc, &mut r), random_op(&doc, &mut r));
            let (a2, b2) = transform(&a, &b).unwrap();
            let via_a = apply(&apply(&doc, &a).unwrap(), &b2).unwrap();
            let via_b = apply(&apply(&doc, &b).unwrap(), &a2).unwrap();
            assert_eq!(via_a, via_b, "{a:?} {b:?}");
        }
    }

    fn blank(text: &str) -> Session {
        Session {
            home: "/x".into(),
            repo: 1,
            branch: "main".into(),
            path: "a".into(),
            starter: 1,
            base: None,
            base_blob: Some(blob_id(&u(text))),
            text: u(text),
            version: 0,
            history: VecDeque::new(),
            weight: 0,
            people: HashMap::new(),
            authors: HashMap::new(),
            events: broadcast::channel(4).0,
            touched: Instant::now(),
        }
    }

    fn person(color: usize, streams: usize, ago: u64) -> Person {
        Person { name: format!("p{color}"), color, cursor: (0, 0), streams, seen: Instant::now() - Duration::from_secs(ago) }
    }

    #[test]
    fn history_is_kept_under_its_size_and_people_who_never_connected_go() {
        let mut s = blank("");
        let big = vec![Part::Insert(vec![b'x' as u16; 1 << 20])];
        for _ in 0..10 {
            let text = apply(&[], &big).unwrap();
            s.text.clear();
            s.record(big.clone(), text);
            s.text.clear();
        }
        assert!(s.weight <= HISTORY_SIZE && s.history.len() == 3, "{} {}", s.weight, s.history.len());
        assert_eq!(s.weight, s.history.iter().map(weight).sum::<usize>());
        // Joined long ago and never connected: gone. Connected, or just joined: still here.
        s.people.insert(1, person(0, 0, 60));
        s.people.insert(2, person(1, 1, 60));
        s.people.insert(3, person(2, 0, 1));
        s.prune();
        let mut left: Vec<i64> = s.people.keys().copied().collect();
        left.sort();
        assert_eq!(left, vec![2, 3]);
        // Idle: an hour untouched and nobody watching.
        assert!(!s.idle());
        s.touched = Instant::now() - IDLE;
        assert!(!s.idle(), "someone is watching");
        s.people.remove(&2);
        assert!(s.idle());
        // The twenty-first person is turned away.
        for i in 0..17 {
            s.people.insert(10 + i, person(3 + i as usize, 1, 0));
        }
        s.people.insert(2, person(1, 1, 0));
        assert_eq!(s.people.len(), MAX_PEOPLE - 1);
        let mut u = User { id: 99, username: "x".into(), display_name: String::new(), email: String::new(), is_admin: false, limits: None };
        assert!(enter(&mut s, &u).is_ok());
        u.id = 100;
        assert!(enter(&mut s, &u).is_err());
        assert_eq!(blob_id(&self::u("hello\n")), "ce013625030ba8dba906f756967f9e9ca394464a");
    }

    #[test]
    fn inserts_at_one_place_keep_the_first_ones_first_and_cursors_follow() {
        let doc = u("ab");
        let (a, b) = (parse(&json!([1, "X", 1])).unwrap(), parse(&json!([1, "Y", 1])).unwrap());
        let (a2, _) = transform(&a, &b).unwrap();
        assert_eq!(String::from_utf16_lossy(&apply(&apply(&doc, &b).unwrap(), &a2).unwrap()), "aXYb");
        assert_eq!(moved(1, &parse(&json!([1, "XY", 1])).unwrap()), 3);
        assert_eq!(moved(1, &parse(&json!([-2])).unwrap()), 0);
        assert_eq!(moved(2, &parse(&json!(["Z", 2])).unwrap()), 3);
        assert!(apply(&doc, &parse(&json!([3])).unwrap()).is_err());
        assert!(parse(&json!([0])).is_err() && parse(&json!({})).is_err());
        assert_eq!(to_json(&parse(&json!([2, 1, "a", "b", -1, -1])).unwrap()), json!([3, "ab", -2]));
    }
}
