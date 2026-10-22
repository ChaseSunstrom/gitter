---
id: T-0104
type: FIX
tier: M
status: dropped
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T19:47:50Z
updated: 2026-10-22T20:01:43Z
---
# - `releases.rs:365-378`: publishing a draft makes the tag with `make_tag`, and if the late
## Raw request
> - `releases.rs:365-378`: publishing a draft makes the tag with `make_tag`, and if the later `UPDATE` fails the tag is left behind. It is the same bug as the create path, in `update
> (noticed by an agent; research T-0061-review)
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
- 2026-10-22T19:47:50Z created
- 2026-10-22T20:01:43Z dropped: done in T-0061's lane: publishing a draft takes back its tag when the update fails
## Follow-ups captured
