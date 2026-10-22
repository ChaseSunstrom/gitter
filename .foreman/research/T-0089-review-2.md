## intent: ok

- LOW — `src/api/oauth.rs:446` vs `src/api/account.rs:58` — Provider sign-up is limited to 30 starts per 10 minutes per IP bucket (`oauth:`). Password sign-up allows 5 per hour (`signup:`). A provider sign-up therefore gets a far looser per-address limit than the one the site already has for account creation. — Count the account-creating path under the existing key, e.g. `st.limiter.allow("signup:{bucket}", 5, 3600)` inside `signup()` in oauth.rs. — confidence medium (limited by needing distinct provider accounts)
- LOW — `static/js/pages/admin.js:1384` with `src/api/oauth.rs:966` — The admin form derives the slug from the button name and has no slug field. Adding a second GitHub or GitHub Enterprise provider with the default name returns "A provider with that short name exists". The user can't see a "short name" anywhere, so the message misleads. — Say "Give this provider a different button name" in the UI, or add a numeric suffix. Reproduce: add two `github` providers with a blank name. — confidence high
- LOW — `src/api/account.rs:201` — An OAuth account with 2FA whose challenge has expired sees "Enter your password again", but it has no password. — Reword to "Start again." for `restart`. — confidence high

## operator: ok

No findings. These all check out:
- The v29 migration is append-only. `enc` has one definition now, in `net.rs`, and `mcp.rs` imports it.
- The new `/me/identities` and existing `/auth` routes are in `ACCOUNT_ROUTES`, so limited tokens can't reach them.
- The README section matches the code: https-only unless `allow_private`, no redirects, `link` needs a fresh confirmation, `unlink` needs Sudo, and the last-method rule holds.
- `GITTER_URL` defaults to `http://localhost:3000`, and the admin page shows the real callback URL, so a wrong default is visible.

## Pre-audit items

- Secret-looking values (`s3cret-value`, `e2e-secret`) are test fixtures. Dismissed.
- The `console.log` calls in `scripts/ui-check.mjs` are that harness's normal reporting. Dismissed.
- "`enc` still named" is dismissed. Only `net.rs` defines it.
- The security-sensitive files were reviewed and I found no defect:
  - `change_password` still requires Sudo.
  - Password login can't match the `!` hash. The org rows already use it.
  - Suspended users are excluded by `user_by_id`.
  - The state is single-use and bound to the browser by cookie.
  - `local()` blocks open redirects.
  - The `html` template escapes the `innerHTML` use.

## Acceptance criteria

- **Mock-provider criterion: not verifiable here** (I'm read-only and didn't run `cargo test`). The code covers every listed point, and `tests/oauth.rs` has a test for each:
  - state and PKCE are checked, and a state works once
  - sign-up needs both the provider and the site to allow it
  - only verified emails are kept, and a taken email refuses instead of linking
  - linking and unlinking work
  - 2FA is still asked for
  - OAuth-only accounts have no password to log in with
  - the last way to sign in can't be removed
  - the API returns `has_secret` and never the secret
- **Buttons render without console errors: not verifiable here.** `ui-check.mjs` covers the sign-in page, the admin page and the Settings sign-in methods list, and fails on console errors.

## Files vs steps

Every hunk supports an acceptance criterion; none is scope creep. The plan just doesn't name the supporting files. `account.rs` is the first password for OAuth-only accounts. `security.rs` is reauth through a provider. `db.rs` is the migration. `net.rs` is GET support. The rest are `lib.js`, `main.js`, `auth.js` and the CSS.

## Implicit decisions

- OIDC ID-token signatures aren't verified (back channel only). It's documented.
- Pending sign-ins live in memory and a restart drops them.
- The username comes from the provider login, with `-N` appended if taken.
- Changing a provider's `base_url` or `client_id` keeps its linked identities.
- Provider accounts get `pass_hash '!'`.
- Only verified email is stored.

## Not checked

I ran no tests, `scripts/e2e.sh`, clippy or a build. I did not read the tests in `tests/oauth.rs` past the mock and the test names.

**Noticed:** none
