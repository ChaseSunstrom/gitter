**Verdict: changes needed.** Two HIGH findings. Both break the CI trust model that the "admins" CI setting and the built-in runner rely on.

## lens: adversary
- HIGH — src/api/orgs.rs:324-338 (`set_member`), src/ci/queue.rs:16 (`ADMIN_OWNED`) — Any org owner can add any user, including a site admin, as `owner` without that user's consent. `ADMIN_OWNED` then treats the org as admin-owned. That passes the `trigger::enabled` check (trigger.rs:22) and the `admins_only` claim filter (queue.rs:112), so the "admins" CI setting and the built-in host runner are bypassed — fix: drop the org clause from `ADMIN_OWNED` (orgs are never admin-owned), or require every org owner to be a site admin and added by one — confidence high.
  - Repro: non-admin alice runs `POST /orgs {name:acme}`, then `PUT /orgs/acme/members/<siteadmin> {"role":"owner"}`, then pushes `.gitter/ci.yml` to acme/x. With `ci=admins` the built-in runner claims the job and runs alice's code on the host.
- HIGH — src/ci/jobs.rs:50-58, src/ci/trigger.rs:114 — CI trust assumes the pusher is the repo owner, which collaborators and org members now break. Any `write` collaborator can push `.gitter/ci.yml`. `claim` attaches every repo secret to the job, and `enabled()` checks only the owner. On an admin-owned repo the job also runs on the built-in runner. That lets Write read secrets (`echo $S | base64`) and run code on the host, although the README and `administrable` reserve secrets for Admin — fix: give secrets and built-in-runner jobs only to runs pushed by a user with `can_admin`, otherwise run them without secrets or not at all — confidence high.
- LOW — src/api/orgs.rs:~243 (`show`), src/api/account.rs:651-653 — Org member lists and each user's `orgs` list are public to anonymous viewers, with no opt-out — fix: restrict `members` to org members, or document it as intended — confidence medium.
- LOW — src/api/admin.rs:303 — An admin deleting a sole org owner cascades away `org_members`, leaving an org with no owner. Only site admins can manage it afterwards, and `last_owner_check` (orgs.rs:~553) then blocks any removal — fix: refuse the delete, or promote a member — confidence medium.

## lens: edge
- LOW — scripts/e2e.sh:31 — The new base64 `bob:another horse` fixture has no `# pragma: allowlist secret`, unlike line 26. This is the pre-audit hit, and it is a test fixture, not a real secret — fix: add the pragma.
- Concurrency: DB calls run serialized in transactions, so I found no race. The migration is `ADD COLUMN` plus new tables, which is safe on an existing database. The `VISIBLE` 4-parameter change matches every caller, because all use `visible_params`.

## Acceptance criteria
The brief lists none. Checked against the task title instead.
- Roles read/write/admin enforced through `access()` and `administrable`: met. Unit tests and API tests cover it, and the scope_guard narrowing holds (admin is false for non-All tokens).
- Orgs with a base role: met, but CI ownership is unsafe (see HIGH findings).
- Plain-language access list: met.
- I did not run the tests.

## Implicit decisions
- Orgs are `users` rows with `is_org=1`, `pass_hash='!'` and an empty email.
- Membership is added without the user's consent.
- Org membership is public.
- Read collaborators can fork.
- Each user may own at most 10 orgs.
- `hidden` repos are visible only to admins and org owners.

## Not checked
Not read or run: the UI rendering of the JS files, `cargo test`, `smart_http`/`lfs` call sites beyond grep, and the changes to existing e2e flows.

**Noticed:**
- tests/api.rs has no test for the CI and org-ownership interaction, and none for the secrets boundary between write and admin.
- `remove_collaborator` lets a write-scope token remove its own collaborator row (orgs.rs:~345, `uid == u.id`). Minor, but it is a write the scope guard wasn't meant to allow.
