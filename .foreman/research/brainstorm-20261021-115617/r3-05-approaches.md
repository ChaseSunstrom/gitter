# Brainstorm — lens: approaches

- **Build on a Forgejo fork behind a fast front layer** — FEATURE — value 4 — effort M — risk med — category: approaches
  Why: assumption: Forgejo already ships issues, PRs, wiki, packages, Actions and an `act`-based runner. Adopting it covers "all features" quickly, and the Rust SPA and cache layer would sit on top. The trade-off is Go and a second codebase, and the speed gain is limited by what its backend can do. The pack only shows the from-scratch plan.
  Done when: a prototype compose file serves Forgejo through the Gitter frontend, with a recorded page-load comparison against stock Forgejo.

- **Ship CI as a status API plus a reference runner first** — FEATURE — value 4 — effort S — risk low — category: do-less
  Why: T-0002 is only "captured" and T-0001 is still active, so CI risks delaying the core. Build commit statuses, check runs and a runner token API first. A ~300-line reference runner executes `.gitter/ci.sh`. Richer pipeline syntax follows later, and external CI like Woodpecker already works against the API.
  Done when: an external script posts a status through the API and it appears on the commit list and gates a PR merge.

- **Define a pluggable executor protocol for runners** — FEATURE — value 4 — effort M — risk low — category: CI architecture
  Why: the pack lists Docker socket, rootless, bubblewrap, Firecracker and subprocess as separate ideas. One small JSON-over-stdio contract (prepare, run step, stream logs, cleanup) lets them all be plugins, and the default can be the simplest.
  Done when: the same sample pipeline passes on the `subprocess` and `docker` executors using the same runner binary.

- **Use cross-document view transitions and speculation rules instead of an SPA router** — PERFORMANCE — value 4 — effort M — risk med — category: frontend
  Why: the plan is a History-API SPA with hover prefetch. A streamed server-rendered multi-page app with native `@view-transition` and Speculation Rules gives instant navigation with less JS, correct deep links, and no hydration bugs. The trade-off is Chromium-first behaviour and weaker Safari and Firefox support.
  Done when: navigating between repo, tree and blob pages animates without a JS router and a Playwright test reports a transfer size under the budget.

- **Open an existing Forgejo/Gitea data directory in place** — FEATURE — value 3 — effort M — risk med — category: migration
  Why: the pack covers API-based import from GitHub/GitLab but not people already running a small forge. Reading its bare repos and SQLite schema directly gives a no-copy migration, with a read-only dry run first.
  Done when: pointing Gitter at a Gitea fixture volume lists its repos, users and issues without a re-clone.

- **Use the repo's Dockerfile or devcontainer.json as the CI environment** — FEATURE — value 3 — effort M — risk low — category: CI
  Why: assumption: many repos already describe their toolchain this way. This removes the need to learn a pipeline syntax and gives caching through image layers. The trade-off is that cold builds are slower.
  Done when: a repo with only `.devcontainer/devcontainer.json` and a `ci.sh` gets a green check with no other pipeline file.

- **Run agentless runners over SSH** — FEATURE — value 3 — effort M — risk med — category: runners
  Why: the existing ideas all need a registered runner agent. Letting the server SSH into any host that has Docker means a user can add a spare machine with one key and no install. The trade-off is that the server holds the credentials, so scope them per host.
  Done when: adding a host and a key in settings runs a job there with no gitter binary installed on it.

- **Generate server routes, CLI, docs and the JS client from one OpenAPI spec** — CLEAN — value 3 — effort M — risk low — category: API design
  Why: the pack asks for a REST API, an OpenAPI doc, a `gitter` CLI and a Gitea-compatible subset. A single spec avoids the four drifting apart.
  Done when: CI fails if a route lacks a spec entry or the generated client is stale.

- **Compile pipelines to BuildKit graphs** — PERFORMANCE — value 3 — effort L — risk high — category: CI caching
  Why: BuildKit already provides content-addressed step caching, parallelism and remote cache. That could replace a custom input-hash cache. The trade-off is a `buildkitd` dependency, which weakens the one-service story.
  Done when: a two-job pipeline re-run with unchanged inputs finishes with every step reported as cached.

- **Wild: Let pushers' machines run CI and submit signed attestations** — FEATURE — value 3 — effort L — risk high — category: wild CI
  Why: wild. `git push` runs the repo's pipeline locally (building on `gitter ci run`), signs the result, and the server only verifies and gates on it. Cost on the server drops to near zero. The trade-off is trust, so require a second independent attestation for protected branches.
  Done when: a push with a valid attestation turns the check green and one with a tampered log is rejected.

- **Wild: Have the instance edit its own compose file** — FEATURE — value 3 — effort M — risk high — category: wild ops
  Why: wild. A small updater sidecar lets the admin toggle runner count, HTTPS or registry in the UI. It rewrites the compose file, shows the diff, and applies it after approval, so the file stays the single source of truth. It needs the Docker socket, and only on the sidecar.
  Done when: toggling "add a runner" in the UI produces a reviewed compose diff and a running second runner.

- **Wild: Make the whole instance a git-pushable deployment** — FEATURE — value 2 — effort M — risk med — category: wild ops
  Why: wild. `git push` to a special `instance` repo on the host's Gitter reconfigures and upgrades it, so there is no SSH and no CLI to learn. Config and compose live in that repo and history gives free rollback.
  Done when: pushing a changed settings file in the instance repo changes live behaviour and `git revert` restores it.

- **Add a decision matrix doc and a spike benchmark per approach** — CLEAN — value 2 — effort S — risk low — category: process
  Why: assumption: several approaches above (Forgejo base, gitoxide, BuildKit) are unproven. A time-boxed spike for each, measured against the same latency and RAM budget, prevents committing early.
  Done when: a doc in the repo records the measured RAM, cold start and push-to-green time for each spike and names the winner.
