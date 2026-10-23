---
id: T-0123
type: FEATURE
tier: L
status: dropped
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T23:21:01Z
updated: 2026-10-22T23:24:35Z
---
# - Main's uncommitted `land_merge` (src/api/issues.rs:1474) adds `deploy::pull_closed`, whi
## Raw request
> - Main's uncommitted `land_merge` (src/api/issues.rs:1474) adds `deploy::pull_closed`, which this branch's hunk overlaps. Keep that call when integrating
> (noticed by an agent; research T-0101-review)
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
- 2026-10-22T23:21:01Z created
- 2026-10-22T23:24:35Z dropped: done at integration: land_merge on main still calls deploy::pull_closed before the merged webhook
## Follow-ups captured
