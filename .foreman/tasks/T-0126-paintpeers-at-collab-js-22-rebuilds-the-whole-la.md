---
id: T-0126
type: FEATURE
tier: M
status: dropped
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-23T01:27:45Z
updated: 2026-10-23T01:28:07Z
---
# PaintPeers` at `collab.js:22` rebuilds the whole layer's `innerHTML` on every keystroke an
## Raw request
> paintPeers` at `collab.js:22` rebuilds the whole layer's `innerHTML` on every keystroke and remote op, which is O(file size) and up to 1M characters
> (noticed by an agent; research T-0022-review-4)
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
- 2026-10-23T01:27:45Z created
- 2026-10-23T01:28:07Z dropped: folded into T-0022: the peer layer repaints once per frame and stays empty when nobody else is here
## Follow-ups captured
