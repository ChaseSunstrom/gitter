# Brainstorm — lens: reliability

- **Add a git-aware crash-consistency test harness** — FEATURE — value 5 — effort M — risk low — category: testing
  Why: pack plans git CLI + SQLite as dual stores; the existing ideas mention fault injection but not a harness that asserts DB↔repo invariants after every simulated crash point in push/merge/delete paths.
  Done when: a test kills the server at each named failpoint (post-receive, pre-DB-commit, mid-merge) and, after restart, `gitter doctor` reports zero DB/repo mismatches across all failpoints.

- **Give every background job an idempotency key and dead-letter view** — FEATURE — value 4 — effort M — risk low — category: jobs
  Why: assumption: webhooks, CI triggers, repack, mirror sync and notifications all run as background work; outbox is listed but poison messages and operator visibility are not.
  Done when: a job that fails 5 times lands in an admin "failed jobs" page with error and a retry button, and retrying twice produces one side effect.

- **Bound every queue and apply backpressure with clear 429/503s** — FEATURE — value 4 — effort S — risk low — category: overload
  Why: public site, single binary; git subprocess caps exist as an idea but not a global admission controller covering CI queue, SSE subscribers, and uploads.
  Done when: a load test exceeding configured limits yields fast 503 with Retry-After while health and read pages stay under their latency budget.

- **Split health into liveness, readiness and "degraded" with reasons** — FEATURE — value 4 — effort S — risk low — category: observability
  Why: healthchecks are planned, but a forge needs to say "reads OK, pushes disabled: disk 97%" rather than binary up/down.
  Done when: filling the data volume past threshold flips `/readyz` to degraded with a machine-readable reason while GETs still succeed and pushes return a clear message.

- **Make git subprocess failures self-diagnosing** — FIX — value 4 — effort S — risk low — category: diagnostics
  Why: git CLI is the storage engine; stderr from failed calls is the main source of mystery 500s. assumption: errors are currently generic.
  Done when: any failed git call returns a request ID to the user, and the server log for that ID contains the argv (redacted), exit code, stderr and duration.

- **Run a startup "poison repo" quarantine** — FEATURE — value 3 — effort M — risk med — category: isolation
  Why: one corrupt repo must not take down the instance; reconcile-on-boot exists but not per-repo isolation of failures.
  Done when: a repo with a deliberately corrupted object database shows an error banner and is skipped by maintenance, while all other repos serve normally and the server boots.

- **Version and checksum the runner↔server protocol with a compatibility window** — FEATURE — value 4 — effort S — risk low — category: upgrades
  Why: runner is a subcommand of the same binary but may lag during `docker compose pull`; mixed-version pairs during upgrades are unaddressed.
  Done when: a runner one minor version behind keeps running jobs, and a runner outside the window refuses with a message naming the required version.

- **Detect and survive clock jumps in leases and sessions** — FIX — value 3 — effort S — risk low — category: time
  Why: lease heartbeats and session expiry rely on time; fault-injection for clock jump is mentioned in a list but no monotonic-time design is.
  Done when: tests that step wall clock ±1h do not expire live leases or resurrect expired sessions, because lease logic uses monotonic durations.

- **Wild: Add a "replay the crash" bundle** — FEATURE — value 4 — effort M — risk low — category: debugging
  Why: wild; builds on the black-box recorder idea by packaging request log, SQLite snapshot and affected repo refs into one file that a developer can replay locally with `gitter replay bundle.tgz`.
  Done when: replaying a bundle from an injected panic reproduces the same panic locally.

- **Wild: Canary self-test user that continuously pushes, opens PRs and runs CI on the instance** — FEATURE — value 4 — effort M — risk low — category: synthetic monitoring
  Why: wild; failures in the real path (push→CI→merge) get noticed before users do, and the result feeds the status page.
  Done when: breaking the runner shows a red "CI end-to-end" indicator within two canary cycles without any user activity.

- **Wild: Keep a tiny append-only "intent log" so DB and repos can be rebuilt from each other** — FEATURE — value 4 — effort L — risk med — category: recovery
  Why: wild; with repos as truth for code and SQLite for metadata, a lost DB loses issues/PRs. A signed event log in `/data` lets `gitter rebuild` reconstruct metadata after DB corruption.
  Done when: deleting the SQLite file and running `gitter rebuild` restores users, repos, issues and PR state to match a pre-deletion export.

- **Wild: Make the compose file self-heal with a watchdog sidecar contract** — FEATURE — value 3 — effort S — risk low — category: supervision
  Why: wild; a restart policy only catches crashes, not hangs. Server exposes a deadman heartbeat; the healthcheck kills and restarts it if the event loop stalls.
  Done when: a test that blocks the async runtime for 30s causes the container to be restarted automatically and serve requests afterward.

- **Enforce a per-request timeout and cancellation budget across all handlers** — FIX — value 4 — effort S — risk low — category: latency
  Why: slow git calls or SQLite locks could stall tokio workers; timeouts on subprocesses are listed but not end-to-end request deadlines that propagate cancellation to child processes.
  Done when: a request that exceeds its deadline returns 504 and the underlying git child process is gone within one second.

- **Surface SQLite WAL and busy-lock health to operators** — FEATURE — value 3 — effort S — risk low — category: database
  Why: single-writer SQLite is the core bet; WAL growth from a stuck reader or long checkpoint is a classic silent failure.
  Done when: `gitter doctor` and metrics show WAL size, checkpoint lag and busy-retry counts, and an alert flag appears when WAL exceeds a set size.
