**Verdict: changes needed.** Four HIGH findings, each shown by a code path. I found them by reading, not by running anything.

## lens: adversary
- HIGH — src/ci/http.rs:78 (with :39) — `claim` takes `Json<ClaimReq>`, and axum extracts it before the runner token is checked. `DefaultBodyLimit::disable()` covers the whole router, so an unauthenticated `POST /ci-runner/claim` with a multi-GB body is buffered in memory and exhausts it. — Take `Bytes` capped at about 64 KB after `runner()`, or set `DefaultBodyLimit::max(64K)` on this route and disable it only for artifacts. — high
- HIGH — src/ci/config.rs:292-324 (reached from src/api/ci.rs:63,68-71) — `flow` and `scalar` recurse once per nesting level with no depth cap. `x: [[[[…]]]]` about 20 KB deep overflows the stack of a tokio worker. `runs` calls `can_start_manually`, then `pipeline_at`, then `parse`, for any viewer. It does not check `ci`. Any user can commit this file and one GET of `/api/repos/u/r/ci/runs` aborts the server. — Cap nesting at about 8. Skip blobs over about 256 KB using the size `stat` already returns, which `pipeline_at` ignores. Don't parse in `runs` when CI is off. — high
- HIGH — src/ci/config.rs:679-698 (called inside `db.call` at src/ci/trigger.rs:96-100) — `glob` backtracks exponentially. `paths: ["*a*a*a*a*a*a*a*a*b"]` against a long `aaaa…` file or branch name effectively never returns. In `start` it blocks the single SQLite thread, so the whole server stalls. It also blocks a worker in `wants`. It needs CI enabled for the owner, so it is exposed under `ci=everyone`. — Use an iterative or memoised matcher and compute `skip` outside `db.call`. — high
- HIGH — src/ci/runner.rs:363-365, 429 — After a step ends, a process it left running (`server &`) still holds the stdout/stderr pipes. Only the reader's `JoinHandle` is dropped on timeout, so the reader task keeps its `tx` clone. `run_job` then blocks forever on `logger.await`. The job is never finished, heartbeats continue, and the 6h deadline is not enforced because only `run()` checks it. Every step also costs an extra 10 s. — Always `kill -KILL -pgid` after `wait`, abort the readers, and put a timeout around `logger.await`. — high
- MEDIUM — src/api/issues.rs:718,775 → src/ci/trigger.rs:164 — `create_pull` only needs `readable`, and `on_pull_opened` never checks the actor. Any signed-in user can open a PR from the owner's branch, which starts a run with the repo's secrets. With no `on:` all triggers are on, so a deploy job reruns. — Skip unless `r.can_write`, or run without secrets. — high
- MEDIUM — src/api/ci.rs:160, src/ci/queue.rs:102 — Neither `rerun` nor `claim` checks `trigger::enabled`. Turning `ci` off, or demoting an owner, does not stop queued jobs or reruns. — Check it in `rerun`, and cancel queued jobs on a settings change. — high
- MEDIUM — src/ci/runner.rs:265-272, 277 — Masking misses two cases. A multi-line secret (PEM) is split across flushes, which go out on whole lines. A secret that is a substring of another leaves the longer one's suffix, because the replacements are not applied longest-first. — Sort longest first and also mask each line of multi-line secrets. — high
- MEDIUM — src/ci/jobs.rs:43-46, src/security.rs:202-219 — Secrets that fail to decrypt are dropped silently, for example after a restore without the key. A `secret.key` that is not exactly 32 bytes makes `server_key` recurse until the stack overflows, because `create_new` fails with AlreadyExists. — Warn in the job log, write the key by temp file and rename, and return an error on a bad length. — high
- MEDIUM — src/ci/jobs.rs:17-23 — Nothing prunes logs, artifacts, runs or `ci/work` (up to 1 GB per job). Deleting a repo or user leaves private logs on disk. Artifacts are not counted in quotas. — Sweep orphans, add retention, and count artifacts toward quota. — high
- MEDIUM — src/smart_http.rs:222 — `allowReachableSHA1InWant` is set for every upload-pack request, not only CI. Each non-tip want triggers a reachability walk. — Set it only for job-token requests. — medium
- LOW — src/auth.rs:282 — `job_user` skips `u.suspended = 0`. — Add it. — high
- LOW — src/ci/config.rs:476,480 and src/ci/runner.rs:508 — The `timeout` multiply can overflow. `image` is unvalidated, so a leading `-` becomes a docker option. — Use `checked_mul`, validate `image`, and pass `--`. — medium
- LOW — src/api/ci.rs:232-254 — SSE streams poll every 500 ms with no per-IP cap, and access is not rechecked. — Cap streams per IP and recheck access. — medium

## lens: edge
- MEDIUM — src/main.rs:18, src/ci/runner.rs:181,627 — The default `GITTER_DATA=./data` is relative. The built-in runner builds `file://./data/repos/N.git`, which git reads as host `.` and path `/data/…`. The relative workspace also breaks `-v` and `GITTER_WORKSPACE`. — Absolutise `data_dir` at startup. — medium
- MEDIUM — src/ci/trigger.rs:35 — The whole ci.yml blob is read with no size cap, and `runs` does this on every request. — Cap it using the size from `stat`. — high
- LOW — src/ci/runner.rs:275-290 — Each output chunk resets the 400 ms flush timer, so steady slow output stays unflushed until 16 KB. — Flush on elapsed time since the last flush. — high
- LOW — static/js/pages/ci.js:191-201 with src/api/ci.rs:226 — EventSource reconnects to the original `?offset=`, so the log is duplicated. — Honour `Last-Event-ID`. — medium
- LOW — src/ci/queue.rs:105, src/ci/trigger.rs:218-239 — `claim` only looks at the first 500 queued jobs. `tick` marks `last_minute` first, so one `?` error skips the later repos that minute. — Pick by label in SQL and handle errors per repo. — medium
- LOW — src/ci/queue.rs:250 — Rerunning a config-failed run ends as "skipped" with the error cleared. — Refuse a rerun when there are no jobs. — high
- LOW — src/ci/jobs.rs:51,73 and src/api/ci.rs:436 — The local backend's log, step and artifact writes are not fenced by attempt. `delete_runner` requeues the built-in runner's jobs even for the built-in id. — Check the attempt, and filter on `builtin = 0`. — medium

## Acceptance criteria
No criteria were given in the brief, so I could not check against them.

## Implicit decisions
- `secret.key` is stored inside backups next to the database, so anyone holding a backup can decrypt the secrets.
- Runs execute whatever `.gitter/ci.yml` says as the pushing owner, and secrets go to every claimed job.
- The runner API lives outside `/api`, so the per-IP rate limit does not apply to it.
- Job tokens are not masked in logs.

## Not checked
- I did not run anything: no tests, no cargo build.
- I did not review tests/, the other JS pages, src/api/wiki, discussions, snippets, admin or repos.
- I did not check the unrelated T-0015/T-0016 hunks, `Cargo.lock`, or the `check_text`/`repo_ids` pre-audit flags.
- The pre-audit secret-looking values (e.g. `remote-secret-123` in scripts/ci-e2e.sh) are test fixtures, and the ui-check logging is intended output. I dismissed both.

**Noticed:** `.claude/worktrees/agent-*` and `.foreman/research` files are in this task's diff, so the worktrees are still being tracked or staged (T-0041 looks incomplete). The Docker container is not removed on server shutdown, and `gitter-job-{id}-{i}` names collide across attempts.
