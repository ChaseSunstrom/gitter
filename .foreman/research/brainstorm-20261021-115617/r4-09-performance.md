# Brainstorm — lens: performance

- **Isolate latency with priority lanes** — PERFORMANCE — value 5 — effort M — risk low — category: tail latency
  Why: assumption: one tokio runtime and one git-subprocess pool let a big clone, CI log flood or repack delay page loads. The pack has limits and RSS governors but no per-class isolation.
  Done when: while 20 parallel full clones and a CI log flood run, p99 for `/{owner}/{repo}` stays within 1.5x its idle value in `gitter bench`.

- **Collapse duplicate expensive requests into one** — PERFORMANCE — value 4 — effort S — risk low — category: herd protection
  Why: assumption: after a push or a link going viral, many identical requests (tree, diff, archive, pack negotiation) each spawn their own git process. Caches don't help until the first one finishes.
  Done when: 100 concurrent identical cold requests spawn exactly one git subprocess, verified by a counter, and all 100 get the same bytes.

- **Key every cache on a per-repo generation counter** — PERFORMANCE — value 4 — effort M — risk med — category: cache correctness
  Why: OID caches cover immutable content only. Ref lists, branch pages, PR mergeability and counts are mutable and need a rule. Bumping `(repo, gen)` atomically with the ref update replaces TTLs and stale-data bugs.
  Done when: a push makes the new commit visible on the next request with no TTL wait, and an unchanged repo serves from cache with zero git calls.

- **Make per-file "last commit" instant with changed-path Bloom filters** — PERFORMANCE — value 5 — effort M — risk low — category: repo browsing
  Why: assumption: the file-tree view shows the last commit for each entry, the usual slow spot on big histories. Git's changed-path Bloom filters in the commit-graph make this cheap, and the pack already plans commit-graph maintenance.
  Done when: a tree view of a 100k-commit repo renders with last-commit info in under 150 ms on a warm box, with filters enabled by the maintenance job.

- **Highlight on the server with tree-sitter, cached by blob OID and language** — PERFORMANCE — value 4 — effort M — risk med — category: payload and CPU
  Why: the plan vendors highlight.js client-side, which costs bytes and main-thread time on large files. Server tokens keyed by `(oid, lang)` are immutable and cacheable forever, so each file is highlighted once.
  Done when: the JS bundle shrinks by the highlighter's size, and a 5k-line file shows highlighted text in the first paint with no client highlight work.

- **Add local-only real-user monitoring** — FEATURE — value 4 — effort S — risk low — category: perf and privacy
  Why: the pack enforces a performance budget in tests but has no real-world measurement, and the privacy lens is thin. Collecting LCP, INP and navigation timings into SQLite with no third party and no IPs shows the budget holding on real devices.
  Done when: the admin page shows p50/p95 LCP per route from real visits, and no outbound request or identifier is stored.

- **Pull-through registry cache for CI base images** — PERFORMANCE — value 4 — effort M — risk med — category: CI start time
  Why: image pulls usually dominate job start time, and the pack's CI caches cover workspaces and packages but not images. Mirroring pulled layers locally keeps them off the network and makes repeat jobs near-instant.
  Done when: the second job using `node:22` starts with zero bytes pulled from the internet, shown in the job's phase trace.

- **Store CI logs as seekable zstd chunks with a tail index** — PERFORMANCE — value 3 — effort M — risk low — category: logs
  Why: append-only logs are planned, and compression with random access isn't. With chunk-level compression, opening a 200 MB log shows the tail and folds instantly, and storage shrinks roughly 10x.
  Done when: opening the last 100 lines of a 200 MB log takes under 100 ms, and the stored file is under 25% of the raw size.

- **Dedupe artifacts and caches with content-defined chunking** — PERFORMANCE — value 3 — effort L — risk med — category: storage and transfer
  Why: assumption: successive cache and artifact uploads (node_modules, target/) differ by small amounts, so whole-blob storage wastes disk and upload time. FastCDC-style chunks upload only what changed.
  Done when: re-uploading a 1 GB cache with 1% changed transfers under 50 MB and uses under 60 MB extra disk.

- **Guarantee boot in under 300 ms with 100k repos** — PERFORMANCE — value 3 — effort M — risk low — category: startup
  Why: assumption: the self-check and reconcile steps, plus "sleep to zero" and zero-downtime upgrades, all depend on fast start. A DB-driven repo index with lazy per-repo checks avoids directory scans.
  Done when: the server answers `/readyz` within 300 ms against a seeded 100k-repo data dir, and a CI test enforces it.

- **Wild: Auto-tune the instance to its hardware at idle** — FEATURE — value 3 — effort M — risk med — category: self-tuning
  Why: wild. The binary already bundles a load generator, so when idle it can try git pack threads and window, cache sizes, SQLite page cache and runner concurrency against the real box. It then proposes the best settings as a one-click diff and never applies them silently.
  Done when: running `gitter tune` on a 2-core and a 16-core box yields different recommended values, and applying them improves `gitter bench` throughput.

- **Wild: Patch live pages instead of reloading them** — FEATURE — value 3 — effort L — risk med — category: live UI
  Why: wild. One persistent connection carries JSON patches keyed to the page's data (PR checks, issue comments, CI status), so pages update in place with tiny payloads and no refetch. It builds on the planned single SSE/WebSocket hub.
  Done when: a new comment on an open issue arrives in under 100 ms as a payload under 1 KB, with no full-page request made.

- **Wild: Cut cold start with a persisted "warm set" snapshot** — PERFORMANCE — value 2 — effort M — risk low — category: warm cache
  Why: wild. On shutdown, save which repos, pack indexes and rendered pages were hot, then replay them as pre-touches on boot. After an upgrade or restart, the first visitors see warm latency instead of paying for the cold cache.
  Done when: after a restart, the first request to each of the top 50 repos is within 20% of the warm p50 latency.
