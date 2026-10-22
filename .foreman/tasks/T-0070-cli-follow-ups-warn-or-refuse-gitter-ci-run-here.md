---
id: T-0070
type: FIX
tier: S
status: done
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T02:26:50Z
updated: 2026-10-22T22:09:20Z
base: 826af23e438a44d0c2084f736ba5e0fe7a940ca3
base_tree: 196aa9606fb1cd1ee98d60f1f688d00032f7b0aa
plan0_at: 2026-10-22T22:00:31Z
plan0_scope: []
verified: strong
---
# CLI safety follow-ups: ci run --here refuses image jobs, pr create -R elsewhere needs --head, warn on plain-http logins to other machines
## Raw request
> CLI follow-ups: warn or refuse 'gitter ci run --here' for image jobs (it mounts and chowns the real tree); 'pr create -R other/repo' should require --head when -R doesn't match the cwd remote; warn on non-loopback http:// logins (from T-0013 review)
## Interpretation
## Assumptions (confidence)
## Acceptance criteria
- [x] the three cases behave as stated — verify with `cargo test -q --test cli`
## Non-goals
## Approach (options → choice → why)
## Risks and rollback
## Execution prompt
## Steps
1. [x] tests for the three cases, then the checks in src/cli.rs
## Resume here
<!-- auto -->
- 2026-10-22T22:01:02Z step 1/1: tests for the three cases, then the checks in src/cli.rs
- git: main; 0 uncommitted
## Docs impact
README (ci run --here, -R with pr create, plain-http warning) and the CLI help line
## Verification evidence
- (step 1) `cargo test -q --test cli` → red as expected (exit 101) · test result: FAILED. 1 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.08s / error: test failed, to rerun pass `--test cli` [ran] [tree 9ad16c9392a2] (2026-10-22T22:05:26Z)
- (step 1) `cargo test -q --test cli && cargo test -q --lib plain_http` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 113 filtered out; finished in 0.00s [ran] [tree acb643703013] (2026-10-22T22:08:00Z)
- (ac 1) `cargo test -q --test cli` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.07s [ran] [tree acb643703013] (2026-10-22T22:08:05Z)
- (ac 1) `cargo test -q --test cli` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.16s [ran] [tree acb643703013] (2026-10-22T22:08:10Z)
- (audit self) `self: each of the three checks has a failing-then-passing test and was mutation-checked (3 killed); the -R check compares server and repository with this folder's remote, so -R naming the same repository still works; IPv6 brackets and loopback ranges handled` → no findings [tree acb643703013] (2026-10-22T22:08:10Z)
- (step 1) `cargo test -q --test cli` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.03s [ran] [tree acb643703013] (2026-10-22T22:08:19Z)
- (audit self) `self: each of the three checks has a failing-then-passing test and was mutation-checked (3 killed); the -R check compares server and repository with this folder's remote, so -R naming the same repository still works; IPv6 brackets and loopback ranges handled` → no findings [tree acb643703013] (2026-10-22T22:08:20Z)
- `cargo test -q --test cli` → ✗ exit 101 · test result: FAILED. 1 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.55s / error: test failed, to rerun pass `--test cli` (start tree + this task's tests) [ran] [tree acb643703013] (2026-10-22T22:09:16Z)
- `cargo test -q --test cli` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.52s [ran] [tree acb643703013] (2026-10-22T22:09:16Z)
- (audit self) `self: each of the three checks has a failing-then-passing test and was mutation-checked (3 killed); the -R check compares server and repository with this folder's remote, so -R naming the same repository still works; IPv6 brackets and loopback ranges handled` → no findings [tree acb643703013] (2026-10-22T22:09:19Z)
## Log
- 2026-10-22T02:26:50Z created
- 2026-10-22T22:00:31Z planned from capture
- 2026-10-22T22:00:31Z focused
- 2026-10-22T22:01:03Z paused: focus moved to T-0116
- 2026-10-22T22:04:54Z re-based: what other work changed while it was paused isn't its own change
- 2026-10-22T22:04:54Z focused
- 2026-10-22T22:09:19Z done
- 2026-10-22T22:09:20Z committed a6c62e6 (5 files)
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research T-0013-review: Verdict:** approve with nits. I found no token leak and no change to server-side runner behaviour. The findings below are two MEDIUM and several …
- research T-0001-review-2: Verdict:** changes needed. I read the diff and spot-checked `src/`, `scripts/` and `static/js` for line numbers. I did not run cargo or the e2e…
- research T-0004-review: Verdict: changes needed.** None of these block the four acceptance scripts. Findings 1, 2 and 3 break the documented Docker workflow. I verified …
- research T-0083-review: Verdict: approve with nits.** I found no SSRF bypass, credential leak or unguarded ref-writing path. There are two MEDIUM issues (unbounded fetch…
## Files touched
- src/cli.rs
- tests/cli.rs
- README.md
