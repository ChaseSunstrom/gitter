---
id: T-0142
type: FIX
tier: S
status: done
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-23T03:52:14Z
updated: 2026-10-23T05:03:20Z
base: 5064f2e8a5d3750568a8ab4b5fb57c9173b9208b
base_tree: 1e38398d8dca062e68df1cd96e95d1e87a4785eb
plan0_at: 2026-10-23T04:31:19Z
plan0_scope: []
verified: strong
---
# Cancelling a CI run runs its follow-ups (merge queue, auto-merge, notification) once
## Raw request
> Cancelling a CI run (api::ci::cancel, lifecycle) never runs follow-ups (auto-merge, merge queue, notifications); queue::cancel should say whether it ended the run so they can run once.
## Interpretation
## Assumptions (confidence)
## Acceptance criteria
- [x] cancelling a merge queue's test run takes its pull request out of the queue at once, and cancelling a run that already ended does nothing more — verify with `cargo test -q --test ci cancel_runs_follow_ups`
## Non-goals
## Approach (options → choice → why)
## Risks and rollback
## Execution prompt
## Steps
1. [x] test: cancel the queue's run, the entry leaves within seconds; fix: queue::cancel says whether it ended the run, api::ci::cancel then calls jobs::run_ended
## Resume here
## Docs impact
## Verification evidence
- (step 1) `cargo test -q --test ci cancel_runs_follow_ups` → red as expected (exit 101) · test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 20 filtered out; finished in 3.48s / error: test failed, to rerun pass `--test ci` [ran] [tree 0b8e77249497] (2026-10-23T04:31:51Z)
- (ac 1) `cargo test -q --test ci cancel_runs_follow_ups 2>&1 | grep -E '^error|warning|panicked|test result' -A3 | head -8` → exit 0 · test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 20 filtered out; finished in 0.76s [ran] [tree 4dc0b3381463] (2026-10-23T04:32:18Z)
- (step 1) `cargo test -q --test ci` → exit 0 · ..................... / test result: ok. 21 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.67s [ran] [tree 4dc0b3381463] (2026-10-23T04:32:28Z)
- (ac 1) `cargo test -q --test ci cancel_runs_follow_ups` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 20 filtered out; finished in 0.76s [ran] [tree 4dc0b3381463] (2026-10-23T04:32:29Z)
- `cargo test -q --test ci cancel_runs_follow_ups` → ✗ exit 101 · test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 20 filtered out; finished in 3.39s / error: test failed, to rerun pass `--test ci` (start tree + this task's tests) [ran] [tree 4dc0b3381463] (2026-10-23T04:33:08Z)
- `cargo test -q --test ci cancel_runs_follow_ups` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 20 filtered out; finished in 0.76s [ran] [tree 4dc0b3381463] (2026-10-23T04:33:08Z)
- (ac 1) `cargo test -q --test ci cancel_runs_follow_ups` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 20 filtered out; finished in 0.76s [ran] [tree 4dc0b3381463] (2026-10-23T05:03:20Z)
- (audit self) `self: cancel reports whether it ended the run (status before vs after), and only then runs jobs::run_ended, so cancelling an ended run adds nothing; trashing a repository still cancels without follow-ups (the repository is going)` → no findings [tree 4dc0b3381463] (2026-10-23T05:03:20Z)
## Log
- 2026-10-23T03:52:14Z created
- 2026-10-23T04:31:19Z planned from capture
- 2026-10-23T04:31:19Z focused
- 2026-10-23T05:03:20Z done
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- T-0086 [FIX M, done, 2 steps] CI: one pull request run when a pull request opens right after its branch is pushed, and follow-ups after failed runs — lesson: When two co…
- T-0094 [FIX M, done, 2 steps] Notifications follow-ups (T-0076 review): mentions in 4-space-indented list items are skip — lesson: Markdown-aware text scanning needs the…
- T-0051 [FEATURE L, done, 4 steps] Cross-repository pull requests from forks: open a PR from bob/hello:feature into alice/hel — lesson: A permission one user grants on be…
- T-0084 [FEATURE L, done, 4 steps] Integrate builder batch 4 into main: notifications (T-0076), mirrors (T-0083), CI extras (T-0012) — lesson: Merge lanes one at a time a…
## Files touched
- tests/ci.rs
- src/api/ci.rs
- src/ci/queue.rs
