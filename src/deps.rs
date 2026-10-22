//! Dependencies read from lockfiles, vulnerabilities matched against an OSV
//! snapshot the site admin imports (offline: JSON files or osv.dev's zips),
//! and SBOMs (SPDX 2.3 and CycloneDX 1.5 JSON).

use std::cmp::Ordering;
use std::io::Read;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::AppState;
use crate::error::{AppError, R};
use crate::repo::Repo;

/// One package a lockfile pins.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Dep {
    pub ecosystem: String,
    pub name: String,
    pub version: String,
    /// The lockfile it came from (the first, when several pin it).
    pub path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LockFile {
    pub path: String,
    pub ecosystem: String,
    pub count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Deps {
    pub files: Vec<LockFile>,
    pub deps: Vec<Dep>,
}

const MAX_FILES: usize = 50;
const MAX_FILE: usize = 32 << 20;
const MAX_TOTAL: usize = 64 << 20;
/// What one OSV zip may unpack to in all, and how many files it may have.
const MAX_UNZIPPED: usize = 1 << 30;
const MAX_ENTRIES: usize = 500_000;
const MAX_DEPS: usize = 20_000;

/// The lockfiles read, and the OSV ecosystem each one's packages live in.
fn ecosystem(file: &str) -> Option<&'static str> {
    Some(match file {
        "Cargo.lock" => "crates.io",
        "package-lock.json" | "npm-shrinkwrap.json" | "yarn.lock" | "pnpm-lock.yaml" => "npm",
        "go.sum" => "Go",
        "requirements.txt" | "poetry.lock" => "PyPI",
        "Gemfile.lock" => "RubyGems",
        _ => return None,
    })
}

/// (name, version) pairs from one lockfile.
pub fn parse(file: &str, text: &str) -> Vec<(String, String)> {
    match file {
        "Cargo.lock" => toml_packages(text, true),
        "poetry.lock" => toml_packages(text, false),
        "package-lock.json" | "npm-shrinkwrap.json" => npm(text),
        "yarn.lock" => yarn(text),
        "pnpm-lock.yaml" => pnpm(text),
        "go.sum" => go_sum(text),
        "requirements.txt" => requirements(text),
        "Gemfile.lock" => gemfile(text),
        _ => Vec::new(),
    }
}

/// `[[package]]` tables (Cargo.lock, poetry.lock). Cargo's own workspace crates
/// have no `source` and aren't dependencies.
fn toml_packages(text: &str, need_source: bool) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let (mut name, mut version, mut source, mut top) = (None, None, false, false);
    let mut flush = |name: &mut Option<String>, version: &mut Option<String>, source: bool| {
        if let (Some(n), Some(v)) = (name.take(), version.take())
            && (source || !need_source)
        {
            out.push((n, v));
        }
    };
    for l in text.lines() {
        let l = l.trim();
        if l.starts_with('[') {
            top = l == "[[package]]";
            if top {
                flush(&mut name, &mut version, source);
                source = false;
            }
            continue;
        }
        if !top {
            continue;
        }
        if let Some(v) = toml_str(l, "name") {
            name = Some(v);
        } else if let Some(v) = toml_str(l, "version") {
            version = Some(v);
        } else if l.starts_with("source") {
            source = true;
        }
    }
    flush(&mut name, &mut version, source);
    out
}

fn toml_str(l: &str, key: &str) -> Option<String> {
    let v = l.strip_prefix(key)?.trim_start().strip_prefix('=')?.trim().strip_prefix('"')?;
    Some(v.split('"').next()?.to_string())
}

/// package-lock.json / npm-shrinkwrap.json, lockfile versions 1 to 3.
fn npm(text: &str) -> Vec<(String, String)> {
    let Ok(v) = serde_json::from_str::<Value>(text) else { return Vec::new() };
    let mut out = Vec::new();
    if let Some(pkgs) = v["packages"].as_object() {
        for (k, p) in pkgs {
            let Some(at) = k.rfind("node_modules/") else { continue };
            if p["link"].as_bool() == Some(true) {
                continue;
            }
            let name = p["name"].as_str().unwrap_or(&k[at + 13..]);
            if let Some(ver) = p["version"].as_str() {
                out.push((name.to_string(), ver.to_string()));
            }
        }
        return out;
    }
    fn walk(deps: &Value, out: &mut Vec<(String, String)>) {
        for (name, p) in deps.as_object().into_iter().flatten() {
            if let Some(ver) = p["version"].as_str() {
                out.push((name.clone(), ver.to_string()));
            }
            walk(&p["dependencies"], out);
        }
    }
    walk(&v["dependencies"], &mut out);
    out
}

/// `name@range` -> name (scoped names start with @).
fn spec_name(spec: &str) -> &str {
    match spec[1.min(spec.len())..].find('@') {
        Some(i) => &spec[..i + 1],
        None => spec,
    }
}

/// yarn.lock, classic (`version "1.2.3"`) and berry (`version: 1.2.3`).
fn yarn(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut name: Option<String> = None;
    for l in text.lines() {
        if l.starts_with('#') || l.trim().is_empty() {
            continue;
        }
        if !l.starts_with(' ') {
            name = None;
            let Some(head) = l.strip_suffix(':') else { continue };
            let first = head.split(',').next().unwrap_or("").trim().trim_matches('"');
            if first == "__metadata" || ["@workspace:", "@link:", "@portal:", "@file:"].iter().any(|p| first.contains(p)) {
                continue;
            }
            name = Some(spec_name(first).to_string());
            continue;
        }
        let t = l.trim();
        if let (Some(n), Some(v)) = (&name, t.strip_prefix("version")) {
            let v = v.trim_start_matches(':').trim().trim_matches('"');
            if !v.is_empty() {
                out.push((n.clone(), v.to_string()));
            }
            name = None;
        }
    }
    out
}

/// pnpm-lock.yaml's `packages:` keys: `/name/1.2.3` (v5), `/name@1.2.3(peer)` (v6), `name@1.2.3` (v9).
fn pnpm(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut inside = false;
    for l in text.lines() {
        if !l.starts_with(' ') && !l.trim().is_empty() {
            inside = l.trim_end() == "packages:";
            continue;
        }
        let Some(key) = l.strip_prefix("  ").filter(|k| !k.starts_with(' ')).and_then(|k| k.trim_end().strip_suffix(':')) else { continue };
        if !inside {
            continue;
        }
        let key = key.trim_matches(|c| c == '\'' || c == '"').trim_start_matches('/');
        let key = key.split('(').next().unwrap_or(key);
        let (name, ver) = match key[1.min(key.len())..].rfind('@') {
            Some(i) => (&key[..i + 1], &key[i + 2..]),
            None => match key.rfind('/') {
                Some(i) => (&key[..i], &key[i + 1..]),
                None => continue,
            },
        };
        if !name.is_empty() && !ver.is_empty() {
            out.push((name.to_string(), ver.to_string()));
        }
    }
    out
}

/// go.sum: `module version hash`, once per module (not its go.mod line).
fn go_sum(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|l| {
            let mut p = l.split_whitespace();
            let (m, v) = (p.next()?, p.next()?);
            (!v.ends_with("/go.mod")).then(|| (m.to_string(), v.to_string()))
        })
        .collect()
}

/// requirements.txt: pinned `name==version` lines.
fn requirements(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for l in text.lines() {
        let l = l.split(" #").next().unwrap_or("").trim().trim_end_matches('\\').trim();
        if l.starts_with('#') || l.starts_with('-') {
            continue;
        }
        let l = l.split(';').next().unwrap_or("").trim();
        let Some((name, ver)) = l.split_once("==") else { continue };
        let (name, ver) = (name.split('[').next().unwrap_or("").trim(), ver.trim_start_matches('=').trim());
        if !name.is_empty() && !ver.is_empty() && !ver.contains('*') && !ver.contains(',') {
            out.push((name.to_string(), ver.to_string()));
        }
    }
    out
}

/// Gemfile.lock: `    name (version)` under GEM specs.
fn gemfile(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut gem = false;
    for l in text.lines() {
        if !l.starts_with(' ') {
            gem = l.trim() == "GEM";
            continue;
        }
        let Some(spec) = l.strip_prefix("    ").filter(|s| !s.starts_with(' ')) else { continue };
        if !gem {
            continue;
        }
        if let Some((name, rest)) = spec.split_once(" (") {
            let ver = rest.trim_end_matches(')');
            // Platform gems: 1.13.10-x86_64-linux.
            let ver = ver.split('-').next().unwrap_or(ver);
            out.push((name.to_string(), ver.to_string()));
        }
    }
    out
}

/// The dependencies on commit `sha` (cached: a commit never changes).
pub async fn of_commit(st: &AppState, repo: &Repo, sha: &str) -> R<Deps> {
    let key = format!("deps:{}:{sha}", repo.id);
    if let Some(d) = st.git.memo.get::<Deps>(&key) {
        return Ok(d);
    }
    let (list, _) = st.git.run_capped(&repo.path, &["ls-tree", "-r", "-z", "--name-only", sha], 16 << 20).await?;
    let mut paths: Vec<String> = list
        .split(|b| *b == 0)
        .filter_map(|p| std::str::from_utf8(p).ok())
        .filter(|p| {
            let base = p.rsplit('/').next().unwrap_or(p);
            ecosystem(base).is_some() && !p.split('/').any(|c| matches!(c, "node_modules" | "vendor" | "third_party"))
        })
        .map(String::from)
        .collect();
    paths.sort();
    paths.truncate(MAX_FILES);
    let mut texts = Vec::new();
    // At most 64 MB of lockfiles in memory for one listing (anyone may ask for a public repo's).
    let mut budget = MAX_TOTAL;
    for p in paths {
        if budget == 0 {
            break;
        }
        let (data, cut) = st.git.run_capped(&repo.path, &["cat-file", "blob", &format!("{sha}:{p}")], MAX_FILE.min(budget)).await?;
        if !cut {
            budget -= data.len().min(budget);
            texts.push((p, String::from_utf8(data).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())));
        }
    }
    let d = tokio::task::spawn_blocking(move || collect(texts)).await.map_err(AppError::internal)?;
    st.git.memo.put(key, &d);
    Ok(d)
}

fn collect(texts: Vec<(String, String)>) -> Deps {
    let mut d = Deps::default();
    let mut seen = std::collections::HashSet::new();
    for (path, text) in texts {
        let base = path.rsplit('/').next().unwrap_or(&path);
        let eco = ecosystem(base).unwrap_or("");
        let found = parse(base, &text);
        d.files.push(LockFile { path: path.clone(), ecosystem: eco.to_string(), count: found.len() });
        for (name, version) in found {
            if d.deps.len() < MAX_DEPS && seen.insert((eco, name.clone(), version.clone())) {
                d.deps.push(Dep { ecosystem: eco.to_string(), name, version, path: path.clone() });
            }
        }
    }
    d.deps.sort_by(|a, b| (&a.ecosystem, &a.name, &a.version).cmp(&(&b.ecosystem, &b.name, &b.version)));
    d
}

// ---------- versions ----------

/// Compares versions loosely enough for every ecosystem here: dot-separated
/// numbers (a leading v and +build ignored), then a -prerelease sorts first.
pub fn cmp_versions(a: &str, b: &str) -> Ordering {
    fn split(v: &str) -> (&str, Option<&str>) {
        let v = v.trim().trim_start_matches('v');
        let v = v.split('+').next().unwrap_or(v);
        match v.split_once('-') {
            Some((m, p)) => (m, Some(p)),
            None => (v, None),
        }
    }
    fn num(s: &str) -> (Option<u64>, &str) {
        let n = s.bytes().take_while(u8::is_ascii_digit).count();
        (s[..n].parse::<u64>().ok(), &s[n..])
    }
    fn seg(a: &str, b: &str) -> Ordering {
        match (num(a), num(b)) {
            ((Some(x), ra), (Some(y), rb)) => x.cmp(&y).then_with(|| match (ra.is_empty(), rb.is_empty()) {
                (true, true) => Ordering::Equal,
                // 1.0 is after 1.0rc1.
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                _ => ra.cmp(rb),
            }),
            ((Some(_), _), (None, _)) => Ordering::Less,
            ((None, _), (Some(_), _)) => Ordering::Greater,
            _ => a.cmp(b),
        }
    }
    fn parts(a: &str, b: &str) -> Ordering {
        let (mut x, mut y) = (a.split('.'), b.split('.'));
        loop {
            match (x.next(), y.next()) {
                (None, None) => return Ordering::Equal,
                (p, q) => {
                    let o = seg(p.unwrap_or("0"), q.unwrap_or("0"));
                    if o != Ordering::Equal {
                        return o;
                    }
                }
            }
        }
    }
    let ((am, ap), (bm, bp)) = (split(a), split(b));
    parts(am, bm).then_with(|| match (ap, bp) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(p), Some(q)) => parts(p, q),
    })
}

/// Whether `v` is affected (OSV `versions` and SEMVER/ECOSYSTEM `ranges`), and the version that fixes it.
pub fn affected(v: &str, versions: &[String], ranges: &[Value]) -> (bool, Option<String>) {
    if versions.iter().any(|x| cmp_versions(x, v) == Ordering::Equal) {
        return (true, None);
    }
    for r in ranges {
        let mut hit = false;
        for e in r["events"].as_array().into_iter().flatten() {
            if let Some(i) = e["introduced"].as_str()
                && (i == "0" || cmp_versions(v, i) != Ordering::Less)
            {
                hit = true;
            }
            if let Some(f) = e["fixed"].as_str() {
                if hit && cmp_versions(v, f) == Ordering::Less {
                    return (true, Some(f.to_string()));
                }
                hit = false;
            }
            if let Some(l) = e["last_affected"].as_str() {
                if hit && cmp_versions(v, l) != Ordering::Greater {
                    return (true, None);
                }
                hit = false;
            }
        }
        if hit {
            return (true, None);
        }
    }
    (false, None)
}

/// PyPI names compare case-insensitively with -, _ and . the same.
fn norm_name(eco: &str, name: &str) -> String {
    if eco != "PyPI" {
        return name.to_string();
    }
    let mut out = String::new();
    for c in name.chars() {
        let c = if matches!(c, '_' | '.') { '-' } else { c.to_ascii_lowercase() };
        if !(c == '-' && out.ends_with('-')) {
            out.push(c);
        }
    }
    out
}

// ---------- OSV ----------

/// One advisory, as stored.
pub struct Advisory {
    id: String,
    summary: String,
    severity: String,
    aliases: String,
    modified: String,
    withdrawn: bool,
    /// (ecosystem, name, versions JSON, ranges JSON)
    affected: Vec<(String, String, String, String)>,
}

fn advisory(v: &Value) -> Option<Advisory> {
    let id = v["id"].as_str().filter(|s| !s.is_empty() && s.len() <= 200)?.to_string();
    let cut = |s: &str, n: usize| s.chars().take(n).collect::<String>();
    let summary = v["summary"].as_str().or_else(|| v["details"].as_str()).map(|s| cut(s.lines().next().unwrap_or(""), 300)).unwrap_or_default();
    let severity = v["database_specific"]["severity"].as_str().map(|s| cut(&s.to_ascii_uppercase(), 20)).unwrap_or_default();
    let aliases: Vec<&str> = v["aliases"].as_array().into_iter().flatten().filter_map(Value::as_str).take(20).collect();
    let mut affected = Vec::new();
    for a in v["affected"].as_array().into_iter().flatten().take(1000) {
        let (Some(eco), Some(name)) = (a["package"]["ecosystem"].as_str(), a["package"]["name"].as_str()) else { continue };
        if eco.len() > 100 || name.len() > 500 {
            continue;
        }
        let ranges: Vec<&Value> = a["ranges"].as_array().into_iter().flatten().filter(|r| matches!(r["type"].as_str(), Some("SEMVER" | "ECOSYSTEM"))).collect();
        let versions = a.get("versions").filter(|v| v.is_array()).cloned().unwrap_or_else(|| json!([]));
        affected.push((eco.to_string(), norm_name(eco, name), versions.to_string(), json!(ranges).to_string()));
    }
    Some(Advisory {
        id,
        summary,
        severity,
        aliases: json!(aliases).to_string(),
        modified: cut(v["modified"].as_str().unwrap_or(""), 40),
        withdrawn: v.get("withdrawn").is_some_and(|w| !w.is_null()),
        affected,
    })
}

/// Advisories in an upload: one OSV JSON record, a JSON array of them, or a zip of `.json` files.
pub fn read_snapshot(body: &[u8]) -> Result<Vec<Advisory>, String> {
    let mut out = Vec::new();
    let mut add = |bytes: &[u8]| -> bool {
        match serde_json::from_slice::<Value>(bytes) {
            Ok(Value::Array(a)) => {
                out.extend(a.iter().filter_map(advisory));
                true
            }
            Ok(v @ Value::Object(_)) => {
                out.extend(advisory(&v));
                true
            }
            _ => false,
        }
    };
    if body.starts_with(b"PK\x03\x04") {
        unzip(body, |name, data| {
            if name.ends_with(".json") {
                add(&data);
            }
        })?;
    } else if !add(body) {
        return Err("That isn't an OSV JSON file, a JSON array of them, or a zip of them".into());
    }
    if out.is_empty() {
        return Err("No OSV advisories in that file".into());
    }
    Ok(out)
}

/// The files of a zip archive (stored or deflated; a 16 MB cap each).
fn unzip(b: &[u8], mut each: impl FnMut(&str, Vec<u8>)) -> Result<(), String> {
    let bad = || "That zip file is damaged or uses a format this server can't read".to_string();
    let u16at = |i: usize| b.get(i..i + 2).map(|s| u16::from_le_bytes([s[0], s[1]]) as usize);
    let u32at = |i: usize| b.get(i..i + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]) as usize);
    let u64at = |i: usize| b.get(i..i + 8).map(|s| u64::from_le_bytes(s.try_into().unwrap()) as usize);
    let eocd = (0..b.len().saturating_sub(21)).rev().take(65_558).find(|&i| b[i..].starts_with(b"PK\x05\x06")).ok_or_else(bad)?;
    let mut p = u32at(eocd + 16).ok_or_else(bad)?;
    // Zip64: the real offset is in the zip64 end record the locator points at.
    if p == 0xFFFF_FFFF || u16at(eocd + 10) == Some(0xFFFF) {
        let loc = eocd.checked_sub(20).filter(|&l| b[l..].starts_with(b"PK\x06\x07")).ok_or_else(bad)?;
        let rec = u64at(loc + 8).ok_or_else(bad)?;
        if !b.get(rec..).is_some_and(|s| s.starts_with(b"PK\x06\x06")) {
            return Err(bad());
        }
        p = u64at(rec + 48).ok_or_else(bad)?;
    }
    let (mut unpacked, mut entries) = (0usize, 0usize);
    while b.get(p..).is_some_and(|s| s.starts_with(b"PK\x01\x02")) {
        entries += 1;
        if entries > MAX_ENTRIES {
            return Err(format!("That zip has more than {MAX_ENTRIES} files: import it in parts"));
        }
        let (method, csize) = (u16at(p + 10).ok_or_else(bad)?, u32at(p + 20).ok_or_else(bad)?);
        let (nlen, elen, clen) = (u16at(p + 28).ok_or_else(bad)?, u16at(p + 30).ok_or_else(bad)?, u16at(p + 32).ok_or_else(bad)?);
        let off = u32at(p + 42).ok_or_else(bad)?;
        let name = String::from_utf8_lossy(b.get(p + 46..p + 46 + nlen).ok_or_else(bad)?).into_owned();
        p += 46 + nlen + elen + clen;
        if !b.get(off..).is_some_and(|s| s.starts_with(b"PK\x03\x04")) {
            return Err(bad());
        }
        let start = off + 30 + u16at(off + 26).ok_or_else(bad)? + u16at(off + 28).ok_or_else(bad)?;
        let Some(data) = b.get(start..start.saturating_add(csize)) else { return Err(bad()) };
        let mut out = Vec::new();
        match method {
            0 => out.extend_from_slice(&data[..data.len().min(16 << 20)]),
            8 => {
                let _ = flate2::read::DeflateDecoder::new(data).take(16 << 20).read_to_end(&mut out);
            }
            _ => continue,
        }
        unpacked += out.len();
        if unpacked > MAX_UNZIPPED {
            return Err("That zip unpacks to more than 1 GB: import it in parts".into());
        }
        each(&name, out);
    }
    Ok(())
}

/// Store advisories (replacing ones with the same id; withdrawn ones are removed): (imported, withdrawn).
pub async fn import(st: &AppState, list: Vec<Advisory>) -> R<(usize, usize)> {
    // ponytail: one transaction for the whole snapshot; fine for an admin's occasional import.
    st.db
        .call(move |c| {
            let tx = c.transaction()?;
            let (mut imported, mut withdrawn) = (0, 0);
            for a in &list {
                tx.execute("DELETE FROM osv_affected WHERE vuln_id = ?1", [&a.id])?;
                if a.withdrawn {
                    withdrawn += tx.execute("DELETE FROM osv_vulns WHERE id = ?1", [&a.id])?.max(1);
                    continue;
                }
                tx.execute(
                    "INSERT INTO osv_vulns (id, summary, severity, aliases, modified) VALUES (?1,?2,?3,?4,?5)
                     ON CONFLICT(id) DO UPDATE SET summary = ?2, severity = ?3, aliases = ?4, modified = ?5",
                    rusqlite::params![a.id, a.summary, a.severity, a.aliases, a.modified],
                )?;
                for (eco, name, versions, ranges) in &a.affected {
                    tx.execute(
                        "INSERT INTO osv_affected (vuln_id, ecosystem, name, versions, ranges) VALUES (?1,?2,?3,?4,?5)",
                        rusqlite::params![a.id, eco, name, versions, ranges],
                    )?;
                }
                imported += 1;
            }
            tx.commit()?;
            Ok((imported, withdrawn))
        })
        .await
}

/// A dependency an advisory covers.
#[derive(Serialize)]
pub struct Vuln {
    pub id: String,
    pub summary: String,
    pub severity: String,
    pub aliases: Value,
    pub ecosystem: String,
    pub name: String,
    pub version: String,
    pub fixed: Option<String>,
    pub path: String,
}

pub async fn vulnerabilities(st: &AppState, deps: Vec<Dep>) -> R<Vec<Vuln>> {
    st.db
        .call(move |c| {
            let mut s = c.prepare_cached(
                "SELECT a.vuln_id, a.versions, a.ranges, v.summary, v.severity, v.aliases FROM osv_affected a
                 JOIN osv_vulns v ON v.id = a.vuln_id WHERE a.ecosystem = ?1 AND a.name = ?2",
            )?;
            let mut out: Vec<Vuln> = Vec::new();
            for d in deps {
                let rows = s.query_map(rusqlite::params![d.ecosystem, norm_name(&d.ecosystem, &d.name)], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, String>(4)?, r.get::<_, String>(5)?))
                })?;
                for row in rows {
                    let (id, versions, ranges, summary, severity, aliases) = row?;
                    let versions: Vec<String> = serde_json::from_str(&versions).unwrap_or_default();
                    let ranges: Vec<Value> = serde_json::from_str(&ranges).unwrap_or_default();
                    let (hit, fixed) = affected(&d.version, &versions, &ranges);
                    if hit && !out.iter().any(|v| v.id == id && v.name == d.name && v.version == d.version) {
                        let aliases = serde_json::from_str(&aliases).unwrap_or_else(|_| json!([]));
                        let (ecosystem, name, version, path) = (d.ecosystem.clone(), d.name.clone(), d.version.clone(), d.path.clone());
                        out.push(Vuln { id, summary, severity, aliases, ecosystem, name, version, fixed, path });
                    }
                }
            }
            let rank = |s: &str| ["CRITICAL", "HIGH", "MODERATE", "MEDIUM", "LOW"].iter().position(|x| *x == s).unwrap_or(9);
            out.sort_by(|a, b| (rank(&a.severity), &a.id, &a.name).cmp(&(rank(&b.severity), &b.id, &b.name)));
            Ok(out)
        })
        .await
}

// ---------- SBOM ----------

fn pct(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out += &format!("%{b:02X}");
        }
    }
    out
}

/// The package URL (purl) of a dependency.
pub fn purl(d: &Dep) -> String {
    let (kind, name) = match d.ecosystem.as_str() {
        "crates.io" => ("cargo", d.name.clone()),
        "npm" => ("npm", d.name.clone()),
        "Go" => ("golang", d.name.clone()),
        "PyPI" => ("pypi", norm_name("PyPI", &d.name)),
        "RubyGems" => ("gem", d.name.clone()),
        other => (other, d.name.clone()),
    };
    let path: Vec<String> = name.split('/').map(pct).collect();
    format!("pkg:{kind}/{}@{}", path.join("/"), pct(&d.version))
}

fn iso(t: i64) -> String {
    let (y, mo, d, h, mi, _) = crate::ci::config::civil(t);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{:02}Z", t.rem_euclid(60))
}

fn uuid4() -> String {
    let mut b = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut b);
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h = hex::encode(b);
    format!("{}-{}-{}-{}-{}", &h[..8], &h[8..12], &h[12..16], &h[16..20], &h[20..])
}

/// SPDX 2.3 JSON for `repo` at `sha`.
pub fn spdx(base_url: &str, repo: &str, sha: &str, deps: &[Dep], now: i64) -> Value {
    let mut packages = vec![json!({
        "SPDXID": "SPDXRef-Repository", "name": repo, "versionInfo": sha,
        "downloadLocation": format!("{base_url}/{repo}.git"), "filesAnalyzed": false,
        "licenseConcluded": "NOASSERTION", "licenseDeclared": "NOASSERTION", "copyrightText": "NOASSERTION",
    })];
    let mut rel = vec![json!({ "spdxElementId": "SPDXRef-DOCUMENT", "relationshipType": "DESCRIBES", "relatedSpdxElement": "SPDXRef-Repository" })];
    for (i, d) in deps.iter().enumerate() {
        let id = format!("SPDXRef-Package-{}", i + 1);
        packages.push(json!({
            "SPDXID": id, "name": d.name, "versionInfo": d.version, "downloadLocation": "NOASSERTION", "filesAnalyzed": false,
            "licenseConcluded": "NOASSERTION", "licenseDeclared": "NOASSERTION", "copyrightText": "NOASSERTION",
            "externalRefs": [{ "referenceCategory": "PACKAGE-MANAGER", "referenceType": "purl", "referenceLocator": purl(d) }],
        }));
        rel.push(json!({ "spdxElementId": "SPDXRef-Repository", "relationshipType": "DEPENDS_ON", "relatedSpdxElement": id }));
    }
    json!({
        "spdxVersion": "SPDX-2.3", "dataLicense": "CC0-1.0", "SPDXID": "SPDXRef-DOCUMENT",
        "name": format!("{repo}@{}", &sha[..sha.len().min(12)]),
        "documentNamespace": format!("{base_url}/{repo}/sbom/{sha}-{}", uuid4()),
        "creationInfo": { "created": iso(now), "creators": [format!("Tool: gitter-{}", env!("CARGO_PKG_VERSION"))] },
        "packages": packages, "relationships": rel,
    })
}

/// CycloneDX 1.5 JSON for `repo` at `sha`.
pub fn cyclonedx(repo: &str, sha: &str, deps: &[Dep], now: i64) -> Value {
    let comps: Vec<Value> = deps.iter().map(|d| {
        let p = purl(d);
        json!({ "type": "library", "bom-ref": p, "name": d.name, "version": d.version, "purl": p })
    }).collect();
    let refs: Vec<String> = deps.iter().map(purl).collect();
    json!({
        "bomFormat": "CycloneDX", "specVersion": "1.5", "serialNumber": format!("urn:uuid:{}", uuid4()), "version": 1,
        "metadata": {
            "timestamp": iso(now),
            "tools": { "components": [{ "type": "application", "name": "gitter", "version": env!("CARGO_PKG_VERSION") }] },
            "component": { "type": "application", "bom-ref": "repository", "name": repo, "version": sha },
        },
        "components": comps,
        "dependencies": [{ "ref": "repository", "dependsOn": refs }],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_like_people_expect() {
        use Ordering::*;
        for (a, b, o) in [
            ("1.6.0", "1.6.1", Less),
            ("1.10.0", "1.9.9", Greater),
            ("v0.3.7", "0.3.7", Equal),
            ("1.0", "1.0.0", Equal),
            ("1.0.0-rc.1", "1.0.0", Less),
            ("1.0rc1", "1.0", Less),
            ("2.0.0+build", "2.0.0", Equal),
            ("0.6.14", "0.6.3", Greater),
        ] {
            assert_eq!(cmp_versions(a, b), o, "{a} vs {b}");
        }
    }

    #[test]
    fn osv_ranges() {
        let r = vec![json!({ "type": "SEMVER", "events": [{ "introduced": "0.6.3" }, { "fixed": "0.6.14" }, { "introduced": "1.0.0" }, { "fixed": "1.6.1" }] })];
        assert_eq!(affected("1.6.0", &[], &r), (true, Some("1.6.1".into())));
        assert_eq!(affected("0.6.5", &[], &r), (true, Some("0.6.14".into())));
        assert_eq!(affected("0.7.0", &[], &r), (false, None));
        assert_eq!(affected("1.6.1", &[], &r), (false, None));
        assert_eq!(affected("0.1.0", &[], &r), (false, None));
        let open = vec![json!({ "type": "ECOSYSTEM", "events": [{ "introduced": "0" }, { "last_affected": "2.0" }] })];
        assert_eq!(affected("2.0", &[], &open), (true, None));
        assert_eq!(affected("2.1", &[], &open), (false, None));
        assert!(affected("4.17.20", &["4.17.20".into()], &[]).0);
    }

    #[test]
    fn lockfile_formats() {
        let yarn1 = "# yarn\n\"@babel/core@^7.0.0\", \"@babel/core@^7.1.0\":\n  version \"7.22.0\"\n  resolved \"x\"\n\nlodash@^4:\n  version \"4.17.21\"\n";
        assert_eq!(parse("yarn.lock", yarn1), [("@babel/core".into(), "7.22.0".into()), ("lodash".into(), "4.17.21".into())]);
        let berry = "__metadata:\n  version: 6\n\n\"left-pad@npm:^1.3.0\":\n  version: 1.3.0\n\n\"app@workspace:.\":\n  version: 0.0.0-use.local\n";
        assert_eq!(parse("yarn.lock", berry), [("left-pad".to_string(), "1.3.0".to_string())]);
        let pnpm = "lockfileVersion: '9.0'\n\nimporters:\n  .:\n    dependencies: {}\n\npackages:\n\n  '@types/node@20.1.0':\n    resolution: {}\n\n  ms@2.1.3:\n    resolution: {}\n  /debug/4.3.4:\n    resolution: {}\n  /chalk@5.0.0(supports-color@9.0.0):\n    resolution: {}\n\nsnapshots:\n\n  ms@9.9.9: {}\n";
        assert_eq!(
            parse("pnpm-lock.yaml", pnpm),
            [("@types/node", "20.1.0"), ("ms", "2.1.3"), ("debug", "4.3.4"), ("chalk", "5.0.0")].map(|(a, b)| (a.to_string(), b.to_string()))
        );
        let gem = "GEM\n  remote: https://rubygems.org/\n  specs:\n    rack (2.2.3)\n      ruby2_keywords\n    nokogiri (1.13.10-x86_64-linux)\n\nPLATFORMS\n  ruby\n";
        assert_eq!(parse("Gemfile.lock", gem), [("rack".to_string(), "2.2.3".to_string()), ("nokogiri".to_string(), "1.13.10".to_string())]);
        let poetry = "[[package]]\nname = \"requests\"\nversion = \"2.31.0\"\n\n[package.dependencies]\nidna = \">=2.5\"\n\n[[package]]\nname = \"idna\"\nversion = \"3.4\"\n\n[metadata]\nlock-version = \"2.0\"\n";
        assert_eq!(parse("poetry.lock", poetry), [("requests".to_string(), "2.31.0".to_string()), ("idna".to_string(), "3.4".to_string())]);
        let npm1 = r#"{"lockfileVersion": 1, "dependencies": {"a": {"version": "1.0.0", "dependencies": {"b": {"version": "2.0.0"}}}}}"#;
        assert_eq!(parse("package-lock.json", npm1), [("a".to_string(), "1.0.0".to_string()), ("b".to_string(), "2.0.0".to_string())]);
    }

    #[test]
    fn purls_and_zip_errors() {
        let d = |e: &str, n: &str, v: &str| Dep { ecosystem: e.into(), name: n.into(), version: v.into(), path: String::new() };
        assert_eq!(purl(&d("npm", "@babel/core", "7.0.0")), "pkg:npm/%40babel/core@7.0.0");
        assert_eq!(purl(&d("PyPI", "Foo_Bar", "1.0+local")), "pkg:pypi/foo-bar@1.0%2Blocal");
        assert_eq!(purl(&d("Go", "golang.org/x/text", "v0.3.7")), "pkg:golang/golang.org/x/text@v0.3.7");
        assert!(read_snapshot(b"PK\x03\x04garbage").is_err());
        assert!(read_snapshot(b"[]").is_err());
        assert_eq!(iso(0), "1970-01-01T00:00:00Z");
        assert_eq!(uuid4().len(), 36);
    }
}
