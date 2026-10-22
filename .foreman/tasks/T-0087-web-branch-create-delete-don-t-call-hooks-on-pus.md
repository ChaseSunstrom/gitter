---
id: T-0087
type: FIX
tier: M
status: active
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T15:59:35Z
updated: 2026-10-22T18:52:33Z
builder: 2026-10-22T18:44:20Z
builder_proc: archbox-hardened|pid:[4026531836]|4747|8111
lane: /home/chase/projects/gitter/.claude/worktrees/agent-a4c5fcb5c8188dfae
lane_branch: worktree-agent-a4c5fcb5c8188dfae
base: 3f1062a8e68b7f5a983ae468076379016ce4623e
base_tree: 3e3611341a05e1eca090276633f232b64ee0d0ed
plan0_at: 2026-10-22T18:44:29Z
plan0_scope: []
---
# Web branch create/delete don't call hooks::on_push (CI, webhooks, merge queue never see th
## Raw request
> Web branch create/delete don't call hooks::on_push (CI, webhooks, merge queue never see them); releases::make_tag reports every update_ref failure as 'created by someone else'
## Interpretation
Creating or deleting a branch on the web (POST/DELETE /api/repos/O/R/branches) moves a ref like a push does, but only push mirrors hear about it: CI, push webhooks, branch pushers, the merge queue and size refresh never do. Route both through hooks::on_push with a RefUpdate (old None for a new branch, new None for a deletion) as merges and web commits already do, dropping the direct mirrors::changed calls it replaces. Also, releases::make_tag reports every update_ref failure as 'created by someone else'; only an existing tag should say that, other failures should surface as errors.
## Assumptions (confidence)
## Acceptance criteria
- [ ] A branch created on the web starts a push run (when the pipeline wants it) and sends a push webhook; a deleted one sends a deletion webhook and is forgotten by branch pushers; a tag that already exists is a conflict, other tag failures aren't reported as one — verify with `cargo test -q --test api web_branch`
## Non-goals
## Approach (options → choice → why)
(a) call hooks::on_push from create_branch and delete_branch with the right RefUpdate [chosen: one path for every ref move, as web commits and merges use]; (b) call each consumer separately (misses the next one added). make_tag: check whether the tag exists after a failed update_ref and report a conflict only then.
## Risks and rollback
## Execution prompt
## Steps
1. [ ] failing tests for web branch create/delete hooks and make_tag errors  <- CURRENT
2. [x] route create/delete through hooks::on_push; make_tag error mapping
## Resume here
## Docs impact
## Verification evidence
- (step 1) `CARGO_INCREMENTAL=0 cargo test -q --test api web_branch` → ✗ exit 101 · test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 72 filtered out; finished in 0.67s / error: test failed, to rerun pass `--test api` [ran] [tree 8dfe732b1f82] (2026-10-22T18:50:29Z)
- (step 2) `CARGO_INCREMENTAL=0 cargo test -q --test api web_branch` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 72 filtered out; finished in 0.42s [ran] [tree 87bc3e36524a] (2026-10-22T18:52:33Z)
## Log
- 2026-10-22T15:59:35Z created
- 2026-10-22T18:44:19Z set type=FIX
- 2026-10-22T18:44:20Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0087.builder.md
- 2026-10-22T18:44:29Z focused
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research T-0017-review: Verdict: changes needed.** I found no way for a non-admin to merge a PR whose required checks haven't passed on the exact landed commit. The `lan…
- research T-0044-review: Approve with nits. I found no security defect. I did not run anything: the review is from reading the diff and the worktree, so the passing tests…
- research T-0024-recon: 1. **Answer [stale: src/api/issues.rs, src/db.rs, src/hooks.rs changed since it was written]
- research T-0001-review-3: Verdict: changes needed.** I found no HIGH issues, but several MEDIUM defects and one test-coverage gap. I could not run `cargo test`, `cargo c…
