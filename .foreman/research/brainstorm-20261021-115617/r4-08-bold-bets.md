# Brainstorm — lens: bold bets

- **Ship a local-only privacy mode that proves nothing leaves the box** — FEATURE — value 5 — effort M — risk low — category: privacy
  Why: the pack flags privacy as thin; the ideas list has webhooks, SMTP, import and update checks but no outbound-traffic story. Assumption: self-hosters want verifiable egress control.
  Done when: with `GITTER_OFFLINE=1`, an e2e test with all outbound network blocked passes core flows (push, PR, CI), and `gitter doctor` lists every outbound destination as "none".

- **Make the instance a federated mesh of single-file "pods"** — FEATURE — value 4 — effort L — risk high — category: federation
  Why: the pack has federation only as a wild idea and no concrete protocol. Assumption: PR-across-servers needs signed identity plus ref-based transport.
  Done when: a PR opened from instance A against a repo on instance B appears in B's PR list with A's signed author identity, and merges.

- **Add a first-class `gitter.toml` reconciler with dry-run diff** — FEATURE — value 4 — effort M — risk med — category: ops
  Why: the pack has settings in the UI, GitOps config, and a single `GITTER_URL`, but nothing reconciling declared state against live state. Assumption: operators want review before change.
  Done when: `gitter apply --dry-run` prints a diff of users, repos, runners and settings versus live state, and `gitter apply` converges it idempotently (a second run changes nothing).

- **Offer per-repo encrypted-at-rest storage with user-held keys** — SECURITY — value 4 — effort L — risk high — category: privacy
  Why: privacy is thin; the only encryption idea is client-side encrypted repos (wild) and encrypted backups. Assumption: private repos on shared hosts need protection from the host admin.
  Done when: with the key withheld, an admin reading the repo directory and DB sees no plaintext blobs or file names, and an authorized push, clone and web view still work.

- **Strip telemetry and metadata from every outbound surface** — SECURITY — value 3 — effort S — risk low — category: privacy
  Why: assumption: avatars, link unfurls, fonts, and CDN assets are potential leaks. The pack already vendors its JS, which makes this cheap to enforce.
  Done when: a Playwright run records zero requests to a foreign origin across all main pages, and CI fails if one is added.

- **Add runner "trust receipts" for every CI job** — SECURITY — value 4 — effort M — risk med — category: CI trust
  Why: the pack has sandboxing, attestation badges and OIDC tokens, but no per-job record of exactly what ran. Assumption: users want proof after the fact.
  Done when: each finished job exposes a signed receipt (image digest, commit, network egress log, resource caps) that `gitter verify-job <id>` validates offline.

- **Build a "migrate off and back" round-trip test suite** — CLEAN — value 3 — effort M — risk low — category: portability
  Why: the pack has an exit guarantee and a plain-folder export, but no check that exported data really round-trips. Assumption: lock-in fears block adoption.
  Done when: export, then import into a fresh instance, yields identical repos, issues, PRs and CI history, checked by a diff script in CI.

- **Provide a single-binary "gitter appliance" image for tiny hardware** — PERFORMANCE — value 3 — effort M — risk med — category: footprint
  Why: the pack stresses low RAM and a published number, but not a hardware target. Assumption: a Raspberry Pi or 512MB VPS is the real proof of "simpler than GitLab".
  Done when: a published ARM64 image runs on a 512MB device serving browse, clone and one CI job, with measured RSS under a stated cap.

- **Wild: Let the instance run entirely from a USB stick as a portable forge** — FEATURE — value 3 — effort M — risk low — category: portability
  Why: wild. Builds on the one-volume `/data` idea: plug the volume into any machine, run one command, and the forge is back with its history.
  Done when: copying `/data` to a fresh host and running `gitter serve --data ./data` restores the full instance with no other setup.

- **Wild: Add an "air-gapped bundle" that carries repos, runners and images across a gap** — FEATURE — value 4 — effort L — risk med — category: privacy
  Why: wild. Assumption: regulated users cannot pull images or packages online. Pairs with the pull-through proxy idea by exporting its cache.
  Done when: `gitter bundle create` produces one file that, imported on a network-isolated instance, lets a CI job build with all dependencies and images present.

- **Wild: Let users revoke a past clone by rotating a repo's history-encryption epoch** — SECURITY — value 2 — effort L — risk high — category: privacy
  Why: wild. Assumption: a leaked or ex-collaborator clone should stop receiving updates, with fetches gated by epoch keys.
  Done when: after rotation, a collaborator's old credential cannot fetch new commits, while current members can.

- **Wild: Run a "pre-mortem" command that simulates your instance's worst day** — FEATURE — value 3 — effort M — risk low — category: reliability
  Why: wild. It combines the chaos mode, fault injection, backup verification and capacity forecast ideas into one report. Assumption: operators trust what they have rehearsed.
  Done when: `gitter premortem` runs disk-full, kill and restore drills against a copy of the data and outputs a pass/fail report with fixes.

- **Add a compose-file linter that rejects unsafe exposure before boot** — SECURITY — value 3 — effort S — risk low — category: simplicity
  Why: the pack wants one hardened compose file and warns on public exposure, but nothing validates user edits to it. Assumption: people will edit the compose file.
  Done when: `gitter doctor --compose docker-compose.yml` flags a mounted Docker socket, privileged mode or an unpinned image, and exits non-zero.
