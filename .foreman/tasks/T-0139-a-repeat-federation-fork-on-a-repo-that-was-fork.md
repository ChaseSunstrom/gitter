---
id: T-0139
type: FEATURE
tier: M
status: dropped
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-23T02:38:10Z
updated: 2026-10-23T02:38:51Z
---
# A repeat `/federation/fork` on a repo that was forked and later emptied hits the `remote_f
## Raw request
> a repeat `/federation/fork` on a repo that was forked and later emptied hits the `remote_forks` primary key and returns 500
> (noticed by an agent; research T-0023-review-2)
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
- 2026-10-23T02:38:10Z created
- 2026-10-23T02:38:51Z dropped: folded into T-0023 (a repeat fork replaces the old record)
## Follow-ups captured
