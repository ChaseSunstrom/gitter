---
id: T-0086
type: FIX
tier: M
status: done
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T15:59:34Z
updated: 2026-10-23T04:25:27Z
base: 6ac98dbba2f956b1f0c5e60f7ff2a8321d1763d0
base_tree: b8c1aafe88afd87500183c698ce71d0fb7f0e8e2
plan0_at: 2026-10-23T03:42:13Z
plan0_scope: []
verified: strong
---
# CI: one pull request run when a pull request opens right after its branch is pushed, and follow-ups after failed runs
## Raw request
> CI: opening a pull request right after pushing its branch starts two PR runs (race between trigger::on_push for open PRs and on_pull_opened); runs that fail without jobs::finish (create_failed_run, lost leases) skip follow-ups (auto-merge, merge queue, notifications)
## Interpretation
Two CI races. Pushing a branch and opening its pull request right after can start two pull_request runs: trigger::on_push starts runs for open PRs of that branch, and on_pull_opened starts one too. And runs that end without jobs::finish (create_failed_run, or a job whose lease is lost) skip what finishing does: auto-merge, the merge queue, notifications.
## Assumptions (confidence)
## Acceptance criteria
- [x] opening a pull request right after pushing its branch starts one pull request run, not two — verify with `cargo test -q --test ci pull_request_opened_once`
- [x] a run that fails without jobs::finish (create_failed_run, a lost lease) still triggers its follow-ups (auto-merge, merge queue, notifications) — verify with `cargo test -q --test ci failed_run_follow_ups`
## Non-goals
## Approach (options → choice → why)
Options: (1) a uniqueness check before inserting a pull_request run for the same (repo, sha, PR number, workflow) that's queued or running; (2) a unique index on those columns; (3) serialize triggers per repository with a lock. Choice: 1 inside the insert transaction (cheap, no schema change), or 2 if runs already have the columns. For follow-ups, one function that every way a run ends goes through.
## Risks and rollback
## Execution prompt
## Steps
1. [x] test: push then open at once gives one PR run; fix: dedupe in trigger (same repo, sha, event, PR number)
2. [x] test: create_failed_run and a lost lease call the same follow-ups as finish; fix: route them through one finished-run function
## Resume here
## Docs impact
none: behaviour people expected already (one run, notifications on failure)
## Verification evidence
- (ac 1) `cargo test -q --test ci pull_request_opened_once 2>&1 | tail -8` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 15 filtered out; finished in 0.70s [ran] [tree 86f669595448] (2026-10-23T03:46:30Z)
- (step 1) `cargo test -q --test ci pull_request_opened_once 2>&1 | tail -8` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 15 filtered out; finished in 0.70s [ran] [tree 86f669595448] (2026-10-23T03:46:30Z)
- (step 1) `cargo test -q --test ci pull_request_opened_once` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 15 filtered out; finished in 0.46s [ran] [tree 86f669595448] (2026-10-23T03:46:34Z)
- (ac 2) `cargo test -q --test ci failed_run_follow_ups 2>&1 | grep -v "^$" | tail -20` → ✗ exit 1 · test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 16 filtered out; finished in 5.55s / error: test failed, to rerun pass `--test ci` [ran] [tree 36afd2069a22] (2026-10-23T03:47:24Z)
- (ac 2) `cargo test -q --test ci failed_run_follow_ups 2>&1 | grep -v "^$" | tail -20` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 16 filtered out; finished in 0.59s [ran] [tree d4d0f412f1e1] (2026-10-23T03:49:43Z)
- (step 2) `cargo test -q --test ci failed_run_follow_ups 2>&1 | grep -v "^$" | tail -20` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 16 filtered out; finished in 0.59s [ran] [tree d4d0f412f1e1] (2026-10-23T03:49:43Z)
- (step 2) `cargo test -q --test ci failed_run_follow_ups` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 16 filtered out; finished in 0.48s [ran] [tree d4d0f412f1e1] (2026-10-23T03:51:26Z)
- (ac 1) `cargo test -q --test ci pull_request_opened_once 2>&1 | grep -v "^$" | tail -12` → ✗ exit 1 · test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 17 filtered out; finished in 1.13s / error: test failed, to rerun pass `--test ci` [ran] [tree d8aee47b762b] (2026-10-23T03:55:12Z)
- (step 1) `cargo test -q --test ci pull_request_opened_once` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 17 filtered out; finished in 0.45s [ran] [tree 9758a870e734] (2026-10-23T03:57:05Z)
- (step 1) `cargo test -q --test ci pull_request_opened_once` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 17 filtered out; finished in 0.82s [ran] [tree 42d37a1b0838] (2026-10-23T04:20:49Z)
- (step 2) `cargo test -q --test ci failed_run_follow_ups` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 17 filtered out; finished in 0.80s [ran] [tree 42d37a1b0838] (2026-10-23T04:20:50Z)
- (ac 1) `cargo test -q --test ci pull_request_opened_once` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 17 filtered out; finished in 0.78s [ran] [tree 42d37a1b0838] (2026-10-23T04:20:51Z)
- (ac 2) `cargo test -q --test ci failed_run_follow_ups` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 17 filtered out; finished in 0.80s [ran] [tree 42d37a1b0838] (2026-10-23T04:20:53Z)
- `cargo test -q --test ci pull_request_opened_once` → ✗ exit 101 · For more information about this error, try `rustc --explain E0425`. / error: could not compile `gitter` (test "ci") due to 1 previous error (start tree + this task's tests) [ran] [tree 42d37a1b0838] (2026-10-23T04:21:49Z)
- `cargo test -q --test ci pull_request_opened_once` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 17 filtered out; finished in 0.82s [ran] [tree 42d37a1b0838] (2026-10-23T04:21:49Z)
- (audit intent) `fm-reviewer (T-0086 branch review)` → both starters now build the same run (pull_files from the merge base), so keeping the first loses nothing; follow-ups for broken configs and lost jobs [tree 42d37a1b0838] (2026-10-23T04:22:33Z)
- (audit edge) `fm-reviewer` → no legitimate run skipped (sha in the key, re-runs bypass start, other events untouched); a fast first run that already finished still dedupes (30 s window); follow-ups idempotent [tree 42d37a1b0838] (2026-10-23T04:22:33Z)
- (audit operator) `fm-reviewer` → spawned merges avoids the merge queue lock; follow-up errors logged instead of failing committed requests [tree 42d37a1b0838] (2026-10-23T04:22:34Z)
- (ac 1) `cargo test -q --test ci pull_request_opened_once` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 17 filtered out; finished in 0.93s [ran] [tree 42d37a1b0838] (2026-10-23T04:25:26Z)
- (ac 2) `cargo test -q --test ci failed_run_follow_ups` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 17 filtered out; finished in 0.91s [ran] [tree 42d37a1b0838] (2026-10-23T04:25:26Z)
## Log
- 2026-10-22T15:59:34Z created
- 2026-10-23T03:41:48Z planned from capture
- 2026-10-23T03:41:51Z approved: the user asked to implement every queued feature (standing request)
- 2026-10-23T03:41:51Z set approved=True
- 2026-10-23T03:41:55Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0086.builder.md
- 2026-10-23T03:42:13Z focused
- 2026-10-23T04:20:04Z merged worktree-agent-a35568caec7903747 (7 file(s))
- 2026-10-23T04:20:09Z lane removed: /home/chase/projects/gitter/.claude/worktrees/agent-a35568caec7903747
- 2026-10-23T04:20:12Z focused
- 2026-10-23T04:25:27Z done
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research T-0018-review-2: Verdict** — changes needed. The diff file T-0018.diff is out of date against the working tree. I reviewed the working tree, so my line numbers …
- T-0051 [FEATURE L, done, 4 steps] Cross-repository pull requests from forks: open a PR from bob/hello:feature into alice/hel — lesson: A permission one user grants on be…
- research T-0018-review-3: Verdict:** changes needed. No HIGH findings. Findings 1 and 2 break README promises and finding 3 is a test gap on a security rule. I confirmed…
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
## Why not caught
the earlier tests opened pull requests after pushes had settled; no test raced the two starters
## Lessons
- When two code paths race to create the same thing and only the first is kept, make sure both would build it the same way
## Files touched
- tests/ci.rs
- src/ci/trigger.rs
- src/ci/jobs.rs
- src/api/notifications.rs
- src/ci/queue.rs
- src/ci/mod.rs
- src/api/ci.rs
