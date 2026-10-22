---
id: T-0043
type: FEATURE
tier: L
status: dropped
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-21T23:41:18Z
updated: 2026-10-21T23:54:55Z
---
# Can_start_manually` (src/api/ci.rs:68) does a git stat, blob read and YAML parse on every
## Raw request
> can_start_manually` (src/api/ci.rs:68) does a git stat, blob read and YAML parse on every `/ci/runs` list request; cache it per sha
> (noticed by an agent; research T-0006-review-3)
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
- 2026-10-21T23:41:18Z created
- 2026-10-21T23:54:55Z dropped: now only writers trigger it (can_run needs enabled + can_write); parse is capped at 256 KB
## Follow-ups captured
