---
id: T-0112
type: FIX
tier: S
status: blocked
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T21:34:05Z
updated: 2026-10-23T03:02:06Z
group: fork-related gaps
---
# Webhook pull request payloads say which repository the head comes from
## Raw request
> src/webhooks.rs:146` reports only `head_branch` for fork PRs, so the payload has no head repo
> (noticed by an agent; research T-0051-review-1)
## Interpretation
## Assumptions (confidence)
## Acceptance criteria
- [ ] pull_request webhook payloads for a fork's pull request carry head_repo {full_name, clone_url} (a fork here) or its address on another server; same-repository ones carry this repository — verify with `cargo test -q --test forks webhook`
## Non-goals
## Approach (options → choice → why)
## Risks and rollback
## Execution prompt
## Steps
1. [ ] test: a fork PR's webhook delivery payload has head_repo; fix: issue_json in src/webhooks.rs reads the head repo (pulls.head_repo_id, or remote_pulls for one from another server)  <- CURRENT
## Resume here
## Docs impact
## Verification evidence
## Log
- 2026-10-22T21:34:05Z created
- 2026-10-23T03:01:14Z planned from capture
- 2026-10-23T03:01:20Z approved: the user asked to implement every queued feature (standing request)
- 2026-10-23T03:01:20Z set approved=True
- 2026-10-23T03:02:06Z blocked: needs T-0023 committed first: its payload covers pull requests from other servers (remote_pulls), which a builder's worktree from main doesn't have yet
## Follow-ups captured
