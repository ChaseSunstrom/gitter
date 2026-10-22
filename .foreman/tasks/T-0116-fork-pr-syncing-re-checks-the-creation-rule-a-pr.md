---
id: T-0116
type: SECURITY
tier: S
status: done
priority: normal
scope: []
depends_on: []
source: user
created: 2026-10-22T22:01:02Z
updated: 2026-10-22T22:04:48Z
base: 826af23e438a44d0c2084f736ba5e0fe7a940ca3
base_tree: 037fd34f1871545ffb4f5b4d67011a8399123f77
plan0_at: 2026-10-22T22:01:03Z
plan0_scope: []
verified: strong
---
# Fork PR syncing re-checks the creation rule: a private fork's commits are copied only while the PR author can push to it
## Raw request
> Fork PR syncing re-checks the creation rule: a private fork's commits are copied only while the PR author can push to it
## Interpretation
## Assumptions (confidence)
## Acceptance criteria
- [x] pushes to a fork that went private no longer reach the base unless the author can push to it — verify with `cargo test -q --test forks`
## Non-goals
## Approach (options → choice → why)
## Risks and rollback
## Execution prompt
## Steps
1. [x] test (fork goes private, author loses access, push → refs/pull unchanged), then the check in sync_fork_pulls and refresh_fork_head
## Resume here
## Docs impact
## Verification evidence
- (step 1) `cargo test -q --test forks gone_private 2>&1 | grep -E 'panicked|test result' | head -2` → red as expected (exit 1) · test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 3 filtered out; finished in 2.15s / fm: the output reports failing tests but the command exited 0 (a pipe such as | tail hides the exit) [ran] [tree a23b493a7a16] (2026-10-22T22:01:31Z)
- (step 1) `cargo test -q --test forks` → exit 0 · .... / test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.21s [ran] [tree 677190cf159f] (2026-10-22T22:04:28Z)
- (ac 1) `cargo test -q --test forks` → exit 0 · .... / test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.15s [ran] [tree 677190cf159f] (2026-10-22T22:04:34Z)
- (ac 1) `cargo test -q --test forks` → exit 0 · .... / test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.06s [ran] [tree 677190cf159f] (2026-10-22T22:04:47Z)
- (audit adversary) `self + adversary: the opening rule (private fork needs the author's push access) is now re-checked by one helper before every copy (push sync and reopen); fails closed on any lookup error; maintainer pushes already require the author's write; 3 mutants killed` → a fork made private, or an author who lost access, no longer feeds the base; deleted forks are skipped by the reopen query and can't be pushed [tree 677190cf159f] (2026-10-22T22:04:47Z)
- (audit self) `self + adversary: the opening rule (private fork needs the author's push access) is now re-checked by one helper before every copy (push sync and reopen); fails closed on any lookup error; maintainer pushes already require the author's write; 3 mutants killed` → no findings [tree 677190cf159f] (2026-10-22T22:04:47Z)
## Log
- 2026-10-22T22:01:02Z created
- 2026-10-22T22:01:03Z focused
- 2026-10-22T22:04:47Z done
- 2026-10-22T22:04:48Z committed 6e807da (5 files)
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- T-0051 [FEATURE L, done, 4 steps] Cross-repository pull requests from forks: open a PR from bob/hello:feature into alice/hel — lesson: A permission one user grants on be…
- research T-0051-review-4: Verdict: approve with nits.** I checked each listed fix against the working tree and found no way around the CRITICAL one. The new findings are…
- research T-0051-review-3: Pre-audit flags: the secret-looking values in `scripts/ui-check.mjs` and `tests/forks.rs` are test passwords and runtime tokens, and the `conso…
- research T-0051-review-1: Verdict: changes needed.** Maintainers can be granted push access to branches of a repository the PR author has no rights on. I confirmed this …
## Lessons
- A rule checked when something is created must be checked again wherever the thing keeps acting on its own later (syncs, retries), not only at creation
## Files touched
- tests/forks.rs
- src/api/issues.rs
