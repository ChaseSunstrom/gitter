# Brainstorm — lens: approaches

- **Seal CI secrets to the runner's public key** — SECURITY — value 5 — effort M — risk med — category: privacy
  Why: the pack plans secrets at repo, org and environment scope, but a server that can decrypt them is the weakest link on a public instance. Assumption: runners hold a keypair created at registration.
  Done when: the SQLite file and a full server memory dump contain no plaintext secret, and a job on the registered runner still receives it.

- **Test the compose file on every runtime** — CLEAN — value 4 — effort M — risk low — category: reliability
  Why: runners are planned around the Docker socket, with rootless and Podman "later". The single-file promise breaks on the first unsupported host. Assumption: CI matrix capacity exists.
  Done when: a scripted matrix (Docker, rootless Podman, amd64, arm64) runs `compose up`, push, a green pipeline and `down`, and passes on all targets.

- **Key every cache by viewer permission** — SECURITY — value 5 — effort S — risk med — category: privacy
  Why: the plan has permanent OID ETags, a negotiated-pack cache and pre-rendered static pages. All of them can leak private content across users if the cache key ignores who is asking.
  Done when: a test fetches a private blob as owner, then anonymous and a non-member, through every cache layer, and the latter two get 404 with no body or timing-distinct response.

- **Ship zero-egress by default** — SECURITY — value 4 — effort M — risk low — category: privacy
  Why: the pack stresses self-hosting and local-first, but it also lists webhooks, mirrors, SMTP and ACME. Each one is an outbound path. Assumption: the compose network can enforce a default-deny egress policy.
  Done when: a fresh instance makes no outbound connections except to listed allowlist hosts, `gitter doctor` prints the observed egress log, and a test fails on any third-party origin in the CSP or HTML.

- **Encrypt the data volume at rest** — SECURITY — value 4 — effort M — risk med — category: encryption
  Why: one `/data` volume holds everything, so one stolen disk exposes all repos. Encrypted backups are covered, but the live volume is not. Assumption: SQLCipher or an age-wrapped data key is acceptable, and it needs an unlock step at boot.
  Done when: with the key withheld, the server refuses to start, and `strings` over the raw volume finds no repo content.

- **Minimize logs and IP retention by default** — SECURITY — value 3 — effort S — risk low — category: privacy
  Why: a public multi-user site accumulates IPs and user agents in access logs and the audit log. Assumption: abuse controls need only short-lived raw IPs.
  Done when: raw IPs older than N days (default 7) are hashed or dropped, a settings toggle exists, and a test shows the abuse limiter still works after rotation.

- **Add break-glass admin access to private repos** — FEATURE — value 3 — effort M — risk med — category: privacy
  Why: on a public instance, the admin reading users' private repos is an unstated risk. Assumption: owners can be notified in-app.
  Done when: admin access to a private repo requires a typed reason, writes a tamper-evident audit entry, and shows the owner a notice in their inbox.

- **Show a private-repo access log to owners** — FEATURE — value 3 — effort S — risk low — category: transparency
  Why: no planned feature lets an owner see who cloned or viewed a private repo. The idea reuses the audit log and token scopes already in the plan.
  Done when: the repo settings page lists the last 90 days of clones, views and token uses, with actor and time.

- **Wild: Ingest signed git bundles for air-gapped sync** — FEATURE — value 3 — effort M — risk low — category: offline
  Why: wild. It extends the planned read-only mirror mode with a write path. Verified, signed bundles carried on a USB stick or by email are imported as branches or PRs, so isolated networks stay private. Assumption: bundle signing reuses commit signing keys.
  Done when: `gitter bundle export` on instance A and `import` on instance B, with no network between them, create the same refs, and an unsigned or tampered bundle is rejected.

- **Wild: Offer a built-in Tor onion address** — FEATURE — value 2 — effort M — risk med — category: privacy
  Why: wild. Optional onion service support makes the forge reachable without a public IP or domain, which pairs with the "private by default, publish later" repo lifecycle. Assumption: an embedded Rust Tor client (Arti) is usable.
  Done when: enabling the setting prints a `.onion` URL, and a Tor-only client can clone and push over it.

- **Wild: Browse any remote git URL without importing** — FEATURE — value 3 — effort M — risk med — category: viewer
  Why: wild. The fast UI works as a viewer over a throwaway blobless partial clone of any public URL, discarded on a timer. It gives users a reason to try Gitter before migrating, and it keeps the viewer mode distinct from permanent mirrors. Assumption: SSRF guards from the webhook design apply.
  Done when: pasting a public GitHub URL opens a browsable tree and log in under 3 seconds, and the temp clone is deleted after its TTL.

- **Run the migration parity check on real data** — FIX — value 3 — effort M — risk low — category: migration
  Why: the plan includes GitHub and GitLab import plus a "migrate in 10 minutes" wizard, but nothing verifies that the result matches. Assumption: import can also produce a machine-readable diff.
  Done when: after an import, a report lists counts and checksums for refs, issues, PRs and comments against the source, and any mismatch is flagged.

- **Scrub metadata from uploads and unfurl cards** — SECURITY — value 3 — effort S — risk low — category: privacy
  Why: avatars, issue attachments and release assets can carry EXIF location data. Share cards for private repos are the other leak path. Assumption: image handling is in-process.
  Done when: uploaded JPEGs have EXIF stripped, and a private repo URL returns a generic unfurl card to anonymous fetchers.
