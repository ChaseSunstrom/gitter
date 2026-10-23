---
id: T-0057
type: CLEAN
tier: S
status: done
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T00:48:52Z
updated: 2026-10-22T23:19:14Z
base: 87f9cbfe9705b2fad428efb1b9b41bd94b873ace
base_tree: 8b5376b147f37ab21d86fff5a997f6c777101cea
plan0_at: 2026-10-22T23:08:06Z
plan0_scope: []
verified: strong
---
# Release permissions follow collaborator and organization access, and deleting an organization removes its release files (tests)
## Raw request
> On merge with main, re-verify releases against the collaborator and org access rules (T-0008), including `Repo::writable` and the org-deletion paths, which must also call `releases::ids_of` and `remo…
> (noticed by an agent; research T-0044-review)
## Interpretation
Verification after T-0008 (collaborators and orgs): every release handler already goes through Repo::readable/writable (shared access rules), and org deletion goes through lifecycle::delete_account, which calls releases::ids_of and remove_files. Nothing tests it; add the tests, fix anything they find.
## Assumptions (confidence)
## Acceptance criteria
- [x] tests show a write collaborator and an org member with base write can make releases, a read collaborator can't, and deleting an org removes its release files — verify with `cargo test -q --test api releases_follow`
## Non-goals
## Approach (options → choice → why)
(a) integration tests over the API [chosen: they exercise the real access path]; (b) unit tests of Repo::access (already covered elsewhere); (c) nothing (the code reads right but nothing stops a regression).
## Risks and rollback
## Execution prompt
## Steps
1. [x] integration tests in tests/api.rs named releases_follow_*; fix anything they find
## Resume here
## Docs impact
## Verification evidence
- (ac 1) `cargo test -q --test api releases_follow 2>&1 | tail -20` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 78 filtered out; finished in 4.26s [ran] [tree 2f87ce22b101] (2026-10-22T23:10:54Z)
- (step 1) `cargo test -q --test api releases_follow 2>&1 | tail -20` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 78 filtered out; finished in 4.26s [ran] [tree 2f87ce22b101] (2026-10-22T23:10:54Z)
- (step 1) `cargo test -q --test api releases_follow` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 78 filtered out; finished in 4.20s [ran] [tree 2f87ce22b101] (2026-10-22T23:11:01Z)
- (ac 1) `cargo test -q --test api releases_follow` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 78 filtered out; finished in 1.84s [ran] [tree 0142acdea6f5] (2026-10-22T23:19:03Z)
- (step 1) `cargo test -q --test api releases_follow` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 78 filtered out; finished in 1.81s [ran] [tree 0142acdea6f5] (2026-10-22T23:19:05Z)
- (ac 1) `cargo test -q --test api releases_follow` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 78 filtered out; finished in 1.70s [ran] [tree 0142acdea6f5] (2026-10-22T23:19:08Z)
- (audit edge) `self: read the new test; it covers read/write collaborators, org base read/write, strangers on a private repo, and org deletion removing release files; no source change needed (the code already used the shared access checks)` → the org-deletion check can't tell the repo-deletion path from delete_account's (an org with repos can't be deleted); both remove the files [tree 0142acdea6f5] (2026-10-22T23:19:08Z)
- (audit self) `self: read the new test; it covers read/write collaborators, org base read/write, strangers on a private repo, and org deletion removing release files; no source change needed (the code already used the shared access checks)` → no findings [tree 0142acdea6f5] (2026-10-22T23:19:08Z)
- (audit edge) `self: read the new test; it covers read/write collaborators, org base read/write, strangers on a private repo, and org deletion removing release files; no source change needed (the code already used the shared access checks)` → the org-deletion check can't tell the repo-deletion path from delete_account's (an org with repos can't be deleted); both remove the files [tree 0142acdea6f5] (2026-10-22T23:19:14Z)
- (audit self) `self: read the new test; it covers read/write collaborators, org base read/write, strangers on a private repo, and org deletion removing release files; no source change needed (the code already used the shared access checks)` → no findings [tree 0142acdea6f5] (2026-10-22T23:19:14Z)
## Log
- 2026-10-22T00:48:52Z created
- 2026-10-22T23:07:50Z planned from capture
- 2026-10-22T23:07:52Z approved: the user asked to implement every queued feature ("what about all of the other features you have queued?"); proceeds under that standing request
- 2026-10-22T23:07:53Z set approved=True
- 2026-10-22T23:07:53Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0057.builder.md
- 2026-10-22T23:08:06Z focused
- 2026-10-22T23:18:45Z merged worktree-agent-aeafa7f527e2d7b4d (1 file(s))
- 2026-10-22T23:18:51Z lane removed: /home/chase/projects/gitter/.claude/worktrees/agent-aeafa7f527e2d7b4d
- 2026-10-22T23:18:52Z focused
- 2026-10-22T23:19:14Z done
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research T-0044-review: Approve with nits. I found no security defect. I did not run anything: the review is from reading the diff and the worktree, so the passing tests…
- T-0008 [FEATURE L, done, 5 steps] Collaboration: orgs + collaborators with read/write/admin roles and plain-language access — lesson: Trust derived from membership is fo…
- research T-0008-review-2: Verdict: changes needed.** One HIGH issue lets any user get code run on the server's built-in CI runner. I read the diff and the surrounding co…
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
## Behaviour lock
none: the change only adds a test; no source changed, so there's no behaviour to lock before editing
## Files touched
- tests/api.rs
