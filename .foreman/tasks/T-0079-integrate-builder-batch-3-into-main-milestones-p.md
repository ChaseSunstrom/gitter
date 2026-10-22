---
id: T-0079
type: CLEAN
tier: M
status: planned
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T05:49:31Z
updated: 2026-10-22T05:57:02Z
base: 160ee7ebd7ffd673a338a8ffd294233d59bea233
base_tree: b0927ac64981ef32ee55ad63666e752179ea75bf
plan0_at: 2026-10-22T05:50:26Z
plan0_scope: []
paused_tree: 3580b69b6fe88957469d34a4879e154ea51f4b13
---
# Integrate builder batch 3 into main: milestones, polish, SSH, scanning (resolve conflicts, apply review fixes)
## Raw request
> Integrate builder batch 3 into main: milestones, polish, SSH, scanning (resolve conflicts, apply review fixes)
## Interpretation
Land the four builder branches of batch 3 (T-0050 milestones/templates/assignees, T-0054 polish, T-0077 SSH, T-0019 scanning) on main one at a time: merge main into each lane, renumber its migration after main's, fix conflicts and the reviewer's findings in the lane, run its criteria, then fm lane merge and close it. SSH must also go through branch rules (T-0007) and push scanning (T-0019).
## Assumptions (confidence)
## Acceptance criteria
- [ ] every merged branch's criteria pass on main together — verify with `cargo test -q && bash scripts/e2e.sh && bash scripts/backup-roundtrip.sh`
## Non-goals
## Approach (options → choice → why)
Lane by lane on a refreshed main with fixes in the lane worktree, as in T-0069 [chosen: conflicts stay small and each lane's criteria are rerun on what lands]; (b) one integration branch with all lanes at once (rejected: conflicts compound, harder to bisect).
## Risks and rollback
## Execution prompt
## Steps
1. [ ] merge main into each lane branch in its worktree, fix conflicts and review findings there, then fm lane merge  <- CURRENT
## Resume here
<!-- auto -->
- 2026-10-22T05:57:01Z step 1/1: merge main into each lane branch in its worktree, fix conflicts and review findings there, then fm lane merge
- git: main; 0 uncommitted
## Docs impact
## Verification evidence
## Log
- 2026-10-22T05:49:31Z created
- 2026-10-22T05:50:26Z focused
- 2026-10-22T05:57:02Z paused: focus moved to T-0050
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- T-0069 [CLEAN M, done, 1 steps] Integrate builder batch 2 into main: CLI, insights, MCP server, registry (resolve conflicts, apply review fixes) — lesson: Merge lanes on…
- T-0058 [CLEAN M, done, 1 steps] Integrate builder branches into main: releases, lifecycle, webhooks, signed commits (resolve conflicts, apply review fixes) — lesson: Mer…
- T-0078 [CLEAN M, dropped, 1 steps] Integrate builder batch 3 into main: milestones, polish, SSH, scanning (resolve conflicts, apply review fixes)
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
- similar finished tasks took 1–1 steps
