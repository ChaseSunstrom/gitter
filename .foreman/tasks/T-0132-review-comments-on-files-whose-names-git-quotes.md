---
id: T-0132
type: FIX
tier: S
status: done
priority: normal
scope: []
depends_on: []
source: user
created: 2026-10-23T02:03:59Z
updated: 2026-10-23T02:08:36Z
base: c6966f03148eeb2db287612aad8b7294e803970c
base_tree: 3f89264f1d60e3d40aaaf92858ca0a2a75d3e916
plan0_at: 2026-10-23T02:04:00Z
plan0_scope: []
verified: strong
---
# Review comments on files whose names git quotes
## Raw request
> Review comments on files whose names git quotes
## Interpretation
## Assumptions (confidence)
## Acceptance criteria
- [x] a review comment on a changed file named with a quote is accepted — verify with `cargo test -q --test api review_comments_on_quoted_names`
## Non-goals
## Approach (options → choice → why)
## Risks and rollback
## Execution prompt
## Steps
1. [x] test: PR changing a file named with a quote, comment on it; fix: unquote the diff --name-only lines in reviews.rs
## Resume here
## Docs impact
## Verification evidence
- (step 1) `cargo test -q --test api review_comments_on_quoted_names` → red as expected (exit 101) · test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 86 filtered out; finished in 0.66s / error: test failed, to rerun pass `--test api` [ran] [tree 9211f690325e] (2026-10-23T02:04:19Z)
- (step 1) `cargo test -q --test api review_comments_on_quoted_names` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 86 filtered out; finished in 0.70s [ran] [tree 113029eb4001] (2026-10-23T02:04:43Z)
- (ac 1) `cargo test -q --test api review_comments_on_quoted_names` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 86 filtered out; finished in 0.68s [ran] [tree 113029eb4001] (2026-10-23T02:04:45Z)
- `cargo test -q --test api review_comments_on_quoted_names` → ✗ exit 101 · test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 86 filtered out; finished in 0.64s / error: test failed, to rerun pass `--test api` (start tree + this task's tests) [ran] [tree 113029eb4001] (2026-10-23T02:05:52Z)
- `cargo test -q --test api review_comments_on_quoted_names` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 86 filtered out; finished in 0.67s [ran] [tree 113029eb4001] (2026-10-23T02:05:52Z)
- (ac 1) `cargo test -q --test api review_comments_on_quoted_names` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 86 filtered out; finished in 0.35s [ran] [tree 113029eb4001] (2026-10-23T02:08:36Z)
- (audit self) `self: grep of every git name listing in src (ls-tree, log/diff --name-only/--name-status): all use -z or diff::unquote now; this was the last plain one` → no findings [tree 113029eb4001] (2026-10-23T02:08:36Z)
## Log
- 2026-10-23T02:03:59Z created
- 2026-10-23T02:04:00Z focused
- 2026-10-23T02:08:36Z done
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- T-0131 [FIX S, done, 1 steps] mode_of handles file names git quotes
- research T-0122-review: Verdict:** approve with nits. I found no HIGH or CRITICAL defect. I had no shell, so this is a code reading and I did not run the tests.
- playbook Shell script errors (bash -n, ShellCheck, bats) (skills/playbooks/references/build/shell-build.md)
- research T-0097-review: Verdict: changes needed.** Static read only: I ran no code. The "secret-looking value" pre-audit hit is a false positive. `compat.rs:1108` assert…
- the similar finished task took 1 steps
- start here (files T-0131 touched): tests/api.rs, src/git.rs
## Files touched
- tests/api.rs
- src/api/reviews.rs
