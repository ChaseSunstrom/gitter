# Brainstorm — lens: unspoken needs

- **Ship a "doctor for the first hour" onboarding checklist** — FEATURE — value 5 — effort M — risk low — category: onboarding
  Why: assumption: a self-hoster's real fear is "did I set this up right?" (DNS, HTTPS, SSH port, runner registered, backup configured, email working). Existing ideas cover the wizard and `gitter doctor`, not a persistent, ordered, self-ticking checklist in the admin UI.
  Done when: a fresh instance shows an admin checklist that turns each item green only after a live probe passes (e.g. test clone, test CI job, test email), and unresolved items persist across logins.

- **Send transactional email without an SMTP server** — FEATURE — value 4 — effort M — risk med — category: email
  Why: assumption: PRs, mentions, password resets and CI failures need notification, but one-file compose has no mail service. Nothing in the pack covers email; users hit this the first time they forget a password.
  Done when: with no SMTP configured, reset and notification flows fall back to in-app inbox plus an admin-visible reset link, and with SMTP env set, a test email is delivered and logged.

- **Add a notifications inbox with per-repo watch levels** — FEATURE — value 4 — effort M — risk low — category: notifications
  Why: assumption: the pack lists reviews, CI and issues but no way to find out what needs attention; GitHub's inbox is what people live in.
  Done when: a mention, review request, and failed required check each create an inbox item that can be marked done, and watch levels suppress or allow them.

- **Make CI failures actionable in the diff view** — FEATURE — value 4 — effort M — risk low — category: CI UX
  Why: the user asked for status checks on commits/PRs; the unspoken need is not hunting through logs. Parse compiler/test output (file:line patterns) into annotations on the PR diff.
  Done when: a failing job whose log contains `src/x.rs:12:` shows an inline annotation on that line of the PR diff, linked to the log.

- **Provide a one-click "rerun / rerun failed / rerun with debug shell" control** — FEATURE — value 4 — effort M — risk med — category: CI UX
  Why: assumption: flaky and misconfigured jobs are the main CI pain; the pack has logs and replay but no remediation loop. A debug shell reuses the sandbox with a time-limited, authenticated web terminal.
  Done when: a failed run can be rerun from the UI for only failed jobs, and a debug session opens a terminal into that job's container that expires after a set time.

- **Add disk-usage accounting with reclaim suggestions** — FEATURE — value 4 — effort M — risk low — category: operations
  Why: assumption: a single volume holds repos, logs, artifacts, caches and backups; the first outage on a small VPS is a full disk. Quotas exist as ideas, but no visibility or cleanup guidance.
  Done when: the admin page shows per-repo and per-category usage, warns at a threshold, and a "reclaim" action (gc, expire artifacts, prune caches) frees measurable space.

- **Detect and warn on unsafe public exposure** — SECURITY — value 4 — effort S — risk low — category: safety
  Why: assumption: people paste the compose file onto a public VPS with default ports and open signup. Warn when registration is open with no abuse controls, when HTTP is served on a public IP, or when the Docker socket is mounted.
  Done when: starting under each misconfiguration produces a visible admin banner and a log line naming the fix.

- **Offer a "private by default, publish later" repo lifecycle** — FEATURE — value 3 — effort S — risk low — category: repos
  Why: assumption: on a public site people commit secrets or half-finished work and fear accidental exposure; visibility changes should be deliberate, with a pre-publish scan.
  Done when: changing a repo to public runs a secret scan and shows a confirmation listing findings and what becomes visible.

- **Keep a human-readable activity and "what changed since I left" digest** — FEATURE — value 3 — effort M — risk low — category: user value
  Why: assumption: returning users want to catch up fast. Combine pushes, CI results, comments and merges into a per-user summary since last visit.
  Done when: the home page shows a "since your last visit" section with counts and links, and it is empty only if nothing happened.

- **Wild: Let the instance explain its own slowness** — FEATURE — value 3 — effort M — risk low — category: observability
  Why: wild; the user cares intensely about speed. Every response carries a Server-Timing breakdown (git, SQLite, render), and a hidden overlay (press a key) shows the slowest recent requests with causes.
  Done when: toggling the overlay shows per-request timing split by subsystem and flags any request exceeding the performance budget.

- **Wild: Offer an "undo" for destructive actions on a timer** — FEATURE — value 4 — effort M — risk med — category: safety
  Why: wild; assumption: deleting a repo, force-pushing, or closing a PR are scary on a public site. A soft-delete with a 7-day trash and a reflog-backed branch restore removes the fear.
  Done when: a deleted repo or force-pushed branch can be restored from the UI within the retention window, and is purged afterwards.

- **Wild: Ship a "gitter pair" read-only live session link** — FEATURE — value 2 — effort L — risk med — category: collaboration
  Why: wild; share a link that mirrors your current file, scroll position and CI run so someone can follow along while debugging, using the SSE channel already planned.
  Done when: opening the link in a second browser follows the host's navigation within one second and ends when the host closes it.

- **Wild: Generate a repo "contributor onboarding" page automatically** — FEATURE — value 2 — effort M — risk low — category: docs
  Why: wild; assumption: newcomers to a repo want to know how to build and test it. Infer commands from the CI pipeline file and show "how to run this locally" on the repo page, tied to `gitter ci run`.
  Done when: a repo with a pipeline shows a generated "build and test locally" section whose commands match the pipeline steps.

- **Document the product with a self-hosted docs site inside the instance** — FEATURE — value 3 — effort S — risk low — category: docs
  Why: the pack flags docs as thin. Serving versioned docs from the binary at `/help` means they work offline and always match the running version.
  Done when: `/help` serves docs embedded in the binary, and the version shown matches `gitter --version`.
