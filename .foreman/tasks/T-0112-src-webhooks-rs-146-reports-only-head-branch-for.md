---
id: T-0112
type: FIX
tier: S
status: active
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T21:34:05Z
updated: 2026-10-23T03:17:11Z
group: fork-related gaps
builder: 2026-10-23T03:11:43Z
builder_proc: archbox-hardened|pid:[4026531836]|16446|602195
lane: /home/chase/projects/gitter/.claude/worktrees/agent-a8f68af43beffd8a4
lane_branch: worktree-agent-a8f68af43beffd8a4
base: 581b0f60641f1883454c267e68b1f22eb002f966
base_tree: f2daa4f48068b9d32471ec0bce6d490c43c8ef81
plan0_at: 2026-10-23T03:11:55Z
plan0_scope: []
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
1. [x] test: a fork PR's webhook delivery payload has head_repo; fix: issue_json in src/webhooks.rs reads the head repo (pulls.head_repo_id, or remote_pulls for one from another server)
## Resume here
## Docs impact
## Verification evidence
- (ac 1) `cargo test -q --test forks webhook 2>&1 | tail -15` → ✗ exit 1 · test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 4 filtered out; finished in 2.22s / error: test failed, to rerun pass `--test forks` [ran] [tree f1ad2286ead6] (2026-10-23T03:16:00Z)
- (ac 1) `cargo test -q --test forks webhook 2>&1 | tail -12` → ✗ exit 1 · test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 4 filtered out; finished in 3.66s / error: test failed, to rerun pass `--test forks` [ran] [tree 0184fec33e29] (2026-10-23T03:16:12Z)
- (ac 1) `cargo test -q --test forks webhook 2>&1 | tail -12` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 2.65s [ran] [tree abd901a589dd] (2026-10-23T03:17:05Z)
- (step 1) `cargo test -q --test forks webhook 2>&1 | tail -12` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 2.65s [ran] [tree abd901a589dd] (2026-10-23T03:17:05Z)
- (step 1) `cargo test -q --test forks webhook` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 3.24s [ran] [tree abd901a589dd] (2026-10-23T03:17:11Z)
## Log
- 2026-10-22T21:34:05Z created
- 2026-10-23T03:01:14Z planned from capture
- 2026-10-23T03:01:20Z approved: the user asked to implement every queued feature (standing request)
- 2026-10-23T03:01:20Z set approved=True
- 2026-10-23T03:02:06Z blocked: needs T-0023 committed first: its payload covers pull requests from other servers (remote_pulls), which a builder's worktree from main doesn't have yet
- 2026-10-23T03:11:42Z set status=planned
- 2026-10-23T03:11:43Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0112.builder.md
- 2026-10-23T03:11:54Z focused
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- T-0051 [FEATURE L, done, 4 steps] Cross-repository pull requests from forks: open a PR from bob/hello:feature into alice/hel — lesson: A permission one user grants on be…
- research T-0051-review-1: Verdict: changes needed.** Maintainers can be granted push access to branches of a repository the PR author has no rights on. I confirmed this …
- research T-0051-review-3: Pre-audit flags: the secret-looking values in `scripts/ui-check.mjs` and `tests/forks.rs` are test passwords and runtime tokens, and the `conso…
- research T-0023-recon: Answer [stale: src/api/issues.rs, src/api/repos.rs, src/auth.rs changed since it was written]
- similar past tasks were L: consider --tier L
- the similar finished task took 4 steps
