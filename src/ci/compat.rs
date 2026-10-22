//! CI from other hosts' files: GitHub Actions workflows (`.github/workflows/*.yml`)
//! and `.gitlab-ci.yml`, translated into the pipeline `.gitter/ci.yml` describes.
//!
//! Each file becomes the same JSON value a `.gitter/ci.yml` parses into, so it goes
//! through the same checks and limits. What can't run here (most `uses:` actions,
//! services, expressions about the triggering event) is left out, and each job's log
//! starts with a note saying what.

use serde_json::{Map, Value, json};

use super::config::{self, Filter, Pipeline, Triggers};

pub const GITHUB_DIR: &str = ".github/workflows";
pub const GITLAB_FILE: &str = ".gitlab-ci.yml";
/// Workflow files read at most (each is also capped like `.gitter/ci.yml`).
pub const MAX_WORKFLOWS: usize = 20;

/// What every translated step does first: the variables scripts written for the
/// other host expect, and what earlier steps wrote to `$GITHUB_ENV` and `$GITHUB_PATH`.
const PRELUDE: &str = r#"export CI=true GITHUB_ACTIONS=true GITHUB_SHA="$GITTER_SHA" GITHUB_REF="$GITTER_REF" GITHUB_REF_NAME="$GITTER_BRANCH" GITHUB_REPOSITORY="$GITTER_REPOSITORY" GITHUB_WORKSPACE="$GITTER_WORKSPACE" GITHUB_RUN_NUMBER="$GITTER_RUN_NUMBER" RUNNER_OS=Linux RUNNER_ARCH=X64
case "$GITTER_EVENT" in manual) GITHUB_EVENT_NAME=workflow_dispatch ;; merge_queue) GITHUB_EVENT_NAME=merge_group ;; *) GITHUB_EVENT_NAME="$GITTER_EVENT" ;; esac
export GITHUB_EVENT_NAME
export GITHUB_ENV="$RUNNER_TEMP/env" GITHUB_PATH="$RUNNER_TEMP/path" GITHUB_OUTPUT="$RUNNER_TEMP/output" GITHUB_STEP_SUMMARY="$RUNNER_TEMP/summary"
touch "$GITHUB_ENV" "$GITHUB_PATH" "$GITHUB_OUTPUT" "$GITHUB_STEP_SUMMARY"
while IFS= read -r l; do case "$l" in [A-Za-z_]*=*) export "${l%%=*}=${l#*=}" ;; esac; done < "$GITHUB_ENV"
while IFS= read -r p; do [ -n "$p" ] && PATH="$p:$PATH"; done < "$GITHUB_PATH"; export PATH
"#;

/// GitLab's predefined variables that scripts use most, from Gitter's.
const GITLAB_PRELUDE: &str = r#"export CI=true GITLAB_CI=true CI_COMMIT_SHA="$GITTER_SHA" CI_COMMIT_SHORT_SHA="$(printf %.8s "$GITTER_SHA")" CI_COMMIT_REF_NAME="$GITTER_BRANCH" CI_COMMIT_BRANCH="$GITTER_BRANCH" CI_PROJECT_DIR="$GITTER_WORKSPACE" CI_PROJECT_PATH="$GITTER_REPOSITORY" CI_PROJECT_NAME="${GITTER_REPOSITORY#*/}" CI_PIPELINE_IID="$GITTER_RUN_NUMBER" CI_JOB_NAME="$GITTER_JOB"
case "$GITTER_EVENT" in pull_request|merge_queue) CI_PIPELINE_SOURCE=merge_request_event ;; manual) CI_PIPELINE_SOURCE=web ;; *) CI_PIPELINE_SOURCE="$GITTER_EVENT" ;; esac; export CI_PIPELINE_SOURCE
"#;

/// A step: run `script` with `shell` ("bash" as GitHub runs it, "gitlab" as GitLab
/// does, "sh", "python", "pwsh"), after the prelude and `exports`, in `dir`, only
/// if `cond` (a shell test) holds.
#[allow(clippy::too_many_arguments)]
fn step(name: &str, prelude: &str, exports: &[String], dir: Option<&str>, cond: Option<(&str, &str)>, shell: &str, script: &str, keep_going: bool) -> Value {
    // Kept inside the checkout's .git, so it lasts from step to step (also in a job's
    // container, where each step is a fresh one) without showing up as a file.
    let mut s = String::from(
        "if [ -d \"$GITTER_WORKSPACE/.git\" ]; then RUNNER_TEMP=\"$GITTER_WORKSPACE/.git/runner-temp\"; else RUNNER_TEMP=\"${HOME:-/tmp}/.runner-temp\"; fi\nexport RUNNER_TEMP\nmkdir -p \"$RUNNER_TEMP\"\n",
    );
    s.push_str(prelude);
    for e in exports {
        s.push_str(e);
        s.push('\n');
    }
    if let Some((test, said)) = cond {
        s.push_str(&format!("if ! {{ {test}; }}; then echo {}; exit 0; fi\n", sh_quote(&format!("Skipped (if: {said})"))));
    }
    if let Some(d) = dir {
        s.push_str(&format!("cd {}\n", sh_quote(d)));
    }
    // bash isn't in every image: plain sh then.
    let pick = |bash: &str| format!("if command -v bash >/dev/null 2>&1; then run='{bash}'; else run='sh -e'; fi\n");
    s.push_str(&match shell {
        "sh" => "run='sh -e'\n".to_string(),
        "python" => "run='python3'\n".to_string(),
        "pwsh" => "run='pwsh -NoProfile -File'\n".to_string(),
        "gitlab" => pick("bash --noprofile --norc -e"),
        _ => pick("bash --noprofile --norc -eo pipefail"),
    });
    let body = if script.ends_with('\n') { script.to_string() } else { format!("{script}\n") };
    s.push_str(&format!("f=\"$RUNNER_TEMP/step-$$\"\ncat > \"$f\" <<'GITTER_STEP_EOF'\n{body}GITTER_STEP_EOF\n"));
    if keep_going {
        s.push_str("$run \"$f\" || echo 'This step failed; the job goes on.'\n");
    } else {
        s.push_str("exec $run \"$f\"\n");
    }
    json!({ "name": name, "run": s })
}

fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// A first step that prints what the translation left out (none if nothing was).
fn notes_step(file: &str, notes: &[String]) -> Option<Value> {
    if notes.is_empty() {
        return None;
    }
    let file: String = file.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let mut text = format!("Translated from {file}. Not run here:\n");
    for n in notes {
        text.push_str(&format!("  - {n}\n"));
    }
    text.push_str("Add a .gitter/ci.yml to say exactly what runs.");
    Some(json!({ "name": format!("Notes on {file}"), "run": format!("cat <<'GITTER_NOTES_EOF'\n{text}\nGITTER_NOTES_EOF\n") }))
}

fn push_note(notes: &mut Vec<String>, n: String) {
    // One line each, so nothing in a note can end the text it's printed in.
    let n: String = n.chars().map(|c| if c.is_control() { ' ' } else { c }).take(300).collect();
    if !notes.contains(&n) && notes.len() < 30 {
        notes.push(n);
    }
}

/// Text or lines of text ("a" or ["a", "b"] or a block with newlines) → lines.
fn lines(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::String(s)) => s.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect(),
        Some(Value::Array(a)) => a.iter().filter_map(|x| match x {
            Value::String(s) => Some(s.clone()),
            Value::Number(n) => Some(n.to_string()),
            _ => None,
        }).collect(),
        _ => vec![],
    }
}

fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Keep cache and artifact paths that are inside the workspace.
fn workspace_paths(paths: Vec<String>, what: &str, notes: &mut Vec<String>) -> Vec<String> {
    paths
        .into_iter()
        .filter(|p| {
            let inside = !(p.starts_with('/') || p.starts_with('~') || p.starts_with('$') || p.starts_with('!') || p.split('/').any(|s| s == ".."));
            if !inside {
                push_note(notes, format!("{what} path {p} (only paths inside the workspace are kept)"));
            }
            inside
        })
        .map(|p| p.trim_start_matches("./").to_string())
        .collect()
}

// ---------------------------------------------------------------- GitHub expressions

/// `${{ … }}` in text: matrix values stay for the pipeline to fill in; contexts
/// become shell variables; the rest becomes empty, with a note.
fn gh_text(s: &str, notes: &mut Vec<String>) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find("${{") {
        out.push_str(&rest[..i]);
        let Some(j) = rest[i..].find("}}") else {
            out.push_str(&rest[i..]);
            return out;
        };
        let expr = rest[i + 3..i + j].trim();
        out.push_str(&gh_value(expr, notes));
        rest = &rest[i + j + 2..];
    }
    out.push_str(rest);
    out
}

/// A matrix key the pipeline can fill in (anything else stays out of scripts).
fn matrix_key(k: &str) -> Option<&str> {
    (!k.is_empty() && k.len() <= 60 && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')).then_some(k)
}

/// One expression as shell text.
fn gh_value(expr: &str, notes: &mut Vec<String>) -> String {
    if let Some(k) = expr.strip_prefix("matrix.").and_then(matrix_key) {
        return format!("${{{{ matrix.{k} }}}}");
    }
    if let Some(lit) = quoted(expr) {
        return lit;
    }
    match gh_var(expr) {
        Some(v) => v,
        None => {
            if expr == "secrets.GITHUB_TOKEN" || expr == "github.token" {
                push_note(notes, "GITHUB_TOKEN (there's no GitHub token here; add a secret if a step needs one)".into());
            } else {
                push_note(notes, format!("the expression ${{{{ {expr} }}}} (left empty)"));
            }
            String::new()
        }
    }
}

/// A context the shell has: `${NAME}`.
fn gh_var(expr: &str) -> Option<String> {
    let name = |n: &str| Some(format!("${{{n}}}"));
    let valid = |n: &str| !n.is_empty() && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !n.starts_with(|c: char| c.is_ascii_digit());
    if let Some(n) = expr.strip_prefix("secrets.").or_else(|| expr.strip_prefix("env.")).or_else(|| expr.strip_prefix("vars."))
        && valid(n)
        && n != "GITHUB_TOKEN"
    {
        return name(n);
    }
    match expr {
        "github.sha" => name("GITHUB_SHA"),
        "github.ref" => name("GITHUB_REF"),
        "github.ref_name" | "github.head_ref" => name("GITHUB_REF_NAME"),
        "github.repository" => name("GITHUB_REPOSITORY"),
        "github.workspace" => name("GITHUB_WORKSPACE"),
        "github.run_number" | "github.run_id" => name("GITHUB_RUN_NUMBER"),
        "github.event_name" => name("GITHUB_EVENT_NAME"),
        "github.repository_owner" => Some("${GITHUB_REPOSITORY%%/*}".into()),
        "runner.os" => Some("Linux".into()),
        "runner.arch" => Some("X64".into()),
        "runner.temp" => name("RUNNER_TEMP"),
        _ => None,
    }
}

fn quoted(s: &str) -> Option<String> {
    let s = s.trim();
    (s.len() >= 2 && s.starts_with('\'') && s.ends_with('\'')).then(|| s[1..s.len() - 1].replace("''", "'"))
}

/// An `if:` as a shell test, or None if it can't be decided here.
fn gh_cond(expr: &str) -> Option<String> {
    // Bounded: the text is the file's, and each level recurses.
    if expr.len() > 1000 {
        return None;
    }
    cond_at(expr, 0)
}

fn cond_at(expr: &str, depth: usize) -> Option<String> {
    if depth > 16 {
        return None;
    }
    let e = expr.trim();
    let e = e.strip_prefix("${{").and_then(|x| x.strip_suffix("}}")).unwrap_or(e).trim();
    if let Some(inner) = e.strip_prefix('(').and_then(|x| x.strip_suffix(')'))
        && balanced(inner)
    {
        return cond_at(inner, depth + 1);
    }
    if let Some(parts) = split_top(e, "||") {
        return Some(parts.iter().map(|p| cond_at(p, depth + 1).map(|c| format!("{{ {c}; }}"))).collect::<Option<Vec<_>>>()?.join(" || "));
    }
    if let Some(parts) = split_top(e, "&&") {
        return Some(parts.iter().map(|p| cond_at(p, depth + 1).map(|c| format!("{{ {c}; }}"))).collect::<Option<Vec<_>>>()?.join(" && "));
    }
    if let Some(inner) = e.strip_prefix('!') {
        return cond_at(inner, depth + 1).map(|c| format!("! {{ {c}; }}"));
    }
    match e {
        "success()" | "always()" | "!cancelled()" | "true" => return Some("true".into()),
        "failure()" | "cancelled()" | "false" => return Some("false".into()),
        _ => {}
    }
    for (op, neg) in [("==", false), ("!=", true)] {
        if let Some((a, b)) = e.split_once(op) {
            let (a, b) = (operand(a)?, operand(b)?);
            let t = format!("[ \"{a}\" = \"{b}\" ]");
            return Some(if neg { format!("! {t}") } else { t });
        }
    }
    for (f, pat) in [("startsWith(", "{b}*"), ("endsWith(", "*{b}"), ("contains(", "*{b}*")] {
        if let Some(args) = e.strip_prefix(f).and_then(|x| x.strip_suffix(')'))
            && let Some((a, b)) = args.split_once(',')
        {
            let (a, b) = (operand(a)?, quoted(b)?);
            // The text becomes a case pattern: plain characters only.
            if b.is_empty() || !b.chars().all(|c| c.is_ascii_alphanumeric() || "._/@:-".contains(c)) {
                return None;
            }
            let glob = pat.replace("{b}", &b);
            return Some(format!("case \"{a}\" in {glob}) true ;; *) false ;; esac"));
        }
    }
    None
}

fn balanced(s: &str) -> bool {
    let mut d = 0i32;
    for c in s.chars() {
        d += match c {
            '(' => 1,
            ')' => -1,
            _ => 0,
        };
        if d < 0 {
            return false;
        }
    }
    d == 0
}

/// Split on `op` outside parentheses and quotes; None if it isn't there.
fn split_top<'a>(s: &'a str, op: &str) -> Option<Vec<&'a str>> {
    let (mut parts, mut depth, mut q, mut start, mut skip) = (Vec::new(), 0i32, false, 0, 0);
    for (i, c) in s.char_indices() {
        if i < skip {
            continue;
        }
        match c {
            '\'' => q = !q,
            '(' if !q => depth += 1,
            ')' if !q => depth -= 1,
            _ if !q && depth == 0 && s[i..].starts_with(op) => {
                parts.push(s[start..i].trim());
                start = i + op.len();
                skip = start;
            }
            _ => {}
        }
    }
    if parts.is_empty() {
        return None;
    }
    parts.push(s[start..].trim());
    Some(parts)
}

/// A side of a comparison, as shell text: a quoted literal or a known context.
fn operand(s: &str) -> Option<String> {
    let s = s.trim();
    if let Some(l) = quoted(s) {
        return Some(l.replace(['"', '\\', '$', '`'], ""));
    }
    if matches!(s, "true" | "false") || s.parse::<f64>().is_ok() {
        return Some(s.to_string());
    }
    if let Some(k) = s.strip_prefix("matrix.") {
        return matrix_key(k).map(|k| format!("${{{{ matrix.{k} }}}}"));
    }
    gh_var(s)
}

// ---------------------------------------------------------------- GitHub workflows

/// GitHub workflows (path, text) → one pipeline. Each workflow's jobs keep its
/// triggers (`Job::when`), so an event runs only the workflows that want it.
///
/// Ok(None): none of them runs here (tags, releases and reusable workflows only).
/// A workflow that can't be read doesn't stop the others; their jobs say so.
pub fn github(files: &[(String, String)]) -> Result<Option<Pipeline>, String> {
    let mut all = Pipeline { triggers: Triggers::default(), jobs: vec![] };
    let several = files.len() > 1;
    let mut broken = Vec::new();
    for (path, src) in files {
        let read = config::parse_yaml(src).and_then(|v| Ok((gh_workflow(&v, path)?, v)));
        let (cfg, v) = match read {
            Ok((Some(cfg), v)) => (cfg, v),
            Ok((None, _)) => continue,
            Err(e) => {
                broken.push(format!("{path}: {e}"));
                continue;
            }
        };
        let mut p = match config::parse_value(cfg) {
            Ok(p) => p,
            Err(e) => {
                broken.push(format!("{path}: {e}"));
                continue;
            }
        };
        let stem = path.rsplit('/').next().unwrap_or(path).trim_end_matches(".yml").trim_end_matches(".yaml").to_string();
        let title = v.get("name").and_then(Value::as_str).map(String::from).unwrap_or_else(|| stem.clone());
        for j in &mut p.jobs {
            j.when = Some(p.triggers.clone());
            if several {
                j.key = format!("{stem}.{}", j.key);
                j.needs = j.needs.iter().map(|n| format!("{stem}.{n}")).collect();
                j.name = format!("{title} / {}", j.name);
            }
        }
        merge_triggers(&mut all.triggers, &p.triggers);
        all.jobs.extend(p.jobs);
    }
    if all.jobs.is_empty() {
        return if broken.is_empty() { Ok(None) } else { Err(broken.join("; ")) };
    }
    if all.jobs.len() > 100 {
        return Err(format!("{GITHUB_DIR}: at most 100 jobs in all"));
    }
    if !broken.is_empty() {
        let text: Vec<String> = broken.iter().map(|b| format!("  - {}", b.chars().map(|c| if c.is_control() { ' ' } else { c }).collect::<String>())).collect();
        let run = format!("cat <<'GITTER_NOTES_EOF'\nThese workflows couldn't be read, so they don't run:\n{}\nGITTER_NOTES_EOF\n", text.join("\n"));
        for j in &mut all.jobs {
            j.steps.insert(0, config::Step { name: "Workflows that couldn't be read".into(), run: run.clone() });
        }
    }
    Ok(Some(all))
}

/// The pipeline's own triggers: anything any workflow wants (each job's own decide).
fn merge_triggers(all: &mut Triggers, t: &Triggers) {
    if t.push.is_some() {
        all.push = Some(Filter::default());
    }
    if t.pull_request.is_some() {
        all.pull_request = Some(Filter::default());
    }
    all.manual |= t.manual;
    for c in &t.schedule {
        if !all.schedule.contains(c) {
            all.schedule.push(c.clone());
        }
    }
}

/// One workflow → the value a `.gitter/ci.yml` parses into. None: it never runs here.
fn gh_workflow(v: &Value, path: &str) -> Result<Option<Value>, String> {
    let Value::Object(w) = v else { return Err(format!("{path}: expected a workflow (on: and jobs:)")) };
    let mut wnotes = Vec::new();
    let on = gh_on(w.get("on").or_else(|| w.get("true")), &mut wnotes);
    if on.as_object().is_none_or(|m| m.is_empty()) {
        return Ok(None);
    }
    let Some(Value::Object(jobs)) = w.get("jobs") else { return Err(format!("{path}: no jobs")) };
    let wenv = w.get("env").and_then(Value::as_object).cloned().unwrap_or_default();
    let wdefaults = w.get("defaults").and_then(|d| d.get("run")).and_then(Value::as_object).cloned().unwrap_or_default();
    let mut out = Map::new();
    for (key, job) in jobs {
        let Value::Object(job) = job else { return Err(format!("{path}: jobs.{key}: expected a job")) };
        let mut notes = wnotes.clone();
        out.insert(key.clone(), gh_job(key, job, &wenv, &wdefaults, path, &mut notes));
    }
    Ok(Some(json!({ "on": on, "jobs": out })))
}

/// GitHub's `on:` → Gitter's (push, pull_request, manual, schedule).
fn gh_on(v: Option<&Value>, notes: &mut Vec<String>) -> Value {
    let mut on = Map::new();
    let events: Vec<(String, Value)> = match v {
        Some(Value::String(s)) => vec![(s.clone(), Value::Null)],
        Some(Value::Array(a)) => a.iter().filter_map(|x| x.as_str().map(|s| (s.to_string(), Value::Null))).collect(),
        Some(Value::Object(m)) => m.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        _ => vec![],
    };
    for (event, spec) in events {
        match event.as_str() {
            "push" | "pull_request" | "pull_request_target" | "merge_group" => {
                let to = if event == "push" { "push" } else { "pull_request" };
                let m = spec.as_object().cloned().unwrap_or_default();
                if m.contains_key("types") {
                    push_note(notes, format!("{event}: types (it runs when the pull request is opened or pushed to)"));
                }
                let mut f = Map::new();
                let branches: Vec<String> = lines(m.get("branches"));
                if branches.iter().any(|b| b.starts_with('!')) || m.contains_key("branches-ignore") || m.contains_key("paths-ignore") || lines(m.get("paths")).iter().any(|p| p.starts_with('!')) {
                    push_note(notes, format!("{event}: branch or path exclusions (it runs for all of them here)"));
                }
                let keep: Vec<String> = branches.into_iter().filter(|b| !b.starts_with('!')).collect();
                if !keep.is_empty() && !m.contains_key("branches-ignore") {
                    f.insert("branches".into(), json!(keep));
                }
                let paths: Vec<String> = lines(m.get("paths"));
                if !paths.is_empty() && !paths.iter().any(|p| p.starts_with('!')) && !m.contains_key("paths-ignore") {
                    f.insert("paths".into(), json!(paths));
                }
                if event == "push" && m.contains_key("tags") && !m.contains_key("branches") {
                    push_note(notes, "push: tags (tag pushes don't start runs here)".into());
                    continue;
                }
                on.insert(to.into(), Value::Object(f));
            }
            "workflow_dispatch" => {
                on.insert("manual".into(), Value::Bool(true));
            }
            "schedule" => {
                let crons: Vec<Value> = spec.as_array().map(|a| a.iter().filter_map(|c| c.get("cron").cloned()).collect()).unwrap_or_default();
                if !crons.is_empty() {
                    on.insert("schedule".into(), Value::Array(crons));
                }
            }
            "workflow_call" => {}
            other => push_note(notes, format!("on: {other}")),
        }
    }
    Value::Object(on)
}

fn gh_job(key: &str, job: &Map<String, Value>, wenv: &Map<String, Value>, wdefaults: &Map<String, Value>, path: &str, notes: &mut Vec<String>) -> Value {
    let mut out = Map::new();
    if let Some(n) = job.get("name").and_then(Value::as_str) {
        out.insert("name".into(), json!(gh_text(n, notes)));
    }
    let needs = lines(job.get("needs"));
    if !needs.is_empty() {
        out.insert("needs".into(), json!(needs));
    }
    // A reusable workflow can't be called here: say so in a job that fails.
    if let Some(u) = job.get("uses").and_then(Value::as_str) {
        return failing(out, key, &format!("it calls the reusable workflow {u}"));
    }
    if let Some(r) = job.get("runs-on") {
        // Through the matrix too: runs-on: ${{ matrix.os }}.
        let mut names = lines(Some(r));
        let axes = job.get("strategy").and_then(|s| s.get("matrix")).and_then(Value::as_object);
        for n in names.clone() {
            if let Some(k) = n.split("matrix.").nth(1).map(|k| k.trim_end_matches('}').trim())
                && let Some(vals) = axes.and_then(|a| a.get(k))
            {
                names.extend(lines(Some(vals)));
            }
        }
        for os in names.iter().filter(|n| ["windows", "macos"].iter().any(|o| n.contains(o))) {
            push_note(notes, format!("runs-on {os} (jobs run on Linux here)"));
        }
        // Labels that pick particular machines (self-hosted ones) are kept: such a
        // job waits for a runner that has them rather than running anywhere.
        let generic = |l: &str| l.starts_with("ubuntu") || l.starts_with("windows") || l.starts_with("macos") || matches!(l.to_ascii_lowercase().as_str(), "self-hosted" | "linux" | "x64");
        let labels: Vec<String> = lines(Some(r)).into_iter().filter(|l| !l.contains("${{") && !generic(l)).collect();
        if !labels.is_empty() {
            push_note(notes, format!("runs-on {} (it waits for a runner with those labels: gitter runner --labels)", labels.join(", ")));
            out.insert("runs-on".into(), json!(labels));
        }
    }
    let img = match job.get("container") {
        Some(Value::String(s)) => Some(gh_text(s, notes)),
        Some(Value::Object(c)) => {
            if c.keys().any(|k| k != "image") {
                push_note(notes, "container options, env, ports and volumes".into());
            }
            c.get("image").and_then(Value::as_str).map(|i| gh_text(i, notes))
        }
        _ => None,
    };
    if let Some(i) = img {
        match image(&i) {
            Some(i) => {
                out.insert("image".into(), json!(i));
            }
            // Not on the runner's own machine instead of the container it asked for.
            None => return failing(out, key, &format!("its container image {i} can't be used here (it uses variables)")),
        }
    }
    if job.contains_key("services") {
        push_note(notes, "services (start them in a step, or use an image that has them)".into());
    }
    if let Some(t) = job.get("timeout-minutes").and_then(Value::as_u64) {
        out.insert("timeout".into(), json!(t.clamp(1, 360)));
    }
    if let Some(e) = environment(job.get("environment"), notes) {
        out.insert("environment".into(), e);
    }
    if let Some(Value::Object(m)) = job.get("strategy").and_then(|s| s.get("matrix")) {
        let mut axes = Map::new();
        for (k, v) in m {
            match (k.as_str(), v) {
                ("include" | "exclude", _) => push_note(notes, format!("strategy.matrix.{k}")),
                (_, Value::Array(a)) if a.iter().all(|x| !x.is_object() && !x.is_array()) => {
                    axes.insert(k.clone(), v.clone());
                }
                _ => push_note(notes, format!("strategy.matrix.{k} (only lists of plain values)")),
            }
        }
        if !axes.is_empty() {
            out.insert("matrix".into(), Value::Object(axes));
        }
    }
    // Plain env values are the job's env; ones with expressions are exported by each step.
    let mut env = Map::new();
    let mut exports = Vec::new();
    for (k, v) in wenv.iter().chain(job.get("env").and_then(Value::as_object).into_iter().flatten()) {
        env_entry(k, &text(v), &mut env, &mut exports, notes);
    }
    if !env.is_empty() {
        out.insert("env".into(), Value::Object(env));
    }
    let jdefaults = job.get("defaults").and_then(|d| d.get("run")).and_then(Value::as_object);
    let default = |k: &str| jdefaults.and_then(|d| d.get(k)).or_else(|| wdefaults.get(k)).and_then(Value::as_str).map(String::from);
    let job_cond = job.get("if").map(|c| condition(&text(c), "job", notes));
    let mut steps = Vec::new();
    let (mut cache, mut artifacts): (Option<Value>, Vec<String>) = (None, vec![]);
    for (i, s) in job.get("steps").and_then(Value::as_array).into_iter().flatten().enumerate() {
        let Value::Object(s) = s else { continue };
        let label = s.get("name").and_then(Value::as_str).map(|n| gh_text(n, notes));
        if let Some(u) = s.get("uses").and_then(Value::as_str) {
            gh_action(u, s, &mut cache, &mut artifacts, notes);
            continue;
        }
        let Some(run) = s.get("run") else { continue };
        let script = gh_text(&text(run), notes);
        let mut ex = exports.clone();
        let mut senv = Map::new();
        for (k, v) in s.get("env").and_then(Value::as_object).into_iter().flatten() {
            env_entry(k, &text(v), &mut senv, &mut ex, notes);
        }
        // A step's own plain env is exported by it too.
        for (k, v) in senv {
            ex.push(format!("export {k}={}", sh_quote(&text(&v))));
        }
        let cond = match (&job_cond, s.get("if").map(|c| condition(&text(c), "step", notes))) {
            (Some(j), Some(c)) => Some((format!("{{ {}; }} && {{ {}; }}", j.0, c.0), format!("{} and {}", j.1, c.1))),
            (j, c) => c.or_else(|| j.clone()),
        };
        let shell = s.get("shell").and_then(Value::as_str).map(String::from).or_else(|| default("shell")).unwrap_or_default();
        let shell = match shell.split_whitespace().next().unwrap_or("") {
            "" | "bash" => "bash",
            "sh" => "sh",
            "python" => "python",
            "pwsh" | "powershell" => "pwsh",
            other => {
                push_note(notes, format!("shell: {other} (bash runs it)"));
                "bash"
            }
        };
        let dir = s.get("working-directory").and_then(Value::as_str).map(String::from).or_else(|| default("working-directory"));
        let keep_going = s.get("continue-on-error").is_some_and(|v| v == &Value::Bool(true));
        let name = label.unwrap_or_else(|| first_line(&script).unwrap_or_else(|| format!("Step {}", i + 1)));
        let condition = cond.as_ref().map(|(t, said)| (t.as_str(), said.as_str()));
        steps.push(step(&name, PRELUDE, &ex, dir.as_deref(), condition, shell, &script, keep_going));
    }
    if steps.is_empty() {
        steps.push(json!({ "name": "Nothing to run here", "run": "echo 'Every step of this job uses an action that does not run here.'" }));
    }
    if let Some(n) = notes_step(path, notes) {
        steps.insert(0, n);
    }
    out.insert("steps".into(), Value::Array(steps));
    if let Some(c) = cache {
        out.insert("cache".into(), c);
    }
    if !artifacts.is_empty() {
        out.insert("artifacts".into(), json!(artifacts));
    }
    Value::Object(out)
}

/// A GitHub or GitLab job's `environment:` (a name, or name and url), as Gitter's: names and
/// URLs built from expressions or variables are left out, with a note.
fn environment(v: Option<&Value>, notes: &mut Vec<String>) -> Option<Value> {
    let (name, url) = match v? {
        Value::String(s) => (s.clone(), String::new()),
        Value::Object(m) => {
            // GitLab's stop, prepare, verify and access jobs name an environment without deploying to it.
            if m.get("action").map(text).is_some_and(|a| a != "start") {
                return None;
            }
            (m.get("name").map(text).unwrap_or_default(), m.get("url").map(text).unwrap_or_default())
        }
        _ => return None,
    };
    let dynamic = |s: &str| s.contains('$');
    if dynamic(&name) || !crate::api::deploy::env_name(&name) {
        push_note(notes, format!("environment: {name} (only a plain name deploys here)"));
        return None;
    }
    let mut e = Map::new();
    e.insert("name".into(), json!(name));
    if dynamic(&url) || !(url.is_empty() || crate::validate::web_link(&url)) {
        push_note(notes, format!("environment url {url}"));
    } else if !url.is_empty() {
        e.insert("url".into(), json!(url));
    }
    Some(Value::Object(e))
}

/// An image name Gitter can use (matrix values are filled in later), or None.
fn image(i: &str) -> Option<String> {
    let bare = i.replace("${{ matrix.", "").replace(" }}", "");
    let ok = !i.is_empty() && !i.starts_with('-') && bare.chars().all(|c| c.is_ascii_alphanumeric() || "._-/:@".contains(c));
    ok.then(|| i.to_string())
}

/// A job that can't run here: one step that says why and fails.
fn failing(mut out: Map<String, Value>, key: &str, why: &str) -> Value {
    let msg = format!("{key} can't run here: {why}. Add a .gitter/ci.yml to run it another way.");
    let msg: String = msg.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    out.insert("steps".into(), json!([{ "name": "Can't run here", "run": format!("echo {}; exit 1", sh_quote(&msg)) }]));
    Value::Object(out)
}

/// An `if:` as (shell test, the text): one that can't be decided here is false,
/// so what it guards doesn't run.
fn condition(c: &str, what: &str, notes: &mut Vec<String>) -> (String, String) {
    if ["always()", "failure()", "cancelled()"].iter().any(|f| c.contains(f)) {
        push_note(notes, format!("if: {c} (steps here stop at the first failure, so nothing runs after one)"));
    }
    match gh_cond(c) {
        Some(t) => (t, c.to_string()),
        None => {
            push_note(notes, format!("the {what} condition if: {c} (the {what} is skipped)"));
            ("false".into(), c.to_string())
        }
    }
}

fn first_line(s: &str) -> Option<String> {
    let l = s.lines().find(|l| !l.trim().is_empty())?.trim();
    Some(if l.chars().count() > 60 { format!("{}…", l.chars().take(60).collect::<String>()) } else { l.to_string() })
}

/// One env entry: plain text into `env`, text with expressions as an export line.
fn env_entry(k: &str, v: &str, env: &mut Map<String, Value>, exports: &mut Vec<String>, notes: &mut Vec<String>) {
    let valid = !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !k.starts_with(|c: char| c.is_ascii_digit());
    if !valid {
        push_note(notes, format!("env {k} (not a variable name)"));
        return;
    }
    if !v.contains("${{") {
        env.insert(k.to_string(), json!(v));
        return;
    }
    // Literal parts stay literal; the expressions become variables.
    let mut s = String::from("\"");
    let mut rest = v;
    while let Some(i) = rest.find("${{") {
        s.push_str(&dq_escape(&rest[..i]));
        let Some(j) = rest[i..].find("}}") else {
            rest = &rest[i..];
            break;
        };
        let expr = rest[i + 3..i + j].trim();
        let val = gh_value(expr, notes);
        // A quoted literal is text; a context is a variable reference.
        s.push_str(&if quoted(expr).is_some() { dq_escape(&val) } else { val });
        rest = &rest[i + j + 2..];
    }
    s.push_str(&dq_escape(rest));
    s.push('"');
    exports.push(format!("export {k}={s}"));
}

fn dq_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"").replace('$', "\\$").replace('`', "\\`")
}

/// A `uses:` step: what Gitter has an equivalent of goes into the job; the rest is noted.
fn gh_action(uses: &str, s: &Map<String, Value>, cache: &mut Option<Value>, artifacts: &mut Vec<String>, notes: &mut Vec<String>) {
    let name = uses.split('@').next().unwrap_or(uses).to_ascii_lowercase();
    let with = s.get("with").and_then(Value::as_object).cloned().unwrap_or_default();
    match name.as_str() {
        "actions/checkout" => {
            if with.get("fetch-depth").is_some_and(|d| d != &json!(1)) || with.contains_key("submodules") || with.contains_key("lfs") {
                push_note(notes, "checkout options (the workspace is the commit itself, without history or submodules)".into());
            }
        }
        "actions/cache" | "actions/cache/restore" | "actions/cache/save" => {
            if cache.is_some() {
                push_note(notes, "a second cache (one per job here)".into());
                return;
            }
            let paths = workspace_paths(lines(with.get("path")), "cache", notes);
            let key = with.get("key").map(text).map(|k| gh_cache_key(&k, notes)).unwrap_or_default();
            if paths.is_empty() || key.is_empty() {
                return;
            }
            let restore: Vec<String> = lines(with.get("restore-keys")).iter().map(|k| gh_cache_key(k, notes)).filter(|k| !k.is_empty()).take(10).collect();
            let mut c = json!({ "key": key, "paths": paths.into_iter().take(20).collect::<Vec<_>>() });
            if !restore.is_empty() {
                c["restore-keys"] = json!(restore);
            }
            *cache = Some(c);
        }
        "actions/upload-artifact" => {
            artifacts.extend(workspace_paths(lines(with.get("path")), "artifact", notes));
        }
        "actions/download-artifact" => push_note(notes, "download-artifact (jobs don't share artifacts here)".into()),
        n if n.starts_with("actions/setup-") || n.contains("toolchain") || n.contains("setup-") || n.ends_with("/setup") => {
            push_note(notes, format!("{uses} (tool setup: the runner or the job's image needs the tool)"));
        }
        _ => push_note(notes, format!("{uses} (actions don't run here)")),
    }
}

/// A cache key with GitHub expressions → one with Gitter's templates. Empty: can't.
fn gh_cache_key(k: &str, notes: &mut Vec<String>) -> String {
    let mut out = String::new();
    let mut rest = k;
    while let Some(i) = rest.find("${{") {
        out.push_str(&rest[..i]);
        let Some(j) = rest[i..].find("}}") else { return String::new() };
        let e = rest[i + 3..i + j].trim();
        if let Some(args) = e.strip_prefix("hashFiles(").and_then(|x| x.strip_suffix(')')) {
            let globs: Vec<String> = args.split(',').filter_map(quoted).filter(|g| !g.starts_with('!')).collect();
            if globs.is_empty() {
                return String::new();
            }
            out.push_str(&format!("{{{{ hash {} }}}}", globs.join(" ")));
        } else if e == "runner.os" {
            out.push_str("{{ os }}");
        } else if e == "runner.arch" {
            out.push_str("{{ arch }}");
        } else if let Some(m) = e.strip_prefix("matrix.").and_then(matrix_key) {
            out.push_str(&format!("${{{{ matrix.{m} }}}}"));
        } else {
            push_note(notes, format!("${{{{ {e} }}}} in a cache key"));
        }
        rest = &rest[i + j + 2..];
    }
    out.push_str(rest);
    let out = out.trim_matches('-').to_string();
    if super::cache::check_key(&out).is_err() { String::new() } else { out }
}

// ---------------------------------------------------------------- GitLab CI

/// Keys at the top of `.gitlab-ci.yml` that aren't jobs.
const GITLAB_GLOBAL: &[&str] = &["image", "services", "stages", "variables", "default", "include", "workflow", "before_script", "after_script", "cache", "spec"];
/// Runner tags that only say "a normal Linux runner": dropped. Others pick machines.
const GITLAB_GENERIC_TAGS: &[&str] = &["docker", "linux", "shared", "gitlab-org", "gitlab-org-docker"];

/// `.gitlab-ci.yml` → a pipeline: stages become `needs`, `extends` and anchors are
/// merged, and `only`/`except`/`rules` become each job's triggers where they can
/// (a rule that can't be understood here doesn't match, so the job doesn't run).
pub fn gitlab(src: &str, default_branch: &str) -> Result<Pipeline, String> {
    let v = config::parse_yaml(src).map_err(|e| format!("{GITLAB_FILE}: {e}"))?;
    let Value::Object(top) = &v else { return Err(format!("{GITLAB_FILE}: expected jobs")) };
    let mut gnotes = Vec::new();
    for (k, what) in [("include", "include (only this file is read)"), ("services", "services"), ("workflow", "workflow (each job's own rules decide)")] {
        if top.contains_key(k) {
            push_note(&mut gnotes, what.into());
        }
    }
    let default = top.get("default").and_then(Value::as_object).cloned().unwrap_or_default();
    let global = |k: &str| default.get(k).or_else(|| top.get(k)).cloned();
    let stages: Vec<String> = {
        let s = lines(top.get("stages"));
        let s = if s.is_empty() { vec!["build".into(), "test".into(), "deploy".into()] } else { s };
        std::iter::once(".pre".to_string()).chain(s).chain(std::iter::once(".post".to_string())).collect()
    };
    // Jobs: every other key not starting with '.', after `extends`.
    let mut memo = std::collections::HashMap::new();
    let mut jobs: Vec<(String, Map<String, Value>)> = Vec::new();
    for (k, v) in top {
        if GITLAB_GLOBAL.contains(&k.as_str()) || k.starts_with('.') {
            continue;
        }
        let Value::Object(j) = v else { continue };
        let j = extend(j, top, &mut vec![k.clone()], &mut memo).map_err(|e| format!("{GITLAB_FILE}: {k}: {e}"))?;
        if j.contains_key("trigger") {
            push_note(&mut gnotes, format!("{k}: trigger (child and multi-project pipelines)"));
            continue;
        }
        jobs.push((gl_key(k), j));
    }
    let stage_of = |j: &Map<String, Value>| j.get("stage").and_then(Value::as_str).unwrap_or("test").to_string();
    let index = |s: &str| stages.iter().position(|x| x == s);
    let mut out = Map::new();
    let mut whens: Vec<(String, Triggers)> = Vec::new();
    let vars = top.get("variables").and_then(Value::as_object).cloned().unwrap_or_default();
    for (key, j) in &jobs {
        let mut notes = gnotes.clone();
        let stage = stage_of(j);
        let Some(si) = index(&stage) else { return Err(format!("{GITLAB_FILE}: {key}: stage '{stage}' isn't in stages")) };
        let mut o = Map::new();
        // needs: as written; else every job of the stage before (GitLab's order).
        let needs: Vec<String> = match j.get("needs") {
            Some(Value::Array(a)) => a.iter().filter_map(|n| n.as_str().map(String::from).or_else(|| n.get("job").and_then(Value::as_str).map(String::from))).map(|n| gl_key(&n)).collect(),
            _ => {
                let prev = (0..si).rev().find(|&i| jobs.iter().any(|(_, x)| index(&stage_of(x)) == Some(i)));
                prev.map(|p| jobs.iter().filter(|(_, x)| index(&stage_of(x)) == Some(p)).map(|(k, _)| k.clone()).collect()).unwrap_or_default()
            }
        };
        let needs: Vec<String> = needs.into_iter().filter(|n| jobs.iter().any(|(k, _)| k == n)).collect();
        if !needs.is_empty() {
            o.insert("needs".into(), json!(needs));
        }
        // Tags pick runners: specific ones are kept, so the job waits for such a runner.
        let tags: Vec<String> = lines(j.get("tags")).into_iter().filter(|t| !GITLAB_GENERIC_TAGS.contains(&t.as_str()) && !t.contains('$')).collect();
        if !tags.is_empty() {
            push_note(&mut notes, format!("tags {} (it waits for a runner with those labels: gitter runner --labels)", tags.join(", ")));
            o.insert("runs-on".into(), json!(tags));
        }
        let img = match j.get("image").cloned().or_else(|| global("image")) {
            Some(Value::String(s)) => Some(s),
            Some(Value::Object(m)) => m.get("name").and_then(Value::as_str).map(String::from),
            _ => None,
        };
        if let Some(i) = img {
            match image(&i) {
                Some(i) => {
                    o.insert("image".into(), json!(i));
                }
                None => {
                    out.insert(key.clone(), failing(o, key, &format!("its image {i} can't be used here (it uses variables)")));
                    whens.push((key.clone(), gl_when(j, default_branch, &mut notes)));
                    continue;
                }
            }
        }
        if j.contains_key("services") {
            push_note(&mut notes, "services".into());
        }
        let mut env = Map::new();
        for (k, v) in vars.iter().chain(j.get("variables").and_then(Value::as_object).into_iter().flatten()) {
            let val = match v {
                Value::Object(m) => m.get("value").map(text).unwrap_or_default(),
                other => text(other),
            };
            if !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !k.starts_with(|c: char| c.is_ascii_digit()) {
                env.insert(k.clone(), json!(val));
            }
        }
        if !env.is_empty() {
            o.insert("env".into(), Value::Object(env));
        }
        if let Some(t) = j.get("timeout").and_then(Value::as_str).and_then(gl_duration) {
            o.insert("timeout".into(), json!(t));
        }
        match j.get("parallel") {
            Some(Value::Object(p)) => {
                if let Some(Value::Object(pm)) = p.get("matrix").and_then(Value::as_array).and_then(|a| a.first()) {
                    let axes: Map<String, Value> = pm.iter().map(|(k, v)| (k.clone(), if v.is_array() { v.clone() } else { json!([text(v)]) })).collect();
                    if p["matrix"].as_array().is_some_and(|a| a.len() > 1) {
                        push_note(&mut notes, "parallel:matrix beyond its first entry".into());
                    }
                    o.insert("matrix".into(), Value::Object(axes));
                }
            }
            Some(n) => push_note(&mut notes, format!("parallel: {} (it runs once)", text(n))),
            None => {}
        }
        if let Some(e) = environment(j.get("environment"), &mut notes) {
            o.insert("environment".into(), e);
        }
        if let Some(Value::Object(a)) = j.get("artifacts") {
            let paths = workspace_paths(lines(a.get("paths")), "artifact", &mut notes);
            if !paths.is_empty() {
                o.insert("artifacts".into(), json!(paths));
            }
            if let Some(r) = a.get("reports").and_then(Value::as_object) {
                let junit = workspace_paths(lines(r.get("junit")), "report", &mut notes);
                if !junit.is_empty() {
                    o.insert("reports".into(), json!(junit));
                }
                if let Some(c) = r.get("coverage_report").and_then(|c| c.get("path")).and_then(Value::as_str) {
                    o.insert("coverage".into(), json!([c]));
                }
            }
        }
        if let Some(c) = j.get("cache").cloned().or_else(|| global("cache")).and_then(|c| gl_cache(&c, &mut notes)) {
            o.insert("cache".into(), c);
        }
        let when = gl_when(j, default_branch, &mut notes);
        let before = script(j.get("before_script").cloned().or_else(|| global("before_script")).as_ref());
        let main = script(j.get("script"));
        let after = script(j.get("after_script").cloned().or_else(|| global("after_script")).as_ref());
        if main.is_empty() {
            return Err(format!("{GITLAB_FILE}: {key}: a job needs a script"));
        }
        if !after.is_empty() {
            push_note(&mut notes, "after_script runs only when the script passed".into());
        }
        let mut steps = Vec::new();
        if let Some(n) = notes_step(GITLAB_FILE, &notes) {
            steps.push(n);
        }
        // before_script and script share one shell, as in GitLab.
        let body = before.iter().chain(main.iter()).cloned().collect::<Vec<_>>().join("\n");
        steps.push(step("script", GITLAB_PRELUDE, &[], None, None, "gitlab", &body, false));
        if !after.is_empty() {
            steps.push(step("after_script", GITLAB_PRELUDE, &[], None, None, "gitlab", &after.join("\n"), true));
        }
        o.insert("steps".into(), Value::Array(steps));
        out.insert(key.clone(), Value::Object(o));
        whens.push((key.clone(), when));
    }
    if out.is_empty() {
        return Err(format!("{GITLAB_FILE}: no jobs"));
    }
    let cfg = json!({ "on": { "push": {}, "pull_request": {}, "manual": true }, "jobs": out });
    let mut p = config::parse_value(cfg).map_err(|e| format!("{GITLAB_FILE}: {e}"))?;
    let mut all = Triggers::default();
    for j in &mut p.jobs {
        if let Some((_, t)) = whens.iter().find(|(k, _)| *k == j.key) {
            merge_triggers(&mut all, t);
            j.when = Some(t.clone());
        }
    }
    p.triggers = all;
    Ok(p)
}

/// A script: a list of commands, or one block of text kept as written.
fn script(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::String(s)) => vec![s.clone()],
        Some(Value::Array(a)) => a.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
        _ => vec![],
    }
}

/// GitLab job names may have spaces and slashes; Gitter keys can't.
fn gl_key(k: &str) -> String {
    let s: String = k.chars().map(|c| if c.is_ascii_alphanumeric() || "-_.".contains(c) { c } else { '-' }).collect();
    s.chars().take(60).collect()
}

/// A job with its `extends` merged in (bases first; the job's own keys win). Each
/// base is worked out once; a loop is an error.
fn extend(j: &Map<String, Value>, top: &Map<String, Value>, chain: &mut Vec<String>, memo: &mut std::collections::HashMap<String, Map<String, Value>>) -> Result<Map<String, Value>, String> {
    if chain.len() > 10 {
        return Err("extends goes more than 10 levels deep".into());
    }
    let bases = lines(j.get("extends"));
    if bases.len() > 10 {
        return Err("extends names more than 10 templates".into());
    }
    let mut out = Map::new();
    for base in bases {
        if chain.contains(&base) {
            return Err(format!("extends loops back to {base}"));
        }
        let resolved = match memo.get(&base) {
            Some(r) => r.clone(),
            None => {
                let Some(Value::Object(b)) = top.get(&base) else { return Err(format!("extends: there's no {base}")) };
                chain.push(base.clone());
                let r = extend(b, top, chain, memo)?;
                chain.pop();
                memo.insert(base.clone(), r.clone());
                r
            }
        };
        merge_job(&mut out, &resolved);
    }
    merge_job(&mut out, j);
    out.remove("extends");
    Ok(out)
}

/// GitLab's merge for `extends`: mappings (variables) merge key by key; the rest replaces.
fn merge_job(out: &mut Map<String, Value>, from: &Map<String, Value>) {
    for (k, v) in from {
        if k == "extends" {
            continue;
        }
        match (out.get_mut(k), v) {
            (Some(Value::Object(old)), Value::Object(new)) if k == "variables" => {
                for (a, b) in new {
                    old.insert(a.clone(), b.clone());
                }
            }
            _ => {
                out.insert(k.clone(), v.clone());
            }
        }
    }
}

/// "1h 30m", "90 minutes", "2h" → minutes (1 to 360).
fn gl_duration(s: &str) -> Option<u64> {
    let (mut mins, mut num) = (0u64, String::new());
    for w in s.split_whitespace().flat_map(|w| {
        let i = w.find(|c: char| !c.is_ascii_digit()).unwrap_or(w.len());
        [w[..i].to_string(), w[i..].to_string()]
    }) {
        if w.is_empty() {
            continue;
        }
        if w.chars().all(|c| c.is_ascii_digit()) {
            num = w;
            continue;
        }
        let n: u64 = num.parse().ok()?;
        mins += match w.trim_end_matches('s') {
            "h" | "hr" | "hour" => n * 60,
            "m" | "min" | "minute" => n,
            "d" | "day" => n * 24 * 60,
            _ => return None,
        };
        num.clear();
    }
    (mins > 0).then_some(mins.clamp(1, 360))
}

fn gl_cache(c: &Value, notes: &mut Vec<String>) -> Option<Value> {
    let c = if let Value::Array(a) = c { a.first()? } else { c };
    let paths = workspace_paths(lines(c.get("paths")), "cache", notes);
    if paths.is_empty() {
        return None;
    }
    let key = match c.get("key") {
        Some(Value::Object(k)) => {
            let files = lines(k.get("files"));
            let prefix = k.get("prefix").map(text).unwrap_or_default();
            if files.is_empty() { prefix } else { format!("{prefix}{}{{{{ hash {} }}}}", if prefix.is_empty() { "" } else { "-" }, files.join(" ")) }
        }
        Some(v) => text(v).replace("$CI_COMMIT_REF_SLUG", "").replace("${CI_COMMIT_REF_SLUG}", "").replace("$CI_JOB_NAME", "").trim_matches('-').to_string(),
        None => "default".into(),
    };
    let key = if key.is_empty() || key.contains('$') { "default".to_string() } else { key };
    super::cache::check_key(&key).ok()?;
    Some(json!({ "key": key, "paths": paths.into_iter().take(20).collect::<Vec<_>>() }))
}

/// One understood piece of a rule's `if:`.
enum Atom {
    Source(&'static str),
    Branch(String),
    Never,
}

/// A rule's `if:` as the events it allows, or None if any part of it isn't understood.
fn gl_if(c: &str, default_branch: &str) -> Option<Vec<Atom>> {
    if c.len() > 1000 || c.contains("||") || c.contains("=~") || c.contains('(') {
        return None;
    }
    let mut out = Vec::new();
    for part in c.split("&&") {
        let p = part.replace(' ', "");
        let val = |p: &str, var: &str| p.strip_prefix(&format!("{var}==")).map(|v| v.trim_matches(|x| x == '"' || x == '\'').to_string());
        let atom = if let Some(s) = val(&p, "$CI_PIPELINE_SOURCE") {
            Atom::Source(match s.as_str() {
                "merge_request_event" => "pull_request",
                "push" => "push",
                "web" => "manual",
                _ => return None,
            })
        } else if let Some(b) = val(&p, "$CI_COMMIT_BRANCH").or_else(|| val(&p, "$CI_COMMIT_REF_NAME")) {
            match b.as_str() {
                "$CI_DEFAULT_BRANCH" => Atom::Branch(default_branch.to_string()),
                b if !b.contains('$') && !b.is_empty() => Atom::Branch(b.to_string()),
                _ => return None,
            }
        } else {
            match p.as_str() {
                "$CI_COMMIT_BRANCH" => Atom::Source("push"),
                "$CI_MERGE_REQUEST_IID" | "$CI_MERGE_REQUEST_ID" => Atom::Source("pull_request"),
                "$CI_COMMIT_TAG" => Atom::Never,
                _ => return None,
            }
        };
        out.push(atom);
    }
    Some(out)
}

/// When a GitLab job runs: `when: manual`, `only`/`except` with branch names and
/// merge_requests, and `rules` on the pipeline source and branch. Anything not
/// understood keeps the job from running (with a note) rather than running it anywhere.
fn gl_when(j: &Map<String, Value>, default_branch: &str, notes: &mut Vec<String>) -> Triggers {
    let all = Triggers { push: Some(Filter::default()), pull_request: None, manual: true, schedule: vec![] };
    let never = Triggers::default();
    if j.get("when").and_then(Value::as_str) == Some("manual") {
        return Triggers { manual: true, ..Default::default() };
    }
    if let Some(except) = j.get("except") {
        let refs = match except {
            Value::Object(m) => lines(m.get("refs")),
            other => lines(Some(other)),
        };
        // Tags and schedules never start these runs anyway; anything else can't be left out here.
        if refs.iter().any(|r| !matches!(r.as_str(), "tags" | "schedules" | "triggers" | "api" | "external" | "pipelines")) || except.get("changes").is_some() || except.get("variables").is_some() {
            push_note(notes, format!("except: {} (it can't be left out of those here, so it doesn't run)", refs.join(", ")));
            return never;
        }
    }
    if let Some(only) = j.get("only") {
        let refs = match only {
            Value::Object(m) => lines(m.get("refs")),
            other => lines(Some(other)),
        };
        let mut t = Triggers { manual: true, ..Default::default() };
        let mut branches = Vec::new();
        for r in refs {
            match r.as_str() {
                "merge_requests" => t.pull_request = Some(Filter::default()),
                "branches" | "pushes" => t.push = Some(Filter::default()),
                "web" => {}
                "tags" | "schedules" | "api" | "triggers" | "pipelines" | "external" => push_note(notes, format!("only: {r}")),
                b if !b.starts_with('/') && !b.contains('$') => branches.push(b.to_string()),
                b => push_note(notes, format!("only: {b} (patterns aren't understood here)")),
            }
        }
        if !branches.is_empty() && t.push.is_none() {
            t.push = Some(Filter { branches, paths: vec![] });
        }
        if let Value::Object(m) = only
            && let Some(ch) = m.get("changes")
            && let Some(p) = &mut t.push
        {
            p.paths = lines(Some(ch));
        }
        return t;
    }
    let Some(Value::Array(rules)) = j.get("rules") else { return all };
    // Rules are tried in order and the first match decides: a `when: never` that
    // isn't a plain source can't be expressed, so the job doesn't run.
    let mut t = Triggers::default();
    let mut barred: Vec<&'static str> = Vec::new();
    for r in rules {
        let when = r.get("when").and_then(Value::as_str).unwrap_or("on_success");
        let paths = lines(r.get("changes"));
        let atoms = match r.get("if").and_then(Value::as_str) {
            None => Some(vec![]),
            Some(c) => gl_if(c, default_branch).or_else(|| {
                push_note(notes, format!("rules: if {c} (not understood here, so this rule doesn't match)"));
                None
            }),
        };
        let Some(atoms) = atoms else {
            if when == "never" {
                return never;
            }
            continue;
        };
        if atoms.iter().any(|a| matches!(a, Atom::Never)) {
            continue;
        }
        let sources: Vec<&'static str> = atoms.iter().filter_map(|a| if let Atom::Source(s) = a { Some(*s) } else { None }).collect();
        let branches: Vec<String> = atoms.iter().filter_map(|a| if let Atom::Branch(b) = a { Some(b.clone()) } else { None }).collect();
        if sources.len() > 1 || branches.len() > 1 {
            continue;
        }
        if when == "never" {
            match (sources.first(), branches.is_empty()) {
                (Some(s), true) => barred.push(s),
                (None, true) => return t,
                _ => {
                    push_note(notes, "rules: when: never for some branches (it can't be left out of them here, so it doesn't run)".into());
                    return never;
                }
            }
            continue;
        }
        if when == "manual" {
            t.manual = true;
            continue;
        }
        let filter = Filter { branches: branches.clone(), paths: paths.clone() };
        let src = sources.first().copied();
        if src.is_none_or(|s| s == "push") && !barred.contains(&"push") {
            add(&mut t.push, &filter);
        }
        if src == Some("pull_request") && !barred.contains(&"pull_request") {
            add(&mut t.pull_request, &filter);
        }
        if src.is_none_or(|s| s == "manual") && !barred.contains(&"manual") {
            t.manual = true;
        }
    }
    t
}

/// Widen `slot` to also allow `f` (no filter allows everything).
fn add(slot: &mut Option<Filter>, f: &Filter) {
    match slot {
        None => *slot = Some(f.clone()),
        Some(old) => {
            if old.branches.is_empty() || f.branches.is_empty() {
                old.branches.clear();
            } else {
                old.branches.extend(f.branches.iter().cloned());
            }
            if old.paths.is_empty() || f.paths.is_empty() {
                old.paths.clear();
            } else {
                old.paths.extend(f.paths.iter().cloned());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job<'a>(p: &'a Pipeline, key: &str) -> &'a config::Job {
        p.jobs.iter().find(|j| j.key == key).unwrap_or_else(|| panic!("no job {key}: {:?}", p.jobs.iter().map(|j| &j.key).collect::<Vec<_>>()))
    }

    #[test]
    fn environments_come_across_when_they_are_plain() {
        let gh = "on: push\njobs:\n  deploy:\n    runs-on: ubuntu-latest\n    environment: { name: production, url: https://example.com }\n    steps: [{ run: echo hi }]\n  pr:\n    runs-on: ubuntu-latest\n    environment: preview-${{ github.event.number }}\n    steps: [{ run: echo hi }]\n";
        let p = github(&[(".github/workflows/ci.yml".into(), gh.into())]).unwrap().unwrap();
        assert_eq!(job(&p, "deploy").environment, Some(config::Environment { name: "production".into(), url: "https://example.com".into() }));
        assert_eq!(job(&p, "pr").environment, None, "a name made by an expression stays out");
        let gl = "deploy:\n  script: [echo hi]\n  environment: staging\nreview:\n  script: [echo hi]\n  environment:\n    name: review/$CI_COMMIT_REF_SLUG\nstop:\n  script: [echo bye]\n  environment: { name: staging, action: stop }\n";
        let p = gitlab(gl, "main").unwrap();
        assert_eq!(job(&p, "deploy").environment.as_ref().map(|e| e.name.as_str()), Some("staging"));
        assert_eq!(job(&p, "review").environment, None);
        assert_eq!(job(&p, "stop").environment, None, "stopping an environment isn't deploying to it");
    }

    fn all_runs(j: &config::Job) -> String {
        j.steps.iter().map(|s| s.run.as_str()).collect::<Vec<_>>().join("\n---\n")
    }

    #[test]
    fn github_workflows_translate_with_their_own_triggers() {
        let ci = r#"
name: CI
on:
  push:
    branches: [main]
  pull_request:
  workflow_dispatch:
env:
  CARGO_TERM_COLOR: always
  TOKEN: ${{ secrets.API_TOKEN }}
jobs:
  test:
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, windows-latest]
        rust: [stable]
        include:
          - os: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: actions/cache@v4
        with:
          path: |
            target
            ~/.cargo/registry
          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}
          restore-keys: |
            ${{ runner.os }}-cargo-
      - name: Test
        run: cargo test --verbose
        env:
          RUST: ${{ matrix.rust }}
      - name: Only on main
        if: github.ref == 'refs/heads/main' && github.event_name == 'push'
        run: echo deploying ${{ github.sha }}
      - name: Draft check
        if: github.event.pull_request.draft == false
        run: echo not draft
      - uses: actions/upload-artifact@v4
        with:
          name: bin
          path: target/release/app
  lint:
    needs: test
    runs-on: ubuntu-latest
    container: rust:1.80
    steps:
      - run: cargo clippy
        working-directory: crates/core
        shell: bash
"#;
        let nightly = "on:\n  schedule:\n    - cron: '0 3 * * *'\njobs:\n  fuzz:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo fuzz\n";
        let called = "on: workflow_call\njobs:\n  x:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo hi\n";
        let p = github(&[(".github/workflows/ci.yml".into(), ci.into()), (".github/workflows/nightly.yml".into(), nightly.into()), (".github/workflows/called.yml".into(), called.into())]).unwrap().unwrap();
        // Two workflows ran (the reusable one alone never runs); job keys carry the workflow.
        let keys: Vec<&str> = p.jobs.iter().map(|j| j.key.as_str()).collect();
        assert_eq!(keys, ["ci.test", "ci.test", "ci.lint", "nightly.fuzz"]);
        let t = job(&p, "ci.test");
        assert_eq!(t.name, "CI / test (ubuntu-latest, stable)");
        let when = t.when.as_ref().unwrap();
        assert_eq!(when.push.as_ref().unwrap().branches, ["main"]);
        assert!(when.pull_request.is_some() && when.manual && when.schedule.is_empty());
        assert_eq!(job(&p, "nightly.fuzz").when.as_ref().unwrap().schedule, ["0 3 * * *"]);
        assert_eq!(p.triggers.schedule, ["0 3 * * *"], "the pipeline wants what any workflow wants");
        assert_eq!(job(&p, "ci.lint").needs, ["ci.test"]);
        assert_eq!(job(&p, "ci.lint").image.as_deref(), Some("rust:1.80"));
        // Cache and artifacts map; checkout is already done; setup actions are noted.
        let c = t.cache.as_ref().unwrap();
        assert_eq!((c.key.as_str(), c.paths.clone(), c.restore_keys.clone()), ("{{ os }}-cargo-{{ hash **/Cargo.lock }}", vec!["target".to_string()], vec!["{{ os }}-cargo".to_string()]));
        assert_eq!(t.artifacts, ["target/release/app"]);
        let runs = all_runs(t);
        assert!(runs.contains("dtolnay/rust-toolchain@stable (tool setup") && runs.contains("strategy.matrix.include") && runs.contains("windows-latest (jobs run on Linux here)"), "{runs}");
        assert!(runs.contains("cache path ~/.cargo/registry"), "{runs}");
        assert!(!runs.contains("actions/checkout"), "{runs}");
        // Expressions: secrets and env as variables, matrix filled in, github.* mapped.
        assert!(t.env.get("CARGO_TERM_COLOR").is_some_and(|v| v == "always"));
        assert!(runs.contains("export TOKEN=\"${API_TOKEN}\""), "{runs}");
        assert!(runs.contains("export RUST=\"stable\""), "the matrix is filled in: {runs}");
        assert!(runs.contains("echo deploying ${GITHUB_SHA}"), "{runs}");
        assert!(runs.contains("if ! { { [ \"${GITHUB_REF}\" = \"refs/heads/main\" ]; } && { [ \"${GITHUB_EVENT_NAME}\" = \"push\" ]; }; }; then"), "{runs}");
        assert!(runs.contains("github.event.pull_request.draft == false (the step is skipped)"), "{runs}");
        let lint = all_runs(job(&p, "ci.lint"));
        assert!(lint.contains("cd 'crates/core'") && lint.contains("bash --noprofile --norc -eo pipefail"), "{lint}");
        assert!(!lint.contains("Notes on"), "nothing left out of lint: {lint}");
    }

    #[test]
    fn a_reusable_workflow_call_fails_loudly_and_tags_only_never_run() {
        let w = "on: push\njobs:\n  call:\n    uses: org/repo/.github/workflows/x.yml@main\n";
        let p = github(&[(".github/workflows/w.yml".into(), w.into())]).unwrap().unwrap();
        assert!(p.jobs[0].steps[0].run.contains("exit 1") && p.jobs[0].steps[0].run.contains("reusable workflow"));
        let tags = "on:\n  push:\n    tags: ['v*']\njobs:\n  rel:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo\n";
        assert!(github(&[(".github/workflows/rel.yml".into(), tags.into())]).unwrap().is_none(), "nothing here runs: no pipeline, not a failed one");
    }

    #[test]
    fn conditions_become_shell_tests() {
        assert_eq!(gh_cond("always()").as_deref(), Some("true"));
        assert_eq!(gh_cond("${{ failure() }}").as_deref(), Some("false"));
        assert_eq!(gh_cond("startsWith(github.ref, 'refs/tags/')").as_deref(), Some("case \"${GITHUB_REF}\" in refs/tags/*) true ;; *) false ;; esac"));
        assert_eq!(gh_cond("matrix.os != 'windows-latest'").as_deref(), Some("! [ \"${{ matrix.os }}\" = \"windows-latest\" ]"));
        assert!(gh_cond("contains(github.event.head_commit.message, '[skip]')").is_none());
        // Nothing from the file reaches the shell unquoted.
        assert_eq!(gh_cond("github.ref == 'a\"$(rm -rf /)`x`'").as_deref(), Some("[ \"${GITHUB_REF}\" = \"a(rm -rf /)x\" ]"));
    }

    #[test]
    fn gitlab_stages_extends_anchors_and_rules() {
        let src = r#"
stages: [build, test, deploy]
variables:
  CARGO_HOME: .cargo
default:
  image: rust:1.80
  before_script:
    - rustc --version
.cache: &cache
  cache:
    key:
      files: [Cargo.lock]
    paths: [.cargo, target/]
.tests:
  stage: test
  script:
    - cargo test
build:
  stage: build
  <<: *cache
  script: cargo build
  artifacts:
    paths: [target/debug/app]
unit tests:
  extends: .tests
  variables:
    RUST_BACKTRACE: "1"
  artifacts:
    reports:
      junit: report.xml
lint:
  stage: test
  image: alpine
  script: [echo lint]
  rules:
    - if: $CI_PIPELINE_SOURCE == "merge_request_event"
deploy:
  stage: deploy
  script: ./deploy.sh
  after_script: [echo done]
  rules:
    - if: $CI_COMMIT_BRANCH == "main"
  timeout: 1h 30m
manual job:
  stage: deploy
  when: manual
  needs: [build]
  script: echo by hand
"#;
        let p = gitlab(src, "main").unwrap();
        let b = job(&p, "build");
        assert_eq!((b.image.as_deref(), b.needs.len()), (Some("rust:1.80"), 0));
        assert_eq!(b.cache.as_ref().unwrap().key, "{{ hash Cargo.lock }}");
        assert_eq!(b.artifacts, ["target/debug/app"]);
        let u = job(&p, "unit-tests");
        assert_eq!(u.needs, ["build"], "the stage before");
        assert_eq!((u.env.get("RUST_BACKTRACE").map(String::as_str), u.env.get("CARGO_HOME").map(String::as_str)), (Some("1"), Some(".cargo")));
        assert_eq!(u.reports, ["report.xml"]);
        assert!(all_runs(u).contains("rustc --version\ncargo test"), "before_script and script in one shell");
        let l = job(&p, "lint");
        assert_eq!(l.image.as_deref(), Some("alpine"));
        let lw = l.when.as_ref().unwrap();
        assert!(lw.pull_request.is_some() && lw.push.is_none(), "merge requests only");
        let d = job(&p, "deploy");
        assert_eq!(d.needs, ["unit-tests", "lint"]);
        assert_eq!(d.when.as_ref().unwrap().push.as_ref().unwrap().branches, ["main"]);
        assert_eq!(d.timeout_secs, 90 * 60);
        assert_eq!(d.steps.len(), 3, "a note, the script, then after_script");
        assert!(d.steps[0].run.contains("after_script runs only when the script passed"));
        let m = job(&p, "manual-job");
        assert_eq!((m.needs.clone(), m.when.as_ref().unwrap().manual, m.when.as_ref().unwrap().push.is_none()), (vec!["build".to_string()], true, true));
        assert!(gitlab("x:\n  stage: nope\n  script: [a]\n", "main").unwrap_err().contains("stage 'nope'"));
    }
}

#[cfg(test)]
mod review_tests {
    //! What the review of the first version found.
    use super::*;

    fn gl(src: &str) -> Result<Pipeline, String> {
        gitlab(src, "trunk")
    }

    #[test]
    fn hostile_files_cant_hang_crash_or_inject() {
        // extends: loops are errors, and a fan-out of templates is worked out once each.
        assert!(gl(".a:\n  extends: .b\n.b:\n  extends: .a\nj:\n  extends: .a\n  script: [x]\n").unwrap_err().contains("loops"));
        // 8 levels of 8 templates, each extending all 8 below: 8^8 merges without memoizing.
        let mut fan = String::new();
        for i in 0..8 {
            for k in 0..8 {
                let next: Vec<String> = (0..8).map(|m| format!(".t{}_{m}", i + 1)).collect();
                fan += &format!(".t{i}_{k}:\n  extends: [{}]\n", next.join(", "));
            }
        }
        for m in 0..8 {
            fan += &format!(".t8_{m}:\n  script: [x]\n");
        }
        fan += "j:\n  extends: .t0_0\n";
        let t = std::time::Instant::now();
        assert!(gl(&fan).is_ok() && t.elapsed() < std::time::Duration::from_secs(2));
        // Deeply nested conditions are refused, not recursed into.
        assert!(gh_cond(&"!".repeat(5000)).is_none());
        assert!(gh_cond(&format!("{}true{}", "(".repeat(400), ")".repeat(400))).is_none());
        // Non-ASCII text in a condition doesn't panic.
        assert!(gh_cond("github.ref == 'é' && runner.os == 'Linux'").is_some());
        // Case patterns and matrix keys can't carry shell syntax.
        assert!(gh_cond("startsWith(github.ref, 'x) true ;; esac; id; case y in z')").is_none());
        assert!(gh_cond("matrix.v\"; id; echo \" == 'x'").is_none());
        let mut n = vec![];
        assert_eq!(gh_value("matrix.x\" ; id ; \"", &mut n), "");
        let (mut env, mut ex) = (Map::new(), vec![]);
        env_entry("A", "${{ 'a\"$(id)' }}b", &mut env, &mut ex, &mut n);
        assert_eq!(ex, ["export A=\"a\\\"\\$(id)b\""]);
        env_entry("B", "pre ${{ unterminated", &mut env, &mut ex, &mut n);
        assert_eq!(ex[1], "export B=\"pre \\${{ unterminated\"", "the prefix appears once");
        // A note can't end the text it's printed in.
        push_note(&mut n, "a\nGITTER_NOTES_EOF\nid".into());
        assert!(notes_step("x", &n).unwrap()["run"].as_str().unwrap().matches("\nGITTER_NOTES_EOF\n").count() == 1);
    }

    #[test]
    fn workflows_fail_closed_and_dont_take_each_other_down() {
        let good = "on: push\njobs:\n  a:\n    runs-on: [self-hosted, prod]\n    steps:\n      - run: echo a\n";
        let bad = "on: push\njobs:\n  b: [\n";
        let var_image = "on: push\njobs:\n  c:\n    runs-on: ubuntu-latest\n    container: ${{ vars.IMAGE }}\n    steps:\n      - run: echo c\n";
        let p = github(&[(".github/workflows/good.yml".into(), good.into()), (".github/workflows/bad.yml".into(), bad.into()), (".github/workflows/img.yml".into(), var_image.into())]).unwrap().unwrap();
        let a = p.jobs.iter().find(|j| j.key == "good.a").unwrap();
        assert_eq!(a.runs_on, ["prod"], "a deploy to particular machines waits for them");
        assert!(a.steps[0].run.contains("bad.yml"), "the broken workflow is named: {}", a.steps[0].run);
        let c = p.jobs.iter().find(|j| j.key == "img.c").unwrap();
        assert!(c.image.is_none() && c.steps.iter().any(|s| s.run.contains("exit 1")), "no image it can use: it fails rather than running on the host");
        assert!(github(&[(".github/workflows/bad.yml".into(), bad.into())]).is_err(), "only broken: an error to show");
    }

    #[test]
    fn gitlab_rules_run_only_when_understood() {
        let t = |rules: &str| {
            let p = gl(&format!("j:\n  script: [x]\n  rules:\n{rules}")).unwrap();
            p.jobs[0].when.clone().unwrap()
        };
        // Both halves understood: main's pushes only.
        let w = t("    - if: $CI_COMMIT_BRANCH == \"main\" && $CI_PIPELINE_SOURCE == \"push\"\n");
        assert_eq!(w.push.unwrap().branches, ["main"]);
        // Not understood: it doesn't run (instead of running everywhere).
        let w = t("    - if: $CI_COMMIT_REF_PROTECTED == \"true\"\n");
        assert!(w.push.is_none() && w.pull_request.is_none() && !w.manual);
        let w = t("    - if: $CI_COMMIT_BRANCH =~ /^release/\n");
        assert!(w.push.is_none());
        // The default branch is the repository's.
        assert_eq!(t("    - if: $CI_COMMIT_BRANCH == $CI_DEFAULT_BRANCH\n").push.unwrap().branches, ["trunk"]);
        // when: never on merge requests, then always: pushes, not merge requests.
        let w = t("    - if: $CI_PIPELINE_SOURCE == \"merge_request_event\"\n      when: never\n    - when: always\n");
        assert!(w.push.is_some() && w.pull_request.is_none());
        // never for some branches can't be expressed: it doesn't run.
        let w = t("    - if: $CI_COMMIT_BRANCH == \"main\"\n      when: never\n    - when: always\n");
        assert!(w.push.is_none() && !w.manual);
        // except a branch can't be expressed either.
        let p = gl("j:\n  script: [x]\n  except: [main]\n").unwrap();
        assert!(p.jobs[0].when.as_ref().unwrap().push.is_none());
        let p = gl("j:\n  script: [x]\n  except: [tags]\n").unwrap();
        assert!(p.jobs[0].when.as_ref().unwrap().push.is_some(), "tags never start runs here anyway");
    }

    #[test]
    fn scripts_keep_their_text_and_tags_pick_runners() {
        let p = gl("j:\n  tags: [docker, gpu]\n  script: |\n    if true; then\n      echo indented\n\n    fi\n").unwrap();
        assert!(p.jobs[0].steps.last().unwrap().run.contains("if true; then\n  echo indented\n\nfi"), "{}", p.jobs[0].steps.last().unwrap().run);
        assert_eq!(p.jobs[0].runs_on, ["gpu"]);
        assert!(config::parse_yaml("a: |\n  x\n  && y\n  ! grep z\n  *glob\n  ---\n").is_ok(), "block text can start with anything");
    }

    #[test]
    fn a_job_left_out_hands_its_needs_on() {
        let p = gl("stages: [build, test, deploy]\nbuild:\n  stage: build\n  script: [b]\nmr:\n  stage: test\n  script: [t]\n  rules:\n    - if: $CI_PIPELINE_SOURCE == \"merge_request_event\"\ndeploy:\n  stage: deploy\n  script: [d]\n").unwrap();
        let picked = crate::ci::trigger::pick(Ok(p), "push", "main", &None).unwrap().unwrap();
        let keys: Vec<(&str, Vec<String>)> = picked.jobs.iter().map(|j| (j.key.as_str(), j.needs.clone())).collect();
        assert_eq!(keys, [("build", vec![]), ("deploy", vec!["build".to_string()])], "deploy still waits for build");
    }
}
