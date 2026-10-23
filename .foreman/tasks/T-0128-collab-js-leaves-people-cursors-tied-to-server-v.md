---
id: T-0128
type: FEATURE
tier: M
status: dropped
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-23T01:29:42Z
updated: 2026-10-23T01:29:48Z
---
# Collab.js` leaves `people` cursors tied to server versions while local unconfirmed ops shi
## Raw request
> collab.js` leaves `people` cursors tied to server versions while local unconfirmed ops shift positions, so cursors drift briefly while typing. Worth its own task
> (noticed by an agent; research T-0022-review-3)
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
- 2026-10-23T01:29:42Z created
- 2026-10-23T01:29:48Z dropped: folded into T-0022: local edits now move others' cursors and their reported cursors are mapped over our unconfirmed changes
## Follow-ups captured
