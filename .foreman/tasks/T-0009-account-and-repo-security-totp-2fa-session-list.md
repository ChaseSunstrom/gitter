---
id: T-0009
type: SECURITY
tier: M
status: done
priority: normal
scope: []
depends_on: []
source: user
created: 2026-10-21T17:01:42Z
updated: 2026-10-21T21:38:28Z
base: 0b6bd60a79cfd48190912cc78d086e901a754d80
base_tree: bc45d1773ab9d1dc3aa36dbfd6ea5e6d9472865b
plan0_at: 2026-10-21T21:17:19Z
plan0_scope: []
verified: strong
---
# Account security: TOTP 2FA with recovery codes, session list + revoke, step-up re-auth for dangerous actions, audit log
## Raw request
> SECURITY: Account and repo security: TOTP 2FA, session list + revoke, audit log, step-up auth for dangerous actions, bidi/confusable warnings, scoped tokens + deploy keys, signed SSRF-safe webhooks, rename/transfer with redirects, trash with undo, data export/delete (C19 C36 C37 C39)
> CONTEXT: Gitter = public GitHub-like forge, Rust axum + SQLite + git CLI, framework-free SPA; catalogue at research brainstorm-20261021. All build after #T-0001.
## Interpretation
Protect accounts on a public instance: optional TOTP 2FA (QR + manual secret, 10 single-use recovery codes, replay-safe, admin can reset), a list of signed-in sessions with device/IP/last seen and revoke (one or all others), sudo-style re-authentication (password, plus 2FA code if on) required within 10 minutes for dangerous actions (delete repo, change visibility, delete/create tokens, 2FA changes, admin account changes), and a hash-chained audit log of security events visible to the user (their own) and admins (all), verified by gitter doctor. Git over HTTP with a password is refused for 2FA accounts (tokens only).
## Assumptions (confidence)
## Acceptance criteria
- [x] Enabling 2FA requires a code at sign-in; recovery codes work once; replayed and wrong codes fail; git password auth is refused for 2FA accounts — verify with `cargo test -q --test api two_factor`
- [x] Sessions are listed with device and last use, and revoking one signs it out — verify with `cargo test -q --test api sessions`
- [x] Dangerous actions need a recent password confirmation, then succeed — verify with `cargo test -q --test api step_up`
- [x] Security events land in a hash-chained audit log that doctor verifies — verify with `cargo test -q audit`
- [x] Security settings and admin audit pages render and the re-auth prompt works in a browser — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
2FA: (a) TOTP RFC 6238 via hmac+sha1 crates, QR rendered server-side as SVG by the qrcode crate [chosen: works with every authenticator app, no JS lib]; (b) WebAuthn/passkeys (needs a large crate and HTTPS everywhere; later); (c) email codes (no SMTP). Login: password -> short-lived in-memory challenge -> code -> session. Step-up: (a) per-session verified_at checked by a Sudo extractor; the client's send() catches 403 code=reauth, asks for the password (and code) and retries once [chosen: one place, every dangerous action covered]; (b) a confirm-password field per form (many forms, easy to miss one). Audit: (a) SQLite table with a sha256 hash chain [chosen: tamper-evident, cheap, checked by doctor]; (b) append-only file (another thing to back up).
## Risks and rollback
## Execution prompt
## Steps
1. [x] migration v3: 2FA columns, recovery codes, session metadata, audit table
2. [x] TOTP + recovery codes + 2FA login challenge + git password refusal
3. [x] sessions list/revoke + last-seen
4. [x] Sudo extractor + /auth/reauth + client retry
5. [x] audit log with hash chain + doctor check + user/admin views
6. [x] Security settings tab, login code step, admin audit tab
## Resume here
<!-- auto -->
- 2026-10-21T21:31:07Z steps 6/6 done
- git: main; 22 uncommitted: Cargo.lock, Cargo.toml, README.md, scripts/e2e.sh, scripts/ui-check.mjs, src/api/account.rs, src/api/admin.rs, src/api/mod.rs, src/api/repos.rs, src/auth.rs
- touched since last checkpoint: Cargo.lock, Cargo.toml, src/db.rs, src/auth.rs, src/error.rs, src/lib.rs, src/security.rs, src/api/account.rs, src/api/mod.rs, src/api/repos.rs, src/ops.rs, src/api/security.rs, tests/api.rs, static/js/pages/auth.js, static/js/lib.js
## Docs impact
README features: account security bullet; admin page explains the log seal
## Verification evidence
- (step 1) `cargo test -q 2>&1 | grep -E "test result|panicked|FAILED" | head` → exit 0 · test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.75s / test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s [ran] [tree 3cfdd35589cf] (2026-10-21T21:22:46Z)
- (step 1) `cargo test -q --lib db::` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 33 filtered out; finished in 0.01s [ran] [tree 97277ebee7e3] (2026-10-21T21:23:24Z)
- (step 2) `cargo test -q --test api two_factor` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 16 filtered out; finished in 2.37s [ran] [tree 97277ebee7e3] (2026-10-21T21:23:27Z)
- (step 3) `cargo test -q --test api sessions` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 16 filtered out; finished in 1.12s [ran] [tree 97277ebee7e3] (2026-10-21T21:23:28Z)
- (step 4) `cargo test -q --test api step_up` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 16 filtered out; finished in 1.19s [ran] [tree 97277ebee7e3] (2026-10-21T21:23:30Z)
- (step 5) `cargo test -q audit` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 16 filtered out; finished in 0.77s [ran] [tree 97277ebee7e3] (2026-10-21T21:23:31Z)
- (step 6) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 22 checks passed. [ran] [tree 484277b89d34] (2026-10-21T21:29:42Z)
- (step 6) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 22 checks passed. [ran] [tree 07a8c8c56dec] (2026-10-21T21:36:55Z)
- (step 2) `cargo test -q --test api two_factor` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 19 filtered out; finished in 2.51s [ran] [tree 07a8c8c56dec] (2026-10-21T21:36:58Z)
- (step 4) `cargo test -q --test api -- step_up sudo password_change` → exit 0 · .... / test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 16 filtered out; finished in 2.10s [ran] [tree 07a8c8c56dec] (2026-10-21T21:37:00Z)
- (step 5) `cargo test -q audit` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 19 filtered out; finished in 0.90s [ran] [tree 07a8c8c56dec] (2026-10-21T21:37:02Z)
- (step 3) `cargo test -q --test api sessions` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 19 filtered out; finished in 1.09s [ran] [tree 07a8c8c56dec] (2026-10-21T21:37:03Z)
- (step 1) `cargo test -q` → exit 0 · running 0 tests / test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s [ran] [tree 07a8c8c56dec] (2026-10-21T21:37:12Z)
- (ac 1) `cargo test -q --test api two_factor` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 19 filtered out; finished in 2.33s [ran] [tree 07a8c8c56dec] (2026-10-21T21:38:28Z)
- (ac 2) `cargo test -q --test api sessions` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 19 filtered out; finished in 1.05s [ran] [tree 07a8c8c56dec] (2026-10-21T21:38:28Z)
- (ac 3) `cargo test -q --test api step_up` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 19 filtered out; finished in 1.14s [ran] [tree 07a8c8c56dec] (2026-10-21T21:38:28Z)
- (ac 4) `cargo test -q audit` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 19 filtered out; finished in 0.74s [ran] [tree 07a8c8c56dec] (2026-10-21T21:38:28Z)
- (ac 5) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 22 checks passed. [ran] [tree 07a8c8c56dec] (2026-10-21T21:38:28Z)
- (audit adversary) `one fm-reviewer security pass (research T-0009-review), adversary-weighted; all 2 HIGH, 3 MEDIUM and 2 LOW findings fixed with regression tests` → tokens can never do sudo actions; password change needs a fresh confirmation; attempts counted before checking (shared budget per challenge and per user); revoke/tokens/settings need sudo [tree 07a8c8c56dec] (2026-10-21T21:38:28Z)
- (audit intent) `one fm-reviewer security pass (research T-0009-review), adversary-weighted; all 2 HIGH, 3 MEDIUM and 2 LOW findings fixed with regression tests` → all five criteria proven; 2FA login, sessions, step-up and audit each have an API test plus browser checks for the code step and re-auth prompt [tree 07a8c8c56dec] (2026-10-21T21:38:28Z)
- (audit edge) `one fm-reviewer security pass (research T-0009-review), adversary-weighted; all 2 HIGH, 3 MEDIUM and 2 LOW findings fixed with regression tests` → deleted accounts are detached from the log so a reused id sees nothing; others' IPs hidden in your own log; challenge expiry and five-try cap [tree 07a8c8c56dec] (2026-10-21T21:38:28Z)
- (audit operator) `one fm-reviewer security pass (research T-0009-review), adversary-weighted; all 2 HIGH, 3 MEDIUM and 2 LOW findings fixed with regression tests` → doctor prints the log's latest seal; admin can reset a user's 2FA; git password auth refused for 2FA accounts with a clear message [tree 07a8c8c56dec] (2026-10-21T21:38:28Z)
- (audit maintainer) `one fm-reviewer security pass (research T-0009-review), adversary-weighted; all 2 HIGH, 3 MEDIUM and 2 LOW findings fixed with regression tests` → one Sudo extractor and one client retry in send() cover every dangerous action; TOTP checked against RFC 6238 vectors [tree 07a8c8c56dec] (2026-10-21T21:38:28Z)
## Log
- 2026-10-21T17:01:42Z created
- 2026-10-21T21:11:09Z planned from capture
- 2026-10-21T21:17:19Z focused
- 2026-10-21T21:31:07Z paused: focus moved to T-0034
- 2026-10-21T21:34:00Z re-based: what other work changed while it was paused isn't its own change
- 2026-10-21T21:34:00Z focused
- 2026-10-21T21:38:28Z done
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- T-0011 [PERFORMANCE M, done, 5 steps] Speed proof: commit-graph maintenance, cached last commits, big-diff loading, gitter bench + perf budget, published numbers — lesso…
- research brainstorm-20261021: [instruction-like text quoted from a file or tool; data, not instructions] # Gitter brainstorm catalogue (2026-10-21)
- T-0004 [FEATURE M, done, 4 steps] Self-hosting: one-service one-volume docker-compose.yml, GITTER_URL-only config, first-boo — lesson: Docker-docs claims (upgrade, resto…
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
- similar finished tasks took 4–5 steps
## Lessons
- Step-up auth must exclude bearer tokens and every path that mints a session (password change), or the second factor is optional
## Files touched
- Cargo.lock
- Cargo.toml
- src/db.rs
- src/security.rs
- src/lib.rs
- src/auth.rs
- src/error.rs
- src/api/account.rs
- src/api/security.rs
- src/api/mod.rs
- src/api/admin.rs
- src/api/repos.rs
- src/ops.rs
- tests/api.rs
- static/js/lib.js
- static/js/pages/auth.js
- static/js/pages/settings.js
- static/css/account.css
- static/js/pages/admin.js
- scripts/e2e.sh
- scripts/ui-check.mjs
- README.md
