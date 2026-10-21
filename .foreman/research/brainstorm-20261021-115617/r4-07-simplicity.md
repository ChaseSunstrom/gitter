# Brainstorm — lens: simplicity

- **Enforce a one-service, one-volume, one-port compose lint in CI** — CLEAN — value 4 — effort S — risk low — category: simplicity budget
  Why: assumption: the "simpler than GitLab" requirement drifts as features land; existing ideas cap env vars but don't gate the compose file itself.
  Done when: a test parses the shipped docker-compose.yml and fails if it has more than 1 service (runner folded in), more than 1 named volume, more than 1 published port, or more than 5 env vars.

- **Merge `gitter doctor` and `gitter explain` into one `gitter status` that prints effective config** — FEATURE — value 4 — effort S — risk low — category: operability
  Why: the idea list has doctor, explain, status page and "what is this instance doing" as separate surfaces; one command is simpler than four.
  Done when: `gitter status` prints every effective setting with its source (default/env/UI), the moving-parts count, and a pass/fail per self-check, exit nonzero on failure.

- **Collapse all permissions to three roles: read, write, admin** — FEATURE — value 4 — effort M — risk med — category: access model
  Why: assumption: GitLab's permission matrix (guest/reporter/developer/maintainer/owner) is a big source of complexity; the pack plans orgs, teams, CODEOWNERS and rulesets which would otherwise multiply levels.
  Done when: every permission check in the codebase resolves to one of three roles via a single function, and a test enumerates all routes against that function.

- **Make the data directory the only config: settings stored in SQLite, env only for `GITTER_URL`** — CLEAN — value 4 — effort M — risk low — category: configuration
  Why: the user asked for a "single simple docker compose file"; env files are the usual sprawl. Builds on the planned settings-in-UI idea but forbids the fallback.
  Done when: removing every env var except `GITTER_URL` from compose still boots and every other setting is changeable in the admin UI and survives restart.

- **Ship a "defaults ledger" that justifies every setting that exists** — CLEAN — value 3 — effort S — risk low — category: simplicity budget
  Why: assumption: knobs accrete. A required entry (default, why, who'd change it) per setting makes adding one a deliberate act.
  Done when: a test fails if any setting key in code lacks an entry in `docs/settings.md`, and the ledger is rendered on the admin page.

- **Unify issues, PRs and CI runs into one "thread" model with one timeline component** — CLEAN — value 4 — effort L — risk med — category: data model
  Why: the plan has issues, PRs, review comments, status checks, and deployments each with their own views; a shared timeline of events halves tables, routes and UI code.
  Done when: issues and PRs share one table and one timeline renderer, with CI status shown as timeline events, and existing e2e flows pass unchanged.

- **Run CI jobs as plain subprocesses by default and make containers an opt-in per-job `image:` line** — FEATURE — value 5 — effort M — risk med — category: CI simplicity
  Why: Docker-socket access is the biggest compose complication; the pack lists subprocess mode but not as the default path with image as the escalation.
  Done when: a repo with `.gitter/ci.sh` runs green on a stock one-service compose with no socket mounted; adding `image: alpine` to the config runs the same job in a container when a socket is available, and gives a clear error when it is not.

- **Generate the README's quickstart from the e2e script so docs cannot drift** — CLEAN — value 3 — effort S — risk low — category: docs
  Why: no README exists yet; the simplicity promise is only as true as the documented steps, which should be the ones tested.
  Done when: the quickstart block in README is produced by a script from the commands e2e.sh executes, and CI fails if they differ.

- **Wild: Ship the whole forge as a single static binary that also works with no Docker at all (`./gitter`)** — FEATURE — value 4 — effort M — risk low — category: distribution
  Why: wild; compose is the requested path, but a double-click binary with embedded runner proves nothing else is hiding.
  Done when: downloading one file and running it on a clean Linux box serves a working instance with clone, push and a subprocess CI run.

- **Wild: Add a "delete a feature" admin switch that truly removes tables, routes and UI for what you don't use** — FEATURE — value 3 — effort L — risk high — category: modularity
  Why: wild; extends the pack's feature-flag idea so a wiki-less, CI-less instance is genuinely smaller and faster, not just hidden.
  Done when: disabling CI makes its routes return 404, its tables go unused, and idle RSS measurably drops in the benchmark.

- **Wild: Make `docker compose up` print the one URL and one password, then nothing else** — FEATURE — value 3 — effort S — risk low — category: first-run polish
  Why: wild; a deliberately silent log (warnings only) makes the simplicity visible and every extra line a bug.
  Done when: first boot logs at most three lines to stdout and subsequent boots log one line, enforced by a test counting lines.

- **Cap the codebase's direct dependency count and fail the build above it** — CLEAN — value 3 — effort S — risk low — category: simplicity budget
  Why: assumption: fewer crates means faster builds, smaller binary and less supply-chain exposure, which fits a minimal-moving-parts product.
  Done when: CI runs a script counting direct dependencies in Cargo.toml and fails if above the committed threshold, which must be raised in a reviewed change.

- **Use one identifier scheme: every object has a short human slug that works in URL, CLI, API and commit messages** — FEATURE — value 3 — effort M — risk low — category: consistency
  Why: extends "same URL scheme for everything"; adds cross-references like `owner/repo#12` resolving identically everywhere including CI logs.
  Done when: `owner/repo#12` auto-links in issues, PR descriptions, commit messages and CI logs, and `gitter open owner/repo#12` resolves the same target.
