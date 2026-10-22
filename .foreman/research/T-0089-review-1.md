**Verdict: changes needed.** The core flow holds up. State is single-use, bound to the browser, and PKCE-protected. The `next` redirect is validated, ID-token claims and `sub` are checked, and suspended accounts are refused. Two findings (the sudo downgrade and the missing signup throttle) should be fixed before merge.

## lens: adversary

- MEDIUM — src/api/oauth.rs:245 (+ static/js/lib.js reauth modal) — provider "reauth" is offered and accepted for accounts that have a password. It sets `verified_at` with no credential, and the authorize URL has no `prompt=login` or `max_age=0` (line 268 only adds `nonce`). Fix: in `start`, reject `reauth` when `has_password(uid)` is true (this matches the README), and add `prompt=login&max_age=0` for OIDC reauth. — confidence high
  - file: src/api/oauth.rs, line 245
  - summary: Provider reauth bypasses the password check on accounts that have one.
  - short_summary: Provider reauth weakens sudo for password accounts
  - failure_scenario: Alice has a password, no 2FA, and a linked GitHub account. Someone at her unlocked browser opens Settings, clicks a sudo-gated action, then "Confirm with GitHub". GitHub auto-approves its remembered session and `verified_at` is set with no password. They then call `/me/password` (the current password is still required there, so they use a different sudo-gated action such as adding an SSH key or token). Before this change the password stopped them.
  - category: auth-bypass
  - verdict: CONFIRMED

- MEDIUM — src/api/oauth.rs:480 — provider sign-up skips the `signup:<ip>` limiter (5 per hour) that password sign-up uses (account.rs:58). Only `start`'s 30 per 10 minutes applies. Fix: call the same limiter in `signup()` before the insert. — confidence high
  - file: src/api/oauth.rs, line 480
  - summary: Provider sign-up has a much weaker per-IP throttle than password sign-up.
  - short_summary: OAuth signup bypasses the signup rate limit
  - failure_scenario: One IP loops start and callback with throwaway GitHub accounts and creates up to 30 accounts per 10 minutes, versus 5 per hour by password.
  - category: rate-limit
  - verdict: CONFIRMED

- LOW — src/api/oauth.rs:360, 456, 523 — the provider's `login` has no length or control-character filter. It goes into `identities.login` and the audit `detail` (and `display_name` is only length-capped). An OIDC `preferred_username` can be up to 1 MiB. Fix: strip control characters and truncate to about 100 characters in `text()` or `Who` construction. — confidence medium

- LOW — src/api/oauth.rs:805 and `apply` — `update` keeps the stored secret when `base_url` or `client_id` changes. The saved secret is then sent to the new address, and existing identities (matched by `sub` within the provider) now trust the new IdP. Fix: require a new secret when either field changes. — confidence medium

- LOW — src/api/oauth.rs:183 — a flood past 10,000 pending states runs `clear()`, which drops every in-flight sign-in. Rotating IPv6 /64s gets around the per-IP limit. Fix: evict the oldest entries instead of clearing. — confidence low

- LOW — static/js/pages/auth.js:91 (`codeStep`) — the `?challenge=` value stays in the URL and history, and `reauth_challenge` is stripped only on later navigations. It is only a half-factor with 5 tries. Fix: `history.replaceState` the parameter away. — confidence medium

## lens: edge

- LOW — src/api/oauth.rs:213 and the callback `bound` check — there is one `gitter_oauth` cookie for all flows. A second tab or provider click overwrites it, so the first flow fails with the misleading "started in another browser". The same happens when users reach the site on a host other than `cfg.url` (the cookie is host-only). Fix: key the cookie by state, for example `gitter_oauth_<state8>`, or improve the message. — confidence high
- LOW — the profile exchange makes up to 3 sequential 10-second outbound calls inside the callback request (src/api/oauth.rs, `profile`). The browser waits up to ~30 seconds on a slow provider, and it is not wrong on the happy path.
- LOW — tests/oauth.rs has no case for a wrong or missing binding cookie, a suspended user, a disabled provider, or refusal of private or http addresses with `allow_private` false.

## Acceptance criteria
The brief lists none. I checked against the README claims:
- Matching by provider id, not email: met (src/api/oauth.rs:656).
- Linking and unlinking need a recent confirmation: met.
- Provider confirmation "for accounts without a password": **not met** (see the first finding).
- State works once, expires after 10 minutes, bound to the browser: met.
- OIDC issuer, audience, expiry and nonce checks: met.

## Implicit decisions
- The OIDC token endpoint is authenticated with `client_secret_post`.
- ID-token signatures are not verified (OIDC Core 3.1.3.7).
- Pending sign-in state is in memory only and lost on restart.
- One identity per (provider, user).
- Account names are `login`, made unique with `-N` (N up to 99).
- Provider reauth reuses the 2FA `Challenges` store.

## Not checked
- The remaining ~560 lines of tests/oauth.rs, beyond its test names.
- Behaviour of the CSRF/Origin middleware on `start`.
- How `strip`, `html` and `toast` behave outside what I read.
- Pre-audit flags: the secret-looking values are test fixtures and masks. The `ui-check` console lines are the script's normal output.

Noticed: none
