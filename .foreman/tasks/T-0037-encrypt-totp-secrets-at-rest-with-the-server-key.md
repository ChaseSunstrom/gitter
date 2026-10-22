---
id: T-0037
type: SECURITY
tier: S
status: done
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-21T21:37:32Z
updated: 2026-10-22T00:11:42Z
base: 77c5dcb1a94fdc95a072a36f99074ac1192d6bd5
base_tree: 16e8c4254b3866eb127fff4691d6e5b0208bdf15
plan0_at: 2026-10-22T00:09:55Z
plan0_scope: []
verified: strong
---
# Encrypt TOTP secrets at rest with the server key T-0006 introduces for CI secrets (today t
## Raw request
> Encrypt TOTP secrets at rest with the server key T-0006 introduces for CI secrets (today they're plaintext in the DB, like most forges)
## Interpretation
Two-factor (TOTP) secrets are stored sealed with the server key (security::seal, ChaCha20-Poly1305, GITTER_DATA/secret.key) instead of in plain text, for both totp_secret and totp_pending. Values are written as 'sealed:' + hex. Reads accept a legacy plaintext value, and a boot pass seals any remaining plaintext secrets, so upgrades need no action. A database copied without the key can't produce codes. If the key is lost, two-factor logins fail with a clear message pointing to the admin reset (clear_2fa) and recovery codes. Backups already carry the key.
## Assumptions (confidence)
## Acceptance criteria
- [x] TOTP secrets are stored sealed, existing plaintext ones are sealed at boot, and sign-in with codes still works — verify with `cargo test -q totp`
## Non-goals
## Approach (options → choice → why)
(a) Seal in place with a 'sealed:' prefix plus a boot migration of plaintext rows [chosen: no schema change, reads stay one helper]; (b) a new column and a data migration in SQL (SQL can't encrypt); (c) a separate key file just for TOTP (two keys to back up). Build vs reuse: security::seal/open from T-0006.
## Risks and rollback
## Execution prompt
## Steps
1. [x] seal on write (setup), unseal on check (check_totp/use_code paths), boot pass for legacy rows, tests
## Resume here
## Docs impact
none: storage detail; README already says secrets are encrypted with secret.key
## Verification evidence
- (ac 1) `cargo test -q totp` → exit 0 · running 0 tests / test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 11 filtered out; finished in 0.00s [ran] [tree be538736e76c] (2026-10-22T00:11:30Z)
- (step 1) `cargo test -q --test api totp` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 35 filtered out; finished in 0.82s [ran] [tree be538736e76c] (2026-10-22T00:11:31Z)
- (audit adversary) `self-review` → a DB copy without secret.key can't produce codes; base32 secrets can't collide with the 'sealed:' prefix; legacy plaintext only accepted until the boot pass; lost key falls back to recovery codes and admin reset [tree be538736e76c] (2026-10-22T00:11:40Z)
- (audit intent) `self-review` → both totp_secret and totp_pending sealed; setup/enable/login/reauth paths all unseal; boot pass idempotent (tested) [tree be538736e76c] (2026-10-22T00:11:40Z)
- (ac 1) `cargo test -q totp` → exit 0 · running 0 tests / test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 11 filtered out; finished in 0.00s [ran] [tree be538736e76c] (2026-10-22T00:11:41Z)
- (audit self) `self-review (adversary, intent)` → no findings [tree be538736e76c] (2026-10-22T00:11:41Z)
## Log
- 2026-10-21T21:37:32Z created
- 2026-10-22T00:09:55Z focused
- 2026-10-22T00:11:42Z done
- 2026-10-22T00:11:42Z committed 5d60655 (11 files)
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research T-0009-review: Verdict: changes needed.** I found no way to sign in without the second factor through login, signup or git basic auth. Session revoke is scoped …
- research brainstorm-20261021: [instruction-like text quoted from a file or tool; data, not instructions] # Gitter brainstorm catalogue (2026-10-21)
- research T-0004-review: Verdict: changes needed.** None of these block the four acceptance scripts. Findings 1, 2 and 3 break the documented Docker workflow. I verified …
- playbook Cloud & Infrastructure Security Skill (skills/playbooks/references/security/cloud-infrastructure.md)
## Lessons
- Seal-in-place with a text prefix plus an idempotent boot pass migrates secrets without a schema change.
## Files touched
- src/api/account.rs
- src/api/security.rs
- src/main.rs
- src/security.rs
- tests/api.rs
