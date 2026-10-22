---
id: T-0079
type: CLEAN
tier: M
status: done
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T05:49:31Z
updated: 2026-10-22T15:20:16Z
base: 160ee7ebd7ffd673a338a8ffd294233d59bea233
base_tree: b0927ac64981ef32ee55ad63666e752179ea75bf
plan0_at: 2026-10-22T05:50:26Z
plan0_scope: []
verified: strong
---
# Integrate builder batch 3 into main: milestones, polish, SSH, scanning (resolve conflicts, apply review fixes)
## Raw request
> Integrate builder batch 3 into main: milestones, polish, SSH, scanning (resolve conflicts, apply review fixes)
## Interpretation
Land the four builder branches of batch 3 (T-0050 milestones/templates/assignees, T-0054 polish, T-0077 SSH, T-0019 scanning) on main one at a time: merge main into each lane, renumber its migration after main's, fix conflicts and the reviewer's findings in the lane, run its criteria, then fm lane merge and close it. SSH must also go through branch rules (T-0007) and push scanning (T-0019).
## Assumptions (confidence)
## Acceptance criteria
- [x] every merged branch's criteria pass on main together — verify with `cargo test -q && bash scripts/e2e.sh && bash scripts/backup-roundtrip.sh`
## Non-goals
## Approach (options → choice → why)
Lane by lane on a refreshed main with fixes in the lane worktree, as in T-0069 [chosen: conflicts stay small and each lane's criteria are rerun on what lands]; (b) one integration branch with all lanes at once (rejected: conflicts compound, harder to bisect).
## Risks and rollback
## Execution prompt
## Steps
1. [x] merge main into each lane branch in its worktree, fix conflicts and review findings there, then fm lane merge
## Resume here
<!-- auto -->
- 2026-10-22T15:00:51Z step 1/1: merge main into each lane branch in its worktree, fix conflicts and review findings there, then fm lane merge
- git: main; 0 uncommitted
## Docs impact
README sections updated in each lane merge
## Verification evidence
- (ac 1) `cargo test -q 2>&1 | grep -E 'test result|FAILED' && bash scripts/e2e.sh 2>&1 | tail -1 && bash scripts/backup-roundtrip.sh 2>&1 | tail -1` → exit 0 · All 42 checks passed. / Backup round-trip passed. [ran] [tree d1109834611a] (2026-10-22T15:14:10Z)
- (step 1) `git log --oneline --merges -12 | grep -c 'Merge T-00\(50\|54\|77\|19\):'` → exit 0 · 4 [ran] [tree d1109834611a] (2026-10-22T15:15:48Z)
- (ac 1) `cargo test -q && bash scripts/e2e.sh && bash scripts/backup-roundtrip.sh` → exit 0 · ✓ restored server has the same history, issues, wiki, discussions, snippets, trash, logins, clones, LFS files, release files, images and the key for CI secrets / Backup round-trip passed. [ran] [tree d1109834611a] (2026-10-22T15:20:15Z)
- (audit intent) `each lane reviewed by fm-reviewer and fixed in its worktree before landing (T-0050, T-0054, T-0077, T-0019); full gates on main after the last merge` → all four batch-3 branches landed; cargo test (gate), e2e 42/42 and backup round trip pass together [tree d1109834611a] (2026-10-22T15:20:15Z)
- (audit edge) `each lane reviewed by fm-reviewer and fixed in its worktree before landing (T-0050, T-0054, T-0077, T-0019); full gates on main after the last merge` → migrations v19 trust, v20 milestones, v21 auto-merge pin, v22 SSH keys, v23 scanning in order; one shared pre-receive hook for rules and scanning [tree d1109834611a] (2026-10-22T15:20:15Z)
- (audit adversary) `each lane reviewed by fm-reviewer and fixed in its worktree before landing (T-0050, T-0054, T-0077, T-0019); full gates on main after the last merge` → SSH goes through the same transfer command as HTTPS, so branch rules and push protection apply to both [tree d1109834611a] (2026-10-22T15:20:15Z)
## Log
- 2026-10-22T05:49:31Z created
- 2026-10-22T05:50:26Z focused
- 2026-10-22T05:57:02Z paused: focus moved to T-0050
- 2026-10-22T13:54:59Z other work changed the same lines while it was paused: its diff still includes that work
- 2026-10-22T13:54:59Z focused
- 2026-10-22T14:07:29Z paused: focus moved to T-0054
- 2026-10-22T14:13:28Z focused
- 2026-10-22T14:33:46Z paused: focus moved to T-0077
- 2026-10-22T14:39:40Z focused
- 2026-10-22T15:00:52Z paused: focus moved to T-0019
- 2026-10-22T15:09:58Z focused
- 2026-10-22T15:20:16Z done
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- T-0069 [CLEAN M, done, 1 steps] Integrate builder batch 2 into main: CLI, insights, MCP server, registry (resolve conflicts, apply review fixes) — lesson: Merge lanes on…
- T-0058 [CLEAN M, done, 1 steps] Integrate builder branches into main: releases, lifecycle, webhooks, signed commits (resolve conflicts, apply review fixes) — lesson: Mer…
- T-0078 [CLEAN M, dropped, 1 steps] Integrate builder batch 3 into main: milestones, polish, SSH, scanning (resolve conflicts, apply review fixes)
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
- similar finished tasks took 1–1 steps
## Lessons
- Land lanes that touch the same path (push handling) in dependency order, and move shared policy into the shared helper before the second lane lands
## Files touched
- Cargo.lock
- Cargo.toml
- Dockerfile
- README.md
- scripts/e2e.sh
- scripts/ui-check.mjs
- src/api/account.rs
- src/api/checks.rs
- src/api/ci.rs
- src/api/code.rs
- src/api/issues.rs
- src/api/lifecycle.rs
- src/api/milestones.rs
- src/api/mod.rs
- src/api/repos.rs
- src/api/revert.rs
- src/api/reviews.rs
- src/api/scanning.rs
- src/ci/config.rs
- src/ci/runner.rs
- src/ci/trigger.rs
- src/db.rs
- src/demo.rs
- src/deps.rs
- src/error.rs
- src/git.rs
- src/lib.rs
- src/main.rs
- src/protect.rs
- src/scan.rs
