---
id: T-0131
type: FIX
tier: S
status: captured
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-23T01:42:01Z
updated: 2026-10-23T01:42:01Z
---
# Git.rs mode_of runs ls-tree without -z: a path with a quote, backslash or newline comes ba
## Raw request
> git.rs mode_of runs ls-tree without -z: a path with a quote, backslash or newline comes back quoted, so its mode is never found (an executable file edited in the browser loses its mode). Use -z and split on NUL, as collab.rs file_at does.
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
- 2026-10-23T01:42:01Z created
## Follow-ups captured
