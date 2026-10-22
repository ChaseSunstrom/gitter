**Verdict:** approve with nits. I found no CRITICAL or HIGH defects. I did not run the tests or the build, so everything below comes from reading the diff.

## lens: maintainer
- MEDIUM — `src/api/security.rs:241` — The new `/auth/reauth` challenge branch has no test. This covers a 2FA account confirming through a provider, the `reauth_challenge` redirect, a challenge belonging to another user, and an expired challenge. `tests/oauth.rs` only covers the no-2FA reauth. Fix: add a test with a 2FA user. Do a reauth-mode `sign_in` and check that `location` carries `reauth_challenge`. Then POST `/api/auth/reauth {challenge}` and expect 401 `need_code`. Retry with a valid code and expect 200, and with another user's challenge and expect 401. — confidence high
- LOW — `src/api/oauth.rs:472` — `username_base("a...b")` returns `"a..b"`, because `replace("..", ".")` runs once. `validate::name` rejects `..`, so all 99 candidates fail and sign-up says "Couldn't pick a username". Reproducer: add `("a...b", "a.b")` to the unit test. Fix: collapse runs of `.` in a loop or fold. — confidence high
- LOW — `static/js/pages/admin.js:137` — `providerFields` is used in both the add form and the edit modal, so ids like `pv-name` and `pv-id` are duplicated while the modal is open. Label `for=` then targets the field behind the modal. Fix: pass an id prefix. — confidence high
- LOW — `src/api/oauth.rs:774` — `update` lets an admin change `base_url` or `client_id` and keeps the sealed secret. The next sign-in then posts the secret to the new address, which breaks the "never shown again" promise. Fix: require `client_secret` when either field changes. — confidence medium
- LOW — `static/js/lib.js:137` — `.catch(() => {})` on `GET /me/identities` is silent. If it fails, a password-less account sees a password field that can never work, with no hint. Fix: show an error in `#ra-err`. — confidence medium
- LOW — `src/api/oauth.rs:625,828` — The `'!'` literal is hard-coded in SQL while `NO_PASSWORD` exists, and `orgs.rs:229` also uses `'!'`. Fix: bind `NO_PASSWORD` as a parameter. — confidence medium
- LOW — `src/api/oauth.rs:331,59` — `finish` needs `#[allow(clippy::too_many_arguments)]`. `provider(slug: Option, id)` matches on `slug = ? OR id = ?` with a dummy `0`. The file is 862 lines and mixes the sign-in flow with admin CRUD. Fix: bundle the request context in a struct, split lookup by slug and by id, and move the admin CRUD to its own module. — confidence medium

## Acceptance criteria
The brief lists none beyond the task title ("Sign in with GitHub, GitLab or any OpenID Connect provider").
- The behaviours in `tests/oauth.rs` appear to be covered. They are PKCE, single-use state, the browser binding cookie, ID-token claim checks, only verified emails kept, a taken email refusing sign-up, closed and invite sign-up, 2FA after the provider, link and unlink with the last-method rule, and provider removal being refused when it would strand someone. I did not run them (not verifiable by reading).
- The reauth-with-2FA path is not covered (see the MEDIUM finding).

## Pre-audit items
- Dismissed: the "secret-looking" values are fixtures (`e2e-secret`, `s3cret-value`).
- Dismissed: the `console.log` lines in `scripts/ui-check.mjs` follow that script's reporting convention.
- Dismissed: the `innerHTML` in `lib.js` uses `html```, which escapes its values.
- Dismissed: `enc` was moved to `net.rs`, not removed.

## Files vs steps
The unnamed files all serve the feature, so none is scope creep. The plan just lacks steps for the migration, `net::fetch`, the reauth change, `auth.js`, `lib.js`, `main.js`, CSS and `ui-check`. The `mcp.rs` change is a small move of `enc`.

## Implicit decisions
- `client_secret_post` is the only token-endpoint auth method.
- Pending sign-ins live in memory and are cleared whole at 10,000 entries.
- There is one `gitter_oauth` cookie, so two tabs signing in at once break the first.
- A provider-only account has `pass_hash = '!'`.
- Provider reauth works even for accounts that have a password, and there is no `prompt=login`.
- ID-token signatures are not verified (OIDC Core 3.1.3.7).

## Not checked
Whether the build, clippy or tests pass. `static/css`, the middle of `ui-check.mjs`, and the lines of the tests file beyond a skim.

**Noticed:** the 2FA `challenge` stays in the `/login?challenge=` URL, in history and Referer, until it expires. `auth.js` could `replaceState` it away once it is read.
