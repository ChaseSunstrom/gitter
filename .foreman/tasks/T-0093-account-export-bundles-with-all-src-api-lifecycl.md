---
id: T-0093
type: FIX
tier: L
status: dropped
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T16:05:35Z
updated: 2026-10-22T16:06:49Z
---
# Account export bundles with `--all` (src/api/lifecycle.rs:728) include the `refs/gitter-qu
## Raw request
> account export bundles with `--all` (src/api/lifecycle.rs:728) include the `refs/gitter-queue/*` refs. Orphaned queue refs, left when a `delete_ref` fails or `start_test` races a dequeue, are never p…
> (noticed by an agent; research T-0017-review)
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
- 2026-10-22T16:05:35Z created
- 2026-10-22T16:06:49Z dropped: fixed in T-0017: export bundles exclude refs/gitter-queue/*
## Follow-ups captured
