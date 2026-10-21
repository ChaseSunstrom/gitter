use std::collections::{HashMap, HashSet};
use std::path::Path as FsPath;
use std::process::Stdio;
use std::time::Duration;

use axum::Json;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::Response;
use base64::Engine;
use serde::Deserialize;
use serde_json::json;
use tokio::io::{AsyncBufReadExt, BufReader};

use super::J;
use crate::auth::{Authed, Viewer};
use crate::error::{AppError, R};
use crate::git::{Change, CommitSpec, LogQuery, parse_blame};
use crate::repo::{Repo, record_event, touch};
use crate::smart_http::RefUpdate;
use crate::{AppState, diff, validate};

const BLOB_DISPLAY_CAP: u64 = 1_500_000;
const README_CAP: u64 = 512 * 1024;

#[derive(Deserialize)]
pub struct RevPath {
    rev: Option<String>,
    path: Option<String>,
}

/// Validate and resolve `rev` (default branch if absent) to a commit sha.
pub async fn resolve_rev(st: &AppState, r: &Repo, rev: Option<&str>) -> R<String> {
    let rev = rev.filter(|s| !s.is_empty()).unwrap_or(&r.default_branch);
    if !validate::rev(rev) {
        return Err(AppError::bad("That isn't a valid branch, tag or commit"));
    }
    st.git
        .resolve(&r.path, rev)
        .await?
        .ok_or_else(|| AppError::new(StatusCode::NOT_FOUND, "Branch, tag or commit not found"))
}

fn clean_path(p: Option<&str>) -> R<String> {
    let p = p.unwrap_or("").trim_matches('/').to_string();
    if !validate::path(&p) {
        return Err(AppError::bad("Invalid path"));
    }
    Ok(p)
}

async fn last_commit(
    st: &AppState,
    r: &Repo,
    sha: &str,
    path: &str,
) -> R<Option<crate::git::Commit>> {
    let key = format!("lc:{}:{sha}:{path}", r.id);
    if let Some(hit) = st.git.memo.get(&key) {
        return Ok(hit);
    }
    let q = LogQuery {
        range: sha,
        path: Some(path),
        skip: 0,
        limit: 1,
        grep: None,
        author: None,
        first_parent: false,
    };
    let c = st.git.log(&r.path, &q).await?.into_iter().next();
    st.git.memo.put(key, &c);
    Ok(c)
}

/// Commits reachable from `sha` (immutable, so cached).
async fn commit_count(st: &AppState, r: &Repo, sha: &str) -> R<u64> {
    let key = format!("cnt:{}:{sha}", r.id);
    if let Some(n) = st.git.memo.get(&key) {
        return Ok(n);
    }
    let n = st.git.count(&r.path, sha).await?;
    st.git.memo.put(key, &n);
    Ok(n)
}

fn is_readme(name: &str) -> Option<u8> {
    let l = name.to_ascii_lowercase();
    match l.as_str() {
        "readme.md" | "readme.markdown" => Some(0),
        "readme" | "readme.txt" | "readme.rst" | "readme.org" => Some(1),
        _ => None,
    }
}

pub async fn tree(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    Path((o, n)): Path<(String, String)>,
    Query(q): Query<RevPath>,
) -> J<serde_json::Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    if q.rev.is_none()
        && st
            .git
            .refs(&r.path)
            .await?
            .iter()
            .all(|x| x.kind != "branch")
    {
        return Ok(Json(json!({ "empty": true })));
    }
    let sha = resolve_rev(&st, &r, q.rev.as_deref()).await?;
    let path = clean_path(q.path.as_deref())?;
    if !path.is_empty() {
        match st.git.stat(&r.path, &sha, &path).await? {
            Some((_, kind, _)) if kind == "tree" => {}
            Some((_, kind, _)) if kind == "blob" => return Ok(Json(json!({ "redirect": "blob" }))),
            _ => {
                return Err(AppError::new(
                    StatusCode::NOT_FOUND,
                    "That path doesn't exist on this branch",
                ));
            }
        }
    }
    let entries = st.git.ls_tree(&r.path, &sha, &path).await?;
    let readme_entry = entries
        .iter()
        .filter(|e| e.kind == "blob")
        .filter_map(|e| is_readme(&e.name).map(|rank| (rank, e)))
        .min_by_key(|(rank, _)| *rank)
        .map(|(_, e)| e);
    let readme = match readme_entry {
        Some(e) if e.size.unwrap_or(0) <= README_CAP => {
            let data = st.git.blob(&r.path, &e.oid).await?;
            Some(
                json!({ "name": e.name, "path": e.path, "content": String::from_utf8_lossy(&data) }),
            )
        }
        _ => None,
    };
    let (last, count) = tokio::join!(
        last_commit(&st, &r, &sha, &path),
        commit_count(&st, &r, &sha)
    );
    Ok(Json(
        json!({ "sha": sha, "path": path, "entries": entries, "readme": readme, "last_commit": last?, "commit_count": count? }),
    ))
}

/// The newest commit touching each entry of a directory. Walks history once and
/// stops when every entry is found, or after a bounded number of commits.
pub async fn tree_commits(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    Path((o, n)): Path<(String, String)>,
    Query(q): Query<RevPath>,
) -> J<serde_json::Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let sha = resolve_rev(&st, &r, q.rev.as_deref()).await?;
    let path = clean_path(q.path.as_deref())?;
    let key = format!("tc:{}:{sha}:{path}", r.id);
    if let Some(found) = st.git.memo.get::<HashMap<String, serde_json::Value>>(&key) {
        return Ok(Json(json!({ "sha": sha, "commits": found })));
    }
    let names: HashSet<String> = st
        .git
        .ls_tree(&r.path, &sha, &path)
        .await?
        .into_iter()
        .map(|e| e.name)
        .collect();
    let (found, complete) = last_commits(&st, &r.path, &sha, &path, names).await?;
    if complete {
        st.git.memo.put(key, &found);
    }
    Ok(Json(json!({ "sha": sha, "commits": found })))
}

async fn last_commits(
    st: &AppState,
    repo: &FsPath,
    sha: &str,
    dir: &str,
    mut want: HashSet<String>,
) -> R<(HashMap<String, serde_json::Value>, bool)> {
    let _permit = st.git.permit().await?;
    let mut cmd = st.git.command(Some(repo));
    cmd.args([
        "log",
        "--format=%x1e%H%x1f%at%x1f%an%x1f%s",
        "--name-only",
        "--no-renames",
        "-n",
        "3000",
        sha,
        "--",
    ]);
    if !dir.is_empty() {
        cmd.arg(dir);
    }
    cmd.stderr(Stdio::null());
    let mut child = cmd.spawn().map_err(AppError::internal)?;
    let mut lines = BufReader::new(child.stdout.take().expect("stdout")).lines();
    let mut out = HashMap::new();
    let mut cur = json!(null);
    let prefix = if dir.is_empty() {
        String::new()
    } else {
        format!("{dir}/")
    };
    let work = async {
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(h) = line.strip_prefix('\x1e') {
                let f: Vec<&str> = h.split('\x1f').collect();
                if f.len() == 4 {
                    cur = json!({ "sha": f[0], "time": f[1].parse::<i64>().unwrap_or(0), "author": f[2], "subject": f[3] });
                }
                continue;
            }
            if line.is_empty() {
                continue;
            }
            let p = diff::unquote(&line);
            let Some(rel) = p.strip_prefix(&prefix) else {
                continue;
            };
            let top = rel.split('/').next().unwrap_or(rel);
            if want.remove(top) {
                out.insert(top.to_string(), cur.clone());
                if want.is_empty() {
                    break;
                }
            }
        }
    };
    // Only a walk that finished cleanly (or found everything) is repeatable, so only that is cached.
    let finished = tokio::time::timeout(Duration::from_secs(10), work).await.is_ok();
    let found_all = want.is_empty();
    let exited_ok = finished
        && !found_all
        && matches!(tokio::time::timeout(Duration::from_secs(2), child.wait()).await, Ok(Ok(s)) if s.success());
    let _ = child.kill().await;
    Ok((out, found_all || exited_ok))
}

fn is_image(path: &str) -> Option<&'static str> {
    let ext = path.rsplit('.').next()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "avif" => "image/avif",
        "svg" => "image/svg+xml",
        _ => return None,
    })
}

pub async fn blob(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    Path((o, n)): Path<(String, String)>,
    Query(q): Query<RevPath>,
) -> J<serde_json::Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let sha = resolve_rev(&st, &r, q.rev.as_deref()).await?;
    let path = clean_path(q.path.as_deref())?;
    let (oid, kind, size) = st.git.stat(&r.path, &sha, &path).await?.ok_or_else(|| {
        AppError::new(
            StatusCode::NOT_FOUND,
            "That file doesn't exist on this branch",
        )
    })?;
    if kind == "tree" {
        return Ok(Json(json!({ "redirect": "tree" })));
    }
    let image = is_image(&path).is_some();
    let too_large = size > BLOB_DISPLAY_CAP;
    let (mode, last) = tokio::join!(
        st.git.mode_of(&r.path, &sha, &path),
        last_commit(&st, &r, &sha, &path)
    );
    let (mode, last) = (mode?.unwrap_or_default(), last?);
    // The browser editor only round-trips plain UTF-8 text with consistent line endings.
    let (mut content, mut binary, mut editable, mut eol) = (None, false, false, "lf");
    if too_large || (image && !path.ends_with(".svg")) {
        binary = !path.ends_with(".svg");
    } else {
        let data = st.git.blob(&r.path, &oid).await?;
        if data.iter().take(8000).any(|b| *b == 0) {
            binary = true;
        } else {
            match String::from_utf8(data) {
                Ok(text) => {
                    let (crlf, lf) = (text.matches("\r\n").count(), text.matches('\n').count());
                    eol = if crlf > 0 && crlf == lf { "crlf" } else { "lf" };
                    editable = (crlf == 0 || crlf == lf) && mode != "120000";
                    content = Some(text);
                }
                Err(e) => content = Some(String::from_utf8_lossy(e.as_bytes()).into_owned()),
            }
        }
    }
    let lang = crate::langs::detect(&path).map(|l| l.0);
    Ok(Json(json!({
        "sha": sha, "path": path, "oid": oid, "size": size, "binary": binary, "image": image, "mode": mode,
        "too_large": too_large, "content": content, "editable": editable, "eol": eol, "last_commit": last, "language": lang,
    })))
}

/// Raw file bytes: `/raw/{owner}/{repo}/{rev}/{path}`. Served as inert content
/// (text/plain or image, sandboxed CSP) so user files can never run script on this origin.
pub async fn raw(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    Path((o, n, rest)): Path<(String, String, String)>,
) -> R<Response> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let (rev, path) = split_rev_path(&st, &r, &rest).await?;
    let sha = resolve_rev(&st, &r, Some(&rev)).await?;
    let path = clean_path(Some(&path))?;
    let (oid, kind, _) = st
        .git
        .stat(&r.path, &sha, &path)
        .await?
        .ok_or_else(AppError::not_found)?;
    if kind != "blob" {
        return Err(AppError::not_found());
    }
    let data = st.git.blob(&r.path, &oid).await?;
    let ctype = match is_image(&path) {
        Some(t) => t,
        None if data.iter().take(8000).any(|b| *b == 0) => "application/octet-stream",
        None => "text/plain; charset=utf-8",
    };
    let immutable = rev.len() == 40 && validate::sha(&rev);
    Ok(inert(data, ctype, immutable, r.private))
}

/// User file bytes served so they can never run script on this origin.
pub fn inert(data: Vec<u8>, ctype: &'static str, immutable: bool, private: bool) -> Response {
    let mut resp = Response::new(Body::from(data));
    let h = resp.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static(ctype));
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'none'; img-src 'self' data:; style-src 'unsafe-inline'; sandbox",
        ),
    );
    h.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    let cache = match (private, immutable) {
        (true, _) => "private, no-store",
        (false, true) => "public, max-age=31536000, immutable",
        (false, false) => "private, max-age=60",
    };
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    resp
}

/// Split "feature/x/src/main.rs" into (rev, path) using the longest matching branch or tag.
pub async fn split_rev_path(st: &AppState, r: &Repo, rest: &str) -> R<(String, String)> {
    let refs = st.git.refs(&r.path).await?;
    let mut best: Option<&str> = None;
    for x in &refs {
        let n = x.name.as_str();
        if (rest == n || rest.starts_with(&format!("{n}/")))
            && best.is_none_or(|b| n.len() > b.len())
        {
            best = Some(n);
        }
    }
    let rev = match best {
        Some(b) => b.to_string(),
        None => rest.split('/').next().unwrap_or("").to_string(),
    };
    let path = rest[rev.len()..].trim_start_matches('/').to_string();
    Ok((rev, path))
}

pub async fn archive(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    Path((o, n, spec)): Path<(String, String, String)>,
) -> R<Response> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let (rev, fmt, ext) = if let Some(x) = spec.strip_suffix(".zip") {
        (x, "zip", "zip")
    } else if let Some(x) = spec.strip_suffix(".tar.gz") {
        (x, "tar.gz", "tar.gz")
    } else {
        return Err(AppError::not_found());
    };
    let sha = resolve_rev(&st, &r, Some(rev)).await?;
    let label = format!("{}-{}", r.name, rev.replace('/', "-"));
    let permit = st.git.permit_stream().await?;
    let mut cmd = st.git.command(Some(&r.path));
    cmd.args([
        "archive",
        &format!("--format={fmt}"),
        &format!("--prefix={label}/"),
        &sha,
    ])
    .stderr(Stdio::null());
    let mut child = cmd.spawn().map_err(AppError::internal)?;
    let stdout = child.stdout.take().expect("stdout");
    tokio::spawn(async move {
        let _p = permit;
        if tokio::time::timeout(Duration::from_secs(300), child.wait())
            .await
            .is_err()
        {
            let _ = child.start_kill();
        }
    });
    let body = Body::from_stream(tokio_util::io::ReaderStream::new(stdout));
    let mut resp = Response::new(body);
    let h = resp.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(if ext == "zip" {
            "application/zip"
        } else {
            "application/gzip"
        }),
    );
    if let Ok(v) = HeaderValue::from_str(&format!("attachment; filename=\"{label}.{ext}\"")) {
        h.insert(header::CONTENT_DISPOSITION, v);
    }
    Ok(resp)
}

pub async fn files(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    Path((o, n)): Path<(String, String)>,
    Query(q): Query<RevPath>,
) -> J<serde_json::Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let sha = resolve_rev(&st, &r, q.rev.as_deref()).await?;
    let (out, cut) = st
        .git
        .run_capped(
            &r.path,
            &["ls-tree", "-r", "--name-only", "-z", &sha],
            4 * 1024 * 1024,
        )
        .await?;
    let mut files: Vec<String> = out
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect();
    if cut {
        files.pop();
    }
    let truncated = cut || files.len() > 50_000;
    files.truncate(50_000);
    Ok(Json(
        json!({ "sha": sha, "files": files, "truncated": truncated }),
    ))
}

#[derive(Deserialize)]
pub struct GrepQ {
    q: String,
    rev: Option<String>,
}

pub async fn grep(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    Path((o, n)): Path<(String, String)>,
    Query(q): Query<GrepQ>,
) -> J<serde_json::Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let needle = q.q.trim();
    if needle.is_empty() || needle.len() > 200 {
        return Ok(Json(json!({ "results": [] })));
    }
    let sha = resolve_rev(&st, &r, q.rev.as_deref()).await?;
    let (out, cut) = st
        .git
        .run_capped(
            &r.path,
            &[
                "grep",
                "-n",
                "-I",
                "-i",
                "-F",
                "-z",
                "--max-count=20",
                "-e",
                needle,
                &sha,
                "--",
            ],
            1024 * 1024,
        )
        .await?;
    let text = String::from_utf8_lossy(&out);
    let mut by_file: Vec<(String, Vec<serde_json::Value>)> = Vec::new();
    let prefix = format!("{sha}:");
    for line in text.lines() {
        let mut parts = line.splitn(3, '\0');
        let (Some(file), Some(no), Some(body)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        let file = file.strip_prefix(&prefix).unwrap_or(file).to_string();
        let body: String = body.chars().take(300).collect();
        let m = json!({ "line": no.parse::<u32>().unwrap_or(0), "text": body });
        match by_file.last_mut() {
            Some((f, v)) if *f == file => v.push(m),
            _ => {
                if by_file.len() >= 200 {
                    break;
                }
                by_file.push((file, vec![m]));
            }
        }
    }
    let results: Vec<_> = by_file
        .into_iter()
        .map(|(f, m)| json!({ "path": f, "matches": m }))
        .collect();
    Ok(Json(
        json!({ "sha": sha, "results": results, "truncated": cut }),
    ))
}

#[derive(Deserialize)]
pub struct CommitsQ {
    rev: Option<String>,
    path: Option<String>,
    q: Option<String>,
    author: Option<String>,
    page: Option<usize>,
}

pub async fn commits(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    Path((o, n)): Path<(String, String)>,
    Query(q): Query<CommitsQ>,
) -> J<serde_json::Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let sha = resolve_rev(&st, &r, q.rev.as_deref()).await?;
    let path = clean_path(q.path.as_deref())?;
    let page = super::page(q.page);
    let per = 35;
    let grep =
        q.q.as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty() && s.len() < 200);
    let author = q
        .author
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty() && s.len() < 200);
    let lq = LogQuery {
        range: &sha,
        path: Some(&path),
        skip: (page - 1) * per,
        limit: per + 1,
        grep,
        author,
        first_parent: false,
    };
    let mut list = st.git.log(&r.path, &lq).await?;
    let more = list.len() > per;
    list.truncate(per);
    Ok(Json(
        json!({ "sha": sha, "commits": list, "has_more": more, "page": page }),
    ))
}

#[derive(Deserialize, Default)]
pub struct FileQ {
    /// Diff only this file, with a larger size limit (for "Load diff").
    pub path: Option<String>,
    /// The file's old path when it was renamed, so the rename still pairs up.
    pub old: Option<String>,
}

/// Diff `from..to`, optionally just one file (plus its old name).
pub async fn diff_of(st: &AppState, repo: &FsPath, from: Option<&str>, to: &str, f: &FileQ) -> R<diff::DiffSummary> {
    let mut paths = Vec::new();
    for p in [&f.path, &f.old].into_iter().flatten() {
        let p = clean_path(Some(p))?;
        if !p.is_empty() && !paths.contains(&p) {
            paths.push(p);
        }
    }
    let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
    let (text, cut) = st.git.diff(repo, from, to, &refs).await?;
    Ok(if refs.is_empty() { diff::parse(&text, cut) } else { diff::parse_one(&text, cut) })
}

pub async fn commit(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    Path((o, n, sha)): Path<(String, String, String)>,
    Query(fq): Query<FileQ>,
) -> J<serde_json::Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let sha = resolve_rev(&st, &r, Some(&sha)).await?;
    let info = st
        .git
        .commit_info(&r.path, &sha)
        .await?
        .ok_or_else(AppError::not_found)?;
    let parent = info.parents.first().cloned();
    let d = diff_of(&st, &r.path, parent.as_deref(), &sha, &fq).await?;
    Ok(Json(json!({ "commit": info, "diff": d })))
}

#[derive(Deserialize)]
pub struct CompareQ {
    base: String,
    head: String,
    #[serde(flatten)]
    file: FileQ,
}

pub struct Comparison {
    pub base_sha: String,
    pub head_sha: String,
    pub merge_base: Option<String>,
    pub ahead: u64,
    pub behind: u64,
    pub commits: Vec<crate::git::Commit>,
    pub diff: diff::DiffSummary,
}

impl Comparison {
    pub fn to_json(&self) -> serde_json::Value {
        json!({ "base_sha": self.base_sha, "head_sha": self.head_sha, "merge_base": self.merge_base,
                "ahead": self.ahead, "behind": self.behind, "commits": self.commits, "diff": self.diff })
    }
}

/// What `head` adds on top of `base` (three-dot semantics, like a pull request).
pub async fn compare_shas(st: &AppState, path: &FsPath, base: &str, head: &str, file: &FileQ) -> R<Comparison> {
    let mb = st.git.merge_base(path, base, head).await?;
    let from = mb.clone().unwrap_or_else(|| base.to_string());
    let range = format!("{from}..{head}");
    let lq = LogQuery {
        range: &range,
        path: None,
        skip: 0,
        limit: 250,
        grep: None,
        author: None,
        first_parent: false,
    };
    let (fwd, back) = (format!("{base}..{head}"), format!("{head}..{base}"));
    let (commits, ahead, behind, d) = tokio::join!(
        st.git.log(path, &lq),
        st.git.count(path, &fwd),
        st.git.count(path, &back),
        diff_of(st, path, Some(&from), head, file),
    );
    let mut commits = commits?;
    commits.reverse();
    Ok(Comparison {
        base_sha: base.into(),
        head_sha: head.into(),
        merge_base: mb,
        ahead: ahead?,
        behind: behind?,
        commits,
        diff: d?,
    })
}

pub async fn compare(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    Path((o, n)): Path<(String, String)>,
    Query(q): Query<CompareQ>,
) -> J<serde_json::Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let base = resolve_rev(&st, &r, Some(&q.base)).await?;
    let head = resolve_rev(&st, &r, Some(&q.head)).await?;
    let c = compare_shas(&st, &r.path, &base, &head, &q.file).await?;
    Ok(Json(c.to_json()))
}

pub async fn blame(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    Path((o, n)): Path<(String, String)>,
    Query(q): Query<RevPath>,
) -> J<serde_json::Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let sha = resolve_rev(&st, &r, q.rev.as_deref()).await?;
    let path = clean_path(q.path.as_deref())?;
    let (_, kind, size) = st
        .git
        .stat(&r.path, &sha, &path)
        .await?
        .ok_or_else(AppError::not_found)?;
    if kind != "blob" {
        return Err(AppError::bad("Blame works on files"));
    }
    if size > BLOB_DISPLAY_CAP {
        return Err(AppError::bad("This file is too large to blame"));
    }
    let key = format!("bl:{}:{sha}:{path}", r.id);
    let chunks = match st.git.memo.get::<serde_json::Value>(&key) {
        Some(c) => c,
        None => {
            let out = st.git.blame(&r.path, &sha, &path).await?;
            let c = serde_json::to_value(parse_blame(&String::from_utf8_lossy(&out))).map_err(AppError::internal)?;
            st.git.memo.put(key, &c);
            c
        }
    };
    Ok(Json(
        json!({ "sha": sha, "path": path, "chunks": chunks, "language": crate::langs::detect(&path).map(|l| l.0) }),
    ))
}

pub async fn languages(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    Path((o, n)): Path<(String, String)>,
) -> J<serde_json::Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let Some(head) = st.git.resolve(&r.path, "HEAD").await? else {
        return Ok(Json(json!({ "languages": [] })));
    };
    let key = format!("lang:{}:{head}", r.id);
    let s = match st.git.memo.get(&key) {
        Some(s) => s,
        None => {
            let s = crate::langs::stats(&st, &r.path, &head).await;
            if let Some(s) = &s {
                st.git.memo.put(key, s);
            }
            s.unwrap_or_default()
        }
    };
    let total: u64 = s.iter().map(|x| x.2).sum::<u64>().max(1);
    let out: Vec<_> = s
        .iter()
        .map(|(name, color, bytes)| json!({ "name": name, "color": color, "bytes": bytes, "percent": (*bytes as f64) * 100.0 / total as f64 }))
        .collect();
    Ok(Json(json!({ "languages": out })))
}

pub async fn contributors(
    State(st): State<AppState>,
    Viewer(v): Viewer,
    Path((o, n)): Path<(String, String)>,
) -> J<serde_json::Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    if st.git.resolve(&r.path, "HEAD").await?.is_none() {
        return Ok(Json(json!({ "contributors": [], "weeks": [] })));
    }
    let (sl, times) = tokio::join!(
        st.git
            .run_capped(&r.path, &["shortlog", "-sne", "HEAD", "--"], 512 * 1024),
        st.git.run_capped(
            &r.path,
            &["log", "--format=%at", "--since=1.year", "HEAD", "--"],
            2 * 1024 * 1024
        ),
    );
    let (sl, _) = sl?;
    let contributors: Vec<_> = String::from_utf8_lossy(&sl)
        .lines()
        .take(100)
        .filter_map(|l| {
            let (n, rest) = l.trim().split_once('\t')?;
            let (name, email) = rest.rsplit_once(" <").map(|(a, b)| (a, b.trim_end_matches('>'))).unwrap_or((rest, ""));
            Some(json!({ "name": name, "email_hash": crate::auth::sha256_hex(&email.to_lowercase()), "commits": n.parse::<u64>().unwrap_or(0) }))
        })
        .collect();
    let now = crate::db::now();
    let mut weeks = vec![0u32; 52];
    for t in String::from_utf8_lossy(&times?.0)
        .lines()
        .filter_map(|l| l.parse::<i64>().ok())
    {
        let age = ((now - t) / (7 * 86400)) as usize;
        if age < 52 {
            weeks[51 - age] += 1;
        }
    }
    Ok(Json(
        json!({ "contributors": contributors, "weeks": weeks }),
    ))
}

#[derive(Deserialize)]
pub struct FileChange {
    path: String,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    base64: bool,
    #[serde(default)]
    delete: bool,
}

#[derive(Deserialize)]
pub struct WriteReq {
    branch: String,
    /// Branch head the editor started from; the commit fails if the branch moved.
    expected_sha: Option<String>,
    /// Commit onto a new branch created from `branch` (for "propose changes").
    new_branch: Option<String>,
    message: String,
    changes: Vec<FileChange>,
}

pub async fn write_contents(
    State(st): State<AppState>,
    Authed(u): Authed,
    Path((o, n)): Path<(String, String)>,
    Json(req): Json<WriteReq>,
) -> J<serde_json::Value> {
    let r = Repo::writable(&st, Some(&u), &o, &n).await?;
    if !validate::ref_name(&req.branch) {
        return Err(AppError::bad("That isn't a valid branch name"));
    }
    let message = req.message.trim();
    if message.is_empty() || message.len() > 10_000 {
        return Err(AppError::bad("Write a commit message"));
    }
    if req.changes.is_empty() || req.changes.len() > 500 {
        return Err(AppError::bad("Nothing to commit"));
    }
    let mut decoded: Vec<(String, Option<Vec<u8>>)> = Vec::new();
    let mut total = 0usize;
    for c in &req.changes {
        let p = c.path.trim_matches('/');
        if !validate::write_path(p) {
            return Err(AppError::bad(format!(
                "'{}' isn't a valid file path",
                c.path
            )));
        }
        if c.delete {
            decoded.push((p.to_string(), None));
            continue;
        }
        let content = c.content.as_deref().unwrap_or("");
        let bytes = if c.base64 {
            base64::engine::general_purpose::STANDARD
                .decode(content)
                .map_err(|_| AppError::bad("Bad file encoding"))?
        } else {
            content.as_bytes().to_vec()
        };
        total += bytes.len();
        if total > 25 * 1024 * 1024 {
            return Err(AppError::bad(
                "That's more than 25 MB; push large files with git instead",
            ));
        }
        decoded.push((p.to_string(), Some(bytes)));
    }

    let head = st
        .git
        .resolve(&r.path, &format!("refs/heads/{}", req.branch))
        .await?;
    let has_branches = st
        .git
        .refs(&r.path)
        .await?
        .iter()
        .any(|x| x.kind == "branch");
    if head.is_none() && has_branches {
        return Err(AppError::bad("That branch doesn't exist"));
    }
    if let (Some(exp), Some(h)) = (&req.expected_sha, &head)
        && exp != h
    {
        return Err(AppError::conflict(
            "Someone else changed this branch. Reload to see their changes.",
        ));
    }
    let target = match &req.new_branch {
        Some(nb) => {
            if !validate::ref_name(nb) {
                return Err(AppError::bad("That isn't a valid branch name"));
            }
            if st
                .git
                .resolve(&r.path, &format!("refs/heads/{nb}"))
                .await?
                .is_some()
            {
                return Err(AppError::conflict("A branch with that name already exists"));
            }
            nb.clone()
        }
        None => req.branch.clone(),
    };
    let expect_old = if req.new_branch.is_some() {
        None
    } else {
        head.clone()
    };
    // Keep existing executables executable, and never turn a symlink into a file.
    let mut exec = Vec::with_capacity(decoded.len());
    for (p, d) in &decoded {
        let mode = match (&head, d) {
            (Some(h), Some(_)) => st.git.mode_of(&r.path, h, p).await?,
            _ => None,
        };
        if mode.as_deref() == Some("120000") {
            return Err(AppError::bad(format!(
                "{p} is a symbolic link; change it with git"
            )));
        }
        exec.push(mode.as_deref() == Some("100755"));
    }
    let changes: Vec<Change> = decoded
        .iter()
        .zip(exec)
        .map(|((p, d), x)| match d {
            Some(bytes) => Change::Write(p, bytes, x),
            None => Change::Delete(p),
        })
        .collect();
    let (an, ae) = u.signature();
    let spec = CommitSpec {
        branch: &target,
        parent: head.as_deref(),
        expect_old: expect_old.as_deref(),
        changes: &changes,
        message,
        author: (&an, &ae),
    };
    let sha = st.git.commit_files(&r.path, &spec).await?;
    if !has_branches {
        st.git.set_head(&r.path, &target).await?;
        let (id, b) = (r.id, target.clone());
        st.db
            .call(move |c| {
                Ok(c.execute(
                    "UPDATE repos SET default_branch = ?1 WHERE id = ?2",
                    rusqlite::params![b, id],
                )?)
            })
            .await?;
    }
    touch(&st.db, r.id, true).await?;
    record_event(&st.db, u.id, Some(r.id), "push", &target, &sha, 1).await?;
    let upd = RefUpdate {
        refname: format!("refs/heads/{target}"),
        old: expect_old,
        new: Some(sha.clone()),
    };
    crate::hooks::on_push(&st, &r, &u, &[upd]).await;
    Ok(Json(json!({ "sha": sha, "branch": target })))
}
