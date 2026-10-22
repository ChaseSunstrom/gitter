---
id: T-0065
type: FEATURE
tier: M
status: dropped
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T02:17:09Z
updated: 2026-10-22T23:07:50Z
---
# Account export and user deletion (T-0033) may not cover the new `reviews` and `review_comm
## Raw request
> account export and user deletion (T-0033) may not cover the new `reviews` and `review_comments` tables
> (noticed by an agent; research T-0049-review-1)
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
- 2026-10-22T02:17:09Z created
- 2026-10-22T23:07:50Z dropped: already covered: the account export writes reviews.json (reviews and line comments the user wrote, src/api/lifecycle.rs ~804), and reviews/review_comments reference users ON DELETE CASCADE, so account deletion removes them
## Follow-ups captured
