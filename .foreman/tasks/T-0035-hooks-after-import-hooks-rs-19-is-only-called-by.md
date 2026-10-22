---
id: T-0035
type: FEATURE
tier: M
status: dropped
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-21T21:16:09Z
updated: 2026-10-21T23:39:31Z
---
# Hooks::after_import` (`hooks.rs:19`) is only called by the bench, although its comment say
## Raw request
> hooks::after_import` (`hooks.rs:19`) is only called by the bench, although its comment says "an import"
> (noticed by an agent; research T-0011-review)
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
- 2026-10-21T21:16:09Z created
- 2026-10-21T23:39:31Z dropped: folded into T-0010 (import by URL will call hooks::after_import)
## Follow-ups captured
