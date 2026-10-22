---
id: T-0047
type: FEATURE
tier: M
status: dropped
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-21T23:47:05Z
updated: 2026-10-21T23:54:56Z
---
# Claude/worktrees/agent-*` and `.foreman/research` files are in this task's diff, so the wo
## Raw request
> claude/worktrees/agent-*` and `.foreman/research` files are in this task's diff, so the worktrees are still being tracked or staged (T-0041 looks incomplete). The Docker container is not removed on s…
> (noticed by an agent; research T-0006-review-1)
## Interpretation
## Assumptions (confidence)
## Acceptance criteria
## Non-goals
## Approach (options → choice → why)
## Risks and rollback
## Execution prompt
## Steps
## Resume here
## Docs impact
## Verification evidence
## Log
- 2026-10-21T23:47:05Z created
- 2026-10-21T23:54:56Z dropped: worktrees untracked in 7a0d2e4 (.gitignore); containers are removed on shutdown via SHUTDOWN and names include the attempt
## Follow-ups captured
