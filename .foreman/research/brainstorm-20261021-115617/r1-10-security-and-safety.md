# Brainstorm — lens: security and safety

- **Sandbox CI jobs with rootless, no-socket runners** — SECURITY — value 5 — effort L — risk high — category: CI isolation
  Why: T-0002 plans docker-based runners on a public multi-user site, and the usual shortcut of mounting `/var/run/docker.sock` gives any pusher host root. Assumption: the compose file will want to run runners itself. Use a separate runner container with rootless Podman or sysbox, or gVisor, with no socket mount. Jobs get dropped caps, no-new-privileges, CPU/mem/pids/time limits, and a default-deny network with an opt-in allowlist.
  Done when: a CI job that tries `docker ps`, mounting `/`, reaching the metadata IP, or forking 10k processes fails, and an e2e test asserts each failure.

- **Gate CI on untrusted pushes and forks** — SECURITY — value 5 — effort M — risk med — category: CI trust
  Why: PRs from forks can exfiltrate secrets or mine crypto. Assumption: the public site allows forks (the plan lists forks). Fork PR pipelines run with no secrets and need maintainer approval for first-time contributors. Secrets are scoped per branch or environment and masked in logs.
  Done when: a fork PR pipeline cannot read a repo secret (the e2e prints it and sees empty), and the run stays "awaiting approval" until a maintainer approves.

- **Ship a hardened one-file compose by default** — SECURITY — value 4 — effort S — risk low — category: secure defaults
  Why: the user wants one simple docker-compose.yml, so its defaults are the security posture. Use a non-root user, read-only root FS, a tmpfs for tmp, `cap_drop: ALL`, `no-new-privileges`, healthcheck, pinned image digest, and a single data volume. Generate the secret key on first boot. Do not bind to 0.0.0.0 without a TLS note (optionally bundle Caddy for auto-HTTPS as a second service).
  Done when: `docker inspect` shows non-root, read-only, and caps dropped, and the app starts with zero env vars set.

- **Serve user content from an isolated origin** — SECURITY — value 5 — effort M — risk low — category: XSS containment
  Why: the plan serves `raw`, rendered README, and uploads from a SPA with DOMPurify and a strict CSP, but raw HTML or SVG on the main origin steals sessions. Serve raw and attachments from a separate cookieless origin or with `Content-Disposition: attachment` plus `nosniff` and `sandbox` CSP. Configure it with a single `RAW_HOST` setting.
  Done when: an uploaded `<script>` SVG viewed via raw cannot read `document.cookie` or call the API, and a test asserts the headers.

- **Add abuse controls: rate limits, quotas, and signup friction** — SECURITY — value 4 — effort M — risk low — category: abuse
  Why: it is a public site, and the pack already plans push size limits. Add per-IP and per-token token-bucket limits (login, clone, API), per-user repo and storage quotas, optional email/invite-only/admin-approval signup, and an argon2 proof-of-work or CAPTCHA hook for registration.
  Done when: the 101st login attempt in a minute returns 429, and a user over quota gets a clear push rejection.

- **Defend git endpoints against bombs and slow attacks** — SECURITY — value 4 — effort M — risk med — category: git hardening
  Why: `git http-backend` is a CGI-style subprocess and is DoS-prone. Add per-request timeouts, concurrent-process caps, packfile size/object count caps, `transfer.fsckObjects`, `receive.maxInputSize`, a path-traversal-safe worktree for the web editor, and denial of symlinks escaping the repo when serving blobs.
  Done when: pushing a 10 GB pack or a tree with a `../` symlink is rejected quickly, and the server RSS stays flat in a stress test.

- **Scope tokens and add passkeys/2FA** — SECURITY — value 4 — effort M — risk low — category: authN
  Why: the plan has personal access tokens and cookie sessions. Add fine-grained scopes (repo read, write, CI), expiry, hashed-at-rest tokens (store only SHA-256), TOTP and WebAuthn, session list and revoke, and a "sudo mode" before destructive actions.
  Done when: a read-only token's push gets 403, and a DB dump contains no usable token values.

- **Block SSRF in webhooks and mirrors** — SECURITY — value 4 — effort S — risk low — category: SSRF
  Why: assumption: webhooks, mirrors, and CI artifact fetching are likely sooner or later. Resolve DNS and then connect to the pinned IP, deny private, loopback, link-local, and metadata ranges, cap redirects and response size, and sign payloads with HMAC.
  Done when: a webhook to `http://169.254.169.254` or a DNS-rebinding host is refused, with a test.

- **Write a tamper-evident audit log** — FEATURE — value 3 — effort M — risk low — category: audit
  Why: assumption: self-hosters need to know who did what. Log logins, token use, permission changes, force pushes, and CI secret reads in a hash-chained append-only table, with a UI view and export.
  Done when: editing a past row makes `gitter audit verify` fail.

- **Scan pushes for secrets and block them** — FEATURE — value 4 — effort M — risk med — category: secret scanning
  Why: public-site users will commit keys. A built-in pre-receive scan in Rust (regex plus entropy, with provider formats) warns or blocks, and the same scan masks values in CI logs.
  Done when: pushing a fake AWS key is rejected with a clear message, and `--push-option=allow-secret` is required to override it.

- **Run one-command encrypted backup and restore with verification** — FEATURE — value 4 — effort M — risk low — category: recovery
  Why: single binary plus SQLite plus bare repos is simple to snapshot, but without a safe path people lose data. Use SQLite online backup plus repo `git bundle` or a tar, with age encryption, scheduled in the compose. A restore drill runs against a scratch dir.
  Done when: backup then restore into an empty volume yields identical repo refs and DB checksums, checked in a test.

- **Wild: Make CI jobs run in disposable microVMs** — FEATURE — value 4 — effort L — risk high — category: CI isolation
  Why: wild. Firecracker/Cloud Hypervisor microVMs give hardware-level isolation with ~125 ms boot, so strangers' code is safe to run on a shared box. Assumption: the host exposes /dev/kvm, and Docker is the fallback.
  Done when: a job runs in a microVM and a kernel-exploit-style escape test cannot see host processes.

- **Wild: Add a "reproducible build attestation" badge** — FEATURE — value 3 — effort L — risk med — category: supply chain
  Why: wild. After CI, sign the artifact and a provenance statement (SLSA-style, sigstore-compatible, or ed25519 with a per-instance key) and show a verified badge on releases. Users can verify with `gitter verify`.
  Done when: tampering with a release artifact makes `gitter verify` fail, and the badge shows on the release page.

- **Wild: Ship a self-pentest command with a live security score** — FEATURE — value 3 — effort M — risk low — category: self-audit
  Why: wild. `gitter doctor --attack` runs the instance's own checks against itself (CSP, cookie flags, open ports, default credentials, exposed docker socket, outdated image, no TLS) and shows a score in the admin page. This lets the simple-compose audience harden without reading docs.
  Done when: a deliberately weakened config (e.g. socket mounted, HTTP only) lowers the score and lists each finding with a fix.

- **Wild: Quarantine suspicious new accounts' repos automatically** — FEATURE — value 3 — effort M — risk med — category: trust and safety
  Why: wild. A new-account trust tier keeps repos unlisted from explore and CI disabled until the account ages, passes a check, or an admin approves. This prevents spam and crypto-miner floods without blocking real users, since the owner can still push and clone.
  Done when: a fresh account's repo is absent from explore and its CI runs are queued as "pending trust", and approval flips both.
