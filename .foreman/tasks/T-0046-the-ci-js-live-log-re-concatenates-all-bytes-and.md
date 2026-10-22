---
id: T-0046
type: FEATURE
tier: M
status: dropped
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-21T23:42:49Z
updated: 2026-10-21T23:54:56Z
---
# The `ci.js` live log re-concatenates all bytes and re-renders every fold on each SSE chunk
## Raw request
> The `ci.js` live log re-concatenates all bytes and re-renders every fold on each SSE chunk, which will be slow near the 16 MB log cap
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
- 2026-10-21T23:54:56Z dropped: fixed in T-0006: chunks joined lazily, redraws throttled to 5/s
## Follow-ups captured
