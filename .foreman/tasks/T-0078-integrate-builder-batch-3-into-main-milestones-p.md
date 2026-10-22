---
id: T-0078
type: CLEAN
tier: M
status: dropped
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T04:13:21Z
updated: 2026-10-22T05:49:30Z
---
# Integrate builder batch 3 into main: milestones, polish, SSH, scanning (resolve conflicts, apply review fixes)
## Raw request
> Integrate builder batch 3 into main: milestones, polish, SSH, scanning (resolve conflicts, apply review fixes)
## Interpretation
## Assumptions (confidence)
## Acceptance criteria
- [ ] every merged branch's criteria pass on main together — verify with `cargo test -q && bash scripts/e2e.sh && bash scripts/backup-roundtrip.sh`
## Non-goals
## Approach (options → choice → why)
## Risks and rollback
## Execution prompt
## Steps
1. [ ] merge main into each lane branch in its worktree, fix conflicts and review findings there, then fm lane merge  <- CURRENT
## Resume here
## Docs impact
## Verification evidence
## Log
- 2026-10-22T04:13:21Z created
- 2026-10-22T05:49:30Z dropped: recreated with its interpretation and approach
## Follow-ups captured
