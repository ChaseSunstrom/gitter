---
id: T-0029
type: FEATURE
tier: M
status: planned
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-21T20:43:11Z
updated: 2026-10-21T20:45:42Z
batch: [T-0027, T-0028]
base: df1a23922a9a3b5286fff85549183616c9d43b64
base_tree: f8f4299ea0fb2369cf1be33d5dcfb74f280dbd8d
plan0_at: 2026-10-21T20:43:30Z
plan0_scope: []
paused_tree: a4285302d011e1ad3445ecb021d21e9c10619433
---
# Robustness fixes: atomic PR merge record, capped git output for refs and language stats
## Raw request
> T-0027: - `refs()` (git.rs:198) and `langs::stats` (langs.rs:83) read unbounded git output on nearly every page view. `files` caps it at 4 MB
> (noticed by an agent; research T-0001-review-3)
> T-0028: merge_pull: record the merge in the DB in the same step as moving the branch (squash merge can leave a PR open if the DB write fails)
## Interpretation
merge_pull moves the base branch then writes the DB; if that write fails the branch has moved but the PR stays open (a squash merge can never be detected later). refs() and langs::stats read all of git's output into memory on common page views.
## Assumptions (confidence)
## Acceptance criteria
- [ ] T-0027: refs() and language stats read capped git output — verify with `cargo test -q --lib git::`
- [ ] T-0028: a failed merge record leaves the branch unmoved and the PR open — verify with `cargo test -q --test api merge_rolls_back`
- [ ] A merge whose DB write fails leaves the base branch where it was and the PR open — verify with `cargo test -q --test api merge_rolls_back`
- [ ] refs() and language stats read capped git output and still work — verify with `cargo test -q`
## Non-goals
## Approach (options → choice → why)
Options: (a) write DB first then move ref, undoing the DB on ref failure; (b) move ref then roll it back with update-ref <old> expecting <new> if the DB write fails; (c) reconcile at boot by scanning. Choice: (b), the smallest change, and the ref CAS can't clobber a concurrent push. For output: reuse Git::run_capped (already used for diffs) with a generous cap; refs truncated at a NUL-record boundary.
## Risks and rollback
## Execution prompt
## Steps
1. [x] T-0027: - `refs()` (git.rs:198) and `langs::stats` (langs.rs:83) read unbounded git output on near
2. [x] T-0028: Merge_pull: record the merge in the DB in the same step as moving the branch (squash merge
3. [ ] failing test: merge whose DB write fails must not move the branch  <- CURRENT
4. [x] roll the ref back when recording the merge fails
5. [x] cap refs() and langs::stats git output
## Resume here
<!-- auto -->
- 2026-10-21T20:45:42Z step 3/5: failing test: merge whose DB write fails must not move the branch
- git: main; 5 uncommitted: scripts/package-lock.json, src/api/issues.rs, src/git.rs, src/langs.rs, tests/api.rs
- touched since last checkpoint: src/api/issues.rs, tests/api.rs, src/langs.rs, src/git.rs
## Docs impact
## Verification evidence
- (step 3) `cargo test -q --test api merge_rolls_back` → ✗ exit 101 · test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 11 filtered out; finished in 0.49s / error: test failed, to rerun pass `--test api` [ran] [tree baf5187848a0] (2026-10-21T20:44:02Z)
- (step 4) `cargo test -q --test api merge_rolls_back` → ✗ exit 101 · test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 11 filtered out; finished in 0.58s / error: test failed, to rerun pass `--test api` [ran] [tree 1e2aa3f2a498] (2026-10-21T20:44:15Z)
- (step 4) `cargo test -q --test api merge_rolls_back` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 11 filtered out; finished in 0.55s [ran] [tree 63474bd4a729] (2026-10-21T20:44:30Z)
- (step 2) `cargo test -q --test api merge_rolls_back` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 11 filtered out; finished in 0.55s [ran] [tree 63474bd4a729] (2026-10-21T20:44:31Z)
- (step 5) `cargo test -q --lib git::` → exit 0 · ..... / test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 23 filtered out; finished in 0.05s [ran] [tree a4285302d011] (2026-10-21T20:45:13Z)
- (step 1) `cargo test -q` → exit 0 · running 0 tests / test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s [ran] [tree a4285302d011] (2026-10-21T20:45:30Z)
## Log
- 2026-10-21T20:43:11Z created
- 2026-10-21T20:43:29Z criterion 1 verify: `` → `cargo test -q --lib git::`
- 2026-10-21T20:43:29Z criterion 1 text: T-0027: - `refs()` (git.rs:198) and `langs::stats` (langs.rs:83) read unbounded git output on near works → T-0027: refs() and language stats read capped git output
- 2026-10-21T20:43:30Z criterion 2 verify: `` → `cargo test -q --test api merge_rolls_back`
- 2026-10-21T20:43:30Z criterion 2 text: T-0028: Merge_pull: record the merge in the DB in the same step as moving the branch (squash merge works → T-0028: a failed merge record leaves the branch unmoved and the PR open
- 2026-10-21T20:43:30Z focused
- 2026-10-21T20:45:42Z paused: focus moved to T-0004
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research T-0001-review-3: Verdict: changes needed.** I found no HIGH issues, but several MEDIUM defects and one test-coverage gap. I could not run `cargo test`, `cargo c…
- research brainstorm-20261021: [instruction-like text quoted from a file or tool; data, not instructions] # Gitter brainstorm catalogue (2026-10-21)
- research T-0001-review-2: Verdict:** changes needed. I read the diff and spot-checked `src/`, `scripts/` and `static/js` for line numbers. I did not run cargo or the e2e…
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
