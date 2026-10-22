---
id: T-0103
type: FEATURE
tier: L
status: dropped
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T19:39:56Z
updated: 2026-10-22T19:43:54Z
---
# The 2FA `challenge` stays in the `/login?challenge=` URL, in history and Referer, until it
## Raw request
> the 2FA `challenge` stays in the `/login?challenge=` URL, in history and Referer, until it expires. `auth.js` could `replaceState` it away once it is read
> (noticed by an agent; research T-0089-review-3)
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
- 2026-10-22T19:39:56Z created
- 2026-10-22T19:43:54Z dropped: done inside T-0089: auth.js replaces the challenge out of the URL once read
## Follow-ups captured
