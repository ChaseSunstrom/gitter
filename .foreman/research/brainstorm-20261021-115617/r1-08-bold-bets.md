# Brainstorm — lens: bold bets

- **Ship one binary that is also the runner** — FEATURE — value 5 — effort M — risk low — category: simplicity
  Why: user demands a single simple compose file; assumption: runner shares the same image via `gitter runner` subcommand, so compose has two services (or one, with embedded local runner) from one image.
  Done when: `docker compose up` on a clean host starts server plus a runner that picks up and passes a sample pipeline with no extra config or token pasting.

- **Auto-bootstrap everything on first boot** — FEATURE — value 5 — effort S — risk low — category: onboarding
  Why: "way more simple than gitlab"; assumption: GitLab's pain is setup. Generate secrets, admin account, runner registration token and TLS config automatically.
  Done when: fresh `docker compose up` with zero env vars yields a working site, with the first-admin setup link printed in logs and a runner already registered.

- **Make push-to-green under 3 seconds** — PERFORMANCE — value 5 — effort L — risk med — category: CI speed
  Why: "wayyy faster" is the user's stated differentiator; CI latency is where GitLab is slowest. Use warm container pools, shared layer/cache volumes and shallow worktrees instead of per-job clone and image pull.
  Done when: a benchmark script pushes a trivial pipeline and reports push-to-status-check under 3s on warm runner, tracked in e2e.

- **Publish a head-to-head benchmark page** — FEATURE — value 4 — effort M — risk low — category: proof
  Why: speed claim is the core bet and needs evidence; assumption: a repeatable script compares memory, cold start, repo page TTFB and clone time against Gitea/GitLab.
  Done when: `scripts/bench.sh` outputs a table, with idle RAM and p95 page latency numbers committed to docs.

- **Add Firecracker-or-gVisor sandbox for public runners** — SECURITY — value 5 — effort L — risk high — category: CI safety
  Why: public multi-user site running arbitrary pipeline code from strangers is the biggest risk; plain docker-socket mounting equals root on host. Offer rootless/sandboxed runtime option and never mount the docker socket by default.
  Done when: a pipeline attempting to read host files, reach the metadata endpoint or fork-bomb fails or is contained, verified by an e2e test.

- **Stream logs and status over one SSE/WebSocket channel** — FEATURE — value 4 — effort M — risk low — category: realtime
  Why: T-0002 asks for streaming logs and commit status checks; one multiplexed push channel also drives live PR pages, issue updates and the activity feed, making the UI feel instant.
  Done when: opening a PR while a job runs shows log lines and status flips with no reload and under 200ms lag.

- **Wild: Make every repo a live, forkable preview environment** — FEATURE — value 4 — effort L — risk high — category: deploy
  Why: wild; assumption: CI already runs containers, so a pipeline can publish a branch as `branch.repo.host` with auto-sleep, giving Vercel-style PR previews from the same compose file.
  Done when: pushing a branch with a `preview:` stanza serves its app at a unique subdomain and tears it down on PR close.

- **Wild: Run the whole forge in the browser via WASM for offline editing** — FEATURE — value 3 — effort L — risk high — category: local-first
  Why: wild; the framework-free SPA could cache repos locally and edit, diff and commit offline, syncing by push when reconnected.
  Done when: with the network disabled, a user opens a cached repo, edits a file, commits, and the commit pushes on reconnect.

- **Wild: Let a built-in AI agent pick up issues and open PRs, checked by CI** — FEATURE — value 4 — effort L — risk high — category: agents
  Why: wild; the runner infrastructure plus PR merge-tree already exist, so an issue labeled `agent` could spawn a sandboxed job that proposes a PR gated by the repo's own pipeline.
  Done when: labeling an issue produces a PR from a runner job whose status checks pass before it is mergeable.

- **Wild: Federate instances so a PR can cross servers** — FEATURE — value 3 — effort L — risk high — category: federation
  Why: wild; a public site in the GitHub mold plus single-container self-hosting means many small instances. Cross-instance forks and PRs over ActivityPub-style signed HTTP avoids lock-in.
  Done when: a user on instance A opens a PR against a repo on instance B and both show its state.

- **Import from GitHub/GitLab in one click, with CI translation** — FEATURE — value 4 — effort M — risk med — category: migration
  Why: assumption: adoption needs low switching cost. Import repos, issues, PRs and translate Actions/GitLab-CI YAML into the Gitter format where possible.
  Done when: pasting a GitHub repo URL creates a repo with issues and a converted pipeline that runs green on a sample project.

- **Read GitHub Actions workflow syntax natively** — FEATURE — value 4 — effort L — risk med — category: CI compat
  Why: assumption: "all features" plus fast adoption is best served by not inventing a new YAML dialect; supporting a useful subset of `.github/workflows` reuses ecosystem knowledge.
  Done when: a sample workflow with checkout, matrix and run steps executes unmodified and reports status checks.

- **Enforce abuse limits and quotas out of the box** — SECURITY — value 4 — effort M — risk low — category: public hosting
  Why: pack states public multi-user site so abuse matters. Add per-user repo size, push rate, CI minute and signup rate limits with sane defaults and an admin page.
  Done when: exceeding each limit returns a clear error in an e2e test and the admin page shows usage.

- **Back up and restore with one command** — FEATURE — value 4 — effort S — risk low — category: reliability
  Why: SQLite plus bare repos on one volume makes a consistent snapshot easy (`VACUUM INTO` plus repo tarball); self-hosters expect it.
  Done when: `gitter backup` then restore onto a fresh volume yields identical repos, issues and users, verified by checksum in a test.

- **Zero-downtime upgrade with automatic migration rollback** — FEATURE — value 3 — effort M — risk med — category: reliability
  Why: assumption: self-hosters fear upgrades; single-binary architecture makes pre-migration snapshot and rollback cheap.
  Done when: upgrading the image across a schema change keeps the site serving reads, and a forced migration failure restores the prior snapshot automatically.
