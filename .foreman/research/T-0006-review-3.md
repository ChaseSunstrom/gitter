**Verdict:** changes needed. Three MEDIUM issues, one in each of `src/api/ci.rs`, `src/ci/queue.rs` and `static/js/pages/ci.js`. The rest are LOW. Nothing was run; findings come from reading the code.

## lens: maintainer

- MEDIUM — src/api/ci.rs:434-441 — `delete_runner` runs its `UPDATE ... SET status='queued'` for any id, even though the name lookup and the `DELETE` are limited to `builtin = 0`. Scenario: `DELETE` on the built-in runner's id returns 404, but its running job has already lost its token and been requeued. The same SQL is also a second copy of `queue::requeue_expired`'s requeue, minus the `MAX_ATTEMPTS` cap. — Return 404 first when `name` is None. Move the requeue into a `queue::` function and call it from both places. — high
- MEDIUM — src/ci/queue.rs:241 — "Re-run failed" selects `status IN (..., 'skipped')`, which can't tell a job skipped because a dependency failed from one skipped by its `paths` filter (`create_run`, line 53). Scenario: a run has a docs job skipped by `paths` and a failing test job. "Re-run failed" sets the docs job to `waiting`, then `queued`, so it runs even though its paths didn't match. The doc comment says only "what they skipped". No test covers it, and `create_run`'s `skip` argument is untested too. — Re-run only jobs skipped through `needs`, or mark path-skipped jobs distinctly. Add a test. — high
- MEDIUM — static/js/pages/ci.js:192-201 — The SSE URL is built once, with `offset=bytes.length` fixed at that moment. `es.onerror` only handles `CLOSED`, so the browser's own reconnect reuses the stale offset. The server also ignores `Last-Event-ID` (src/api/ci.rs:241 sets the id but never reads it). It also ends the stream silently after 3600 polls (~30 min, src/api/ci.rs:232). Scenario: a network blip or a job longer than 30 min makes the browser reconnect and re-send the log from the old offset. `bytes` then holds duplicated output and the step folds, which are cut by byte offset, are wrong. — On error, close and call `stream(el)` again with the current `bytes.length`. — medium
- LOW — src/ci/queue.rs:105 — `claim` reads up to 500 queued specs and parses the JSON of each to match labels. A job past the first 500 that needs a label no runner has can starve other jobs. — Filter on labels in SQL, or store `runs_on` in a column. — medium
- LOW — src/ci/runner.rs:30 and src/ci/http.rs:29 — `ARTIFACT_CAP` is defined twice with the same value. — Define it once, for example in `jobs.rs` next to `LOG_CAP`. — high
- LOW — src/ci/runner.rs:84-175 (`Backend::heartbeat` and `finish`), src/ci/http.rs:107-163 — Local and Remote each call `queue::heartbeat` and `queue::finish` themselves, and each notifies `ci_wake` on its own (runner.rs:164, http.rs:161). The step-status whitelist sits in `jobs::step` (jobs.rs:74) while the runner sends its own `&'static str`. — Add `jobs::finish` and `jobs::heartbeat` that wake the queue; both backends then call them. — medium
- LOW — src/ci/runner.rs:52-60 — `Backend::http` returns a runtime error "not a remote runner" for a state the types could rule out, and every method repeats `format!("Bearer {token}")` and the `status == 200` check. — Move the HTTP methods onto a `struct Remote`, and `source` could return `(url, key, value)` instead of a one-element array indexed with `config[0]`. — medium
- LOW — src/ci/runner.rs:440-545 — `steps()` is 105 lines doing checkout, docker probing, step execution and the chown, with no unit tests. `pump`'s secret masking and batching is the riskiest logic and can't be tested without a `Backend`. — Split out `checkout()`, and take the log sink as a closure or trait in `pump`. — medium
- LOW — src/ci/runner.rs:594 — `Saved {n} file(s)` reports `files.len()`, not the number packed, when the size cap cut the archive. The `Ok(Err(e))` and `Err(e)` arms (lines 598-599) are the same. — Count the files actually appended and merge the two arms. — high
- LOW — src/ci/config.rs:383 and src/ci/config.rs:436 — The "all triggers" literal appears twice. — Add `Triggers::all()`. — high
- LOW — src/ci/config.rs:530 — The comment says "spaces optional", but only exactly one space or none is matched (line 534). — Reword the comment, or normalise the whitespace. — high
- LOW — src/ci/config.rs:753-754 — `let _ = y;` silences an unused year. — Drop the year from `civil`'s tuple and its test. — high
- LOW — src/ci/config.rs:590-594 — `paths`, `timeout`, `artifacts` and `runs-on` are re-parsed for every matrix combination. — Hoist them above the `for combo` loop. — high
- LOW — src/api/ci.rs:149, 162, 91 — `load_run(...)?["id"].as_i64().unwrap_or(0)` loads the full run JSON just to get the id. A missing id becomes 0 instead of an error. — Add `run_id(st, repo, number) -> R<i64>`. — high
- LOW — src/api/ci.rs:406-409 and 427-430 — These handlers use `Sudo` plus a manual `is_admin` check, while `runners` uses the `Admin` extractor. — Add an extractor for admin-with-fresh-sign-in, or at least comment on the split. — medium
- LOW — static/js/pages/ci.js:220 and 240 — The job-head `replaceWith(Object.assign(document.createElement('div'), ...))` is copy-pasted. Line 206 doesn't check `r.ok`, so an error body would render as log text. `ansi` is exported but used nowhere else. — Add a `renderJobHead(el)` helper, check `r.ok`, and drop the `export`. — high
- LOW — static/css/ci.css:17, 23, 42, 44 — `margin-left`, `padding-left` and `text-align:right` are physical properties, which breaks the css-layout card's RTL item. — Use the logical equivalents. — medium

## Acceptance criteria
Not stated in the brief; the audit file gives no AC list. Not verifiable.

## Implicit decisions
- Job state lives as a JSON `spec` in SQLite and is re-parsed on every claim and every `advance`.
- A runner sends its token as a Bearer header, and the job token also serves as git Basic auth.
- Log streaming polls the file and the database every 500 ms (marked `ponytail:`).
- Cron and `paths` matching are hand-rolled instead of using a crate.
- The `ureq` global timeout is 120 s, which also bounds a 1 GB artifact upload (runner.rs:45). This may cut off slow links (plausible, unmeasured).

## Not checked
The remaining files in the 9658-line diff outside the named modules, the Rust compile and clippy output, and the scripts. I didn't run tests.

**Noticed:** `can_start_manually` (src/api/ci.rs:68) does a git stat, blob read and YAML parse on every `/ci/runs` list request; cache it per sha.
