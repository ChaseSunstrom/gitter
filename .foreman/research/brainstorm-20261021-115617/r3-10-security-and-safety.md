# Brainstorm — lens: security and safety

- **Close the first-run admin claim race** — SECURITY — value 5 — effort S — risk low — category: bootstrap
  Why: the pack plans zero-config first boot on a public site, so whoever reaches the setup page first could become admin. Add a one-time setup token printed to the container log, required to create the first admin, and expiring after use.
  Done when: a fresh instance refuses first-admin creation without the logged token, and a test shows the token cannot be reused after setup.

- **Partition CI caches by trust tier** — SECURITY — value 5 — effort M — risk med — category: CI supply chain
  Why: the pack plans a cache/artifact store and input-hash task caching, but only gates fork CI, not what a fork can write into. Key caches by (repo, trust tier, protected-ref flag) so an untrusted PR can read but never write caches that default-branch or release jobs restore.
  Done when: an e2e test has a fork PR job write a poisoned cache entry, and a default-branch job with the same key does not receive it.

- **Harden server-side git against hostile repositories** — SECURITY — value 5 — effort M — risk med — category: git safety
  Why: git CLI is the storage engine, and merge-tree, web-editor commits and blob rendering all touch attacker-controlled content. Run every git call under a fixed config (`receive.fsckObjects`, protocol allowlist, no filters or textconv, no submodule fetch), and reject symlink and `..` path traversal in editor, raw and blob handlers.
  Done when: a corpus of malicious repos (symlink escape, crafted `.gitmodules`, `.gitattributes` filter, malformed objects) is pushed and browsed in a test suite, and none reads or writes outside its repo dir.

- **Give tokens a scannable format and auto-revoke leaks** — SECURITY — value 4 — effort S — risk low — category: credentials
  Why: PATs exist and secret scanning on push is planned, but nothing ties them together. Issue tokens as `gtr_<id>_<checksum>` so scanners can detect them with few false positives, and when one appears in a pushed blob, a CI log or an issue body, revoke it and notify the owner.
  Done when: pushing a file containing a live token revokes it within seconds and creates an inbox notification, and an offline checksum validator rejects typos.

- **Scope runner identities with mTLS and a blast-radius view** — SECURITY — value 4 — effort M — risk med — category: runners
  Why: runners register by token and the bundled runner auto-registers, but a stolen token or compromised runner has unclear reach. Give each runner a short-lived certificate bound to repo, org or tier, and add a "revoke" button that first lists the jobs and secrets that runner touched.
  Done when: a revoked runner is rejected on its next heartbeat, and the revoke dialog lists the jobs it ran in the last 30 days.

- **Redact credentials centrally in logs, errors and metrics** — SECURITY — value 4 — effort S — risk low — category: leak prevention
  Why: clone URLs with embedded tokens, `Authorization` headers and panic payloads routinely end up in access logs and the planned black-box dump. Add one tracing layer that strips token patterns and session cookies before anything is written.
  Done when: a test sends requests with tokens in the URL, header and body, and a grep of all log output and the crash dump finds none of them.

- **Add an incident lockdown switch** — FEATURE — value 4 — effort S — risk low — category: incident response
  Why: planned controls are per-feature (quotas, read-only on disk-full), but no single response exists for an active abuse wave. Provide `gitter lockdown on` and an admin toggle that freezes signups, new CI, forks and pushes from unverified accounts while reads continue.
  Done when: with lockdown on, a new signup and an unverified push are rejected with a clear message, and a normal clone still works.

- **Warn on bidi, invisible and confusable Unicode** — SECURITY — value 3 — effort M — risk low — category: impersonation
  Why: this is a public multi-user site with strict name regexes already planned, but there is no defence against lookalike names or Trojan Source in diffs. Add a confusable-skeleton uniqueness check on usernames and repo names, and a visible banner on diffs and files containing bidi controls or zero-width characters.
  Done when: registering `paypa1`-style or Cyrillic lookalikes of an existing name is refused, and a diff with a bidi override shows a warning.

- **Sign outbound webhooks with timestamps and key rotation** — SECURITY — value 3 — effort S — risk low — category: integrations
  Why: SSRF blocking is covered, but receivers have no way to authenticate deliveries or reject replays. Add HMAC-SHA256 signatures over timestamp and body, with two active secrets per hook to allow rotation.
  Done when: a reference verifier accepts a fresh delivery, rejects one older than 5 minutes, and accepts either secret during a rotation window.

- **Ship a signed, pinned release with SBOM and dependency policy** — SECURITY — value 3 — effort M — risk low — category: supply chain
  Why: the compose file is the product, so a tampered image compromises everything. Sign the image with cosign, publish an SBOM, pin the compose file to a digest, enforce `cargo deny` and `cargo audit` in CI, and have `gitter doctor` verify the running digest.
  Done when: `gitter doctor` reports signature verification passing, and CI fails on a dependency with a known advisory.

- **Wild: Plant honeypot repos and endpoints that auto-ban scanners** — SECURITY — value 3 — effort S — risk low — category: deception
  Why: wild; assumption: public instances attract automated scrapers and vulnerability scanners. Expose invisible tripwire paths and a decoy "admin-backup" repo that no legitimate client would request, and temporarily ban any IP that touches them.
  Done when: a request to a tripwire path bans the source IP for the configured window and records an audit entry.

- **Wild: Add a dead-man's switch that tightens an unattended instance** — SECURITY — value 3 — effort S — risk low — category: neglect safety
  Why: wild; assumption: many self-hosted instances are set up once and forgotten while unpatched. If no admin has logged in for N days, or the running version is far behind the latest, auto-disable open signups and public CI and show an admin banner.
  Done when: with the clock advanced past the threshold in a test, signups and public CI are off and re-enabled by an admin login.

- **Wild: Support client-side encrypted private repos** — FEATURE — value 3 — effort L — risk high — category: zero-knowledge
  Why: wild; assumption: some owners want a host that cannot read their code. Ship a `git-remote-gitter-crypt` helper that encrypts objects before push, so the server stores only ciphertext and the web UI shows an "encrypted, preview unavailable" state.
  Done when: a repo pushed via the helper clones and decrypts correctly on a second machine, and a grep of the server's repo directory finds no plaintext source strings.

- **Wild: Require two admins to approve destructive instance actions** — SECURITY — value 2 — effort M — risk med — category: dual control
  Why: wild; assumption: larger deployments have multiple admins and worry about a single compromised account. Make deleting an org, mass-revoking tokens, restoring a backup and disabling the audit log require a second admin's approval, with a solo-admin override that is explicit.
  Done when: a restore started by one admin stays pending until a second admin approves it, and both actions appear in the audit log.
