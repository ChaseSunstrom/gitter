---
id: T-0063
type: FEATURE
tier: M
status: done
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T01:37:33Z
updated: 2026-10-22T23:25:10Z
base: bd06469bc921899300447656ee6f763f635d8c9c
base_tree: 17b16e49fa76898550e35d97e125b806d2aa2fb0
plan0_at: 2026-10-22T22:10:49Z
plan0_scope: []
verified: strong
---
# Signed commits: re-check cached Good GPG results once the key has expired, cache tool timeouts for a minute, proof of possession when adding a key
## Raw request
> Signed commits: re-check cached Good GPG results when the key's expiry passes, negative-cache tool timeouts for a minute, and add proof of possession when adding a key (sign a server challenge) so a badge can name the key holder even before email confirmation exists (from T-0020 review)
## Interpretation
Signature badges are cached per commit (st.git.memo, keyed by repo and sha, invalidated by a keyring version). Three gaps: a cached Good GPG result stays Good after the key expires; a gpg/ssh-keygen timeout is retried on every page view; and a key can be added without proving the adder holds its private half.
## Assumptions (confidence)
## Acceptance criteria
- [x] a cached Good result for a GPG key that has since expired shows as expired/unverified on the next read — verify with `cargo test -q signing`
- [x] a gpg/ssh-keygen timeout is remembered for a minute: the same commit isn't re-checked within it — verify with `cargo test -q signing`
- [x] adding a signing key needs a signature over a server challenge made with that key; the key page and badge say the holder proved it — verify with `cargo test -q signing`
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
README: signing keys paragraph (proof of possession, Proved/Not proved)
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
- (ac 1) `cargo test -q --test api signing && cargo test -q --lib signing` → exit 0 · ........... / test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 114 filtered out; finished in 0.05s [ran] [tree 507b792bd6d6] (2026-10-22T23:23:39Z)
- (ac 2) `cargo test -q --test api signing && cargo test -q --lib signing` → exit 0 · ........... / test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 114 filtered out; finished in 0.05s [ran] [tree 507b792bd6d6] (2026-10-22T23:23:47Z)
- (ac 3) `cargo test -q --test api signing && cargo test -q --lib signing` → exit 0 · ........... / test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 114 filtered out; finished in 0.05s [ran] [tree 507b792bd6d6] (2026-10-22T23:23:56Z)
- (step 1) `cargo test -q --test api signing && cargo test -q --lib signing` → exit 0 · ........... / test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 114 filtered out; finished in 0.05s [ran] [tree 507b792bd6d6] (2026-10-22T23:24:05Z)
- (step 2) `cargo test -q --test api signing && cargo test -q --lib signing` → exit 0 · ........... / test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 114 filtered out; finished in 0.06s [ran] [tree 507b792bd6d6] (2026-10-22T23:24:13Z)
- (step 3) `cargo test -q --test api signing && cargo test -q --lib signing` → exit 0 · ........... / test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 114 filtered out; finished in 0.05s [ran] [tree 507b792bd6d6] (2026-10-22T23:24:22Z)
- (ac 1) `cargo test -q signing` → exit 0 · running 0 tests / test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.00s [ran] [tree 507b792bd6d6] (2026-10-22T23:25:09Z)
- (ac 2) `cargo test -q signing` → exit 0 · running 0 tests / test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.00s [ran] [tree 507b792bd6d6] (2026-10-22T23:25:09Z)
- (ac 3) `cargo test -q signing` → exit 0 · running 0 tests / test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.00s [ran] [tree 507b792bd6d6] (2026-10-22T23:25:09Z)
- (audit intent) `fm-reviewer on the lane (five lenses), eight items fixed test-first by the builder, migration renumbered to v33 at integration; re-run on main: api signing and lib signing pass` → cached Good results end at the key's expiry; timeouts remembered a minute; adding a key needs a signature over a one-use challenge, shown as Proved [tree 507b792bd6d6] (2026-10-22T23:25:09Z)
- (audit adversary) `fm-reviewer on the lane (five lenses), eight items fixed test-first by the builder, migration renumbered to v33 at integration; re-run on main: api signing and lib signing pass` → challenge per account with site and 128 random bits, one use (atomic), an hour; signature fingerprint must match; GPG in a keyring of only that key, detached only, nothing after the armor; Sudo refuses tokens [tree 507b792bd6d6] (2026-10-22T23:25:09Z)
- (audit edge) `fm-reviewer on the lane (five lenses), eight items fixed test-first by the builder, migration renumbered to v33 at integration; re-run on main: api signing and lib signing pass` → keys without expiry, expired subkeys, clock skew on proofs, keys added before the migration show Not proved [tree 507b792bd6d6] (2026-10-22T23:25:09Z)
- (audit operator) `fm-reviewer on the lane (five lenses), eight items fixed test-first by the builder, migration renumbered to v33 at integration; re-run on main: api signing and lib signing pass` → migration v33 adds a column and a table; old keys keep working, marked Not proved [tree 507b792bd6d6] (2026-10-22T23:25:09Z)
- (audit maintainer) `fm-reviewer on the lane (five lenses), eight items fixed test-first by the builder, migration renumbered to v33 at integration; re-run on main: api signing and lib signing pass` → remember(), fresh(), key_expiry and prove in signing.rs, each with unit tests [tree 507b792bd6d6] (2026-10-22T23:25:09Z)
## Log
- 2026-10-22T01:37:33Z created
- 2026-10-22T22:09:51Z planned from capture
- 2026-10-22T22:10:28Z approved: the user asked to implement every queued feature ("what about all of the other features you have queued?"); proceeds under that standing request
- 2026-10-22T22:10:28Z set approved=True
- 2026-10-22T22:10:35Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0063.builder.md
- 2026-10-22T22:10:49Z focused
- 2026-10-22T23:22:12Z merged worktree-agent-aeb57c60de24a98b5 (10 file(s))
- 2026-10-22T23:22:21Z lane removed: /home/chase/projects/gitter/.claude/worktrees/agent-aeb57c60de24a98b5
- 2026-10-22T23:22:22Z focused
- 2026-10-22T23:25:10Z done
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research T-0020-review: Changes needed. One likely HIGH in the GPG path, and the Verified guarantee is weak while emails are unconfirmed and keys need no proof of owners…
- T-0020 [FEATURE M, done, 4 steps] Signed commit verification (C30) — lesson: Test a security fix red-to-green: the GPG bypass didn't reproduce on gpg 2.4.9, so the check…
- T-0011 [PERFORMANCE M, done, 5 steps] Speed proof: commit-graph maintenance, cached last commits, big-diff loading, gitter bench + perf budget, published numbers — lesso…
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
## Lessons
- A cache entry's age must be stamped when it's stored, not when the request that made it began; long requests otherwise write entries that are already stale
## Files touched
- tests/api.rs
- src/signing.rs
- src/db.rs
- src/api/mod.rs
- src/api/account.rs
- static/js/lib.js
- static/js/pages/settings.js
- scripts/e2e.sh
- scripts/ui-check.mjs
- README.md
- src/api/ci.rs
- src/api/deploy.rs
- src/api/issues.rs
- src/api/preview.rs
- src/ci/compat.rs
- src/ci/config.rs
- src/ci/jobs.rs
- src/cli.rs
- src/imports.rs
- src/lib.rs
- src/main.rs
- src/net.rs
- src/validate.rs
- static/js/main.js
- static/js/pages/auth.js
- static/js/pages/deployments.js
- static/js/pages/issue.js
- static/js/pages/reposettings.js
- static/js/repohead.js
- tests/cli.rs
