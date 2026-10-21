---
id: T-0030
type: FEATURE
tier: M
status: captured
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-21T20:55:44Z
updated: 2026-10-21T20:55:44Z
---
# Run_capped` has no `kill_on_drop` visible, so on the 30s timeout path (git.rs:462-467) the
## Raw request
> run_capped` has no `kill_on_drop` visible, so on the 30s timeout path (git.rs:462-467) the child may be left running. Check whether `command()` sets `kill_on_drop
> (noticed by an agent; research T-0029-review)
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
- 2026-10-21T20:55:44Z created
## Follow-ups captured
