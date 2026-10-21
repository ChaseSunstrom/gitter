# Brainstorm — lens: approaches

- **Run CI in bubblewrap/Landlock sandboxes, with no Docker daemon** — FEATURE — value 5 — effort L — risk med — category: CI sandbox
  Why: the pack's ideas depend on the Docker socket, with rootless and microVM options only "later". A pluggable executor trait (bwrap, then Docker, then Firecracker) removes the biggest simplicity and security cost from the compose file. Trade-off: you must unpack OCI images yourself and the isolation is weaker than a microVM. Assumption: the host kernel allows unprivileged user namespaces.
  Done when: a job runs from `docker-compose.yml` with no `/var/run/docker.sock` mount and cannot read host paths outside its workspace.

- **Skip jobs by input hash with a content-addressed task cache** — FEATURE — value 5 — effort M — risk med — category: CI speed
  Why: the pack covers warm runners and a cache store, but not skipping work. Hash the job's command, image, env and declared input tree OIDs; if the hash was seen green, replay the logs and status in milliseconds. Git already hashes trees, so this fits the "way faster" goal. Trade-off: users must declare inputs, and wrong declarations cause stale greens.
  Done when: a second push with an unchanged declared input tree shows the job as "cached" in under 1s and runs no container.

- **Expose a Gitea/GitHub-compatible REST subset** — FEATURE — value 4 — effort M — risk low — category: ecosystem reuse
  Why: reusing the existing ecosystem is a distinct approach. `gh`, `tea`, IDE plugins and Renovate-style bots work on day one, and the pack already lists a `gitter` CLI and OpenAPI. Assumption: a subset covering repos, issues, PRs and statuses is enough. Trade-off: you inherit their quirks.
  Done when: `gh`/`tea` pointed at the instance can list repos, open an issue and read a commit status.

- **Dogfood: Gitter hosts and builds itself from milestone one** — CLEAN — value 4 — effort S — risk low — category: verification
  Why: the pack's verification pillar is the real path. Mirroring this repo into a running instance, with its own CI running `cargo test` and `scripts/e2e.sh`, finds forge bugs earlier than synthetic tests do. Assumption: the instance can run on the dev box.
  Done when: a push to the dogfood repo triggers a green pipeline running the project's own test suite, shown on its commit page.

- **Use Compose profiles so core is one service and CI is opt-in** — FEATURE — value 4 — effort S — risk low — category: simplicity
  Why: this is the "do less" approach. `docker compose up` starts only the server, and `--profile ci` adds the runner service. The default footprint is smaller and public instances don't get CI exposure by accident. It also supports the pack's "CI opt-in per repo" idea. Trade-off: two documented modes.
  Done when: the default `up` starts exactly one container, and `--profile ci` adds a registered runner without editing the file.

- **Pre-render public pages into an OID-keyed static cache** — PERFORMANCE — value 4 — effort M — risk med — category: speed
  Why: the pack lists SSR, ETags and packfile caches separately. The unifying approach is that anything addressed by commit OID is immutable, so render it once to compressed files and serve with sendfile. Trade-off: disk use and the need to invalidate on ref moves.
  Done when: a repeat request for a blob or tree page at a fixed commit is served from the cache with no git subprocess and a p99 under 5ms in the benchmark.

- **Store instance settings as a git repo (GitOps config)** — FEATURE — value 3 — effort M — risk low — category: operations
  Why: it resolves the tension between "settings in the UI, not env files" and reproducibility. Settings live in a hidden `_gitter/config` repo, so UI edits are commits with history, diff and rollback, and backup covers them for free. Trade-off: a schema must be kept stable.
  Done when: changing a setting in the UI creates a commit in the config repo, and reverting that commit restores the old behaviour.

- **Run read-only pull-through replicas for scaling** — FEATURE — value 3 — effort L — risk high — category: scale
  Why: this is the approach for public traffic without a bigger box. Replicas stream the SQLite WAL and `git fetch` from the primary, then serve clones and browsing. They suit the "mirror mode" idea and need no queue service. Assumption: public read traffic is the main scale problem.
  Done when: a second container started from the same compose file with `GITTER_REPLICA_OF=<url>` serves a repo page within 10s of a push to the primary.

- **Wild: Store repos as a single file, Fossil-style** — FEATURE — value 3 — effort L — risk high — category: storage
  Why: it questions "git CLI as the storage engine". One SQLite file holding objects would make backup, restore and replication atomic and trivial. The cost is losing the native `git http-backend` and needing a compatibility layer. Wild.
  Done when: a prototype repo clones and pushes via standard git while all its data lives in one file, with a benchmark against the bare-repo path.

- **Wild: Write pipelines as Starlark or WASM components instead of YAML** — FEATURE — value 3 — effort L — risk med — category: pipeline language
  Why: YAML subsets always grow loops and conditionals. A small deterministic Starlark script gives matrices, reuse and testing without a templating mess, and it can be checked offline by `gitter ci run`. Wild.
  Done when: a pipeline script generates a 3x3 matrix, and `gitter ci lint` validates it without running anything.

- **Wild: Let volunteers join runners over a mesh network** — FEATURE — value 2 — effort L — risk high — category: runners
  Why: public instances can't afford big runner fleets. A signed, sandboxed, opt-in runner pool lets trusted users donate spare machines, with jobs limited to public repos and verified by a second runner. Wild. Assumption: bwrap-level isolation is trusted enough.
  Done when: a runner on a separate machine registers using a join token and completes a public job, and a duplicate verification run matches its output hash.

- **Wild: Make the forge's own state a live git repo** — FEATURE — value 2 — effort M — risk med — category: portability
  Why: issues, PRs and comments are stored as refs and files in each repo, such as `refs/gitter/issues`. `git clone` then carries the project's whole history, with SQLite as a fast index rebuilt from it. This makes migration and federation approaches nearly free. Wild.
  Done when: deleting the SQLite index and running `gitter reindex` restores all issues and PRs from the repo alone.

- **Ship a conformance suite that runs real git clients against every release** — CLEAN — value 3 — effort S — risk low — category: reliability
  Why: git protocol edge cases (shallow, partial clone, protocol v2, large pushes) are an approach in themselves: test against git's own behaviour rather than assuming it. The pack only mentions curl, git and Playwright e2e. Assumption: git versions vary across users' machines.
  Done when: CI runs clone, shallow, partial and push scenarios against several git versions and fails on any difference.
