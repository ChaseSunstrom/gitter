//! Secret scanning: the rules, a scanner for added lines and `git diff`
//! output, push protection (git pushes through a pre-receive hook, web commits
//! before they're written) and the on-demand scan of the default branch.
//!
//! A found secret is never stored or printed whole: only its kind, place, a
//! masked form (a few characters) and a fingerprint, which is what a one-time
//! allowance names.

use std::collections::HashSet;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::AppState;
use crate::auth::User;
use crate::db::now;
use crate::error::{AppError, R};
use crate::git::Change;
use crate::repo::Repo;

/// Findings kept per push, commit or scan; the rest are counted.
pub const MAX_FINDINGS: usize = 100;
/// Longest part of one line that is scanned (minified files).
const MAX_LINE: usize = 64 * 1024;
/// Most diff output one push or scan reads.
const HOOK_CAP: u64 = 512 << 20;
const SCAN_CAP: usize = 64 << 20;
/// How long an allowance waits for the push or commit that uses it.
pub const ALLOW_FOR: i64 = 3 * 3600;

/// A secret found on one line.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Finding {
    pub kind: String,
    pub path: String,
    pub line: usize,
    pub masked: String,
    pub fingerprint: String,
    #[serde(default)]
    pub commit: String,
}

// ---------- rules ----------

/// A token with a fixed prefix and a run of `body` characters after it.
struct Token {
    kind: &'static str,
    prefix: &'static str,
    body: fn(u8) -> bool,
    min: usize,
    max: usize,
}

fn upper(b: u8) -> bool {
    b.is_ascii_uppercase() || b.is_ascii_digit()
}
fn alnum(b: u8) -> bool {
    b.is_ascii_alphanumeric()
}
fn alnum_(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}
fn b64url(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
}
fn dashed(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-'
}

const fn t(kind: &'static str, prefix: &'static str, body: fn(u8) -> bool, min: usize, max: usize) -> Token {
    Token { kind, prefix, body, min, max }
}

const TOKENS: &[Token] = &[
    t("AWS access key ID", "AKIA", upper, 16, 16),
    t("AWS access key ID", "ASIA", upper, 16, 16),
    t("Gitter access token", crate::auth::TOKEN_PREFIX, b64url, 43, 43),
    t("Gitter runner token", crate::ci::http::RUNNER_TOKEN_PREFIX, b64url, 43, 43),
    t("Gitter CI job token", crate::ci::queue::JOB_TOKEN_PREFIX, b64url, 43, 43),
    t("GitHub token", "ghp_", alnum, 36, 36),
    t("GitHub token", "gho_", alnum, 36, 36),
    t("GitHub token", "ghu_", alnum, 36, 36),
    t("GitHub token", "ghs_", alnum, 36, 36),
    t("GitHub token", "ghr_", alnum, 36, 36),
    t("GitHub token", "github_pat_", alnum_, 82, 82),
    t("GitLab token", "glpat-", b64url, 20, 64),
    t("Slack token", "xoxb-", dashed, 10, 250),
    t("Slack token", "xoxp-", dashed, 10, 250),
    t("Slack token", "xoxa-", dashed, 10, 250),
    t("Slack token", "xoxe-", dashed, 10, 250),
    t("Slack token", "xoxr-", dashed, 10, 250),
    t("Slack token", "xoxs-", dashed, 10, 250),
    t("Stripe live key", "sk_live_", alnum, 24, 247),
    t("Stripe live key", "rk_live_", alnum, 24, 247),
    t("Google API key", "AIza", b64url, 35, 35),
    t("npm token", "npm_", alnum, 36, 36),
    t("Anthropic API key", "sk-ant-", b64url, 32, 300),
    t("OpenAI API key", "sk-proj-", b64url, 32, 300),
    t("PyPI token", "pypi-AgEIcHlwaS5vcmc", b64url, 50, 300),
];

/// Bytes a token or a private key header can start with (a one-pass prefilter).
const fn first_bytes() -> [bool; 256] {
    let mut f = [false; 256];
    let mut i = 0;
    while i < TOKENS.len() {
        f[TOKENS[i].prefix.as_bytes()[0] as usize] = true;
        i += 1;
    }
    f
}
static FIRST: [bool; 256] = first_bytes();

/// Names that make the value assigned to them a secret, when it looks random.
const KEYWORDS: &[&str] = &[
    "password", "passwd", "secret", "token", "api_key", "apikey", "api-key", "access_key", "private_key", "credential",
];
const PLACEHOLDERS: &[&str] = &[
    "example", "changeme", "change_me", "placeholder", "dummy", "sample", "xxxx", "your_", "your-", "redacted", "****", "${", "{{", "<", ">", "%s",
];

fn word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Prefixed tokens on a line: (kind, token).
fn tokens(line: &[u8]) -> Vec<(&'static str, &[u8])> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < line.len() {
        if !FIRST[line[i] as usize] || (i > 0 && word(line[i - 1])) {
            i += 1;
            continue;
        }
        let mut hit = None;
        for t in TOKENS {
            if !line[i..].starts_with(t.prefix.as_bytes()) {
                continue;
            }
            let start = i + t.prefix.len();
            let n = line[start..].iter().take(t.max + 1).take_while(|b| (t.body)(**b)).count();
            if n >= t.min && n <= t.max {
                let tok = &line[i..start + n];
                // The AWS documentation's sample keys.
                if !tok.windows(7).any(|w| w == b"EXAMPLE") {
                    hit = Some((t.kind, tok));
                }
                break;
            }
        }
        match hit {
            Some((kind, tok)) => {
                out.push((kind, tok));
                i += tok.len();
            }
            None => i += 1,
        }
    }
    out
}

/// A PEM private key header on the line: (header, the rest of the line after it).
fn key_header(line: &[u8]) -> Option<(&[u8], &[u8])> {
    let at = line.windows(11).position(|w| w == b"-----BEGIN ")?;
    let rest = &line[at + 11..];
    let end = rest.windows(5).position(|w| w == b"-----")?;
    let label = &rest[..end];
    let ok = label.len() < 40
        && label.iter().all(|b| b.is_ascii_uppercase() || *b == b' ')
        && (label.ends_with(b"PRIVATE KEY") || label.ends_with(b"PRIVATE KEY BLOCK"));
    ok.then(|| (&line[at..at + 11 + end + 5], &rest[end + 5..]))
}

/// The start of base64 key material (after a header, or on the next line).
fn key_material(s: &[u8]) -> Option<&[u8]> {
    let mut s = s;
    loop {
        let t = s.trim_ascii_start();
        let t = t.strip_prefix(b"\\n").or_else(|| t.strip_prefix(b"\"")).or_else(|| t.strip_prefix(b"'")).unwrap_or(t);
        if t.len() == s.len() {
            break;
        }
        s = t;
    }
    let n = s.iter().take_while(|b| b.is_ascii_alphanumeric() || **b == b'+' || **b == b'/' || **b == b'=').count();
    (n >= 20).then(|| &s[..n])
}

/// Shannon entropy in bits per character.
fn entropy(s: &[u8]) -> f64 {
    let mut n = [0u32; 256];
    for b in s {
        n[*b as usize] += 1;
    }
    let l = s.len() as f64;
    n.iter().filter(|c| **c > 0).map(|&c| {
        let p = f64::from(c) / l;
        -p * p.log2()
    }).sum()
}

fn random_enough(v: &[u8]) -> bool {
    let low = v.to_ascii_lowercase();
    let has = |f: fn(&u8) -> bool| v.iter().any(f);
    (16..=200).contains(&v.len())
        && has(u8::is_ascii_digit)
        && has(u8::is_ascii_alphabetic)
        // Mixed case, or long (hex): `chacha20poly1305` in prose isn't a password.
        && ((has(u8::is_ascii_uppercase) && has(u8::is_ascii_lowercase)) || v.len() >= 32)
        && !v.iter().any(|b| b.is_ascii_whitespace())
        && !v.starts_with(b"$")
        && !PLACEHOLDERS.iter().any(|p| low.windows(p.len()).any(|w| w == p.as_bytes()))
        // Random hex carries 4 bits a character at most.
        && entropy(v) >= if v.len() >= 32 { 3.0 } else { 3.5 }
}

/// A random-looking value assigned to a secret-sounding name
/// (`api_key = "…"`, `"password": "…"`, `DB_TOKEN=…`).
fn assignment(line: &[u8]) -> Option<&[u8]> {
    if !line.iter().any(|b| *b == b'=' || *b == b':') {
        return None;
    }
    let low = line.to_ascii_lowercase();
    for kw in KEYWORDS {
        let mut from = 0;
        while let Some(p) = low[from..].windows(kw.len()).position(|w| w == kw.as_bytes()) {
            let at = from + p;
            from = at + 1;
            let mut j = at + kw.len();
            while j < line.len() && (word(line[j]) || line[j] == b'-') {
                j += 1;
            }
            if j < line.len() && matches!(line[j], b'"' | b'\'') {
                j += 1;
            }
            while j < line.len() && line[j] == b' ' {
                j += 1;
            }
            let rest = &line[j..];
            let op = if rest.starts_with(b"=>") || rest.starts_with(b":=") {
                2
            } else if rest.starts_with(b"==") {
                0
            } else if rest.starts_with(b"=") || rest.starts_with(b":") {
                1
            } else {
                0
            };
            if op == 0 {
                continue;
            }
            let rest = rest[op..].trim_ascii_start();
            let value = match rest.first() {
                Some(q @ (b'"' | b'\'' | b'`')) => {
                    let Some(end) = rest[1..].iter().position(|b| b == q) else { continue };
                    &rest[1..1 + end]
                }
                _ => {
                    let n = rest.iter().take_while(|b| b.is_ascii_alphanumeric() || b"+/=_-.~".contains(b)).count();
                    // A call, an index or a field (`get_token()`, `env["X"]`), not a value.
                    if rest.get(n).is_some_and(|b| b"([{".contains(b)) {
                        continue;
                    }
                    &rest[..n]
                }
            };
            if random_enough(value) {
                return Some(value);
            }
        }
    }
    None
}

/// `AKIA…7Q`: the start and the last two characters.
pub fn mask(secret: &str) -> String {
    let c: Vec<char> = secret.chars().collect();
    if c.len() < 12 {
        return format!("{}…", c.iter().take(2).collect::<String>());
    }
    format!("{}…{}", c[..4].iter().collect::<String>(), c[c.len() - 2..].iter().collect::<String>())
}

/// What an allowance names: the kind, the file and the secret, hashed.
pub fn fingerprint(kind: &str, path: &str, secret: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(kind.as_bytes());
    h.update([0]);
    h.update(path.as_bytes());
    h.update([0]);
    h.update(secret);
    hex::encode(&h.finalize()[..10])
}

// ---------- scanners ----------

/// Scans the added lines of one file after another, in order.
#[derive(Default)]
pub struct Scanner {
    pub findings: Vec<Finding>,
    /// Findings past MAX_FINDINGS, not kept.
    pub more: usize,
    path: String,
    commit: String,
    /// A private key header waiting for its key material on the next line: (line, header).
    key: Option<(usize, String)>,
    seen: HashSet<String>,
}

impl Scanner {
    pub fn file(&mut self, path: &str) {
        self.path = path.to_string();
        self.key = None;
    }

    fn found(&mut self, kind: &str, line: usize, masked: String, secret: &[u8]) {
        let fingerprint = fingerprint(kind, &self.path, secret);
        if self.seen.contains(&fingerprint) {
            return;
        }
        // Repeats are only told apart up to a bound (a file of a million tokens).
        if self.seen.len() < 100 * MAX_FINDINGS {
            self.seen.insert(fingerprint.clone());
        }
        if self.findings.len() >= MAX_FINDINGS {
            self.more += 1;
            return;
        }
        let (kind, path, commit) = (kind.to_string(), self.path.clone(), self.commit.clone());
        self.findings.push(Finding { kind, path, line, masked, fingerprint, commit });
    }

    /// One added line, numbered as in the new file.
    pub fn line(&mut self, n: usize, text: &[u8]) {
        let text = &text[..text.len().min(MAX_LINE)];
        if let Some((at, header)) = self.key.take()
            && at + 1 == n
            && let Some(m) = key_material(text)
        {
            self.found("Private key", at, header, m);
        }
        if let Some((header, rest)) = key_header(text) {
            let header = String::from_utf8_lossy(header).into_owned();
            match key_material(rest) {
                Some(m) => self.found("Private key", n, header, m),
                None => self.key = Some((n, header)),
            }
        }
        let toks = tokens(text);
        for (kind, tok) in &toks {
            self.found(kind, n, mask(&String::from_utf8_lossy(tok)), tok);
        }
        // `aws_key = AKIA…` is one finding, the token.
        if toks.is_empty()
            && let Some(v) = assignment(text)
        {
            self.found("Secret in an assignment", n, mask(&String::from_utf8_lossy(v)), v);
        }
    }
}

/// The lines of `new` that `old` didn't have (each old line matches once), numbered from 1.
pub fn added_lines<'a>(old: &[u8], new: &'a [u8]) -> Vec<(usize, &'a [u8])> {
    let mut have: std::collections::HashMap<&[u8], usize> = std::collections::HashMap::new();
    for l in old.split(|b| *b == b'\n') {
        *have.entry(l.strip_suffix(b"\r").unwrap_or(l)).or_default() += 1;
    }
    let mut out = Vec::new();
    for (i, l) in new.split(|b| *b == b'\n').enumerate() {
        let l = l.strip_suffix(b"\r").unwrap_or(l);
        match have.get_mut(l) {
            Some(c) if *c > 0 => *c -= 1,
            _ if l.is_empty() => {}
            _ => out.push((i + 1, l)),
        }
    }
    out
}

/// Feeds `git log -p -U0 --format=%x00%H` or `git diff -U0` output to a
/// Scanner, a line at a time.
#[derive(Default)]
pub struct DiffScan {
    pub scan: Scanner,
    line: usize,
    in_hunk: bool,
}

impl DiffScan {
    pub fn feed(&mut self, l: &[u8]) {
        let l = l.strip_suffix(b"\n").unwrap_or(l);
        if let Some(sha) = l.strip_prefix(b"\0") {
            self.scan.commit = String::from_utf8_lossy(sha).into_owned();
            self.in_hunk = false;
            return;
        }
        // Inside a hunk every content line starts with + - space or \, so this is a header.
        if l.starts_with(b"diff --git ") {
            self.in_hunk = false;
            self.scan.file("");
            return;
        }
        if l.starts_with(b"@@ ") {
            self.in_hunk = true;
            self.line = hunk_start(l).unwrap_or(1);
            self.scan.key = None;
            return;
        }
        if !self.in_hunk {
            if let Some(p) = l.strip_prefix(b"+++ ") {
                let p = unquote(p);
                self.scan.file(p.strip_prefix("b/").unwrap_or(&p));
            }
            return;
        }
        match l.first() {
            Some(b'+') => {
                if !self.scan.path.is_empty() {
                    self.scan.line(self.line, &l[1..]);
                }
                self.line += 1;
            }
            Some(b' ') => self.line += 1,
            _ => {}
        }
    }
}

/// The new side's first line in `@@ -a,b +c,d @@`.
fn hunk_start(l: &[u8]) -> Option<usize> {
    let s = String::from_utf8_lossy(l);
    let plus = s.split(' ').find_map(|w| w.strip_prefix('+'))?;
    plus.split(',').next()?.parse().ok()
}

/// A path as git prints it: "quoted with \\ escapes" when it has odd characters.
fn unquote(p: &[u8]) -> String {
    let s = String::from_utf8_lossy(p);
    let Some(inner) = s.strip_prefix('"').and_then(|s| s.strip_suffix('"')) else {
        return s.into_owned();
    };
    let mut out = String::new();
    let mut it = inner.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some(c) => out.push(c),
            None => {}
        }
    }
    out
}

/// Reads one line of at most `cap` bytes into `out` (the rest of a longer line is skipped).
fn next_line(r: &mut impl BufRead, out: &mut Vec<u8>, cap: usize) -> std::io::Result<usize> {
    out.clear();
    let mut total = 0;
    loop {
        let buf = r.fill_buf()?;
        if buf.is_empty() {
            return Ok(total);
        }
        let (n, done) = match buf.iter().position(|b| *b == b'\n') {
            Some(i) => (i + 1, true),
            None => (buf.len(), false),
        };
        let room = cap.saturating_sub(out.len());
        out.extend_from_slice(&buf[..n.min(room)]);
        r.consume(n);
        total += n;
        if done {
            return Ok(total);
        }
    }
}

/// Scans diff output; returns the scanner and how many bytes it read (it stops after `cap`).
pub fn scan_diff(mut r: impl BufRead, cap: u64) -> (Scanner, bool) {
    let mut d = DiffScan::default();
    let (mut buf, mut read) = (Vec::new(), 0u64);
    while let Ok(n) = next_line(&mut r, &mut buf, MAX_LINE + 2) {
        if n == 0 {
            return (d.scan, false);
        }
        read += n as u64;
        if read > cap {
            return (d.scan, true);
        }
        d.feed(&buf);
    }
    (d.scan, false)
}

// ---------- push protection: git pushes ----------

static HOOK_EXE: OnceLock<PathBuf> = OnceLock::new();

/// The program git runs as the pre-receive hook: this one (`gitter pre-receive`),
/// unless set first (tests point it at the built binary).
pub fn set_hook_exe(p: impl Into<PathBuf>) {
    let _ = HOOK_EXE.set(p.into());
}

fn hook_exe() -> PathBuf {
    HOOK_EXE.get().cloned().or_else(|| std::env::current_exe().ok()).unwrap_or_else(|| "gitter".into())
}

const HOOK: &str = "#!/bin/sh\n# Gitter push protection: scans what a push adds for secrets.\nexec \"$GITTER_HOOK_EXE\" pre-receive\n";

/// GITTER_DATA/hooks with the pre-receive hook in it (written when missing or changed).
fn hooks_dir(data: &Path) -> std::io::Result<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let dir = data.join("hooks");
    let file = dir.join("pre-receive");
    if std::fs::read_to_string(&file).ok().as_deref() != Some(HOOK) {
        std::fs::create_dir_all(&dir)?;
        let tmp = dir.join(format!(".pre-receive.{}", rand_hex()));
        std::fs::write(&tmp, HOOK)?;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
        std::fs::rename(&tmp, &file)?;
    }
    Ok(dir)
}

fn rand_hex() -> String {
    let mut b = [0u8; 8];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut b);
    hex::encode(b)
}

/// What the hook tells the server after a push.
#[derive(Serialize, Deserialize, Default)]
struct Report {
    blocked: Vec<Finding>,
    /// Allowances the push used.
    allowed: Vec<String>,
}

/// One push's protection: the hook's configuration, and its report afterwards.
pub struct PushGuard {
    hooks: PathBuf,
    out: PathBuf,
    allow: Vec<String>,
    url: String,
    repo_id: i64,
    user_id: i64,
}

impl PushGuard {
    pub async fn start(st: &AppState, repo: &Repo, user: &User) -> R<PushGuard> {
        let hooks = hooks_dir(&st.cfg.data_dir)?;
        let out = hooks.join(format!("report-{}.json", rand_hex()));
        Ok(PushGuard {
            hooks,
            out,
            allow: allowances(st, repo.id, user.id).await?,
            url: format!("{}/{}/{}/security", st.cfg.url, repo.owner, repo.name),
            repo_id: repo.id,
            user_id: user.id,
        })
    }

    /// Turn the hook on for `git http-backend` (before its subcommand).
    pub fn apply(&self, cmd: &mut tokio::process::Command) {
        cmd.arg("-c")
            .arg(format!("core.hooksPath={}", self.hooks.display()))
            .env("GITTER_HOOK_EXE", hook_exe())
            .env("GITTER_SCAN_ALLOW", self.allow.join(","))
            .env("GITTER_SCAN_OUT", &self.out)
            .env("GITTER_SCAN_URL", &self.url);
    }

    /// After git exits: keep what was refused (so its author can allow it) and use up the allowances spent.
    pub async fn finish(self, st: &AppState) {
        let Ok(text) = tokio::fs::read(&self.out).await else { return };
        let Ok(rep) = serde_json::from_slice::<Report>(&text) else { return };
        if let Err(e) = remember(st, self.repo_id, self.user_id, rep.blocked, rep.allowed).await {
            eprintln!("push protection: {}", e.msg);
        }
    }
}

impl Drop for PushGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.out);
    }
}

const LOG_ARGS: &[&str] = &[
    "log", "-p", "-U0", "-M", "--no-color", "--no-ext-diff", "--no-textconv", "--diff-filter=AMR", "--format=%x00%H",
];

/// `gitter pre-receive`: run by git while a push waits, with the pushed objects
/// quarantined. Scans the lines the new commits add; refuses the push (exit 1)
/// when one holds a secret its pusher hasn't allowed.
pub fn hook_main() -> i32 {
    let mut news = Vec::new();
    for l in std::io::stdin().lock().lines().map_while(Result::ok) {
        let mut p = l.split(' ');
        if let (Some(_old), Some(new)) = (p.next(), p.next())
            && new.len() >= 40
            && new.bytes().all(|b| b.is_ascii_hexdigit())
            && new.bytes().any(|b| b != b'0')
        {
            news.push(new.to_string());
        }
    }
    if news.is_empty() {
        return 0;
    }
    let child = std::process::Command::new("git")
        .args(LOG_ARGS)
        .args(&news)
        .args(["--not", "--all"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn();
    let Ok(mut child) = child else {
        eprintln!("Push protection couldn't run git; push refused.");
        return 1;
    };
    let out = std::io::BufReader::with_capacity(256 * 1024, child.stdout.take().expect("piped stdout"));
    let (scan, cut) = scan_diff(out, HOOK_CAP);
    let _ = child.kill();
    let _ = child.wait();
    let allow: HashSet<String> = std::env::var("GITTER_SCAN_ALLOW").unwrap_or_default().split(',').filter(|s| !s.is_empty()).map(String::from).collect();
    let (allowed, blocked): (Vec<Finding>, Vec<Finding>) = scan.findings.into_iter().partition(|f| allow.contains(&f.fingerprint));
    let rep = Report { allowed: allowed.into_iter().map(|f| f.fingerprint).collect(), blocked };
    if let Ok(path) = std::env::var("GITTER_SCAN_OUT")
        && let Ok(json) = serde_json::to_vec(&rep)
    {
        let _ = std::fs::write(path, json);
    }
    if cut {
        eprintln!("Push protection scanned the first {} MB of this push.", HOOK_CAP >> 20);
    }
    if rep.blocked.is_empty() {
        return 0;
    }
    eprint!("{}", refusal(&rep.blocked, scan.more, &std::env::var("GITTER_SCAN_URL").unwrap_or_default()));
    1
}

/// The message a refused push prints (git shows it as `remote: …` lines).
pub fn refusal(blocked: &[Finding], more: usize, url: &str) -> String {
    let n = blocked.len() + more;
    let mut s = format!(
        "\nPush refused: it adds {} that look{} like a secret.\n\n",
        if n == 1 { "something".to_string() } else { format!("{n} things") },
        if n == 1 { "s" } else { "" }
    );
    for f in blocked {
        let at = if f.commit.len() >= 7 { format!("  (commit {})", &f.commit[..7]) } else { String::new() };
        s += &format!("  {}:{}  {}  {}{at}\n", clean(&f.path), f.line, f.kind, clean(&f.masked));
    }
    if more > 0 {
        s += &format!("  …and {more} more\n");
    }
    let fps: Vec<&str> = blocked.iter().map(|f| f.fingerprint.as_str()).collect();
    s += "\nTake it out of the commits (removing it in a later commit isn't enough) and push again.\n";
    s += "If it's a false positive, a test value, or a secret you'll revoke, allow it once here and push again:\n";
    s += &format!("  {url}?allow={}\n\n", fps.join(","));
    s
}

/// No control characters (a path is the pusher's text, printed in their terminal).
fn clean(s: &str) -> String {
    s.chars().map(|c| if c.is_control() { '?' } else { c }).collect()
}

// ---------- push protection: web commits ----------

/// Push protection for a commit made on the web: the lines `changes` add
/// (against the files at `parent`) are scanned before anything is written.
/// Findings its author allowed once go through, using up the allowance.
pub async fn guard_commit(st: &AppState, repo: &Repo, user: &User, parent: Option<&str>, changes: &[Change<'_>]) -> R<()> {
    let mut sc = Scanner::default();
    for ch in changes {
        let Change::Write(path, data, _) = ch else { continue };
        if data[..data.len().min(8000)].contains(&0) {
            continue;
        }
        let old = match parent {
            Some(p) => {
                let spec = format!("{p}:{path}");
                let o = st.git.run(&repo.path, &["cat-file", "blob", &spec], None, &[]).await?;
                if o.code == 0 { o.stdout } else { Vec::new() }
            }
            None => Vec::new(),
        };
        sc.file(path);
        for (n, l) in added_lines(&old, data) {
            sc.line(n, l);
        }
    }
    if sc.findings.is_empty() {
        return Ok(());
    }
    let allow = allowances(st, repo.id, user.id).await?;
    let (allowed, blocked): (Vec<Finding>, Vec<Finding>) = sc.findings.into_iter().partition(|f| allow.contains(&f.fingerprint));
    let used: Vec<String> = allowed.into_iter().map(|f| f.fingerprint).collect();
    if blocked.is_empty() {
        return remember(st, repo.id, user.id, Vec::new(), used).await;
    }
    let list: Vec<String> = blocked.iter().map(|f| format!("{}:{} ({})", f.path, f.line, f.kind)).collect();
    let details = serde_json::json!({ "findings": blocked, "more": sc.more });
    remember(st, repo.id, user.id, blocked, Vec::new()).await?;
    Err(AppError::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        format!("This commit adds what looks like a secret: {}. Take it out, or allow it once if it's a false positive, a test value, or you'll revoke it.", list.join(", ")),
    )
    .with_code("secret_detected")
    .with_details(details))
}

// ---------- allowances ----------

/// Fingerprints `user` allowed once in `repo` and hasn't used yet.
async fn allowances(st: &AppState, repo_id: i64, user_id: i64) -> R<Vec<String>> {
    st.db
        .call(move |c| {
            let mut s = c.prepare("SELECT fingerprint FROM secret_bypasses WHERE repo_id = ?1 AND user_id = ?2 AND used_at IS NULL AND expires_at > ?3")?;
            Ok(s.query_map(rusqlite::params![repo_id, user_id, now()], |r| r.get(0))?.collect::<Result<_, _>>()?)
        })
        .await
}

/// Keep refused findings for their author (a week) and mark allowances used.
async fn remember(st: &AppState, repo_id: i64, user_id: i64, blocked: Vec<Finding>, used: Vec<String>) -> R<()> {
    st.db
        .call(move |c| {
            let tx = c.transaction()?;
            let t = now();
            tx.execute("DELETE FROM secret_blocks WHERE created_at < ?1", [t - 7 * 86400])?;
            for f in blocked.iter().take(MAX_FINDINGS) {
                tx.execute(
                    "INSERT INTO secret_blocks (repo_id, user_id, fingerprint, kind, path, line, masked, commit_sha, created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                    rusqlite::params![repo_id, user_id, f.fingerprint, f.kind, f.path, f.line as i64, f.masked, f.commit, t],
                )?;
            }
            for fp in &used {
                tx.execute(
                    "UPDATE secret_bypasses SET used_at = ?4 WHERE repo_id = ?1 AND user_id = ?2 AND fingerprint = ?3 AND used_at IS NULL",
                    rusqlite::params![repo_id, user_id, fp, t],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
        .await
}

// ---------- on-demand scan ----------

/// Scan every file on `sha` (all its lines count as added).
pub async fn scan_commit(st: &AppState, repo: &Repo, sha: &str) -> R<(Vec<Finding>, usize, bool)> {
    let args = ["diff", "-U0", "--no-color", "--no-ext-diff", "--no-textconv", "--no-renames", crate::git::EMPTY_TREE, sha];
    let (out, cut) = st.git.run_capped(&repo.path, &args, SCAN_CAP).await?;
    let sha = sha.to_string();
    let (mut scan, _) = tokio::task::spawn_blocking(move || scan_diff(&out[..], u64::MAX)).await.map_err(AppError::internal)?;
    for f in &mut scan.findings {
        f.commit = sha.clone();
    }
    Ok((scan.findings, scan.more, cut))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(lines: &[&str]) -> Vec<Finding> {
        let mut s = Scanner::default();
        s.file("f.txt");
        for (i, l) in lines.iter().enumerate() {
            s.line(i + 1, l.as_bytes());
        }
        s.findings
    }
    fn kinds(lines: &[&str]) -> Vec<String> {
        scan(lines).into_iter().map(|f| f.kind).collect()
    }

    // Built at run time so this file doesn't trip scanners itself.
    fn aws() -> String {
        format!("AKIA{}", "Q3EGRIW5ZKVNOM2T")
    }
    fn gtr() -> String {
        format!("gtr_{}", "aB3dE5fG7hJ9kL1mN3pQ5rS7tU9vW1xY3zA5bC7dE9f")
    }

    #[test]
    fn finds_tokens_with_their_kind_and_line() {
        let k = format!("aws_access_key_id = {}", aws());
        let f = scan(&["nothing here", &k]);
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!((f[0].kind.as_str(), f[0].line, f[0].path.as_str()), ("AWS access key ID", 2, "f.txt"));
        assert_eq!(f[0].masked, "AKIA…2T");
        assert!(!format!("{f:?}").contains(&aws()), "the secret itself is never kept");
        assert_eq!(kinds(&[&format!("TOKEN={}", gtr())]), ["Gitter access token"]);
        assert_eq!(kinds(&[&format!("x {}{}", "ghp_", "a".repeat(18) + &"1".repeat(18))]), ["GitHub token"]);
    }

    #[test]
    fn token_edges_dont_match() {
        assert!(kinds(&[&format!("X{}", aws())]).is_empty(), "inside a longer word");
        assert!(kinds(&[&format!("{}Z", aws())]).is_empty(), "too long");
        assert!(kinds(&["AKIAIOSFODNN7EXAMPLE"]).is_empty(), "the AWS docs' sample");
        assert!(kinds(&[&gtr()[..30]]).is_empty(), "too short");
    }

    #[test]
    fn private_keys_need_key_material() {
        let body = "MIIEpAIBAAKCAQEA3Tz2mr7SZiAMfQyuvBjM2Tg";
        assert_eq!(kinds(&["-----BEGIN RSA PRIVATE KEY-----", body]), ["Private key"]);
        assert_eq!(kinds(&["-----BEGIN OPENSSH PRIVATE KEY-----", body]), ["Private key"]);
        assert_eq!(kinds(&[&format!("key = \"-----BEGIN PRIVATE KEY-----\\n{body}\"")]), ["Private key"]);
        assert!(kinds(&["if s.starts_with(\"-----BEGIN RSA PRIVATE KEY-----\") {", "}"]).is_empty(), "code that looks for a header");
        assert!(kinds(&["-----BEGIN PUBLIC KEY-----", body]).is_empty());
        let f = scan(&["-----BEGIN EC PRIVATE KEY-----", body]);
        assert_eq!((f[0].line, f[0].masked.as_str()), (1, "-----BEGIN EC PRIVATE KEY-----"));
    }

    #[test]
    fn assignments_need_a_random_value() {
        assert_eq!(kinds(&["api_key = \"q8Zr2LmX9vT4kWp7Ns3B\""]), ["Secret in an assignment"]);
        assert_eq!(kinds(&["\"password\": \"hX7#kQ2mZ9vL4pR8wT\","]), ["Secret in an assignment"]);
        assert_eq!(kinds(&[&format!("DB_PASSWORD={}", "Jq3Vn8Ls2Kd7Xw4Rt9")]), ["Secret in an assignment"]);
        assert_eq!(kinds(&[&format!("token: {}", "3f786850e387550fdab836ed7e6dc881")]), ["Secret in an assignment"], "long hex");
        for fine in [
            "password = \"changeme1234567890\"",
            "token = get_token(config, 1234567890abcdef)",
            "if password == \"q8Zr2LmX9vT4kWp7Ns3B\" {",
            "secret: ${{ secrets.DEPLOY_KEY_2024_PROD }}",
            "password = \"short1\"",
            "\"node_modules/js-tokens\": {",
            "token = \"aaaaaaaaaaaaaaaaaaaa1\"",
            "name = \"tokenizers-0.15.2-abc123def456\"",
            "CI secrets: chacha20poly1305 at rest",
        ] {
            assert!(kinds(&[fine]).is_empty(), "{fine}");
        }
    }

    #[test]
    fn fingerprints_name_kind_file_and_secret() {
        let a = fingerprint("k", "a.txt", b"s");
        assert_eq!(a.len(), 20);
        assert_ne!(a, fingerprint("k", "b.txt", b"s"));
        assert_ne!(a, fingerprint("k", "a.txt", b"t"));
        let twice = scan(&[&aws(), &aws()]);
        assert_eq!(twice.len(), 1, "one finding per secret and file");
    }

    #[test]
    fn diff_output_gives_paths_lines_and_commits() {
        let d = format!(
            "\0{sha}\n\ndiff --git a/x b/x\nindex 1..2 100644\n--- a/x\n+++ b/x\n@@ -1,0 +5,2 @@\n+fine\n+key {aws}\n@@ -9 +11 @@\n-old\n++++ b/fake {gtr}\n\0{sha2}\ndiff --git a/\"q\\tq\" b/\"q\\tq\"\nnew file mode 100644\n--- /dev/null\n+++ \"b/q\\tq\"\n@@ -0,0 +1 @@\n+{aws}2\n",
            sha = "a".repeat(40),
            sha2 = "b".repeat(40),
            aws = aws(),
            gtr = gtr()
        );
        let (s, cut) = scan_diff(d.as_bytes(), u64::MAX);
        assert!(!cut);
        let got: Vec<(String, usize, String, String)> = s.findings.iter().map(|f| (f.path.clone(), f.line, f.kind.clone(), f.commit[..1].to_string())).collect();
        assert_eq!(
            got,
            [
                ("x".to_string(), 6, "AWS access key ID".to_string(), "a".to_string()),
                ("x".to_string(), 11, "Gitter access token".to_string(), "a".to_string()),
            ],
            "a +++ line inside a hunk is content; a too-long key isn't one"
        );
        let (_, cut) = scan_diff(d.as_bytes(), 10);
        assert!(cut);
    }

    #[test]
    #[ignore]
    fn tmp_self_scan() {
        let out = std::process::Command::new("git").args(["diff", "-U0", "--no-color", crate::git::EMPTY_TREE, "HEAD"]).output().unwrap().stdout;
        let (s, _) = scan_diff(&out[..], u64::MAX);
        for f in &s.findings {
            eprintln!("FOUND {}:{} {} {}", f.path, f.line, f.kind, f.masked);
        }
    }

    #[test]
    fn added_lines_skip_what_was_there() {
        let got = added_lines(b"a\nb\nc\n", b"a\nb\nnew\nc\nb\n");
        assert_eq!(got, [(3, &b"new"[..]), (5, &b"b"[..])]);
    }

    #[test]
    fn long_lines_are_capped_not_buffered() {
        let mut big = vec![b'x'; 200_000];
        big.extend_from_slice(b"\nnext\n");
        let (mut r, mut buf) = (&big[..], Vec::new());
        assert_eq!(next_line(&mut r, &mut buf, 10).unwrap(), 200_001);
        assert_eq!(buf.len(), 10);
        next_line(&mut r, &mut buf, 10).unwrap();
        assert_eq!(buf, b"next\n");
    }

    #[test]
    fn refusal_names_file_line_kind_and_link() {
        let f = Finding { kind: "AWS access key ID".into(), path: "a\x1b[31m.txt".into(), line: 3, masked: "AKIA…2T".into(), fingerprint: "ff".into(), commit: "c".repeat(40) };
        let m = refusal(&[f], 0, "http://h/o/r/security");
        assert!(m.contains("a?[31m.txt:3  AWS access key ID  AKIA…2T  (commit ccccccc)"), "{m}");
        assert!(m.contains("http://h/o/r/security?allow=ff"));
    }
}
