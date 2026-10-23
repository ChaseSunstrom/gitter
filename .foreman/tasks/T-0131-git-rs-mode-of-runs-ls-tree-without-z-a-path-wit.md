---
id: T-0131
type: FIX
tier: S
status: done
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-23T01:42:01Z
updated: 2026-10-23T02:03:41Z
base: c46da8615ab942ef8b8ae00079a55f359768782c
base_tree: 55ec9151954f2221b5a4fec3650fd429fa535a68
plan0_at: 2026-10-23T01:57:26Z
plan0_scope: []
verified: strong
---
# mode_of handles file names git quotes
## Raw request
> git.rs mode_of runs ls-tree without -z: a path with a quote, backslash or newline comes back quoted, so its mode is never found (an executable file edited in the browser loses its mode). Use -z and split on NUL, as collab.rs file_at does.
## Interpretation
## Assumptions (confidence)
## Acceptance criteria
- [x] editing an executable file whose name has a quote keeps its exec bit — verify with `cargo test -q --test api web_commits_keep_exec_bits`
## Non-goals
## Approach (options → choice → why)
## Risks and rollback
## Execution prompt
## Steps
1. [x] test: extend the exec-bit test with a quoted name; fix: ls-tree -z in git.rs mode_of
## Resume here
## Docs impact
## Verification evidence
- (step 1) `cargo test -q --test api web_commits_keep_exec_bits` → red as expected (exit 101) · test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 85 filtered out; finished in 0.35s / error: test failed, to rerun pass `--test api` [ran] [tree 0d1731d2d786] (2026-10-23T01:57:59Z)
- (step 1) `cargo test -q --test api web_commits_keep_exec_bits` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 85 filtered out; finished in 0.68s [ran] [tree 2e5da18fa18a] (2026-10-23T01:58:38Z)
- (ac 1) `cargo test -q --test api web_commits_keep_exec_bits` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 85 filtered out; finished in 0.68s [ran] [tree 2e5da18fa18a] (2026-10-23T01:58:39Z)
- `cargo test -q --test api web_commits_keep_exec_bits` → ✗ exit 101 · test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 85 filtered out; finished in 0.35s / error: test failed, to rerun pass `--test api` (start tree + this task's tests) [ran] [tree 2e5da18fa18a] (2026-10-23T01:59:41Z)
- `cargo test -q --test api web_commits_keep_exec_bits` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 85 filtered out; finished in 0.35s [ran] [tree 2e5da18fa18a] (2026-10-23T01:59:41Z)
- (ac 1) `cargo test -q --test api web_commits_keep_exec_bits` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 85 filtered out; finished in 0.68s [ran] [tree 2e5da18fa18a] (2026-10-23T02:03:20Z)
- (audit self) `self: ls-tree -z is the only change; mode_of's three callers (blob view, web commits, review suggestions) take the same path; no other ls-tree parses names without -z except ones that already use -z (grep)` → no findings [tree 2e5da18fa18a] (2026-10-23T02:03:20Z)
## Log
- 2026-10-23T01:42:01Z created
- 2026-10-23T01:57:25Z planned from capture
- 2026-10-23T01:57:26Z focused
- 2026-10-23T01:57:44Z criterion 1 verify: `cargo test -q --test api contents_keep` → `cargo test -q --test api web_commits_keep_exec_bits`
- 2026-10-23T02:03:21Z done
- 2026-10-23T02:03:22Z committed b0684a8 (4 files)
- 2026-10-23T02:03:41Z correction: the audit's 'no other' was wrong — log --name-only (code.rs:196) and diff --name-only (reviews.rs:168) also parse quoted names; fixed in a follow-up
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research T-0022-review-1: Verdict: changes needed.** The brief gave no acceptance criteria beyond the task title, so I checked the code against that. I read the code and…
- research T-0122-review: Verdict:** approve with nits. I found no HIGH or CRITICAL defect. I had no shell, so this is a code reading and I did not run the tests.
- playbook Shell script errors (bash -n, ShellCheck, bats) (skills/playbooks/references/build/shell-build.md)
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
## Files touched
- tests/api.rs
- src/git.rs
