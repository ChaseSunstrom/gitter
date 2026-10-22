---
id: T-0063
type: FEATURE
tier: M
status: active
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T01:37:33Z
updated: 2026-10-22T22:41:07Z
builder: 2026-10-22T22:10:35Z
builder_proc: archbox-hardened|pid:[4026531836]|4747|8111
lane: /home/chase/projects/gitter/.claude/worktrees/agent-aeb57c60de24a98b5
lane_branch: worktree-agent-aeb57c60de24a98b5
base: 6e1ef85012b118f0c31812348206b1ebc277b4cf
base_tree: f33bc7eef8ec3e3883ad8e7a3e322250ec4c6692
plan0_at: 2026-10-22T22:10:49Z
plan0_scope: []
---
# Signed commits: re-check cached Good GPG results once the key has expired, cache tool timeouts for a minute, proof of possession when adding a key
## Raw request
> Signed commits: re-check cached Good GPG results when the key's expiry passes, negative-cache tool timeouts for a minute, and add proof of possession when adding a key (sign a server challenge) so a badge can name the key holder even before email confirmation exists (from T-0020 review)
## Interpretation
Signature badges are cached per commit (st.git.memo, keyed by repo and sha, invalidated by a keyring version). Three gaps: a cached Good GPG result stays Good after the key expires; a gpg/ssh-keygen timeout is retried on every page view; and a key can be added without proving the adder holds its private half.
## Assumptions (confidence)
## Acceptance criteria
- [ ] a cached Good result for a GPG key that has since expired shows as expired/unverified on the next read — verify with `cargo test -q signing`
- [ ] a gpg/ssh-keygen timeout is remembered for a minute: the same commit isn't re-checked within it — verify with `cargo test -q signing`
- [ ] adding a signing key needs a signature over a server challenge made with that key; the key page and badge say the holder proved it — verify with `cargo test -q signing`
## Non-goals
## Approach (options → choice → why)
Options: (a) store the key's expiry in the Check and treat a cached Good past it as a miss [chosen: one field, no schedule]; (b) a timer that bumps the keyring version at each key's expiry; (c) no caching for GPG. Timeouts: a short-lived negative memo entry (60 s) with an outcome of its own, never shown as Bad. Proof of possession: POST /api/me/signing-keys/challenge gives a random text; adding a key requires its detached signature (gpg --verify, or ssh-keygen -Y verify with namespace gitter); keys stored with proved_at; old keys stay but show as not proved. Captured instead: re-verifying old keys.
## Risks and rollback
## Execution prompt
## Steps
1. [x] expiry re-check of cached results (test first)
2. [x] negative cache for tool timeouts (test first)
3. [x] proof of possession: challenge endpoint, signature check on add, UI and README
## Resume here
## Docs impact
## Verification evidence
- (step 1) `cargo test -q signing` → exit 0 · running 0 tests / test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.00s [ran] [tree 50027bc1c649] (2026-10-22T22:18:07Z)
- (step 2) `cargo test -q signing` → exit 0 · running 0 tests / test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.00s [ran] [tree ba596c926bde] (2026-10-22T22:20:32Z)
- (step 3) `cargo test -q signing` → exit 0 · running 0 tests / test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.00s [ran] [tree 907c9b407427] (2026-10-22T22:27:40Z)
- (step 1) `cargo test -q --lib signing` → exit 0 · ........... / test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 109 filtered out; finished in 0.05s [ran] [tree c1a69462d402] (2026-10-22T22:40:31Z)
- (step 1) `cargo test -q --test api signing` → exit 0 · .... / test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 78 filtered out; finished in 8.81s [ran] [tree c1a69462d402] (2026-10-22T22:40:41Z)
- (step 2) `cargo test -q --lib signing` → exit 0 · ........... / test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 109 filtered out; finished in 0.06s [ran] [tree c1a69462d402] (2026-10-22T22:40:50Z)
- (step 2) `cargo test -q --test api signing` → exit 0 · .... / test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 78 filtered out; finished in 7.27s [ran] [tree c1a69462d402] (2026-10-22T22:40:58Z)
- (step 3) `cargo test -q --lib signing` → exit 0 · ........... / test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 109 filtered out; finished in 0.06s [ran] [tree c1a69462d402] (2026-10-22T22:40:58Z)
- (step 3) `cargo test -q --test api signing` → exit 0 · .... / test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 78 filtered out; finished in 7.86s [ran] [tree c1a69462d402] (2026-10-22T22:41:07Z)
## Log
- 2026-10-22T01:37:33Z created
- 2026-10-22T22:09:51Z planned from capture
- 2026-10-22T22:10:28Z approved: the user asked to implement every queued feature ("what about all of the other features you have queued?"); proceeds under that standing request
- 2026-10-22T22:10:28Z set approved=True
- 2026-10-22T22:10:35Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0063.builder.md
- 2026-10-22T22:10:49Z focused
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research T-0020-review: Changes needed. One likely HIGH in the GPG path, and the Verified guarantee is weak while emails are unconfirmed and keys need no proof of owners…
- T-0020 [FEATURE M, done, 4 steps] Signed commit verification (C30) — lesson: Test a security fix red-to-green: the GPG bypass didn't reproduce on gpg 2.4.9, so the check…
- T-0011 [PERFORMANCE M, done, 5 steps] Speed proof: commit-graph maintenance, cached last commits, big-diff loading, gitter bench + perf budget, published numbers — lesso…
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
