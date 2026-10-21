**Verdict: changes needed.** I found no way to sign in without the second factor through login, signup or git basic auth. Session revoke is scoped to the caller's own sessions, and the client-side templates escape output. These are the verified findings.

## lens: adversary
- HIGH — src/auth.rs:362 — `Fresh` returns true for any token-authenticated request (`None => true`), so `Sudo` and `Fresh` never apply to access tokens. A leaked CI token can then call `POST /api/me/2fa/disable`, `/me/2fa/recovery`, `DELETE /api/repos/{o}/{r}` and `PATCH /admin/users/{u}` with `reset_2fa`, with no password or code. It can also call `/me/2fa/setup` then `/enable` with the attacker's own secret. That turns 2FA on for the attacker, gives them the recovery codes, and locks the owner out. No Origin check applies, because that check only runs when a session cookie is sent (src/web.rs:146). Reproducer: `curl -X POST -H "Authorization: [REDACTED]" $BASE/api/me/2fa/disable` returns 200. Fix: have the 2FA management routes, `reauth`, admin `reset_2fa` and `delete_user` accept a browser session only, and make `Sudo` reject token auth for those routes. — high confidence

- HIGH — src/api/account.rs:364-384 — `change_password` needs only the current password, with no 2FA code and no rate limit. It then mints a session with `verified_at = now` (src/auth.rs, `create_session`). Scenario: an attacker holds a stolen session cookie (outside the sudo window) plus the victim's phished password. They POST `/api/me/password` and get a fresh, sudo-valid session without ever entering a TOTP code. They can then call `2fa/disable` or delete repos. This skips the code that `reauth` demands at src/api/security.rs:240. The same route also lets someone guess the current password with no lockout, unlike `reauth`, which locks after 10 failures. Fix: gate `change_password` on `Sudo` (or require the 2FA code), apply the `reauth:{uid}` limiter to it, and create the new session with `verified_at = 0`. — high confidence

- MEDIUM — src/api/account.rs:186-201 and src/api/security.rs:233-245 — the limit is checked first (`blocked`) and the failure is counted only after the code is verified (`limiter.fail`). Sending N concurrent requests with the same challenge and N different codes lets all of them through before the first failure is counted. Both the 5-per-challenge cap and the 10-per-user cap are bypassed, and a burst's cost is only DB round-trips (no argon2). With the password in hand, a 10k-request burst gives about a 3% chance per burst of hitting one of the three valid TOTP codes. Fix: count the attempt before verifying (`allow`) and reset it on success, or take a per-user async mutex around the check. — medium confidence (read from the code, not run)

- MEDIUM — src/db.rs:18 and src/api/security.rs:269 — `users.id` is `INTEGER PRIMARY KEY` with no AUTOINCREMENT, so deleting the newest user frees its id for reuse. The audit table has no foreign keys and is keyed on `actor_id` and `subject_id`. A new signup that reuses the id sees the deleted user's whole security log in `/me/audit` (IPs, repo names, token names, 2FA events), and the admin view shows the new username on the old events. Fix: use `AUTOINCREMENT` on `users` (needs a migration), or store a never-reused account uid in the audit rows. — high confidence

- MEDIUM — src/security.rs:250-266 and static/js/pages/admin.js (audit view text) — the chain is unkeyed SHA-256 with no stored head or count. Deleting the newest rows leaves `verify` reporting "intact". Anyone with write access to the database can edit rows and recompute the chain. The admin page's claim that "edits or deletions would show here" is false for tail deletions. Fix: use an HMAC keyed from a secret held outside the database (or periodically print the chain head), and store a row count or head anchor. — high confidence

- LOW — src/api/security.rs:264-269 — `/me/audit` shows `ip` and `actor` on events where you are the subject. An admin who suspends or resets you, or a collaborator who changes visibility on your repo, has their IP shown to you. Fix: blank the IP for rows where the actor is not you. — medium confidence

- LOW — src/api/security.rs:156 and src/api/security.rs:187 — `revoke`, `revoke_others`, `delete_token` and admin `put_settings` need only `Authed`, not `Sudo`. A stolen session cookie can sign the owner out of every other session, or delete their CI tokens, without a re-check. Fix: require `Sudo` on `revoke_others` at minimum. — medium confidence

## Acceptance criteria
- Not verifiable here: I did not run any of the five verify commands, only read the code.
- 2FA at sign-in, code replay refused, recovery codes working once, and git password auth refused for 2FA accounts all look met in code (`use_code`, `totp_last_step`, auth.rs:267).
- Session list and revoke look met. Step-up looks met for repo delete, repo visibility, token create and 2FA changes, with the token and `change_password` bypasses above.
- The audit chain exists, but its tamper-evidence is weaker than the UI claims (see the chain finding).

## Implicit decisions
- Tokens count as always "fresh".
- The TOTP secret is stored in plaintext in the database.
- The TOTP window is ±1 step and the sudo window is 600 seconds.
- Recovery codes use a roughly 31-letter alphabet, 8 characters.
- Challenges live in memory only, so a restart invalidates them.
- Audit writes are best-effort and outside the action's transaction, except in `update_user` and `delete_user`.

## Not checked
I did not run the tests or the Playwright script. I did not review Cargo.lock or crate versions, the README, the CSS, or the ui-check python/sqlite helper.

**Noticed:** `password_login` matches `email = ?1` while emails are not unique-constrained against usernames, so a user can set their email to another user's username. Pre-existing and worth its own task.
