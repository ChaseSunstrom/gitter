## maintainer: changes needed

**Verdict:** changes needed. No HIGH findings. Findings 1 and 2 break README promises and finding 3 is a test gap on a security rule. I confirmed these by reading the code; I did not run the reproducers.

Working tree matches the diff. The `[REDACTED]` at `deploy.rs:299` in the diff is a diff artifact, and the real line is `token = [REDACTED]

**Pre-audit, dismissed:**
- The `console.log` calls in `ui-check.mjs` follow that file's own pattern.
- The `pragma: allowlist secret` values are test fixtures.
- `secret_name` only became `pub(crate)`.
- The `auth.js` change only adds `preview` to `RESERVED`. The `.innerHTML` use in `deployments.js` and `issue.js` is safe, because `html` escapes values and `.s` is its output.

**Findings**

- MEDIUM — `src/api/issues.rs:641` — The "merged by hand" path (head already in base) sets `state = 'merged'` but never calls `deploy::pull_closed`. The README promises merging takes the preview down. Reproducer: a PR with a live preview, then merge locally and push base. Load the PR page, which runs this status code. The PR shows merged, and `/preview/<token>/` still returns 200 for up to 30 days. Fix: call `pull_closed` after that UPDATE.
  - file `src/api/issues.rs`, line 641, category `missing-cleanup`, CONFIRMED
  - failure_scenario: PR with preview, base then contains head, status check marks it merged, preview and `success` deployment stay.

- MEDIUM — `src/api/deploy.rs:174-203` (`settle`) — `pull_closed` ignores `in_progress` deployments, and `settle` publishes without checking the PR is still open. Reproducer: start a slow `preview:` job, close the PR while it runs, let it pass. `GET deployments?pr=N` then shows `success` with a live preview URL. Fix: in `settle`, if `pr_number` is set and the issue isn't open, skip `publish` and set `inactive`.
  - file `src/api/deploy.rs`, line 174, category `race-condition`, CONFIRMED
  - failure_scenario: PR closed while its job runs, job finishes, closed PR is served for 30 days.

- MEDIUM — `src/api/deploy.rs:158-162` and `tests/deploy.rs` — Withholding environment secrets from agent-started runs (`by_agent`) has no test. Deleting the `if !by_agent` arm leaves every test green. Also untested: `allowed()` with `refs/tags/v*` patterns, `set_status` out-of-order completion, and the `land_merge` hook. Fix: add an agent-run test asserting the log has no `prod=***`, plus a table test for `allowed`.
  - file `src/api/deploy.rs`, line 158, category `missing-test`, CONFIRMED

- LOW — `src/api/deploy.rs:143` — `job_started` doesn't check `l.event`. `merge_queue` runs carry `pr_number` (`trigger.rs:325`) and use the pull_request filter (`trigger.rs:98`). A `preview:` job in a queue run replaces the PR's real preview with the queue commit's, under ref `refs/gitter-queue/N`. Fix: `if preview && l.event == "pull_request"`.
  - file `src/api/deploy.rs`, line 143, category `wrong-trigger`, PLAUSIBLE

- LOW — `src/api/deploy.rs:371-420` — `put_environment` and `delete_environment` use `Authed`, with no `Sudo` and no audit `record`. The secret endpoints do both. Setting `branches` to `''` opens production secrets to every writer's branch. Fix: use `Sudo` and record `environment_rule_changed` and `environment_deleted`.
  - category `inconsistent-guard`, CONFIRMED

- LOW — `src/api/preview.rs:146` — `urlencoding` is a copy of `crate::net::enc` (`net.rs:36`). Reuse it.

- LOW — `src/api/deploy.rs:47`, `src/ci/config.rs:632`, `src/ci/compat.rs:635` — The http(s)+length+whitespace URL rule is written three times, and a fourth copy lives in `checks.rs:139`. Add one shared helper.

- LOW — `src/api/preview.rs` `sweep` (about line 150) — `parse::<i64>()` fails on `12.new`, so the hourly sweep deletes an in-flight publish's temp dir. It can also delete a just-renamed dir before `settle` writes its token. Fix: skip `*.new`, and skip dirs younger than a few minutes.

- LOW — `src/api/deploy.rs:548` — `create` stores the raw `refname`, so it can be `refs/heads/main`. The CI path stores the stripped name. Store the stripped name in both.

- LOW — `src/ci/jobs.rs:43-103` — The new `loop` body isn't re-indented. Extract the post-claim step into a function or run rustfmt.

- LOW — `static/js/pages/deployments.js:1388,1399` — `loadSecrets();` isn't awaited. A rejection there is unhandled. Use `await loadSecrets()`.

## Not checked
- I didn't run the build, tests, rustfmt or UI.
- I didn't do the adversary pass on sandbox escape, CSRF or cookie SameSite. I took the brief's trust model as given.
- I didn't check the migration number.

**Noticed:**
- `issues.rs:625-648`: a read-only status GET writes merged state, so the hand-merge path has side effects. Worth its own task.
- A deployment whose `job_id` becomes NULL when its job is deleted can sit `in_progress` forever, and the PATCH endpoint then accepts it.
