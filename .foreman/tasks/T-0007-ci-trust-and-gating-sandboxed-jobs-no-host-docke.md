---
id: T-0007
type: FEATURE
tier: M
status: done
priority: normal
scope: []
depends_on: []
source: user
created: 2026-10-21T17:01:42Z
updated: 2026-10-22T13:54:18Z
group: CI gating and pipeline features
base: e941324bfaef92031a9b57f3c06b9cd62e17b538
base_tree: e1a506cff584757c25291d3de4d8f9d943854d4e
plan0_at: 2026-10-22T03:35:31Z
plan0_scope: []
verified: strong
---
# CI trust and gating: sandboxed jobs (no host docker socket, cpu/mem/time limits, network p
## Raw request
> FEATURE: CI trust and gating: sandboxed jobs (no host docker socket, cpu/mem/time limits, network policy), fork-PR approval, commit/PR status checks, branch protection with required checks, auto-merge, status badges + unfurl cards (C4 C5 C40)
> CONTEXT: Gitter = public GitHub-like forge, Rust axum + SQLite + git CLI, framework-free SPA; catalogue at research brainstorm-20261021. All build after #T-0001.
## Interpretation
Make CI safe to offer and useful for gating merges. Jobs run sandboxed: in a container with no host Docker socket, CPU, memory, process and time limits, a read-only root with a writable work dir, and a network policy (on by default, off per job or site-wide). The built-in runner only takes jobs whose run was triggered by someone with admin on an admin-trusted repo (fixing the gap T-0008 found where write collaborators' pushes ran on the host); other runs wait for a remote runner. Pull requests from people without write (and from forks) wait for approval by a writer before their run starts, and never get secrets. Commit statuses: external systems post statuses (context, state, description, URL) to a commit, shown next to CI runs. Branch protection per branch pattern: required status checks, required approving reviews (once T-0049 lands, until then a count of zero), no force-push, no deletion, admins can bypass or not. Auto-merge: a PR merges itself once checks pass. Status badges (SVG per branch and workflow) and link unfurl cards (Open Graph tags on repo, issue and PR pages).
## Assumptions (confidence)
## Acceptance criteria
- [x] Jobs run with resource limits and no host socket; fork and non-writer PR runs wait for approval and get no secrets; the built-in runner only runs runs triggered by admins — verify with `cargo test -q --test ci trust`
- [x] Commit statuses post and list; branch protection blocks force-push, deletion and merges without required checks, over git and the web; auto-merge merges when checks pass — verify with `cargo test -q --test api protection`
- [x] Badges render as SVG, pages carry Open Graph tags, the settings pages for rules and approvals render — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
(a) Sandbox by always running steps in docker with --cpus, --memory, --pids-limit, --read-only plus a tmpfs, --cap-drop ALL, --security-opt no-new-privileges, and --network none when the policy says so; shell steps without docker are refused unless the admin opts in [chosen: docker is already the runner's executor]; (b) bubblewrap/nsjail (another dependency, Linux only). Statuses in a commit_statuses table keyed by repo, sha, context; protection in a branch_rules table checked in the receive-pack pre-receive path (src/hooks.rs) and in the merge API, so git pushes and web merges follow one rule. Auto-merge is a flag on the pull that the CI finish path and status posts re-check.
## Risks and rollback
## Execution prompt
## Steps
1. [x] sandboxed docker steps: limits, read-only root, cap-drop, no-new-privileges, network policy; shell steps only when an admin allows; tests
2. [x] trust: runs record who triggered them; the built-in runner and secrets need an admin trigger on a trusted repo; approval gate for fork and non-writer PRs
3. [x] commit statuses API and display beside CI runs
4. [x] branch protection rules (patterns, required checks, no force-push or delete, admin bypass) enforced in the pre-receive hook and the merge API; settings UI
5. [x] auto-merge flag re-checked when runs finish or statuses arrive; badges SVG; Open Graph unfurl tags; e2e + README
## Resume here
<!-- auto -->
- 2026-10-22T05:50:25Z steps 5/5 done
- git: main; 0 uncommitted
## Docs impact
README CI: runners trust, sandbox, approval, branch rules/checks/auto-merge/badges; AI agents section
## Verification evidence
- (step 1) `cargo test -q --test ci trust_job_containers` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 11 filtered out; finished in 1.33s [ran] [tree 7d58282ec7cb] (2026-10-22T03:37:35Z)
- (step 2) `cargo test -q --test ci trust && cargo test -q --test api mcp_agent_ci` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 68 filtered out; finished in 0.61s [ran] [tree e627f26e580c] (2026-10-22T03:41:38Z)
- (step 3) `cargo test -q --test api protection_merges` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 70 filtered out; finished in 2.86s [ran] [tree db81c712b9b5] (2026-10-22T03:56:09Z)
- (step 4) `cargo test -q --test api protection_rules` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 70 filtered out; finished in 10.16s [ran] [tree db81c712b9b5] (2026-10-22T03:56:20Z)
- (step 5) `bash scripts/e2e.sh 2>&1 | tail -1` → exit 0 · All 39 checks passed. [ran] [tree b0927ac64981] (2026-10-22T04:12:27Z)
- (ac 3) `bash scripts/e2e.sh 2>&1 | tail -2` → exit 0 · ✓ UI pages render without console errors / All 39 checks passed. [ran] [tree 49229fa7e876] (2026-10-22T13:51:53Z)
- (ac 2) `cargo test -q --test api protection` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 69 filtered out; finished in 8.40s [ran] [tree 49229fa7e876] (2026-10-22T13:52:02Z)
- (ac 1) `cargo test -q --test ci trust` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 11 filtered out; finished in 0.93s [ran] [tree 49229fa7e876] (2026-10-22T13:54:18Z)
- (ac 2) `cargo test -q --test api protection` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 69 filtered out; finished in 4.85s [ran] [tree 49229fa7e876] (2026-10-22T13:54:18Z)
- (ac 3) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 39 checks passed. [ran] [tree 49229fa7e876] (2026-10-22T13:54:18Z)
- (audit intent) `fm-reviewer five-lens review plus the automated security review; 2 HIGH and 4 MEDIUM fixed with tests (auto-merge pinned to the armed head, skipped-by-needs fails, statuses can't mask CI, approvals per head, PR runs credited to the branch pusher, hook path fixed at startup and fail-closed); concurrent-push attribution captured as T-0080` → all three criteria pass on main: ci trust 2/2 (real docker sandbox), api protection 2/2 (real git push through the hook), e2e 39/39 [tree 49229fa7e876] (2026-10-22T13:54:18Z)
- (audit adversary) `fm-reviewer five-lens review plus the automated security review; 2 HIGH and 4 MEDIUM fixed with tests (auto-merge pinned to the armed head, skipped-by-needs fails, statuses can't mask CI, approvals per head, PR runs credited to the branch pusher, hook path fixed at startup and fail-closed); concurrent-push attribution captured as T-0080` → built-in runner only runs admin-started code (push, merge, Run, schedule by last pusher, PR by head pusher); agent runs wait for a person; rules hold on git, web, deletion and merge [tree 49229fa7e876] (2026-10-22T13:54:18Z)
- (audit edge) `fm-reviewer five-lens review plus the automated security review; 2 HIGH and 4 MEDIUM fixed with tests (auto-merge pinned to the armed head, skipped-by-needs fails, statuses can't mask CI, approvals per head, PR runs credited to the branch pusher, hook path fixed at startup and fail-closed); concurrent-push attribution captured as T-0080` → exact branch matching, stale approvals, armed auto-merge lapses on others' pushes, rules env bounded, link race [tree 49229fa7e876] (2026-10-22T13:54:18Z)
- (audit operator) `fm-reviewer five-lens review plus the automated security review; 2 HIGH and 4 MEDIUM fixed with tests (auto-merge pinned to the armed head, skipped-by-needs fails, statuses can't mask CI, approvals per head, PR runs credited to the branch pusher, hook path fixed at startup and fail-closed); concurrent-push attribution captured as T-0080` → bypassed merges and rule changes audited; v21 seeds branch pushers from push history; admin limits in settings [tree 49229fa7e876] (2026-10-22T13:54:18Z)
## Log
- 2026-10-21T17:01:42Z created
- 2026-10-22T00:39:12Z from T-0008 review: write collaborators (and org members with write) on an admin-owned repo can push .gitter/ci.yml that runs on the built-in host runner with the repo's secrets. Documented for now; T-0007 should gate built-in-runner jobs and secrets by the pusher's role (or need approval for non-admin pushers)
- 2026-10-22T03:35:31Z focused
- 2026-10-22T05:50:26Z paused: focus moved to T-0079
- 2026-10-22T13:44:13Z other work changed the same lines while it was paused: its diff still includes that work
- 2026-10-22T13:44:13Z focused
- 2026-10-22T13:54:18Z done
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research brainstorm-20261021: [instruction-like text quoted from a file or tool; data, not instructions] # Gitter brainstorm catalogue (2026-10-21)
- T-0006 [FEATURE L, done, 7 steps] CI core: .gitter/ci.yml (small YAML subset: jobs, steps, needs, image, env, matrix, path f — lesson: A CI runner inside the server is t…
- research T-0006-review-2: Verdict: changes needed.** The feature matches the request and the README. The operator-facing gaps are the unbounded built-in runner access, a…
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
- similar past tasks were L: consider --tier L
## Lessons
- Trust must follow who wrote the code, not who clicked: record the last pusher per branch and attribute runs, schedules and PR runs to it; pin automatic actions to the exact commit they were approved for
## Files touched
- src/ci/config.rs
- src/ci/queue.rs
- src/cli.rs
- src/ci/jobs.rs
- src/ci/runner.rs
- src/settings.rs
- static/js/pages/admin.js
- tests/ci.rs
- src/api/repos.rs
- src/db.rs
- src/hooks.rs
- src/ci/trigger.rs
- src/api/ci.rs
- src/api/mod.rs
- static/js/pages/ci.js
- tests/api.rs
- README.md
- src/protect.rs
- src/auth.rs
- src/lib.rs
- src/main.rs
- src/repo.rs
- src/api/issues.rs
- src/api/code.rs
- src/smart_http.rs
- src/api/reviews.rs
- src/api/checks.rs
- tests/common/mod.rs
- static/js/lib.js
- static/js/pages/reposettings.js
