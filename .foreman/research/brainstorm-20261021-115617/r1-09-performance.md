# Brainstorm — lens: performance

- **Keep warm `git cat-file --batch` processes per repo** — PERFORMANCE — value 5 — effort M — risk med — category: git I/O
  Why: the stack plan uses the git CLI as the storage engine, so a fork+exec for every tree, blob or log request will dominate latency. Assumption: spawn cost is the main bottleneck.
  Done when: a bench shows p50 blob/tree read under 5 ms, with no per-request git spawn in strace.

- **Cache immutable content by OID with permanent ETags** — PERFORMANCE — value 5 — effort S — risk low — category: caching
  Why: assumption: the tree, blob, commit and diff at a given SHA never change, so rendered markdown, highlighted blobs and diffs can sit in a byte-bounded LRU. They can also be served as `Cache-Control: immutable`, with 304s for branch URLs after one cheap ref lookup.
  Done when: a second request for a SHA-pinned URL is served with zero git calls and returns 304 on revalidation.

- **Ship a CI-enforced performance budget and a benchmark harness** — FEATURE — value 5 — effort M — risk low — category: benchmarking
  Why: the user's stated goal is "wayyy faster" than GitLab, and nothing in the pack measures it. Bench cold start, idle RAM, TTFB for the repo page, clone time and CI queue latency, and fail the build on regression.
  Done when: `scripts/bench.sh` prints a table, and CI fails if idle RSS exceeds 100 MB or repo-page TTFB exceeds 30 ms on the fixture.

- **Precompress and fingerprint embedded assets** — PERFORMANCE — value 4 — effort S — risk low — category: frontend delivery
  Why: the SPA is embedded via rust-embed, so brotli and gzip can be built in at compile time. Hashed filenames with immutable caching mean zero-copy sends and no per-request compression.
  Done when: the first-load transfer is under 60 KB compressed, and a repeat visit makes zero asset requests.

- **Split SQLite into one writer and a read-connection pool** — PERFORMANCE — value 4 — effort M — risk med — category: database
  Why: WAL is already planned. A single writer task with batched transactions, N read-only connections, a prepared-statement cache and FTS5 for search avoids lock contention under public traffic.
  Done when: a load test of 1k concurrent reads during continuous writes shows no `SQLITE_BUSY` and a p99 under 20 ms.

- **Render first paint on the server, then hydrate** — PERFORMANCE — value 4 — effort L — risk med — category: first paint
  Why: a framework-free SPA means a blank screen until JS runs, which hurts public pages and link previews. Inline the repo tree, README and issue HTML in the response, and let the router take over afterwards.
  Done when: Playwright with JS disabled shows the repo file list and README, and LCP is under 300 ms on localhost.

- **Run background maintenance: commit-graph, multi-pack-index and scheduled repack** — PERFORMANCE — value 4 — effort M — risk low — category: git maintenance
  Why: log, blame and merge-base are slow on large histories without commit-graph files, and pushes pile up loose objects. Run `git maintenance` incrementally with low priority after pushes.
  Done when: after a push of 1k commits, `git log --oneline -n50` and ahead/behind compare finish in under 10 ms.

- **Pre-warm CI workspaces and runners for sub-second job start** — PERFORMANCE — value 5 — effort L — risk med — category: CI latency
  Why: T-0002 specifies docker runners, and container pull plus clone time usually dominates "pipeline started". Keep a pool of warm containers, use shared dependency and build-cache volumes, and give each job a worktree or reflink checkout instead of a fresh clone.
  Done when: the time from push to first log line is under 1 s with a warm image.

- **Stream logs over SSE with a ring buffer and virtualized rendering** — PERFORMANCE — value 3 — effort M — risk low — category: CI logs
  Why: live log streaming is requested, and naive DOM appends choke on 100k-line logs. Chunk to append-only files, resume via `Last-Event-ID`, and render a virtual window in the UI.
  Done when: a 500k-line log scrolls at 60 fps and the tab's memory stays under 150 MB.

- **Virtualize large diffs and compute them lazily** — PERFORMANCE — value 3 — effort M — risk low — category: diff view
  Why: commit and PR diffs are core views. Return a stat summary immediately, load per-file hunks on expand, and run highlighting in a Web Worker.
  Done when: a 5k-file commit shows its file list in under 200 ms and the main thread never blocks for more than 50 ms.

- **Wild: Replace hot-path git CLI reads with in-process gitoxide, falling back to the CLI** — PERFORMANCE — value 4 — effort L — risk high — category: git engine
  Why: wild; assumption: `gix` covers tree walk, blob read and log, and the CLI stays the safe path for push, merge and http-backend. It gives zero-process reads with the same on-disk repos.
  Done when: a differential test over a corpus shows identical output to git for tree, blob and log, with a measured speedup.

- **Wild: Speculative navigation that predicts the next click** — FEATURE — value 3 — effort M — risk low — category: perceived speed
  Why: wild; hover-prefetch is already planned. Extend it with a model of likely clicks (the file you just saw in the finder, the PR after the issue list) and use Speculation Rules plus view transitions, so navigation feels instantaneous.
  Done when: the median click-to-paint is under 50 ms in Playwright on the common flows.

- **Wild: Copy-on-write CI sandboxes with snapshot-restore microVMs** — PERFORMANCE — value 3 — effort L — risk high — category: CI isolation
  Why: wild; assumption: the host supports reflink filesystems or KVM. Snapshot a booted VM or container with deps installed and restore it in milliseconds per job. This is also safer for a public site that runs untrusted code.
  Done when: a hello-world job completes end to end in under 300 ms on a warm snapshot.

- **Wild: Hot-repo packfile cache that serves popular clones from RAM or tmpfs** — PERFORMANCE — value 3 — effort M — risk med — category: clone and fetch
  Why: wild; a public site will see repeated full clones of the same popular repos, and each one repacks. Cache the generated pack keyed by the ref tips, advertise bundle-uri, and stream via sendfile.
  Done when: the second identical clone of a 100 MB repo uses near-zero CPU and finishes in under 25% of the first clone's time.
