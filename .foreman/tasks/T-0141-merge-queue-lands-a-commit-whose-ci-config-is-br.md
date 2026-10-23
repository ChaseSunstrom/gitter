---
id: T-0141
type: FIX
tier: S
status: done
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-23T03:52:14Z
updated: 2026-10-23T04:31:10Z
base: d894107e8e2cf6e61543fe5e50992603af6494db
base_tree: 42d37a1b0838595eb54d2ca369f23c83bcbc34c6
plan0_at: 2026-10-23T04:26:02Z
plan0_scope: []
verified: strong
---
# Merge queue: a commit whose CI config is broken isn't landed
## Raw request
> Merge queue lands a commit whose CI config is broken: the run has no jobs, so no checks, and on a branch rule with no required checks the queue reads that as a pass. A failed run (even jobless) should fail the queue entry.
## Interpretation
## Assumptions (confidence)
## Acceptance criteria
- [x] with a merge queue and no required checks, a queued pull request whose merged commit has a broken CI config is taken out of the queue, not merged — verify with `cargo test -q --test ci broken_config_never_lands`
## Non-goals
## Approach (options → choice → why)
## Risks and rollback
## Execution prompt
## Steps
1. [x] test: queue a pull request whose branch breaks .gitter/ci.yml; fix: mergequeue verdict fails on a failed run for the commit even without checks
## Resume here
## Docs impact
## Verification evidence
- (step 1) `cargo test -q --test ci broken_config_never_lands` → red as expected (exit 101) · test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 19 filtered out; finished in 0.68s / error: test failed, to rerun pass `--test ci` [ran] [tree 51ce7ca88452] (2026-10-23T04:26:24Z)
- (step 1) `cargo test -q --test ci broken_config_never_lands && cargo test -q --test api merge_queue` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 88 filtered out; finished in 3.81s [ran] [tree be78c0fdd186] (2026-10-23T04:27:01Z)
- (ac 1) `cargo test -q --test ci broken_config_never_lands` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 19 filtered out; finished in 0.71s [ran] [tree be78c0fdd186] (2026-10-23T04:27:03Z)
- `cargo test -q --test ci broken_config_never_lands` → ✗ exit 101 · test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 19 filtered out; finished in 0.70s / error: test failed, to rerun pass `--test ci` (start tree + this task's tests) [ran] [tree be78c0fdd186] (2026-10-23T04:28:08Z)
- `cargo test -q --test ci broken_config_never_lands` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 19 filtered out; finished in 0.71s [ran] [tree be78c0fdd186] (2026-10-23T04:28:08Z)
- (ac 1) `cargo test -q --test ci broken_config_never_lands` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 19 filtered out; finished in 0.39s [ran] [tree be78c0fdd186] (2026-10-23T04:31:09Z)
- (audit self) `self: the verdict now fails on a failed run with an error for the merged commit (a config that couldn't start), before deciding pass/wait; same sha means same config, so an older run's error is the same; existing merge queue tests still pass` → no findings [tree be78c0fdd186] (2026-10-23T04:31:09Z)
## Log
- 2026-10-23T03:52:14Z created
- 2026-10-23T04:26:01Z planned from capture
- 2026-10-23T04:26:02Z focused
- 2026-10-23T04:31:10Z done
- 2026-10-23T04:31:10Z committed c6ae285 (5 files)
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- T-0017 [FEATURE M, done, 4 steps] Merge queue: pull requests merge one at a time after passing checks on the combined result — lesson: Builder lanes sharing the session …
- research T-0017-review: Verdict: changes needed.** I found no way for a non-admin to merge a PR whose required checks haven't passed on the exact landed commit. The `lan…
- research T-0018-review-3: Verdict:** changes needed. No HIGH findings. Findings 1 and 2 break README promises and finding 3 is a test gap on a security rule. I confirmed…
- T-0007 [FEATURE M, done, 5 steps] CI trust and gating: sandboxed jobs (no host docker socket, cpu/mem/time limits, network p — lesson: Trust must follow who wrote the co…
## Files touched
- tests/ci.rs
- src/mergequeue.rs
