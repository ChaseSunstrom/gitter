# Brainstorm — lens: security and safety

- **Mint short-lived OIDC identity tokens per CI job** — FEATURE — value 4 — effort M — risk med — category: CI security
  Why: assumption: deploy jobs will need cloud credentials, and the pack only covers static secrets and masking. The instance would act as an OIDC issuer, so jobs get signed tokens carrying repo, ref and run claims, and no long-lived keys sit in settings.
  Done when: a job fetches a token whose JWT verifies against `/.well-known/jwks.json` and carries correct `repo`, `ref` and `run_id` claims, and the token is rejected after the job ends.

- **Require approval for permission changes in workflow files** — SECURITY — value 4 — effort M — risk low — category: CI security
  Why: the pack plans "gate CI on untrusted pushes" but not changes to what a pipeline may do. Workflows declare permissions (network, secrets, privileged mounts), and any diff that widens them pauses the run until a maintainer approves.
  Done when: a PR that adds `network: any` to a job leaves it in "awaiting approval" and it runs only after a maintainer approves.

- **Run jobs on single-use just-in-time runners** — SECURITY — value 5 — effort M — risk med — category: runner trust
  Why: the pack has auto-registration and lease heartbeats but nothing on runner compromise. Each runner takes exactly one job with a one-time credential, then is destroyed, so a malicious job cannot persist or poison a cache for the next one.
  Done when: after a job finishes, its runner credential is rejected on any later poll and no container or workspace from it remains.

- **Route CI egress through an allowlist proxy with a connection log** — SECURITY — value 4 — effort M — risk med — category: network
  Why: "no network by default" exists, but real builds need package registries. A built-in proxy allows named domains only and records every connection in the run page, which makes exfiltration visible.
  Done when: a job can reach an allowlisted registry, a request to a non-listed host fails, and both appear in the run's network log.

- **Verify signed commits and enforce signing per branch** — FEATURE — value 4 — effort M — risk low — category: supply chain
  Why: assumption: the planned branch protection has no identity-of-author check. Verify SSH and GPG signatures against keys registered on the account, show a "Verified" badge, and let branch rules require it.
  Done when: a push of an unsigned commit to a branch with "require signed" is rejected, and a commit signed with a registered SSH key shows as verified.

- **Add private security advisories with a temporary private fork** — FEATURE — value 3 — effort L — risk med — category: disclosure
  Why: a public multi-user host needs a way to report vulnerabilities without opening a public issue. Support a `SECURITY.md` contact, a private report form, a private collaboration space, and a coordinated publish step.
  Done when: a non-collaborator submits a report visible only to maintainers, and publishing it makes it public with the fix linked.

- **Scan lockfiles against an offline vulnerability database** — FEATURE — value 3 — effort M — risk low — category: supply chain
  Why: assumption: all-features parity implies dependency alerts. Using a bundled OSV snapshot refreshed on a schedule keeps the single-binary promise and sends no repo data out.
  Done when: pushing a `Cargo.lock` with a known-vulnerable crate version raises an alert on the repo, with no outbound request at scan time.

- **Provide a trash and undo for deletes and force-pushes** — FEATURE — value 4 — effort M — risk low — category: recoverability
  Why: assumption: a public site will see accidental and malicious destruction. Soft-delete repos for 30 days, and keep a ref-update journal so a deleted or force-overwritten branch can be restored in the UI.
  Done when: a deleted repo is restored intact from the trash, and a force-pushed branch is reset to its prior tip with one click.

- **Add a report-and-moderate workflow with a blob hash blocklist** — FEATURE — value 3 — effort M — risk low — category: moderation
  Why: the pack covers signup friction and quarantine but gives admins no queue for abusive content. Add a report button, an admin queue, takedown with a tombstone, and a blocklist of content hashes rejected on push.
  Done when: a blocked blob hash causes a push containing it to fail, and a reported repo appears in the admin queue and can be disabled.

- **Detect transformed secret leaks in CI logs** — SECURITY — value 3 — effort S — risk low — category: CI security
  Why: the pack plans log masking, which exact-match masking misses. Also mask base64, URL-encoded, reversed and per-line-split forms of each secret.
  Done when: a job that echoes a secret in base64 produces a log with that string redacted.

- **Manage sessions and alert on suspicious logins** — FEATURE — value 3 — effort S — risk low — category: account safety
  Why: the pack plans passkeys and scoped tokens but no session list. Show active sessions and tokens with last-used IP, allow revocation, and notify on a login from a new device or country.
  Done when: revoking a session in the UI makes its next request return 401, and a login from a new device creates a notification.

- **Plant canary secrets in CI to catch exfiltration** — SECURITY — value 3 — effort S — risk low — category: detection — wild
  Why: assumption: fork and PR jobs are the main attack path. Each untrusted job gets a fake credential that alerts if it is used or shows up in egress, which exposes malicious PRs without needing to read the code.
  Done when: a test job that sends the canary to an external host triggers an alert and the run is auto-cancelled.

- **Fuzz the instance's own git and HTTP endpoints nightly** — SECURITY — value 3 — effort M — risk low — category: self-testing — wild
  Why: `gitter doctor` and a self-pentest are planned, but a continuous fuzzer for smart-HTTP, pack parsing and path handling is not. It would run as a built-in scheduled CI pipeline on the instance itself and open an issue on any crash.
  Done when: the nightly job runs against a staging copy and a seeded crash bug is filed as an issue with a reproducer.

- **Restore the whole instance to a point in time after an incident** — FEATURE — value 3 — effort L — risk high — category: forensics — wild
  Why: backup and Litestream-style replication are planned but only as restores of the latest state. Combining the WAL stream with the ref journal would let an admin roll the instance back to just before a compromise and export what changed.
  Done when: restoring to a chosen timestamp yields DB and repo state matching a snapshot taken at that time, verified by checksum.
