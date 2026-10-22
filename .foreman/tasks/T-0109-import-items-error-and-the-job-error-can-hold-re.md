---
id: T-0109
type: FEATURE
tier: M
status: dropped
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T20:41:53Z
updated: 2026-10-22T20:54:41Z
---
# Import_items.error` and the job `error` can hold remote-controlled text (already redacted
## Raw request
> import_items.error` and the job `error` can hold remote-controlled text (already redacted and stripped of control characters), so the UI relies on `html` escaping
> (noticed by an agent; research T-0090-review-1)
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
- 2026-10-22T20:41:53Z created
- 2026-10-22T20:54:41Z dropped: by design: remote text is redacted, stripped of control characters and rendered through the escaping html template
## Follow-ups captured
