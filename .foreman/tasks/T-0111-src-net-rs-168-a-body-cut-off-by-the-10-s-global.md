---
id: T-0111
type: FEATURE
tier: M
status: dropped
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T20:51:21Z
updated: 2026-10-22T20:54:41Z
---
# Src/net.rs:168` — a body cut off by the 10 s global timeout becomes a permanent "wasn't JS
## Raw request
> src/net.rs:168` — a body cut off by the 10 s global timeout becomes a permanent "wasn't JSON (or was over 8 MB)" failure in `imports.rs:178`, not a retry
> (noticed by an agent; research T-0090-review-4)
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
- 2026-10-22T20:51:21Z created
- 2026-10-22T20:54:41Z dropped: done in T-0090: a cut-off or non-JSON answer is retried
## Follow-ups captured
