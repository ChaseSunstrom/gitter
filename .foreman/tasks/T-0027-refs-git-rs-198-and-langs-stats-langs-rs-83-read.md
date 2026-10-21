---
id: T-0027
type: FEATURE
tier: M
status: done
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-21T18:07:06Z
updated: 2026-10-21T20:55:51Z
done_in: T-0029
---
# - `refs()` (git.rs:198) and `langs::stats` (langs.rs:83) read unbounded git output on near
## Raw request
> - `refs()` (git.rs:198) and `langs::stats` (langs.rs:83) read unbounded git output on nearly every page view. `files` caps it at 4 MB
> (noticed by an agent; research T-0001-review-3)
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
- 2026-10-21T18:07:06Z created
- 2026-10-21T20:43:11Z batched into T-0029
- 2026-10-21T20:55:51Z done in T-0029 (batch)
## Follow-ups captured
