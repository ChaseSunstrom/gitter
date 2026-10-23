---
id: T-0059
type: FIX
tier: S
status: done
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T00:51:49Z
updated: 2026-10-23T03:21:52Z
group: storage quota accounting gaps
base: 581b0f60641f1883454c267e68b1f22eb002f966
base_tree: f2daa4f48068b9d32471ec0bce6d490c43c8ef81
plan0_at: 2026-10-23T03:11:58Z
plan0_scope: []
verified: strong
---
# A repository's recorded size isn't zeroed when its folder is gone mid-push
## Raw request
> - hooks.rs:21 `refresh_size` can zero `size_bytes` for a repo trashed mid-push, which would understate quota
> (noticed by an agent; research T-0033-review)
## Interpretation
## Assumptions (confidence)
## Acceptance criteria
- [x] refresh_size leaves size_bytes alone when the repository folder isn't there (moved to the trash mid-push) — verify with `cargo test -q --test api size_survives`
## Non-goals
## Approach (options → choice → why)
## Risks and rollback
## Execution prompt
## Steps
1. [x] test: folder moved away, refresh_size keeps the recorded size; fix: src/hooks.rs refresh_size returns when the folder is missing
## Resume here
## Docs impact
## Verification evidence
- (step 1) `cargo test -q --test api size_survives` → red as expected (exit 101) · test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 87 filtered out; finished in 0.54s / error: test failed, to rerun pass `--test api` [ran] [tree 96ab39e7eaaf] (2026-10-23T03:12:18Z)
- (step 1) `cargo test -q --test api size_survives` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 87 filtered out; finished in 0.82s [ran] [tree 0f191da06a7d] (2026-10-23T03:13:21Z)
- (ac 1) `cargo test -q --test api size_survives` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 87 filtered out; finished in 0.66s [ran] [tree 0f191da06a7d] (2026-10-23T03:13:23Z)
- `cargo test -q --test api size_survives` → ✗ exit 101 · test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 87 filtered out; finished in 0.78s / error: test failed, to rerun pass `--test api` (start tree + this task's tests) [ran] [tree 0f191da06a7d] (2026-10-23T03:15:18Z)
- `cargo test -q --test api size_survives` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 87 filtered out; finished in 0.56s [ran] [tree 0f191da06a7d] (2026-10-23T03:15:18Z)
- (ac 1) `cargo test -q --test api size_survives` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 87 filtered out; finished in 0.30s [ran] [tree 0f191da06a7d] (2026-10-23T03:21:51Z)
- (audit self) `self: refresh_size's callers (on_push, after_import, backfill, fork copies) all pass a live repository id; a missing folder means trashed or not yet made, and both keep their last size; restore re-measures on its next push` → no findings [tree 0f191da06a7d] (2026-10-23T03:21:51Z)
## Log
- 2026-10-22T00:51:49Z created
- 2026-10-23T03:11:57Z planned from capture
- 2026-10-23T03:11:58Z focused
- 2026-10-23T03:21:52Z done
- 2026-10-23T03:21:52Z committed a7a89af (6 files)
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research T-0033-review: Verdict: changes needed.** The core is solid. Trashed repos are excluded from `lookup`, `VISIBLE`, CI, LFS and git, and private repos don't leak …
- T-0062 [CLEAN S, done, 3 steps] Lifecycle follow-ups: free-disk check before building an export (refuse below ~2x the acco — lesson: When a constant becomes a setting, g…
- research T-0062-review: Verdict — changes needed.** The logic is sound and I found no HIGH issues. The UI copy is still stale in seven places, and lowering `trash_days` …
- T-0105 [FEATURE M, dropped, 0 steps] - `restore` (lifecycle.rs:441) doesn't call `refresh_size`, so a repo whose size was alrea
- the similar finished task took 3 steps
## Files touched
- tests/api.rs
- src/hooks.rs
