---
id: T-0114
type: FEATURE
tier: L
status: dropped
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T21:34:05Z
updated: 2026-10-22T21:41:36Z
---
# After_push in src/smart_http.rs:509 diffs all refs, so a concurrent server-side `refs/pull
## Raw request
> after_push in src/smart_http.rs:509 diffs all refs, so a concurrent server-side `refs/pull/*` update can show up as a user push in webhooks
> (noticed by an agent; research T-0051-review-3)
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
- 2026-10-22T21:34:05Z created
- 2026-10-22T21:41:36Z dropped: fixed inline in T-0051: ref_map leaves refs/pull/* out, so server-kept pull heads never show up as a user's push
## Follow-ups captured
