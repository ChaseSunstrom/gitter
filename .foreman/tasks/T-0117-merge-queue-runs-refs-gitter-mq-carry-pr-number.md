---
id: T-0117
type: FEATURE
tier: M
status: dropped
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T22:55:45Z
updated: 2026-10-22T22:56:01Z
---
# Merge-queue runs (`refs/gitter/mq/…`) carry `pr_number`, so a `preview:` job in one would
## Raw request
> Merge-queue runs (`refs/gitter/mq/…`) carry `pr_number`, so a `preview:` job in one would replace the PR's real preview. Worth its own task
> (noticed by an agent; research T-0018-review-1)
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
- 2026-10-22T22:55:45Z created
- 2026-10-22T22:56:01Z dropped: fixed inline in T-0018: job_started makes a preview deployment only for event == pull_request runs, so merge-queue runs never touch a PR's preview
## Follow-ups captured
