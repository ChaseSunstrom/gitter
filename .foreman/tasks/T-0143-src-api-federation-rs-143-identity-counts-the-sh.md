---
id: T-0143
type: FEATURE
tier: L
status: dropped
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-23T03:54:58Z
updated: 2026-10-23T03:55:29Z
---
# Src/api/federation.rs:143` — `identity()` counts the shared 120/min lookup limit before th
## Raw request
> src/api/federation.rs:143` — `identity()` counts the shared 120/min lookup limit before the cache check, so first-seen servers compete; this is worth its own look
> (noticed by an agent; research T-0134-review)
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
- 2026-10-23T03:54:58Z created
- 2026-10-23T03:55:29Z dropped: not so: identity() returns a cached server before the shared lookup budget is counted; only first-seen (or forced fresh) lookups share it, by design
## Follow-ups captured
