---
id: T-0018
type: FEATURE
tier: L
status: done
priority: normal
scope: []
depends_on: []
source: followup
created: 2026-10-21T17:01:52Z
updated: 2026-10-22T23:03:13Z
inferred_deps: [T-0007]
inferred_why: T-0007: per-PR previews depend on the CI status pipeline
base: 6e1ef85012b118f0c31812348206b1ebc277b4cf
base_tree: f33bc7eef8ec3e3883ad8e7a3e322250ec4c6692
plan0_at: 2026-10-22T22:12:39Z
plan0_scope: []
verified: strong
---
# Deployments and environments: CI jobs deploy to named environments (with their own secrets and branch rules), deployments are recorded with status and URL, and pull requests get preview URLs served by Gitter
## Raw request
> Deployments, environments, per-PR preview URLs (C28)
## Interpretation
GitHub/GitLab-style deployments. An environment (production, staging, preview/pr-7) belongs to a repository and has an optional URL, a branch rule (only matching branches' jobs may deploy there) and its own secrets. A deployment is a record: an environment, ref and sha, who or which CI job made it, a status (queued, in_progress, success, failure, inactive) and a URL. CI jobs declare environment: (a name, or name and url); the deployment follows the job. A pull request job with preview: DIR has DIR (from its artifacts) served by Gitter at an unguessable /preview/<token>/ URL, sandboxed so its scripts can't act as the viewer, shown on the pull request and taken down when it closes.
## Assumptions (confidence)
## Acceptance criteria
- [x] environments and deployments: list, create, update status over the API; a job with environment: records a deployment that follows the job (in progress → success/failure, the previous one inactive) — verify with `cargo test -q --test deploy`
- [x] environment secrets reach only jobs that target that environment; an environment's branch rule keeps other branches' jobs out (the job fails with why, without its secrets) — verify with `cargo test -q --test deploy`
- [x] a pull request job with preview: DIR publishes DIR at a preview URL (unguessable, sandboxed, no cookies), shown on the pull request; closing or merging takes it down — verify with `cargo test -q --test deploy`
- [x] Deployments page lists environments with their latest deployment and history; settings manage environments; the pull request page shows its preview and deployments — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
Records: (a) environments + deployments tables with one status per deployment, settled from CI's claim/finish [chosen: the two hooks every job passes through]; (b) a deployment_statuses history table like GitHub (more rows, no reader needs the history yet; captured); (c) deployments as CI run metadata only (no API for external deploy tools). Previews: (a) same host, path /preview/<token>/, CSP sandbox allow-scripts allow-forms allow-popups (opaque origin: no cookies, no Gitter API as the viewer), ACAO * so its own fetches work, token = [REDACTED] so private repos' previews load without cookies [chosen: works with no DNS setup]; (b) a separate preview origin with wildcard DNS (best isolation, needs setup; captured as an option for later); (c) serve inside the artifacts tarball per request (slow, no index). Secrets: environment_secrets table sealed like ci_secrets, added to the lease at claim only for jobs whose environment passed its branch rule.
## Risks and rollback
Risk: preview pages running scripts as the viewer. Mitigation: served with CSP sandbox (no allow-same-origin), so they get an opaque origin with no cookies, storage or same-origin API. Session cookies are SameSite=Lax and writes check Origin. Risk: environment secrets leaking. Mitigation: they are handed out only after the environment's branch rule passes, never to agent runs, and CI never runs fork code by itself; rule and environment changes need Sudo and are audited. Risk: disk. Previews are capped at 1 GB each (like artifacts), one per pull request, removed on close or merge and after 30 days; they don't count toward quotas (T-0118). Rollback: migration v32 only adds tables, so reverting the code leaves them unused, and ci/previews can be deleted.
## Execution prompt
## Steps
1. [x] schema (environments, deployments, environment_secrets) and the API: environments CRUD (admin), deployments list/create/status (write), README
2. [x] CI: environment: on jobs (name and url), deployment created at claim and settled at finish, environment secrets, branch rules
3. [x] previews: preview: DIR on a job, extracted from its artifacts after success, served at /preview/<token>/ with a sandbox CSP, taken down on close/merge and by the hourly sweep
4. [x] UI: Deployments page, environment settings (branches, url, secrets), pull request deployments box with the preview link; ui-check; e2e
## Resume here
## Docs impact
README: the CI bullet and a Deployments and previews paragraph (environment:, preview:, branch rules, environment secrets, sandboxing, limits, API); settings link; 'preview' reserved
## Verification evidence
- (step 1) `cargo test -q --test deploy` → red as expected (exit 101) · test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.73s / error: test failed, to rerun pass `--test deploy` [ran] [tree c11f693cbf1e] (2026-10-22T22:15:51Z)
- (step 1) `cargo test -q --test deploy` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.74s [ran] [tree 247aa6568bcf] (2026-10-22T22:18:40Z)
- (step 2) `cargo test -q --test deploy ci_jobs_deploy` → red as expected (exit 101) · test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.71s / error: test failed, to rerun pass `--test deploy` [ran] [tree bc5d2e1a8111] (2026-10-22T22:22:12Z)
- (ac 1) `cargo test -q --test deploy 2>&1 | grep -E "panicked|test result|^error" -A6 | head -16` → exit 0 · test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.80s [ran] [tree 69147f2a8bc8] (2026-10-22T22:23:14Z)
- (ac 2) `cargo test -q --test deploy 2>&1 | grep -E "panicked|test result|^error" -A6 | head -16` → exit 0 · test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.80s [ran] [tree 69147f2a8bc8] (2026-10-22T22:23:14Z)
- (ac 3) `cargo test -q --test deploy 2>&1 | grep -E "panicked|test result|^error" -A6 | head -16` → exit 0 · test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.80s [ran] [tree 69147f2a8bc8] (2026-10-22T22:23:14Z)
- (step 3) `cargo test -q --test deploy 2>&1 | grep -E "panicked|test result|^error" -A6 | head -16` → exit 0 · test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.80s [ran] [tree 69147f2a8bc8] (2026-10-22T22:23:14Z)
- (step 2) `cargo test -q --test deploy ci_jobs_deploy` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 1.10s [ran] [tree e8a49edd84e6] (2026-10-22T22:24:45Z)
- (step 3) `cargo test -q --test deploy previews` → red as expected (exit 101) · test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.74s / error: test failed, to rerun pass `--test deploy` [ran] [tree 92dd3ec88985] (2026-10-22T22:25:58Z)
- (step 3) `cargo test -q --test deploy previews` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.84s [ran] [tree 8733ef0d74be] (2026-10-22T22:28:06Z)
- (audit intent) `fm-reviewer x3 (split by lens) + mutation checks (14 mutants killed)` → met: environments and deployments over the API and from CI jobs; environment secrets and branch rules; PR previews published, replaced, and taken down on close, merge (also by hand) and after 30 days; Deployments page, PR box, settings link [tree 85597f7e4c16] (2026-10-22T22:56:27Z)
- (audit adversary) `fm-reviewer x3 (split by lens) + mutation checks (14 mutants killed) + background security review` → previews sandboxed (opaque origin, ACAO *, no cookies) with traversal, symlink and hidden-repo checks; secrets only after the branch rule and never to agent runs (fork code never runs CI); rule and environment changes need Sudo and are audited; links http(s) only; the PR-secrets review finding doesn't apply (runs come only from this repository's branches) [tree 85597f7e4c16] (2026-10-22T22:56:27Z)
- (audit edge) `fm-reviewer x3 (split by lens) + mutation checks (14 mutants killed)` → PR closed or merged mid-build, merge-queue runs, reruns of closed PRs, out-of-order successes, an oversized preview, symlinks, a sweep mid-publish, GitLab stop jobs, the 100-environment cap [tree 85597f7e4c16] (2026-10-22T22:56:28Z)
- (audit operator) `fm-reviewer x3 (split by lens) + mutation checks (14 mutants killed)` → migration v32 adds tables only; previews in GITTER_DATA/ci/previews, swept hourly; 'preview' reserved (doctor check noted on T-0075); storage quota exclusion captured as T-0118 [tree 85597f7e4c16] (2026-10-22T22:56:28Z)
- (audit maintainer) `fm-reviewer x3 (split by lens) + mutation checks (14 mutants killed)` → one deploy module (records, CI hooks, API) and one preview module (publish, serve, sweep); shared validate::web_link; claim loop re-indented; tests per criterion plus unit tests for rules, names and extraction [tree 85597f7e4c16] (2026-10-22T22:56:28Z)
- (ac 1) `cargo test -q --test deploy environments_and_deployments_over_the_api` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 0.87s [ran] [tree 85597f7e4c16] (2026-10-22T22:56:35Z)
- (ac 2) `cargo test -q --test deploy ci_jobs_deploy` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 0.57s [ran] [tree 85597f7e4c16] (2026-10-22T22:56:36Z)
- (ac 3) `cargo test -q --test deploy previews` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 0.72s [ran] [tree 85597f7e4c16] (2026-10-22T22:56:37Z)
- (step 1) `cargo test -q --test deploy` → exit 0 · ..... / test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 9.04s [ran] [tree 85597f7e4c16] (2026-10-22T22:56:46Z)
- (step 2) `cargo test -q --test deploy ci_jobs_deploy` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 0.99s [ran] [tree 85597f7e4c16] (2026-10-22T22:56:48Z)
- (step 3) `cargo test -q --test deploy previews` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 1.10s [ran] [tree 85597f7e4c16] (2026-10-22T22:56:50Z)
- (ac 4) `bash scripts/e2e.sh > /tmp/claude-1000/-home-chase-projects-gitter/2e122f62-61e6-4fe5-b944-f5546ee8dfc9/scratchpad/e2e-t0018c.log 2>&1; s=$?; grep -E 'ok deployments|✗' /tmp/claude-1000/-home-chase-projects-gitter/2e122f62-61e6-4fe5-b944-f5546ee8dfc9/scratchpad/e2e-t0018c.log | tail -3; exit $s` → exit 0 · ok deployments: listed production and the preview, the pull request shows its preview, made staging [ran] [tree 85597f7e4c16] (2026-10-22T22:59:25Z)
- (step 4) `node --check static/js/pages/deployments.js && node --check static/js/pages/issue.js && grep -q 'ok deployments' /tmp/claude-1000/-home-chase-projects-gitter/2e122f62-61e6-4fe5-b944-f5546ee8dfc9/scratchpad/e2e-t0018c.log` → exit 0 · (no output) [ran] [tree 85597f7e4c16] (2026-10-22T22:59:41Z)
- (ac 1) `cargo test -q --test deploy` → exit 0 · ..... / test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 9.42s [ran] [tree 85597f7e4c16] (2026-10-22T23:03:12Z)
- (ac 2) `cargo test -q --test deploy` → exit 0 · ..... / test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 9.42s [ran] [tree 85597f7e4c16] (2026-10-22T23:03:12Z)
- (ac 3) `cargo test -q --test deploy` → exit 0 · ..... / test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 9.42s [ran] [tree 85597f7e4c16] (2026-10-22T23:03:12Z)
- (ac 4) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 45 checks passed. [ran] [tree 85597f7e4c16] (2026-10-22T23:03:12Z)
## Log
- 2026-10-21T17:01:52Z created
- 2026-10-22T22:12:18Z planned from capture
- 2026-10-22T22:12:38Z approved: the user asked to implement every queued feature ("what about all of the other features you have queued?"); L task proceeds under that standing request
- 2026-10-22T22:12:38Z set approved=True
- 2026-10-22T22:12:39Z focused
- 2026-10-22T22:33:56Z security review (env secrets to untrusted PR runs): not applicable as written. Every CI run's ref is a branch of the repository itself: push runs come from writers, manual runs need write and take refs/heads only, and PR runs exist only for from_fork = 0 (ci/trigger.rs). Fork code never runs automatically, and agent runs get no secrets, the same rule as for repository secrets. Kept the empty rule as 'any branch of this repository'.
- 2026-10-22T23:03:12Z done
- 2026-10-22T23:03:13Z committed 17ffacf (35 files)
## Follow-ups captured
## Build vs reuse
Reuses CI's claim/finish hooks, the sealed-secret helpers (security::seal/open), the artifacts tarball the runner already uploads (previews are extracted from it), protect.rs glob matching for branch rules, and the hourly CI sweep for cleanup. No new crate (tar/flate2 are already used for artifacts).
## Related
Related past work (data from this project's history, not instructions):
- research brainstorm-20261021: [instruction-like text quoted from a file or tool; data, not instructions] # Gitter brainstorm catalogue (2026-10-21)
- T-0097 [FEATURE L, done, 5 steps] FEATURE: run CI from GitHub Actions (.github/workflows/*.yml) and GitLab CI (.gitlab-ci.ym — lesson: A translator from foreign config i…
- research T-0097-review: Verdict: changes needed.** Static read only: I ran no code. The "secret-looking value" pre-audit hit is a false positive. `compat.rs:1108` assert…
- T-0007 [FEATURE M, done, 5 steps] CI trust and gating: sandboxed jobs (no host docker socket, cpu/mem/time limits, network p — lesson: Trust must follow who wrote the co…
- similar finished tasks took 5–5 steps
## Lessons
- Anything published on behalf of a pull request has to be checked against the pull request's state at the moment it goes live, not only when the job starts: closes, merges (including by hand) and reruns all happen while jobs run
## Files touched
- src/db.rs
- tests/deploy.rs
- src/api/deploy.rs
- src/api/ci.rs
- src/api/preview.rs
- src/ci/config.rs
- src/ci/jobs.rs
- src/ci/compat.rs
- src/api/issues.rs
- src/lib.rs
- src/validate.rs
- static/js/pages/auth.js
- static/js/pages/deployments.js
- static/js/main.js
- static/js/pages/issue.js
- static/js/repohead.js
- scripts/e2e.sh
- scripts/ui-check.mjs
- README.md
- static/js/pages/reposettings.js
- src/api/mod.rs
