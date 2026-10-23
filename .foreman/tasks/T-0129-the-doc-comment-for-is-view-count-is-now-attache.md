---
id: T-0129
type: FEATURE
tier: L
status: dropped
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-23T01:31:42Z
updated: 2026-10-23T01:32:25Z
---
# The doc comment for `is_view_count` is now attached to `is_collab_typing` (`src/api/mod.rs
## Raw request
> the doc comment for `is_view_count` is now attached to `is_collab_typing` (`src/api/mod.rs:415-417`). `Repo::writable` doesn't reject mirror repos at join, so the commit is refused only afterwards
> (noticed by an agent; research T-0022-review-1)
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
- 2026-10-23T01:31:42Z created
- 2026-10-23T01:32:25Z dropped: folded into T-0022 (the misplaced doc comment is fixed there)
## Follow-ups captured
