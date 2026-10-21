# Brainstorm — lens: reliability

- **Crash-safe git writes via atomic ref transactions** — FIX — value 5 — effort M — risk med — category: data integrity
  Why: git CLI is the storage engine and SQLite holds metadata, so a crash between a push, a PR merge or a web-editor commit and the matching DB update can leave them out of step. Assumption: the planned design has no reconciliation step.
  Done when: killing the server with SIGKILL at each step of push, merge and web-edit leaves repo and DB consistent after restart, shown by a fault-injection test.

- **Ship one-command backup and verified restore** — FEATURE — value 5 — effort M — risk low — category: backup
  Why: a self-hosted public site holds users' only copies of code; the pack mentions no backup path. Assumption: data lives under one volume (SQLite plus bare repos).
  Done when: `gitter backup` makes a consistent snapshot (SQLite online backup API plus `git bundle` or a quiesced copy) while the server is serving, and `gitter restore` into a fresh compose stack passes a clone-and-diff check.

- **Add startup self-check and repair command** — FEATURE — value 4 — effort M — risk low — category: integrity
  Why: DB rows and on-disk repos are two sources of truth. Assumption: drift is inevitable over time.
  Done when: `gitter fsck` reports orphan repos, missing repos, bad refs and DB schema mismatches, and `--repair` fixes the seeded faults in a test.

- **Make CI jobs survive restarts** — FEATURE — value 5 — effort L — risk med — category: CI durability
  Why: T-0002 plans docker runners with live logs. If the server restarts mid-run, jobs would be stuck "running" forever. Assumption: job state is not yet designed.
  Done when: restarting the server mid-pipeline either resumes it or marks it failed with a reason, within a lease timeout, and logs up to that point remain readable.

- **Lease-based runner heartbeats and job requeue** — FEATURE — value 4 — effort M — risk med — category: runners
  Why: runners are separate containers in the same compose file and can die independently. Assumption: pull-based claiming over HTTP.
  Done when: killing a runner container mid-job causes the job to be requeued or failed after the lease expires, and a second runner picks it up in an e2e test.

- **Enforce resource limits on every job container** — SECURITY — value 5 — effort M — risk med — category: abuse safety
  Why: the site is public and runs arbitrary user code in docker. CPU, memory, pids, wall-clock and log-size caps protect the host. Assumption: not yet specified.
  Done when: a fork bomb, a memory hog and an infinite log spewer each get killed at their cap, and the host and server stay responsive.

- **Add per-user and per-IP rate limits and quotas** — SECURITY — value 4 — effort M — risk low — category: abuse safety
  Why: the pack plans push size limits but nothing for request floods, repo count, storage or CI minutes.
  Done when: exceeding a configured limit returns 429 or a clear quota error, and the limits are settable via env vars in the compose file.

- **Serve health, readiness and metrics endpoints with compose healthchecks** — FEATURE — value 4 — effort S — risk low — category: ops
  Why: a single compose file should self-heal. Assumption: none exists yet.
  Done when: `/healthz` checks DB and disk, `/metrics` exposes Prometheus counters, and `restart: unless-stopped` plus a healthcheck brings the container back after a forced hang.

- **Run schema migrations with automatic pre-upgrade snapshot** — FEATURE — value 4 — effort M — risk low — category: upgrades
  Why: users will pull new images over old data. Assumption: SQLite migrations are planned but rollback is not.
  Done when: upgrading from a seeded old-version volume migrates cleanly, and a deliberately failing migration restores the snapshot and refuses to start with a clear message.

- **Cap git subprocess concurrency and time** — PERFORMANCE — value 4 — effort S — risk low — category: stability
  Why: the git CLI is spawned per request, so a burst of clones or diffs could exhaust processes and memory despite the fast Rust core.
  Done when: a load test with 500 concurrent clones keeps latency bounded, extra requests queue or get 503, and no zombie git processes remain.

- **Wild: Run a chaos mode that randomly kills things** — FEATURE — value 3 — effort M — risk low — category: chaos testing — wild
  Why: assumption: reliability claims are worthless unless exercised. A `GITTER_CHAOS=1` compose profile kills the server, runners and git processes at random while an e2e loop pushes, merges and runs CI.
  Done when: a 10-minute chaos run ends with zero lost commits, zero stuck jobs and a clean `gitter fsck`.

- **Wild: Keep a live hot-standby replica via Litestream-style streaming** — FEATURE — value 3 — effort L — risk high — category: replication — wild
  Why: assumption: some self-hosters want near-zero data loss. Stream SQLite WAL and repo ref changes to an S3 or second-host target from an optional compose service.
  Done when: destroying the primary volume and starting the replica yields a site with at most a few seconds of lost writes.

- **Wild: Make repos self-healing via content-addressed shadow mirror** — FEATURE — value 3 — effort L — risk med — category: data integrity — wild
  Why: assumption: disk corruption or an accidental `rm` of a repo should not be fatal. Keep a periodic, deduplicated mirror, and offer one-click "restore this repo" in the admin UI.
  Done when: deleting a bare repo directory on disk, then clicking restore, brings back all refs as of the last mirror pass.

- **Add a status page and an in-app incident banner** — FEATURE — value 2 — effort S — risk low — category: transparency
  Why: assumption: a public site benefits from showing degraded states (CI queue stalled, runner offline) rather than silently hanging.
  Done when: stopping all runners makes the UI show "CI runners offline" on the pipelines page and in the admin dashboard within one heartbeat interval.
