---
id: T-0101
type: FEATURE
tier: M
status: captured
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T19:03:54Z
updated: 2026-10-22T19:03:54Z
---
# Ref name clashes (a branch or tag 'v3' in the way of 'v3/rc') give unclear errors: create_
## Raw request
> Ref name clashes (a branch or tag 'v3' in the way of 'v3/rc') give unclear errors: create_branch says the branch already exists, make_tag returns a 500; say which existing ref is in the way, as a 409. Also merge's head-branch deletion skips hooks::on_push (no deletion webhook) and web-made tags only notify push mirrors
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
- 2026-10-22T19:03:54Z created
## Follow-ups captured
