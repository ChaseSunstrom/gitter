**Verdict: changes needed.** One HIGH issue lets any user get code run on the server's built-in CI runner. I read the diff and the surrounding code but did not run any tests, `cargo`, `scripts/e2e.sh` or `scripts/ui-check.mjs`.

## lens: intent
- HIGH — src/ci/queue.rs:16 (used at 107 and src/ci/trigger.rs:22) — `ADMIN_OWNED` treats any org with a site admin among its owners as admin-owned. `orgs::set_member` (src/api/orgs.rs:528) lets any org owner add any user, including a site admin, as `owner` without their consent. Under the default CI setting `"admins"` (src/settings.rs:43), the built-in runner and `trigger::enabled` only run CI for admin-owned repos, so this bypasses that gate. Repro: sign up as a normal user, `POST /api/orgs {"name":"x"}`, then `PUT /api/orgs/x/members/<adminname> {"role":"owner"}`. Admin names are public, because the profile page shows an "Admin" tag. Then create a repo in `x`, push `.gitter/ci.yml`, and the job runs on the host. Fix: don't derive CI trust from membership. Use `EXISTS (SELECT 1 FROM orgs o JOIN users a ON a.id = o.created_by WHERE o.id = u.id AND a.is_admin = 1)`, or make adding an owner an invitation the invitee accepts. Add a test where a non-admin org owner adds an admin and CI is still refused. — high
- MEDIUM — src/api/repos.rs:54-66 — The repo-count limit, `storage_left` and the creation rate limit are all keyed on the owner id, and each org is its own owner. Up to 10 owned orgs (`MAX_ORGS_PER_USER`, src/api/orgs.rs:231) give a user about 11 times the repo count and storage quota. The limit only counts orgs you currently own, so deleting an empty org and creating another resets it. Fix: charge org repos to the creator's quota, or rate-limit org creation. — medium
- LOW — src/api/orgs.rs:400 and static/js/pages/orgs.js:1241 — The text says "delete or transfer/move repositories first", but no transfer feature exists. Existing repos can't be moved into an org, only created there. Fix: reword, or capture a transfer task. — high
- LOW — src/api/orgs.rs:454 and src/api/account.rs:651 — `GET /orgs/{name}` shows the full member list to anonymous visitors, and user profiles list every org a user belongs to. Membership of an org that only has private repos becomes public. Fix: show members only to org members, or add a public-membership flag. — medium
- LOW — src/api/admin.rs:303 — Admin `delete_user` cascades `org_members`. Deleting an org's sole owner leaves an org with no owner, which breaks the "last owner can't leave" rule. Site admins can still manage it. Fix: refuse, or promote an admin. — medium
- LOW — tests/api.rs:1589 — The "orgs can't sign in" test logs in with password `"!"`, the stored hash string. That fails to parse as a hash even without the `is_org = 0` filter, so it doesn't pin the filter. Fix: set a real Argon2 hash on the org row and assert 401 or 400. — medium

## lens: operator
- LOW — scripts/e2e.sh:193 — The pre-audit hit is a test password (`bob:another horse`), not a real secret. Dismissed, but the line lacks the `# pragma: allowlist secret` that the neighbouring lines carry. Fix: add it. — high
- LOW — src/api/orgs.rs:356 — `remove_collaborator` only needs read access to leave, so a write-scoped or deploy token can delete its own user's collaborator row. `token_may` allows writes under `/repos/o/n/collaborators`. This contradicts the README line "only full-access tokens administer". Fix: for a limited token, require `r.can_admin` even when removing yourself. — medium
- src/auth.rs adversary lens: the change is just `AND is_org = 0` in `password_login` (src/auth.rs:367). An org name behaves like an unknown user, and there is no other password path. Nothing wrong here.

## Acceptance criteria
- **Collaborator roles, revoke, admin-only settings:** met by the code. `administrable` gates settings, deploy tokens and secrets, and git uses `Repo::load` with `can_read` and `can_write`. The `collaborators_get_exactly_their_role` test matches the `collaborators` filter. I did not run it.
- **Organizations own repos, base permission, no sign-in, last owner, lists follow access:** met by the code. `VISIBLE` matches `repo::access`, including the hidden-repo rule. The test matches the `organizations` filter. Not run, and the sign-in assertion is weak (see above).
- **Org pages, repo access settings and owner picker in a browser via `bash scripts/e2e.sh`:** not verifiable from the code. The e2e additions only exercise the API. The pages are checked in `ui-check.mjs`, which this command may not run.

## Implicit decisions
- The default org `base_role` is `read`, so new members can see every private org repo.
- Org creation is open to any signed-in user, capped at 10 owned orgs.
- Members are added directly with no invitation or consent.
- Site admins count as org owners.
- Org members and collaborators are public information.
- Org names share the user namespace and the reserved-name list.
- The CI trust gate counts orgs with an admin owner as admin-owned.

## Not checked
Nothing was executed. I did not review the ledger or `.foreman` task files, or CSS and layout. Frontend behaviour beyond a read-through is unchecked.

**Noticed:**
- The dashboard "mine" list (src/api/account.rs:696) only shows repos you own, so repos shared with you through collaborator or org access show up only in explore and search.
- A write collaborator can read CI secrets by pushing a `.gitter/ci.yml`, even though the REST secrets routes are admin-only. Document it or restrict it.
