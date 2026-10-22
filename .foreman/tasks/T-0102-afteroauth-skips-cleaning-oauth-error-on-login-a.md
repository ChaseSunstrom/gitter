---
id: T-0102
type: FEATURE
tier: L
status: dropped
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T19:36:36Z
updated: 2026-10-22T19:36:57Z
---
# AfterOAuth()` skips cleaning `oauth_error` on `/login` and `/signup`, so the error stays i
## Raw request
> afterOAuth()` skips cleaning `oauth_error` on `/login` and `/signup`, so the error stays in the URL there and reappears on reload
> (noticed by an agent; research T-0089-review-4)
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
- 2026-10-22T19:36:36Z created
- 2026-10-22T19:36:57Z dropped: done inside T-0089: auth.js strips oauth_error from the URL after showing it
## Follow-ups captured
