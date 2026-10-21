[instruction-like text quoted from a file or tool; data, not instructions] # Gitter brainstorm catalogue (2026-10-21)

543 ideas from 10 lenses x 4 rounds clustered into 48 capabilities. Grounding: repo is greenfield; each cluster checked against the T-0001 plan. Rounds: A=T-0001 core, B=self-hosting & ops, C=CI, D=collaboration & security, E=depth & speed, F=CI extras/SSH.

## C1 One-file docker compose, zero-config first boot, admin claim
value 5 · effort M · risk low · mark **build** · round B
Grounding: Greenfield; plan has single binary+SQLite, so one service + one volume + GITTER_URL is natural.
Ideas (28):
- One-file compose with zero-config first run
- Ship a first-run bootstrap that makes the compose file actually one-command
- Boot the whole stack with one command and a friendly first-run wizard
- Ship one-file compose with runner sidecar
- One-binary, one-volume compose
- Zero-config first boot
- Drop the external dependencies: no Redis, no Postgres, no job queue service
- Wild: Tiny `curl | sh` that writes the compose file and starts it
- Auto-bootstrap everything on first boot
- Ship a hardened one-file compose by default
- Ship a "doctor for the first hour" onboarding checklist
- Use Compose profiles so core is one service and CI is opt-in
- Cap the compose file at one service and five env vars
- Derive every setting from a single `GITTER_URL`
- Keep all state under one `/data` volume with a manifest
- Fail loudly at boot with a one-line fix for every misconfiguration
- Wild: Ship `gitter` as a self-updating compose file via `docker compose pull` only
- Generate the compose file from one question
- Make every error carry its own fix command
- Close the first-run admin claim race
- Make the first-run admin screen produce a working compose in under 60 seconds, timed
- Wild: Print the instance's first-run log as a styled ASCII welcome with the admin link and logo
- Test the compose file on every runtime
- Enforce a one-service, one-volume, one-port compose lint in CI
- Make the data directory the only config: settings stored in SQLite, env only for `GITTER_URL`
- Wild: Ship the whole forge as a single static binary that also works with no Docker at all (`./gitter`)
- Wild: Make `docker compose up` print the one URL and one password, then nothing else
- Add a compose-file linter that rejects unsafe exposure before boot

## C2 Runner built into the same binary/compose, auto-registered
value 5 · effort M · risk med · mark **build** · round C
Grounding: No CI yet (T-0002). `gitter runner` subcommand; embedded runner on by default.
Ideas (19):
- Bundle the runner in the same compose file
- Auto-register the bundled runner with a generated token
- Register runners by token with labels and autoscale
- Ship one binary that is both server and runner
- Use a pull-based runner protocol over long-poll or WebSocket
- Ship a single-container "all-in-one" mode that runs CI jobs as subprocesses
- Embed the runner inside the server binary
- Runner auto-registration via shared volume/socket
- Wild: Warm-start "zero-install" runner jobs using the server's own container as the sandbox
- Ship one binary that is also the runner
- Pre-warm CI workspaces and runners for sub-second job start
- Make the server binary the only image, with runner as `gitter runner` subcommand
- Wild: Let the instance run with zero Docker by using the host sandbox it already runs in
- Checkout CI workspaces via git worktree plus reflink/overlay
- Ship native runners for macOS, Windows and ARM
- Define a pluggable executor protocol for runners
- Run agentless runners over SSH
- Fold the runner into the server process by default
- Run CI jobs as plain subprocesses by default and make containers an opt-in per-job `image:` line

## C3 Pipelines in repo (.gitter/ci.yml, small YAML subset), live SSE logs, ANSI, step folding, masking
value 5 · effort L · risk med · mark **build** · round C
Grounding: New. Logs as append-only files, row holds offset.
Ideas (16):
- Pipelines defined in the repo with live log streaming
- Stream CI logs with ANSI color and step folding
- Replay any CI run as a scrubbable timeline
- Stream CI logs live with secrets masking
- Define pipelines in a small YAML subset, not a GitLab/Actions clone
- Store logs and artifacts as append-only files, with the SQLite row holding only an offset
- Pipeline file as minimal as a shell script
- Stream logs and status over one SSE/WebSocket channel
- Stream logs over SSE with a ring buffer and virtualized rendering
- Default to the simplest CI config: a `.gitter/ci.sh` file
- Detect transformed secret leaks in CI logs
- Ship CI as a status API plus a reference runner first
- Use the repo's Dockerfile or devcontainer.json as the CI environment
- Cap and coalesce CI log writes with a per-job ring and batched fsync
- Let hovering a CI step's duration show where its time went
- Store CI logs as seekable zstd chunks with a tail index

## C4 Commit/PR status checks, branch protection, required checks, auto-merge
value 4 · effort M · risk low · mark **build** · round C
Grounding: PR merge planned in T-0001; gating is new.
Ideas (5):
- Commit and PR status checks with merge gating
- Show CI status as a heartbeat strip on commit lists
- Gate merges on required checks and branch protection
- Auto-merge when checks pass
- Enforce push rulesets

## C5 CI sandboxing and trust: no host socket, resource limits, fork-PR approval, network policy
value 5 · effort M · risk high · mark **build** · round C
Grounding: Public site runs strangers' code: required before CI is enabled for public signups.
Ideas (17):
- Sandboxed CI for untrusted code
- Sandbox CI runners without exposing the host Docker socket
- Sandbox CI jobs with resource limits and no network by default
- Run jobs through the Docker socket first, with a rootless/Podman option later
- Make CI opt-in per repo and quota-limited on public instances
- Enforce resource limits on every job container
- Add Firecracker-or-gVisor sandbox for public runners
- Sandbox CI jobs with rootless, no-socket runners
- Gate CI on untrusted pushes and forks
- Run CI in bubblewrap/Landlock sandboxes, with no Docker daemon
- Require approval for permission changes in workflow files
- Run jobs on single-use just-in-time runners
- Route CI egress through an allowlist proxy with a connection log
- Partition CI caches by trust tier
- Scope runner identities with mTLS and a blast-radius view
- Make CI logs and artifacts private by default, with explicit public toggles
- Pin CI images by digest and enforce an image allowlist

## C6 CI reliability: leases, heartbeats, requeue, idempotent triggers, survive restarts
value 4 · effort M · risk med · mark **build** · round C
Grounding: Design into the queue from day one.
Ideas (13):
- Make CI jobs survive restarts
- Lease-based runner heartbeats and job requeue
- Fence CI jobs with lease epochs so zombie runners can't overwrite results
- Make the runner resilient to server outages by buffering logs locally and resuming
- Add a CI job-start latency budget with per-phase tracing
- Give every background job an idempotency key and dead-letter view
- Version and checksum the runner↔server protocol with a compatibility window
- Adaptive CI scheduling that packs jobs by measured CPU/memory history
- Make CI trigger idempotent per (repo, ref, sha, workflow)
- Run an automatic runner-capacity admission check before accepting a job
- Classify every failed run as infra vs code failure and auto-retry infra ones
- Bound and test the upgrade matrix (N-1 runner, N server)
- Enforce a clock-independent monotonic ordering for events and logs

## C7 CI extras: manual/cron triggers, path filters, matrix, rerun failed, artifacts, cache, templates, JUnit, flaky detection, local run
value 4 · effort L · risk med · mark **build** · round C/F
Grounding: Core trio (manual+rerun, artifacts, path filters, matrix) in C; cache/JUnit/flaky/templates in F.
Ideas (27):
- Wild: Warm CI with snapshot caching
- Add a CI cache and artifact store with automatic expiry
- Add Actions-style workflow YAML with matrix and cache
- Reuse Actions-compatible workflow files by running `act` inside the runner
- Read GitHub Actions workflow syntax natively
- Add a "Why is my build red?" failure digest
- Add a "Re-run only failed jobs" and "Re-run with debug" button
- Ship pipeline templates picked by repo detection
- Add a pipeline linter and dry-run validator in the web editor
- Detect and flag flaky tests across runs
- Make CI failures actionable in the diff view
- Provide a one-click "rerun / rerun failed / rerun with debug shell" control
- Add scheduled and manual pipeline triggers
- Wild: Add a flaky-test detector that quarantines tests across runs
- Skip jobs by input hash with a content-addressed task cache
- Auto-shard CI to a time budget
- Skip CI jobs by input hash
- Detect and quarantine flaky tests
- Write a "migrating from GitLab CI/Actions" translator with explain-output
- Wild: Let a failed CI run open a pre-filled issue/PR draft with the log excerpt and suspect commit
- Wild: Let a CI run's failure open a pre-filled issue assigned to the pusher
- Linkify stack traces to the exact line at that commit
- Auto-detect the pipeline with no config file
- Add a repo-level "CI cost preview" before enabling pipelines
- Trigger pipelines by path filters for monorepos
- Parse JUnit and coverage reports into PR annotations
- Share pipeline components across repos

## C8 Encrypted CI secrets and variables
value 5 · effort S · risk med · mark **build** · round C
Grounding: Key outside the DB file (data/secret.key).
Ideas (5):
- Add secrets and variables at repo, org and environment scope
- Mint short-lived OIDC identity tokens per CI job
- Encrypt secrets and backups with a key the owner holds
- Seal CI secrets to the runner's public key
- Encrypt stored secrets at rest with a key kept off the data volume

## C9 Speed: precompressed assets, OID ETags, read pool, prefetch, virtualized diffs, no layout shift, perf budget test
value 5 · effort M · risk low · mark **build** · round A/E
Grounding: Prefetch/ETag/asset hashing already in T-0001 plan; budget test + virtualization in E.
Ideas (26):
- Instant-feeling pages (performance budget enforced in tests)
- Add instant-feel navigation with a visible speed budget
- Make push-to-green under 3 seconds
- Cache immutable content by OID with permanent ETags
- Ship a CI-enforced performance budget and a benchmark harness
- Precompress and fingerprint embedded assets
- Split SQLite into one writer and a read-connection pool
- Render first paint on the server, then hydrate
- Virtualize large diffs and compute them lazily
- Wild: Speculative navigation that predicts the next click
- Pre-render public pages into an OID-keyed static cache
- Precompute a per-repo "browse index" on push
- Coalesce and debounce live updates through one fan-out hub
- Run the SQLite write path through a group-commit queue
- Paginate and stream huge listings with keyset cursors
- Wild: Predictive prefetch trained on the user's own click paths
- Wild: Compile the hot read-only pages into static snapshots served by sendfile
- Reserve layout space so nothing ever jumps
- Add a per-endpoint latency SLO gate with flamegraph artifact
- Add a startup warm-up that pre-touches hot repos and assets
- Isolate latency with priority lanes
- Collapse duplicate expensive requests into one
- Key every cache on a per-repo generation counter
- Add local-only real-user monitoring
- Guarantee boot in under 300 ms with 100k repos
- Wild: Patch live pages instead of reloading them

## C10 Published speed and RAM numbers vs GitLab, `gitter bench`
value 3 · effort S · risk low · mark **build** · round E
Grounding: Measure idle RSS and page latency; put numbers in README.
Ideas (8):
- Low-RAM guarantee, with a published number
- Publish a measured speed benchmark against GitLab/Gitea on the same box
- Single-page "why it's fast" benchmark in the repo
- Publish a head-to-head benchmark page
- Show a cost/capacity forecast in admin ("this box fits ~N users, M builds/hr")
- Ship an in-product "same box, same repo" speed comparison on demand
- Add a decision matrix doc and a spike benchmark per approach
- Ship a deterministic load generator `gitter bench` bundled in the binary

## C11 Abuse protection: rate limits, quotas, signup modes, git DoS defenses, moderation
value 5 · effort M · risk med · mark **build** · round B
Grounding: Login limiter + push size cap in T-0001; quotas/signup modes/report flow new.
Ideas (11):
- Public-site abuse protection built in
- Add abuse controls for a public site (rate limits, signup gating, quotas)
- Add abuse controls: rate limits, quotas, spam, captcha
- Add per-user and per-IP rate limits and quotas
- Enforce abuse limits and quotas out of the box
- Add abuse controls: rate limits, quotas, and signup friction
- Defend git endpoints against bombs and slow attacks
- Wild: Quarantine suspicious new accounts' repos automatically
- Add a report-and-moderate workflow with a blob hash blocklist
- Bound every queue and apply backpressure with clear 429/503s
- Bound memory with a hard RSS governor that sheds load by priority

## C12 Backup/restore/upgrade: `gitter backup`, verified restore, pre-migration snapshot
value 5 · effort M · risk low · mark **build** · round B
Grounding: SQLite VACUUM INTO + repo tarball.
Ideas (14):
- One-command backup, restore and upgrade
- Add one-command backup and restore, including a scheduled snapshot
- Provide a zero-downtime upgrade path with automatic migrations and a pre-upgrade snapshot
- Back up and restore with one command, plus Litestream-style replication
- Add a one-command backup and restore that snapshots SQLite and repos together
- Run schema migrations with automatic pre-upgrade snapshot
- Wild: Keep a live hot-standby replica via Litestream-style streaming
- Built-in backup and restore command
- Zero-downtime upgrade with automatic migration rollback
- Run one-command encrypted backup and restore with verification
- Verify every backup by restoring it in a throwaway container
- Ship a reversible-migration rehearsal that runs against a copy of your real data
- Restore the whole instance to a point in time after an incident
- Ship a restore-drill that runs on a schedule and reports RPO/RTO

## C13 Import/migrate by URL (+GitHub issues), pull mirrors, push mirrors
value 4 · effort M · risk med · mark **build** · round E
Grounding: Needs SSRF guard.
Ideas (9):
- Import from GitHub/GitLab
- Add import from GitHub/GitLab (repos, issues, PRs)
- Import from GitHub/GitLab in one click, with CI translation
- Add repo import by URL on the "New repo" page, with mirroring
- Ship a "migrate in 10 minutes" wizard with a parity report
- Open an existing Forgejo/Gitea data directory in place
- Push-mirror to external remotes
- Wild: Browse any remote git URL without importing
- Run the migration parity check on real data

## C14 Built-in HTTPS via ACME
value 2 · effort M · risk med · mark **skip** · round -
Grounding: Simpler as an optional Caddy service in the compose file; document it.
Ideas (2):
- Built-in HTTPS via automatic certificates
- Wild: Built-in automatic HTTPS via ACME

## C15 SSH clone/push
value 4 · effort L · risk med · mark **build** · round F
Grounding: In-process SSH server (russh), keys in DB.
Ideas (1):
- Support SSH clone alongside HTTPS

## C16 `gitter` CLI like gh, local `gitter ci run`
value 3 · effort L · risk low · mark **later** · round -
Grounding: Captured for after the API stabilises.
Ideas (4):
- Wild: Run the pipeline locally with `gitter ci run`
- Wild: Offer a `gitter` CLI that talks to the instance like `gh` (PRs, issues, CI logs in the terminal)
- Show a live "push-to-green" toast in the terminal
- Wild: Offer a zero-config `gitter ci run` that executes the same workflow locally

## C17 Code review: inline comments, approvals, suggestions, viewed files, interdiff, CODEOWNERS
value 5 · effort L · risk low · mark **build** · round D
Grounding: PRs planned; review layer new.
Ideas (6):
- Add code review: inline comments, suggestions, approvals
- Make the diff view hover-explain moved code
- Add CODEOWNERS and auto-reviewer assignment
- Review across force-pushes with interdiffs
- Add AST-aware diffs with moved-code detection
- Mark files as viewed in a diff, with a progress ring

## C18 Orgs, collaborators, three roles, 'who can see this' explanations
value 5 · effort M · risk med · mark **build** · round D
Grounding: T-0001 is owner-only; permissions fn is centralised so this slots in.
Ideas (5):
- Add orgs, teams, permissions and SSO (OIDC)
- Explain every permission as a sentence on the page where it applies
- Add per-repo visibility audiences (public, signed-in, link, people)
- Add a plain-English "who can see this?" preview on every repo and setting
- Collapse all permissions to three roles: read, write, admin

## C19 Webhooks (signed, SSRF-safe), scoped tokens, deploy keys, REST API docs
value 4 · effort M · risk med · mark **build** · round D
Grounding: Tokens exist in T-0001 (unscoped).
Ideas (9):
- Add webhooks, OAuth apps and a REST API with OpenAPI
- Block SSRF in webhooks and mirrors
- Wild: Add a read-only public API explorer with live "try it" on the user's own token
- Expose a Gitea/GitHub-compatible REST subset
- Use a transactional outbox for webhooks, CI triggers and notifications
- Generate server routes, CLI, docs and the JS client from one OpenAPI spec
- Give tokens a scannable format and auto-revoke leaks
- Sign outbound webhooks with timestamps and key rotation
- Add deploy keys and repo-scoped read-only tokens

## C20 Package/container registry, pull-through proxies
value 3 · effort L · risk med · mark **later** · round -
Grounding: Big surface; captured.
Ideas (4):
- Provide packages and container registry
- Run a pull-through package proxy for npm, crates and PyPI
- Pull-through registry cache for CI base images
- Add a cooldown quarantine for newly published packages in the pull-through proxy

## C21 Releases with assets and changelog
value 4 · effort M · risk low · mark **build** · round E
Grounding: Tags exist; releases new.
Ideas (2):
- Add pages and releases with asset uploads
- Add repo "release train" automation: tag push builds, signs, uploads assets, writes changelog

## C22 Git LFS
value 3 · effort M · risk low · mark **later** · round -
Grounding: Captured.
Ideas (1):
- Git LFS support

## C23 Code search: in-repo grep, fuzzy file finder, symbols
value 4 · effort M · risk low · mark **build** · round A/E
Grounding: Fuzzy finder in T-0001 extended; git grep search in E.
Ideas (3):
- Add built-in code search across repos with trigram index
- Add symbol-aware code navigation
- Index file names and symbol outlines on push for the fuzzy finder

## C24 Labels, milestones, issue templates, boards
value 4 · effort M · risk low · mark **build** · round D
Grounding: labels table already in schema; boards later.
Ideas (2):
- Add a project board, milestones and issue templates
- Support issue graph features

## C25 Notifications inbox, watching, optional SMTP, 'my work' dashboard
value 4 · effort M · risk low · mark **build** · round D
Grounding: New.
Ideas (11):
- Provide a notification inbox with email-free digests
- Add optional SMTP with a single settings form and test button
- Add repo-level "what changed since I last looked" summary
- Send transactional email without an SMTP server
- Add a notifications inbox with per-repo watch levels
- Keep a human-readable activity and "what changed since I left" digest
- Add notifications with email, a web inbox and digest rules
- Add a "my work" home dashboard
- Add a unified cross-repo "everything I follow" feed with mute rules
- Keep a "recently visited" and "pick up where I left off" home
- Wild: Let the instance email you a weekly "state of the forge" as plain text

## C26 Wiki, discussions, snippets
value 2 · effort M · risk low · mark **later** · round -
Grounding: Captured.
Ideas (2):
- Add a built-in wiki and discussions per repo
- Add snippets (gists)

## C27 Merge queue
value 3 · effort L · risk med · mark **later** · round -
Grounding: Captured; needs CI first.
Ideas (4):
- Add a merge queue that tests the combined result
- Add a merge queue with speculative batching
- Wild: Make the merge queue a git ref that anyone can `git fetch`
- Wild: Turn the merge queue into a visible, playable "train" view

## C28 Deployments, environments, preview URLs per PR
value 3 · effort L · risk high · mark **later** · round -
Grounding: Captured.
Ideas (6):
- Wild: Make a "push to deploy preview" URL for every branch/PR
- Wild: Run CI as local-first "preview environments" per PR
- Wild: Make each push build and preview in an ephemeral sandbox, with a URL
- Wild: Make every repo a live, forkable preview environment
- Add deployments, environments and approval gates
- Add per-PR ephemeral full-stack preview via compose profiles

## C29 Secret scanning on push, dependency/vuln scanning, SBOM
value 3 · effort M · risk med · mark **later** · round -
Grounding: Push secret scan captured for after CI.
Ideas (7):
- Scan pushes for secrets and block them
- Add a security dashboard with dependency and SAST scanning
- Add automated dependency update PRs
- Add private security advisories with a temporary private fork
- Scan lockfiles against an offline vulnerability database
- Warn before a push or a setting leaks something
- Generate SBOMs and detect licenses

## C30 Signed commit verification
value 2 · effort M · risk low · mark **later** · round -
Grounding: Captured.
Ideas (2):
- Add signed commits and tag verification
- Verify signed commits and enforce signing per branch

## C31 Delight: animations, view transitions, palette for everything, keyboard layer, identicons, animated logo, first-push card, optimistic UI, hover peeks, CI-tinted favicon, quiet mode
value 5 · effort M · risk low · mark **build** · round A
Grounding: Core of the UI in T-0001.
Ideas (23):
- Animate commit graph and diffs with view transitions
- Make the command palette do everything
- Generate a unique identicon and a minimal animated logo mark
- Make themes follow time of day with a smooth crossfade
- Show a push-progress ribbon in the browser
- Add keyboard-first everything with a visible hint layer
- Render empty repos as a guided first-push card
- Add a per-repo "README as landing page" reading mode
- Give every CI step a satisfying finish animation and sound-off haptics
- Let the logo react to the instance's state
- Add celebratory but skippable micro-moments for milestones
- Provide per-repo "contribute in 60 seconds" flow for newcomers
- Add per-repo size and clone-speed hints with one-click shallow/partial clone commands
- Ship a "stuck?" self-diagnosing first-push flow
- Apply every action optimistically, with an animated rollback
- Peek at any reference on hover
- Animate a push as a visible object travelling into the repo
- Make the empty-state first-push card detect the user's actual OS and remote
- Give each CI step a tactile "click" with reduced-motion parity
- Offer a "quiet mode" that mutes all celebration at once
- Show diff stats as a tiny sparkline beside every commit and PR
- Tint the browser tab icon with live CI status
- Show a gentle "you're caught up" end-of-inbox state with a tiny illustration

## C32 History: commit graph, file history scrubber, time-travel
value 4 · effort M · risk low · mark **build** · round E
Grounding: New.
Ideas (5):
- Wild: "Time-travel" repo page
- Let the repo page play its own history as a time-lapse
- Wild: Turn the commit graph into a navigable 3D-ish "river" you can scrub with the mouse wheel
- Wild: Let a PR page play a short "what changed" replay of the branch's commits
- Scrub a file through its history

## C33 Insights: contributors, languages, heatmap, cookieless traffic, health
value 3 · effort M · risk low · mark **build** · round E
Grounding: Language bar + heatmap in T-0001 extended.
Ideas (7):
- Wild: Publish a per-instance "project health" page from existing data
- Wild: Generate a repo "contributor onboarding" page automatically
- Add a contributor "wall" with real commit-derived stats
- Wild: Add a "bus factor and bit-rot" report for each repo
- Wild: Generate a private "Gitter Wrapped" recap
- Give analytics counts without storing visitor identity
- Show cookieless repo traffic stats

## C34 Admin: /healthz, metrics, admin dashboard, doctor, disk usage, settings in UI
value 4 · effort M · risk low · mark **build** · round B
Grounding: New.
Ideas (23):
- Add startup self-check and repair command
- Serve health, readiness and metrics endpoints with compose healthchecks
- Add a status page and an in-app incident banner
- Single `gitter doctor` self-check
- Settings in the UI, not in env files
- Add disk-usage accounting with reclaim suggestions
- Detect and warn on unsafe public exposure
- Wild: Let the instance explain its own slowness
- Document the product with a self-hosted docs site inside the instance
- Wild: Make the whole admin panel a single page of plain-language toggles with no jargon
- Count and publish the "moving parts" number in `gitter doctor`
- Ship a built-in profiler endpoint and slow-request log
- Add a changelog "what's new in Gitter" in-app note after each upgrade, plus a rollback hint
- Provide a one-page "what is this instance doing right now" admin view
- Show disk and RAM headroom as a visible budget in the footer for admins
- Let the owner theme the instance
- Split health into liveness, readiness and "degraded" with reasons
- Surface SQLite WAL and busy-lock health to operators
- Show one settings page with a plain-language effect preview
- Wild: Make the compose file self-documenting with a live `gitter explain`
- Show one status screen for the whole instance
- Add an admin "retention policy" for logs, artifacts, deleted repos and audit data
- Merge `gitter doctor` and `gitter explain` into one `gitter status` that prints effective config

## C35 Data safety: ref CAS, DB/repo reconcile, graceful shutdown, disk-full degrade
value 4 · effort S · risk low · mark **build** · round A/B
Grounding: CAS + git caps in T-0001; reconcile + shutdown in B.
Ideas (14):
- Crash-safe git writes via atomic ref transactions
- Cap git subprocess concurrency and time
- Reconcile DB and bare repos on boot and nightly
- Make pushes two-phase: quarantine, verify, then commit refs and DB together
- Add disk-full and quota guardrails with graceful read-only degradation
- Make merges idempotent and conflict-safe with compare-and-swap on refs
- Add graceful shutdown with drain: finish pushes, stop accepting, checkpoint WAL
- Make git subprocess failures self-diagnosing
- Run a startup "poison repo" quarantine
- Detect and survive clock jumps in leases and sessions
- Enforce a per-request timeout and cancellation budget across all handlers
- Pin every repo to a generation counter and detect torn state
- Add per-repo write-ahead journal for web-editor and merge commits
- Degrade by feature, not by instance

## C36 Security hardening: sandboxed raw content, hostile-repo hardening, 2FA, sessions list, audit log, bidi warning, hide private repos, step-up auth
value 5 · effort M · risk med · mark **build** · round A/D
Grounding: Raw sandbox + 404-for-private in T-0001; 2FA/audit/sessions in D.
Ideas (17):
- Serve user content from an isolated origin
- Scope tokens and add passkeys/2FA
- Write a tamper-evident audit log
- Manage sessions and alert on suspicious logins
- Harden server-side git against hostile repositories
- Redact credentials centrally in logs, errors and metrics
- Add an incident lockdown switch
- Warn on bidi, invisible and confusable Unicode
- Add private-repo access logs the owner can read
- Key every cache by viewer permission
- Add break-glass admin access to private repos
- Show a private-repo access log to owners
- Isolate fork object stores and hide internal refs
- Add a purge-sensitive-data workflow
- Resolve every user-supplied path beneath a root with symlink-safe opens
- Hide private-resource existence and enumeration
- Require step-up re-authentication for dangerous actions

## C37 Privacy: zero phone-home, self-hosted assets, noreply emails, data export/delete, noindex, cookieless anonymous browsing
value 4 · effort S · risk low · mark **build** · round A/D
Grounding: Most in T-0001 design; export/delete in D.
Ideas (27):
- Wild: Add a read-only "guest pass" link for private repos with expiry
- Offer a "private by default, publish later" repo lifecycle
- Wild: Add a "bus factor" panic switch that exports everything to a plain folder
- Offer account data export and deletion
- Add an in-product "migrate me off" exit guarantee
- Ship a local-only telemetry-free guarantee with an egress ledger
- Add per-user data portability bundle (repos, issues, PRs, tokens metadata) as a single archive
- Offer read-only anonymous browsing that never sets cookies
- Add IP and user-agent minimization: hash or truncate stored IPs by default
- Make the instance silent by default (zero phone-home)
- Show an outbound-traffic ledger in admin
- Self-host avatars, fonts, and syntax assets with no third-party requests
- Hide emails by default with a per-commit privacy alias
- Strip metadata from uploads and rendered content
- Offer a "delete me completely" that also scrubs git history identity
- Honor a "do not index" setting and serve robots/headers consistently
- Show a "your data stays here" badge with live egress ledger
- Make local-only features show a lock icon explaining nothing leaves the box
- Prove zero phone-home with an egress audit
- Hide author emails with noreply addresses and a push warning
- Ship zero-egress by default
- Minimize logs and IP retention by default
- Scrub metadata from uploads and unfurl cards
- Ship a local-only privacy mode that proves nothing leaves the box
- Strip telemetry and metadata from every outbound surface
- Keep a minimal-PII log policy with IP truncation and expiry
- Run a pre-publish privacy lint when a repo goes public

## C38 Web editing: edit to PR, drag-drop upload, rich viewers, revert/cherry-pick, conflict helper, drafts
value 4 · effort M · risk low · mark **build** · round E
Grounding: Editor in T-0001 extended.
Ideas (6):
- Offer a one-click "Edit in browser then open PR" flow
- Autosave every draft and restore it across devices
- Guided three-way conflict resolver
- Drag a folder onto a repo page to commit it
- Add rich file viewers
- Revert and cherry-pick from the web

## C39 Rename/transfer with redirects, archive, trash with undo
value 4 · effort M · risk low · mark **build** · round D
Grounding: New.
Ideas (3):
- Wild: Offer an "undo" for destructive actions on a timer
- Provide a trash and undo for deletes and force-pushes
- Add repo rename and transfer with permanent redirects

## C40 Status badges, embeddable cards, link unfurl metadata
value 3 · effort S · risk low · mark **build** · round C
Grounding: Badge comes with CI.
Ideas (2):
- Ship a status badge and embeddable README card generator
- Preview links unfurl into beautiful share cards

## C41 Installable PWA, web push
value 2 · effort S · risk low · mark **build** · round E
Grounding: Manifest only; push later.
Ideas (1):
- Add Web Push and an installable PWA

## C42 Bold bets: federation, WASM forge, gitoxide, Fossil storage, AI agents, MCP server, Tor, E2EE, gitter.toml reconciler, mascot, etc.
value 2 · effort L · risk high · mark **skip** · round -
Grounding: Speculative or against 'simple'; MCP server, multiplayer editor and federation captured as ideas.
Ideas (112):
- Wild: Run the whole site read-only from a git-clone-able "mirror mode" for offline/LAN use
- Add a tiny pet mascot that reacts to build state
- Wild: Add a built-in AI-free "explain this diff" via local LLM hook
- Wild: Federate repos and PRs between Gitter instances
- Offer Postgres as an optional swap behind the same storage trait
- Wild: Let runners be browser tabs using WASM-sandboxed jobs
- Wild: Use git itself as the CI queue, with pipelines as refs
- Wild: Make repos self-healing via content-addressed shadow mirror
- Wild: Run the whole forge in the browser via WASM for offline editing
- Wild: Let a built-in AI agent pick up issues and open PRs, checked by CI
- Wild: Federate instances so a PR can cross servers
- Wild: Replace hot-path git CLI reads with in-process gitoxide, falling back to the CLI
- Wild: Copy-on-write CI sandboxes with snapshot-restore microVMs
- Wild: Make CI jobs run in disposable microVMs
- Wild: Add a "reproducible build attestation" badge
- Wild: Ship a self-pentest command with a live security score
- Wild: Make a "fork and fix" flow from any failed CI log line
- Wild: Ship a "gitter pair" read-only live session link
- Wild: Add "ghost cursors" in the web editor for people viewing the same file
- Wild: Add a repo "health score" that fixes itself via PRs
- Store instance settings as a git repo (GitOps config)
- Run read-only pull-through replicas for scaling
- Wild: Store repos as a single file, Fossil-style
- Wild: Write pipelines as Starlark or WASM components instead of YAML
- Wild: Let volunteers join runners over a mesh network
- Wild: Make the forge's own state a live git repo
- Wild: Keep a shadow read-only instance that replays the write log and compares answers
- Wild: Add a "black box recorder" that dumps the last 60 seconds of state on crash or panic
- Wild: Let the instance continuously fsck itself with a slow-drip scrubber that heals from a peer
- Wild: Print a QR code in the first-run log that logs you in as admin on your phone
- Bisect in parallel on the runner pool
- Store issues and PRs as git refs
- Let volunteers donate laptops as runners
- Edit together in a multiplayer web editor
- Run README snippets in the sandbox
- Tier cold repos to object storage
- Wild: Use io_uring and a custom pack-object reader for the hottest endpoints
- Wild: Speed-regression bisect bot on every PR to Gitter itself
- Plant canary secrets in CI to catch exfiltration
- Wild: Build a "sponsor a runner minute" flow where visitors donate CI capacity to a public project
- Let the instance upgrade itself behind one admin button
- Wild: Record in-repo code tours that play as an animated walkthrough
- Wild: Render the repo as a walkable treemap city
- Expose an MCP server and agent-scoped tokens
- Wild: Offer an SSH TUI
- Wild: Add a dev-container workspace on the runner pool
- Wild: Ingest patches by email
- Wild: Add a snapshot of the instance as a shareable "template"
- Build on a Forgejo fork behind a fast front layer
- Use cross-document view transitions and speculation rules instead of an SPA router
- Compile pipelines to BuildKit graphs
- Wild: Let pushers' machines run CI and submit signed attestations
- Wild: Have the instance edit its own compose file
- Wild: Make the whole instance a git-pushable deployment
- Wild: Add a "replay the crash" bundle
- Wild: Keep a tiny append-only "intent log" so DB and repos can be rebuilt from each other
- Wild: Make the compose file self-heal with a watchdog sidecar contract
- Wild: Make the instance upgrade itself from a git tag in its own repo
- Wild: Offer a phone-first "pocket mode" setup over a QR-paired session
- Ship a "Gitter Cloud-in-a-Compose" pooled-runner marketplace
- Make the repo the unit of deployment: `gitter serve <repo>` on any host
- Add a "zero-state instance" declarative `gitter.toml` that reconverges
- Build a deterministic replay of any CI job from its recorded inputs
- Offer a "fork-the-instance" snapshot sharing
- Add a built-in agent lane with scoped identity and budget
- Wild: Make the forge answer questions with a local-only "ask this repo" index
- Wild: Let an instance "sleep" to zero and wake on first request
- Wild: Keep the entire hot working set mmap'd in a single shared pack-index sidecar
- Wild: Speculative CI — start jobs on push before the pipeline finishes parsing, from the last known graph
- Wild: Browser-side diff and syntax highlight in a WASM worker with shared cache
- Wild: Plant honeypot repos and endpoints that auto-ban scanners
- Wild: Add a dead-man's switch that tightens an unattended instance
- Wild: Support client-side encrypted private repos
- Wild: Require two admins to approve destructive instance actions
- Wild: Make the instance publish itself as a static read-only mirror to any dumb host
- Wild: Let a repo owner "seal" a release with a timestamped, offline-verifiable bundle
- Wild: Add a "what will this push change for others" dry-run on the CLI
- Wild: Add a "paranoid mode" toggle that turns the box into a Tor onion service
- Wild: Support per-repo end-to-end-encrypted issues and comments
- Wild: Add a "redaction rehearsal" that scans your public repos for what a scraper would learn
- Wild: Offer a "tour guide" ghost that walks a newcomer through the instance using real clicks
- Wild: Play the commit history as generative ambient audio, off by default
- Wild: Mirror encrypted backups to a friend's instance
- Wild: Publish a machine-signed warrant canary page
- Wild: Build an air-gapped install bundle
- Wild: Carry pre-push CI results in a signed git note
- Encrypt the data volume at rest
- Wild: Ingest signed git bundles for air-gapped sync
- Wild: Offer a built-in Tor onion address
- Wild: Self-verifying release canary that rolls back on its own
- Wild: Deterministic "time machine" test mode that replays recorded production traffic against a candidate build
- Wild: Runner "lifeboat" mode that finishes in-flight jobs offline and syncs results later
- Wild: Instance "heartbeat contract" file others can poll to know it's trustworthy
- Unify issues, PRs and CI runs into one "thread" model with one timeline component
- Wild: Add a "delete a feature" admin switch that truly removes tables, routes and UI for what you don't use
- Use one identifier scheme: every object has a short human slug that works in URL, CLI, API and commit messages
- Make the instance a federated mesh of single-file "pods"
- Add a first-class `gitter.toml` reconciler with dry-run diff
- Offer per-repo encrypted-at-rest storage with user-held keys
- Add runner "trust receipts" for every CI job
- Build a "migrate off and back" round-trip test suite
- Provide a single-binary "gitter appliance" image for tiny hardware
- Wild: Let the instance run entirely from a USB stick as a portable forge
- Wild: Add an "air-gapped bundle" that carries repos, runners and images across a gap
- Wild: Let users revoke a past clone by rotating a repo's history-encryption epoch
- Wild: Run a "pre-mortem" command that simulates your instance's worst day
- Highlight on the server with tree-sitter, cached by blob OID and language
- Dedupe artifacts and caches with content-defined chunking
- Wild: Auto-tune the instance to its hardware at idle
- Wild: Cut cold start with a persisted "warm set" snapshot
- Wild: Publish a transparency log with an external witness for releases and admin actions
- Wild: Add a duress login that opens a decoy workspace

## C43 Simplicity guardrails: compose lint, dependency cap, signed releases
value 3 · effort S · risk low · mark **build** · round B
Grounding: Compose lint test in B.
Ideas (9):
- Hide advanced features behind feature flags that default to off
- Cut the feature set into three named tiers
- Use the same URL scheme for everything
- Delete rarely used knobs by choosing defaults
- Enforce an executable "simpler than GitLab" budget in CI
- Adopt a signed, reproducible release with a one-line verify
- Ship a signed, pinned release with SBOM and dependency policy
- Ship a "defaults ledger" that justifies every setting that exists
- Cap the codebase's direct dependency count and fail the build above it

## C44 Conformance/e2e with real git clients, post-deploy smoke, fault injection
value 4 · effort M · risk low · mark **build** · round A
Grounding: scripts/e2e.sh in T-0001; fault injection later.
Ideas (9):
- Wild: Run a chaos mode that randomly kills things
- Dogfood: Gitter hosts and builds itself from milestone one
- Ship a conformance suite that runs real git clients against every release
- Run a fault-injection test suite in CI (kill, ENOSPC, slow disk, clock jump)
- Fuzz the instance's own git and HTTP endpoints nightly
- Add a git-aware crash-consistency test harness
- Wild: Canary self-test user that continuously pushes, opens PRs and runs CI on the instance
- Make the e2e suite a permanent post-deploy smoke test
- Generate the README's quickstart from the e2e script so docs cannot drift

## C45 Git performance: commit-graph maintenance, changed-path bloom, cat-file batch, archive cache
value 4 · effort M · risk low · mark **build** · round E
Grounding: New.
Ideas (10):
- Keep warm `git cat-file --batch` processes per repo
- Run background maintenance: commit-graph, multi-pack-index and scheduled repack
- Wild: Hot-repo packfile cache that serves popular clones from RAM or tmpfs
- Serve clones from a negotiated-pack cache
- Build a dependency-aware incremental diff and blame cache
- Serve partial clone and bundle-uri
- Serve smart-HTTP fetches from a have/want negotiation cache
- Make runner checkouts shallow-by-default with server-side partial-clone filters
- Generate zip/tar archives incrementally and cache by tree OID
- Make per-file "last commit" instant with changed-path Bloom filters

## C46 Cross-repo (fork) PRs, stacked PRs
value 4 · effort M · risk low · mark **build** · round D
Grounding: Forks in T-0001 extended; fork PRs new.
Ideas (2):
- Support native stacked PRs
- Provide a "fork a PR into my own namespace" contribution path without write access

## C47 Demo mode: seed a sample org with history and a red-then-green build
value 3 · effort S · risk low · mark **build** · round E
Grounding: Also feeds e2e.
Ideas (3):
- Wild: Instance-in-a-box demo mode
- Offer a hosted-free "try before install" public sandbox with expiring accounts
- Wild: Make a "demo me" button that seeds a believable sample org with history and a red-then-green build

## C48 Phone-friendly review
value 3 · effort S · risk low · mark **build** · round A
Grounding: Responsive layout is part of the UI.
Ideas (1):
- Offer a phone-friendly review and approve mode
