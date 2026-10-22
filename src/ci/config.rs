//! `.gitter/ci.yml` (or `.gitter/ci.sh`): what to run, and when.
//!
//! The YAML is a documented subset: block mappings and lists, `- ` items,
//! `|` and `>` block text, quoted or plain scalars, `[a, b]` and `{a: b}`
//! flow collections, and `#` comments. Anchors, tags and multi-document
//! files are not supported (and give a clear error).

use std::collections::BTreeMap;

use serde_json::{Map, Value};

// ---------------------------------------------------------------- YAML subset

struct Line<'a> {
    no: usize,
    indent: usize,
    text: &'a str,
    /// The line before comment stripping (block text keeps its `#`s).
    raw: &'a str,
}

/// Parse the YAML subset into JSON values. Errors name the line.
/// Nesting deeper than this is refused (real pipelines use a handful of levels).
const MAX_DEPTH: usize = 32;
/// A pipeline file larger than this is refused before parsing.
pub const MAX_CONFIG_BYTES: u64 = 256 * 1024;

pub fn parse_yaml(src: &str) -> Result<Value, String> {
    let mut lines = Vec::new();
    for (i, raw) in src.lines().enumerate() {
        let no = i + 1;
        if raw.chars().take_while(|c| c.is_whitespace()).any(|c| c == '\t') {
            return Err(format!("line {no}: indent with spaces, not tabs"));
        }
        let full = raw.trim_end();
        let text = strip_comment(raw).trim_end();
        let body = text.trim_start();
        if body.is_empty() {
            lines.push(Line { no, indent: usize::MAX, text: "", raw: full }); // blank: kept for block text
            continue;
        }
        if body == "---" && lines.iter().all(|l: &Line| l.text.is_empty()) {
            continue;
        }
        if body.starts_with('&') || body.starts_with('*') || body.starts_with('!') || body == "---" || body == "..." {
            return Err(format!("line {no}: anchors, tags and multiple documents aren't supported"));
        }
        lines.push(Line { no, indent: text.len() - body.len(), text: body, raw: full });
    }
    let mut p = Parser { lines, pos: 0, depth: 0 };
    p.skip_blank();
    if p.pos >= p.lines.len() {
        return Ok(Value::Object(Map::new()));
    }
    let indent = p.lines[p.pos].indent;
    let v = p.block(indent)?;
    p.skip_blank();
    if let Some(l) = p.lines.get(p.pos) {
        return Err(format!("line {}: unexpected indentation", l.no));
    }
    Ok(v)
}

fn strip_comment(s: &str) -> &str {
    let (mut sq, mut dq) = (false, false);
    let b = s.as_bytes();
    for (i, &c) in b.iter().enumerate() {
        match c {
            b'\'' if !dq => sq = !sq,
            b'"' if !sq => dq = !dq,
            b'#' if !sq && !dq && (i == 0 || b[i - 1] == b' ') => return &s[..i],
            _ => {}
        }
    }
    s
}

struct Parser<'a> {
    lines: Vec<Line<'a>>,
    pos: usize,
    depth: usize,
}

impl<'a> Parser<'a> {
    fn skip_blank(&mut self) {
        while self.lines.get(self.pos).is_some_and(|l| l.text.is_empty()) {
            self.pos += 1;
        }
    }

    fn peek(&mut self) -> Option<(usize, &'a str, usize)> {
        self.skip_blank();
        self.lines.get(self.pos).map(|l| (l.indent, l.text, l.no))
    }

    /// A mapping or list whose lines start at exactly `indent`.
    fn block(&mut self, indent: usize) -> Result<Value, String> {
        let (_, text, no) = self.peek().expect("caller checked");
        // Recursion is bounded so a hostile file can't overflow the stack.
        if self.depth >= MAX_DEPTH {
            return Err(format!("line {no}: nested more than {MAX_DEPTH} levels deep"));
        }
        self.depth += 1;
        let v = if text == "-" || text.starts_with("- ") { self.list(indent) } else { self.map(indent, None) };
        self.depth -= 1;
        v
    }

    fn list(&mut self, indent: usize) -> Result<Value, String> {
        let mut out = Vec::new();
        while let Some((ind, text, no)) = self.peek() {
            if ind != indent || !(text == "-" || text.starts_with("- ")) {
                if ind > indent {
                    return Err(format!("line {no}: unexpected indentation"));
                }
                break;
            }
            self.pos += 1;
            let rest = text[1..].trim_start();
            let col = indent + (text.len() - rest.len());
            if rest.is_empty() {
                out.push(self.child(indent)?);
            } else if split_key(rest).is_some() && !rest.starts_with('{') && !rest.starts_with('"') && !rest.starts_with('\'') {
                // "- key: value" starts a mapping whose keys sit at the column after "- ".
                out.push(self.map(col, Some((rest, no)))?);
            } else {
                out.push(scalar(rest, no)?);
            }
        }
        Ok(Value::Array(out))
    }

    fn map(&mut self, indent: usize, first: Option<(&'a str, usize)>) -> Result<Value, String> {
        let mut out = Map::new();
        let mut pending = first.map(|(t, n)| (t.to_string(), n));
        loop {
            let (text, no) = match pending.take() {
                Some(x) => x,
                None => match self.peek() {
                    Some((ind, text, no)) if ind == indent => {
                        let t = text.to_string();
                        self.pos += 1;
                        (t, no)
                    }
                    Some((ind, _, no)) if ind > indent => return Err(format!("line {no}: unexpected indentation")),
                    _ => break,
                },
            };
            if text.starts_with("- ") || text == "-" {
                return Err(format!("line {no}: a list item where a 'key: value' was expected"));
            }
            let (key, rest) = split_key(&text).ok_or_else(|| format!("line {no}: expected 'key: value'"))?;
            let key = unquote(key, no)?;
            if out.contains_key(&key) {
                return Err(format!("line {no}: '{key}' appears twice"));
            }
            let v = match rest {
                "" => self.child(indent)?,
                r if r.starts_with('|') || r.starts_with('>') => self.text_block(indent, r, no)?,
                r => scalar(r, no)?,
            };
            out.insert(key, v);
        }
        Ok(Value::Object(out))
    }

    /// The value after "key:" with nothing on the line: an indented block, a
    /// list at the same indent ("steps:\n- run: x"), or null.
    fn child(&mut self, indent: usize) -> Result<Value, String> {
        match self.peek() {
            Some((ind, _, _)) if ind > indent => self.block(ind),
            Some((ind, text, _)) if ind == indent && (text == "-" || text.starts_with("- ")) => self.list(ind),
            _ => Ok(Value::Null),
        }
    }

    /// `|` keeps newlines, `>` folds them into spaces; `-` drops the final newline.
    fn text_block(&mut self, indent: usize, head: &str, no: usize) -> Result<Value, String> {
        let folded = head.starts_with('>');
        let strip = head[1..].trim() == "-";
        if !matches!(head[1..].trim(), "" | "-" | "+") {
            return Err(format!("line {no}: only |, |-, > and >- block text are supported"));
        }
        let start = self.pos;
        let mut end = start;
        let mut base = usize::MAX;
        // Block text is read raw: a `#` inside it is text, not a comment.
        let raw_indent = |l: &Line| l.raw.len() - l.raw.trim_start().len();
        while let Some(l) = self.lines.get(end) {
            if l.raw.trim().is_empty() {
                end += 1;
                continue;
            }
            if raw_indent(l) <= indent {
                break;
            }
            base = base.min(raw_indent(l));
            end += 1;
        }
        // Trailing blank lines belong to what follows.
        while end > start && self.lines[end - 1].raw.trim().is_empty() {
            end -= 1;
        }
        let mut parts = Vec::new();
        for l in &self.lines[start..end] {
            if l.raw.trim().is_empty() {
                parts.push(String::new());
            } else {
                parts.push(format!("{}{}", " ".repeat(raw_indent(l) - base), l.raw.trim_start()));
            }
        }
        self.pos = end;
        let mut s = if folded { parts.join(" ") } else { parts.join("\n") };
        if !strip {
            s.push('\n');
        }
        Ok(Value::String(s))
    }
}

/// "key: rest" → (key, rest). The colon must be outside quotes and followed by a space or the end.
fn split_key(s: &str) -> Option<(&str, &str)> {
    let (mut sq, mut dq) = (false, false);
    let b = s.as_bytes();
    for (i, &c) in b.iter().enumerate() {
        match c {
            b'\'' if !dq => sq = !sq,
            b'"' if !sq => dq = !dq,
            b'[' | b'{' if !sq && !dq && i == 0 => return None,
            b':' if !sq && !dq && (i + 1 == b.len() || b[i + 1] == b' ') => {
                return Some((s[..i].trim(), s[i + 1..].trim()));
            }
            _ => {}
        }
    }
    None
}

fn unquote(s: &str, no: usize) -> Result<String, String> {
    match scalar(s, no)? {
        Value::String(s) => Ok(s),
        other => Ok(other.to_string()),
    }
}

fn scalar(s: &str, no: usize) -> Result<Value, String> {
    let s = s.trim();
    if let Some(inner) = s.strip_prefix('"') {
        let inner = inner.strip_suffix('"').ok_or_else(|| format!("line {no}: unclosed \""))?;
        let mut out = String::new();
        let mut it = inner.chars();
        while let Some(c) = it.next() {
            if c == '\\' {
                match it.next() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some('"') => out.push('"'),
                    Some('\\') => out.push('\\'),
                    Some(o) => {
                        out.push('\\');
                        out.push(o);
                    }
                    None => {}
                }
            } else {
                out.push(c);
            }
        }
        return Ok(Value::String(out));
    }
    if let Some(inner) = s.strip_prefix('\'') {
        let inner = inner.strip_suffix('\'').ok_or_else(|| format!("line {no}: unclosed '"))?;
        return Ok(Value::String(inner.replace("''", "'")));
    }
    if s.starts_with('[') || s.starts_with('{') {
        return flow(s, no);
    }
    if s.starts_with('&') || s.starts_with('*') || s.starts_with('!') {
        return Err(format!("line {no}: anchors, aliases and tags aren't supported (quote the value if it's text)"));
    }
    Ok(match s {
        "~" | "null" | "Null" | "NULL" => Value::Null,
        "true" | "True" | "TRUE" => Value::Bool(true),
        "false" | "False" | "FALSE" => Value::Bool(false),
        _ => {
            if let Ok(n) = s.parse::<i64>() {
                Value::from(n)
            } else if let Ok(f) = s.parse::<f64>()
                && s.chars().any(|c| c.is_ascii_digit())
                && !s.contains(char::is_alphabetic)
            {
                Value::from(f)
            } else {
                Value::String(s.to_string())
            }
        }
    })
}

/// `[a, b, "c, d"]` and `{a: 1, b: [x]}` on one line.
fn flow(s: &str, no: usize) -> Result<Value, String> {
    let (open, close) = (s.as_bytes()[0], if s.starts_with('[') { b']' } else { b'}' });
    if s.bytes().filter(|b| matches!(b, b'[' | b'{')).count() > MAX_DEPTH {
        return Err(format!("line {no}: too many nested [ ] or {{ }}"));
    }
    if !s.ends_with(close as char) {
        return Err(format!("line {no}: unclosed {}", open as char));
    }
    let inner = &s[1..s.len() - 1];
    let mut parts = Vec::new();
    let (mut depth, mut sq, mut dq, mut start) = (0i32, false, false, 0);
    for (i, c) in inner.char_indices() {
        match c {
            '\'' if !dq => sq = !sq,
            '"' if !sq => dq = !dq,
            '[' | '{' if !sq && !dq => depth += 1,
            ']' | '}' if !sq && !dq => depth -= 1,
            ',' if !sq && !dq && depth == 0 => {
                parts.push(&inner[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&inner[start..]);
    let parts: Vec<&str> = parts.into_iter().map(str::trim).filter(|p| !p.is_empty()).collect();
    if open == b'[' {
        Ok(Value::Array(parts.into_iter().map(|p| scalar(p, no)).collect::<Result<_, _>>()?))
    } else {
        let mut m = Map::new();
        for p in parts {
            let (k, v) = split_key(p).ok_or_else(|| format!("line {no}: expected key: value inside {{}}"))?;
            m.insert(unquote(k, no)?, if v.is_empty() { Value::Null } else { scalar(v, no)? });
        }
        Ok(Value::Object(m))
    }
}

// ---------------------------------------------------------------- pipeline

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Filter {
    /// Branch globs; empty means every branch.
    pub branches: Vec<String>,
    /// Changed-file globs; empty means any change.
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Triggers {
    pub push: Option<Filter>,
    pub pull_request: Option<Filter>,
    pub manual: bool,
    /// Five-field cron lines (UTC), run on the default branch.
    pub schedule: Vec<String>,
}

impl Triggers {
    /// What a pipeline without `on:` runs on: pushes, pull requests and the Run button.
    pub fn all() -> Triggers {
        Triggers { push: Some(Filter::default()), pull_request: Some(Filter::default()), manual: true, schedule: vec![] }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Step {
    pub name: String,
    pub run: String,
}

/// One job after matrix expansion.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Job {
    /// Name shown in the UI, e.g. "test (stable)".
    pub name: String,
    /// The job's key in the file (what `needs` refers to).
    pub key: String,
    pub image: Option<String>,
    pub needs: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub paths: Vec<String>,
    pub timeout_secs: u64,
    pub steps: Vec<Step>,
    pub artifacts: Vec<String>,
    /// Runner labels the job needs.
    pub runs_on: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pipeline {
    pub triggers: Triggers,
    pub jobs: Vec<Job>,
}

pub const CONFIG_PATH: &str = ".gitter/ci.yml";
pub const SCRIPT_PATH: &str = ".gitter/ci.sh";
const MAX_JOBS: usize = 100;

/// The pipeline for a plain `.gitter/ci.sh`: one job running it.
pub fn script_pipeline() -> Pipeline {
    Pipeline {
        triggers: Triggers::all(),
        jobs: vec![Job {
            name: "ci".into(),
            key: "ci".into(),
            image: None,
            needs: vec![],
            env: BTreeMap::new(),
            paths: vec![],
            timeout_secs: 30 * 60,
            steps: vec![Step { name: SCRIPT_PATH.into(), run: format!("sh {SCRIPT_PATH}") }],
            artifacts: vec![],
            runs_on: vec![],
        }],
    }
}

fn strings(v: &Value, what: &str) -> Result<Vec<String>, String> {
    match v {
        Value::Null => Ok(vec![]),
        Value::String(s) => Ok(vec![s.clone()]),
        Value::Array(a) => a
            .iter()
            .map(|x| match x {
                Value::String(s) => Ok(s.clone()),
                Value::Number(n) => Ok(n.to_string()),
                Value::Bool(b) => Ok(b.to_string()),
                _ => Err(format!("{what}: expected a list of text")),
            })
            .collect(),
        Value::Number(n) => Ok(vec![n.to_string()]),
        _ => Err(format!("{what}: expected text or a list")),
    }
}

fn filter(v: &Value, what: &str) -> Result<Filter, String> {
    match v {
        Value::Null | Value::Bool(true) => Ok(Filter::default()),
        Value::Object(m) => {
            for k in m.keys() {
                if !matches!(k.as_str(), "branches" | "paths") {
                    return Err(format!("{what}: unknown setting '{k}' (use branches or paths)"));
                }
            }
            Ok(Filter {
                branches: strings(m.get("branches").unwrap_or(&Value::Null), &format!("{what}.branches"))?,
                paths: strings(m.get("paths").unwrap_or(&Value::Null), &format!("{what}.paths"))?,
            })
        }
        _ => Err(format!("{what}: expected branches/paths or nothing")),
    }
}

fn triggers(v: Option<&Value>) -> Result<Triggers, String> {
    let Some(v) = v else { return Ok(Triggers::all()) };
    let mut t = Triggers::default();
    let set = |name: &str, val: &Value, t: &mut Triggers| -> Result<(), String> {
        match name {
            "push" => t.push = Some(filter(val, "on.push")?),
            "pull_request" => t.pull_request = Some(filter(val, "on.pull_request")?),
            "manual" => t.manual = !matches!(val, Value::Bool(false)),
            "schedule" => {
                for c in strings(val, "on.schedule")? {
                    Cron::parse(&c).map_err(|e| format!("on.schedule: {e}"))?;
                    t.schedule.push(c);
                }
            }
            other => return Err(format!("on: unknown trigger '{other}' (use push, pull_request, manual or schedule)")),
        }
        Ok(())
    };
    match v {
        Value::String(s) => set(s, &Value::Null, &mut t)?,
        Value::Array(a) => {
            for x in a {
                let s = x.as_str().ok_or("on: expected trigger names")?;
                set(s, &Value::Null, &mut t)?;
            }
        }
        Value::Object(m) => {
            for (k, val) in m {
                set(k, val, &mut t)?;
            }
        }
        _ => return Err("on: expected push, pull_request, manual or schedule".into()),
    }
    Ok(t)
}

/// "30m", "1h", "90s", or a number of minutes.
fn duration(v: &Value) -> Result<u64, String> {
    let secs = match v {
        Value::Null => return Ok(30 * 60),
        Value::Number(n) => n.as_u64().and_then(|n| n.checked_mul(60)).ok_or("timeout: use a whole number of minutes")?,
        Value::String(s) => {
            let (num, unit) = s.trim().split_at(s.trim().find(|c: char| !c.is_ascii_digit()).unwrap_or(s.trim().len()));
            let n: u64 = num.parse().map_err(|_| format!("timeout: can't read '{s}' (try 30m)"))?;
            let unit = match unit.trim() {
                "s" => 1,
                "m" | "" => 60,
                "h" => 3600,
                _ => return Err(format!("timeout: can't read '{s}' (try 30m)")),
            };
            n.saturating_mul(unit)
        }
        _ => return Err("timeout: try 30m".into()),
    };
    if !(1..=6 * 3600).contains(&secs) {
        return Err("timeout: between 1 second and 6 hours".into());
    }
    Ok(secs)
}

fn steps(v: &Value, job: &str) -> Result<Vec<Step>, String> {
    let list = match v {
        Value::Array(a) => a.clone(),
        Value::String(_) => vec![v.clone()],
        _ => return Err(format!("jobs.{job}.steps: expected a list of steps")),
    };
    let mut out = Vec::new();
    for (i, s) in list.iter().enumerate() {
        let step = match s {
            Value::String(cmd) => Step { name: first_line(cmd), run: cmd.clone() },
            Value::Object(m) => {
                for k in m.keys() {
                    if !matches!(k.as_str(), "name" | "run") {
                        return Err(format!("jobs.{job}.steps[{}]: unknown setting '{k}' (use name and run)", i + 1));
                    }
                }
                let run = m.get("run").and_then(Value::as_str).ok_or(format!("jobs.{job}.steps[{}]: needs a run command", i + 1))?;
                let name = m.get("name").and_then(Value::as_str).map(String::from).unwrap_or_else(|| first_line(run));
                Step { name, run: run.to_string() }
            }
            _ => return Err(format!("jobs.{job}.steps[{}]: expected a command or name/run", i + 1)),
        };
        out.push(step);
    }
    if out.is_empty() {
        return Err(format!("jobs.{job}: add at least one step"));
    }
    Ok(out)
}

fn first_line(s: &str) -> String {
    let l = s.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
    if l.chars().count() > 60 { format!("{}…", l.chars().take(60).collect::<String>()) } else { l.to_string() }
}

/// Replace `${{ matrix.key }}` (or `${{matrix.key}}`) with the matrix values.
fn interpolate(s: &str, vars: &[(String, String)]) -> String {
    let mut out = s.to_string();
    for (k, v) in vars {
        for pat in [format!("${{{{ matrix.{k} }}}}"), format!("${{{{matrix.{k}}}}}")] {
            out = out.replace(&pat, v);
        }
    }
    out
}

/// Parse a pipeline file (YAML subset) into jobs, expanding matrices.
pub fn parse(src: &str) -> Result<Pipeline, String> {
    let v = parse_yaml(src)?;
    let Value::Object(top) = v else { return Err("the file should be a mapping with 'jobs:'".into()) };
    for k in top.keys() {
        if !matches!(k.as_str(), "on" | "jobs" | "env" | "name") {
            return Err(format!("unknown top-level setting '{k}' (use on, env and jobs)"));
        }
    }
    let triggers = triggers(top.get("on"))?;
    let global_env = env_map(top.get("env"), "env")?;
    let Some(Value::Object(jobs)) = top.get("jobs") else { return Err("add a 'jobs:' section".into()) };
    let mut out = Vec::new();
    let keys: Vec<&String> = jobs.keys().collect();
    for (key, spec) in jobs {
        if !crate::validate::name(key, 60) {
            return Err(format!("jobs.{key}: job names use letters, numbers, '-', '_' or '.'"));
        }
        let Value::Object(m) = spec else { return Err(format!("jobs.{key}: expected steps and settings")) };
        for k in m.keys() {
            if !matches!(k.as_str(), "steps" | "image" | "needs" | "env" | "matrix" | "paths" | "timeout" | "artifacts" | "runs-on" | "name") {
                return Err(format!("jobs.{key}: unknown setting '{k}'"));
            }
        }
        let needs = strings(m.get("needs").unwrap_or(&Value::Null), &format!("jobs.{key}.needs"))?;
        for n in &needs {
            if !keys.iter().any(|k| *k == n) || n == key {
                return Err(format!("jobs.{key}.needs: there's no other job called '{n}'"));
            }
        }
        let mut env = global_env.clone();
        env.extend(env_map(m.get("env"), &format!("jobs.{key}.env"))?);
        let steps = steps(m.get("steps").unwrap_or(&Value::Null), key)?;
        let image = m.get("image").and_then(Value::as_str).map(String::from);
        if image.as_ref().is_some_and(|i| i.len() > 256) {
            return Err(format!("jobs.{key}.image: too long"));
        }
        let matrix = matrix(m.get("matrix"), key)?;
        let title = m.get("name").and_then(Value::as_str).unwrap_or(key).to_string();
        for combo in &matrix {
            let image = image.as_deref().map(|i| interpolate(i, combo));
            if let Some(i) = &image
                && (i.starts_with('-') || i.is_empty() || !i.chars().all(|c| c.is_ascii_alphanumeric() || "._-/:@".contains(c)))
            {
                return Err(format!("jobs.{key}.image: '{i}' isn't an image name"));
            }
            let suffix = if combo.is_empty() { String::new() } else { format!(" ({})", combo.iter().map(|(_, v)| v.as_str()).collect::<Vec<_>>().join(", ")) };
            let mut env = env.clone();
            for (k, v) in combo {
                env.insert(format!("MATRIX_{}", k.to_ascii_uppercase().replace('-', "_")), v.clone());
            }
            let env = env.into_iter().map(|(k, v)| (k, interpolate(&v, combo))).collect();
            out.push(Job {
                name: format!("{title}{suffix}"),
                key: key.clone(),
                image,
                needs: needs.clone(),
                env,
                paths: strings(m.get("paths").unwrap_or(&Value::Null), &format!("jobs.{key}.paths"))?,
                timeout_secs: duration(m.get("timeout").unwrap_or(&Value::Null)).map_err(|e| format!("jobs.{key}.{e}"))?,
                steps: steps.iter().map(|s| Step { name: interpolate(&s.name, combo), run: interpolate(&s.run, combo) }).collect(),
                artifacts: strings(m.get("artifacts").unwrap_or(&Value::Null), &format!("jobs.{key}.artifacts"))?,
                runs_on: strings(m.get("runs-on").unwrap_or(&Value::Null), &format!("jobs.{key}.runs-on"))?,
            });
            if out.len() > MAX_JOBS {
                return Err(format!("a pipeline can have at most {MAX_JOBS} jobs (after matrix expansion)"));
            }
        }
    }
    if out.is_empty() {
        return Err("add at least one job under 'jobs:'".into());
    }
    check_cycles(&out)?;
    Ok(Pipeline { triggers, jobs: out })
}

fn env_map(v: Option<&Value>, what: &str) -> Result<BTreeMap<String, String>, String> {
    let mut out = BTreeMap::new();
    match v {
        None | Some(Value::Null) => {}
        Some(Value::Object(m)) => {
            for (k, v) in m {
                if !k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') || k.starts_with(|c: char| c.is_ascii_digit()) {
                    return Err(format!("{what}: '{k}' isn't a valid variable name"));
                }
                let s = match v {
                    Value::String(s) => s.clone(),
                    Value::Null => String::new(),
                    other => other.to_string(),
                };
                out.insert(k.clone(), s);
            }
        }
        _ => return Err(format!("{what}: expected NAME: value pairs")),
    }
    Ok(out)
}

/// Every combination of the matrix values, in file order (one empty combination without a matrix).
fn matrix(v: Option<&Value>, job: &str) -> Result<Vec<Vec<(String, String)>>, String> {
    let mut combos: Vec<Vec<(String, String)>> = vec![vec![]];
    let Some(v) = v.filter(|v| !v.is_null()) else { return Ok(combos) };
    let Value::Object(m) = v else { return Err(format!("jobs.{job}.matrix: expected name: [values]")) };
    for (k, vals) in m {
        let vals = strings(vals, &format!("jobs.{job}.matrix.{k}"))?;
        if vals.is_empty() {
            return Err(format!("jobs.{job}.matrix.{k}: give at least one value"));
        }
        combos = combos
            .into_iter()
            .flat_map(|c| vals.iter().map(move |v| { let mut c = c.clone(); c.push((k.clone(), v.clone())); c }))
            .collect();
        if combos.len() > 50 {
            return Err(format!("jobs.{job}.matrix: at most 50 combinations"));
        }
    }
    Ok(combos)
}

fn check_cycles(jobs: &[Job]) -> Result<(), String> {
    let deps: BTreeMap<&str, Vec<&str>> = jobs.iter().map(|j| (j.key.as_str(), j.needs.iter().map(String::as_str).collect())).collect();
    fn visit<'a>(k: &'a str, deps: &BTreeMap<&'a str, Vec<&'a str>>, path: &mut Vec<&'a str>, done: &mut Vec<&'a str>) -> Result<(), String> {
        if done.contains(&k) {
            return Ok(());
        }
        if path.contains(&k) {
            return Err(format!("jobs wait on each other in a loop: {} → {k}", path.join(" → ")));
        }
        path.push(k);
        for d in deps.get(k).into_iter().flatten() {
            visit(d, deps, path, done)?;
        }
        path.pop();
        done.push(k);
        Ok(())
    }
    let mut done = Vec::new();
    for k in deps.keys() {
        visit(k, &deps, &mut Vec::new(), &mut done)?;
    }
    Ok(())
}

// ---------------------------------------------------------------- globs

/// `**` matches across folders, `*` within one, `?` one character. A pattern
/// without a `/` also matches the file name alone ("*.md" anywhere).
pub fn glob(pattern: &str, path: &str) -> bool {
    /// `**` (any run, slashes too), `*` (no slashes), `?` and literal bytes, memoised so
    /// a pattern like `*a*a*a*a*b` costs at most pattern × path steps instead of exploding.
    fn m(p: &[u8], s: &[u8]) -> bool {
        fn go(p: &[u8], s: &[u8], i: usize, j: usize, memo: &mut [u8], w: usize) -> bool {
            let k = i * w + j;
            if memo[k] != 0 {
                return memo[k] == 1;
            }
            let r = match (p.get(i), s.get(j)) {
                (None, None) => true,
                (Some(b'*'), c) if p.get(i + 1) == Some(&b'*') => {
                    let rest = (if p.get(i + 2) == Some(&b'/') { i + 3 } else { i + 2 }).min(p.len());
                    go(p, s, rest, j, memo, w) || (c.is_some() && go(p, s, i, j + 1, memo, w))
                }
                (Some(b'*'), c) => go(p, s, i + 1, j, memo, w) || (c.is_some_and(|c| *c != b'/') && go(p, s, i, j + 1, memo, w)),
                (Some(b'?'), Some(c)) if *c != b'/' => go(p, s, i + 1, j + 1, memo, w),
                (Some(a), Some(b)) if a == b => go(p, s, i + 1, j + 1, memo, w),
                _ => false,
            };
            memo[k] = if r { 1 } else { 2 };
            r
        }
        if p.len() > 256 || s.len() > 4096 {
            return false;
        }
        let mut memo = vec![0u8; (p.len() + 1) * (s.len() + 1)];
        go(p, s, 0, 0, &mut memo, s.len() + 1)
    }
    let p = pattern.trim_start_matches('/');
    if p.ends_with('/') {
        return path.starts_with(p) || m(format!("{p}**").as_bytes(), path.as_bytes());
    }
    m(p.as_bytes(), path.as_bytes()) || (!p.contains('/') && m(p.as_bytes(), path.rsplit('/').next().unwrap_or(path).as_bytes()))
}

pub fn any_glob(patterns: &[String], paths: &[String]) -> bool {
    patterns.is_empty() || paths.iter().any(|f| patterns.iter().any(|p| glob(p, f)))
}

// ---------------------------------------------------------------- cron

/// A five-field cron line (minute hour day-of-month month day-of-week), UTC.
#[derive(Debug, Clone, PartialEq)]
pub struct Cron([Vec<bool>; 5]);

impl Cron {
    pub fn parse(s: &str) -> Result<Cron, String> {
        let f: Vec<&str> = s.split_whitespace().collect();
        if f.len() != 5 {
            return Err(format!("'{s}' needs five fields: minute hour day month weekday"));
        }
        let ranges = [(0, 59), (0, 23), (1, 31), (1, 12), (0, 7)];
        let mut out: [Vec<bool>; 5] = Default::default();
        for (i, (field, (lo, hi))) in f.iter().zip(ranges).enumerate() {
            let mut set = vec![false; hi + 1];
            for part in field.split(',') {
                let (range, step) = match part.split_once('/') {
                    Some((r, st)) => (r, st.parse::<usize>().map_err(|_| format!("bad step in '{s}'"))?),
                    None => (part, 1),
                };
                if step == 0 {
                    return Err(format!("bad step in '{s}'"));
                }
                let (a, b) = if range == "*" {
                    (lo, hi)
                } else if let Some((a, b)) = range.split_once('-') {
                    (a.parse().map_err(|_| format!("bad range in '{s}'"))?, b.parse().map_err(|_| format!("bad range in '{s}'"))?)
                } else {
                    let n = range.parse().map_err(|_| format!("bad value '{range}' in '{s}'"))?;
                    (n, if part.contains('/') { hi } else { n })
                };
                if a < lo || b > hi || a > b {
                    return Err(format!("'{part}' is out of range in '{s}'"));
                }
                for v in (a..=b).step_by(step) {
                    set[v] = true;
                }
            }
            if i == 4 && set[7] {
                set[0] = true; // 7 is Sunday too
            }
            out[i] = set;
        }
        Ok(Cron(out))
    }

    /// Whether the minute containing unix time `t` matches.
    pub fn matches(&self, t: i64) -> bool {
        let (_, mo, d, h, mi, wd) = civil(t);
        let [min, hour, dom, mon, dow] = &self.0;
        let dom_all = dom.iter().skip(1).all(|x| *x);
        let dow_all = dow.iter().all(|x| *x);
        // Like cron: when both day fields are restricted, either may match.
        let day = match (dom_all, dow_all) {
            (true, true) => true,
            (true, false) => dow[wd],
            (false, true) => dom[d],
            (false, false) => dom[d] || dow[wd],
        };
        min[mi] && hour[h] && mon[mo] && day
    }
}

/// (year, month 1-12, day 1-31, hour, minute, weekday 0=Sunday) of unix time `t`, UTC.
fn civil(t: i64) -> (i64, usize, usize, usize, usize, usize) {
    let days = t.div_euclid(86400);
    let secs = t.rem_euclid(86400);
    // Howard Hinnant's days-from-civil, inverted.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    let wd = (days + 4).rem_euclid(7); // 1970-01-01 was a Thursday
    (y, m as usize, d as usize, (secs / 3600) as usize, ((secs % 3600) / 60) as usize, wd as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn yaml_subset() {
        let src = r#"
# a comment
on:
  push:
    branches: [main, "release/*"]
  pull_request:
  schedule: "0 3 * * 1-5"
env:
  GREETING: 'it''s here'   # trailing comment
jobs:
  build:
    image: rust:1
    steps:
      - run: cargo build
      - name: Test everything
        run: |
          cargo test
            --all # indented on purpose
      - echo "done: really"
  lint:
    needs: build
    steps:
    - run: >-
        cargo clippy
        -- -D warnings
"#;
        let v = parse_yaml(src).unwrap();
        assert_eq!(v["on"]["push"]["branches"], json!(["main", "release/*"]));
        assert_eq!(v["on"]["pull_request"], Value::Null);
        assert_eq!(v["env"]["GREETING"], "it's here");
        assert_eq!(v["jobs"]["build"]["steps"][1]["run"], "cargo test\n  --all # indented on purpose\n");
        assert_eq!(v["jobs"]["build"]["steps"][2], "echo \"done: really\"");
        assert_eq!(v["jobs"]["lint"]["steps"][0]["run"], "cargo clippy -- -D warnings");
        let order: Vec<&String> = v["jobs"].as_object().unwrap().keys().collect();
        assert_eq!(order, ["build", "lint"], "jobs keep file order");
        assert!(parse_yaml("a:\n\tb: 1").is_err());
        assert!(parse_yaml("a: &x 1").is_err());
        assert!(parse_yaml("a: 1\na: 2").unwrap_err().contains("twice"));
        assert!(parse_yaml("a:\n  - x\n   y: 1").is_err());
    }

    #[test]
    fn pipelines_expand_and_validate() {
        let p = parse(
            "on: [push]\njobs:\n  test:\n    matrix: { rust: [stable, beta], os: [linux] }\n    image: rust:${{ matrix.rust }}\n    steps: [cargo test]\n  deploy:\n    needs: [test]\n    timeout: 1h\n    steps:\n      - run: ./deploy.sh\n",
        )
        .unwrap();
        assert_eq!(p.triggers.push, Some(Filter::default()));
        assert!(p.triggers.pull_request.is_none() && !p.triggers.manual);
        let names: Vec<&str> = p.jobs.iter().map(|j| j.name.as_str()).collect();
        assert_eq!(names, ["test (stable, linux)", "test (beta, linux)", "deploy"]);
        assert_eq!(p.jobs[1].image.as_deref(), Some("rust:beta"));
        assert_eq!(p.jobs[1].env["MATRIX_RUST"], "beta");
        assert_eq!(p.jobs[2].timeout_secs, 3600);
        assert!(parse("jobs:\n  a:\n    needs: [b]\n    steps: [x]\n  b:\n    needs: [a]\n    steps: [y]\n").unwrap_err().contains("loop"));
        assert!(parse("jobs:\n  a:\n    needs: [nope]\n    steps: [x]\n").unwrap_err().contains("no other job"));
        assert!(parse("jobs:\n  a:\n    stepz: [x]\n").unwrap_err().contains("unknown setting"));
        assert!(parse("on: deploy\njobs:\n  a:\n    steps: [x]\n").is_err());
        assert!(parse("jobs:\n  a:\n    image: --privileged\n    steps: [x]\n").unwrap_err().contains("image name"));
        assert!(parse("jobs:\n  a:\n    timeout: 99999999999999999999h\n    steps: [x]\n").is_err());
        // No `on:` means push, pull requests and manual runs.
        let d = parse("jobs:\n  a:\n    steps: [x]\n").unwrap();
        assert!(d.triggers.push.is_some() && d.triggers.pull_request.is_some() && d.triggers.manual);
        assert_eq!(script_pipeline().jobs[0].steps[0].run, "sh .gitter/ci.sh");
    }

    #[test]
    fn the_readme_example_parses() {
        let readme = include_str!("../../README.md");
        let at = readme.find("## CI").unwrap();
        let start = readme[at..].find("```yaml\n").unwrap() + at + 8;
        let end = readme[start..].find("```").unwrap() + start;
        let p = parse(&readme[start..end]).unwrap();
        assert!(p.triggers.manual && p.triggers.pull_request.is_some());
        assert_eq!(p.triggers.push.as_ref().unwrap().branches, ["main", "release/*"]);
        assert_eq!(p.triggers.schedule, ["0 3 * * *"]);
        let names: Vec<&str> = p.jobs.iter().map(|j| j.name.as_str()).collect();
        assert_eq!(names, ["test (stable)", "test (beta)", "docs", "deploy"]);
        assert_eq!(p.jobs[1].env["RUSTUP_TOOLCHAIN"], "beta");
        assert_eq!((p.jobs[2].paths.len(), p.jobs[2].artifacts.len(), p.jobs[3].timeout_secs), (1, 1, 600));
    }

    #[test]
    fn hostile_files_and_patterns_are_cheap() {
        let deep = format!("x: {}{}", "[".repeat(5000), "]".repeat(5000));
        assert!(parse_yaml(&deep).unwrap_err().contains("nested"));
        let mut indented = String::new();
        for i in 0..200 {
            indented.push_str(&format!("{}k{i}:\n", " ".repeat(i)));
        }
        assert!(parse_yaml(&indented).unwrap_err().contains("levels deep"));
        let t = std::time::Instant::now();
        let long = "a".repeat(3000);
        assert!(!glob("*a*a*a*a*a*a*a*a*a*a*b", &long));
        assert!(!glob("**a**a**a**a**a**b", &long));
        assert!(t.elapsed() < std::time::Duration::from_secs(1), "took {:?}", t.elapsed());
    }

    #[test]
    fn globs() {
        assert!(glob("src/**", "src/a/b.rs"));
        assert!(glob("src/", "src/a/b.rs"));
        assert!(glob("*.md", "docs/guide.md"), "no slash: matches the file name anywhere");
        assert!(!glob("src/*.rs", "src/a/b.rs"));
        assert!(glob("src/*.rs", "src/b.rs"));
        assert!(glob("**/test_*.py", "a/b/test_x.py"));
        assert!(glob("release/*", "release/1.2"));
        assert!(!glob("main", "maintenance"));
        assert!(any_glob(&[], &["x".into()]));
        assert!(!any_glob(&["docs/**".into()], &["src/a".into()]));
    }

    #[test]
    fn cron() {
        // 2026-10-21 03:00 UTC is a Wednesday.
        let t = 1792551600;
        assert_eq!(civil(t), (2026, 10, 21, 3, 0, 3));
        assert!(Cron::parse("0 3 * * *").unwrap().matches(t));
        assert!(Cron::parse("*/15 * * * 1-5").unwrap().matches(t));
        assert!(!Cron::parse("0 3 * * 0,6").unwrap().matches(t));
        assert!(Cron::parse("0 3 21 * 0").unwrap().matches(t), "either day field may match");
        assert!(!Cron::parse("1 3 * * *").unwrap().matches(t));
        assert!(Cron::parse("0 3 * * 7").unwrap().matches(t + 4 * 86400), "7 is Sunday");
        for bad in ["* * * *", "60 * * * *", "*/0 * * * *", "a * * * *"] {
            assert!(Cron::parse(bad).is_err(), "{bad}");
        }
    }
}
