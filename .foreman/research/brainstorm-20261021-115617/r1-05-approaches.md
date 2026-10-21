# Brainstorm — lens: approaches

- **Ship one binary that is both server and runner** — FEATURE — value 5 — effort M — risk low — category: deployment
  Why: the user wants "a single simple docker compose file" with runners. The same `gitter` binary running as `gitter serve` or `gitter runner` means one image and a two-service compose file.
  Done when: `docker compose up` starts both services from a single image, and a pushed commit with a pipeline file runs and reports status with no further setup.

- **Auto-register the bundled runner with a bootstrap token** — FEATURE — value 5 — effort S — risk low — category: onboarding
  Why: GitLab runner registration is a known pain. Assumption: the server generates a one-time token on first boot, shared with the runner via a compose volume or env var.
  Done when: a fresh `docker compose up` shows one online runner in the admin UI without the user copying any token.

- **Use a pull-based runner protocol over long-poll or WebSocket** — FEATURE — value 5 — effort M — risk med — category: CI
  Why: runners that dial out work behind NAT, so remote machines can join without opening ports. Log streaming (T-0002) needs the same channel.
  Done when: a runner on a separate host with only outbound HTTPS claims a job and streams logs that appear live in the browser.

- **Define pipelines in a small YAML subset, not a GitLab/Actions clone** — FEATURE — value 4 — effort M — risk med — category: CI
  Why: the user wants it simpler than GitLab. Assumption: a minimal schema (steps, image, needs, matrix, secrets) covers about 90% of use. Full GitHub Actions compatibility is a much bigger job.
  Done when: a `.gitter/ci.yml` with two parallel jobs and a dependent third runs in the correct order, and invalid YAML shows a line-numbered error on the commit page.

- **Run jobs through the Docker socket first, with a rootless/Podman option later** — SECURITY — value 4 — effort M — risk high — category: CI isolation
  Why: this is a public multi-user site, so untrusted pipeline code is the biggest risk. Mounting the Docker socket is simplest but gives host access. Per-job containers with no privileged mode, CPU/memory/time limits, and network off by default contain it.
  Done when: a test job that tries to read host files or exceed its memory limit is blocked or killed, and the result shows in the logs.

- **Make CI opt-in per repo and quota-limited on public instances** — SECURITY — value 4 — effort S — risk low — category: abuse control
  Why: public hosting plus free compute invites crypto-miners. Assumption: owner-level approval or per-user minutes caps are enough.
  Done when: a new user's repo can't run pipelines until enabled, and exceeding the minutes quota blocks new jobs with a clear message.

- **Store logs and artifacts as append-only files, with the SQLite row holding only an offset** — PERFORMANCE — value 4 — effort S — risk low — category: storage
  Why: SQLite is the chosen simplicity bet. Many small log writes to it would hurt the "wayyy faster" goal.
  Done when: a job emitting 100k lines does not slow unrelated page loads, and the log page tails live with Server-Sent Events.

- **Offer Postgres as an optional swap behind the same storage trait** — CLEAN — value 2 — effort L — risk med — category: scaling
  Why: this is a trade-off, not a recommendation. SQLite WAL is likely enough for most self-hosters. The abstraction stays cheap only if it is introduced early.
  Done when: the test suite passes on both backends, and the default compose file still uses SQLite.

- **Add a one-command backup and restore that snapshots SQLite and repos together** — FEATURE — value 4 — effort S — risk low — category: reliability
  Why: assumption: self-hosters fear data loss in a single-volume setup. `gitter backup` would use SQLite `VACUUM INTO` plus a repo tarball.
  Done when: backing up, wiping the volume, and restoring gives a working instance with identical repos and issues.

- **Wild: Make each push build and preview in an ephemeral sandbox, with a URL** — FEATURE — value 3 — effort L — risk high — category: previews
  Why: wild. Pages-style previews for every branch would be a differentiator, reusing the CI runner. wild.
  Done when: pushing a branch with a static site build step gives a clickable preview URL that serves the output.

- **Wild: Let runners be browser tabs using WASM-sandboxed jobs** — FEATURE — value 2 — effort L — risk high — category: runners
  Why: wild. Contributors donate idle compute through a safe sandbox with no Docker. It suits small jobs like lint and tests built for WASI.
  Done when: opening a runner page claims a WASI job, runs it, and posts logs and status back.

- **Wild: Use git itself as the CI queue, with pipelines as refs** — CLEAN — value 3 — effort M — risk med — category: architecture
  Why: wild. Storing job state and logs under `refs/ci/*` makes CI results clonable and mirrorable, and removes a table. The cost is slower queries.
  Done when: `git fetch origin 'refs/ci/*'` returns the run record and log for a commit.

- **Reuse Actions-compatible workflow files by running `act` inside the runner** — FEATURE — value 3 — effort S — risk med — category: reuse
  Why: this does less. Existing Actions workflows would run with no new syntax. It brings compatibility gaps and a heavy image.
  Done when: a stock `.github/workflows` file with checkout and a shell step runs on the runner and reports status.

- **Ship a single-container "all-in-one" mode that runs CI jobs as subprocesses** — FEATURE — value 3 — effort S — risk high — category: simplicity
  Why: assumption: trusted private instances want zero extra moving parts. It is unsafe for public use, so it must be off by default and clearly flagged.
  Done when: setting `CI_MODE=local` runs a job in the server container, and the admin UI shows a warning banner.
