---
id: T-0042
type: FEATURE
tier: M
status: dropped
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-21T22:49:11Z
updated: 2026-10-22T00:47:31Z
group: Foreman tooling and canary failures
---
# Foreman: fm task finish --commit on a new task keeps refusing 'audit missing: self (files
## Raw request
> Foreman: fm task finish --commit on a new task keeps refusing 'audit missing: self (files changed since the audit)' because each audit rewrites the untracked task file and changes the worktree id; committing by hand first works around it
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
- 2026-10-21T22:49:11Z created
- 2026-10-22T00:47:31Z dropped: no longer reproduces: T-0008's fm task finish --commit committed cleanly (70d66fe)
## Follow-ups captured
