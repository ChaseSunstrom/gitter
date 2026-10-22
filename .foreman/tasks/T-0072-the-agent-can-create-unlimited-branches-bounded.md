---
id: T-0072
type: FEATURE
tier: M
status: dropped
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T02:35:00Z
updated: 2026-10-22T03:30:28Z
---
# The agent can create unlimited branches (bounded only by the 120-per-minute write limit);
## Raw request
> the agent can create unlimited branches (bounded only by the 120-per-minute write limit); runners with docker get `DOCKER_HOST` passed into no-image jobs
> (noticed by an agent; research T-0021-review)
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
- 2026-10-22T02:35:00Z created
- 2026-10-22T03:30:28Z dropped: done in T-0021's merge: agents hold at most 100 branches per repository (MAX_AGENT_BRANCHES), tested in mcp_agents_commit_only_to_branches_agents_created
## Follow-ups captured
