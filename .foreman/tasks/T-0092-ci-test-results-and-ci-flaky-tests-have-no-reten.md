---
id: T-0092
type: FIX
tier: M
status: done
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T16:05:35Z
updated: 2026-10-23T03:33:22Z
base: eda52b57e2544417993b4a27615bd019f72343c2
base_tree: 912d6983db00fdddff09fa68fd061308037e24b0
plan0_at: 2026-10-23T03:01:59Z
plan0_scope: []
verified: strong
---
# CI test results and flaky-test rows don't grow for ever
## Raw request
> ci_test_results and ci_flaky_tests have no retention or pruning. Artifacts are the only CI data that expires
> (noticed by an agent; research T-0012-review)
## Interpretation
Per-test CI results (ci_test_results) are kept for every job ever run, so a busy repository's database grows without end; flaky-test rows are already capped per repository. Results should expire like artifacts do, keeping what pages still show (each job's latest results).
## Assumptions (confidence)
## Acceptance criteria
- [x] test results of jobs that finished before the artifact retention are pruned by the CI sweep, current ones included (like artifacts), and the run page says the details are gone; jobs still running or re-queued keep theirs; flaky rows stay capped per repository — verify with `cargo test -q --test ci_extras retention`
## Non-goals
## Approach (options → choice → why)
Options: (1) prune in the existing CI sweep by age, using the artifact retention days, keeping rows marked current; (2) cap rows per repository by count; (3) delete with their runs when runs are pruned. Choice: 1, it matches how artifacts expire and needs no new setting; keep current = 1 rows so the latest results per job stay visible. Reuse the sweep that already runs on a timer (src/ci/jobs.rs sweep or src/ci/cache.rs sweep).
## Risks and rollback
## Execution prompt
## Steps
1. [x] test: old runs' ci_test_results go after a sweep, recent ones and the newest per job stay; fix: prune in the existing CI sweep (src/ci/jobs.rs sweep or cache sweep)
## Resume here
## Docs impact
README Storage paragraph: per-test results go 30 days after their job; counts and coverage stay
## Verification evidence
- (step 1) `cargo test -q --test ci_extras retention` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 7 filtered out; finished in 0.64s [ran] [tree b1e8b469b1b6] (2026-10-23T03:04:41Z)
- (step 1) `cargo test -q --test ci_extras retention` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 7 filtered out; finished in 0.72s [ran] [tree 8af993edd4e0] (2026-10-23T03:15:29Z)
- (step 1) `cargo test -q --test ci_extras retention` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 7 filtered out; finished in 0.32s [ran] [tree 28c3a2786b3f] (2026-10-23T03:25:24Z)
- (step 1) `cargo test -q --test ci_extras && cargo test -q --test ci` → exit 0 · ............... / test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.80s [ran] [tree 7a6cfb8374e2] (2026-10-23T03:29:09Z)
- (ac 1) `cargo test -q --test ci_extras retention` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 7 filtered out; finished in 0.45s [ran] [tree 7a6cfb8374e2] (2026-10-23T03:29:10Z)
- `cargo test -q --test ci_extras retention` → ✗ exit 101 · test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 7 filtered out; finished in 0.37s / error: test failed, to rerun pass `--test ci_extras` (start tree + this task's tests) [ran] [tree 7a6cfb8374e2] (2026-10-23T03:30:04Z)
- `cargo test -q --test ci_extras retention` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 7 filtered out; finished in 0.42s [ran] [tree 7a6cfb8374e2] (2026-10-23T03:30:04Z)
- (ac 1) `cargo test -q --test ci_extras retention` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 7 filtered out; finished in 0.54s [ran] [tree 7a6cfb8374e2] (2026-10-23T03:33:22Z)
- (audit intent) `fm-reviewer (intent, edge, maintainer): nothing the merge queue or required checks read is deleted; note shown for partly expired runs, retention days from the server, batched deletes, pull request box says details are gone; the task text updated to the decision to expire current rows too` → results expire like artifacts, pages say so [tree 7a6cfb8374e2] (2026-10-23T03:33:22Z)
- (audit edge) `fm-reviewer (intent, edge, maintainer): nothing the merge queue or required checks read is deleted; note shown for partly expired runs, retention days from the server, batched deletes, pull request box says details are gone; the task text updated to the decision to expire current rows too` → in-flight and re-queued jobs keep their rows; a run with one old and one recent job still says what's gone [tree 7a6cfb8374e2] (2026-10-23T03:33:22Z)
## Log
- 2026-10-22T16:05:35Z created
- 2026-10-23T03:01:15Z planned from capture
- 2026-10-23T03:01:21Z approved: the user asked to implement every queued feature (standing request)
- 2026-10-23T03:01:21Z set approved=True
- 2026-10-23T03:01:43Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0092.builder.md
- 2026-10-23T03:01:59Z focused
- 2026-10-23T03:23:16Z criterion 1 text: test results of runs older than the artifact retention are pruned by the CI sweep, keeping each job's latest results; flaky rows stay capped per repository → test results of jobs that finished before the artifact retention are pruned by the CI sweep, current ones included (like artifacts), and the run page says the details are gone; jobs still running or re-queued keep theirs; flaky rows stay capped per repository
- 2026-10-23T03:23:16Z steer: expire each job's latest results too (the main growth); run and PR pages say the details are gone
- 2026-10-23T03:28:08Z merged worktree-agent-aba65f0568f396924 (6 file(s))
- 2026-10-23T03:28:15Z lane removed: /home/chase/projects/gitter/.claude/worktrees/agent-aba65f0568f396924
- 2026-10-23T03:28:27Z focused
- 2026-10-23T03:33:22Z done
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research T-0012-review: Verdict: changes needed.** I found three MEDIUM issues and several LOW ones. I found no path traversal, symlink, XXE, auth or XSS holes. [stale: …
- research T-0006-review-2: Verdict: changes needed.** The feature matches the request and the README. The operator-facing gaps are the unbounded built-in runner access, a…
- research T-0006-review-1: Verdict: changes needed.** Four HIGH findings, each shown by a code path. I found them by reading, not by running anything. [stale: src/api/ci.…
- research T-0018-review-1: Verdict: changes needed.** There are no HIGH findings, and the sandbox and the secret scoping hold. But the "closing or merging takes the previ…
## Why not caught
none: growth over months isn't something a test sees; the review that captured it was the catch
## Lessons
- When a brief names what to keep, check that it isn't where the growth is: the first pass kept exactly the rows that grow
## Files touched
- tests/ci_extras.rs
- src/ci/jobs.rs
- src/api/ci_extras.rs
- static/js/pages/ci.js
- README.md
- static/js/cireport.js
