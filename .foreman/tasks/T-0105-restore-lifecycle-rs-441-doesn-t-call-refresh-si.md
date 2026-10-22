---
id: T-0105
type: FEATURE
tier: M
status: dropped
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T19:48:26Z
updated: 2026-10-22T20:12:11Z
---
# - `restore` (lifecycle.rs:441) doesn't call `refresh_size`, so a repo whose size was alrea
## Raw request
> - `restore` (lifecycle.rs:441) doesn't call `refresh_size`, so a repo whose size was already zeroed by the old race stays at 0 against the quota until the next boot's backfill
> (noticed by an agent; research T-0062-review)
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
- 2026-10-22T19:48:26Z created
- 2026-10-22T20:12:11Z dropped: done in T-0062's lane: restore refreshes the size and backfill skips trashed repos
## Follow-ups captured
