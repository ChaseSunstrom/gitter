---
id: T-0045
type: FEATURE
tier: M
status: dropped
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-21T23:42:49Z
updated: 2026-10-21T23:54:55Z
---
# Local-backend log, step and artifact writes aren't fenced by the job token, which contradi
## Raw request
> Local-backend log, step and artifact writes aren't fenced by the job token, which contradicts the stale-attempt criterion
> (noticed by an agent; research T-0006-review-2)
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
- 2026-10-21T23:42:49Z created
- 2026-10-21T23:54:55Z dropped: fixed in T-0006: Local backend checks jobs::holds before log, step and artifact writes
## Follow-ups captured
