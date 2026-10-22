---
id: T-0089
type: FEATURE
tier: L
status: planned
priority: normal
scope: []
depends_on: [T-0084]
source: user
created: 2026-10-22T16:03:16Z
updated: 2026-10-22T16:04:11Z
---
# Sign in with GitHub, GitLab or any OpenID Connect provider
## Raw request
> Sign in with GitHub, GitLab or any OpenID Connect provider
## Interpretation
Admins add sign-in providers (GitHub, GitLab incl. self-managed, generic OIDC) with a sealed client secret in their own table, not the settings blob. Login and signup pages show a button per enabled provider. The OAuth code flow uses state plus PKCE S256 held server-side and bound to a short-lived cookie. Identities are matched by the provider's stable subject id, never by email. A new identity creates an account only if signup allows it (closed/invite still apply), with an unusable password and the email only when the provider marks it verified. Linking to an existing account happens only from a signed-in session (Settings → Sign-in methods, with sudo), never automatically by email. Accounts with 2FA still pass the 2FA challenge. OAuth-only accounts can't use password login until they set a password; the last sign-in method can't be removed.
## Assumptions (confidence)
## Acceptance criteria
- [ ] Mock provider: state/PKCE checked, unknown identity signs up only when signup allows, verified email only, no auto-link by email, linking from settings works, 2FA still required, OAuth-only accounts refuse password login, last method can't be removed, client secrets never returned by the API — verify with `cargo test -q --test oauth`
- [ ] Provider buttons on login/signup, admin provider page and Settings → Sign-in methods render without console errors — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
(a) hand-rolled code flow over net.rs GET/POST with pinning [chosen: three HTTP calls, no new crate, reuses PublicOnly/pin]; (b) oauth2/openidconnect crates (heavy dependency, JWKS); (c) proxy auth headers (needs an external proxy). OIDC: discovery document, issuer must match, userinfo for claims (no JWKS signature checks: the token comes straight from the token endpoint over TLS). Provider HTTP via a new net::get_json with a larger cap and redirects off; admin may allow private addresses for self-hosted GitLab/OIDC.
## Risks and rollback
## Execution prompt
## Steps
1. [ ] schema (oauth_providers, identities), admin API with sealed secrets, net::get_json  <- CURRENT
2. [ ] start/callback flow with state+PKCE, GitHub/GitLab/OIDC user lookups, sign-up and sign-in, 2FA gate
3. [ ] linking/unlinking from settings, OAuth-only password rules
4. [ ] UI (login/signup buttons, admin providers, settings sign-in methods), README, e2e
## Resume here
## Docs impact
## Verification evidence
## Log
- 2026-10-22T16:03:16Z created
- 2026-10-22T16:03:39Z approved: the user asked to keep going until every queued feature is fully implemented
- 2026-10-22T16:03:40Z set approved=True
- 2026-10-22T16:04:11Z plan decisions (from second read): OIDC asks for 'openid' with a nonce; the ID token from the token endpoint (TLS, code flow — OIDC Core 3.1.3.7) is decoded and checked for iss == discovered issuer, aud contains client_id, exp in the future, nonce match; userinfo sub must equal the ID token sub. GitHub/GitLab use their user APIs (numeric ids as subject). A verified provider email that belongs to an existing account never links or creates: the user is told to sign in and link from settings; GitHub email comes from /user/emails primary+verified only. Every URL used (configured or discovered) passes net::check_url + pin each call with the provider's admin-set allow_private; redirects off. State: random, single-use, 10-minute expiry, bound to an HttpOnly cookie; next= only a relative path starting with one '/'. Disabled providers' identities don't count as a sign-in method; deleting a provider with identities that are someone's only method is refused. Tests: mock provider covering bad/replayed/expired state, wrong nonce/aud/iss, sub mismatch, email collision, and admin GET never containing the secret. Migration additive with UNIQUE(provider_id, subject); secrets sealed with the server key like CI secrets.
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
