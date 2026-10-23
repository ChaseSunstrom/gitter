---
id: T-0115
type: FEATURE
tier: L
status: dropped
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T21:52:46Z
updated: 2026-10-22T23:18:34Z
---
# UI shows the "Allow edits" checkbox to a PR author who no longer has write access to the f
## Raw request
> UI shows the "Allow edits" checkbox to a PR author who no longer has write access to the fork, so toggling it only produces a 403 toast (static/js/pages/issue.js:226)
> (noticed by an agent; research T-0051-review-4)
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
- 2026-10-22T21:52:46Z created
- 2026-10-22T23:18:34Z dropped: fixed in T-0085: the PR JSON has can_allow_edits (author who can push to the fork) and the page shows the toggle only then
## Follow-ups captured
