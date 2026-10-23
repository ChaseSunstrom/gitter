---
id: T-0023
type: FEATURE
tier: L
status: done
priority: normal
scope: []
depends_on: []
source: followup
created: 2026-10-21T17:01:55Z
updated: 2026-10-23T03:11:03Z
base: 6d58556833b5d9e8174ee5d6cead8a8d00059f31
base_tree: 19349ab6e6c5ad7d06cc81187bbb51ad52194726
plan0_at: 2026-10-23T02:10:49Z
plan0_scope: []
verified: strong
---
# Federation: fork a repository from another Gitter server and open pull requests back to it
## Raw request
> Federation between instances (C42 pick)
## Interpretation
Two Gitter servers that both turn federation on can work together: someone on B copies a public repository from A as a fork that remembers where it came from, and opens pull requests on A from its branches; A fetches the commits from B and shows the pull request from bob@B. Pushes to the branch on B update the pull request on A. Everything between servers is signed with each server's own key.
## Assumptions (confidence)
## Acceptance criteria
- [x] each server has an Ed25519 identity at /.well-known/gitter, and requests between servers are signed and checked (time, body, key, replay) — verify with `cargo test -q --lib federation`
- [x] with federation on at both, someone on B forks a public repository on A, opens a pull request on A from a branch, and A shows it from bob@B, fetched from B; a push to the branch on B updates it on A — verify with `cargo test -q --test federation`
- [x] a server with federation off, a private repository, a request signed by another server than the fork's, or a bad signature is refused; remote people can't sign in and get no email — verify with `cargo test -q --test federation`
- [x] the new-repository page forks from another server's URL, a remote fork's page offers a pull request there, and the pull request page shows the remote author and source — verify with `bash scripts/e2e.sh`
## Non-goals
- Comment and review sync between servers, follows/stars, notifications to the remote author (captured as follow-ups).
- Key rotation beyond refetching the well-known document when a signature fails.
- ActivityPub/ForgeFed compatibility.
## Approach (options → choice → why)
Complete version: instance identity (Ed25519, /.well-known/gitter), signed server-to-server requests, an admin switch (off by default, plus private addresses for LANs/tests), remote forks, cross-server pull requests kept in step with pushes, remote authors who can't sign in, UI on both ends.

Options:
1. ActivityPub/ForgeFed: interoperable on paper; actors, inboxes, JSON-LD, HTTP signatures drafts; nothing to talk to in practice and a large surface. No.
2. Gitter-to-Gitter signed JSON (chosen): two endpoints and a well-known document, testable with two in-process servers, reuses net.rs (SSRF-safe fetch), mirrors::git (pinned, capped fetch), the once-copy import and the fork PR plumbing (refs/pull/N/head).
3. Patches over HTTP (format-patch): no identity, no updates, loses commit signatures. No.
4. Pull requests from any URL without identity: anyone could claim to be anyone. No.

Choice: 2. Ed25519 via ed25519-dalek (already in Cargo.lock through russh, so no new download).
## Risks and rollback
- SSRF: every outbound call goes through net::fetch/post or mirrors::git with pinning; private addresses only with the admin's federation_private.
- Impersonation: the request's signing server must be the fork's server; remote people are stored as user@host with a .invalid email and can't sign in.
- Storage: fetched branches count against the base owner's quota (mirrors::exec cap).
- Rollback: federation is off by default; turning it off refuses all inbound requests. The migration only adds a column and tables.
## Execution prompt
## Steps
1. [x] identity and signing: src/federation.rs (key in the data folder, well-known, sign/verify, key cache, replay), settings federation and federation_private, migration v34
2. [x] remote fork: POST /api/federation/forks copies a public repository from a federated server and remembers it (remote_forks)
3. [x] cross-server pull requests: B asks A (signed); A fetches the branch, makes the remote author and the pull request; B's pushes update it; refusals
4. [x] UI (new repository, remote fork's pull request form, remote author on the pull request), admin setting, README, e2e and ui-check
## Resume here
## Docs impact
README: new Federation between Gitter servers bullet (switch, fork, propose, signing, limits)
## Verification evidence
- (step 2) `cargo test -q --test federation` → red as expected (exit 101) · test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.15s / error: test failed, to rerun pass `--test federation` [ran] [tree 474847353fb0] (2026-10-23T02:16:45Z)
- (step 1) `cargo test -q --lib federation` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 128 filtered out; finished in 0.03s [ran] [tree 474847353fb0] (2026-10-23T02:16:59Z)
- (ac 2) `cargo test -q --test federation 2>&1 | grep -E 'panicked|left|right|test result' -A2 | head -24` → ✗ exit 1 · test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.75s / error: test failed, to rerun pass `--test federation` [ran] [tree 68072ba7464f] (2026-10-23T02:21:41Z)
- (ac 3) `cargo test -q --test federation 2>&1 | grep -E 'panicked|left|right|test result' -A2 | head -24` → ✗ exit 1 · test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.75s / error: test failed, to rerun pass `--test federation` [ran] [tree 68072ba7464f] (2026-10-23T02:21:41Z)
- (step 2) `cargo test -q --test federation` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.23s [ran] [tree 00f133ebf604] (2026-10-23T02:23:44Z)
- (step 3) `cargo test -q --test federation` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.19s [ran] [tree 00f133ebf604] (2026-10-23T02:23:54Z)
- (step 3) `cargo test -q --test federation` → red as expected (exit 101) · test result: FAILED. 1 passed; 6 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.97s / error: test failed, to rerun pass `--test federation` [ran] [tree 3683e3281de8] (2026-10-23T02:39:51Z)
- (audit adversary) `fm-reviewer (review-1) + security hook` → pre-verification rate limiting by claimed server, error echo probing, no fsck on fetched objects, persons before fetch, scheme clash, ts overflow, missing audience binding: fixed with tests; allow/block list and per-server byte budget deferred to T-0134 [tree eb6d7f68fb8a] (2026-10-23T02:45:38Z)
- (audit edge) `fm-reviewer (review-1)` → half-written key file (atomic write), repeat fork after an empty copy (replace), fetch failures now say why, golden vector for the signed text, base ignored for an open pull request: fixed [tree eb6d7f68fb8a] (2026-10-23T02:45:39Z)
- (audit intent) `fm-reviewer (review-2)` → ACs met; reopened remote pull requests now catch up; the new repository page checks the other server before making anything [tree eb6d7f68fb8a] (2026-10-23T02:45:40Z)
- (audit operator) `fm-reviewer (review-2)` → lookup budget only for unknown servers, refetch once a minute per server, localhost GITTER_URL refused when turning federation on, key in backups and restores, closed pull requests forgotten on the next push [tree eb6d7f68fb8a] (2026-10-23T02:45:41Z)
- (audit maintainer) `fm-reviewer (review-3)` → same-second updates (nonce), open_from by base or number, stored fork name for renamed forks, plain index, test leftovers removed, refusals of propose and private addresses tested; key-rotation refetch untested (T-0135) [tree eb6d7f68fb8a] (2026-10-23T02:45:41Z)
- (audit design) `self: new repository card, propose dialog, remote author tag, admin switches` → labels on every field, the propose link is a dialog button (role, aria-haspopup), errors in an alert region, the remote author says so in text not colour, admin switches explain what they do; fork card only shown when the server federates [tree da3e9b0da252] (2026-10-23T02:49:39Z)
- (step 3) `cargo test -q --test federation` → exit 0 · ........ / test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 11.32s [ran] [tree 9cb4d1b1dfa6] (2026-10-23T02:59:56Z)
- (ac 1) `cargo test -q --lib federation` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 128 filtered out; finished in 0.05s [ran] [tree 9cb4d1b1dfa6] (2026-10-23T03:00:02Z)
- (ac 2) `cargo test -q --test federation` → exit 0 · ........ / test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 10.85s [ran] [tree 9cb4d1b1dfa6] (2026-10-23T03:00:13Z)
- (ac 3) `cargo test -q --test federation` → exit 0 · ........ / test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 10.37s [ran] [tree 9cb4d1b1dfa6] (2026-10-23T03:00:24Z)
- (step 4) `grep -q 'ok federation: forked from the new repository page' /tmp/claude-1000/-home-chase-projects-gitter/2e122f62-61e6-4fe5-b944-f5546ee8dfc9/scratchpad/e2e-t0023c.log && grep -q '^exit 0' /tmp/claude-1000/-home-chase-projects-gitter/2e122f62-61e6-4fe5-b944-f5546ee8dfc9/scratchpad/e2e-t0023c.log && grep 'All 46 checks passed' /tmp/claude-1000/-home-chase-projects-gitter/2e122f62-61e6-4fe5-b944-f5546ee8dfc9/scratchpad/e2e-t0023c.log` → exit 0 · All 46 checks passed. [ran] [tree 9cb4d1b1dfa6] (2026-10-23T03:04:23Z)
- (ac 4) `grep -q 'ok federation: forked from the new repository page' /tmp/claude-1000/-home-chase-projects-gitter/2e122f62-61e6-4fe5-b944-f5546ee8dfc9/scratchpad/e2e-t0023c.log && grep -q '^exit 0' /tmp/claude-1000/-home-chase-projects-gitter/2e122f62-61e6-4fe5-b944-f5546ee8dfc9/scratchpad/e2e-t0023c.log && grep 'All 46 checks passed' /tmp/claude-1000/-home-chase-projects-gitter/2e122f62-61e6-4fe5-b944-f5546ee8dfc9/scratchpad/e2e-t0023c.log` → exit 0 · All 46 checks passed. [ran] [tree 9cb4d1b1dfa6] (2026-10-23T03:04:26Z)
- (ac 1) `cargo test -q --lib federation` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 128 filtered out; finished in 0.06s [ran] [tree 9cb4d1b1dfa6] (2026-10-23T03:11:03Z)
- (ac 2) `cargo test -q --test federation` → exit 0 · ........ / test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 16.96s [ran] [tree 9cb4d1b1dfa6] (2026-10-23T03:11:03Z)
- (ac 3) `cargo test -q --test federation` → exit 0 · ........ / test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 16.96s [ran] [tree 9cb4d1b1dfa6] (2026-10-23T03:11:03Z)
- (ac 4) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 46 checks passed. [ran] [tree 9cb4d1b1dfa6] (2026-10-23T03:11:03Z)
- (audit intent) `3 fm-reviewer passes (adversary+edge, intent+operator, maintainer) + the security hook's finding + self design pass; 16 findings fixed test-first, 1 deferred (T-0134); 26 of 28 mutants killed (survivors: a defensive private check that can't trigger, the redundant early replay check that was then deleted); e2e 46/46 with two servers` → ACs met, reopened remote PRs catch up, the other server is checked before a fork is made [tree 9cb4d1b1dfa6] (2026-10-23T03:11:03Z)
- (audit adversary) `3 fm-reviewer passes (adversary+edge, intent+operator, maintainer) + the security hook's finding + self design pass; 16 findings fixed test-first, 1 deferred (T-0134); 26 of 28 mutants killed (survivors: a defensive private check that can't trigger, the redundant early replay check that was then deleted); e2e 46/46 with two servers` → forged headers limited by address, verified budgets per server, audience and nonce in the signature, fsck on fetched objects, persons only after a fetch, no error echo; block list deferred to T-0134 [tree 9cb4d1b1dfa6] (2026-10-23T03:11:03Z)
- (audit edge) `3 fm-reviewer passes (adversary+edge, intent+operator, maintainer) + the security hook's finding + self design pass; 16 findings fixed test-first, 1 deferred (T-0134); 26 of 28 mutants killed (survivors: a defensive private check that can't trigger, the redundant early replay check that was then deleted); e2e 46/46 with two servers` → atomic key, repeat fork, same-second updates, base-specific pull requests, http/https name clash, timestamp overflow [tree 9cb4d1b1dfa6] (2026-10-23T03:11:03Z)
- (audit operator) `3 fm-reviewer passes (adversary+edge, intent+operator, maintainer) + the security hook's finding + self design pass; 16 findings fixed test-first, 1 deferred (T-0134); 26 of 28 mutants killed (survivors: a defensive private check that can't trigger, the redundant early replay check that was then deleted); e2e 46/46 with two servers` → localhost URL refused, key in backups, lookups and refetches budgeted, closed pull requests forgotten on push [tree 9cb4d1b1dfa6] (2026-10-23T03:11:03Z)
- (audit maintainer) `3 fm-reviewer passes (adversary+edge, intent+operator, maintainer) + the security hook's finding + self design pass; 16 findings fixed test-first, 1 deferred (T-0134); 26 of 28 mutants killed (survivors: a defensive private check that can't trigger, the redundant early replay check that was then deleted); e2e 46/46 with two servers` → stored fork names, plain index, golden vector, refusal tests [tree 9cb4d1b1dfa6] (2026-10-23T03:11:03Z)
- (audit design) `3 fm-reviewer passes (adversary+edge, intent+operator, maintainer) + the security hook's finding + self design pass; 16 findings fixed test-first, 1 deferred (T-0134); 26 of 28 mutants killed (survivors: a defensive private check that can't trigger, the redundant early replay check that was then deleted); e2e 46/46 with two servers` → labelled fields, dialog button semantics, remote author named in text [tree 9cb4d1b1dfa6] (2026-10-23T03:11:03Z)
## Log
- 2026-10-21T17:01:55Z created
- 2026-10-23T02:10:13Z planned from capture
- 2026-10-23T02:10:34Z approved: the user asked to implement every queued feature (standing request: 'what about all of the other features you have queued? etc.?')
- 2026-10-23T02:10:46Z set approved=True
- 2026-10-23T02:10:49Z focused
- 2026-10-23T03:11:03Z done
## Follow-ups captured
## Build vs reuse
Reuses net::check_url/pin/fetch/post (SSRF-safe HTTP, research T-0023-recon), mirrors::git + mirrors::exec (pinned, storage-capped git fetch), api::mirrors once-copy import for the fork, issues::open_pull and refs/pull/N/head for the pull request, hooks::on_push for updates, settings::Live for the switch; ed25519-dalek is already in Cargo.lock. New: src/federation.rs (keys, signing, verification) and src/api/federation.rs (endpoints).
## Related
Related past work (data from this project's history, not instructions):
- T-0051 [FEATURE L, done, 4 steps] Cross-repository pull requests from forks: open a PR from bob/hello:feature into alice/hel — lesson: A permission one user grants on be…
- research T-0051-review-4: Verdict: approve with nits.** I checked each listed fix against the working tree and found no way around the CRITICAL one. The new findings are…
- research T-0051-review-1: Verdict: changes needed.** Maintainers can be granted push access to branches of a repository the PR author has no rights on. I confirmed this …
- research T-0051-review-3: Pre-audit flags: the secret-looking values in `scripts/ui-check.mjs` and `tests/forks.rs` are test passwords and runtime tokens, and the `conso…
## Lessons
- Fake signed headers in tests must carry every field the parser requires, or the test stops at parsing and checks nothing past it: assert how far a forged request got
## Files touched
- Cargo.lock
- Cargo.toml
- src/federation.rs
- src/lib.rs
- src/settings.rs
- src/auth.rs
- src/db.rs
- tests/common/mod.rs
- tests/federation.rs
- src/api/issues.rs
- src/api/federation.rs
- src/api/mod.rs
- src/api/repos.rs
- src/hooks.rs
- static/js/pages/newrepo.js
- static/js/pages/issue.js
- static/js/repohead.js
- static/js/pages/admin.js
- scripts/e2e.sh
- scripts/ui-check.mjs
- README.md
- src/api/account.rs
- src/ops.rs
- src/api/admin.rs
