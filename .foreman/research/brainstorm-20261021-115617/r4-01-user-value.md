# Brainstorm — lens: user value

- **Ship a local-only telemetry-free guarantee with an egress ledger** — FEATURE — value 5 — effort M — risk low — category: privacy
  Why: the pack flags privacy as thin; self-hosters pick this over GitHub for control. Every outbound connection (webhooks, ACME, mirrors, update checks, SMTP) is off by default and listed in one admin page with last-used time.
  Done when: a fresh instance in a no-network container runs fully, and the egress page lists zero allowed destinations until one is enabled.

- **Add per-repo visibility audiences (public, signed-in, link, people)** — FEATURE — value 5 — effort M — risk med — category: permissions
  Why: assumption: public site plus private-by-default lifecycle needs more than a boolean; "guest pass" exists only as a wild idea.
  Done when: a repo set to "signed-in only" returns 404 to anonymous clone, web and API, verified by an e2e test.

- **Make the first-run admin screen produce a working compose in under 60 seconds, timed** — FEATURE — value 5 — effort M — risk low — category: onboarding
  Why: the core request is "single simple compose file, simpler than GitLab"; existing ideas cover the wizard but none enforce a measured time-to-first-push.
  Done when: CI starts from `docker compose up`, performs signup, repo create and first push by script, and fails if it takes over 60 seconds.

- **Provide a "fork a PR into my own namespace" contribution path without write access** — FEATURE — value 4 — effort M — risk med — category: collaboration
  Why: GitHub-like public site needs outside contributors; forks are planned, but the PR-from-fork flow with CI trust gating is not described end to end.
  Done when: a non-member forks, pushes a branch, opens a PR, and the PR shows CI as "awaiting approval" until a maintainer approves.

- **Add per-user data portability bundle (repos, issues, PRs, tokens metadata) as a single archive** — FEATURE — value 4 — effort M — risk low — category: portability
  Why: "migrate me off" and account export appear separately; a single re-importable format also enables instance-to-instance moves.
  Done when: exporting a user and importing on a clean instance reproduces repos, issues and PR threads, diff-checked in a test.

- **Add a repo-level "CI cost preview" before enabling pipelines** — FEATURE — value 3 — effort S — risk low — category: CI
  Why: public instances with quotas need users to understand limits; assumption: surprises with quota exhaustion are a top complaint.
  Done when: the pipeline editor shows estimated minutes and quota remaining, and a test asserts it matches the scheduler's accounting.

- **Offer read-only anonymous browsing that never sets cookies** — FEATURE — value 3 — effort S — risk low — category: privacy
  Why: a public GitHub-like site benefits from cookie-free visitors (no banner needed); strengthens the privacy thread.
  Done when: anonymous requests to repo, commit and raw pages return no Set-Cookie headers, asserted in tests.

- **Add a plain-English "who can see this?" preview on every repo and setting** — FEATURE — value 4 — effort S — risk low — category: permissions
  Why: assumption: misconfigured visibility is the biggest harm on a public multi-user site; extends "explain every permission" into a live simulation.
  Done when: selecting "view as user X or anonymous" renders the repo as that viewer sees it.

- **Wild: Make the instance publish itself as a static read-only mirror to any dumb host** — FEATURE — value 3 — effort L — risk med — category: resilience
  Why: wild; one command exports browsable HTML plus bare repos so a public project survives the server being down, served from S3 or a USB stick.
  Done when: `gitter export-static` output is browsable and cloneable via plain file or HTTP server with no Gitter process.

- **Wild: Let a repo owner "seal" a release with a timestamped, offline-verifiable bundle** — FEATURE — value 3 — effort M — risk low — category: trust
  Why: wild; combines signed tags, SBOM and reproducible release ideas into one downloadable proof file verifiable without the instance.
  Done when: `gitter verify bundle.gsl` passes offline on a release and fails after any byte is altered.

- **Wild: Add a "what will this push change for others" dry-run on the CLI** — FEATURE — value 3 — effort M — risk med — category: collaboration
  Why: wild; before force-push or branch deletion, show affected PRs, open CI runs, protected rules and reviewers who would lose context.
  Done when: `gitter push --dry-run` lists affected PRs and rules for a force-push scenario in an e2e test.

- **Add an admin "retention policy" for logs, artifacts, deleted repos and audit data** — FEATURE — value 4 — effort M — risk low — category: privacy
  Why: disk accounting and expiry exist for CI only; privacy-minded operators need explicit, documented retention for IPs, sessions, deleted content.
  Done when: setting 30-day retention purges aged access logs and soft-deleted repos on schedule, verified with a clock-advanced test.

- **Add IP and user-agent minimization: hash or truncate stored IPs by default** — SECURITY — value 4 — effort S — risk low — category: privacy
  Why: rate-limiting and abuse controls will store IPs; storing them truncated limits liability on a public site.
  Done when: DB inspection after requests shows only truncated or salted-hash IPs, and rate limiting still works.
