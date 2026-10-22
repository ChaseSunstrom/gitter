---
id: T-0036
type: FEATURE
tier: M
status: dropped
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-21T21:33:59Z
updated: 2026-10-21T23:39:31Z
---
# Password_login` matches `email = ?1` while emails are not unique-constrained against usern
## Raw request
> password_login` matches `email = ?1` while emails are not unique-constrained against usernames, so a user can set their email to another user's username. Pre-existing and worth its own task
> (noticed by an agent; research T-0009-review)
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
- 2026-10-21T21:33:59Z created
- 2026-10-21T23:39:31Z dropped: not a bug: emails must contain @ (validate::email) and usernames can't, so the OR can't match another account
## Follow-ups captured
