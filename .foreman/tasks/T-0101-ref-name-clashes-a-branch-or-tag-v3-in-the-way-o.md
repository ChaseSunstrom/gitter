---
id: T-0101
type: FIX
tier: M
status: active
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T19:03:54Z
updated: 2026-10-22T22:49:30Z
builder: 2026-10-22T22:42:05Z
builder_proc: archbox-hardened|pid:[4026531836]|4747|8111
lane: /home/chase/projects/gitter/.claude/worktrees/agent-a9bf0150e64c0871e
lane_branch: worktree-agent-a9bf0150e64c0871e
base: 6e1ef85012b118f0c31812348206b1ebc277b4cf
base_tree: f33bc7eef8ec3e3883ad8e7a3e322250ec4c6692
plan0_at: 2026-10-22T22:42:17Z
plan0_scope: []
---
# Ref name clashes say which ref is in the way (409), merges' branch deletions and web-made tags run the push hooks
## Raw request
> Ref name clashes (a branch or tag 'v3' in the way of 'v3/rc') give unclear errors: create_branch says the branch already exists, make_tag returns a 500; say which existing ref is in the way, as a 409. Also merge's head-branch deletion skips hooks::on_push (no deletion webhook) and web-made tags only notify push mirrors
## Interpretation
git refuses refs/heads/v3/rc while refs/heads/v3 exists (and the reverse). Today create_branch reports 'already exists' and make_tag a 500. Say which existing ref is in the way, as a 409. Separately, two ref changes skip hooks::on_push: the head-branch deletion after a merge (no deletion webhook) and tags made on the web (only push mirrors are told).
## Assumptions (confidence)
## Acceptance criteria
- [ ] creating branch v3/rc when v3 exists (or a tag the same way) answers 409 naming refs/heads/v3, never 500 — verify with `cargo test -q --test api ref_clash`
- [ ] deleting the head branch on merge and making a tag on the web send the push webhooks and run the push hooks like a git push — verify with `cargo test -q --test api ref_clash`
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
## Verification evidence
- (ac 1) `cargo test -q --test api ref_clash 2>&1 | tail -30` → ✗ exit 1 · error: test failed, to rerun pass `--test api` / ✗ exit 101 (76.2 s; last 18 of 18 lines) [ran] [tree 6b8a41605cc0] (2026-10-22T22:47:07Z)
- (ac 2) `cargo test -q --test api ref_clash 2>&1 | tail -30` → ✗ exit 1 · error: test failed, to rerun pass `--test api` / ✗ exit 101 (76.2 s; last 18 of 18 lines) [ran] [tree 6b8a41605cc0] (2026-10-22T22:47:07Z)
- (step 1) `cargo test -q --test api ref_clash` → ✗ exit 101 · test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 78 filtered out; finished in 0.41s / error: test failed, to rerun pass `--test api` [ran] [tree 6b8a41605cc0] (2026-10-22T22:47:12Z)
- (step 1) `cargo test -q --test api ref_clash` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 78 filtered out; finished in 0.47s [ran] [tree 97033f287f6f] (2026-10-22T22:49:29Z)
- (step 2) `cargo test -q --test api ref_clash` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 78 filtered out; finished in 0.48s [ran] [tree 97033f287f6f] (2026-10-22T22:49:30Z)
## Log
- 2026-10-22T19:03:54Z created
- 2026-10-22T22:42:01Z planned from capture
- 2026-10-22T22:42:02Z approved: the user asked to implement every queued feature ("what about all of the other features you have queued?"); proceeds under that standing request
- 2026-10-22T22:42:03Z set approved=True
- 2026-10-22T22:42:05Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0101.builder.md
- 2026-10-22T22:42:17Z focused
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- T-0087 [FIX M, done, 2 steps] Web branch create/delete don't call hooks::on_push (CI, webhooks, merge queue never see th — lesson: Every way a ref moves should go throug…
- research T-0051-review-4: Verdict: approve with nits.** I checked each listed fix against the working tree and found no way around the CRITICAL one. The new findings are…
- research T-0051-review-1: Verdict: changes needed.** Maintainers can be granted push access to branches of a repository the PR author has no rights on. I confirmed this …
- research T-0017-review: Verdict: changes needed.** I found no way for a non-admin to merge a PR whose required checks haven't passed on the exact landed commit. The `lan…
