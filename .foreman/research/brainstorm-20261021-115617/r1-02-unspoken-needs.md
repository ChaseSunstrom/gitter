# Brainstorm — lens: unspoken needs

- **Ship a first-run bootstrap that makes the compose file actually one-command** — FEATURE — value 5 — effort M — risk low — category: onboarding
  Why: user insists on "single simple docker compose file"; the unspoken need is that `docker compose up` yields a working site with admin, secret keys, and a runner registered, with zero hand-edited env files.
  Done when: on a clean host, `docker compose up -d` followed by opening the URL shows a working site with a setup page (or printed one-time admin link) and a registered runner, with no manual config.

- **Auto-register the bundled runner with a generated token** — FEATURE — value 5 — effort M — risk med — category: CI
  Why: "runners from the same compose file" is a constraint; assumption: users otherwise hit the GitLab-style token copy-paste dance.
  Done when: a pushed `.gitter/ci.yml` with `echo hi` runs on the compose-bundled runner without any token entered by hand, and logs stream to the UI.

- **Sandbox CI runners without exposing the host Docker socket** — SECURITY — value 5 — effort L — risk high — category: CI safety
  Why: the site is public and multi-user, and docker-based runners mean untrusted code execution. Mounting docker.sock hands over the host. Assumption: the user hasn't thought about this yet.
  Done when: a CI job that tries to read host files, reach the Gitter DB, or use the Docker socket fails; resource limits (CPU, memory, time) are enforced and tested.

- **Add one-command backup and restore, including a scheduled snapshot** — FEATURE — value 5 — effort M — risk low — category: operations
  Why: SQLite plus bare repos on disk means a naive volume copy mid-write can corrupt; self-hosters expect safe backups. Assumption: not yet planned.
  Done when: `gitter backup out.tar` on a running instance yields an archive that restores onto a fresh instance with identical repos, issues, and users, verified in e2e.

- **Add abuse controls for a public site (rate limits, signup gating, quotas)** — SECURITY — value 4 — effort M — risk med — category: abuse
  Why: the pack says "public multi-user site, so abuse/safety matters" but lists only push size limits. Missing are signup modes (open/invite/approval), per-user repo and storage quotas, login throttling.
  Done when: an admin setting switches signup to invite-only; a scripted burst of 100 logins is throttled; a user over quota gets a clear push rejection.

- **Publish a measured speed benchmark against GitLab/Gitea on the same box** — FEATURE — value 4 — effort M — risk low — category: proof of speed
  Why: "wayyy faster" is the headline claim; the unspoken need is to be able to prove it and catch regressions.
  Done when: a `scripts/bench.sh` reports cold-start time, idle RAM, page TTFB, clone time, and a CI run on a fixed repo, and a perf budget check fails the build on regression.

- **Provide a zero-downtime upgrade path with automatic migrations and a pre-upgrade snapshot** — FEATURE — value 4 — effort M — risk med — category: operations
  Why: self-hosters pulling `:latest` need upgrades that don't lose data; the SQLite schema will evolve fast in a greenfield project.
  Done when: upgrading from tagged image N to N+1 with a populated DB migrates automatically, takes a snapshot first, and rolls back cleanly if migration fails (tested).

- **Built-in HTTPS with automatic certificates via compose** — FEATURE — value 4 — effort M — risk med — category: deployment
  Why: a public website needs TLS; assumption: the user won't want to hand-write a reverse proxy config, which would break "simple".
  Done when: setting `DOMAIN=git.example.com` in compose gets a valid certificate and HTTP→HTTPS redirect with no extra services beyond one optional container or built-in ACME.

- **Support SSH clone alongside HTTPS** — FEATURE — value 4 — effort L — risk med — category: git protocol
  Why: the plan covers only smart HTTP; most developers expect `git@host:` and SSH keys. Assumption: they will ask once they try to migrate real repos.
  Done when: a user adds an SSH key in settings and `git clone ssh://…` and push work against the compose stack on one exposed port.

- **Add import from GitHub/GitLab (repos, issues, PRs)** — FEATURE — value 4 — effort L — risk med — category: migration
  Why: the intent is to replace those products; adoption needs a migration path. Assumption: not yet planned.
  Done when: pasting a public GitHub repo URL creates a Gitter repo with history, issues, and labels, verified against a fixture repo.

- **Wild: Make a "push to deploy preview" URL for every branch/PR** — FEATURE — value 4 — effort L — risk high — category: CI delight
  Why: wild; assumption: the runner already builds containers, so exposing a built static site or container per PR on a subdomain is a natural extension that GitHub lacks natively.
  Done when: a PR with a static-site CI job gets a clickable preview URL in its status check that serves the built output and expires on close.

- **Wild: Run the whole site read-only from a git-clone-able "mirror mode" for offline/LAN use** — FEATURE — value 2 — effort M — risk low — category: resilience
  Why: wild; assumption: a self-hosted instance that pulls from upstream and serves the fast UI on a LAN or in a plane gives the speed pitch another use.
  Done when: a mirror instance pointed at another Gitter keeps repos in sync and serves browse/clone with the origin offline.

- **Wild: Offer a `gitter` CLI that talks to the instance like `gh` (PRs, issues, CI logs in the terminal)** — FEATURE — value 3 — effort M — risk low — category: CLI
  Why: wild; the surface is listed as "library or CLI", and the unspoken need is staying in the terminal; token auth already exists in the plan.
  Done when: `gitter pr create` and `gitter ci logs -f` work against a live instance using a personal access token.

- **Add a CI cache and artifact store with automatic expiry** — FEATURE — value 3 — effort M — risk med — category: CI
  Why: "way faster" CI depends on dependency caches; unbounded artifacts would also fill the disk on a single-box deployment.
  Done when: a second pipeline run restores a cached dependency directory and is measurably faster; artifacts older than the configured retention are removed.
