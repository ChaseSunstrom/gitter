# Brainstorm — lens: user value

- **Add a "Why is my build red?" failure digest** — FEATURE — value 5 — effort M — risk low — category: CI UX
  Why: the pack covers log streaming and folding but not interpreting failures. The user's main loop is push, wait, red, scroll. Heuristic extraction (first error line, failing test name, exit code, diff vs last green run) is deterministic and needs no AI.
  Done when: a failing run's page and commit status tooltip show the first error, the failing step and "last green: <sha>", verified against a fixture log.

- **Add a "Re-run only failed jobs" and "Re-run with debug" button** — FEATURE — value 4 — effort S — risk low — category: CI UX
  Why: assumption: matrix pipelines make full re-runs costly; the pack mentions matrix and caches but no partial re-run.
  Done when: clicking re-run on a run with one failed job queues only that job and its dependents, and the run shows the earlier passes as reused.

- **Ship pipeline templates picked by repo detection** — FEATURE — value 4 — effort S — risk low — category: onboarding
  Why: the pack plans "pipeline file as minimal as a shell script" but not a starting point. The language bar is already planned, so detecting Cargo.toml, package.json or go.mod can offer a one-click `.gitter/ci.yml` commit through the web editor.
  Done when: a new Rust repo shows a "Add CI" banner that commits a working pipeline and the first run goes green.

- **Add a pipeline linter and dry-run validator in the web editor** — FEATURE — value 4 — effort M — risk low — category: developer experience
  Why: a custom YAML subset means users will make syntax errors, and learning them through failed pushes is slow. This differs from the existing `gitter ci run` idea in that it works with no runner at all.
  Done when: editing a pipeline file in the browser shows inline errors with line numbers, and `POST /api/ci/validate` returns the same diagnostics.

- **Auto-merge when checks pass** — FEATURE — value 5 — effort S — risk med — category: PRs
  Why: the pack has merge gating but not the "set and forget" step. Users who wait for CI and then click merge want the forge to do it. Needs care to re-check protection rules at merge time.
  Done when: enabling auto-merge on a PR with pending checks merges it within seconds of the last check turning green, and cancels if a new push lands.

- **Add a merge queue that tests the combined result** — FEATURE — value 4 — effort L — risk med — category: PRs
  Why: with merge-tree already planned for PR merges, a queue can build a temporary ref combining queued PRs and run CI on it, keeping main green. Assumption: busy repos will hit semantic-conflict breakage.
  Done when: two PRs that each pass alone but break together are caught, and the second is ejected from the queue with the reason.

- **Provide a notification inbox with email-free digests** — FEATURE — value 4 — effort M — risk low — category: notifications
  Why: issues, PRs, reviews and CI results generate events, and a self-hosted box often has no SMTP. An in-app inbox with mark-done and keyboard triage, plus optional webhook or RSS/Atom feeds per user, covers the case without mail setup.
  Done when: a mention and a failed CI run on my PR both appear in `/inbox`, and a per-user token-protected Atom feed lists them.

- **Add optional SMTP with a single settings form and test button** — FEATURE — value 3 — effort S — risk low — category: setup
  Why: password reset and signup verification are needed on a public site, and the pack's zero-config path doesn't say how these work. Making SMTP optional but one form away keeps the single-compose promise.
  Done when: entering SMTP details in admin settings and clicking "send test" delivers a message, and password reset works end to end.

- **Add repo-level "what changed since I last looked" summary** — FEATURE — value 3 — effort M — risk low — category: discovery
  Why: assumption: returning users to active repos mostly want new commits, PRs and CI changes since their last visit. Storing a per-user last-seen marker is cheap.
  Done when: visiting a repo after a gap shows a dismissible strip with counts of new commits, merged PRs and failed runs linking to filtered views.

- **Wild: Make a "fork and fix" flow from any failed CI log line** — FEATURE — value 3 — effort M — risk low — category: wild
  Why: wild. A stack trace that points into a file opens the web editor at that line, on a new branch, ready to open a PR. It chains log linkification, the web editor and the PR flow, all of which are already planned.
  Done when: clicking `src/lib.rs:42` in a log opens the editor at that line on a fresh branch.

- **Wild: Add a read-only "guest pass" link for private repos with expiry** — FEATURE — value 4 — effort M — risk med — category: wild
  Why: wild. Sharing a private repo or a single PR with a client or reviewer without creating an account is a constant friction on GitHub and GitLab. A signed, revocable, expiring link scoped to one repo gives that without extra user management.
  Done when: opening a guest link in a logged-out browser shows the repo read-only, and revoking it returns a 404 immediately.

- **Wild: Publish a per-instance "project health" page from existing data** — FEATURE — value 3 — effort M — risk low — category: wild
  Why: wild. Merge time, CI flake rate, median time-to-green and open-PR age are all computable from tables the plan already has. Showing them makes the "faster than GitLab" claim visible per team.
  Done when: `/org/repo/insights` renders those four numbers over 30 days from a seeded database.

- **Detect and flag flaky tests across runs** — FEATURE — value 4 — effort M — risk low — category: CI insight
  Why: assumption: CI that stores per-job results and logs can see the same commit pass and fail, or a test alternate across runs. Marking it flaky and offering "re-run, don't block" saves developers' time.
  Done when: a job that failed then passed on the same SHA is labelled flaky on the run page and counted in insights.

- **Add repo import by URL on the "New repo" page, with mirroring** — FEATURE — value 4 — effort M — risk med — category: onboarding
  Why: the pack covers import from GitHub and GitLab with issues and PRs, but a plain "paste any git URL and get a repo, optionally kept in sync" path is the fastest first success for a new self-hoster. SSRF protection is already listed, so it must reuse it.
  Done when: pasting a public HTTPS git URL creates a repo with full history, and a mirror repo updates within its configured interval.
