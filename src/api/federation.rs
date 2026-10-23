//! Working with other Gitter servers (see crate::federation for the signatures).
//!
//! Here, for people: fork a public repository from another server into an empty one
//! (`POST {repo}/federation/fork`), and open a pull request there from one of its branches
//! (`POST {repo}/federation/pulls`); pushes to that branch then update it there.
//!
//! Between servers, signed: `POST /api/federation/pulls` opens a pull request here from a fork
//! on the signing server, and `POST /api/federation/pulls/update` makes it take the branch again.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use ed25519_dalek::VerifyingKey;
use rusqlite::OptionalExtension;
use serde::Deserialize;
use serde_json::{Value, json};

use super::J;
use crate::AppState;
use crate::auth::{Authed, User};
use crate::db::now;
use crate::error::{AppError, R};
use crate::federation::{self, HEADER, SKEW};
use crate::repo::Repo;
use crate::validate;

/// How long another server's published key is trusted before it's fetched again.
const KEY_TTL: Duration = Duration::from_secs(3600);
/// Open pull requests one other server may have in a repository here.
const MAX_OPEN: i64 = 10;

/// This server's address, as other servers know it.
pub fn me(st: &AppState) -> String {
    federation::origin(&st.cfg.url).unwrap_or_else(|| st.cfg.url.trim_end_matches('/').to_string())
}

fn off() -> AppError {
    AppError::new(StatusCode::FORBIDDEN, "This server doesn't work with other servers (an admin can turn federation on)")
}

fn unsigned(why: &str) -> AppError {
    AppError::new(StatusCode::UNAUTHORIZED, why)
}

/// GET /.well-known/gitter: who this server is, and its public key.
pub async fn well_known(State(st): State<AppState>) -> R<Json<Value>> {
    let k = federation::key(&st.cfg.data_dir).map_err(AppError::internal)?;
    let s = st.settings.get();
    Ok(Json(json!({ "software": "gitter", "version": env!("CARGO_PKG_VERSION"), "url": me(&st), "name": s.instance_name,
        "federation": s.federation, "public_key": federation::public_key(&k) })))
}

// ---------------------------------------------------------------- other servers

#[derive(Clone)]
struct Server {
    key: VerifyingKey,
    /// Whether it works with other servers (as it said when last asked).
    federation: bool,
}

static SERVERS: LazyLock<Mutex<HashMap<String, (Instant, Server)>>> = LazyLock::new(Default::default);
/// Signatures seen lately (a request is accepted once).
static SEEN: LazyLock<Mutex<HashMap<String, i64>>> = LazyLock::new(Default::default);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// GET `url` from another server (JSON, small), through the client that keeps to public addresses.
async fn get_json(st: &AppState, url: String) -> R<Value> {
    let private = st.settings.get().federation_private;
    let shown = url.clone();
    let reply = tokio::task::spawn_blocking(move || crate::net::fetch(&url, &[("accept", "application/json".into())], None, private, 1 << 20))
        .await
        .map_err(AppError::internal)?
        .map_err(|e| AppError::new(StatusCode::BAD_GATEWAY, format!("{shown}: {e}")))?;
    if reply.status == 404 {
        return Err(AppError::not_found());
    }
    if reply.status != 200 {
        return Err(AppError::new(StatusCode::BAD_GATEWAY, format!("{shown} answered {}", reply.status)));
    }
    serde_json::from_slice(&reply.body).map_err(|_| AppError::new(StatusCode::BAD_GATEWAY, format!("{shown} didn't answer with JSON")))
}

/// Server `origin`'s identity, from its /.well-known/gitter (kept for a while unless `fresh`).
async fn identity(st: &AppState, origin: &str, fresh: bool) -> R<Server> {
    if !fresh && let Some((_, s)) = lock(&SERVERS).get(origin).filter(|(at, _)| at.elapsed() < KEY_TTL) {
        return Ok(s.clone());
    }
    // Any request can name a server to look up: a budget for all of them together.
    if let Err(wait) = st.limiter.check("federation-lookups", 120, 60) {
        return Err(AppError::slow_down("Too many other servers looked up just now", wait));
    }
    let v = get_json(st, format!("{origin}/.well-known/gitter")).await?;
    let says = v["url"].as_str().and_then(federation::origin);
    let key = v["public_key"].as_str().and_then(federation::parse_key);
    let (Some(says), Some(key)) = (says, key) else {
        return Err(AppError::bad(format!("{origin} isn't a Gitter server")));
    };
    if says != origin || v["software"] != "gitter" {
        return Err(AppError::bad(format!("{origin} says it's {says}")));
    }
    let s = Server { key, federation: v["federation"].as_bool().unwrap_or(false) };
    lock(&SERVERS).insert(origin.to_string(), (Instant::now(), s.clone()));
    Ok(s)
}

/// The server that signed this request, once its signature checks out: made just now (within
/// `SKEW`), not seen before, for this server, over this method, path and body, with the key that
/// server publishes. Until it checks out, the caller is limited by address, not by the server it
/// names, and isn't told what went wrong reaching that server.
async fn signer(st: &AppState, ip: &str, headers: &HeaderMap, path: &str, body: &[u8]) -> R<String> {
    if !st.settings.get().federation {
        return Err(off());
    }
    if let Err(wait) = st.limiter.check(&format!("federation-ip:{}", crate::auth::ip_bucket(ip)), 300, 60) {
        return Err(AppError::slow_down("Too many requests between servers from your address", wait));
    }
    let h = headers.get(HEADER).and_then(|v| v.to_str().ok()).and_then(federation::parse_header).ok_or_else(|| unsigned("Requests between servers are signed"))?;
    let t = now();
    if !federation::fresh(h.ts, t) {
        return Err(unsigned("That request's time is too far from this server's"));
    }
    lock(&SEEN).retain(|_, at| t.abs_diff(*at) <= 2 * SKEW as u64);
    let cant = |e: AppError| {
        eprintln!("federation: checking a request from {}: {}", h.server, e.msg);
        unsigned("Couldn't check the signature")
    };
    let mut s = identity(st, &h.server, false).await.map_err(cant)?;
    let here = me(st);
    if !federation::verify(&s.key, &h, &here, "POST", path, body) {
        // Its key may have changed since (asked again at most once a minute).
        if st.limiter.check(&format!("federation-refetch:{}", h.server), 1, 60).is_err() {
            return Err(unsigned("The signature doesn't match that server's key"));
        }
        s = identity(st, &h.server, true).await.map_err(cant)?;
        if !federation::verify(&s.key, &h, &here, "POST", path, body) {
            return Err(unsigned("The signature doesn't match that server's key"));
        }
    }
    if lock(&SEEN).insert(h.raw.clone(), h.ts).is_some() {
        return Err(unsigned("That request was already made"));
    }
    if let Err(wait) = st.limiter.check(&format!("federation:{}", h.server), 120, 60) {
        return Err(AppError::slow_down("Too many requests from that server", wait));
    }
    Ok(h.server)
}

/// POST `body` to `path` on server `origin`, signed as this server. The reply's JSON, or its error.
pub async fn call(st: &AppState, origin: &str, path: &str, body: &Value) -> R<Value> {
    let k = federation::key(&st.cfg.data_dir).map_err(AppError::internal)?;
    let bytes = body.to_string().into_bytes();
    let sig = federation::sign(&k, &me(st), origin, "POST", path, now(), &bytes);
    let (url, private) = (format!("{origin}{path}"), st.settings.get().federation_private);
    let reply = tokio::task::spawn_blocking(move || {
        crate::net::fetch(&url, &[("content-type", "application/json".into()), (HEADER, sig)], Some(&bytes), private, 1 << 20)
    })
    .await
    .map_err(AppError::internal)?
    .map_err(|e| AppError::new(StatusCode::BAD_GATEWAY, format!("{}: {e}", federation::host(origin))))?;
    let v: Value = serde_json::from_slice(&reply.body).unwrap_or(Value::Null);
    if reply.status != 200 {
        let why = v["error"].as_str().unwrap_or("no reason given");
        let code = StatusCode::from_u16(reply.status).ok().filter(|c| c.is_client_error()).unwrap_or(StatusCode::BAD_GATEWAY);
        return Err(AppError::new(code, format!("{} said: {why}", federation::host(origin))));
    }
    Ok(v)
}

// ---------------------------------------------------------------- forks of another server's repositories

/// (server, owner, name) of `https://a.example/alice/notes` (or with .git).
fn parse_repo_url(url: &str) -> Option<(String, String, String)> {
    let url = url.trim().trim_end_matches('/');
    let (scheme, rest) = url.split_once("://")?;
    let (host, path) = rest.split_once('/')?;
    let server = federation::origin(&format!("{scheme}://{host}"))?;
    let (owner, name) = path.split_once('/')?;
    let name = name.strip_suffix(".git").unwrap_or(name);
    (validate::username(owner) && validate::repo_name(name)).then(|| (server, owner.to_string(), name.to_string()))
}

/// Where repository `repo_id` was forked from on another server, if it was.
pub async fn remote_fork(st: &AppState, repo_id: i64) -> R<Option<(String, String, String)>> {
    st.db
        .call(move |c| Ok(c.query_row("SELECT server, owner, name FROM remote_forks WHERE repo_id = ?1", [repo_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).optional()?))
        .await
}

/// For a repository's page: where it was forked from on another server.
pub async fn remote_fork_json(st: &AppState, repo_id: i64) -> R<Value> {
    Ok(match remote_fork(st, repo_id).await? {
        Some((server, owner, name)) => json!({ "server": server, "host": federation::host(&server), "owner": owner, "name": name, "url": format!("{server}/{owner}/{name}") }),
        None => Value::Null,
    })
}

#[derive(Deserialize)]
pub struct ForkReq {
    url: String,
}

/// The public repository at `url` on another Gitter server that works with this one: (server, owner, name).
async fn look_up(st: &AppState, url: &str) -> R<(String, String, String)> {
    if !st.settings.get().federation {
        return Err(off());
    }
    let (server, owner, name) = parse_repo_url(url).ok_or_else(|| AppError::bad("Give the repository's address, like https://git.example.com/owner/name"))?;
    if server == me(st) {
        return Err(AppError::bad("That repository is on this server: fork it from its page"));
    }
    // Asked afresh: whether it works with other servers may have changed.
    if !identity(st, &server, true).await?.federation {
        return Err(AppError::new(StatusCode::FORBIDDEN, format!("{} doesn't work with other servers", federation::host(&server))));
    }
    let missing = || AppError::new(StatusCode::NOT_FOUND, format!("There's no public {owner}/{name} on {}", federation::host(&server)));
    let theirs = get_json(st, format!("{server}/api/repos/{owner}/{name}")).await.map_err(|e| if e.status == StatusCode::NOT_FOUND { missing() } else { e })?;
    if theirs["private"].as_bool() != Some(false) {
        return Err(missing());
    }
    Ok((server, owner, name))
}

/// Whether `url` can be forked from here (the new repository page asks before making one).
pub async fn check(State(st): State<AppState>, Authed(_u): Authed, Json(req): Json<ForkReq>) -> J<Value> {
    let (server, owner, name) = look_up(&st, &req.url).await?;
    Ok(Json(json!({ "server": server, "host": federation::host(&server), "owner": owner, "name": name })))
}

/// Make this empty repository a fork of a public one on another Gitter server: its branches and
/// tags are copied in (as a one-time pull mirror), and it remembers where it came from.
pub async fn fork(State(st): State<AppState>, Authed(u): Authed, Path((o, n)): Path<(String, String)>, Json(req): Json<ForkReq>) -> J<Value> {
    let r = Repo::administrable(&st, Some(&u), &o, &n).await?;
    if r.mirror || !st.git.refs(&r.path).await?.is_empty() {
        return Err(AppError::conflict("Only an empty repository becomes a fork of another server's"));
    }
    let (server, owner, name) = look_up(&st, &req.url).await?;
    let (rid, clone, private) = (r.id, format!("{server}/{owner}/{name}.git"), st.settings.get().federation_private);
    let (sv, ow, nm) = (server.clone(), owner.clone(), name.clone());
    st.db
        .call(move |c| {
            let tx = c.transaction()?;
            let t = now();
            tx.execute(
                "INSERT INTO mirrors(repo_id, direction, url, username, secret, allow_private, interval_secs, next_at, created_at, once) VALUES (?1, 'pull', ?2, '', NULL, ?3, 28800, ?4, ?4, 1)",
                rusqlite::params![rid, clone, private, t],
            )?;
            // An earlier fork whose copy came to nothing (the repository is empty again) is replaced.
            tx.execute("INSERT OR REPLACE INTO remote_forks(repo_id, server, owner, name) VALUES (?1, ?2, ?3, ?4)", rusqlite::params![rid, sv, ow, nm])?;
            tx.commit()?;
            Ok(())
        })
        .await?;
    Ok(Json(json!({ "server": server, "owner": owner, "name": name })))
}

#[derive(Deserialize)]
pub struct ProposeReq {
    branch: String,
    base: Option<String>,
    title: String,
    #[serde(default)]
    body: String,
}

/// Open a pull request on the server this repository was forked from, from one of its branches.
pub async fn propose(State(st): State<AppState>, Authed(u): Authed, Path((o, n)): Path<(String, String)>, Json(req): Json<ProposeReq>) -> J<Value> {
    let r = Repo::writable(&st, Some(&u), &o, &n).await?;
    if !st.settings.get().federation {
        return Err(off());
    }
    let (server, owner, name) = remote_fork(&st, r.id).await?.ok_or_else(|| AppError::bad("This repository isn't a fork of one on another server"))?;
    if r.private {
        // The other server fetches the branch from here, as anyone would.
        return Err(AppError::bad("Make the fork public first: the other server copies the branch from it"));
    }
    if !validate::ref_name(&req.branch) || st.git.resolve(&r.path, &format!("refs/heads/{}", req.branch)).await?.is_none() {
        return Err(AppError::bad("Pick a branch of this repository"));
    }
    let (display, _) = u.signature();
    let ask = json!({ "repo": format!("{owner}/{name}"), "base": req.base, "fork": r.full_name(), "branch": req.branch,
        "title": req.title, "body": req.body, "author": { "username": u.username, "name": display } });
    let v = call(&st, &server, "/api/federation/pulls", &ask).await?;
    let number = v["number"].as_i64().ok_or_else(|| AppError::new(StatusCode::BAD_GATEWAY, "The other server didn't say which pull request"))?;
    let url = format!("{server}/{owner}/{name}/pull/{number}");
    let (rid, branch, url2, fork) = (r.id, req.branch.clone(), url.clone(), r.full_name());
    st.db
        .call(move |c| {
            Ok(c.execute(
                "INSERT OR IGNORE INTO outgoing_pulls(repo_id, fork, branch, number, url, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![rid, fork, branch, number, url2, now()],
            )?)
        })
        .await?;
    Ok(Json(json!({ "number": number, "url": url })))
}

/// Pull requests this repository's branches opened on other servers.
pub async fn outgoing(State(st): State<AppState>, Authed(u): Authed, Path((o, n)): Path<(String, String)>) -> J<Value> {
    let r = Repo::readable(&st, Some(&u), &o, &n).await?;
    let rid = r.id;
    let list: Vec<Value> = st
        .db
        .call(move |c| {
            let mut q = c.prepare("SELECT branch, number, url, created_at FROM outgoing_pulls WHERE repo_id = ?1 ORDER BY id DESC")?;
            Ok(q.query_map([rid], |r| Ok(json!({ "branch": r.get::<_, String>(0)?, "number": r.get::<_, i64>(1)?, "url": r.get::<_, String>(2)?, "created_at": r.get::<_, i64>(3)? })))?
                .collect::<Result<_, _>>()?)
        })
        .await?;
    Ok(Json(json!({ "pulls": list, "fork_of": remote_fork_json(&st, r.id).await? })))
}

/// After a push here: pull requests these branches opened on other servers take the new commits.
/// One that's gone there (closed, merged) is forgotten here.
pub async fn pushed(st: &AppState, repo: &Repo, updates: &[crate::smart_http::RefUpdate]) {
    let branches: Vec<String> = updates.iter().filter(|u| u.new.is_some()).filter_map(|u| u.refname.strip_prefix("refs/heads/").map(String::from)).collect();
    if branches.is_empty() || !st.settings.get().federation {
        return;
    }
    let rid = repo.id;
    let Ok(Some((server, owner, name))) = remote_fork(st, rid).await else { return };
    let rows: Vec<(i64, String, String, i64)> = st
        .db
        .call(move |c| {
            let mut q = c.prepare("SELECT id, fork, branch, number FROM outgoing_pulls WHERE repo_id = ?1")?;
            Ok(q.query_map([rid], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<Result<_, _>>()?)
        })
        .await
        .unwrap_or_default();
    for (id, fork, branch, number) in rows.into_iter().filter(|r| branches.contains(&r.2)) {
        let (st, server) = (st.clone(), server.clone());
        // The fork's name as it was when it asked (the other server knows it by that).
        let body = json!({ "repo": format!("{owner}/{name}"), "number": number, "fork": fork, "branch": branch });
        // Not waited for: a slow or gone server doesn't hold up the push.
        tokio::spawn(async move {
            match call(&st, &server, "/api/federation/pulls/update", &body).await {
                Ok(_) => {}
                Err(e) if e.status == StatusCode::NOT_FOUND => {
                    let _ = st.db.call(move |c| Ok(c.execute("DELETE FROM outgoing_pulls WHERE id = ?1", [id])?)).await;
                }
                Err(e) => eprintln!("federation: telling {server} about {} {}: {}", body["fork"], body["branch"], e.msg),
            }
        });
    }
}

// ---------------------------------------------------------------- requests from other servers

#[derive(Deserialize)]
struct Author {
    username: String,
    #[serde(default)]
    name: String,
}

#[derive(Deserialize)]
struct Incoming {
    /// "owner/name" here.
    repo: String,
    base: Option<String>,
    /// "owner/name" of the fork on the signing server.
    fork: String,
    branch: String,
    title: String,
    #[serde(default)]
    body: String,
    author: Author,
}

fn parse<T: serde::de::DeserializeOwned>(body: &[u8]) -> R<T> {
    serde_json::from_slice(body).map_err(|e| AppError::bad(format!("That request isn't what was expected: {e}")))
}

/// A public repository here that takes pull requests from other servers.
async fn target(st: &AppState, full: &str) -> R<Repo> {
    let (o, n) = full.split_once('/').ok_or_else(AppError::not_found)?;
    let r = Repo::readable(st, None, o, n).await?;
    crate::mirrors::refuse_writes(&r)?;
    Ok(r)
}

/// The fork's address on its server, and how it's fetched.
pub(crate) fn source(server: &str, fork: &str, st: &AppState) -> R<crate::mirrors::Remote> {
    let (o, n) = fork.split_once('/').ok_or_else(|| AppError::bad("Name the fork as owner/name"))?;
    if !validate::username(o) || !validate::repo_name(n) {
        return Err(AppError::bad("Name the fork as owner/name"));
    }
    Ok(crate::mirrors::Remote { url: format!("{server}/{o}/{n}.git"), username: String::new(), password: String::new(), allow_private: st.settings.get().federation_private })
}

/// Someone from another server, as a person here: `bob@b.example`, who can't sign in.
async fn remote_person(st: &AppState, server: &str, a: &Author) -> R<User> {
    let username = format!("{}@{}", a.username, federation::host(server));
    let profile = format!("{server}/{}", a.username);
    let name: String = a.name.chars().filter(|c| !c.is_control()).take(100).collect();
    let (u2, p2) = (username.clone(), profile.clone());
    let id: i64 = st
        .db
        .call(move |c| {
            c.execute(
                "INSERT INTO users(username, email, pass_hash, display_name, remote, created_at) VALUES (?1, '', '!', ?2, ?3, ?4)
                 ON CONFLICT(username) DO UPDATE SET display_name = excluded.display_name WHERE users.remote = excluded.remote",
                rusqlite::params![u2, name, p2, now()],
            )?;
            Ok(c.query_row("SELECT id FROM users WHERE username = ?1 AND remote = ?2", [u2, p2], |r| r.get(0)).optional()?)
        })
        .await?
        // Someone from another server at the same host (http and https) has that name here.
        .ok_or_else(|| AppError::conflict(format!("{username} here is someone else")))?;
    crate::auth::user_by_id(&st.db, id).await?.ok_or_else(AppError::not_found)
}

/// POST /api/federation/pulls (signed): open a pull request here from a branch of a fork on the
/// signing server, or, when one is open from it into that base already, have it take the branch again.
pub async fn incoming(State(st): State<AppState>, ip: super::ClientIp, headers: HeaderMap, body: Bytes) -> J<Value> {
    let server = signer(&st, &ip.0, &headers, "/api/federation/pulls", &body).await?;
    let req: Incoming = parse(&body)?;
    let r = target(&st, &req.repo).await?;
    let base = req.base.clone().filter(|b| !b.trim().is_empty()).unwrap_or_else(|| r.default_branch.clone());
    if !validate::ref_name(&req.branch) || !validate::ref_name(&base) {
        return Err(AppError::bad("Pick two branches"));
    }
    super::issues::check_text(&req.title, &req.body)?;
    if !validate::username(&req.author.username) {
        return Err(AppError::bad("That author's name isn't one"));
    }
    let remote = source(&server, &req.fork, &st)?;
    if let Some(number) = open_from(&st, &r, &server, &req.fork, &req.branch, Some(&base), None).await? {
        refetch(&st, &r, &remote, &req.branch, number).await?;
        return Ok(Json(json!({ "number": number })));
    }
    let base_sha = st.git.resolve(&r.path, &format!("refs/heads/{base}")).await?.ok_or_else(|| AppError::bad("The target branch doesn't exist"))?;
    // Any server can ask: each has a few open at a time per repository (they take its storage).
    let (rid, sv) = (r.id, server.clone());
    let open: i64 = st
        .db
        .call(move |c| {
            Ok(c.query_row(
                "SELECT count(*) FROM remote_pulls rp JOIN issues i ON i.id = rp.issue_id WHERE i.repo_id = ?1 AND i.state = 'open' AND rp.server = ?2",
                rusqlite::params![rid, sv],
                |r| r.get(0),
            )?)
        })
        .await?;
    if open >= MAX_OPEN {
        return Err(AppError::conflict(format!("{} has {MAX_OPEN} pull requests open here already", federation::host(&server))));
    }
    let pull = super::issues::PullReq { title: req.title.clone(), body: req.body.clone(), head: req.branch.clone(), base: base.clone(), head_repo: None, maintainer_can_modify: false };
    let (st2, r2, fork) = (st.clone(), r.clone(), req.fork.clone());
    // In a task of its own, so the copy is cleaned up however the request ends. The person is
    // made only once their branch is here.
    let opened = tokio::spawn(async move {
        let pending = format!("refs/pull/new-{}/head", crate::auth::new_secret("").0.replace(['-', '_'], "x"));
        let opened = async {
            let head = super::issues::fetch_remote_branch(&st2, r2.id, &remote, &pull.head, &pending).await?;
            if head.is_none() {
                return Err(AppError::bad("The source branch doesn't exist"));
            }
            let author = remote_person(&st2, &server, &req.author).await?;
            let o = super::issues::open_pull(&st2, &r2, &author, pull, None, Some((&server, &fork)), Ok(head), &base_sha).await?;
            Ok((o, author))
        }
        .await;
        let _ = st2.git.delete_ref(&r2.path, &pending).await;
        opened
    })
    .await
    .map_err(AppError::internal)?;
    let ((id, num, title, body, ..), author) = opened?;
    super::issues::announce_pull(&st, &r, &author, id, num, &title, &body).await?;
    Ok(Json(json!({ "number": num })))
}

/// The open pull request here from that fork's branch on `server` (into `base`, or numbered `number`).
async fn open_from(st: &AppState, r: &Repo, server: &str, fork: &str, branch: &str, base: Option<&str>, number: Option<i64>) -> R<Option<i64>> {
    let (rid, s, f, b, base, num) = (r.id, server.to_string(), fork.to_string(), branch.to_string(), base.map(String::from), number);
    st.db
        .call(move |c| {
            Ok(c.query_row(
                "SELECT i.number FROM remote_pulls rp JOIN issues i ON i.id = rp.issue_id JOIN pulls p ON p.issue_id = i.id
                 WHERE i.repo_id = ?1 AND i.state = 'open' AND rp.server = ?2 AND rp.repo = ?3 AND rp.branch = ?4
                   AND (?5 IS NULL OR p.base_branch = ?5) AND (?6 IS NULL OR i.number = ?6)",
                rusqlite::params![rid, s, f, b, base, num],
                |r| r.get(0),
            )
            .optional()?)
        })
        .await
}

async fn refetch(st: &AppState, r: &Repo, remote: &crate::mirrors::Remote, branch: &str, number: i64) -> R<String> {
    let sha = super::issues::fetch_remote_branch(st, r.id, remote, branch, &format!("refs/pull/{number}/head"))
        .await?
        .ok_or_else(|| AppError::bad("That branch isn't in the fork"))?;
    crate::mergequeue::kick(st, r.id);
    Ok(sha)
}

/// A reopened pull request from a fork on another server catches up with its branch.
pub(crate) async fn reopened(st: &AppState, r: &Repo, issue_id: i64, number: i64) {
    let found = st
        .db
        .call(move |c| Ok(c.query_row("SELECT server, repo, branch FROM remote_pulls WHERE issue_id = ?1", [issue_id], |x| Ok((x.get::<_, String>(0)?, x.get::<_, String>(1)?, x.get::<_, String>(2)?))).optional()?))
        .await;
    if let Ok(Some((server, fork, branch))) = found {
        let r2 = source(&server, &fork, st).map(|remote| (remote, branch));
        if let Ok((remote, branch)) = r2
            && let Err(e) = refetch(st, r, &remote, &branch, number).await
        {
            eprintln!("federation: {}#{number} from {server}: {}", r.full_name(), e.msg);
        }
    }
}

#[derive(Deserialize)]
struct Update {
    repo: String,
    number: i64,
    fork: String,
    branch: String,
}

/// POST /api/federation/pulls/update (signed): the fork's branch moved; the pull request from
/// it takes the new commits. Only the fork's own server can ask.
pub async fn update(State(st): State<AppState>, ip: super::ClientIp, headers: HeaderMap, body: Bytes) -> J<Value> {
    let server = signer(&st, &ip.0, &headers, "/api/federation/pulls/update", &body).await?;
    let req: Update = parse(&body)?;
    let r = target(&st, &req.repo).await?;
    let number = open_from(&st, &r, &server, &req.fork, &req.branch, None, Some(req.number)).await?.ok_or_else(AppError::not_found)?;
    let sha = refetch(&st, &r, &source(&server, &req.fork, &st)?, &req.branch, number).await?;
    Ok(Json(json!({ "number": number, "sha": sha })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_addresses_name_a_server_and_a_repository() {
        assert_eq!(parse_repo_url("https://A.example/alice/notes.git/"), Some(("https://a.example".into(), "alice".into(), "notes".into())));
        assert_eq!(parse_repo_url("http://127.0.0.1:3000/alice/notes"), Some(("http://127.0.0.1:3000".into(), "alice".into(), "notes".into())));
        for bad in ["https://a.example/alice", "https://a.example/alice/notes/tree/main", "https://a.example/../x", "ftp://a.example/a/b", "https://u@a.example/a/b"] {
            assert!(parse_repo_url(bad).is_none(), "{bad}");
        }
    }
}
