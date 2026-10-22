---
id: T-0081
type: FEATURE
tier: M
status: captured
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T14:31:21Z
updated: 2026-10-22T16:11:48Z
---
# Intermittent SSH test hang: tests/ssh.rs git_over_ssh_follows_the_http_access_rules hung o
## Raw request
> Intermittent SSH test hang: tests/ssh.rs git_over_ssh_follows_the_http_access_rules hung once 60 s (2 of ~25 runs, once on the first run after a rebuild, once in a full cargo test); couldn't reproduce in 20 loops incl. under load. The run() helper now names the hung command. Suspects: the bounded stdin channel in the SSH data() handler blocking the session loop while git's output waits on it, or a stdio pipe held open by a background git process
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
- 2026-10-22T14:31:21Z created
- 2026-10-22T16:11:48Z recurred 2026-10-22 in a full cargo test run during T-0017: 'command hung: ssh ... git-upload-pack alice/pu...' in git_over_ssh_follows_the_http_access_rules; passed on the immediate rerun
## Follow-ups captured
