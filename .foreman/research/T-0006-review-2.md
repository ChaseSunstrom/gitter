**Verdict: changes needed.** The feature matches the request and the README. The operator-facing gaps are the unbounded built-in runner access, a kill switch that doesn't stop everything, and an incomplete restore and cleanup story.

The diff base 7da4362 predates the LFS, wiki, discussions and snippets merges, so about 60% of the diff is not T-0006. I reviewed only the CI parts.

## lens: operator
- HIGH — src/ci/runner.rs:627 — The built-in runner is on by default (2 jobs) and runs steps as the server's OS user, in a workspace at `data/ci/work/job-N`. A collaborator with write access to an admin's repo can read the whole data folder. Reproducer: push a ci.yml step `cat "$GITTER_WORKSPACE/../../../secret.key" "$GITTER_WORKSPACE/../../../gitter.db" | base64`. That gives the key and decrypts every repo's secrets. README:751 and admin.js:7440 warn only that jobs "run as the runner's user". — State that the built-in runner is the server itself. Warn in the admin UI when `ci=everyone` and `builtin_runner_jobs>0`. Consider defaulting the built-in runner to 0 until an admin opts in. — high
- MEDIUM — src/api/ci.rs:1343 — The "Off" setting is not a kill switch. `rerun` never calls `trigger::enabled`, and `queue::claim` ignores `ci`. With `ci=off`, a writer can still re-run a run, and already-queued jobs still execute. Reproducer: finish a run, set `ci=off`, POST `/ci/runs/1/rerun`; it returns 200 and the job runs. — Check `enabled()` in `rerun`, and skip claiming (or cancel queued jobs) when CI is off. — high
- MEDIUM — src/security.rs:6982 — `server_key` recurses forever when `secret.key` exists but isn't 32 bytes. `create_new` hits AlreadyExists, which calls `server_key` again. A disk-full write after creation leaves a 0-byte file, and every later start overflows the stack. Reproducer: `: > data/secret.key; gitter`. — On a wrong length, return an error or panic with a clear message; don't recurse. — high
- MEDIUM — src/ci/jobs.rs:43 — A lost or regenerated `secret.key` silently drops every secret (`filter_map` on `open`). Jobs run with empty `$DEPLOY_KEY` while the settings page still lists the secret. — Count the failures and write a warning line to the job log (or fail the job). — high
- MEDIUM — src/api/ci.rs:1400 — The log stream ends after 30 minutes (3600 × 500 ms), and `EventSource` then reconnects to the same URL with the original `offset=`. The server ignores `Last-Event-ID`, so the whole log is appended again; ci.js:7686 does the same on any network drop. Reproducer: a job with `timeout: 40m`, or kill the connection mid-run. — Read `Last-Event-ID` as the offset (the event ids are already absolute), or rebuild the `EventSource` from `bytes.length` on error. — high
- MEDIUM — src/api/repos.rs:2302 and src/api/admin.rs:1170 — Deleting a repo or user cascades the `ci_*` rows but leaves `ci/logs/<job>.log` and `ci/artifacts/<job>.tar.gz` on disk. Nothing prunes them, and artifacts can be 1 GB each. — Delete the files for the repo's jobs when it's removed, and add a retention sweep. — high
- MEDIUM — src/ops.rs:6549 — Backups include `secret.key` but not `ci/`. After a restore, finished runs still show an Artifacts button (the `ci_artifacts` rows survive) that returns 404, and their logs are empty. README:760 says backup "includes all of them" and doesn't mention this. — Document it, or back up `ci/artifacts` and `ci/logs`. — high
- MEDIUM — src/ci/runner.rs:318 and src/main.rs:6379 — `gitter runner` has no signal handling, and steps run in their own process groups (`process_group(0)`). Ctrl-C or SIGTERM leaves steps and docker containers running. The server requeues the job after 90 s, so a deploy step can run twice at the same time. — Handle SIGINT/SIGTERM by setting `stop` and killing the groups and containers. — medium
- LOW — src/ci/runner.rs:114 and src/ci/runner.rs:51 — Remote HTTP calls have a 120 s global timeout, so a large artifact upload (the cap is 1 GB) over a slow link fails. — Use a longer or no timeout for the artifacts PUT. — medium
- LOW — src/main.rs:6380 — A mistyped `--labels`, or `--jobs abc`, is silently ignored (the runner then gets no labels or 1 job). `--token` shows in `ps`, and README:746 and admin.js:7463 both put it on the command line. — Reject unknown flags and bad numbers, and show `GITTER_RUNNER_TOKEN` in the copied command. — high
- LOW — src/api/ci.rs:1617 — `delete_runner` requeues running jobs for the built-in runner's id before the `builtin = 0` guard, then returns 404. — Run the UPDATE only if the lookup found a row. — high
- LOW — static/js/pages/reposettings.js:8117 — The `pattern="[A-Z_]..."` attribute blocks lowercase input, so the `.toUpperCase()` on line 8135 never gets to help. — Drop the pattern or the uppercase call. — high

## Acceptance criteria
I ran nothing; all of this is from reading the code.
- Config parsing, globs, cron and matrix: not verifiable here. The tests exist, and defaults agree with README:737.
- A push runs the pipeline, a dead runner's job is requeued, and a stale attempt can't overwrite it: partly met. The remote path is fenced (http.rs:63). The local path is not: `append_log`, `step` and `store_artifact` take no token, so a cancelled attempt can write into a re-run job for up to 20 s.
- Secrets encrypted and masked, artifacts, re-run failed: met. Masking is by exact substring, so a multi-line secret is only masked if the whole value appears intact in one send.
- External runner: met by code. https via ureq/rustls was not checked.
- Browser rendering of pipelines, logs and statuses: met. The code is present for the CI tab, the run view, and status badges on the repo page, commit page and PR merge box.

## Implicit decisions
- `uploadpack.allowReachableSHA1InWant=true` now applies to all smart-HTTP fetches (smart_http.rs).
- The binary now bundles an HTTP client and TLS stack (ureq, rustls) for the runner subcommand.
- Job tokens (`gcj_`) act as read-only users for their repo.
- CI "admins" means repos owned by an admin, not pushes made by one.
- The tab bar gains a CI tab on every repo.

## Not checked
config.rs parsing details, whether fork pull requests receive secrets, tests/*, ci.css, and the non-CI hunks (LFS, wiki, discussions, snippets).

**Noticed:** Local-backend log, step and artifact writes aren't fenced by the job token, which contradicts the stale-attempt criterion.
**Noticed:** The `ci.js` live log re-concatenates all bytes and re-renders every fold on each SSE chunk, which will be slow near the 16 MB log cap.
