---
id: T-0006
type: FEATURE
tier: L
status: planned
priority: normal
scope: []
depends_on: [T-0002]
source: user
created: 2026-10-21T17:01:42Z
updated: 2026-10-21T22:30:09Z
base: 5a67c7a92b9dc2febd3e66c65f34268bc55f2caa
base_tree: 7da4362137363dceec0d0331e52c4a205824eb45
plan0_at: 2026-10-21T22:09:16Z
plan0_scope: []
paused_tree: 5efaabf356c276a5b05fb98ebcfa26bfd76767b3
---
# CI core: .gitter/ci.yml (small YAML subset: jobs, steps, needs, image, env, matrix, path f
## Raw request
> FEATURE: CI core: .gitter/ci.yml (small YAML subset: jobs, steps, needs, image, env, matrix, path filters, manual/cron), built-in runner in the same binary (gitter runner, auto-registered), leased jobs with heartbeat/requeue, live SSE logs with ANSI + folding + secret masking, encrypted secrets, artifacts, rerun failed (C2 C3 C6 C7-core C8) #T-0002
> CONTEXT: Gitter = public GitHub-like forge, Rust axum + SQLite + git CLI, framework-free SPA; catalogue at research brainstorm-20261021. All build after #T-0001.
## Interpretation
Built-in CI that needs no extra service: pipelines in .gitter/ci.yml (a small YAML subset: on push / pull_request / manual / schedule with branch and path filters; jobs with steps, needs, image, env, matrix, paths, timeout, artifacts) or just a .gitter/ci.sh script; a runner inside the server process that is on by default, plus 'gitter runner' to add more machines with a runner token; jobs leased with heartbeats and requeued when a runner dies, fenced by an attempt number so a zombie can't overwrite results; live logs streamed over SSE with ANSI colours, one fold per step, and secrets masked; per-repo secrets encrypted at rest with a server key; artifacts downloadable from the run; re-run failed jobs or the whole run; status shown on commits, the repo home and pull requests. CI runs only for repos owned by admins until T-0007 adds sandboxing and fork approval (setting: off / admins / everyone).
## Assumptions (confidence)
## Acceptance criteria
- [ ] Config parsing (YAML subset, ci.sh), globs, cron and matrix expansion are correct — verify with `cargo test -q --lib ci::`
- [ ] A push runs the pipeline on the built-in runner; needs, path filters and failures behave; a dead runner's job is requeued and a stale attempt can't overwrite it — verify with `cargo test -q --test ci`
- [ ] Secrets are encrypted at rest and masked in logs; artifacts download; re-run failed works — verify with `cargo test -q --test ci secrets`
- [ ] An external 'gitter runner' registered with a token picks up and finishes jobs — verify with `bash scripts/ci-e2e.sh`
- [ ] Pipelines, live logs and commit/PR status render in a browser — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
Config: (a) own YAML-subset parser into serde_json::Value then serde structs [chosen: no YAML crate, the subset is small and documented]; (b) serde_yaml (unmaintained); (c) TOML (less familiar for CI). Runner: (a) one runner loop with a Local backend (in-process queue calls) and a Remote backend (HTTP to the server's runner API with ureq) [chosen: same code path, embedded runner needs no network]; (b) runner only as a separate process (extra moving part in the one-file install); (c) Docker-socket executor (refused for safety). Execution: shell steps in a per-job workspace cloned at the commit; image: runs the steps with docker run when docker exists on that runner. Logs: append-only files, SSE tail. Secrets: chacha20poly1305 with GITTER_DATA/secret.key.
## Risks and rollback
## Execution prompt
## Steps
1. [x] ci module: YAML subset, ci.sh, globs, cron, matrix, plan of jobs
2. [ ] schema + queue: runs/jobs/runners, triggers (push, PR, manual, schedule), lease/heartbeat/requeue/fence  <- CURRENT
3. [ ] runner loop: workspace, shell + docker executors, logs with masking, artifacts; embedded + gitter runner
4. [ ] secrets (encrypted), artifacts, rerun, cancel APIs
5. [ ] SSE live logs + UI: pipelines list, run page with folding/ANSI, status on commits/PRs/repo home, secrets + runners settings
6. [ ] tests + external runner e2e + browser checks
## Resume here
<!-- auto -->
- 2026-10-21T22:25:25Z step 2/6: schema + queue: runs/jobs/runners, triggers (push, PR, manual, schedule), lease/heartbeat/requeue/fence
- git: main; 16 uncommitted: README.md, scripts/backup-roundtrip.sh, scripts/e2e.sh, scripts/ui-check.mjs, src/api/code.rs, src/api/repos.rs, src/db.rs, src/lfs.rs, src/lib.rs, src/ops.rs
- touched since last checkpoint: README.md, scripts/backup-roundtrip.sh, scripts/e2e.sh, scripts/ui-check.mjs, src/api/code.rs, src/api/repos.rs, src/lfs.rs, src/lib.rs, src/ops.rs, src/settings.rs, static/js/pages/repo.js, src/db.rs, static/js/pages/admin.js, tests/api.rs
## Docs impact
## Verification evidence
- (step 1) `cargo test -q --lib ci::` → exit 0 · .... / test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 34 filtered out; finished in 0.00s [ran] [tree 50224755b67a] (2026-10-21T22:13:51Z)
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
