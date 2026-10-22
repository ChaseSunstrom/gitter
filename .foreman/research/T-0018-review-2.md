**Verdict** — changes needed. The diff file T-0018.diff is out of date against the working tree. I reviewed the working tree, so my line numbers are tree lines, not diff lines. I could not run anything (read-only tools only), so every finding comes from reading the code and none is test-confirmed.

I dismissed the pre-audit flags:
- **`ui-check.mjs` console.log:** the same pattern is used by the other blocks in that script.
- **"Secret-looking value" in `deploy.rs`:** the diff shows a redaction marker (`token = [REDACTED] where the tree has `token = [REDACTED]
- **`secret_name` and `auth.js`:** `secret_name` was only made `pub(crate)`, and `auth.js` only gained `'preview'` in an array.
- **`innerHTML` in `deployments.js`:** the strings go through the escaping `html` tag, and `safeUrl` guards the URLs.

Two things I suspected from the stale diff turned out to be fixed in the tree: a job finishing after its pull request closed, and a pull request detected as merged by hand. Both now retire the preview.

## lens: intent
- MEDIUM — src/api/deploy.rs:221-224 (also :190-192) — `pull_closed` marks every deployment of the pull request inactive, in any environment, not just `preview/pr-N`. A pull-request run with `environment: staging` therefore shows Inactive after the merge, and the Deployments page hides its URL while the site is still deployed. The same happens in `settle` (:190) when the job ends after the close. — Limit all three to the preview environment (`environment_id IN (SELECT id FROM environments WHERE name = 'preview/pr-' || ?2)`), or keep the status and only retire the preview token. — high
  - Reproducer: a `.gitter/ci.yml` with `on: pull_request` and a job `environment: staging`. Open a pull request, let the job succeed, merge it, then `GET /api/repos/o/r/deployments?environment=staging`. The status is `inactive`.
- LOW — static/js/pages/deployments.js:65,68 — History is built from `GET /deployments`, which returns the latest 100 rows for the whole repository. After 100 or more pull-request preview deployments, production's "earlier deployments" list is empty even though earlier rows exist. — Fetch `?environment=NAME` when an environment's history is opened. — high
- LOW — static/js/pages/deployments.js — The acceptance criterion says "settings manage environments", but environment management sits on the Deployments page and nothing was added to repository settings (`reposettings.js` has no environment section). — Link to it from repository settings, or confirm that is the intent. — medium
- LOW — src/ci/compat.rs:622-641 — GitLab `environment: {name: X, action: stop|prepare|verify|access}` is treated as a deploy because `action` is ignored. A stop job records a new success in X and retires the real current deployment. — Drop `environment` when `action` is anything other than `start`, and add a note. — medium

## lens: operator
- LOW — src/validate.rs:~4 (`RESERVED` now includes `preview`) — The reservation applies only at signup, with no migration check. An existing account or org named `preview` now has `/preview/<repo>` captured by the preview route. The route redirects to `/preview/<repo>/` and then returns 404, so that account's repository pages break after the upgrade. — Fail at startup or warn when such an account exists, or serve the preview under a path no account can use. — medium, PLAUSIBLE
- LOW — src/api/deploy.rs:147 — `ensure` creates environments with no cap in the CI path. The 100-environment cap exists only in `put_environment` and `create`. A writer can create unlimited environments by pushing configs with fresh `environment:` names, and `environments()` runs one query per environment for any reader. — Apply the same cap in `job_started`, failing the job with a message. — high
  - Reproducer: push one commit per new name, for example `environment: e1`, `e2`, and so on.
- LOW — src/api/deploy.rs:194-211 vs :215-231 — There is still a narrow race. `settle` reads the pull request state, then `publish` extracts for seconds, then it writes the token and success. A close or merge in between runs `pull_closed` before the token exists, so the preview stays live. — Re-check the pull request state inside the final DB closure and remove the directory if it is closed. — medium, PLAUSIBLE
- LOW — src/ci/jobs.rs:43-103 — The body of `loop {` is not indented, so `cargo fmt --check` would fail. Earlier reviews flagged the same class. — Run rustfmt. — high

## Acceptance criteria
- **Environments and deployments over the API, with job-following deployments:** not verifiable here. `tests/deploy.rs` exercises it, but I could not run it. The code in `deploy.rs` (`create`, `update`, `set_status`, `job_started`, `settle`) matches the criterion.
- **Environment secrets only to targeting jobs, with branch rules:** met by reading. `job_started` runs the rule check before inserting the deployment or reading secrets, and agent-started runs get none. The merge-queue and fork cases are covered by the context you gave.
- **Pull request preview:** mostly met. It is unguessable (32 random bytes), sandboxed without `allow-same-origin` (the session cookie is `SameSite=Lax`, so cross-site fetches and form posts from the preview carry no cookie), and removed on close or merge. The exception is the narrow race above.
- **Deployments page, settings, pull request page:** partly met; see the settings location and the history truncation above. I could not run `scripts/e2e.sh`.

## Implicit decisions
- Environments are created automatically with no branch rule when a job or the API first names them.
- Environment names are case-insensitive.
- The environment's secrets take precedence over the repository's secrets of the same name.
- Preview tokens are stored in plaintext and shown to anyone who can read the repository.
- Preview lifetime is 30 days (`ARTIFACT_DAYS`), measured from the deployment's `updated_at`.
- The preview CORS header is `*`.

## Not checked
I did not run any tests, `cargo fmt`, or clippy. I did not read `queue::finish`/`cancel`, the runner's artifact upload ordering, or the full Deployments page rendering.

**Noticed:**
- Preview files (up to 1 GiB each, per open pull request) sit outside the owner storage quota, as artifacts do.
- `extract` stops silently at `ARTIFACT_CAP`, which can publish a partial site as success (src/api/preview.rs:72).
