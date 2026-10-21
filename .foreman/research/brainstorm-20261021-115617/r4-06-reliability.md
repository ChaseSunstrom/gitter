# Brainstorm — lens: reliability

- **Pin every repo to a generation counter and detect torn state** — FIX — value 4 — effort M — risk med — category: consistency
  Why: assumption: pushes touch bare repo refs and SQLite separately; earlier ideas cover two-phase pushes and reconcile, but nothing lets a reader know a repo view is mid-update. A per-repo generation (bumped after refs and DB agree) lets caches, web pages and CI triggers reject or retry stale reads.
  Done when: a test that kills the server between ref update and DB commit shows the next request either serves the old consistent generation or triggers reconcile, never a mixed page, and the generation is visible in `gitter doctor`.

- **Make CI trigger idempotent per (repo, ref, sha, workflow)** — FIX — value 4 — effort S — risk low — category: CI correctness
  Why: assumption: with outbox, retries, restarts and webhooks, duplicate push events are likely; earlier ideas give idempotency keys for background jobs but not a dedupe rule for pipeline creation itself.
  Done when: replaying the same push event 10 times, including across a restart, yields exactly one pipeline run.

- **Run an automatic runner-capacity admission check before accepting a job** — FEATURE — value 3 — effort M — risk low — category: scheduling
  Why: the pack proposes adaptive scheduling and RSS governors, but a runner that accepts a job it cannot fit gets OOM-killed and flaky results. A pre-flight check (free RAM, disk, image present) makes "failed" mean the code failed, not the host.
  Done when: on a runner with artificially low memory, a heavy job stays queued with a visible reason instead of starting and dying, and failures are labelled "infra" vs "code" in the UI.

- **Classify every failed run as infra vs code failure and auto-retry infra ones** — FEATURE — value 4 — effort M — risk low — category: CI trust
  Why: assumption: users losing trust in CI is the main reliability risk; the pack has rerun buttons and flaky detection but no systematic separation of runner death, image pull failure or lease loss from real test failures.
  Done when: killing a runner mid-job produces a run marked "infra: runner lost" that is requeued once automatically, and does not count toward flaky-test stats.

- **Ship a restore-drill that runs on a schedule and reports RPO/RTO** — FEATURE — value 4 — effort M — risk low — category: backup
  Why: pack has verified backups and point-in-time restore separately; combining them into a scheduled drill that measures actual recovery time and data-loss window gives operators a real number rather than a hope.
  Done when: admin page shows "last drill: passed, restored in 42s, data loss window 3s" and a failed drill raises the incident banner.

- **Add per-repo write-ahead journal for web-editor and merge commits** — FIX — value 3 — effort M — risk med — category: crash safety
  Why: web-editor commits and PR merges use multiple git plumbing steps (hash-object, mktree, commit-tree, update-ref). A crash midway leaves orphaned objects or an unannounced commit. Journaling the intended ref update lets boot finish or discard it.
  Done when: fault-injection killing the process after commit-tree but before update-ref leaves the branch untouched and the journal entry is resolved on next boot with a log line.

- **Degrade by feature, not by instance** — FEATURE — value 4 — effort M — risk med — category: graceful degradation
  Why: pack has "degraded" health and read-only on disk-full, but not independent feature breakers. If CI, search index or webhooks fail, browsing and git push/pull should keep working and the UI should say which part is down.
  Done when: disabling the CI subsystem via fault injection leaves clone, push, browse and issues passing the e2e suite, with a banner naming CI as degraded.

- **Bound and test the upgrade matrix (N-1 runner, N server)** — FIX — value 3 — effort S — risk low — category: upgrades
  Why: pack versions the protocol with a compatibility window; what's missing is a CI test that actually runs the previous release's runner against the new server, since the single compose file upgrades both only if pull succeeds on both.
  Done when: a release gate starts last release's runner image against the candidate server and completes a job.

- **Wild: Self-verifying release canary that rolls back on its own** — FEATURE — value 4 — effort L — risk high — category: upgrades
  Why: wild; assumption: combines pre-upgrade snapshot, canary self-test user and migration rollback already listed. After a self-upgrade, the canary runs push, PR, CI; if any step fails within 5 minutes, the instance restores the snapshot and previous image and records why.
  Done when: upgrading to a deliberately broken build ends with the old version serving traffic and a rollback report, with no manual step.

- **Wild: Deterministic "time machine" test mode that replays recorded production traffic against a candidate build** — FEATURE — value 3 — effort L — risk med — category: testing
  Why: wild; builds on the shadow instance and replay-the-crash ideas, but oriented to pre-release: record sanitized request logs and git protocol sessions, replay against the new binary, diff responses.
  Done when: replaying a captured hour of traffic against a build with an injected regression reports the differing endpoints.

- **Wild: Runner "lifeboat" mode that finishes in-flight jobs offline and syncs results later** — FEATURE — value 3 — effort M — risk med — category: runner resilience
  Why: wild; extends log buffering by letting a runner complete the job, retain artifacts locally and upload when the server returns, with lease-epoch fencing deciding whether the result is still accepted.
  Done when: stopping the server for 2 minutes mid-job still ends with the job's logs and artifacts attached to the run, or a clear "superseded" marker if it was requeued.

- **Wild: Instance "heartbeat contract" file others can poll to know it's trustworthy** — FEATURE — value 2 — effort S — risk low — category: observability
  Why: wild; assumption: federated or mirrored users want to know a source is healthy. A signed, tiny `/.well-known/gitter-health` with generation, last backup age, last drill result and fsck status.
  Done when: curl returns a signed JSON document whose backup age exceeds a threshold flips the status to "stale".

- **Enforce a clock-independent monotonic ordering for events and logs** — FIX — value 3 — effort S — risk low — category: consistency
  Why: pack covers detecting clock jumps for leases; log lines, activity feeds and status checks also sort by time. A monotonic sequence per run and per repo keeps ordering correct after a jump.
  Done when: a test that steps the system clock backwards mid-run still shows CI log lines and activity entries in true order.

- **Make the e2e suite a permanent post-deploy smoke test** — CLEAN — value 3 — effort S — risk low — category: verification
  Why: scripts/e2e.sh already exists with curl, git and Playwright; reusing a safe subset as `gitter doctor --smoke` against a live instance, using a throwaway repo, proves the real path rather than just process liveness.
  Done when: `gitter doctor --smoke` creates, clones, pushes, opens a PR, runs a trivial CI job and cleans up, exiting non-zero on any failed step.
