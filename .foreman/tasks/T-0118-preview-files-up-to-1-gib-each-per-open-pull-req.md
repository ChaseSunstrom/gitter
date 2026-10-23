---
id: T-0118
type: FIX
tier: M
status: done
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T22:55:46Z
updated: 2026-10-23T03:41:21Z
group: storage quota accounting gaps
base: 87ffb1c0a29a3105209392c38bd7244cd31d6e32
base_tree: 7a6cfb8374e2dbcdb68fa1187f28b3569bb652ac
plan0_at: 2026-10-23T03:02:03Z
plan0_scope: []
verified: strong
---
# Pull request preview files count toward the owner's storage
## Raw request
> - Preview files (up to 1 GiB each, per open pull request) sit outside the owner storage quota, as artifacts do
> (noticed by an agent; research T-0018-review-2)
## Interpretation
Preview files for pull requests (src/api/preview.rs, up to 1 GiB each) sit outside the owner's storage quota, so an account at its limit can still fill the disk through previews. Their size should count in the owner's usage while they exist and be released when they're removed, and a preview bigger than the space left should be refused.
## Assumptions (confidence)
## Acceptance criteria
- [x] a published preview's bytes count in the owner's storage use (and are released when it's removed); a preview past the space left is refused — verify with `cargo test -q --test deploy preview`
## Non-goals
## Approach (options → choice → why)
Options: (1) record each preview's size in a table that usage() sums; (2) measure the previews folder in usage() on every call; (3) count previews in the repository's size_bytes. Choice: 1 if usage() already sums per-feature tables (LFS, releases), else 3: whichever matches how src/repo.rs usage() counts other stored files. Check need_room/storage_left before publishing (src/repo.rs).
## Risks and rollback
## Execution prompt
## Steps
1. [x] test: publish a preview with a small quota, usage grows and an oversized one is refused; fix: src/api/preview.rs publish checks need_room and records its size where usage() sums (src/repo.rs)
## Resume here
## Docs impact
README Deployments and previews: previews count toward the owner's storage
## Verification evidence
- (ac 1) `cargo test -q --test deploy preview 2>&1 | tail -30` → ✗ exit 1 · error: test failed, to rerun pass `--test deploy` / ✗ exit 101 (58.1 s; last 13 of 13 lines) [ran] [tree 56bf53820ffc] (2026-10-23T03:04:23Z)
- (step 1) `cargo test -q --test deploy preview` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 3 filtered out; finished in 8.67s [ran] [tree 036b55887049] (2026-10-23T03:06:48Z)
- `fm check: cargo test -q` → exit 0 · 1 passed: cargo test -q → exit 0 · running 0 tests / test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s [ran] [tree 036b55887049] (2026-10-23T03:09:05Z)
- (step 1) `cargo test -q --test deploy && cargo test -q --test api transfer` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 85 filtered out; finished in 2.65s [ran] [tree bd6e22b7af46] (2026-10-23T03:35:25Z)
- (ac 1) `cargo test -q --test deploy preview` → exit 0 · .... / test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 3 filtered out; finished in 9.64s [ran] [tree bd6e22b7af46] (2026-10-23T03:35:35Z)
- `cargo test -q --test deploy preview` → ✗ exit 101 · test result: FAILED. 2 passed; 2 failed; 0 ignored; 0 measured; 3 filtered out; finished in 9.40s / error: test failed, to rerun pass `--test deploy` (start tree + this task's tests) [ran] [tree bd6e22b7af46] (2026-10-23T03:37:01Z)
- `cargo test -q --test deploy preview` → exit 0 · .... / test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 3 filtered out; finished in 9.40s [ran] [tree bd6e22b7af46] (2026-10-23T03:37:01Z)
- (ac 1) `cargo test -q --test deploy preview` → exit 0 · .... / test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 3 filtered out; finished in 9.41s [ran] [tree 5e1f037e413c] (2026-10-23T03:41:19Z)
- (audit intent) `fm-reviewer (intent, edge, adversary): replacement previews not blocked by the one they replace, transfers count live previews, rows whose folder is gone stop counting, all fixed with tests; no way found for a preview to exceed the room` → preview bytes count while up and are released on close, retire or sweep [tree 5e1f037e413c] (2026-10-23T03:41:19Z)
- (audit edge) `fm-reviewer (intent, edge, adversary): replacement previews not blocked by the one they replace, transfers count live previews, rows whose folder is gone stop counting, all fixed with tests; no way found for a preview to exceed the room` → replacement, transfer, missing folder; two simultaneous publishes can overshoot as every need_room caller can [tree 5e1f037e413c] (2026-10-23T03:41:19Z)
- (audit adversary) `fm-reviewer (intent, edge, adversary): replacement previews not blocked by the one they replace, transfers count live previews, rows whose folder is gone stop counting, all fixed with tests; no way found for a preview to exceed the room` → sizes come from the server's own extraction, the room is checked before each file is written [tree 5e1f037e413c] (2026-10-23T03:41:19Z)
## Log
- 2026-10-22T22:55:46Z created
- 2026-10-23T03:01:15Z planned from capture
- 2026-10-23T03:01:21Z approved: the user asked to implement every queued feature (standing request)
- 2026-10-23T03:01:22Z set approved=True
- 2026-10-23T03:01:46Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0118.builder.md
- 2026-10-23T03:02:03Z focused
- 2026-10-23T03:34:18Z merged worktree-agent-ad2e67ab5a3ba0fd0 (7 file(s))
- 2026-10-23T03:34:30Z lane removed: /home/chase/projects/gitter/.claude/worktrees/agent-ad2e67ab5a3ba0fd0
- 2026-10-23T03:34:33Z focused
- 2026-10-23T03:41:20Z done
- 2026-10-23T03:41:21Z committed af21bbb (5 files)
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research T-0015-review: Verdict: changes needed.** The temp-file guard, the slow-upload check, the `Cache-Control` fixes and the batch limiter are sound. The storage-cap…
- research T-0018-review-2: Verdict** — changes needed. The diff file T-0018.diff is out of date against the working tree. I reviewed the working tree, so my line numbers …
- research T-0005-review: Verdict: changes needed.** Two HIGH findings: the quota bypass and the fork that undoes a hide. [stale: src/api/account.rs, src/api/code.rs, src/…
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
## Why not caught
none: a quota on previews didn't exist before this task
## Lessons
- Quota checks for a replacement must give back what the replaced thing holds, or the second push fails for ever
## Files touched
- tests/deploy.rs
- src/repo.rs
- src/db.rs
- src/api/preview.rs
- src/api/deploy.rs
- Cargo.lock
- Cargo.toml
- README.md
- scripts/e2e.sh
- scripts/ui-check.mjs
- src/api/account.rs
- src/api/admin.rs
- src/api/federation.rs
- src/api/issues.rs
- src/api/mod.rs
- src/api/repos.rs
- src/auth.rs
- src/federation.rs
- src/hooks.rs
- src/lib.rs
- src/ops.rs
- src/settings.rs
- static/js/pages/admin.js
- static/js/pages/issue.js
- static/js/pages/newrepo.js
- static/js/repohead.js
- tests/common/mod.rs
- tests/federation.rs
- tests/api.rs
- src/api/lifecycle.rs
