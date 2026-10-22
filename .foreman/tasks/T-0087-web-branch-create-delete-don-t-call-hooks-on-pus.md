---
id: T-0087
type: FIX
tier: M
status: done
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T15:59:35Z
updated: 2026-10-22T19:10:24Z
base: 0389685efe0c0f417f4d227d2feb445093c77624
base_tree: 3490cafdae47b1f078f0f3490fe1c5380f65f5a3
plan0_at: 2026-10-22T18:44:29Z
plan0_scope: []
verified: strong
---
# Web branch create/delete don't call hooks::on_push (CI, webhooks, merge queue never see th
## Raw request
> Web branch create/delete don't call hooks::on_push (CI, webhooks, merge queue never see them); releases::make_tag reports every update_ref failure as 'created by someone else'
## Interpretation
Creating or deleting a branch on the web (POST/DELETE /api/repos/O/R/branches) moves a ref like a push does, but only push mirrors hear about it: CI, push webhooks, branch pushers, the merge queue and size refresh never do. Route both through hooks::on_push with a RefUpdate (old None for a new branch, new None for a deletion) as merges and web commits already do, dropping the direct mirrors::changed calls it replaces. Also, releases::make_tag reports every update_ref failure as 'created by someone else'; only an existing tag should say that, other failures should surface as errors.
## Assumptions (confidence)
## Acceptance criteria
- [x] A branch created on the web starts a push run (when the pipeline wants it) and sends a push webhook; a deleted one sends a deletion webhook and is forgotten by branch pushers; a tag that already exists is a conflict, other tag failures aren't reported as one — verify with `cargo test -q --test api web_branch`
## Non-goals
## Approach (options → choice → why)
(a) call hooks::on_push from create_branch and delete_branch with the right RefUpdate [chosen: one path for every ref move, as web commits and merges use]; (b) call each consumer separately (misses the next one added). make_tag: check whether the tag exists after a failed update_ref and report a conflict only then.
## Risks and rollback
## Execution prompt
## Steps
1. [x] failing tests for web branch create/delete hooks and make_tag errors
2. [x] route create/delete through hooks::on_push; make_tag error mapping
## Resume here
## Docs impact
none: behavior now matches what the README already says about pushes
## Verification evidence
- (step 1) `CARGO_INCREMENTAL=0 cargo test -q --test api web_branch` → ✗ exit 101 · test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 72 filtered out; finished in 0.67s / error: test failed, to rerun pass `--test api` [ran] [tree 8dfe732b1f82] (2026-10-22T18:50:29Z)
- (step 2) `CARGO_INCREMENTAL=0 cargo test -q --test api web_branch` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 72 filtered out; finished in 0.42s [ran] [tree 87bc3e36524a] (2026-10-22T18:52:33Z)
- (ac 1) `cargo test -q --test api web_branch` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 72 filtered out; finished in 0.37s [ran] [tree 742a33e7c3f0] (2026-10-22T19:10:17Z)
- (step 2) `cargo test -q --test api web_branch` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 72 filtered out; finished in 0.38s [ran] [tree 742a33e7c3f0] (2026-10-22T19:10:19Z)
- (ac 1) `cargo test -q --test api web_branch` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 72 filtered out; finished in 0.43s [ran] [tree 742a33e7c3f0] (2026-10-22T19:10:24Z)
- (audit intent) `self-review of the 194-line lane diff (create/delete branch through hooks::on_push, make_tag error mapping, two tests); merged with main and the full suite passed in the lane` → met — web branch create/delete now reach CI, push webhooks, branch pushers and the merge queue like pushes [tree 742a33e7c3f0] (2026-10-22T19:10:24Z)
- (audit edge) `self-review of the 194-line lane diff (create/delete branch through hooks::on_push, make_tag error mapping, two tests); merged with main and the full suite passed in the lane` → second create is a 409, second delete is a no-op without a webhook; tag clashes with another tag name return a 500 rather than a clear 409 (captured as T-0101) [tree 742a33e7c3f0] (2026-10-22T19:10:24Z)
## Log
- 2026-10-22T15:59:35Z created
- 2026-10-22T18:44:19Z set type=FIX
- 2026-10-22T18:44:20Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0087.builder.md
- 2026-10-22T18:44:29Z focused
- 2026-10-22T19:07:45Z merged worktree-agent-a4c5fcb5c8188dfae (4 file(s))
- 2026-10-22T19:07:46Z lane removed: /home/chase/projects/gitter/.claude/worktrees/agent-a4c5fcb5c8188dfae
- 2026-10-22T19:09:53Z focused
- 2026-10-22T19:10:24Z done
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research T-0017-review: Verdict: changes needed.** I found no way for a non-admin to merge a PR whose required checks haven't passed on the exact landed commit. The `lan…
- research T-0044-review: Approve with nits. I found no security defect. I did not run anything: the review is from reading the diff and the worktree, so the passing tests…
- research T-0024-recon: 1. **Answer [stale: src/api/issues.rs, src/db.rs, src/hooks.rs changed since it was written]
- research T-0001-review-3: Verdict: changes needed.** I found no HIGH issues, but several MEDIUM defects and one test-coverage gap. I could not run `cargo test`, `cargo c…
## Why not caught
no test created a branch on the web and looked for its CI run or webhook; web_branch_* tests do now
## Lessons
- Every way a ref moves should go through the one push hook; adding a consumer to it then covers web edits, merges and pushes alike
## Plan gaps
- failed first: step 1 failing tests for web branch create/delete hooks and make_tag errors
## Files touched
- tests/api.rs
- src/api/releases.rs
- src/git.rs
- src/api/repos.rs
- README.md
- src/api/ci.rs
- src/ci/compat.rs
- src/ci/config.rs
- src/ci/mod.rs
- src/ci/trigger.rs
- static/js/pages/ci.js
- tests/ci.rs
- src/api/notifications.rs
- src/main.rs
- tests/notifications.rs
