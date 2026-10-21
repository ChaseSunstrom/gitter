---
id: T-0026
type: FIX
tier: M
status: dropped
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-21T18:07:05Z
updated: 2026-10-21T20:43:08Z
group: DB write consistency fixes
---
# - `repos.rs:96` returns from `create` without `drop_row` when the README commit fails, lea
## Raw request
> - `repos.rs:96` returns from `create` without `drop_row` when the README commit fails, leaving an orphan row
> (noticed by an agent; research T-0001-review-2)
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
- 2026-10-21T18:07:05Z created
- 2026-10-21T20:43:08Z dropped: already fixed: api/repos.rs create() calls drop_row and removes the dir on any init_repo failure, README commit included
## Follow-ups captured
