//! Test reports and coverage: the runner sends the files a job's `reports`
//! (JUnit XML) and `coverage` (Cobertura XML or LCOV) patterns match, and the
//! server reads them into results: counts, the failed tests with message and
//! file:line, a coverage percentage, and flaky tests (failed, then passed on
//! the same commit).
//!
//! The XML reader is a small one for this: no DTDs (so no entity expansion,
//! internal or external), only the five predefined entities and character
//! references, bounded size, depth and element count.

use std::io::Read;
use std::path::Path;

use rusqlite::params;

use crate::AppState;
use crate::db::now;
use crate::error::{AppError, R};

/// What the runner sends at most (compressed), and what one file may be.
pub const UPLOAD_CAP: u64 = 32 << 20;
const FILE_CAP: usize = 16 << 20;
/// All files of one upload together, uncompressed.
const TOTAL_CAP: u64 = 64 << 20;
const MAX_FILES: usize = 200;
const MAX_DEPTH: usize = 64;
const MAX_ELEMENTS: usize = 1_000_000;
const MAX_MESSAGE: usize = 4000;
/// Rows kept per job (failed and flaky tests; passing ones are only counted).
const MAX_ROWS: usize = 1000;

enum Ev<'a> {
    Start(&'a str, Vec<(&'a str, String)>),
    End(&'a str),
    Text(String),
}

/// `&lt;` and friends and `&#…;`; any other `&name;` stays as it is.
fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        // Bytes: ';' is ASCII, so its index is a character boundary.
        let end = rest.as_bytes()[..rest.len().min(12)].iter().position(|b| *b == b';');
        let ch = end.and_then(|e| match &rest[1..e] {
            "lt" => Some('<'),
            "gt" => Some('>'),
            "amp" => Some('&'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            n => {
                let code = match n.strip_prefix("#x").or_else(|| n.strip_prefix("#X")) {
                    Some(h) => u32::from_str_radix(h, 16).ok(),
                    None => n.strip_prefix('#').and_then(|d| d.parse().ok()),
                };
                code.and_then(char::from_u32)
            }
        });
        match (ch, end) {
            (Some(c), Some(e)) => {
                out.push(c);
                rest = &rest[e + 1..];
            }
            _ => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn is_name(c: char) -> bool {
    c.is_alphanumeric() || "_-.:".contains(c)
}

/// Walk an XML document, calling `f` per event; `f` returns false to stop early.
/// A self-closing element gives a Start and an End.
fn events<'a>(src: &'a str, mut f: impl FnMut(Ev<'a>) -> bool) -> Result<(), String> {
    let mut s = src.strip_prefix('\u{feff}').unwrap_or(src);
    let (mut stack, mut elements, mut root_done): (Vec<&str>, usize, bool) = (Vec::new(), 0, false);
    let after = |s: &'a str, pat: &str, what: &str| -> Result<&'a str, String> { s.find(pat).map(|i| &s[i + pat.len()..]).ok_or(format!("unclosed {what}")) };
    while !s.is_empty() {
        let Some(lt) = s.find('<') else {
            if !s.trim().is_empty() && stack.is_empty() {
                return Err("text outside the document".into());
            }
            if !stack.is_empty() && !f(Ev::Text(unescape(s))) {
                return Ok(());
            }
            break;
        };
        if lt > 0 {
            let text = &s[..lt];
            if stack.is_empty() && !text.trim().is_empty() {
                return Err("text outside the document".into());
            }
            if !stack.is_empty() && !f(Ev::Text(unescape(text))) {
                return Ok(());
            }
        }
        s = &s[lt..];
        if let Some(r) = s.strip_prefix("<?") {
            s = after(r, "?>", "<?")?;
        } else if let Some(r) = s.strip_prefix("<!--") {
            s = after(r, "-->", "comment")?;
        } else if let Some(r) = s.strip_prefix("<![CDATA[") {
            let end = r.find("]]>").ok_or("unclosed CDATA")?;
            if stack.is_empty() {
                return Err("text outside the document".into());
            }
            if !f(Ev::Text(r[..end].to_string())) {
                return Ok(());
            }
            s = &r[end + 3..];
        } else if s.starts_with("<!") {
            return Err("DTDs and entity declarations aren't allowed".into());
        } else if let Some(r) = s.strip_prefix("</") {
            let end = r.find('>').ok_or("unclosed end tag")?;
            let name = r[..end].trim();
            if stack.pop() != Some(name) {
                return Err(format!("</{name}> doesn't match"));
            }
            root_done |= stack.is_empty();
            if !f(Ev::End(name)) {
                return Ok(());
            }
            s = &r[end + 1..];
        } else {
            let r = &s[1..];
            let n = r.find(|c: char| !is_name(c)).unwrap_or(r.len());
            let name = &r[..n];
            if name.is_empty() {
                return Err("a '<' that starts no tag".into());
            }
            if root_done && stack.is_empty() {
                return Err("more than one root element".into());
            }
            elements += 1;
            if elements > MAX_ELEMENTS {
                return Err(format!("more than {MAX_ELEMENTS} elements"));
            }
            if stack.len() >= MAX_DEPTH {
                return Err(format!("nested deeper than {MAX_DEPTH}"));
            }
            let mut rest = &r[n..];
            let mut attrs = Vec::new();
            let closed = loop {
                rest = rest.trim_start();
                if let Some(x) = rest.strip_prefix("/>") {
                    rest = x;
                    break true;
                }
                if let Some(x) = rest.strip_prefix('>') {
                    rest = x;
                    break false;
                }
                let k = rest.find(|c: char| !is_name(c)).unwrap_or(rest.len());
                if k == 0 {
                    return Err(format!("a broken attribute in <{name}>"));
                }
                let key = &rest[..k];
                let v = rest[k..].trim_start().strip_prefix('=').ok_or(format!("attribute {key} has no value"))?.trim_start();
                let q = v.chars().next().filter(|c| *c == '"' || *c == '\'').ok_or(format!("attribute {key} isn't quoted"))?;
                let end = v[1..].find(q).ok_or(format!("attribute {key} isn't closed"))?;
                attrs.push((key, unescape(&v[1..1 + end])));
                rest = &v[end + 2..];
            };
            stack.push(name);
            if !f(Ev::Start(name, attrs)) {
                return Ok(());
            }
            if closed {
                stack.pop();
                root_done |= stack.is_empty();
                if !f(Ev::End(name)) {
                    return Ok(());
                }
            }
            s = rest;
        }
    }
    if !stack.is_empty() {
        return Err(format!("<{}> isn't closed", stack.last().copied().unwrap_or_default()));
    }
    if !root_done {
        return Err("no XML element".into());
    }
    Ok(())
}

fn attr<'a>(attrs: &'a [(&str, String)], k: &str) -> Option<&'a str> {
    attrs.iter().find(|a| a.0 == k).map(|a| a.1.as_str())
}

/// A workspace-relative path: `./x` → `x`; an absolute one only when it runs
/// through a job workspace (`/workspace/…` in containers, `…/job-N/…` on runners).
fn relative(p: &str) -> Option<String> {
    let p = p.trim();
    let p = if p.starts_with('/') {
        if let Some(i) = p.find("/workspace/") {
            &p[i + 11..]
        } else {
            let i = p.match_indices("/job-").find(|(i, _)| {
                let rest = &p[i + 5..];
                let d = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
                d > 0 && rest[d..].starts_with('/')
            })?;
            let rest = &p[i.0 + 5..];
            &rest[rest.find('/')? + 1..]
        }
    } else {
        p
    };
    let p = p.trim_start_matches("./");
    (!p.is_empty() && p.len() <= 1024 && !p.starts_with('/') && !p.split('/').any(|s| s == "..")).then(|| p.to_string())
}

/// The first `path/to/file.ext:LINE` in a failure's text.
fn locate(text: &str) -> Option<(String, i64)> {
    for tok in text.split(|c: char| c.is_whitespace() || "()'\",<>[]".contains(c)).take(5000) {
        let mut ps = tok.trim_end_matches([':', '.']).split(':');
        let (Some(file), Some(line)) = (ps.next(), ps.next()) else { continue };
        let has_ext = file.rsplit('/').next().is_some_and(|n| n.rfind('.').is_some_and(|i| i > 0 && i + 1 < n.len()));
        if has_ext && (1..=9).contains(&line.len()) && line.bytes().all(|b| b.is_ascii_digit())
            && let (Some(f), Ok(n)) = (relative(file), line.parse::<i64>())
            && n > 0
        {
            return Some((f, n));
        }
    }
    None
}

/// One test case of a report.
#[derive(Debug, Clone)]
pub struct Case {
    pub suite: String,
    pub name: String,
    /// "passed", "failed" or "skipped".
    pub status: &'static str,
    pub message: String,
    pub file: Option<String>,
    pub line: Option<i64>,
    /// It failed and passed within this report (a runner's own re-runs).
    pub flaky: bool,
}

/// Read a JUnit XML report (`<testsuites>`/`<testsuite>`/`<testcase>`).
pub fn parse_junit(data: &[u8]) -> Result<Vec<Case>, String> {
    if data.len() > FILE_CAP {
        return Err("the report is over 16 MB".into());
    }
    let src = std::str::from_utf8(data).map_err(|_| "the report isn't UTF-8")?;
    // The case being read, its failure's message attribute, and its failure text.
    let (mut cases, mut suites, mut cur): (Vec<Case>, Vec<String>, Option<(Case, String, String)>) = (vec![], vec![], None);
    let mut capture = false;
    events(src, |ev| {
        match ev {
            Ev::Start("testsuite", a) => suites.push(attr(&a, "name").unwrap_or("").to_string()),
            Ev::Start("testcase", a) => {
                let suite = attr(&a, "classname").map(String::from).or_else(|| suites.last().cloned()).unwrap_or_default();
                let case = Case {
                    suite,
                    name: attr(&a, "name").unwrap_or("").chars().take(500).collect(),
                    status: "passed",
                    message: String::new(),
                    file: attr(&a, "file").and_then(relative),
                    line: attr(&a, "line").and_then(|l| l.trim().parse().ok()).filter(|l| *l > 0),
                    flaky: false,
                };
                cur = Some((case, String::new(), String::new()));
            }
            Ev::Start(k @ ("failure" | "error" | "skipped" | "flakyFailure" | "flakyError" | "rerunFailure" | "rerunError"), a) => {
                if let Some((c, msg, _)) = cur.as_mut() {
                    match k {
                        "failure" | "error" => {
                            c.status = "failed";
                            if msg.is_empty() {
                                *msg = attr(&a, "message").unwrap_or("").to_string();
                            }
                            capture = true;
                        }
                        "skipped" if c.status != "failed" => c.status = "skipped",
                        "skipped" => {}
                        _ => c.flaky = true,
                    }
                }
            }
            Ev::Text(t) if capture => {
                if let Some((_, _, text)) = cur.as_mut()
                    && text.len() < MAX_MESSAGE
                {
                    text.push_str(&t);
                }
            }
            Ev::End("failure" | "error") => capture = false,
            Ev::End("testcase") => {
                if let Some((mut c, msg, text)) = cur.take() {
                    let text = text.trim();
                    if c.status == "failed" {
                        c.flaky = false;
                        let m: Vec<&str> = [msg.trim(), text].into_iter().filter(|s| !s.is_empty()).collect();
                        c.message = m.join("\n").chars().take(MAX_MESSAGE).collect();
                        match (c.file.is_some(), c.line.is_some(), locate(text)) {
                            (false, _, Some((f, l))) => (c.file, c.line) = (Some(f), Some(l)),
                            (true, false, Some((f, l))) if c.file.as_deref() == Some(f.as_str()) => c.line = Some(l),
                            _ => {}
                        }
                    }
                    cases.push(c);
                }
            }
            Ev::End("testsuite") => {
                suites.pop();
            }
            _ => {}
        }
        true
    })?;
    Ok(cases)
}

/// Covered and coverable lines from a Cobertura XML (its root's counts, or
/// `line-rate` as a share of 100) or an LCOV file (its LH/LF totals).
pub fn parse_coverage(data: &[u8]) -> Option<(f64, f64)> {
    let src = std::str::from_utf8(data.get(..data.len().min(FILE_CAP))?).ok()?;
    if src.trim_start_matches('\u{feff}').trim_start().starts_with('<') {
        let mut out = None;
        let _ = events(src, |ev| {
            if let Ev::Start(name, a) = ev {
                if name == "coverage" {
                    let num = |k: &str| attr(&a, k).and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| v.is_finite() && *v >= 0.0);
                    out = match (num("lines-covered"), num("lines-valid"), num("line-rate")) {
                        (Some(c), Some(v), _) if v > 0.0 && c <= v => Some((c, v)),
                        (_, _, Some(r)) if r <= 1.0 => Some((r * 100.0, 100.0)),
                        _ => None,
                    };
                }
                return false;
            }
            true
        });
        return out;
    }
    let (mut hit, mut found) = (0f64, 0f64);
    for l in src.lines() {
        if let Some(v) = l.strip_prefix("LF:").and_then(|v| v.trim().parse::<u64>().ok()) {
            found += v as f64;
        } else if let Some(v) = l.strip_prefix("LH:").and_then(|v| v.trim().parse::<u64>().ok()) {
            hit += v as f64;
        }
    }
    (found > 0.0 && hit <= found).then_some((hit, found))
}

/// What an upload held.
struct Parsed {
    cases: Vec<Case>,
    coverage: Option<(f64, f64)>,
}

/// Read the archive the runner sent: `junit/…` and `coverage/…` entries, bounded.
fn unpack(file: &Path) -> std::io::Result<Parsed> {
    let f = std::fs::File::open(file)?;
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(f));
    let (mut out, mut total, mut n) = (Parsed { cases: vec![], coverage: None }, 0u64, 0);
    for e in tar.entries()? {
        let e = e?;
        n += 1;
        if n > MAX_FILES || total > TOTAL_CAP {
            break;
        }
        if e.header().entry_type() != tar::EntryType::Regular {
            continue;
        }
        let name = e.path()?.to_string_lossy().into_owned();
        let mut data = Vec::new();
        e.take(FILE_CAP as u64 + 1).read_to_end(&mut data)?;
        total += data.len() as u64;
        if data.len() > FILE_CAP {
            continue;
        }
        if name.starts_with("junit/") {
            // A broken report is skipped: the others still count.
            out.cases.extend(parse_junit(&data).unwrap_or_default());
        } else if name.starts_with("coverage/")
            && let Some((h, f)) = parse_coverage(&data)
        {
            let (h0, f0) = out.coverage.unwrap_or((0.0, 0.0));
            out.coverage = Some((h0 + h, f0 + f));
        }
    }
    Ok(out)
}

/// Store job `job`'s reports (an archive from the runner): counts, coverage, the
/// failed tests, and passing tests that failed before on this commit (flaky).
pub async fn store(st: &AppState, job: i64, file: &Path) -> R<()> {
    let f = file.to_path_buf();
    let parsed = tokio::task::spawn_blocking(move || unpack(&f)).await.map_err(AppError::internal)?.map_err(|_| AppError::bad("Couldn't read the reports"))?;
    st.db
        .call(move |c| {
            let tx = c.transaction()?;
            let (repo, sha, run_number, job_name): (i64, String, i64, String) = tx.query_row(
                "SELECT r.repo_id, r.sha, r.number, j.name FROM ci_jobs j JOIN ci_runs r ON r.id = j.run_id WHERE j.id = ?1",
                [job],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )?;
            let mut cases = parsed.cases;
            {
                let mut before = tx.prepare_cached(
                    "SELECT EXISTS (SELECT 1 FROM ci_test_results WHERE repo_id = ?1 AND sha = ?2 AND job = ?3 AND suite = ?4 AND name = ?5 AND status = 'failed')",
                )?;
                for c in cases.iter_mut().filter(|c| c.status == "passed") {
                    c.flaky = c.flaky || before.query_row(params![repo, sha, job_name, c.suite, c.name], |r| r.get(0))?;
                }
            }
            // This attempt replaces the job's earlier ones (they were only needed for the check above).
            tx.execute("DELETE FROM ci_test_results WHERE job_id = ?1", [job])?;
            let count = |s: &str| cases.iter().filter(|c| c.status == s).count() as i64;
            let flaky = cases.iter().filter(|c| c.flaky).count() as i64;
            let (hit, total) = parsed.coverage.map_or((None, None), |(h, t)| (Some(h), Some(t)));
            tx.execute(
                "INSERT OR REPLACE INTO ci_job_reports(job_id, tests, failed, skipped, flaky, cov_hit, cov_total) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![job, cases.len() as i64, count("failed"), count("skipped"), flaky, hit, total],
            )?;
            let t = now();
            let mut ins = tx.prepare_cached(
                "INSERT INTO ci_test_results(job_id, repo_id, sha, job, suite, name, status, message, file, line, flaky) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            )?;
            let mut mark = tx.prepare_cached(
                "INSERT INTO ci_flaky_tests(repo_id, name, flips, last_sha, last_run, last_seen) VALUES (?1, ?2, 1, ?3, ?4, ?5)
                 ON CONFLICT(repo_id, name) DO UPDATE SET flips = flips + 1, last_sha = excluded.last_sha, last_run = excluded.last_run, last_seen = excluded.last_seen",
            )?;
            for c in cases.iter().filter(|c| c.status == "failed" || c.flaky).take(MAX_ROWS) {
                ins.execute(params![job, repo, sha, job_name, c.suite, c.name, c.status, c.message, c.file, c.line, c.flaky])?;
                if c.flaky {
                    let full = if c.suite.is_empty() { c.name.clone() } else { format!("{}::{}", c.suite, c.name) };
                    mark.execute(params![repo, full, sha, run_number, t])?;
                }
            }
            drop((ins, mark));
            tx.commit()?;
            Ok(())
        })
        .await
}
