---
id: T-0057
type: CLEAN
tier: S
status: active
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T00:48:52Z
updated: 2026-10-22T23:11:01Z
builder: 2026-10-22T23:07:53Z
builder_proc: archbox-hardened|pid:[4026531836]|4747|8111
lane: /home/chase/projects/gitter/.claude/worktrees/agent-aeafa7f527e2d7b4d
lane_branch: worktree-agent-aeafa7f527e2d7b4d
base: 6339a7eb39a0b3561f9b00937eaed4765e6da487
base_tree: a936b328b71e9b678215d2012065c3ae56b2ba86
plan0_at: 2026-10-22T23:08:06Z
plan0_scope: []
---
# Release permissions follow collaborator and organization access, and deleting an organization removes its release files (tests)
## Raw request
> On merge with main, re-verify releases against the collaborator and org access rules (T-0008), including `Repo::writable` and the org-deletion paths, which must also call `releases::ids_of` and `remo…
> (noticed by an agent; research T-0044-review)
## Interpretation
Verification after T-0008 (collaborators and orgs): every release handler already goes through Repo::readable/writable (shared access rules), and org deletion goes through lifecycle::delete_account, which calls releases::ids_of and remove_files. Nothing tests it; add the tests, fix anything they find.
## Assumptions (confidence)
## Acceptance criteria
- [ ] tests show a write collaborator and an org member with base write can make releases, a read collaborator can't, and deleting an org removes its release files — verify with `cargo test -q --test api releases_follow`
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
## Log
- 2026-10-22T00:48:52Z created
- 2026-10-22T23:07:50Z planned from capture
- 2026-10-22T23:07:52Z approved: the user asked to implement every queued feature ("what about all of the other features you have queued?"); proceeds under that standing request
- 2026-10-22T23:07:53Z set approved=True
- 2026-10-22T23:07:53Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0057.builder.md
- 2026-10-22T23:08:06Z focused
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research T-0044-review: Approve with nits. I found no security defect. I did not run anything: the review is from reading the diff and the worktree, so the passing tests…
- T-0008 [FEATURE L, done, 5 steps] Collaboration: orgs + collaborators with read/write/admin roles and plain-language access — lesson: Trust derived from membership is fo…
- research T-0008-review-2: Verdict: changes needed.** One HIGH issue lets any user get code run on the server's built-in CI runner. I read the diff and the surrounding co…
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
