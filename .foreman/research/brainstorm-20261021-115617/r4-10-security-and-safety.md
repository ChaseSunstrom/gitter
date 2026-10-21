# Brainstorm — lens: security and safety

- **Isolate fork object stores and hide internal refs** — SECURITY — value 5 — effort M — risk med — category: data isolation
  Why: assumption: forks share objects through alternates and PRs live under `refs/pull/*`. That setup is how cross-fork and deleted-fork data leaks happen on other forges, and the pack lists no mitigation.
  Done when: an e2e test fetches a private fork's commit by SHA through the public parent and a deleted fork's commit through a sibling, and both fail. `allowAnySHA1InWant` is off and internal refs are not advertised.

- **Add a purge-sensitive-data workflow** — FEATURE — value 5 — effort M — risk med — category: privacy
  Why: the pack has secret scanning, trash/undo and force-push recovery. Nothing removes a leaked secret for good from PR refs, archive caches, the OID-keyed page cache or packs, and the trash/undo feature would otherwise keep it alive.
  Done when: purging a blob makes it return 404 on every surface (raw, archive, diff, cache, `git fetch`). It also drops out of the next backup, and the action is audit-logged.

- **Resolve every user-supplied path beneath a root with symlink-safe opens** — SECURITY — value 4 — effort S — risk low — category: path safety
  Why: the pack plans a web editor, drag-folder commits, artifact uploads, archives and a task cache, and each is a symlink or `..` traversal surface. The plan only covers strict name regexes.
  Done when: fuzz tests with symlinks, `..`, absolute paths and Unicode tricks cannot read or write outside the repo or artifact root. File access uses `openat2` with `RESOLVE_BENEATH` (or an equivalent check).

- **Encrypt stored secrets at rest with a key kept off the data volume** — SECURITY — value 4 — effort M — risk med — category: secrets
  Why: CI secrets, webhook signing keys and SMTP credentials will sit in SQLite and be copied by every backup. The pack's encrypted backups protect the archive, not the live DB or an unencrypted snapshot.
  Done when: a raw dump of `/data` contains no plaintext secret values. The key is supplied by a file, env var or Docker secret, and the CLI rotates it without downtime.

- **Hide private-resource existence and enumeration** — SECURITY — value 4 — effort S — risk low — category: privacy
  Why: a public multi-user site is scraped for usernames, private repo names and emails. The pack adds rate limits but does not mention uniform responses.
  Done when: a private repo, a missing repo and an unauthorized one return identical status, body and timing within a tolerance. Login and signup do not reveal whether an account exists. Public pages show a commit-email alias instead of the real address.

- **Keep a minimal-PII log policy with IP truncation and expiry** — SECURITY — value 4 — effort S — risk low — category: privacy
  Why: the pack's "thin so far" list names privacy, and there are access logs, sessions, audit logs and abuse limits but no retention rules.
  Done when: access and abuse logs store truncated or salted-hash IPs and auto-expire on a setting. `gitter doctor` lists every place personal data is stored, with its retention period.

- **Run a pre-publish privacy lint when a repo goes public** — FEATURE — value 4 — effort M — risk low — category: privacy
  Why: the pack plans a "private by default, publish later" lifecycle and secret scanning on pushes, but nothing checks history for internal hostnames, personal emails, image GPS data or large files at the moment of publishing.
  Done when: flipping a repo to public shows a report built from a seeded test repo. The report lists each finding with the commit and file, and the user has to acknowledge it or fix it before publishing.

- **Pin CI images by digest and enforce an image allowlist** — SECURITY — value 4 — effort M — risk low — category: supply chain
  Why: the pack covers sandboxes, OIDC, egress proxies and caches but not what code the runner pulls in as a base image. A mutable tag lets an upstream compromise reach every job.
  Done when: an admin policy can require digest-pinned or allowlisted images. A job using an unpinned tag fails with a one-line fix, and the lint offers to rewrite the tag to a digest.

- **Require step-up re-authentication for dangerous actions** — SECURITY — value 3 — effort S — risk low — category: authentication
  Why: the pack has passkeys/2FA and two-admin approval, but a stolen session cookie can still delete repos or mint tokens.
  Done when: token creation, repo delete, transfer and admin changes need a fresh passkey or password check within the last 5 minutes. An automated test shows a stale session gets a 401 step-up challenge.

- **Add a cooldown quarantine for newly published packages in the pull-through proxy** — SECURITY — value 3 — effort S — risk low — category: supply chain
  Why: the pack proposes an npm/crates/PyPI proxy. Typosquats and hijacked releases are typically caught within hours, so a delay blocks most of them.
  Done when: a package version younger than the configured cooldown is refused unless explicitly allowed. The refusal says when it becomes available.

- **Wild: Offer a built-in Tor onion service profile** — FEATURE — value 3 — effort M — risk med — category: privacy
  Why: wild; assumption: some self-hosters and their contributors want access without IP exposure. A Compose profile adds a sidecar and the instance serves the same site, with the onion address shown in settings.
  Done when: `docker compose --profile onion up` prints a working `.onion` URL. A Tor client can browse, clone and push through it with no clearnet hostname leaking in the pages.

- **Wild: Publish a transparency log with an external witness for releases and admin actions** — SECURITY — value 3 — effort L — risk med — category: audit
  Why: wild; the pack has a tamper-evident audit log, but an admin with shell access can rewrite it. Periodically publishing the log head to a witness (a public git repo or a peer instance) makes tampering detectable.
  Done when: the audit-log head hash is pushed on a schedule to a configured witness. `gitter verify-log` detects a test edit to an old entry.

- **Wild: Add a duress login that opens a decoy workspace** — SECURITY — value 2 — effort M — risk med — category: personal safety
  Why: wild; assumption: a user forced to unlock their account on a phone or laptop could enter a second passphrase that shows only a sanitized view. Private repos stay hidden and an alert is sent quietly.
  Done when: logging in with the duress phrase shows only pre-chosen repos. The real private repos return 404 in that session, and the owner is notified out of band.
