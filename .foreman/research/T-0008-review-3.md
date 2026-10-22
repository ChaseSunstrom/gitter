## Verdict
Changes needed. One HIGH finding is a privilege escalation, and it comes from the CI hunks that no plan step names.

## lens: maintainer — ok | changes needed

- HIGH — src/ci/queue.rs:16 (used at queue.rs:112 and trigger.rs:22) — `ADMIN_OWNED` treats an org as admin-owned if any of its owners is a site admin. `set_member` (src/api/orgs.rs:528-561) lets any owner add any non-org account as owner without that person's consent. — Drop the org clause from `ADMIN_OWNED`, so only users with `is_admin` and `is_org = 0` count. Or give orgs an explicit admin-granted "trusted" flag. Don't derive trust from owner membership. Add a test. — high confidence

  - file: src/ci/queue.rs
  - line: 16
  - summary: Org CI trust is inherited from owner membership, which any org owner can grant without the admin's consent.
  - short_summary: Org admin-owner trust is forgeable via set_member
  - failure_scenario: A non-admin attacker creates org `evil` (`POST /orgs`) and adds site admin root1 as owner (`PUT /orgs/evil/members/root1 {"role":"owner"}`). The attacker then creates a repo in `evil` (they are an owner, so `can_create_in` passes) and pushes `.gitter/ci.yml`. `trigger::enabled` returns true in the default "admins" mode. `queue::claim` also hands the job to the built-in runner, which accepts only admin-owned repos. The attacker's pipeline now runs on the server's host trust level, with no sandbox, under the T-0007 gating.
  - category: privilege-escalation
  - verdict: CONFIRMED

- MEDIUM — tests/api.rs (new tests) and src/api/mod.rs:342 (VISIBLE) — The org arm of `VISIBLE` has no test. The collaborator arm is covered only by the `/api/explore?q=p` check. Nothing checks that an org member sees an org repo in explore, search or the profile list, or that a hidden org repo is excluded for non-owners. `ADMIN_OWNED` has no test either. SQL and `access()` encode the same rule twice, so they can drift. — Add an explore/profile assertion for an org member with `base_role` read versus none, and for a hidden repo. Add a test that an org with an admin owner does not make a non-admin's repos CI-trusted. — high confidence

  - file: src/api/mod.rs
  - line: 342
  - summary: The org and hidden branches of the duplicated VISIBLE SQL rule are untested.
  - short_summary: VISIBLE org/hidden arms and ADMIN_OWNED untested
  - failure_scenario: Change `o.base_role IN ('read','write')` to `('write')` in VISIBLE. `cargo test` still passes, but org members with read lose repos in listings while `access()` still grants them.
  - category: test-coverage

- LOW — tests/api.rs:1589 — The "Organizations can't sign in" assertion is `assert_ne!(s, OK)` with password `"!"`. `'!'` is not a valid Argon2 hash, so `verify_password` fails whether or not `AND is_org = 0` is present (src/auth.rs:367). The test does not fail without the change. — Assert the 401 status. Or make the check meaningful by setting a real hash on the org row and expecting 401. — high confidence

  - file: tests/api.rs
  - line: 1589
  - summary: The org-login test passes even if the `is_org = 0` filter is removed.
  - short_summary: Org login test doesn't fail without the filter
  - failure_scenario: Remove `AND is_org = 0` from `password_login` and the test still passes, because the stored `'!'` hash never verifies.
  - category: weak-test

- LOW — src/repo.rs:48 — `role_level` is `pub` and never called (grep finds only its definition). `ROLE_SQL` repeats the same mapping in a SQL CASE. — Delete `role_level`. — high confidence

  - file: src/repo.rs
  - line: 48
  - summary: `role_level` is dead code.
  - short_summary: Unused pub fn role_level
  - failure_scenario: Nothing calls it. A future edit to one role mapping will leave the unused copy stale.
  - category: dead-code

- LOW — src/api/mod.rs:340-342 — The doc comment on `VISIBLE` is duplicated and stale: the old line "Binds one parameter (viewer id, or -1)" sits above the new line, though the constant now binds four. `pub mod orgs;` is also inserted out of alphabetical order at src/api/mod.rs:4. — Keep one comment, saying it takes `visible_params` (4 bindings). — high confidence

  - file: src/api/mod.rs
  - line: 340
  - summary: The VISIBLE doc is contradictory: "one parameter" above the new "takes visible_params".
  - short_summary: Stale duplicate doc on VISIBLE
  - failure_scenario: A new caller reads "binds one parameter" and passes one value, so the query fails at runtime with a bind-count error.
  - category: docs

- LOW — scripts/e2e.sh:193 — `BOB_AUTH=` carries no `# pragma: allowlist secret`, unlike the neighbouring line 188. The pre-audit "secret-looking value" hit is a test-password base64 and is dismissed, but it will trip the scanner. — Add the pragma. — high confidence

  - file: scripts/e2e.sh
  - line: 193
  - summary: A test credential line is missing the allowlist pragma used elsewhere.
  - short_summary: Missing allowlist pragma on test credential
  - failure_scenario: The secret scanner flags the line and blocks the commit or CI.
  - category: hygiene

## Acceptance criteria
No criteria were in the brief. Against the task title: roles read/write/admin: met (`access()` and the collaborator test). Orgs and plain-language access: met. The CI trust change is a regression of an existing security boundary (finding 1).

## Implicit decisions
- Org membership and the member list are public on profiles and in `GET /orgs/{org}`.
- Adding a member or collaborator needs no invitation or acceptance.
- Org-owned repos are CI-trusted if any owner is an admin.
- An org is a `users` row with `pass_hash = '!'`.
- Orgs are capped at 10 per owner.
- Org activity is never attributed to the org: events use the actor's id, so an org's "Recent activity" stays empty.

## Not checked
Whether `Sudo` works as intended for org delete. FK behaviour for the audit log when an org is deleted. Frontend rendering. Running the tests.

## Other observations
The pre-audit flag on src/auth.rs is only the `is_org = 0` filter and is fine.

Noticed: none
