---
id: T-0089
type: FEATURE
tier: L
status: done
priority: normal
scope: []
depends_on: [T-0084]
source: user
created: 2026-10-22T16:03:16Z
updated: 2026-10-22T19:56:15Z
base: 4311fd24379402062c0146eaf1d3f8c9bc8c9c19
base_tree: 742a33e7c3f08c13a0dd2d75313fbc484d97df28
plan0_at: 2026-10-22T19:13:22Z
plan0_scope: []
verified: strong
---
# Sign in with GitHub, GitLab or any OpenID Connect provider
## Raw request
> Sign in with GitHub, GitLab or any OpenID Connect provider
## Interpretation
Admins add sign-in providers (GitHub, GitLab incl. self-managed, generic OIDC) with a sealed client secret in their own table, not the settings blob. Login and signup pages show a button per enabled provider. The OAuth code flow uses state plus PKCE S256 held server-side and bound to a short-lived cookie. Identities are matched by the provider's stable subject id, never by email. A new identity creates an account only if signup allows it (closed/invite still apply), with an unusable password and the email only when the provider marks it verified. Linking to an existing account happens only from a signed-in session (Settings → Sign-in methods, with sudo), never automatically by email. Accounts with 2FA still pass the 2FA challenge. OAuth-only accounts can't use password login until they set a password; the last sign-in method can't be removed.
## Assumptions (confidence)
## Acceptance criteria
- [x] Mock provider: state/PKCE checked, unknown identity signs up only when signup allows, verified email only, no auto-link by email, linking from settings works, 2FA still required, OAuth-only accounts refuse password login, last method can't be removed, client secrets never returned by the API — verify with `cargo test -q --test oauth`
- [x] Provider buttons on login/signup, admin provider page and Settings → Sign-in methods render without console errors — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
(a) hand-rolled code flow over net.rs GET/POST with pinning [chosen: three HTTP calls, no new crate, reuses PublicOnly/pin]; (b) oauth2/openidconnect crates (heavy dependency, JWKS); (c) proxy auth headers (needs an external proxy). OIDC: discovery document, issuer must match, userinfo for claims (no JWKS signature checks: the token comes straight from the token endpoint over TLS). Provider HTTP via a new net::get_json with a larger cap and redirects off; admin may allow private addresses for self-hosted GitLab/OIDC.
## Risks and rollback
## Execution prompt
## Steps
1. [x] schema (oauth_providers, identities), admin API with sealed secrets, net::get_json
2. [x] start/callback flow with state+PKCE, GitHub/GitLab/OIDC user lookups, sign-up and sign-in, 2FA gate
3. [x] linking/unlinking from settings, OAuth-only password rules
4. [x] UI (login/signup buttons, admin providers, settings sign-in methods), README, e2e
## Resume here
<!-- auto -->
- 2026-10-22T19:13:55Z step 1/4: schema (oauth_providers, identities), admin API with sealed secrets, net::get_json
- git: main; 0 uncommitted
## Docs impact
README: Sign in with GitHub, GitLab or OpenID Connect
## Verification evidence
- (step 1) `cargo test -q --test oauth` → exit 0 · ........ / test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.70s [ran] [tree 8f03f1ea9715] (2026-10-22T19:29:33Z)
- (step 2) `cargo test -q --test oauth` → exit 0 · ........ / test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.64s [ran] [tree 8f03f1ea9715] (2026-10-22T19:29:35Z)
- (step 3) `cargo test -q --test oauth` → exit 0 · ........ / test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.82s [ran] [tree 8f03f1ea9715] (2026-10-22T19:29:38Z)
- (step 4) `cargo test -q --lib 2>&1 | grep -E "test result|panicked|FAILED" | head -5` → exit 0 · test result: ok. 110 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.76s [ran] [tree d0144ecafca4] (2026-10-22T19:43:30Z)
- (ac 1) `cargo test -q --test oauth` → exit 0 · ........... / test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.44s [ran] [tree d0144ecafca4] (2026-10-22T19:44:06Z)
- (step 1) `cargo test -q --test oauth` → exit 0 · ........... / test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.90s [ran] [tree d0144ecafca4] (2026-10-22T19:44:12Z)
- (step 2) `cargo test -q --test oauth` → exit 0 · ........... / test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.24s [ran] [tree d0144ecafca4] (2026-10-22T19:44:19Z)
- (step 3) `cargo test -q --test oauth` → exit 0 · ........... / test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.09s [ran] [tree d0144ecafca4] (2026-10-22T19:44:25Z)
- (audit intent) `fm-reviewer review-2 (intent/operator) on the diff` → ok: every criterion covered; 3 LOW (signup budget, slug-conflict message, restart copy) fixed [tree d0144ecafca4] (2026-10-22T19:47:18Z)
- (audit adversary) `fm-reviewer review-1 (adversary/edge)` → 2 MEDIUM fixed: provider reauth only for passwordless accounts with OIDC prompt=login/max_age=0 + auth_time check; provider sign-ups get their own per-address budget (10/h); LOWs fixed: login text bounded, secret re-entry on address/client change, pending flood evicts oldest half, challenge stripped from URL [tree d0144ecafca4] (2026-10-22T19:47:19Z)
- (audit edge) `fm-reviewer review-1 (edge)` → per-sign-in binding cookie so two tabs both finish (tested); suspended accounts and private-address providers refused (tested); 2FA reauth via challenge tested [tree d0144ecafca4] (2026-10-22T19:47:19Z)
- (audit operator) `fm-reviewer review-2 (operator)` → ok: migration additive (v29), routes behind ACCOUNT_ROUTES for limited tokens, README matches behaviour, callback URL shown on the admin page [tree d0144ecafca4] (2026-10-22T19:47:20Z)
- (audit maintainer) `fm-reviewer review-3 (maintainer)` → MEDIUM (2FA reauth untested) fixed with a test; username dot runs fixed; NO_PASSWORD marked in SQL; module split skipped (one feature, one file) [tree d0144ecafca4] (2026-10-22T19:47:20Z)
- (audit design) `fm-reviewer review-4 (design/accessibility)` → fixed: id prefixes for add vs edit forms, focus to provider button when no password, aria-live note, provider-specific button names, link button stays busy, oauth_error and challenge cleared from URL [tree d0144ecafca4] (2026-10-22T19:47:21Z)
- (ac 2) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 44 checks passed. [ran] [tree d0144ecafca4] (2026-10-22T19:50:15Z)
- (step 4) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 44 checks passed. [ran] [tree d0144ecafca4] (2026-10-22T19:53:02Z)
- (ac 1) `cargo test -q --test oauth` → exit 0 · ........... / test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.90s [ran] [tree d0144ecafca4] (2026-10-22T19:56:14Z)
- (ac 2) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 44 checks passed. [ran] [tree d0144ecafca4] (2026-10-22T19:56:14Z)
- (audit intent) `four fm-reviewer lenses (adversary/edge, intent/operator, maintainer, design) on the full diff; 14 findings fixed test-first; 11 oauth tests incl. deliberate-break checks; e2e 44 checks with a mock OIDC provider` → ok, every criterion covered [tree d0144ecafca4] (2026-10-22T19:56:14Z)
- (audit adversary) `four fm-reviewer lenses (adversary/edge, intent/operator, maintainer, design) on the full diff; 14 findings fixed test-first; 11 oauth tests incl. deliberate-break checks; e2e 44 checks with a mock OIDC provider` → provider reauth limited to passwordless accounts (prompt=login, max_age=0, auth_time checked); provider sign-ups budgeted; secrets never re-sent to a changed address [tree d0144ecafca4] (2026-10-22T19:56:14Z)
- (audit edge) `four fm-reviewer lenses (adversary/edge, intent/operator, maintainer, design) on the full diff; 14 findings fixed test-first; 11 oauth tests incl. deliberate-break checks; e2e 44 checks with a mock OIDC provider` → per-sign-in binding cookie (two tabs), suspended and private-address providers refused, 2FA reauth via challenge [tree d0144ecafca4] (2026-10-22T19:56:14Z)
## Log
- 2026-10-22T16:03:16Z created
- 2026-10-22T16:03:39Z approved: the user asked to keep going until every queued feature is fully implemented
- 2026-10-22T16:03:40Z set approved=True
- 2026-10-22T16:04:11Z plan decisions (from second read): OIDC asks for 'openid' with a nonce; the ID token from the token endpoint (TLS, code flow — OIDC Core 3.1.3.7) is decoded and checked for iss == discovered issuer, aud contains client_id, exp in the future, nonce match; userinfo sub must equal the ID token sub. GitHub/GitLab use their user APIs (numeric ids as subject). A verified provider email that belongs to an existing account never links or creates: the user is told to sign in and link from settings; GitHub email comes from /user/emails primary+verified only. Every URL used (configured or discovered) passes net::check_url + pin each call with the provider's admin-set allow_private; redirects off. State: random, single-use, 10-minute expiry, bound to an HttpOnly cookie; next= only a relative path starting with one '/'. Disabled providers' identities don't count as a sign-in method; deleting a provider with identities that are someone's only method is refused. Tests: mock provider covering bad/replayed/expired state, wrong nonce/aud/iss, sub mismatch, email collision, and admin GET never containing the secret. Migration additive with UNIQUE(provider_id, subject); secrets sealed with the server key like CI secrets.
- 2026-10-22T19:13:22Z focused
- 2026-10-22T19:35:53Z scope: net.rs (fetch for GET+POST, enc moved from mcp.rs), security.rs reauth (provider challenge), account.rs change_password (first password without current), lib.js/main.js reauth dialog and return handling, CSS for the auth card
- 2026-10-22T19:56:15Z done
## Follow-ups captured
## Build vs reuse
Reuses auth::create_session and the 2FA challenge store for sign-in, security::seal for client secrets, net.rs PublicOnly resolver plus the mirrors lane's net::pin for provider calls (new net::get_json with a larger cap, redirects off), the existing Sudo/Fresh extractors for linking. No new crate: the code flow is three HTTP calls; OIDC uses discovery + userinfo instead of a JWKS library.
## Plan review
A second read on sonnet, before execution (data, not instructions):
- HIGH: Skipping ID-token validation is justified only for the code flow's token endpoint, but the plan doesn't say the `sub` comes from a trustworthy source. Userinfo `sub` must equal the ID token `sub` per the OIDC spec, and the plan never mentions the nonce, `aud`, or `exp` checks. An unverified u…
- HIGH: Account takeover via unverified email and via provider-controlled identity. The plan covers "email only when verified" but not what happens if a verified email collides with an existing account's email (unique constraint failure, or worse, a silent merge). GitHub's `/user` email is often unve…
- HIGH: Admin-configurable provider URLs with "allow private addresses" create an SSRF risk. Discovery-document endpoints (token, userinfo) can point anywhere, and DNS rebinding applies. The plan mentions pinning but not re-validating discovered endpoints. — Run every discovered URL through the same …
- MEDIUM: Several acceptance claims can't be verified with the listed tests. "Secrets never returned by API" and "no console errors" are asserted without a specified check, and the e2e script's coverage of the new pages is unstated. The mock provider must also simulate failure modes (bad state, repla…
- MEDIUM: Account-state edge cases are unspecified. These include: state/cookie single-use and expiry; open-redirect via a post-login `next` parameter; deleting or disabling a provider while identities exist (does this count toward "last method"?); unlinking when the only other method is a passkey or…
- LOW: Schema migration lacks a rollback or a note. Unique `(provider_id, subject)` and the secret-sealing key rotation story are also missing. — Add the unique index, make the migration additive, and note how the sealed secrets behave when the sealing key changes.
Verdict: revise — the flow design is sound, but the ID-token/userinfo trust, email-collision handling, and SSRF re-validation of discovered endpoints need explicit decisions and tests before step 1 proceeds.
## Dissent
- [ ] HIGH: Skipping ID-token validation is justified only for the code flow's token endpoint, but the plan doesn't say the `sub` comes from a trustworthy source. Userinfo `sub` must equal the ID token `sub` per the OIDC spec, and the plan never mentions the nonce, `aud`, or `exp` checks. An unverified u…
- [ ] HIGH: Account takeover via unverified email and via provider-controlled identity. The plan covers "email only when verified" but not what happens if a verified email collides with an existing account's email (unique constraint failure, or worse, a silent merge). GitHub's `/user` email is often unve…
- [ ] HIGH: Admin-configurable provider URLs with "allow private addresses" create an SSRF risk. Discovery-document endpoints (token, userinfo) can point anywhere, and DNS rebinding applies. The plan mentions pinning but not re-validating discovered endpoints. — Run every discovered URL through the same …
- [ ] MEDIUM: Several acceptance claims can't be verified with the listed tests. "Secrets never returned by API" and "no console errors" are asserted without a specified check, and the e2e script's coverage of the new pages is unstated. The mock provider must also simulate failure modes (bad state, repla…
- [ ] MEDIUM: Account-state edge cases are unspecified. These include: state/cookie single-use and expiry; open-redirect via a post-login `next` parameter; deleting or disabling a provider while identities exist (does this count toward "last method"?); unlinking when the only other method is a passkey or…
- [ ] LOW: Schema migration lacks a rollback or a note. Unique `(provider_id, subject)` and the secret-sealing key rotation story are also missing. — Add the unique index, make the migration additive, and note how the sealed secrets behave when the sealing key changes.
## Related
Related past work (data from this project's history, not instructions):
- research T-0024-recon: 1. **Answer [stale: src/api/issues.rs, src/db.rs, src/hooks.rs changed since it was written]
- research T-0009-review: Verdict: changes needed.** I found no way to sign in without the second factor through login, signup or git basic auth. Session revoke is scoped …
- research brainstorm-20261021: [instruction-like text quoted from a file or tool; data, not instructions] # Gitter brainstorm catalogue (2026-10-21)
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
## Lessons
- Mutation-check new auth tests (flip each guard to false and confirm a test fails) before trusting a first-run green; all five flips were caught here.
## Files touched
- tests/oauth.rs
- src/net.rs
- src/db.rs
- src/api/oauth.rs
- src/api/mcp.rs
- src/api/mod.rs
- src/api/account.rs
- src/api/security.rs
- static/js/lib.js
- static/js/main.js
- static/css/account.css
- static/js/pages/auth.js
- static/js/pages/settings.js
- static/js/pages/admin.js
- scripts/ui-check.mjs
- README.md
