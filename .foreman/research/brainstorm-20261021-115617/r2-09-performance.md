# Brainstorm — lens: performance

- **Precompute a per-repo "browse index" on push** — FEATURE — value 5 — effort M — risk med — category: read path
  Why: assumption: tree/blob/log pages shelling out to git per request is the main latency floor. A post-receive step writes last-commit-per-path, file list, language stats and README OID into SQLite, so repo pages become pure DB reads.
  Done when: the repo root page for a 10k-file repo returns in under 10 ms server time with zero git subprocesses (asserted by a test counter).

- **Serve clones from a negotiated-pack cache** — PERFORMANCE — value 4 — effort M — risk med — category: git transport
  Why: assumption: CI runners fetch the same refs repeatedly, and `git http-backend` repacks every time. Cache the full-clone pack keyed by the ref-tip set, and use shallow or `--filter` packs for runners.
  Done when: a second identical clone does not run `pack-objects` (log counter), and is at least 5x faster than the first.

- **Build a dependency-aware incremental diff and blame cache** — PERFORMANCE — value 4 — effort M — risk low — category: caching
  Why: blame and large diffs are the classic slow paths named in the plan (blame, compare). Key cached results by (blob OID, parent OID) so they are immutable and shared across forks.
  Done when: blame of a 5k-line file with 2k commits renders on the second request in under 50 ms, and a fork hits the same cache entry.

- **Add a CI job-start latency budget with per-phase tracing** — FEATURE — value 4 — effort S — risk low — category: CI speed
  Why: "push-to-green under 3 seconds" is listed, but nothing measures where time goes. Emit spans for push, hook, enqueue, lease, workspace checkout, container start and first log byte, and fail the e2e script when the budget regresses.
  Done when: `scripts/e2e.sh` prints a phase breakdown and fails if the push-to-first-log time exceeds the configured budget.

- **Checkout CI workspaces via git worktree plus reflink/overlay** — PERFORMANCE — value 4 — effort M — risk med — category: CI speed
  Why: cloning per job dominates start time for large repos. Keep a bare mirror on the runner host and create workspaces as reflink copies or overlayfs layers over a cached checkout.
  Done when: a job on a 500 MB repo has its workspace ready in under 300 ms on the second run.

- **Coalesce and debounce live updates through one fan-out hub** — PERFORMANCE — value 3 — effort S — risk low — category: realtime
  Why: SSE for logs, status and notifications multiplied by many viewers can thrash the server. A single broadcast hub with batching (e.g. 50 ms frames), shared per-run buffers and backpressure drops keeps CPU flat as watchers grow.
  Done when: a load test with 2,000 SSE clients watching one run keeps the server under a stated CPU/RAM ceiling and each client gets identical log bytes.

- **Run the SQLite write path through a group-commit queue** — PERFORMANCE — value 3 — effort M — risk med — category: database
  Why: the plan has WAL with one writer. Batching concurrent small writes (stars, notifications, log offsets, session touches) into one transaction avoids fsync contention under public traffic.
  Done when: a benchmark of 1,000 concurrent writes shows at least 5x the throughput of one-transaction-per-write, with durability tests passing.

- **Ship a built-in profiler endpoint and slow-request log** — FEATURE — value 3 — effort S — risk low — category: observability
  Why: assumption: speed claims need diagnosis, not only a benchmark. An admin-only page lists the slowest routes, git subprocess time and DB query time per request via Server-Timing headers.
  Done when: every response carries a `Server-Timing` header, and the admin page lists the top 20 slow requests with git and DB breakdowns.

- **Paginate and stream huge listings with keyset cursors** — PERFORMANCE — value 3 — effort S — risk low — category: scalability
  Why: commit logs, issue lists, explore and search degrade with OFFSET. Use keyset pagination everywhere, with streamed JSON for the first rows.
  Done when: page 10,000 of a commit list costs the same as page 1 (within 20%) in a benchmark.

- **Wild: Predictive prefetch trained on the user's own click paths** — FEATURE — value 3 — effort M — risk low — category: navigation
  Why: wild. Hover-prefetch is planned; a tiny local model of transitions (repo → files → blob, PR → files tab) prefetches the likely next page before the pointer moves, within a bandwidth cap.
  Done when: in a Playwright run of a scripted browsing session, at least 60% of navigations are served from the prefetch cache with zero network wait.

- **Wild: Compile the hot read-only pages into static snapshots served by sendfile** — PERFORMANCE — value 3 — effort L — risk med — category: read path
  Why: wild. For popular public repos, pre-render the HTML shell and JSON for the root, README and recent commits into precompressed files on tmpfs, invalidated by push, and serve them with zero-copy.
  Done when: a hot repo page sustains over 50k requests/sec on the reference box with p99 under 5 ms.

- **Wild: Use io_uring and a custom pack-object reader for the hottest endpoints** — PERFORMANCE — value 2 — effort L — risk high — category: git engine
  Why: wild. Beyond the gitoxide idea, bypass process spawns entirely for blob/tree reads with a memory-mapped pack index reader, falling back to the CLI on any mismatch.
  Done when: blob reads from a 1 GB repo run at least 10x faster than `git cat-file` per call, verified against the CLI output on a corpus.

- **Wild: Speed-regression bisect bot on every PR to Gitter itself** — FEATURE — value 3 — effort M — risk low — category: dev workflow
  Why: wild. Gitter dogfoods its own CI: each PR runs the benchmark harness against the base commit and posts a status check with the delta, so the speed claim cannot silently rot.
  Done when: a PR that adds an artificial 20 ms delay gets a failing "perf" status check showing the regression.
