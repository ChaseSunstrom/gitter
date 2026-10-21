# Brainstorm — lens: capability map

- **Add symbol-aware code navigation** — FEATURE — value 4 — effort L — risk med — category: code browsing
  Why: assumption: the pack lists AST-aware diffs and trigram search but no go-to-definition or find-references. GitHub and Sourcegraph have both, and tree-sitter tags run on push could feed the existing precomputed browse index.
  Done when: clicking an identifier in a Rust or JS blob opens a definition/references panel in under 200 ms, using an index built on push.

- **Serve partial clone and bundle-uri** — PERFORMANCE — value 4 — effort M — risk low — category: clone speed
  Why: "wayyy faster" is the headline ask, and the pack covers pack caches but not `--filter=blob:none`, protocol v2 or `bundle-uri` for big repos. Assumption: git http-backend can be configured for these.
  Done when: `git clone --filter=blob:none` and a bundle-uri clone both succeed against the instance, and the e2e script asserts the bundle is used.

- **Add Web Push and an installable PWA** — FEATURE — value 4 — effort M — risk low — category: mobile
  Why: devices are listed as thin. The pack has a notification inbox and a "push-to-green" toast, but only for the terminal and the browser tab. Web Push with VAPID keys needs no email and no third party.
  Done when: an installed PWA on a phone receives a notification for a failed CI run while the tab is closed, and the app shell loads offline.

- **Expose an MCP server and agent-scoped tokens** — FEATURE — value 4 — effort M — risk med — category: agents
  Why: agents are listed as thin and the pack has an AI agent wild idea. It has no stable tool surface for external agents. Read-only repo/issue/CI-log tools plus tokens limited to specific repos and verbs would make that safe.
  Done when: an MCP client can list failing runs and fetch a log with a token scoped to one repo, and the same token is refused on another repo.

- **Enforce push rulesets** — FEATURE — value 3 — effort M — risk low — category: policy
  Why: the pack has branch protection and secret scanning but no declarative rules for max file size, forbidden paths, commit-message patterns or required sign-off. Gitea and GitHub rulesets have these, and pre-receive quarantine already exists in the plan.
  Done when: a push containing a file over the configured limit is rejected with a one-line explanation, and a rules file in the repo is applied.

- **Support issue graph features** — FEATURE — value 3 — effort M — risk low — category: tracking
  Why: the pack has boards, milestones and templates only. A complete tracker also needs sub-issues, blocked-by links, reactions, `#123` and `owner/repo#12` cross-references with backlinks, and saved queries.
  Done when: a mention of #N in a commit or comment creates a backlink on issue N, and a blocked-by issue shows as blocked on a board.

- **Ship native runners for macOS, Windows and ARM** — FEATURE — value 3 — effort M — risk med — category: runners
  Why: the pack's runner is Docker-first, with bubblewrap and a Linux sandbox. Many real projects need macOS or Windows builds, so a plain-binary runner mode with OS and arch labels fills that gap. Assumption: the runner is a Rust binary and cross-compiles.
  Done when: a job with `runs-on: macos-arm64` runs on a native runner binary registered with that label.

- **Offer account data export and deletion** — FEATURE — value 3 — effort S — risk low — category: compliance
  Why: assumption: this is a public multi-user site, so GDPR-style obligations apply, and none of the pack's ideas cover them. A single ZIP export plus a delete flow with a grace period is the minimum.
  Done when: a user can download an archive of their repos, issues and tokens metadata, and a deleted account is fully purged after the grace period.

- **Generate SBOMs and detect licenses** — FEATURE — value 3 — effort M — risk low — category: supply chain
  Why: the pack covers lockfile vulnerability scans and attestation badges but not license detection or SBOM output. Both are expected on release pages.
  Done when: a repo page shows its SPDX license, and a release can attach a CycloneDX SBOM produced from the lockfiles.

- **Wild: Offer an SSH TUI** — FEATURE — value 3 — effort M — risk med — category: devices
  Why: wild. The pack already plans SSH clone. `ssh gitter.host` could drop into a terminal UI for notifications, PRs and CI logs, so there is nothing to install, unlike the `gh`-style CLI idea.
  Done when: an SSH session with a registered key shows an interactive list of the user's open PRs and streams a live CI log.

- **Wild: Add a dev-container workspace on the runner pool** — FEATURE — value 4 — effort L — risk high — category: web IDE
  Why: wild. The pack has a web editor and warm workspaces. Reusing the runner sandbox to open a devcontainer.json environment with a terminal in the browser gives a self-hosted Codespaces with no new infrastructure.
  Done when: clicking "Open workspace" on a repo with a devcontainer.json gives a working shell in the browser within 10 seconds, and it is destroyed on idle timeout.

- **Wild: Ingest patches by email** — FEATURE — value 2 — effort M — risk med — category: workflows
  Why: wild. Sourcehut-style `git send-email` intake would turn a mailed patch series into a PR, with review replies returned by mail. That serves contributors with no account and works in low-bandwidth settings.
  Done when: a patch series sent to a repo's address appears as a PR with one commit per patch.

- **Wild: Add a snapshot of the instance as a shareable "template"** — FEATURE — value 3 — effort M — risk low — category: simplicity
  Why: wild. The pack has demo mode and the config-as-git-repo idea. Exporting settings and sample repos as a seed bundle that a fresh compose can load via one env var lets teams clone a known-good setup, which suits a design built on "one volume, few env vars".
  Done when: a fresh instance started with the bundle URL has the same org structure, runners labels and branch rulesets as the source.

- **Add repo rename and transfer with permanent redirects** — FEATURE — value 3 — effort S — risk low — category: repo lifecycle
  Why: assumption: the plan uses DB-id paths, which makes this cheap. The pack has nothing about renames, and a missing redirect breaks clone URLs and links.
  Done when: after renaming a repo, the old web URL and the old `git clone` URL both still work and return a redirect or a successful fetch.
