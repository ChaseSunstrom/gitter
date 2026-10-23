---
id: T-0092
type: FIX
tier: M
status: active
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T16:05:35Z
updated: 2026-10-23T03:15:29Z
builder: 2026-10-23T03:01:43Z
builder_proc: archbox-hardened|pid:[4026531836]|16446|602195
lane: /home/chase/projects/gitter/.claude/worktrees/agent-aba65f0568f396924
lane_branch: worktree-agent-aba65f0568f396924
base: 6d58556833b5d9e8174ee5d6cead8a8d00059f31
base_tree: 19349ab6e6c5ad7d06cc81187bbb51ad52194726
plan0_at: 2026-10-23T03:01:59Z
plan0_scope: []
---
# CI test results and flaky-test rows don't grow for ever
## Raw request
> ci_test_results and ci_flaky_tests have no retention or pruning. Artifacts are the only CI data that expires
> (noticed by an agent; research T-0012-review)
## Interpretation
Per-test CI results (ci_test_results) are kept for every job ever run, so a busy repository's database grows without end; flaky-test rows are already capped per repository. Results should expire like artifacts do, keeping what pages still show (each job's latest results).
## Assumptions (confidence)
## Acceptance criteria
- [ ] test results of runs older than the artifact retention are pruned by the CI sweep, keeping each job's latest results; flaky rows stay capped per repository — verify with `cargo test -q --test ci_extras retention`
## Non-goals
## Approach (options → choice → why)
Options: (1) prune in the existing CI sweep by age, using the artifact retention days, keeping rows marked current; (2) cap rows per repository by count; (3) delete with their runs when runs are pruned. Choice: 1, it matches how artifacts expire and needs no new setting; keep current = 1 rows so the latest results per job stay visible. Reuse the sweep that already runs on a timer (src/ci/jobs.rs sweep or src/ci/cache.rs sweep).
## Risks and rollback
## Execution prompt
## Steps
1. [x] test: old runs' ci_test_results go after a sweep, recent ones and the newest per job stay; fix: prune in the existing CI sweep (src/ci/jobs.rs sweep or cache sweep)
## Resume here
## Docs impact
## Verification evidence
- (step 1) `cargo test -q --test ci_extras retention` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 7 filtered out; finished in 0.64s [ran] [tree b1e8b469b1b6] (2026-10-23T03:04:41Z)
- (step 1) `cargo test -q --test ci_extras retention` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 7 filtered out; finished in 0.72s [ran] [tree 8af993edd4e0] (2026-10-23T03:15:29Z)
## Log
- 2026-10-22T16:05:35Z created
- 2026-10-23T03:01:15Z planned from capture
- 2026-10-23T03:01:21Z approved: the user asked to implement every queued feature (standing request)
- 2026-10-23T03:01:21Z set approved=True
- 2026-10-23T03:01:43Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0092.builder.md
- 2026-10-23T03:01:59Z focused
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research T-0012-review: Verdict: changes needed.** I found three MEDIUM issues and several LOW ones. I found no path traversal, symlink, XXE, auth or XSS holes. [stale: …
- research T-0006-review-2: Verdict: changes needed.** The feature matches the request and the README. The operator-facing gaps are the unbounded built-in runner access, a…
- research T-0006-review-1: Verdict: changes needed.** Four HIGH findings, each shown by a code path. I found them by reading, not by running anything. [stale: src/api/ci.…
- research T-0018-review-1: Verdict: changes needed.** There are no HIGH findings, and the sandbox and the secret scoping hold. But the "closing or merging takes the previ…
