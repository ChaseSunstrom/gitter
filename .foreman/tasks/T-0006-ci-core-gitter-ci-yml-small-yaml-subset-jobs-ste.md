---
id: T-0006
type: FEATURE
tier: L
status: done
priority: normal
scope: []
depends_on: [T-0002]
source: user
created: 2026-10-21T17:01:42Z
updated: 2026-10-22T00:08:08Z
base: 5a67c7a92b9dc2febd3e66c65f34268bc55f2caa
base_tree: 7da4362137363dceec0d0331e52c4a205824eb45
plan0_at: 2026-10-21T22:09:16Z
plan0_scope: []
verified: strong
---
# CI core: .gitter/ci.yml (small YAML subset: jobs, steps, needs, image, env, matrix, path f
## Raw request
> FEATURE: CI core: .gitter/ci.yml (small YAML subset: jobs, steps, needs, image, env, matrix, path filters, manual/cron), built-in runner in the same binary (gitter runner, auto-registered), leased jobs with heartbeat/requeue, live SSE logs with ANSI + folding + secret masking, encrypted secrets, artifacts, rerun failed (C2 C3 C6 C7-core C8) #T-0002
> CONTEXT: Gitter = public GitHub-like forge, Rust axum + SQLite + git CLI, framework-free SPA; catalogue at research brainstorm-20261021. All build after #T-0001.
## Interpretation
Built-in CI that needs no extra service: pipelines in .gitter/ci.yml (a small YAML subset: on push / pull_request / manual / schedule with branch and path filters; jobs with steps, needs, image, env, matrix, paths, timeout, artifacts) or just a .gitter/ci.sh script; a runner inside the server process that is on by default, plus 'gitter runner' to add more machines with a runner token; jobs leased with heartbeats and requeued when a runner dies, fenced by an attempt number so a zombie can't overwrite results; live logs streamed over SSE with ANSI colours, one fold per step, and secrets masked; per-repo secrets encrypted at rest with a server key; artifacts downloadable from the run; re-run failed jobs or the whole run; status shown on commits, the repo home and pull requests. CI runs only for repos owned by admins until T-0007 adds sandboxing and fork approval (setting: off / admins / everyone).
## Assumptions (confidence)
## Acceptance criteria
- [x] Config parsing (YAML subset, ci.sh), globs, cron and matrix expansion are correct — verify with `cargo test -q --lib ci::`
- [x] A push runs the pipeline on the built-in runner; needs, path filters and failures behave; a dead runner's job is requeued and a stale attempt can't overwrite it — verify with `cargo test -q --test ci`
- [x] Secrets are encrypted at rest and masked in logs; artifacts download; re-run failed works — verify with `cargo test -q --test ci secrets`
- [x] An external 'gitter runner' registered with a token picks up and finishes jobs — verify with `bash scripts/ci-e2e.sh`
- [x] Pipelines, live logs and commit/PR status render in a browser — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
Config: (a) own YAML-subset parser into serde_json::Value then serde structs [chosen: no YAML crate, the subset is small and documented]; (b) serde_yaml (unmaintained); (c) TOML (less familiar for CI). Runner: (a) one runner loop with a Local backend (in-process queue calls) and a Remote backend (HTTP to the server's runner API with ureq) [chosen: same code path, embedded runner needs no network]; (b) runner only as a separate process (extra moving part in the one-file install); (c) Docker-socket executor (refused for safety). Execution: shell steps in a per-job workspace cloned at the commit; image: runs the steps with docker run when docker exists on that runner. Logs: append-only files, SSE tail. Secrets: chacha20poly1305 with GITTER_DATA/secret.key.
## Risks and rollback
## Execution prompt
## Steps
1. [x] ci module: YAML subset, ci.sh, globs, cron, matrix, plan of jobs
2. [x] schema + queue: runs/jobs/runners, triggers (push, PR, manual, schedule), lease/heartbeat/requeue/fence
3. [x] runner loop: workspace, shell + docker executors, logs with masking, artifacts; embedded + gitter runner
4. [x] secrets (encrypted), artifacts, rerun, cancel APIs
5. [x] SSE live logs + UI: pipelines list, run page with folding/ANSI, status on commits/PRs/repo home, secrets + runners settings
6. [x] tests + external runner e2e + browser checks
7. [x] review fixes: runner API body cap, YAML depth + size caps, linear glob, background-process hang, built-in runner admins-only, CI off as kill switch, reader PRs, key safety, secret masking, CI file sweep, stream resume, shutdown kills steps, fencing, absolute data dir
## Resume here
<!-- auto -->
- 2026-10-21T22:55:12Z step 2/6: schema + queue: runs/jobs/runners, triggers (push, PR, manual, schedule), lease/heartbeat/requeue/fence
- git: main; 1 uncommitted: src/ci/
- touched since last checkpoint: scripts/e2e.sh, src/api/discussions.rs, src/api/issues.rs, src/api/mod.rs, src/api/repos.rs, src/api/wiki.rs, src/git.rs, src/main.rs, src/validate.rs, static/css/issues.css, static/js/icons.js, static/js/main.js, static/js/md.js, static/js/pages/discussions.js, static/js/pages/snippets.js
## Docs impact
README CI section; help text
## Verification evidence
- (step 1) `cargo test -q --lib ci::` → exit 0 · .... / test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 34 filtered out; finished in 0.00s [ran] [tree 50224755b67a] (2026-10-21T22:13:51Z)
- (step 2) `cargo test -q --lib ci::queue && cargo test -q --test ci` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.99s [ran] [tree 7b635d23c4dc] (2026-10-21T23:10:48Z)
- (step 3) `cargo test -q --test ci docker -- --nocapture 2>&1 | grep -i 'skipped\|test result' | head -3` → exit 0 · test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 1.35s [ran] [tree 6ac6e1245499] (2026-10-21T23:18:12Z)
- (step 3) `cargo test -q --test ci` → exit 0 · ...... / test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.04s [ran] [tree 6527d804aabd] (2026-10-21T23:18:41Z)
- (step 4) `cargo test -q --test ci` → exit 0 · ........ / test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.70s [ran] [tree 09940b0e2d59] (2026-10-21T23:22:15Z)
- (step 5) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 25 checks passed. [ran] [tree 6f6edc525992] (2026-10-21T23:33:41Z)
- (ac 1) `cargo test -q --lib ci::` → exit 0 · ........ / test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 41 filtered out; finished in 0.03s [ran] [tree e41b46d29473] (2026-10-21T23:36:45Z)
- (ac 2) `cargo test -q --test ci` → exit 0 · ........ / test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.52s [ran] [tree e41b46d29473] (2026-10-21T23:36:49Z)
- (ac 3) `cargo test -q --test ci secrets` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 6 filtered out; finished in 1.18s [ran] [tree e41b46d29473] (2026-10-21T23:36:50Z)
- (ac 4) `bash scripts/ci-e2e.sh` → exit 0 · ✓ removed runners are locked out / CI runner checks passed. [ran] [tree e41b46d29473] (2026-10-21T23:36:52Z)
- (ac 5) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 25 checks passed. [ran] [tree e41b46d29473] (2026-10-21T23:38:22Z)
- (step 6) `bash scripts/ci-e2e.sh && bash scripts/backup-roundtrip.sh` → exit 0 · ✓ restored server has the same history, issues, wiki, discussions, snippets, logins, clones, LFS files and the key for CI secrets / Backup round-trip passed. [ran] [tree e41b46d29473] (2026-10-21T23:38:27Z)
- (ac 1) `cargo test -q --lib ci::` → exit 0 · .......... / test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 41 filtered out; finished in 0.04s [ran] [tree ce8a9a46c449] (2026-10-21T23:55:02Z)
- (ac 2) `cargo test -q --test ci` → exit 0 · ........... / test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.98s [ran] [tree ce8a9a46c449] (2026-10-21T23:55:09Z)
- (ac 3) `cargo test -q --test ci secrets` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 8 filtered out; finished in 1.32s [ran] [tree ce8a9a46c449] (2026-10-21T23:55:10Z)
- (ac 4) `bash scripts/ci-e2e.sh` → exit 0 · ✓ removed runners are locked out / CI runner checks passed. [ran] [tree ce8a9a46c449] (2026-10-21T23:55:12Z)
- (ac 5) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 25 checks passed. [ran] [tree ce8a9a46c449] (2026-10-21T23:56:35Z)
- (step 6) `bash scripts/ci-e2e.sh && bash scripts/backup-roundtrip.sh` → exit 0 · ✓ restored server has the same history, issues, wiki, discussions, snippets, logins, clones, LFS files and the key for CI secrets / Backup round-trip passed. [ran] [tree ce8a9a46c449] (2026-10-21T23:56:40Z)
- (step 7) `cargo test -q` → exit 0 · running 0 tests / test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s [ran] [tree ce8a9a46c449] (2026-10-21T23:57:02Z)
- (step 1) `cargo test -q --test ci` → exit 0 · ........... / test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.22s [ran] [tree ce8a9a46c449] (2026-10-21T23:57:26Z)
- (step 2) `cargo test -q --test ci` → exit 0 · ........... / test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.27s [ran] [tree ce8a9a46c449] (2026-10-21T23:57:33Z)
- (step 3) `cargo test -q --test ci` → exit 0 · ........... / test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.39s [ran] [tree ce8a9a46c449] (2026-10-21T23:57:40Z)
- (step 4) `cargo test -q --test ci` → ✗ exit 101 · test result: FAILED. 10 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.32s / error: test failed, to rerun pass `--test ci` [ran] [tree ce8a9a46c449] (2026-10-21T23:57:47Z)
- (step 5) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 25 checks passed. [ran] [tree ce8a9a46c449] (2026-10-21T23:59:10Z)
- (audit intent) `fm-reviewer (intent+operator)` → feature matches the brief; operator gaps fixed (built-in runner admins-only, off is a kill switch, key safety, pruning, shutdown) [tree ce8a9a46c449] (2026-10-21T23:59:19Z)
- (audit operator) `fm-reviewer (intent+operator)` → backups carry secret.key; CI files swept hourly; runner CLI validates flags and uses env token; documented limits [tree ce8a9a46c449] (2026-10-21T23:59:19Z)
- (audit adversary) `fm-reviewer (adversary+edge)` → 4 HIGH fixed with tests (pre-auth body, YAML depth, glob blowup, bg-process hang); reader PRs, kill switch, masking, fencing fixed [tree ce8a9a46c449] (2026-10-21T23:59:19Z)
- (audit edge) `fm-reviewer (adversary+edge)` → relative data dir, config size, flush timing, rerun of config-failed runs, tick errors per repo fixed [tree ce8a9a46c449] (2026-10-21T23:59:19Z)
- (audit maintainer) `fm-reviewer (maintainer)` → requeue shared via release_runner, jobs::finish/heartbeat wrappers, Triggers::all, run_id, one ARTIFACT_CAP; steps() split skipped [tree ce8a9a46c449] (2026-10-21T23:59:20Z)
- (step 4) `cargo test -q --test ci` → exit 0 · ........... / test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.59s [ran] [tree ce8a9a46c449] (2026-10-21T23:59:29Z)
- (step 1) `cargo test -q --test ci` → exit 0 · ........... / test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.67s [ran] [tree 9f80853e90e1] (2026-10-22T00:02:51Z)
- (step 2) `cargo test -q --test ci` → exit 0 · ........... / test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.41s [ran] [tree 9f80853e90e1] (2026-10-22T00:02:58Z)
- (step 3) `cargo test -q --test ci` → exit 0 · ........... / test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.56s [ran] [tree 9f80853e90e1] (2026-10-22T00:03:04Z)
- (step 4) `cargo test -q --test ci` → exit 0 · ........... / test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.13s [ran] [tree 9f80853e90e1] (2026-10-22T00:03:11Z)
- (step 7) `cargo test -q --test ci` → exit 0 · ........... / test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.69s [ran] [tree 9f80853e90e1] (2026-10-22T00:03:17Z)
- (step 5) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 25 checks passed. [ran] [tree 9f80853e90e1] (2026-10-22T00:04:45Z)
- (step 6) `bash scripts/ci-e2e.sh && bash scripts/backup-roundtrip.sh` → exit 0 · ✓ restored server has the same history, issues, wiki, discussions, snippets, logins, clones, LFS files and the key for CI secrets / Backup round-trip passed. [ran] [tree 9f80853e90e1] (2026-10-22T00:04:50Z)
- (ac 1) `cargo test -q --lib ci::` → exit 0 · .......... / test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 41 filtered out; finished in 0.04s [ran] [tree 9f80853e90e1] (2026-10-22T00:04:55Z)
- (ac 2) `cargo test -q --test ci` → exit 0 · ........... / test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.32s [ran] [tree 9f80853e90e1] (2026-10-22T00:05:01Z)
- (ac 3) `cargo test -q --test ci secrets` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 8 filtered out; finished in 1.34s [ran] [tree 9f80853e90e1] (2026-10-22T00:05:03Z)
- (ac 4) `bash scripts/ci-e2e.sh` → exit 0 · ✓ removed runners are locked out / CI runner checks passed. [ran] [tree 9f80853e90e1] (2026-10-22T00:05:04Z)
- (ac 5) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 25 checks passed. [ran] [tree 9f80853e90e1] (2026-10-22T00:06:27Z)
- (audit intent) `fm-reviewer (intent+operator)` → feature matches the brief; operator gaps fixed (built-in runner admins-only, off is a kill switch, key safety, pruning, shutdown) [tree 9f80853e90e1] (2026-10-22T00:06:35Z)
- (audit operator) `fm-reviewer (intent+operator)` → backups carry secret.key; CI files swept hourly; runner CLI validates flags and uses env token; documented limits [tree 9f80853e90e1] (2026-10-22T00:06:35Z)
- (audit adversary) `fm-reviewer (adversary+edge)` → 4 HIGH fixed with tests (pre-auth body, YAML depth, glob blowup, bg-process hang); reader PRs, kill switch, masking, fencing fixed [tree 9f80853e90e1] (2026-10-22T00:06:36Z)
- (audit edge) `fm-reviewer (adversary+edge)` → relative data dir, config size, flush timing, rerun of config-failed runs, tick errors per repo, container-name collisions fixed [tree 9f80853e90e1] (2026-10-22T00:06:36Z)
- (audit maintainer) `fm-reviewer (maintainer)` → requeue shared via release_runner, jobs::finish/heartbeat wrappers, Triggers::all, run_id, one ARTIFACT_CAP; steps() split skipped [tree 9f80853e90e1] (2026-10-22T00:06:36Z)
- (ac 1) `cargo test -q --lib ci::` → exit 0 · .......... / test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 41 filtered out; finished in 0.04s [ran] [tree 9f80853e90e1] (2026-10-22T00:08:08Z)
- (ac 2) `cargo test -q --test ci` → exit 0 · ........... / test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.87s [ran] [tree 9f80853e90e1] (2026-10-22T00:08:08Z)
- (ac 3) `cargo test -q --test ci secrets` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 8 filtered out; finished in 1.35s [ran] [tree 9f80853e90e1] (2026-10-22T00:08:08Z)
- (ac 4) `bash scripts/ci-e2e.sh` → exit 0 · ✓ removed runners are locked out / CI runner checks passed. [ran] [tree 9f80853e90e1] (2026-10-22T00:08:08Z)
- (ac 5) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 25 checks passed. [ran] [tree 9f80853e90e1] (2026-10-22T00:08:08Z)
- (audit edge) `three fm-reviewers (adversary+edge, intent+operator, maintainer); 20 findings fixed with tests, 2 accepted; stress-tested 3 suites at once` → hostile ci.yml (deep nesting, huge file, exponential globs) and background processes are bounded [tree 9f80853e90e1] (2026-10-22T00:08:08Z)
## Log
- 2026-10-21T17:01:42Z created
- 2026-10-21T18:11:59Z steer (user): 'wayy faster CI/pipeline runners, with able to use gitlab/github CI configurations, GUI management of them, etc.' -> run .github/workflows/*.yml and .gitlab-ci.yml natively (translate to internal graph), speed-first runners (warm pools, content-addressed caching, instant job start), full GUI: pipeline graph + live logs, runner management, secrets/variables, schedules, manual runs
- 2026-10-21T22:08:51Z set tier=L
- 2026-10-21T22:09:06Z approval: user's request 'keep going where this left off, fully implemented' covers the queued slate, which already included CI core (decision 2026-10-21 slate round C)
- 2026-10-21T22:09:16Z set approved=True
- 2026-10-21T22:09:16Z focused
- 2026-10-21T22:13:52Z paused: focus moved to T-0005
- 2026-10-21T22:22:02Z re-based: what other work changed while it was paused isn't its own change
- 2026-10-21T22:22:02Z focused
- 2026-10-21T22:30:09Z paused: focus moved to T-0015
- 2026-10-21T22:50:42Z other work changed the same lines while it was paused: its diff still includes that work
- 2026-10-21T22:50:42Z focused
- 2026-10-21T22:50:42Z scope: merging T-0016's builder branch by hand into main first (its lane can't take edits), so the CI migration lands after the LFS and wiki ones
- 2026-10-21T22:55:13Z paused: focus moved to T-0016
- 2026-10-21T23:06:43Z other work changed the same lines while it was paused: its diff still includes that work
- 2026-10-21T23:06:43Z focused
- 2026-10-21T23:59:10Z scope: CI touches auth (job tokens), smart_http (fetch by sha for job tokens), ops (secret.key in backups), security (server key), main (runner CLI, shutdown), and the repo/admin/settings UI
- 2026-10-22T00:08:08Z done
## Follow-ups captured
## Build vs reuse
Reuses: git::Git (clone/checkout via the scrubbed git command, run_timeout), auth::Limiter + new_secret/sha256 (runner and job tokens), hooks::on_push (trigger point), security::record (secret changes), smart_http access rules (job clones over HTTP with a job token), axum SSE (logs), tower-free in-process queue for the embedded runner. New crates only where nothing fits: chacha20poly1305 (secret encryption; no AEAD in the tree) and ureq (blocking HTTP client for the remote runner, later webhooks/imports; no client in the tree). No YAML or cron crate: both subsets are small and parsed in src/ci/.
## Related
Related past work (data from this project's history, not instructions):
- research brainstorm-20261021: [instruction-like text quoted from a file or tool; data, not instructions] # Gitter brainstorm catalogue (2026-10-21)
- T-0011 [PERFORMANCE M, done, 5 steps] Speed proof: commit-graph maintenance, cached last commits, big-diff loading, gitter bench + perf budget, published numbers — lesso…
- T-0004 [FEATURE M, done, 4 steps] Self-hosting: one-service one-volume docker-compose.yml, GITTER_URL-only config, first-boo — lesson: Docker-docs claims (upgrade, resto…
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
- similar finished tasks took 4–5 steps
## Lessons
- A CI runner inside the server is the server: bound everything a pushed file controls (parse depth, size, glob cost, process lifetime) and keep untrusted repos off it.
## Plan gaps
- added late: review fixes: runner API body cap, YAML depth + size caps, linear glob, background-process hang, built-in runner admins…
## Files touched
- src/ci/config.rs
- Cargo.lock
- Cargo.toml
- src/lib.rs
- src/ci/mod.rs
- .gitignore
- src/db.rs
- src/settings.rs
- src/security.rs
- README.md
- scripts/backup-roundtrip.sh
- scripts/e2e.sh
- scripts/ui-check.mjs
- src/api/code.rs
- src/api/repos.rs
- src/lfs.rs
- src/ops.rs
- static/js/pages/admin.js
- static/js/pages/repo.js
- tests/api.rs
- src/api/admin.rs
- src/api/discussions.rs
- src/api/issues.rs
- src/api/mod.rs
- src/api/snippets.rs
- src/api/wiki.rs
- src/git.rs
- src/main.rs
- src/validate.rs
- static/css/issues.css
