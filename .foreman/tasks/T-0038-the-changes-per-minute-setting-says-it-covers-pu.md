---
id: T-0038
type: FEATURE
tier: L
status: dropped
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-21T22:13:51Z
updated: 2026-10-21T23:39:30Z
---
# The "Changes per minute" setting says it covers pushes, but `rate_limit` only wraps `/api`
## Raw request
> the "Changes per minute" setting says it covers pushes, but `rate_limit` only wraps `/api`. Git pushes are limited only by `TRANSFER_STARTS_PER_MIN` per IP
> (noticed by an agent; research T-0005-review)
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
- 2026-10-21T22:13:51Z created
- 2026-10-21T23:39:30Z dropped: already fixed: smart_http counts pushes toward write:{uid} (src/smart_http.rs:187-192)
## Follow-ups captured
