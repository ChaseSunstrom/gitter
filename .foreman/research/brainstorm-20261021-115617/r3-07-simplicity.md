# Brainstorm — lens: simplicity

- **Generate the compose file from one question** — FEATURE — value 4 — effort S — risk low — category: onboarding
  Why: user wants "a single simple docker compose file"; the existing ideas cover bootstrap and `curl | sh`, but not a minimal template with exactly one required value. Assumption: a hosted generator or `gitter init` is acceptable.
  Done when: `gitter init --url https://git.example.com` writes a compose file under 25 lines that brings up server and runner and passes the health check with no further edits.

- **Fold the runner into the server process by default** — FEATURE — value 5 — effort M — risk med — category: architecture
  Why: the user wants "runners, ci, all features" without GitLab-style service sprawl. Existing ideas separate the runner as a subcommand; this makes the default mode zero extra containers, with external runners opt-in. Assumption: sandboxing holds up in-process.
  Done when: a fresh one-service compose runs a pipeline end to end, and `docker compose ps` lists exactly one container.

- **Auto-detect the pipeline with no config file** — FEATURE — value 4 — effort M — risk low — category: CI
  Why: T-0002 says "pipelines defined in repo", and simplicity means not requiring YAML first. Detecting Cargo.toml, package.json or Makefile and running the obvious build and test goes past the `.gitter/ci.sh` idea. Assumption: detection heuristics cover the common cases.
  Done when: pushing a bare Cargo project with no CI file produces a green status check from `cargo test`.

- **Show one settings page with a plain-language effect preview** — FEATURE — value 3 — effort M — risk low — category: admin
  Why: "settings in the UI" is listed, but nothing shows what a toggle will do. Each toggle states its RAM, disk and exposure impact before saving. Assumption: costs can be estimated cheaply.
  Done when: flipping "enable CI for everyone" shows a predicted resource and risk line before confirm.

- **Make every error carry its own fix command** — FEATURE — value 4 — effort S — risk low — category: DX
  Why: "fail loudly at boot" is covered for boot only. Extending to runtime and web errors (full disk, runner offline, bad token) reduces support load for a single admin.
  Done when: each error class in a catalog test has a non-empty one-line remedy, and the UI renders it.

- **Cut the feature set into three named tiers** — CLEAN — value 3 — effort S — risk low — category: scope
  Why: the user wants "all features" yet simpler than GitLab. Core, CI and extras tiers with a single `GITTER_TIER` switch keep the surface comprehensible. Assumption: feature flags are cheap to gate.
  Done when: at the core tier, the CI and registry routes return 404 and the idle RAM figure is lower than at the full tier.

- **Use the same URL scheme for everything** — CLEAN — value 3 — effort S — risk low — category: API
  Why: web, git, API, runner and logs all hang off one origin and path convention, so reverse proxy and firewall rules need no per-service ports. Assumption: no conflicts with the SSH port.
  Done when: one proxy rule and one open port suffice for web, clone, API and runner traffic in the e2e script.

- **Delete rarely used knobs by choosing defaults** — CLEAN — value 3 — effort S — risk low — category: config
  Why: the "five env vars" idea limits the count but not which knobs exist. A policy that every option must justify itself with a measured need keeps the config small. Assumption: this is enforced in review.
  Done when: a test fails if the documented env var count exceeds the agreed cap.

- **Wild: Make the compose file self-documenting with a live `gitter explain`** — FEATURE — value 2 — effort S — risk low — category: DX
  Why: wild. Running it prints the running topology, volumes, ports and what each does in plain words, derived from the actual state rather than docs.
  Done when: `gitter explain` output matches `docker compose config` for services and volumes in a test.

- **Wild: Make the instance upgrade itself from a git tag in its own repo** — FEATURE — value 4 — effort L — risk high — category: ops
  Why: wild. Gitter hosts its own release repo; a signed tag triggers pull, migration with snapshot and restart, which removes the "docker compose pull" step. Assumption: the signature trust root is manageable.
  Done when: pushing a signed tag to the self-repo upgrades a test instance and rolls back automatically on a failed health check.

- **Wild: Offer a phone-first "pocket mode" setup over a QR-paired session** — FEATURE — value 3 — effort M — risk med — category: onboarding
  Why: wild. A new admin finishes setup, adds the domain and invites teammates from a phone, with the terminal needed only for the first `up`.
  Done when: a Playwright mobile-viewport test completes setup from the first-run QR link.

- **Show one status screen for the whole instance** — FEATURE — value 4 — effort S — risk low — category: ops
  Why: "doctor", health and metrics ideas exist separately; a single green/amber/red page over storage, runner, backup and certificates matches the simplicity brief. Assumption: it reuses the existing health endpoints.
  Done when: killing the runner turns its row amber with a fix hint within 10 seconds.
