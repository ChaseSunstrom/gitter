# Brainstorm — lens: capability map

- **Prove zero phone-home with an egress audit** — FEATURE — value 4 — effort S — risk low — category: privacy
  Why: privacy is the thin area, and the pack plans an egress allowlist for CI but nothing for the server itself. Assumption: self-hosters of a public forge want this proof.
  Done when: `gitter doctor --egress` lists every outbound destination the server has contacted. On a default install the list is empty, and a test fails if new code makes an unlisted call.

- **Hide author emails with noreply addresses and a push warning** — FEATURE — value 4 — effort M — risk low — category: privacy
  Why: assumption: public-site users leak personal emails through commit metadata. The pack has secret scanning but no identity-leak check.
  Done when: web edits and merges use `user@noreply.<host>`. A push containing a non-noreply author email on a public repo returns a warning naming the commit, and the setting is per-user.

- **Show cookieless repo traffic stats** — FEATURE — value 3 — effort M — risk low — category: analytics
  Why: assumption: owners want views, clones and referrers, and GitHub has this. Nothing in the pack covers it.
  Done when: the Insights tab shows daily views and clones per repo from aggregated counters. No IPs or cookies are stored, and a retention setting purges old rows.

- **Trigger pipelines by path filters for monorepos** — FEATURE — value 4 — effort M — risk low — category: CI
  Why: the pack has matrix, caching and input-hash skipping, but not changed-path triggers.
  Done when: a job with `paths: [services/api/**]` runs only when a push touches matching files. The run page shows skipped jobs with the reason.

- **Parse JUnit and coverage reports into PR annotations** — FEATURE — value 4 — effort M — risk low — category: CI
  Why: the pack covers flaky-test detection and failure digests, but not ingesting structured test or coverage output. Assumption: pipelines can upload report files as artifacts.
  Done when: uploading `junit.xml` and `lcov` shows per-test results on the run page and a diff-coverage percentage on the PR. A coverage badge URL also works.

- **Share pipeline components across repos** — FEATURE — value 3 — effort M — risk med — category: CI
  Why: the pack has pipeline templates and a translator, but no reusable include or registry. Assumption: pinning by commit SHA is the safe default.
  Done when: `uses: org/ci-lib/lint@<sha>` resolves inside the sandbox, the pin is enforced, and unpinned refs trigger a lint warning.

- **Push-mirror to external remotes** — FEATURE — value 3 — effort S — risk low — category: mirroring
  Why: the pack has pull mirrors via import by URL and an exit guarantee, but no outbound mirror for a GitHub backup or redundancy.
  Done when: a repo setting with a stored deploy credential pushes every ref update to the remote within 30s. Failures show in the UI and retry with backoff.

- **Add deploy keys and repo-scoped read-only tokens** — FEATURE — value 4 — effort S — risk low — category: access
  Why: the pack has scoped tokens and SSH clone, but not per-repo machine credentials for servers and CI consumers.
  Done when: a key added to one repo can clone it read-only, and push and other repos are rejected. Its last-used time is visible.

- **Add rich file viewers** — FEATURE — value 3 — effort M — risk low — category: viewer
  Why: the pack has AST diffs and syntax highlighting, but no image slider diff, CSV tables, notebooks, PDF or SVG-safe preview. SVG must stay sandboxed under the planned CSP.
  Done when: a PNG change shows swipe and onion-skin comparison, `.csv` renders as a sortable table, and `.ipynb` renders without executing anything.

- **Add snippets (gists)** — FEATURE — value 2 — effort S — risk low — category: content
  Why: assumption: a GitHub-like site is expected to have them, and they are cheap as tiny git repos on the existing storage.
  Done when: a user creates a multi-file snippet that is clonable and forkable, and can mark it secret-link or public.

- **Revert and cherry-pick from the web** — FEATURE — value 3 — effort M — risk med — category: git workflow
  Why: the pack covers the web editor and merge queue, but not undoing or porting a commit by clicking. It would use the existing `merge-tree` path.
  Done when: a "Revert" button on a merged PR opens a new PR with the inverse commit. Cherry-pick to a chosen branch works, and conflicts are shown instead of failing silently.

- **Wild: Mirror encrypted backups to a friend's instance** — FEATURE — value 3 — effort L — risk med — category: privacy
  Why: wild. The pack has encrypted backups and federation, but no offsite exchange. Two admins swap public keys and each stores the other's ciphertext without being able to read it.
  Done when: a nightly encrypted snapshot lands on the peer. The peer admin cannot decrypt it, and a restore on a fresh box succeeds from the peer copy.

- **Wild: Publish a machine-signed warrant canary page** — FEATURE — value 2 — effort S — risk low — category: privacy
  Why: wild. A public forge's operator can sign a dated statement from the instance key saying no legal requests were received. The statement goes stale visibly if the operator stops renewing it.
  Done when: `/.well-known/canary` shows a signed statement. If it is older than the configured interval it shows a "not renewed" banner, and the signature verifies with one command.

- **Wild: Build an air-gapped install bundle** — FEATURE — value 3 — effort M — risk low — category: deploy
  Why: wild. The pack has signed releases and offline vulnerability databases but no way to install without internet. A single tarball would hold the image, runner images, the vulnerability DB and a compose file.
  Done when: `docker load` plus `docker compose up` works on a host with networking disabled, and a first CI job runs with a bundled base image.

- **Wild: Carry pre-push CI results in a signed git note** — FEATURE — value 2 — effort M — risk med — category: CI
  Why: wild. The pack has `gitter ci run` locally and attestations from pushers' machines. Combining them lets the local result travel with the commit and be verified by the server to skip re-running.
  Done when: a locally green run attaches a signed note on push. The server verifies it and marks the check as "verified locally". A tampered note is rejected and the job runs normally.
