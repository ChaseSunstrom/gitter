# Brainstorm — lens: reliability

- **Verify every backup by restoring it in a throwaway container** — FEATURE — value 5 — effort M — risk low — category: backup
  Why: pack lists backup/restore and encrypted verification, but not a proven restore; an untested backup is the classic silent failure. Assumption: backups run on a schedule.
  Done when: a scheduled job restores the latest snapshot into a temp dir, runs `git fsck` on every repo plus SQLite `integrity_check`, and the result appears on the status page; a deliberately corrupted snapshot turns it red.

- **Reconcile DB and bare repos on boot and nightly** — FIX — value 5 — effort M — risk med — category: consistency
  Why: design pairs SQLite rows with git directories on disk, so a crash between "insert repo row" and "git init" (or delete) leaves orphans or ghosts. Pack has self-check/repair but not two-store reconciliation. Assumption: no cross-store transaction exists.
  Done when: a test that kills the process between each step of create, delete, rename and fork converges to a consistent state after restart, and `gitter doctor` reports zero drift.

- **Make pushes two-phase: quarantine, verify, then commit refs and DB together** — FIX — value 5 — effort M — risk med — category: git writes
  Why: a push that lands objects and refs but dies before the DB records the event (CI trigger, webhook, PR head update) loses downstream work. Assumption: post-receive work is currently best effort.
  Done when: killing the server at any point of a push yields either no ref change, or a ref change plus a durable event row that is processed after restart exactly once.

- **Use a transactional outbox for webhooks, CI triggers and notifications** — FEATURE — value 4 — effort M — risk low — category: events
  Why: pack covers job requeue but not event delivery; webhook and CI triggers are lost on crash otherwise.
  Done when: events are written in the same SQLite transaction as the state change, delivered with retry and backoff, and a kill -9 test shows no loss and no duplicate side effects (idempotency keys).

- **Add disk-full and quota guardrails with graceful read-only degradation** — FIX — value 4 — effort S — risk low — category: resilience
  Why: public site with pushes, CI logs and artifacts will fill the single volume; SQLite WAL corrupts badly or wedges when out of space. Assumption: no disk watermark exists.
  Done when: below a configured free-space threshold, pushes and CI start return a clear error and the banner shows, reads still work, and everything resumes automatically once space is freed.

- **Run a fault-injection test suite in CI (kill, ENOSPC, slow disk, clock jump)** — CLEAN — value 4 — effort M — risk low — category: testing
  Why: pack's chaos mode is a wild runtime idea; a deterministic version in the test suite actually prevents regressions.
  Done when: `cargo test --features faults` injects failures at named points in push, merge, migration and job-lease code, and all invariants hold.

- **Fence CI jobs with lease epochs so zombie runners can't overwrite results** — FIX — value 4 — effort S — risk low — category: CI correctness
  Why: lease-based requeue is listed, but a requeued job whose original runner wakes up can still post logs or status for the same job.
  Done when: each job attempt has an epoch, and log/status/artifact writes with a stale epoch get rejected, shown by a test with two runners racing.

- **Make merges idempotent and conflict-safe with compare-and-swap on refs** — FIX — value 4 — effort S — risk med — category: git writes
  Why: merge-tree plus ref update is racy when two merges or a push hit the same branch. Assumption: a plain update-ref is used.
  Done when: concurrent merge and push to one branch results in exactly one winner, and the loser gets a clear "base moved, retry" result with no lost commits.

- **Ship a reversible-migration rehearsal that runs against a copy of your real data** — FEATURE — value 4 — effort M — risk low — category: upgrades
  Why: pack has auto-rollback and pre-upgrade snapshots; rehearsal on the actual data before cutover catches failures that tests miss.
  Done when: `gitter upgrade --dry-run` applies pending migrations to a copy, validates it, reports the timing, and leaves the live instance untouched.

- **Wild: Keep a shadow read-only instance that replays the write log and compares answers** — FEATURE — value 3 — effort L — risk med — category: verification
  Why: wild; a second process following the Litestream-style stream serves sampled reads, and any divergence in tree, diff or log output flags a bug or corruption early.
  Done when: a seeded divergence (flipped byte in a replica repo) is detected and reported within one comparison cycle.

- **Wild: Add a "black box recorder" that dumps the last 60 seconds of state on crash or panic** — FEATURE — value 3 — effort M — risk low — category: diagnostics
  Why: wild; a ring buffer of requests, git subprocess calls, and queue state written out on panic gives a ready-made bug report. Assumption: self-hosters rarely have proper logging set up.
  Done when: a forced panic produces a single redacted file under the data volume that `gitter doctor --bundle` includes.

- **Wild: Let the instance continuously fsck itself with a slow-drip scrubber that heals from a peer** — FEATURE — value 4 — effort L — risk med — category: integrity
  Why: wild; bit rot in packfiles goes unseen until a clone fails. A low-priority scrubber verifies objects and, if a mirror or backup holds the good copy, repairs it.
  Done when: corrupting one object in a repo is detected by the scrubber, and the object is restored from the backup or mirror, with an event logged.

- **Add graceful shutdown with drain: finish pushes, stop accepting, checkpoint WAL** — FIX — value 3 — effort S — risk low — category: lifecycle
  Why: `docker compose down` or upgrade sends SIGTERM; in-flight pushes and log streams otherwise truncate. Assumption: compose default 10s stop grace is not tuned.
  Done when: SIGTERM during a large push lets it finish (within the configured grace period set in compose), new connections are refused with a retry hint, and the WAL is checkpointed on exit.

- **Make the runner resilient to server outages by buffering logs locally and resuming** — FEATURE — value 3 — effort M — risk low — category: CI correctness
  Why: pull-based runners with SSE logs lose output if the server restarts mid-job; jobs survive restarts is listed but log continuity is not.
  Done when: restarting the server during a running job leaves the job running, and the final log is complete and in order with no gaps or duplicates.
