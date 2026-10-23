---
id: T-0118
type: FIX
tier: M
status: active
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T22:55:46Z
updated: 2026-10-23T03:09:05Z
group: storage quota accounting gaps
builder: 2026-10-23T03:01:46Z
builder_proc: archbox-hardened|pid:[4026531836]|16446|602195
lane: /home/chase/projects/gitter/.claude/worktrees/agent-ad2e67ab5a3ba0fd0
lane_branch: worktree-agent-ad2e67ab5a3ba0fd0
base: 6d58556833b5d9e8174ee5d6cead8a8d00059f31
base_tree: 19349ab6e6c5ad7d06cc81187bbb51ad52194726
plan0_at: 2026-10-23T03:02:03Z
plan0_scope: []
---
# Pull request preview files count toward the owner's storage
## Raw request
> - Preview files (up to 1 GiB each, per open pull request) sit outside the owner storage quota, as artifacts do
> (noticed by an agent; research T-0018-review-2)
## Interpretation
Preview files for pull requests (src/api/preview.rs, up to 1 GiB each) sit outside the owner's storage quota, so an account at its limit can still fill the disk through previews. Their size should count in the owner's usage while they exist and be released when they're removed, and a preview bigger than the space left should be refused.
## Assumptions (confidence)
## Acceptance criteria
- [ ] a published preview's bytes count in the owner's storage use (and are released when it's removed); a preview past the space left is refused — verify with `cargo test -q --test deploy preview`
## Non-goals
## Approach (options → choice → why)
Options: (1) record each preview's size in a table that usage() sums; (2) measure the previews folder in usage() on every call; (3) count previews in the repository's size_bytes. Choice: 1 if usage() already sums per-feature tables (LFS, releases), else 3: whichever matches how src/repo.rs usage() counts other stored files. Check need_room/storage_left before publishing (src/repo.rs).
## Risks and rollback
## Execution prompt
## Steps
1. [x] test: publish a preview with a small quota, usage grows and an oversized one is refused; fix: src/api/preview.rs publish checks need_room and records its size where usage() sums (src/repo.rs)
## Resume here
## Docs impact
## Verification evidence
- (ac 1) `cargo test -q --test deploy preview 2>&1 | tail -30` → ✗ exit 1 · error: test failed, to rerun pass `--test deploy` / ✗ exit 101 (58.1 s; last 13 of 13 lines) [ran] [tree 56bf53820ffc] (2026-10-23T03:04:23Z)
- (step 1) `cargo test -q --test deploy preview` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 3 filtered out; finished in 8.67s [ran] [tree 036b55887049] (2026-10-23T03:06:48Z)
- `fm check: cargo test -q` → exit 0 · 1 passed: cargo test -q → exit 0 · running 0 tests / test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s [ran] [tree 036b55887049] (2026-10-23T03:09:05Z)
## Log
- 2026-10-22T22:55:46Z created
- 2026-10-23T03:01:15Z planned from capture
- 2026-10-23T03:01:21Z approved: the user asked to implement every queued feature (standing request)
- 2026-10-23T03:01:22Z set approved=True
- 2026-10-23T03:01:46Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0118.builder.md
- 2026-10-23T03:02:03Z focused
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research T-0015-review: Verdict: changes needed.** The temp-file guard, the slow-upload check, the `Cache-Control` fixes and the batch limiter are sound. The storage-cap…
- research T-0018-review-2: Verdict** — changes needed. The diff file T-0018.diff is out of date against the working tree. I reviewed the working tree, so my line numbers …
- research T-0005-review: Verdict: changes needed.** Two HIGH findings: the quota bypass and the fork that undoes a hide. [stale: src/api/account.rs, src/api/code.rs, src/…
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
