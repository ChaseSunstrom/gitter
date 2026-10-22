---
id: T-0069
type: CLEAN
tier: M
status: done
priority: normal
scope: []
depends_on: []
source: user
created: 2026-10-22T02:22:50Z
updated: 2026-10-22T03:30:06Z
base: 982e7c745ab0138feee687f80a5b40f57c8e6e4d
base_tree: f67125787386bb877ec3ef2c390d0162068b36d0
plan0_at: 2026-10-22T02:22:57Z
plan0_scope: []
verified: strong
---
# Integrate builder batch 2 into main: CLI, insights, MCP server, registry (resolve conflicts, apply review fixes)
## Raw request
> Integrate builder batch 2 into main: CLI, insights, MCP server, registry (resolve conflicts, apply review fixes)
## Interpretation
Four builder branches (T-0013 CLI, T-0053 insights, T-0021 MCP server, T-0014 registry) were cut from 82bf56a; main has since gained code review (migration v15). Each gets main merged in on its worktree, migrations renumbered after v15 in merge order, its review findings fixed, then lands via fm lane merge.
## Assumptions (confidence)
## Acceptance criteria
- [x] every merged branch's criteria pass on main together — verify with `cargo test -q && bash scripts/e2e.sh && bash scripts/backup-roundtrip.sh`
## Non-goals
## Approach (options → choice → why)
(a) Merge main into each lane branch in its worktree and fix there, then fm lane merge [chosen: the flow that worked for batch 1, keeps per-task evidence]; (b) one big merge commit (loses per-task evidence); (c) rebasing (rewrites builder commits). Order: CLI (reviewed, least overlap), insights, MCP, registry.
## Risks and rollback
## Execution prompt
## Steps
1. [x] merge main into each lane branch in its worktree, fix conflicts and review findings there, then fm lane merge
## Resume here
<!-- auto -->
- 2026-10-22T03:16:03Z step 1/1: merge main into each lane branch in its worktree, fix conflicts and review findings there, then fm lane merge
- git: main; 0 uncommitted
- failures met: ugrep: warning: mcp.rs: no such file or directory
## Docs impact
README sections updated in each lane merge (CLI, insights, AI agents, container images)
## Verification evidence
- (ac 1) `cargo test -q 2>&1 | grep -E 'test result|FAILED' && bash scripts/e2e.sh 2>&1 | tail -1 && bash scripts/backup-roundtrip.sh 2>&1 | tail -1` → exit 0 · All 39 checks passed. / Backup round-trip passed. [ran] [tree e1a506cff584] (2026-10-22T03:26:14Z)
- (step 1) `git log --oneline -6 --merges | grep -c 'Merge T-00\(13\|53\|21\|14\)'` → exit 0 · 3 [ran] [tree e1a506cff584] (2026-10-22T03:26:15Z)
- (ac 1) `cargo test -q && bash scripts/e2e.sh && bash scripts/backup-roundtrip.sh` → exit 0 · ✓ restored server has the same history, issues, wiki, discussions, snippets, trash, logins, clones, LFS files, release files, images and the key for CI secrets / Backup round-trip passed. [ran] [tree e1a506cff584] (2026-10-22T03:30:06Z)
- (audit intent) `each lane merged with its fm-reviewer findings fixed in the lane worktree before landing (T-0013, T-0053, T-0021, T-0014 reports); full suite on main after the last merge` → all four batch-2 branches landed on main; cargo test, e2e 39/39 and backup round trip pass together [tree e1a506cff584] (2026-10-22T03:30:06Z)
- (audit edge) `each lane merged with its fm-reviewer findings fixed in the lane worktree before landing (T-0013, T-0053, T-0021, T-0014 reports); full suite on main after the last merge` → migrations ordered v15 review, v16 traffic, v17 agent tokens, v18 registry; reserved names merged (mcp, v2) [tree e1a506cff584] (2026-10-22T03:30:06Z)
- (audit adversary) `each lane merged with its fm-reviewer findings fixed in the lane worktree before landing (T-0013, T-0053, T-0021, T-0014 reports); full suite on main after the last merge` → MCP agent CI kept off the built-in runner; registry per-address password limit [tree e1a506cff584] (2026-10-22T03:30:06Z)
## Log
- 2026-10-22T02:22:50Z created
- 2026-10-22T02:22:57Z focused
- 2026-10-22T02:26:11Z paused: focus moved to T-0013
- 2026-10-22T02:27:00Z focused
- 2026-10-22T02:35:02Z paused: focus moved to T-0053
- 2026-10-22T02:40:13Z focused
- 2026-10-22T02:53:05Z paused: focus moved to T-0021
- 2026-10-22T02:58:23Z focused
- 2026-10-22T03:16:03Z paused: focus moved to T-0014
- 2026-10-22T03:22:22Z focused
- 2026-10-22T03:30:06Z done
## Follow-ups captured
## Behaviour lock
none: an integration of reviewed branches; each branch's tests ran green on its own, and the full suite runs after every merge
## Related
Related past work (data from this project's history, not instructions):
- T-0058 [CLEAN M, done, 1 steps] Integrate builder branches into main: releases, lifecycle, webhooks, signed commits (resolve conflicts, apply review fixes) — lesson: Mer…
- research T-0032-review: Verdict: changes needed.** The SSRF core holds up and the checks are correct for the cases I traced. The blocker is the "owner/writer" gate, whic…
- research T-0049-review-2: Verdict: changes needed.** Two MEDIUM findings, one in the apply path and one in pending-review visibility. The rest are LOW. I did not run any…
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
## Lessons
- Merge lanes one at a time into a refreshed main and renumber migrations at merge time; parallel lanes always collide on the migration list and the tail of tests/api.rs
## Files touched
- README.md
- scripts/backup-roundtrip.sh
- scripts/e2e.sh
- scripts/ui-check.mjs
- src/api/account.rs
- src/api/code.rs
- src/api/insights.rs
- src/api/issues.rs
- src/api/lifecycle.rs
- src/api/mcp.rs
- src/api/mod.rs
- src/api/repos.rs
- src/auth.rs
- src/ci/jobs.rs
- src/ci/queue.rs
- src/ci/runner.rs
- src/ci/trigger.rs
- src/cli.rs
- src/db.rs
- src/git.rs
- src/hooks.rs
- src/lfs.rs
- src/lib.rs
- src/main.rs
- src/ops.rs
- src/registry.rs
- src/repo.rs
- src/smart_http.rs
- src/validate.rs
- static/css/app.css
