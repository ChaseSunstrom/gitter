**Verdict: changes needed.** There are no HIGH findings, and the sandbox and the secret scoping hold. But the "closing or merging takes the preview down" promise has holes, and `pull_closed` over-reaches.

I read the diff and the working tree. I did not run anything.

## lens: adversary
- MEDIUM — src/api/deploy.rs:134-203,207-216 — A preview survives a PR that is closed or rerun while its job is in flight. `pull_closed` only clears rows that already have a `preview`, and only deactivates rows in `success` or `queued`, never `in_progress`. `settle` and `job_started` don't check the PR state. A closed or merged PR's preview then stays public until the 30-day sweep, and `POST /ci/runs/N/rerun` on a closed PR's run creates one too. Reproducer: use a job `preview: public` with `sleep 3` before the mkdir, open the PR, `PATCH issues/1 {"state":"closed"}` at once, then wait. `GET deployments?pr=1` shows `success` with a `preview`, and `GET /preview/<token>/` returns 200. Fix: in `job_started`, return `Err` when the run's `pr_number` is not an open PR. In `settle`, re-check the PR state and skip `publish` if it isn't open. Confidence: high.
- MEDIUM — src/api/issues.rs:641 — The "merged by hand" branch of the PR status handler sets `state = 'merged'` and never calls `deploy::pull_closed`. That branch runs on a plain GET of the PR status. Reproducer: open a PR with a preview, `git push origin feature:main`, then GET the PR status. The preview stays up and still shows on the PR page. Fix: call `pull_closed(st, r.id, number)` right after that UPDATE. Confidence: high.
- MEDIUM — src/api/deploy.rs:213 — `pull_closed` marks every deployment of the PR inactive, not just those in `preview/` environments. `job_started` (deploy.rs:153) stores `pr_number` for any environment. Reproducer: a PR job with `environment: staging` and no rule succeeds, so staging is live. Close the PR, and staging's latest deployment turns `inactive` and loses its Open link. Fix: add `AND environment_id IN (SELECT id FROM environments WHERE name LIKE 'preview/%')`. Confidence: high.
- MEDIUM — src/api/deploy.rs:261,297 — `put_environment` and `delete_environment` take `Authed` and write no audit event. Secret set and delete take `Sudo` and record. Rewriting the branch rule to `""` or deleting the environment (which cascades to its secrets) skips the password re-check. A stolen admin session can open the rule, push a branch with `environment: production`, and read the secrets, bypassing the Sudo gate. Fix: use `Sudo` and `record(...)` on both. Confidence: medium.

## lens: edge
- MEDIUM — src/api/deploy.rs:146 — `ensure()` in `job_started` has no cap, while the API paths cap environments at 100 (:273, :442). A writer can create unlimited environments through CI: matrix `environment: e-${{ matrix.n }}`, up to 100 per push, new names each push. `GET /environments` runs one query per environment, and the Deployments page renders them all. Deployment rows are never pruned either. Fix: apply the 100-environment cap in `ensure`, and fail the job with a clear message when it is exceeded. Confidence: high.
- LOW — src/validate.rs:25, src/lib.rs:160 — "preview" is now reserved, but an existing account or org named `preview` is not migrated. `/preview/<repo>` now 301s (cached by browsers) to `/preview/<repo>/`, which returns 404, and git paths under `/preview/…` are shadowed. Fix: check for existing "preview" accounts on upgrade, or give the route a prefix that can't be a username. Confidence: medium.

## Acceptance criteria
- Branch rules, environment secrets and agent runs getting none: met. `allowed` and `job_started` check the full ref, and the rule is evaluated before secrets load.
- Previews served sandboxed, with no `allow-same-origin`: met (preview.rs:137). `web.rs:247` only inserts a CSP when none is set, so it can't override this one. The session cookie is `SameSite=Lax`, so forms and popups from the sandboxed page carry no cookie.
- Previews taken down when the PR is closed or merged: not met (findings 1 and 2).
- Extraction and serving: met. Only regular files with `Normal` path components are extracted, symlinks are skipped, and `..` and `%2f` are covered by the test.

Pre-audit items dismissed:
- `token = [REDACTED] is an artifact of the diff tool. The real code is `Some(t)` at deploy.rs:189.
- The test secrets carry allowlist pragmas.
- `secret_name` was made `pub(crate)`, not removed.
- `innerHTML` is fed `html``…`.s`, which escapes its interpolations.
- The ui-check output follows the existing block pattern.

## Implicit decisions
- A new environment is open to every branch.
- A preview token is stored raw, unlike other tokens, which are hashed.
- Previews send `ACAO: *`, and `/preview/{token}` itself is not rate-limited.
- Anyone with read access can see preview tokens through the list API.

## Not checked
- Whether the runner uploads artifacts before calling `finish` on external runners.
- The sweep-versus-publish race on `NN.new` directories and on a preview with no token yet (preview.rs sweep): unverified.
- Test-gap lead: `tests/deploy.rs` has no case showing a by-agent run gets no environment secrets.

Noticed: Merge-queue runs (`refs/gitter/mq/…`) carry `pr_number`, so a `preview:` job in one would replace the PR's real preview. Worth its own task.
