---
id: T-0040
type: FEATURE
tier: L
status: dropped
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-21T22:39:49Z
updated: 2026-10-21T23:39:30Z
---
# Delete_user` (src/api/admin.rs:273) and repo deletion have no LFS cleanup hook
## Raw request
> delete_user` (src/api/admin.rs:273) and repo deletion have no LFS cleanup hook
> (noticed by an agent; research T-0015-review)
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
- 2026-10-21T22:39:49Z created
- 2026-10-21T23:39:30Z dropped: fixed in 227e6fb: repo and user deletion collect their LFS objects (lfs::collect)
## Follow-ups captured
