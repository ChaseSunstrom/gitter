---
id: T-0101
type: FIX
tier: M
status: done
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T19:03:54Z
updated: 2026-10-22T23:21:19Z
base: 602591cf0085c4e59e59bc2d093dae7a8b7ceb82
base_tree: 0142acdea6f5d4dc20266333420cbc1f1de4cabd
plan0_at: 2026-10-22T22:42:17Z
plan0_scope: []
verified: strong
---
# Ref name clashes say which ref is in the way (409), merges' branch deletions and web-made tags run the push hooks
## Raw request
> Ref name clashes (a branch or tag 'v3' in the way of 'v3/rc') give unclear errors: create_branch says the branch already exists, make_tag returns a 500; say which existing ref is in the way, as a 409. Also merge's head-branch deletion skips hooks::on_push (no deletion webhook) and web-made tags only notify push mirrors
## Interpretation
git refuses refs/heads/v3/rc while refs/heads/v3 exists (and the reverse). Today create_branch reports 'already exists' and make_tag a 500. Say which existing ref is in the way, as a 409. Separately, two ref changes skip hooks::on_push: the head-branch deletion after a merge (no deletion webhook) and tags made on the web (only push mirrors are told).
## Assumptions (confidence)
## Acceptance criteria
- [x] creating branch v3/rc when v3 exists (or a tag the same way) answers 409 naming refs/heads/v3, never 500 — verify with `cargo test -q --test api ref_clash`
- [x] deleting the head branch on merge and making a tag on the web send the push webhooks and run the push hooks like a git push — verify with `cargo test -q --test api ref_clash`
## Non-goals
## Approach (options → choice → why)
(a) one helper mapping git's 'cannot lock ref'/'exists; cannot create' stderr to a 409 naming the blocking ref, found by listing refs that are a prefix or extension of the new one [chosen: precise message without parsing every git wording]; (b) pre-check with for-each-ref before every create (racy, duplicated); (c) generic 409 without the name (unhelpful). Hooks: build the RefUpdate (old sha → None, or None → new) and call crate::hooks::on_push like the other web writes do.
## Risks and rollback
## Execution prompt
## Steps
1. [x] tests, then one helper that turns git's ref lock/exists errors into a 409 naming the ref in the way, used by create_branch, make_tag and rename paths
2. [x] land_merge's branch deletion and web tag creation call hooks::on_push with their RefUpdate (webhook, mirrors, CI as for pushes)
## Resume here
## Docs impact
none: error messages and hooks; behaviour matches what the README already says about pushes and webhooks
## Verification evidence
- (ac 1) `cargo test -q --test api ref_clash 2>&1 | tail -30` → ✗ exit 1 · error: test failed, to rerun pass `--test api` / ✗ exit 101 (76.2 s; last 18 of 18 lines) [ran] [tree 6b8a41605cc0] (2026-10-22T22:47:07Z)
- (ac 2) `cargo test -q --test api ref_clash 2>&1 | tail -30` → ✗ exit 1 · error: test failed, to rerun pass `--test api` / ✗ exit 101 (76.2 s; last 18 of 18 lines) [ran] [tree 6b8a41605cc0] (2026-10-22T22:47:07Z)
- (step 1) `cargo test -q --test api ref_clash` → ✗ exit 101 · test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 78 filtered out; finished in 0.41s / error: test failed, to rerun pass `--test api` [ran] [tree 6b8a41605cc0] (2026-10-22T22:47:12Z)
- (step 1) `cargo test -q --test api ref_clash` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 78 filtered out; finished in 0.47s [ran] [tree 97033f287f6f] (2026-10-22T22:49:29Z)
- (step 2) `cargo test -q --test api ref_clash` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 78 filtered out; finished in 0.48s [ran] [tree 97033f287f6f] (2026-10-22T22:49:30Z)
- (ac 1) `cargo test -q --test api ref_clash 2>&1 | grep -v "^\s*$" | tail -25` → ✗ exit 1 · error: test failed, to rerun pass `--test api` / ✗ exit 101 (48.0 s; last 16 of 16 lines) [ran] [tree d1e7b5536dda] (2026-10-22T23:07:18Z)
- (ac 2) `cargo test -q --test api ref_clash 2>&1 | grep -v "^\s*$" | tail -25` → ✗ exit 1 · error: test failed, to rerun pass `--test api` / ✗ exit 101 (48.0 s; last 16 of 16 lines) [ran] [tree d1e7b5536dda] (2026-10-22T23:07:18Z)
- (step 1) `cargo test -q --test api ref_clash` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 78 filtered out; finished in 0.62s [ran] [tree 4f8974c1648b] (2026-10-22T23:16:49Z)
- (step 2) `cargo test -q --test api ref_clash` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 78 filtered out; finished in 0.63s [ran] [tree 4f8974c1648b] (2026-10-22T23:16:50Z)
- (ac 1) `cargo test -q --test api ref_clash` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 79 filtered out; finished in 1.12s [ran] [tree 17b16e49fa76] (2026-10-22T23:19:56Z)
- (ac 2) `cargo test -q --test api ref_clash` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 79 filtered out; finished in 0.96s [ran] [tree 17b16e49fa76] (2026-10-22T23:19:58Z)
- (step 1) `cargo test -q --test api ref_clash` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 79 filtered out; finished in 1.16s [ran] [tree 17b16e49fa76] (2026-10-22T23:20:00Z)
- (step 2) `cargo test -q --test api ref_clash` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 79 filtered out; finished in 0.97s [ran] [tree 17b16e49fa76] (2026-10-22T23:20:03Z)
- `cargo test -q --test api ref_clash` → ✗ exit 101 · For more information about this error, try `rustc --explain E0603`. / error: could not compile `gitter` (test "api") due to 2 previous errors (start tree + this task's tests) [ran] [tree 17b16e49fa76] (2026-10-22T23:21:00Z)
- `cargo test -q --test api ref_clash` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 79 filtered out; finished in 1.47s [ran] [tree 17b16e49fa76] (2026-10-22T23:21:00Z)
- (ac 1) `cargo test -q --test api ref_clash` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 79 filtered out; finished in 1.38s [ran] [tree 17b16e49fa76] (2026-10-22T23:21:19Z)
- (ac 2) `cargo test -q --test api ref_clash` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 79 filtered out; finished in 1.38s [ran] [tree 17b16e49fa76] (2026-10-22T23:21:19Z)
- (audit intent) `fm-reviewer on the lane (approve with nits); all six items fixed test-first by the builder; re-run on main: ref_clash 3 passed` → clashes answer 409 naming the blocking ref (branches, tags, drafts, web commits); merge deletions and web tags run the push hooks [tree 17b16e49fa76] (2026-10-22T23:21:19Z)
- (audit adversary) `fm-reviewer on the lane (approve with nits); all six items fixed test-first by the builder; re-run on main: ref_clash 3 passed` → ref names validated before reaching git; the blocker named is in the caller's own namespace [tree 17b16e49fa76] (2026-10-22T23:21:19Z)
- (audit edge) `fm-reviewer on the lane (approve with nits); all six items fixed test-first by the builder; re-run on main: ref_clash 3 passed` → a head branch moved during a merge is kept (update-ref -d with the merged sha); locked refs answer a retryable 409 [tree 17b16e49fa76] (2026-10-22T23:21:19Z)
- (audit operator) `fm-reviewer on the lane (approve with nits); all six items fixed test-first by the builder; re-run on main: ref_clash 3 passed` → deletion failures logged; tag announcements survive a client disconnect (spawned) [tree 17b16e49fa76] (2026-10-22T23:21:19Z)
- (audit maintainer) `fm-reviewer on the lane (approve with nits); all six items fixed test-first by the builder; re-run on main: ref_clash 3 passed` → one create_ref/ref_in_way helper in git.rs used by branches, tags and web commits [tree 17b16e49fa76] (2026-10-22T23:21:19Z)
## Log
- 2026-10-22T19:03:54Z created
- 2026-10-22T22:42:01Z planned from capture
- 2026-10-22T22:42:02Z approved: the user asked to implement every queued feature ("what about all of the other features you have queued?"); proceeds under that standing request
- 2026-10-22T22:42:03Z set approved=True
- 2026-10-22T22:42:05Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0101.builder.md
- 2026-10-22T22:42:17Z focused
- 2026-10-22T23:19:20Z merged worktree-agent-a9bf0150e64c0871e (6 file(s))
- 2026-10-22T23:19:26Z lane removed: /home/chase/projects/gitter/.claude/worktrees/agent-a9bf0150e64c0871e
- 2026-10-22T23:19:27Z focused
- 2026-10-22T23:21:19Z done
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- T-0087 [FIX M, done, 2 steps] Web branch create/delete don't call hooks::on_push (CI, webhooks, merge queue never see th — lesson: Every way a ref moves should go throug…
- research T-0051-review-4: Verdict: approve with nits.** I checked each listed fix against the working tree and found no way around the CRITICAL one. The new findings are…
- research T-0051-review-1: Verdict: changes needed.** Maintainers can be granted push access to branches of a repository the PR author has no rights on. I confirmed this …
- research T-0017-review: Verdict: changes needed.** I found no way for a non-admin to merge a PR whose required checks haven't passed on the exact landed commit. The `lan…
## Why not caught
these paths had no test that pushed during the merge or made a clashing ref
## Lessons
- Deleting a branch after a merge must name the sha it expects (update-ref -d ref old): anything else races with a push and deletes someone's new commits
## Plan gaps
- failed first: step 1 tests, then one helper that turns git's ref lock/exists errors into a 409 naming the ref in the way…
## Files touched
- tests/api.rs
- src/git.rs
- src/api/repos.rs
- src/api/code.rs
- src/api/releases.rs
- src/api/issues.rs
