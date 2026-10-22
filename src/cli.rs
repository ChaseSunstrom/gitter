//! The command-line client (`gitter login`, `repo`, `issue`, `pr`, `ci`), like
//! gh but for a Gitter server, and `gitter ci run`, which runs a pipeline on
//! this machine with the CI runner's own parser and step runner.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::validate;

const USAGE: &str = "gitter — command-line client

  gitter login URL [--token T]       save the server and an access token (or set GITTER_TOKEN; asks when neither)
  gitter auth setup-git              let plain git clone/pull/push use your logins (a git credential helper)
  gitter auth git-credential get     what git runs for that (store and erase do nothing)
  gitter repo create NAME [--description D] [--private] [--readme] [--owner ORG]
  gitter repo clone OWNER/NAME [DIR]
  gitter repo list [OWNER]
  gitter repo view [OWNER/NAME]
  gitter issue list [--state open|closed|all]
  gitter issue create --title T [--body B]
  gitter issue view|close N
  gitter issue comment N --body B
  gitter pr create [--title T] [--body B] [--base BRANCH] [--head BRANCH]
                                     (head: the current branch, pushed first when the server lacks it)
  gitter pr list [--state open|closed|all]
  gitter pr view|checkout N
  gitter pr merge N [--method merge|squash|rebase] [--delete-branch]
  gitter ci status [RUN]             recent runs, or one run's jobs
  gitter ci logs RUN [--job NAME]    print a run's logs (follows running jobs)
  gitter ci rerun RUN [--all]        re-run failed jobs (--all: every job)
  gitter ci run [--here]             run .gitter/ci.yml here, no server needed: each job in a fresh
                                     copy of the work tree (--here: in this folder, not for image
                                     jobs); exits 1 if a job fails

The repository is the one whose git remote points at a server you logged in to,
or -R OWNER/NAME (or -R URL/OWNER/NAME). --json prints the server's answer as JSON.";

const BOOL_FLAGS: &[&str] = &["--json", "--private", "--readme", "--delete-branch", "--all", "--here", "--help"];
const VALUE_FLAGS: &[&str] = &["-R", "--repo", "--token", "--title", "--body", "--state", "--base", "--head", "--method", "--description", "--owner", "--job"];

/// Each command and the flags it takes (--help: all of them). `login`'s second word is the URL.
const COMMANDS: &[(&str, &str, &[&str])] = &[
    ("login", "", &["--token"]),
    ("auth", "setup-git", &[]),
    ("auth", "git-credential", &[]),
    ("repo", "create", &["--description", "--private", "--readme", "--owner", "--json"]),
    ("repo", "clone", &[]),
    ("repo", "list", &["--json"]),
    ("repo", "view", &["-R", "--repo", "--json"]),
    ("issue", "list", &["-R", "--repo", "--json", "--state"]),
    ("pr", "list", &["-R", "--repo", "--json", "--state"]),
    ("issue", "create", &["-R", "--repo", "--json", "--title", "--body"]),
    ("issue", "view", &["-R", "--repo", "--json"]),
    ("pr", "view", &["-R", "--repo", "--json"]),
    ("issue", "close", &["-R", "--repo", "--json"]),
    ("issue", "comment", &["-R", "--repo", "--json", "--body"]),
    ("pr", "create", &["-R", "--repo", "--json", "--title", "--body", "--base", "--head"]),
    ("pr", "checkout", &["-R", "--repo"]),
    ("pr", "merge", &["-R", "--repo", "--json", "--method", "--delete-branch"]),
    ("ci", "status", &["-R", "--repo", "--json"]),
    ("ci", "logs", &["-R", "--repo", "--job"]),
    ("ci", "rerun", &["-R", "--repo", "--json", "--all"]),
    ("ci", "run", &["--here"]),
];

/// The USAGE lines of `gitter cmd sub` (with their continuation lines).
fn usage_of(cmd: &str, sub: &str) -> String {
    let mut out = vec![];
    let mut on = false;
    for l in USAGE.lines() {
        let t = l.trim_start();
        if let Some(rest) = t.strip_prefix("gitter ") {
            let w: Vec<&str> = rest.split_whitespace().collect();
            on = w.first() == Some(&cmd) && (cmd == "login" || w.get(1).is_some_and(|s| s.split('|').any(|x| x == sub)));
        } else if t.is_empty() || l.len() - t.len() <= 2 {
            on = false;
        }
        if on {
            out.push(l);
        }
    }
    out.join("\n")
}

struct Args {
    pos: Vec<String>,
    flags: BTreeMap<String, String>,
}

impl Args {
    fn parse(raw: &[String]) -> Result<Args> {
        let mut a = Args { pos: vec![], flags: BTreeMap::new() };
        let mut it = raw.iter();
        while let Some(s) = it.next() {
            let (k, inline) = match s.split_once('=') {
                Some((k, v)) if k.starts_with("--") => (k, Some(v.to_string())),
                _ => (s.as_str(), None),
            };
            if s == "-h" {
                a.flags.insert("--help".into(), String::new());
            } else if BOOL_FLAGS.contains(&k) && inline.is_none() {
                a.flags.insert(k.into(), String::new());
            } else if VALUE_FLAGS.contains(&k) {
                let v = match inline {
                    Some(v) => v,
                    None => it.next().cloned().ok_or_else(|| anyhow!("{k} needs a value"))?,
                };
                a.flags.insert(k.into(), v);
            } else if s.starts_with('-') && s != "-" {
                // Refused by main with the command's usage (no command takes it).
                a.flags.insert(s.clone(), String::new());
            } else {
                a.pos.push(s.clone());
            }
        }
        Ok(a)
    }
    fn get(&self, k: &str) -> Option<&str> {
        self.flags.get(k).map(String::as_str)
    }
    fn has(&self, k: &str) -> bool {
        self.flags.contains_key(k)
    }
    fn pos(&self, i: usize) -> Option<&str> {
        self.pos.get(i).map(String::as_str)
    }
}

// ---------------------------------------------------------------- config

#[derive(Serialize, Deserialize, Default)]
struct Config {
    /// The server last logged in to: used when nothing else says which.
    #[serde(default)]
    default: String,
    #[serde(default)]
    hosts: BTreeMap<String, Host>,
}

#[derive(Serialize, Deserialize, Clone)]
struct Host {
    user: String,
    token: String,
}

fn config_path() -> Result<PathBuf> {
    let base = match std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        Some(d) => PathBuf::from(d),
        None => PathBuf::from(std::env::var_os("HOME").filter(|v| !v.is_empty()).ok_or_else(|| anyhow!("HOME isn't set"))?).join(".config"),
    };
    Ok(base.join("gitter/config.json"))
}

fn load() -> Result<Config> {
    let p = config_path()?;
    match std::fs::read_to_string(&p) {
        Ok(s) => serde_json::from_str(&s).with_context(|| format!("can't read {}", p.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => Err(e).with_context(|| format!("can't read {}", p.display())),
    }
}

/// Written whole to a new 0600 file, then moved over the old one.
fn save(c: &Config) -> Result<()> {
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    let p = config_path()?;
    let dir = p.parent().ok_or_else(|| anyhow!("bad config path"))?;
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    let tmp = dir.join(format!(".config.json.{}", std::process::id()));
    let _ = std::fs::remove_file(&tmp);
    let mut f = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp)?;
    f.write_all(serde_json::to_string_pretty(c)?.as_bytes())?;
    f.sync_all()?;
    std::fs::rename(&tmp, &p).with_context(|| format!("can't write {}", p.display()))?;
    Ok(())
}

// ---------------------------------------------------------------- HTTP

struct Api {
    base: String,
    token: String,
    agent: ureq::Agent,
}

impl Api {
    fn new(base: &str, token: &str) -> Api {
        let cfg = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(60)))
            .http_status_as_error(false)
            // A renamed repository answers with a redirect on the same server: keep the token for it.
            .redirect_auth_headers(ureq::config::RedirectAuthHeaders::SameHost)
            .user_agent(concat!("gitter-cli/", env!("CARGO_PKG_VERSION")))
            .build();
        Api { base: base.to_string(), token: token.to_string(), agent: ureq::Agent::new_with_config(cfg) }
    }

    /// (status, x-job-status header, body) of `method path`; errors only when the server can't be reached.
    fn raw(&self, method: &str, path: &str, body: Option<&Value>) -> Result<(u16, Option<String>, Vec<u8>)> {
        let url = format!("{}{path}", self.base);
        let req = ureq::http::Request::builder().method(method).uri(&url).header("authorization", format!("Bearer {}", self.token)).header("accept", "application/json");
        let sent = match body {
            Some(b) => self.agent.run(req.header("content-type", "application/json").body(b.to_string())?),
            None => self.agent.run(req.body(())?),
        };
        let mut resp = sent.with_context(|| format!("can't reach {}", self.base))?;
        let job_status = resp.headers().get("x-job-status").and_then(|v| v.to_str().ok()).map(String::from);
        let data = resp.body_mut().with_config().limit(16 << 20).read_to_vec().with_context(|| format!("reading the answer from {}", self.base))?;
        Ok((resp.status().as_u16(), job_status, data))
    }

    fn call(&self, method: &str, path: &str, body: Option<Value>) -> Result<Value> {
        let (status, _, data) = self.raw(method, path, body.as_ref())?;
        if status == 401 {
            bail!("{} refused the token: run `gitter login {}` again", self.base, self.base);
        }
        if status >= 400 {
            let msg = serde_json::from_slice::<Value>(&data)
                .ok()
                .and_then(|v| v["error"].as_str().map(String::from))
                .unwrap_or_else(|| String::from_utf8_lossy(&data[..data.len().min(300)]).into_owned());
            bail!("{} (HTTP {status})", clean(&msg, false));
        }
        Ok(serde_json::from_slice(&data).unwrap_or(Value::Null))
    }

    fn get(&self, path: &str) -> Result<Value> {
        self.call("GET", path, None)
    }
}

// ---------------------------------------------------------------- git

fn git_cmd(args: &[&str], auth: Option<&Api>) -> Command {
    let mut c = Command::new("git");
    c.args(args).env("GIT_TERMINAL_PROMPT", "0");
    if let Some(api) = auth {
        // Through the environment (not the command line or .git/config), and only for this server's URLs.
        let n: usize = std::env::var("GIT_CONFIG_COUNT").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
        let basic = base64::engine::general_purpose::STANDARD.encode(format!("x-access-token:{}", api.token));
        c.env("GIT_CONFIG_COUNT", (n + 1).to_string())
            .env(format!("GIT_CONFIG_KEY_{n}"), format!("http.{}/.extraHeader", api.base))
            .env(format!("GIT_CONFIG_VALUE_{n}"), format!("Authorization: Basic {basic}"));
    }
    c
}

/// Run git and return its output (trimmed), or its error message.
fn git(args: &[&str], auth: Option<&Api>) -> Result<String> {
    let out = git_cmd(args, auth).stdin(Stdio::null()).output().context("can't run git")?;
    if !out.status.success() {
        bail!("git {}: {}", args[0], String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Run git with its progress and messages on the terminal.
fn git_loud(args: &[&str], auth: Option<&Api>) -> Result<()> {
    let s = git_cmd(args, auth).stdin(Stdio::null()).status().context("can't run git")?;
    if !s.success() {
        bail!("git {} failed", args[0]);
    }
    Ok(())
}

/// Plain http to anywhere but this machine: the token would cross the network unencrypted.
fn plain_elsewhere(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("http://") else { return false };
    let authority = rest.split('/').next().unwrap_or("");
    let host = match authority.strip_prefix('[') {
        Some(v6) => v6.split(']').next().unwrap_or(""),
        None => authority.split(':').next().unwrap_or(""),
    };
    !(host.eq_ignore_ascii_case("localhost") || host.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback()))
}

/// `scheme://host/path` without any `user:password@`.
fn strip_userinfo(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else { return url.to_string() };
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let authority = authority.rsplit_once('@').map_or(authority, |x| x.1);
    format!("{scheme}://{authority}/{path}")
}

/// "owner/name" when it is one (an optional `.git` and trailing slash dropped).
fn repo_name(s: &str) -> Option<String> {
    let s = s.trim_end_matches('/');
    let s = s.strip_suffix(".git").unwrap_or(s);
    let (o, n) = s.split_once('/')?;
    (validate::name(o, 39) && validate::repo_name(n)).then(|| format!("{o}/{n}"))
}

// ---------------------------------------------------------------- context

struct Ctx {
    a: Args,
    cfg: Config,
}

impl Ctx {
    fn api(&self, host: &str) -> Result<Api> {
        let h = self.cfg.hosts.get(host).ok_or_else(|| anyhow!("not logged in to {host}: run `gitter login {host}`"))?;
        Ok(Api::new(host, &h.token))
    }

    /// The server (and repository) a URL points into.
    fn match_url(&self, url: &str) -> Option<(String, String)> {
        let u = strip_userinfo(url);
        self.cfg.hosts.keys().find_map(|h| Some((h.clone(), repo_name(u.strip_prefix(h.as_str())?.strip_prefix('/')?)?)))
    }

    /// The (server, owner/name) of this folder's git remotes, `origin` first.
    fn remote_repo(&self) -> Option<(String, String)> {
        let out = git(&["config", "--get-regexp", r"^remote\..*\.url$"], None).ok()?;
        let mut remotes: Vec<(&str, &str)> = out.lines().filter_map(|l| l.split_once(' ')).collect();
        remotes.sort_by_key(|(k, _)| *k != "remote.origin.url");
        remotes.iter().find_map(|(_, url)| self.match_url(url))
    }

    /// The server for commands without a repository.
    fn host(&self) -> Result<Api> {
        if let Some((h, _)) = self.remote_repo() {
            return self.api(&h);
        }
        let h = if self.cfg.hosts.contains_key(&self.cfg.default) { self.cfg.default.clone() } else { self.cfg.hosts.keys().next().cloned().unwrap_or_default() };
        if h.is_empty() {
            bail!("log in first: gitter login https://your.gitter.server");
        }
        self.api(&h)
    }

    /// An OWNER/NAME (or URL/OWNER/NAME) argument: the server and repository.
    fn named(&self, s: &str) -> Result<(Api, String)> {
        if s.contains("://") {
            let (h, r) = self.match_url(s).ok_or_else(|| anyhow!("{s} isn't a repository on a server you logged in to"))?;
            return Ok((self.api(&h)?, r));
        }
        let r = repo_name(s).ok_or_else(|| anyhow!("write the repository as OWNER/NAME, not {s:?}"))?;
        Ok((self.host()?, r))
    }

    /// The repository to act on: -R, else this folder's remote.
    fn repo(&self) -> Result<(Api, String)> {
        if let Some(r) = self.a.get("-R").or(self.a.get("--repo")) {
            return self.named(r);
        }
        let (h, r) = self.remote_repo().ok_or_else(|| anyhow!("no git remote here points at a server you logged in to: pass -R OWNER/NAME"))?;
        Ok((self.api(&h)?, r))
    }

    fn json(&self) -> bool {
        self.a.has("--json")
    }

    fn number(&self, i: usize) -> Result<u64> {
        let s = self.a.pos(i).ok_or_else(|| anyhow!("which number? (see gitter {} --help)", self.a.pos(0).unwrap_or("")))?;
        s.trim_start_matches('#').parse().map_err(|_| anyhow!("{s:?} isn't a number"))
    }
}

// ---------------------------------------------------------------- output

/// Server text made safe for a terminal: no control characters (an escape
/// sequence in an issue title could rewrite the screen or the window title).
/// With `colors` (CI logs), ANSI color codes and carriage returns stay.
fn clean(s: &str, colors: bool) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(c) = rest.chars().next() {
        if c == '\x1b'
            && colors
            && let Some(params) = rest[1..].strip_prefix('[')
        {
            let n = params.find(|c: char| !(c.is_ascii_digit() || c == ';')).unwrap_or(params.len());
            if params[n..].starts_with('m') {
                out.push_str(&rest[..n + 3]);
                rest = &rest[n + 3..];
                continue;
            }
        }
        // Also no bidi overrides or zero-width characters, which can make text read differently than it is.
        let invisible = matches!(c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}');
        if (!c.is_control() || matches!(c, '\n' | '\t') || (colors && c == '\r')) && !invisible {
            out.push(c);
        }
        rest = &rest[c.len_utf8()..];
    }
    // A log can't leave the terminal in a color (or concealed) state.
    if colors && out.contains('\x1b') {
        out.push_str("\x1b[0m");
    }
    out
}

/// One table cell: one line, at most `max` characters.
fn cell(v: &Value, max: usize) -> String {
    let s = match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        v => v.to_string(),
    };
    let s = clean(&s.replace(['\n', '\t'], " "), false);
    if s.chars().count() > max { format!("{}…", s.chars().take(max - 1).collect::<String>()) } else { s }
}

fn table(head: &[&str], rows: &[Vec<String>]) {
    if rows.is_empty() {
        println!("Nothing to show.");
        return;
    }
    let mut w: Vec<usize> = head.iter().map(|h| h.chars().count()).collect();
    for r in rows {
        for (i, c) in r.iter().enumerate() {
            w[i] = w[i].max(c.chars().count());
        }
    }
    let line = |r: &[String]| {
        let cells: Vec<String> = r.iter().enumerate().map(|(i, c)| if i + 1 == r.len() { c.clone() } else { format!("{c}{}", " ".repeat(w[i] - c.chars().count())) }).collect();
        println!("{}", cells.join("  ").trim_end());
    };
    line(&head.iter().map(|h| h.to_string()).collect::<Vec<_>>());
    for r in rows {
        line(r);
    }
}

fn ago(t: &Value) -> String {
    let Some(t) = t.as_i64() else { return String::new() };
    let d = (crate::db::now() - t).max(0);
    match d {
        0..60 => "just now".into(),
        60..3600 => format!("{}m ago", d / 60),
        3600..86400 => format!("{}h ago", d / 3600),
        _ => format!("{}d ago", d / 86400),
    }
}

fn print_json(v: &Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(v)?);
    Ok(())
}

// ---------------------------------------------------------------- commands

pub fn main(raw: &[String]) -> Result<()> {
    let a = Args::parse(raw)?;
    if a.has("--help") || (a.pos.len() < 2 && a.pos(0) != Some("login")) {
        println!("{USAGE}");
        return Ok(());
    }
    let (c0, c1) = (a.pos(0).unwrap_or(""), if a.pos(0) == Some("login") { "" } else { a.pos(1).unwrap_or("") });
    let Some((_, _, takes)) = COMMANDS.iter().find(|(c, s, _)| *c == c0 && *s == c1) else {
        bail!("unknown command `gitter {c0} {c1}`\n\n{USAGE}");
    };
    if let Some(f) = a.flags.keys().find(|f| !takes.contains(&f.as_str())) {
        // Only the name: a mistyped `--tokens=…` mustn't print the secret after it.
        let f = f.split('=').next().unwrap_or("");
        bail!("gitter {c0}{} doesn't take {f}\n\nusage:\n{}", if c1.is_empty() { String::new() } else { format!(" {c1}") }, usage_of(c0, c1));
    }
    // `ci run` needs no server, so no config either (a missing HOME or a broken file doesn't matter).
    let local = a.pos(0) == Some("ci") && a.pos(1) == Some("run");
    let cx = Ctx { cfg: if local { Config::default() } else { load()? }, a };
    let sub = cx.a.pos(1).unwrap_or("");
    match (cx.a.pos(0).unwrap_or(""), sub) {
        ("login", _) => login(cx),
        ("auth", "setup-git") => setup_git(&cx),
        ("auth", "git-credential") => git_credential(&cx),
        ("repo", "create") => repo_create(&cx),
        ("repo", "clone") => repo_clone(&cx),
        ("repo", "list") => repo_list(&cx),
        ("repo", "view") => repo_view(&cx),
        ("issue" | "pr", "list") => issue_list(&cx),
        ("issue", "create") => issue_create(&cx),
        ("issue" | "pr", "view") => issue_view(&cx),
        ("issue", "close") => issue_close(&cx),
        ("issue", "comment") => issue_comment(&cx),
        ("pr", "create") => pr_create(&cx),
        ("pr", "checkout") => pr_checkout(&cx),
        ("pr", "merge") => pr_merge(&cx),
        ("ci", "status") => ci_status(&cx),
        ("ci", "logs") => ci_logs(&cx),
        ("ci", "rerun") => ci_rerun(&cx),
        ("ci", "run") => ci_run(&cx),
        (cmd, sub) => bail!("unknown command `gitter {cmd} {sub}`\n\n{USAGE}"),
    }
}

fn login(mut cx: Ctx) -> Result<()> {
    let url = cx.a.pos(1).ok_or_else(|| anyhow!("usage: gitter login https://your.gitter.server"))?.trim().trim_end_matches('/').to_string();
    if !(url.starts_with("http://") || url.starts_with("https://")) || url.contains('@') {
        bail!("give the server's address, like https://git.example.com");
    }
    if plain_elsewhere(&url) {
        eprintln!("Warning: {url} is plain http to another machine, so your token crosses the network unencrypted. Use https if the server has it.");
    }
    let token = match cx.a.get("--token").map(String::from).or_else(|| std::env::var("GITTER_TOKEN").ok().filter(|t| !t.is_empty())) {
        Some(t) => t,
        None => read_token(&url)?,
    };
    let token = token.trim().to_string();
    if token.is_empty() {
        bail!("no token given");
    }
    let me = Api::new(&url, &token).get("/api/me")?;
    let user = me["user"]["username"].as_str().ok_or_else(|| anyhow!("{url} didn't accept that token"))?.to_string();
    println!("Logged in to {url} as {}", clean(&user, false));
    cx.cfg.hosts.insert(url.clone(), Host { user, token });
    cx.cfg.default = url.clone();
    save(&cx.cfg)?;
    // setup-git was run for other servers, not this one.
    let ours: Vec<String> = global_config(&["--get-regexp", r"^credential\..*\.helper$"])
        .iter()
        .filter_map(|e| e.split_once('\n').filter(|(_, v)| v.contains("auth git-credential")).map(|(k, _)| k.to_string()))
        .collect();
    if !ours.is_empty() && !ours.contains(&format!("credential.{url}.helper")) {
        eprintln!("To let plain git use this login too, run `gitter auth setup-git` again.");
    }
    Ok(())
}

/// `git config --global --null ARGS…`: its entries (empty when there are none or git fails).
fn global_config(args: &[&str]) -> Vec<String> {
    let mut a = vec!["config", "--global", "--null"];
    a.extend(args);
    match git_cmd(&a, None).stdin(Stdio::null()).stderr(Stdio::null()).output() {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).split_terminator('\0').map(String::from).collect(),
        _ => vec![],
    }
}

/// The most read from stdin for a token or a credential request.
const INPUT_LIMIT: usize = 64 << 10;

/// One line from stdin; on a terminal, typed with echo off.
fn read_token(url: &str) -> Result<String> {
    use std::io::{BufRead, IsTerminal, Read};
    let echo = if std::io::stdin().is_terminal() { Some(hide_echo()?) } else { None };
    // The prompt comes after echo is off, so nothing typed at it shows.
    let note = if echo.is_some() { "; it isn't shown as you type" } else { "" };
    eprint!("Paste an access token for {url} (Settings → Access tokens{note}): ");
    let mut t = String::new();
    let read = std::io::stdin().lock().take(INPUT_LIMIT as u64 + 1).read_line(&mut t);
    if let Some(e) = echo {
        drop(e);
        eprintln!();
    }
    read?;
    if t.len() > INPUT_LIMIT {
        bail!("that's too long for a token");
    }
    Ok(t)
}

/// The terminal's settings from before `hide_echo`, put back once (when dropped, or on a signal).
type Saved = std::sync::Arc<std::sync::Mutex<Option<String>>>;

fn restore_tty(saved: &Saved) {
    if let Some(s) = saved.lock().unwrap_or_else(|e| e.into_inner()).take() {
        let _ = Command::new("stty").arg(s).stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
}

struct Echo(Saved);

impl Drop for Echo {
    fn drop(&mut self) {
        restore_tty(&self.0);
    }
}

/// Turns echo off on the terminal on stdin. Until the guard is dropped, Ctrl-C, SIGTERM and SIGHUP put the
/// terminal back and exit (130, 143, 129); after it, they still exit (the handlers stay for the rest of the run).
fn hide_echo() -> Result<Echo> {
    use tokio::signal::unix::{SignalKind, signal};
    let refuse = || anyhow!("can't turn off echo on this terminal, so the token would show: pass --token, set GITTER_TOKEN, or pipe it in");
    let out = Command::new("stty").arg("-g").stdin(Stdio::inherit()).stderr(Stdio::null()).output().map_err(|_| refuse())?;
    let state = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if !out.status.success() || state.is_empty() || !state.chars().all(|c| c.is_ascii_hexdigit() || c == ':') {
        return Err(refuse());
    }
    let saved: Saved = std::sync::Arc::new(std::sync::Mutex::new(Some(state)));
    // The handlers before echo goes off, so no signal lands in between.
    let rt = tokio::runtime::Builder::new_current_thread().enable_io().build()?;
    let (mut int, mut term, mut hup) = {
        let _in = rt.enter();
        (signal(SignalKind::interrupt())?, signal(SignalKind::terminate())?, signal(SignalKind::hangup())?)
    };
    let on_signal = saved.clone();
    std::thread::spawn(move || {
        let code = rt.block_on(async {
            tokio::select! {
                _ = int.recv() => 130,
                _ = term.recv() => 143,
                _ = hup.recv() => 129,
            }
        });
        restore_tty(&on_signal);
        eprintln!();
        std::process::exit(code);
    });
    let guard = Echo(saved);
    if !Command::new("stty").arg("-echo").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success()) {
        return Err(refuse());
    }
    Ok(guard)
}

/// Points git's credential helper for every logged-in server at `gitter auth git-credential`.
fn setup_git(cx: &Ctx) -> Result<()> {
    if cx.cfg.hosts.is_empty() {
        bail!("log in first: gitter login https://your.gitter.server");
    }
    let exe = std::env::current_exe().context("can't find the gitter program")?;
    let exe = exe.to_str().ok_or_else(|| anyhow!("gitter's path isn't UTF-8: {}", exe.display()))?;
    // git runs a `!` helper through the shell. Each server's entry names it, so it answers only with that login.
    let quote = |s: &str| format!("'{}'", s.replace('\'', r"'\''"));
    let mut failed = vec![];
    for host in cx.cfg.hosts.keys() {
        match set_helper(&format!("credential.{host}.helper"), &format!("!{} auth git-credential {}", quote(exe), quote(host))) {
            Ok(()) => println!("git now uses your gitter login for {host}"),
            Err(e) => {
                eprintln!("can't set git's credential helper for {host}: {e:#}");
                failed.push(host.as_str());
            }
        }
    }
    if !failed.is_empty() {
        bail!("git isn't set up for {}", failed.join(", "));
    }
    Ok(())
}

/// Makes `key` exactly ["", helper]: the empty value first drops helpers from other settings (like gh), and a
/// rerun leaves the same two. If the second step fails, the old values are put back, never just the reset.
fn set_helper(key: &str, helper: &str) -> Result<()> {
    let old = global_config(&["--get-all", key]);
    git(&["config", "--global", "--replace-all", key, ""], None)?;
    if let Err(e) = git(&["config", "--global", "--add", key, helper], None) {
        let _ = git(&["config", "--global", "--unset-all", key], None);
        for v in &old {
            let _ = git(&["config", "--global", "--add", key, v], None);
        }
        return Err(e);
    }
    Ok(())
}

/// "scheme", "host[:port]", "path" (no leading slash) of a server URL.
fn url_parts(u: &str) -> Option<(&str, &str, &str)> {
    let (scheme, rest) = u.split_once("://")?;
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    Some((scheme, authority, path))
}

/// git's credential helper protocol, as `gitter auth git-credential [SERVER] get|store|erase`: `get` answers with
/// SERVER's saved token when git asks about that server; `store` and `erase` do nothing. Without SERVER (what
/// setup-git wrote before), only when exactly one login is on that scheme and host, and it has no path.
fn git_credential(cx: &Ctx) -> Result<()> {
    use std::io::{BufRead, Read};
    let (server, verb) = match cx.a.pos(2) {
        Some(s) if s.contains("://") => (Some(s), cx.a.pos(3)),
        v => (None, v),
    };
    let mut q: BTreeMap<String, String> = BTreeMap::new();
    let mut input = std::io::stdin().lock().take(INPUT_LIMIT as u64 + 1);
    let mut total = 0;
    loop {
        let mut line = String::new();
        let n = input.read_line(&mut line)?;
        total += n;
        if total > INPUT_LIMIT {
            return Ok(()); // cut off: no answer rather than one from a partial line
        }
        let line = line.strip_suffix('\n').unwrap_or(&line);
        if line.is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once('=') {
            q.insert(k.to_string(), v.to_string());
        }
    }
    if verb != Some("get") {
        return Ok(());
    }
    let (Some(proto), Some(host)) = (q.get("protocol"), q.get("host")) else { return Ok(()) };
    let on_host = |k: &str| url_parts(k).is_some_and(|(s, a, _)| s == proto && a.eq_ignore_ascii_case(host));
    let key = match server {
        Some(s) => s,
        None => {
            let mut m = cx.cfg.hosts.keys().filter(|k| on_host(k));
            match (m.next(), m.next()) {
                (Some(k), None) if url_parts(k).is_some_and(|(_, _, p)| p.is_empty()) => k.as_str(),
                _ => return Ok(()),
            }
        }
    };
    let Some(h) = cx.cfg.hosts.get(key) else { return Ok(()) };
    let Some((_, _, p)) = url_parts(key).filter(|_| on_host(key)) else { return Ok(()) };
    // With credential.useHttpPath git sends the path: it must be under the server's.
    if let Some(x) = q.get("path").map(|x| x.trim_start_matches('/'))
        && !p.is_empty()
        && x != p
        && !x.starts_with(&format!("{p}/"))
    {
        return Ok(());
    }
    if q.get("username").is_some_and(|u| u != "x-access-token" && !u.eq_ignore_ascii_case(&h.user)) {
        return Ok(());
    }
    // A hand-edited config can't inject more lines into git's answer.
    if h.token.is_empty() || h.token.contains(['\n', '\r', '\0']) {
        return Ok(());
    }
    println!("username=x-access-token\npassword={}", h.token);
    Ok(())
}

fn repo_create(cx: &Ctx) -> Result<()> {
    let name = cx.a.pos(2).ok_or_else(|| anyhow!("usage: gitter repo create NAME"))?;
    let api = cx.host()?;
    let body = json!({ "name": name, "owner": cx.a.get("--owner"), "description": cx.a.get("--description").unwrap_or(""),
        "private": cx.a.has("--private"), "readme": cx.a.has("--readme") });
    let v = api.call("POST", "/api/repos", Some(body))?;
    if cx.json() {
        return print_json(&v);
    }
    let full = format!("{}/{}", cell(&v["owner"], 100), cell(&v["name"], 100));
    println!("Created {full}\n{}/{full}\nClone it with: gitter repo clone {full}", api.base);
    Ok(())
}

fn repo_clone(cx: &Ctx) -> Result<()> {
    let (api, r) = cx.named(cx.a.pos(2).ok_or_else(|| anyhow!("usage: gitter repo clone OWNER/NAME [DIR]"))?)?;
    let url = format!("{}/{r}.git", api.base);
    let mut args = vec!["clone", "--", url.as_str()];
    if let Some(d) = cx.a.pos(3) {
        args.push(d);
    }
    git_loud(&args, Some(&api))
}

fn repo_list(cx: &Ctx) -> Result<()> {
    let api = cx.host()?;
    let owner = match cx.a.pos(2) {
        Some(o) => o.to_string(),
        None => cx.cfg.hosts.get(&api.base).map(|h| h.user.clone()).unwrap_or_default(),
    };
    if !validate::name(&owner, 39) {
        bail!("{owner:?} isn't a user or organization name");
    }
    let v = api.get(&format!("/api/users/{owner}"))?;
    if cx.json() {
        return print_json(&v["repos"]);
    }
    let rows: Vec<Vec<String>> = v["repos"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|r| vec![format!("{}/{}", cell(&r["owner"], 40), cell(&r["name"], 100)), if r["private"] == true { "private" } else { "public" }.into(), cell(&r["description"], 60), ago(&r["updated_at"])])
        .collect();
    table(&["NAME", "VISIBILITY", "DESCRIPTION", "UPDATED"], &rows);
    Ok(())
}

fn repo_view(cx: &Ctx) -> Result<()> {
    let (api, r) = match cx.a.pos(2) {
        Some(s) => cx.named(s)?,
        None => cx.repo()?,
    };
    let v = api.get(&format!("/api/repos/{r}"))?;
    if cx.json() {
        return print_json(&v);
    }
    println!("{r}{}", if v["private"] == true { " (private)" } else { "" });
    if v["description"].as_str().is_some_and(|d| !d.is_empty()) {
        println!("{}", cell(&v["description"], 300));
    }
    println!(
        "default branch {} · {} stars · {} open issues · {} open pull requests\n{}",
        cell(&v["default_branch"], 100),
        v["stars"],
        v["open_issues"],
        v["open_pulls"],
        cell(&v["clone_url"], 300)
    );
    Ok(())
}

fn state(cx: &Ctx) -> Result<&str> {
    match cx.a.get("--state").unwrap_or("open") {
        s @ ("open" | "closed" | "all") => Ok(s),
        s => bail!("--state is open, closed or all, not {s:?}"),
    }
}

fn issue_list(cx: &Ctx) -> Result<()> {
    let (api, r) = cx.repo()?;
    let pr = cx.a.pos(0) == Some("pr");
    let v = api.get(&format!("/api/repos/{r}/issues?state={}{}", state(cx)?, if pr { "&kind=pr" } else { "" }))?;
    if cx.json() {
        return print_json(&v["items"]);
    }
    let rows: Vec<Vec<String>> = v["items"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|i| {
            let mut row = vec![format!("#{}", i["number"]), cell(&i["title"], 70)];
            if pr {
                row.push(format!("{} → {}", cell(&i["head"], 40), cell(&i["base"], 40)));
            }
            row.extend([cell(&i["author"], 40), cell(&i["state"], 10), ago(&i["created_at"])]);
            row
        })
        .collect();
    if pr {
        table(&["#", "TITLE", "BRANCH", "AUTHOR", "STATE", "OPENED"], &rows);
    } else {
        table(&["#", "TITLE", "AUTHOR", "STATE", "OPENED"], &rows);
    }
    Ok(())
}

fn issue_create(cx: &Ctx) -> Result<()> {
    let (api, r) = cx.repo()?;
    let title = cx.a.get("--title").ok_or_else(|| anyhow!("usage: gitter issue create --title T [--body B]"))?;
    let v = api.call("POST", &format!("/api/repos/{r}/issues"), Some(json!({ "title": title, "body": cx.a.get("--body").unwrap_or("") })))?;
    if cx.json() {
        return print_json(&v);
    }
    println!("Created issue #{}\n{}/{r}/issues/{}", v["number"], api.base, v["number"]);
    Ok(())
}

fn issue_view(cx: &Ctx) -> Result<()> {
    let (api, r) = cx.repo()?;
    let n = cx.number(2)?;
    let v = api.get(&format!("/api/repos/{r}/issues/{n}"))?;
    let pr = cx.a.pos(0) == Some("pr");
    if pr && v["issue"]["is_pr"] != true {
        bail!("#{n} is an issue, not a pull request (gitter issue view {n})");
    }
    if cx.json() {
        return print_json(&v);
    }
    let i = &v["issue"];
    println!("{} #{n}\n{} · opened by {} {}", cell(&i["title"], 300), cell(&i["state"], 20), cell(&i["author"], 40), ago(&i["created_at"]));
    let p = &v["pull"];
    if p.is_object() {
        let how = if !p["merged_sha"].is_null() {
            "merged".to_string()
        } else if p["mergeable"] == true {
            "can be merged".into()
        } else if p["reason"] == "conflicts" {
            "has conflicts".into()
        } else {
            cell(&p["reason"], 40)
        };
        println!("{} → {} · {how}", cell(&p["head"], 100), cell(&p["base"], 100));
    }
    if i["body"].as_str().is_some_and(|b| !b.trim().is_empty()) {
        println!("\n{}", clean(i["body"].as_str().unwrap_or(""), false).trim_end());
    }
    for c in v["comments"].as_array().into_iter().flatten() {
        match c["kind"].as_str() {
            Some("comment") => println!("\n— {} {}\n{}", cell(&c["author"], 40), ago(&c["created_at"]), clean(c["body"].as_str().unwrap_or(""), false).trim_end()),
            _ => println!("\n— {} {} this {}", cell(&c["author"], 40), cell(&c["kind"], 20), ago(&c["created_at"])),
        }
    }
    let kind = if i["is_pr"] == true { "pull" } else { "issues" };
    println!("\n{}/{r}/{kind}/{n}", api.base);
    Ok(())
}

fn issue_close(cx: &Ctx) -> Result<()> {
    let (api, r) = cx.repo()?;
    let n = cx.number(2)?;
    let v = api.call("PATCH", &format!("/api/repos/{r}/issues/{n}"), Some(json!({ "state": "closed" })))?;
    if cx.a.has("--json") {
        return print_json(&v);
    }
    println!("Closed #{n}");
    Ok(())
}

fn issue_comment(cx: &Ctx) -> Result<()> {
    let (api, r) = cx.repo()?;
    let n = cx.number(2)?;
    let body = cx.a.get("--body").ok_or_else(|| anyhow!("usage: gitter issue comment N --body B"))?;
    let v = api.call("POST", &format!("/api/repos/{r}/issues/{n}/comments"), Some(json!({ "body": body })))?;
    if cx.a.has("--json") {
        return print_json(&v);
    }
    println!("Commented on #{n}");
    Ok(())
}

fn branch_arg(b: &str) -> Result<&str> {
    if validate::ref_name(b) { Ok(b) } else { bail!("{b:?} isn't a branch name Gitter takes") }
}

fn pr_create(cx: &Ctx) -> Result<()> {
    let (api, r) = cx.repo()?;
    // The current branch is this folder's: pushing it into some other repository has to be asked for.
    let elsewhere = cx.remote_repo().is_none_or(|(h, rr)| h != api.base || !rr.eq_ignore_ascii_case(&r));
    if cx.a.get("--head").is_none() && elsewhere {
        bail!("this folder's remote doesn't point at {r}, so say which branch to push there: pass --head BRANCH");
    }
    let head = match cx.a.get("--head") {
        Some(h) => h.to_string(),
        None => git(&["symbolic-ref", "--short", "-q", "HEAD"], None).map_err(|_| anyhow!("not on a branch: switch to one or pass --head"))?,
    };
    let head = branch_arg(&head)?;
    let base = match cx.a.get("--base") {
        Some(b) => b.to_string(),
        None => api.get(&format!("/api/repos/{r}"))?["default_branch"].as_str().unwrap_or("main").to_string(),
    };
    let local = git(&["rev-parse", "--verify", "-q", &format!("refs/heads/{head}^{{commit}}")], None).map_err(|_| anyhow!("there's no local branch {head}"))?;
    let url = format!("{}/{r}.git", api.base);
    let refname = format!("refs/heads/{head}");
    // ls-remote matches the pattern's tail (refs/heads/x/refs/heads/{head} too): take the exact ref.
    let remote = git(&["ls-remote", &url, &refname], Some(&api))?;
    let remote_sha = remote.lines().find_map(|l| l.split_once('\t').filter(|(_, r)| *r == refname).map(|(s, _)| s.to_string()));
    if remote_sha.as_deref() != Some(local.as_str()) {
        eprintln!("Pushing {head} to {r}…");
        git_loud(&["push", "-q", &url, &format!("refs/heads/{head}:refs/heads/{head}")], Some(&api))?;
    }
    let title = match cx.a.get("--title") {
        Some(t) => t.to_string(),
        None => git(&["log", "-1", "--format=%s", &local], None)?,
    };
    let v = api.call("POST", &format!("/api/repos/{r}/pulls"), Some(json!({ "title": title, "body": cx.a.get("--body").unwrap_or(""), "head": head, "base": base })))?;
    if cx.json() {
        return print_json(&v);
    }
    println!("Created pull request #{} ({head} → {})\n{}/{r}/pull/{}", v["number"], clean(&base, false), api.base, v["number"]);
    Ok(())
}

fn pr_checkout(cx: &Ctx) -> Result<()> {
    let (api, r) = cx.repo()?;
    let n = cx.number(2)?;
    let v = api.get(&format!("/api/repos/{r}/issues/{n}"))?;
    let p = &v["pull"];
    if !p.is_object() {
        bail!("#{n} isn't a pull request");
    }
    if p["head_exists"] == false {
        bail!("#{n}'s branch was deleted on the server");
    }
    let head = branch_arg(p["head"].as_str().unwrap_or(""))?;
    git_loud(&["fetch", "-q", &format!("{}/{r}.git", api.base), &format!("refs/heads/{head}")], Some(&api))?;
    let local = format!("refs/heads/{head}");
    if git(&["rev-parse", "--verify", "-q", &local], None).is_ok() {
        // Already here: bring it up to date, but never throw away local commits (checked before switching).
        if git(&["merge-base", "--is-ancestor", &local, "FETCH_HEAD"], None).is_err() {
            bail!("your branch {head} has commits #{n} doesn't: merge or rename it first (the pull request's head is in FETCH_HEAD)");
        }
        git_loud(&["switch", "-q", head], None)?;
        git_loud(&["merge", "-q", "--ff-only", "FETCH_HEAD"], None)?;
    } else {
        git_loud(&["switch", "-q", "-c", head, "FETCH_HEAD"], None)?;
    }
    println!("Switched to {head} (#{n})");
    Ok(())
}

fn pr_merge(cx: &Ctx) -> Result<()> {
    let (api, r) = cx.repo()?;
    let n = cx.number(2)?;
    let method = cx.a.get("--method").unwrap_or("merge");
    let v = api.call("POST", &format!("/api/repos/{r}/pulls/{n}/merge"), Some(json!({ "method": method, "delete_branch": cx.a.has("--delete-branch") })))?;
    if cx.json() {
        return print_json(&v);
    }
    println!("Merged #{n}");
    Ok(())
}

fn ci_status(cx: &Ctx) -> Result<()> {
    let (api, r) = cx.repo()?;
    if cx.a.pos(2).is_none() {
        let v = api.get(&format!("/api/repos/{r}/ci/runs"))?;
        if cx.json() {
            return print_json(&v);
        }
        let rows: Vec<Vec<String>> = v["runs"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|x| {
                let rf = x["ref"].as_str().unwrap_or("");
                vec![format!("#{}", x["number"]), cell(&x["status"], 12), cell(&x["event"], 14), cell(&json!(rf.strip_prefix("refs/heads/").unwrap_or(rf)), 40), cell(&x["title"], 60), ago(&x["created_at"])]
            })
            .collect();
        table(&["#", "STATUS", "EVENT", "BRANCH", "TITLE", "STARTED"], &rows);
        return Ok(());
    }
    let n = cx.number(2)?;
    let v = api.get(&format!("/api/repos/{r}/ci/runs/{n}"))?;
    if cx.json() {
        return print_json(&v);
    }
    println!("Run #{n} · {} · {} · {} {}", cell(&v["status"], 12), cell(&v["event"], 14), cell(&v["ref"], 100), cell(&v["sha"], 12));
    if v["error"].as_str().is_some_and(|e| !e.is_empty()) {
        println!("{}", cell(&v["error"], 300));
    }
    let rows: Vec<Vec<String>> = v["jobs"].as_array().into_iter().flatten().map(|j| vec![cell(&j["name"], 60), cell(&j["status"], 12), cell(&j["exit_code"], 6), cell(&j["id"], 12)]).collect();
    table(&["JOB", "STATUS", "EXIT", "ID"], &rows);
    println!("{}/{r}/ci/{n}", api.base);
    Ok(())
}

fn ci_logs(cx: &Ctx) -> Result<()> {
    let (api, r) = cx.repo()?;
    let n = cx.number(2)?;
    let v = api.get(&format!("/api/repos/{r}/ci/runs/{n}"))?;
    let want = cx.a.get("--job");
    let jobs: Vec<&Value> = v["jobs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|j| want.is_none_or(|w| j["key"] == w || j["name"] == w || w.parse::<i64>().ok() == j["id"].as_i64()))
        .collect();
    if jobs.is_empty() {
        bail!("run #{n} has no job {}", want.unwrap_or(""));
    }
    let mut out = std::io::stdout();
    for j in &jobs {
        let id = j["id"].as_i64().ok_or_else(|| anyhow!("the server sent a job without an id"))?;
        if jobs.len() > 1 {
            println!("\x1b[1m━━ {}\x1b[0m", cell(&j["name"], 100));
        }
        // ponytail: polls once a second, like a slow `tail -f`; the SSE stream if that's ever too slow.
        let mut offset = 0usize;
        loop {
            let (status, job_status, data) = api.raw("GET", &format!("/api/repos/{r}/ci/jobs/{id}/log?offset={offset}"), None)?;
            if status >= 400 {
                bail!("can't read job {id}'s log (HTTP {status})");
            }
            offset += data.len();
            out.write_all(clean(&String::from_utf8_lossy(&data), true).as_bytes())?;
            out.flush()?;
            let done = job_status.as_deref().is_none_or(crate::ci::queue::terminal);
            if data.is_empty() && done {
                break;
            }
            if data.is_empty() {
                std::thread::sleep(Duration::from_secs(1));
            }
        }
    }
    Ok(())
}

fn ci_rerun(cx: &Ctx) -> Result<()> {
    let (api, r) = cx.repo()?;
    let n = cx.number(2)?;
    let v = api.call("POST", &format!("/api/repos/{r}/ci/runs/{n}/rerun"), Some(json!({ "all": cx.a.has("--all") })))?;
    if cx.a.has("--json") {
        return print_json(&v);
    }
    println!("Re-running run #{n}: gitter ci logs {n} to follow it");
    Ok(())
}

// ---------------------------------------------------------------- gitter ci run

/// Copy what git would see in `root` (tracked files and untracked ones that
/// aren't ignored, as they are now) into `to`. `.git` stays behind.
fn copy_tree(root: &Path, to: &Path) -> Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let out = Command::new("git").args(["ls-files", "-z", "--cached", "--others", "--exclude-standard"]).current_dir(root).stdin(Stdio::null()).output()?;
    if !out.status.success() {
        bail!("git ls-files: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    let mut files: Vec<&[u8]> = out.stdout.split(|b| *b == 0).filter(|f| !f.is_empty()).collect();
    files.sort_unstable();
    files.dedup(); // a conflicted file is listed once per stage
    for f in files {
        let rel = Path::new(std::ffi::OsStr::from_bytes(f));
        let (from, dest) = (root.join(rel), to.join(rel));
        // Deleted in the work tree: gone from the copy too.
        let Ok(m) = std::fs::symlink_metadata(&from) else { continue };
        if let Some(p) = dest.parent() {
            std::fs::create_dir_all(p)?;
        }
        if m.file_type().is_symlink() {
            std::os::unix::fs::symlink(std::fs::read_link(&from)?, &dest)?;
        } else if m.is_file() {
            std::fs::copy(&from, &dest).with_context(|| format!("copying {}", rel.display()))?;
        }
    }
    Ok(())
}

/// Removes the scratch folder however the run ends.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn ci_run(cx: &Ctx) -> Result<()> {
    use crate::ci::config::{CONFIG_PATH, SCRIPT_PATH, parse, script_pipeline};
    if cx.a.pos.len() > 2 {
        bail!("usage: gitter ci run [--here]");
    }
    let here = cx.a.has("--here");
    let root = match git(&["rev-parse", "--show-toplevel"], None) {
        Ok(r) => PathBuf::from(r),
        Err(_) if here => std::env::current_dir()?,
        Err(_) => bail!("this isn't a git repository: run it inside one, or pass --here to run in this folder"),
    };
    let pipeline = match std::fs::read_to_string(root.join(CONFIG_PATH)) {
        Ok(src) => parse(&src).map_err(|e| anyhow!("{CONFIG_PATH}: {e}"))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && root.join(SCRIPT_PATH).is_file() => script_pipeline(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => bail!("there's no {CONFIG_PATH} or {SCRIPT_PATH} in {}", root.display()),
        Err(e) => return Err(e).context(CONFIG_PATH),
    };
    // A container gets its work tree mounted and its files chowned: never the folder itself.
    if let Some(j) = pipeline.jobs.iter().find(|j| j.image.is_some()).filter(|_| here) {
        bail!("job {} runs in an image, which would take over this folder's files: run it without --here (in a copy)", j.name);
    }
    let sha = git(&["rev-parse", "-q", "--verify", "HEAD"], None).unwrap_or_default();
    let branch = git(&["symbolic-ref", "--short", "-q", "HEAD"], None).unwrap_or_default();
    let folder = root.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
    let scratch = Scratch(std::env::temp_dir().join(format!("gitter-ci-{}-{:08x}", std::process::id(), rand::random::<u32>())));
    // Private to you, and never an existing path (a planted symlink is refused, not followed).
    let mut dir = std::fs::DirBuilder::new();
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut dir, 0o700);
    dir.create(&scratch.0).with_context(|| format!("creating {}", scratch.0.display()))?;

    let jobs = pipeline.jobs;
    // None: not run yet; Some(status).
    let mut done: Vec<Option<(&'static str, Option<i32>)>> = vec![None; jobs.len()];
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    rt.spawn(async {
        if tokio::signal::ctrl_c().await.is_ok() {
            crate::ci::runner::SHUTDOWN.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    });
    let stopped = || crate::ci::runner::SHUTDOWN.load(std::sync::atomic::Ordering::SeqCst);
    // Some(true) once every job of `key` succeeded, Some(false) once one didn't, None while some haven't run.
    let key_state = |done: &[Option<(&str, Option<i32>)>], key: &str| -> Option<bool> {
        let mine: Vec<_> = jobs.iter().zip(done).filter(|(j, _)| j.key == key).map(|(_, d)| *d).collect();
        if mine.iter().any(|d| d.is_none()) {
            return None;
        }
        Some(mine.iter().all(|d| matches!(d, Some(("success", _)))))
    };
    // The first job (in file order) whose needs are all settled; there are no loops (parse checks).
    while let Some(i) = (0..jobs.len()).find(|&i| done[i].is_none() && jobs[i].needs.iter().all(|k| key_state(&done, k).is_some())) {
        let job = &jobs[i];
        if stopped() || job.needs.iter().any(|k| key_state(&done, k) == Some(false)) {
            done[i] = Some(("skipped", None));
            continue;
        }
        println!("\x1b[1m━━ {}\x1b[0m", job.name);
        let (ws, home) = (if here { root.clone() } else { scratch.0.join(format!("job-{i}")) }, scratch.0.join(format!("job-{i}-home")));
        std::fs::create_dir_all(&home)?;
        if !here {
            std::fs::create_dir_all(&ws)?;
            copy_tree(&root, &ws)?;
        }
        let lease = crate::ci::queue::Lease {
            job_id: i as i64,
            attempt: 1,
            token: String::new(),
            run_id: 0,
            repo_id: 0,
            repo: folder.clone(),
            sha: sha.clone(),
            refname: if branch.is_empty() { String::new() } else { format!("refs/heads/{branch}") },
            event: "local".into(),
            run_number: 0,
            job: job.clone(),
            secrets: vec![],
            limits: Default::default(),
        };
        let (ok, code) = rt.block_on(crate::ci::runner::run_here(&lease, &ws, &home));
        done[i] = Some((if ok { "success" } else { "failure" }, code));
        if !here {
            let _ = std::fs::remove_dir_all(&ws);
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    println!();
    let mut failed = false;
    for (j, d) in jobs.iter().zip(&done) {
        match d {
            Some(("success", _)) => println!("\x1b[32m✓\x1b[0m {}", j.name),
            Some(("failure", code)) => {
                failed = true;
                println!("\x1b[31m✗\x1b[0m {} (exit code {})", j.name, code.unwrap_or(-1));
            }
            _ => {
                failed = true;
                println!("\x1b[90m-\x1b[0m {} (skipped)", j.name);
            }
        }
    }
    drop(scratch);
    if stopped() {
        std::process::exit(130);
    }
    if failed {
        std::process::exit(1);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_text_is_cleaned_and_remotes_parsed() {
        assert_eq!(clean("a\x1b]0;pwned\x07b\x1b[2Jc\r\n", false), "a]0;pwnedb[2Jc\n");
        assert_eq!(clean("\x1b[31mred\x1b[0m\x1b[2J\r", true), "\x1b[31mred\x1b[0m[2J\r\x1b[0m", "colors are reset at the end");
        assert_eq!(clean("\x1b[8mhidden", true), "\x1b[8mhidden\x1b[0m");
        assert_eq!(clean("abc\u{202e}fed\u{200b}", false), "abcfed", "no bidi overrides or zero-width characters");
        assert_eq!(clean("\x1b[", true), "[");
        assert_eq!(strip_userinfo("https://me:pw@git.example.com/a/b.git"), "https://git.example.com/a/b.git");
        assert_eq!(repo_name("alice/proj.git/").as_deref(), Some("alice/proj"));
        assert_eq!(repo_name("alice/proj/extra"), None);
        assert_eq!(repo_name("-x/y"), None);
    }

    #[test]
    fn every_command_has_a_usage_line() {
        for (c, s, _) in COMMANDS {
            let u = usage_of(c, s);
            assert!(u.trim_start().starts_with(&format!("gitter {c}")), "gitter {c} {s} has no usage line");
        }
    }

    #[test]
    fn plain_http_is_flagged_unless_it_stays_on_this_machine() {
        for u in ["http://git.example.com", "http://10.0.0.5:3000", "http://localhost.example.com", "http://[2001:db8::1]:80"] {
            assert!(plain_elsewhere(u), "{u}");
        }
        for u in ["https://git.example.com", "http://localhost:3000", "http://127.0.0.1:8080", "http://127.5.0.1", "http://[::1]:3000", "http://LOCALHOST"] {
            assert!(!plain_elsewhere(u), "{u}");
        }
    }
}
