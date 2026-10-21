# Brainstorm — lens: simplicity

- **One-binary, one-volume compose** — FEATURE — value 5 — effort M — risk low — category: deployment
  Why: the user's hard constraint is "single simple docker compose file"; the stack (axum + SQLite + git CLI) already allows one service and one `/data` volume.
  Done when: `docker compose up -d` on a clean host with only the compose file yields a working site with a single service and a single volume, and the file is under 25 lines.

- **Embed the runner inside the server binary** — FEATURE — value 5 — effort M — risk med — category: CI
  Why: T-0002 wants docker runners, and the compose must stay simple. A built-in runner mode (`gitter runner`, or on by default in the same process) means no second image and no registration step for the common case.
  Done when: pushing a repo with a pipeline file runs a job on the same compose deployment with no extra service defined, and logs appear in the UI.

- **Zero-config first boot** — FEATURE — value 5 — effort S — risk low — category: onboarding
  Why: assumption: self-hosters are put off by GitLab's setup (secrets, external URL, SMTP). Auto-generate the secret key, and make the first registered user the admin. The only optional env var is `GITTER_URL`.
  Done when: starting with an empty volume and no env vars works, the first signup becomes admin, and the secret persists across restarts.

- **Pipeline file as minimal as a shell script** — FEATURE — value 4 — effort S — risk low — category: CI
  Why: T-0002 says "pipelines defined in repo". A format whose smallest valid form is `image: rust` plus `run: cargo test` beats a sprawling YAML schema, and anything more complex is optional.
  Done when: a 2-line `.gitter/ci.yml` runs successfully, and the docs list the whole schema on one screen.

- **Runner auto-registration via shared volume/socket** — FEATURE — value 4 — effort M — risk med — category: CI
  Why: GitLab runners need a token copy-paste step. For extra runners in the same compose file, a token generated into the shared volume avoids it. Assumption: runners are mostly colocated.
  Done when: adding a `runner` service block with no token lets it appear in the admin runner list within 10 seconds.

- **Built-in backup and restore command** — FEATURE — value 4 — effort S — risk low — category: operations
  Why: assumption: a SQLite plus bare repos layout makes backup a simple job, but users shouldn't have to know that. `gitter backup out.tar` uses SQLite `VACUUM INTO` for a consistent snapshot.
  Done when: backup then restore into a fresh volume gives identical repos, users and issues, verified by an e2e test.

- **Drop the external dependencies: no Redis, no Postgres, no job queue service** — CLEAN — value 4 — effort M — risk low — category: architecture
  Why: simplicity is the stated advantage over GitLab. Keep the job queue, sessions, and caches in SQLite tables, and enforce this with a test that the compose file defines only the app (and optionally runner) services.
  Done when: the app runs on a fresh volume with no network access to other services, and a CI check fails if a second infrastructure service is added to the default compose.

- **Single `gitter doctor` self-check** — FEATURE — value 3 — effort S — risk low — category: operations
  Why: a simple setup still needs debuggability. One command checks the git version, the docker socket, disk space, the data volume's writability, and port reachability, then prints fixes.
  Done when: `docker compose exec gitter gitter doctor` prints pass or fail per check, and a missing docker socket gives an actionable message.

- **Settings in the UI, not in env files** — FEATURE — value 3 — effort M — risk low — category: configuration
  Why: assumption: apart from the URL, everything (signup policy, limits, runner count) is better as a persisted admin page than as config files, which removes the config surface from the compose file.
  Done when: toggling "disable signups" in the admin page takes effect without restart and survives a restart.

- **Wild: Tiny `curl | sh` that writes the compose file and starts it** — FEATURE — value 3 — effort S — risk med — category: onboarding — wild
  Why: assumption: "simpler than GitLab" could mean one command. The script prints the compose file it will write before running, and the file stays the source of truth.
  Done when: running it on a fresh VM gives a working HTTPS site, and the generated file is readable and equals the documented one.

- **Wild: Built-in automatic HTTPS via ACME** — FEATURE — value 4 — effort M — risk med — category: deployment — wild
  Why: a public site normally needs a reverse proxy container, which is a second service. Embedding ACME (e.g. rustls-acme) keeps it to one container.
  Done when: setting `GITTER_DOMAIN` makes the site serve a valid certificate on 443 with no proxy service in the compose file.

- **Wild: Warm-start "zero-install" runner jobs using the server's own container as the sandbox** — FEATURE — value 3 — effort L — risk high — category: CI — wild
  Why: assumption: many users have no docker socket they'd want to hand over. A rootless sandbox (e.g. a user namespace via bubblewrap) for simple jobs means no socket mount, which is safer and simpler.
  Done when: a job with no `image:` runs sandboxed with no docker socket mounted and cannot read other repos' data.

- **Single-page "why it's fast" benchmark in the repo** — CLEAN — value 2 — effort S — risk low — category: performance
  Why: "wayyy faster" is a claim that needs a number. A script that measures cold start, memory idle, and clone time keeps the simplicity bet honest.
  Done when: `scripts/bench.sh` prints idle RSS under a stated bound and page response times, and runs in CI.
