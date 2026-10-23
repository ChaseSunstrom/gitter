---
id: T-0110
type: FIX
tier: S
status: done
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T20:41:54Z
updated: 2026-10-23T14:01:43Z
base: b2b4d5384cf573d6ecd6d7f2077504805da391bd
base_tree: e9d101f79d9491f10019815034bb8b6f06c9fc7d
plan0_at: 2026-10-23T13:42:45Z
plan0_scope: []
verified: strong
---
# Imports keep the source repository's default branch
## Raw request
> the mock in `tests/import.rs` sends `default_branch`, but `Listed` ignores it; `fix_head` presumably handles it, but a master-default source repo is untested
> (noticed by an agent; research T-0090-review-3)
## Interpretation
## Assumptions (confidence)
## Acceptance criteria
- [x] a repository imported from a source whose default branch isn't main (with main also present) gets the source's default — verify with `cargo test -q --test import default_branch`
## Non-goals
## Approach (options → choice → why)
## Risks and rollback
## Execution prompt
## Steps
1. [x] test: the mock's default is feature, import, the repository's default is feature; fix: carry default_branch from the listing to the import item and set HEAD after the clone
## Resume here
## Docs impact
## Verification evidence
- (step 1) `cargo test -q --test import default_branch` → red as expected (exit 101) · test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 8 filtered out; finished in 0.97s / error: test failed, to rerun pass `--test import` [ran] [tree a6d84c1a93de] (2026-10-23T13:43:01Z)
- (step 1) `cargo test -q --test import` → exit 0 · ......... / test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.80s [ran] [tree 58295762e014] (2026-10-23T13:43:54Z)
- (ac 1) `cargo test -q --test import default_branch` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 8 filtered out; finished in 0.98s [ran] [tree 58295762e014] (2026-10-23T13:43:55Z)
- `cargo test -q --test import default_branch` → ✗ exit 101 · test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 8 filtered out; finished in 0.92s / error: test failed, to rerun pass `--test import` (start tree + this task's tests) [ran] [tree 58295762e014] (2026-10-23T13:44:39Z)
- `cargo test -q --test import default_branch` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 8 filtered out; finished in 1.42s [ran] [tree 58295762e014] (2026-10-23T13:44:39Z)
- (ac 1) `cargo test -q --test import default_branch` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 8 filtered out; finished in 0.94s [ran] [tree 58295762e014] (2026-10-23T14:01:43Z)
- (audit self) `self: the listing's default_branch (GitHub and GitLab both name it so) rides on the import item and is set as HEAD after the copy only when that branch arrived; the other 8 import tests still pass` → no findings [tree 58295762e014] (2026-10-23T14:01:43Z)
## Log
- 2026-10-22T20:41:54Z created
- 2026-10-23T13:42:45Z planned from capture
- 2026-10-23T13:42:45Z focused
- 2026-10-23T14:01:43Z done
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research T-0090-review-3: Verdict:** approve with nits. The pre-audit's debug-leftover, secret-looking and `clean`/`pull` flags are all false positives. [stale: src/api/…
- playbook Test-Driven Development Workflow (fm playbooks show verify/tdd-workflow)
- research T-0090-review-1: Changes needed. A non-admin user can starve the shared import worker, and re-running an import overwrites local work. [stale: src/auth.rs, src/…
- T-0096 [FEATURE M, done, 4 steps] FEATURE: New repository page can mirror or import a remote URL directly (creates the repo — lesson: A page that polls for a change must…
- similar past tasks were M: consider --tier M
- the similar finished task took 4 steps
## Files touched
- tests/import.rs
- src/db.rs
- src/imports.rs
