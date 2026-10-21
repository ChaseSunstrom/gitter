# Brainstorm — lens: user value

- **One-file compose with zero-config first run** — FEATURE — value 5 — effort M — risk low — category: self-hosting
  Why: user said "single simple docker compose file, simpler than gitlab". Assumption: one service, one volume, sensible defaults (secret auto-generated on first boot, first registered user becomes admin).
  Done when: `docker compose up -d` on a clean machine with only that file serves a working site, and the first signup is admin, with no .env edits required.

- **Bundle the runner in the same compose file** — FEATURE — value 5 — effort M — risk med — category: CI
  Why: the pack says runners must work from the same compose file. Assumption: a second service (or a `--runner` mode of the same binary/image) registers itself automatically over an internal token.
  Done when: after `compose up`, a pushed repo with a pipeline file runs a job on the bundled runner with no manual registration step.

- **Pipelines defined in the repo with live log streaming** — FEATURE — value 5 — effort L — risk med — category: CI
  Why: T-0002 asks for repo-defined pipelines, docker runners and streaming logs. Assumption: simple YAML file (`.gitter/ci.yml`) with steps, image, env, `needs`.
  Done when: a push triggers a run, and the browser shows log lines arriving within about a second, ending in pass/fail.

- **Commit and PR status checks with merge gating** — FEATURE — value 4 — effort M — risk low — category: CI
  Why: T-0002 names status checks on commits and PRs. Assumption: branch protection can require named checks.
  Done when: a failing pipeline shows a red check on the PR and the merge button is disabled until the check passes (or an admin overrides).

- **Instant-feeling pages (performance budget enforced in tests)** — PERFORMANCE — value 5 — effort M — risk low — category: speed
  Why: user wants "wayyy faster" and "more responsive". Assumption: the benchmark is GitHub/GitLab page loads.
  Done when: an e2e test fails if repo tree, blob or commit-log responses exceed e.g. 50 ms server time on a 10k-file repo, or if the initial JS payload exceeds a set size.

- **Low-RAM guarantee, with a published number** — PERFORMANCE — value 4 — effort S — risk low — category: speed
  Why: the pack contrasts GitLab's ~4GB RAM. Assumption: Gitter's idle target is under ~100MB.
  Done when: a script starts the stack, runs a clone/push/CI workload, and asserts idle and peak RSS under stated limits.

- **Public-site abuse protection built in** — SECURITY — value 4 — effort M — risk med — category: safety
  Why: the pack says a public multi-user site, so abuse matters. Assumption: rate limits, signup throttling, push size and repo quotas, CI job limits.
  Done when: tests show that excess logins, signups and oversized pushes get 429 or rejection, and a CI job is killed at its CPU, memory or time limit.

- **Sandboxed CI for untrusted code** — SECURITY — value 5 — effort M — risk high — category: CI safety
  Why: a public site running arbitrary pipelines is a big risk. Assumption: jobs run unprivileged, without the Docker socket, with a network policy and no host mounts.
  Done when: a test job that tries to read host files, reach the Docker socket or fork-bomb is contained and fails cleanly.

- **One-command backup, restore and upgrade** — FEATURE — value 4 — effort S — risk low — category: ops
  Why: self-hosters need safety with SQLite plus bare repos. Assumption: a consistent snapshot of the DB and repos.
  Done when: `gitter backup` produces one archive, and restoring it onto a fresh volume yields identical repos and data.

- **Import from GitHub/GitLab** — FEATURE — value 4 — effort M — risk low — category: migration
  Why: a new host needs content. Assumption: URL plus token imports repos, issues and PRs.
  Done when: importing a sample public repo produces matching commits, branches and issues.

- **Built-in HTTPS via automatic certificates** — FEATURE — value 4 — effort M — risk med — category: self-hosting
  Why: a public website behind a single compose file shouldn't need a separate proxy. Assumption: ACME in the binary, optional.
  Done when: setting `GITTER_DOMAIN` yields a valid TLS certificate and an HTTPS redirect without a reverse-proxy container.

- **Wild: Warm CI with snapshot caching** — FEATURE — value 4 — effort L — risk high — category: CI speed
  Why: wild; the "wayyy faster" demand applied to CI. Assumption: prewarmed containers and layer or dependency caches keyed by lockfile hash make typical jobs start in under a second.
  Done when: a second run of the same pipeline starts its first step in under 1 s and reuses the cache.

- **Wild: Run the pipeline locally with `gitter ci run`** — FEATURE — value 4 — effort M — risk low — category: dev loop
  Why: wild; the same runner logic as a CLI, so you can debug before pushing. Assumption: the CLI shares the executor.
  Done when: running it in a repo executes the same steps as the server and gives the same result.

- **Wild: "Time-travel" repo page** — FEATURE — value 3 — effort M — risk low — category: delight
  Why: wild; a slider over history that animates the tree and file changes, using the view transitions already planned.
  Done when: dragging the slider over a repo's commits updates the tree view to each commit's state without a full reload.

- **Wild: Instance-in-a-box demo mode** — FEATURE — value 3 — effort S — risk low — category: onboarding
  Why: wild; `compose up` with a flag seeds sample users, repos and a passing pipeline so the first minute shows every feature.
  Done when: with the demo flag, the home page shows seeded repos and one green CI run.
