# Brainstorm — lens: performance

- **Add a per-endpoint latency SLO gate with flamegraph artifact** — FEATURE — value 4 — effort M — risk low — category: perf-testing
  Why: pack lists a "CI-enforced performance budget" but not per-endpoint p99 targets or automatic profiles on regression. Assumption: Gitter's own CI runs on itself (dogfood idea).
  Done when: a PR that slows `GET /:owner/:repo/tree` p99 past its budget fails the check and attaches a flamegraph.

- **Serve smart-HTTP fetches from a have/want negotiation cache** — PERFORMANCE — value 4 — effort M — risk med — category: git-transport
  Why: pack mentions a negotiated-pack cache only as an idea title; CI runners clone the same refs repeatedly, so identical want/have sets dominate.
  Done when: 100 runner clones of the same commit run `git pack-objects` once, verified by a counter on `/metrics`.

- **Make runner checkouts shallow-by-default with server-side partial-clone filters** — PERFORMANCE — value 4 — effort S — risk low — category: ci-startup
  Why: job start latency is a stated goal (push-to-green under 3s); full clones in every job would blow it. Assumption: git CLI supports `filter=blob:none` via http-backend config.
  Done when: a 1 GB-history repo's CI checkout completes in under 1s and fetches blobs on demand.

- **Bound memory with a hard RSS governor that sheds load by priority** — PERFORMANCE — value 4 — effort M — risk med — category: low-RAM
  Why: pack promises a published low-RAM number, but git subprocess fan-out and log buffers can spike. Assumption: single 1–2 GB VPS targets.
  Done when: under a synthetic 200-concurrent-clone load, RSS stays under the configured cap and the server returns 503 with Retry-After to low-priority requests (search, archives) first.

- **Generate zip/tar archives incrementally and cache by tree OID** — PERFORMANCE — value 3 — effort S — risk low — category: downloads
  Why: archive downloads are a classic hot-path DoS on public forges and are absent from the ideas so far.
  Done when: the second download of the same tree is served from cache with zero git invocations, and the first streams bytes before generation finishes.

- **Index file names and symbol outlines on push for the fuzzy finder** — FEATURE — value 3 — effort M — risk low — category: instant-ui
  Why: the fuzzy file finder is planned; "browse index" is listed but not the finder's data path. Large repos need a prebuilt in-memory trie rather than `ls-tree` per keystroke.
  Done when: file finder returns results in under 20ms on a 100k-file repo with no git call per keystroke.

- **Ship a deterministic load generator `gitter bench` bundled in the binary** — FEATURE — value 3 — effort S — risk low — category: benchmarking
  Why: the head-to-head benchmark is planned, but users need to reproduce on their own hardware. Assumption: single binary already carries the CLI subcommands.
  Done when: `gitter bench --target URL` outputs p50/p99 for browse, clone, push, and CI start, as JSON.

- **Adaptive CI scheduling that packs jobs by measured CPU/memory history** — PERFORMANCE — value 3 — effort M — risk med — category: runners
  Why: runner leases are planned, but nothing about using past job resource profiles to avoid overcommit on small hosts.
  Done when: on a 2-core host, four historically heavy jobs queue rather than run concurrently, and total wall time beats naive parallelism in a test.

- **Wild: Keep the entire hot working set mmap'd in a single shared pack-index sidecar** — PERFORMANCE — value 3 — effort L — risk high — category: storage
  Why: wild — a custom index of all repos' recent trees/blobs lets browse requests skip process spawn and even gitoxide parsing; assumption: git object stores remain standard on disk.
  Done when: tree and blob views for the top 1% of repos serve with zero syscalls beyond one mmap read, shown by strace.

- **Wild: Speculative CI — start jobs on push before the pipeline finishes parsing, from the last known graph** — PERFORMANCE — value 3 — effort L — risk med — category: ci-startup
  Why: wild — the first job of a repo's pipeline is almost always the same; pre-warming and pre-checking-out it hides the latency. Assumption: the previous run's graph is a good predictor.
  Done when: median time-to-first-log-line after push is under 500ms, and mispredicted jobs are discarded without side effects.

- **Wild: Browser-side diff and syntax highlight in a WASM worker with shared cache** — PERFORMANCE — value 2 — effort M — risk med — category: frontend
  Why: wild — moving highlight/diff off the server and main thread keeps the server cheap on public sites. Assumption: no-framework SPA can host a worker.
  Done when: opening a 5k-line diff never blocks the main thread over 16ms and server CPU per diff view is zero.

- **Cap and coalesce CI log writes with a per-job ring and batched fsync** — PERFORMANCE — value 3 — effort S — risk low — category: logs
  Why: logs are stored as append-only files with SSE streaming; chatty jobs would otherwise cause one write and one broadcast per line.
  Done when: a job emitting 100k lines/sec causes under 20 fsyncs/sec and the UI still shows output within 100ms.

- **Add a startup warm-up that pre-touches hot repos and assets** — PERFORMANCE — value 2 — effort S — risk low — category: cold-start
  Why: zero-downtime upgrades are planned, but the first requests after restart hit cold page cache and cold SQLite pages.
  Done when: after restart, the first request to the top-20 repos is within 1.5× of steady-state latency.
