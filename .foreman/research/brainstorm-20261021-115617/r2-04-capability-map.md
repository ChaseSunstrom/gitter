# Brainstorm — lens: capability map

- **Add CODEOWNERS and auto-reviewer assignment** — FEATURE — value 4 — effort M — risk low — category: code review
  Why: assumption: pack lists inline review and approvals but not path-based ownership, which GitHub, GitLab and Gitea all ship and which branch protection needs to be meaningful.
  Done when: a PR touching a path listed in CODEOWNERS auto-requests that owner and cannot merge, with protection on, until they approve.

- **Add a merge queue with speculative batching** — FEATURE — value 4 — effort L — risk med — category: merge/CI
  Why: assumption: merge gating is planned, but nothing stops two green PRs from breaking main once combined. GitHub's merge queue and GitLab's merge trains solve this, and the fast-CI pillar makes it affordable.
  Done when: two PRs that conflict only semantically are queued and the second is tested on top of the first, and the failing one is ejected without blocking the rest.

- **Add scheduled and manual pipeline triggers** — FEATURE — value 4 — effort S — risk low — category: CI
  Why: the pack covers push/PR triggers only. Cron schedules, `workflow_dispatch`-style manual runs with inputs, and API-triggered runs are table stakes for CI.
  Done when: a `schedule:` entry fires a run at the expected minute, and a manual run with input values appears in the UI and receives those values.

- **Add secrets and variables at repo, org and environment scope** — FEATURE — value 5 — effort M — risk med — category: CI/security
  Why: the pack mentions log masking but not a secret store. Deploy jobs need encrypted secrets, environment scoping and protected-branch-only exposure.
  Done when: a secret set on an environment is readable by a job on a protected branch, absent on a fork PR, and masked in the logs.

- **Add deployments, environments and approval gates** — FEATURE — value 3 — effort M — risk low — category: CD
  Why: assumption: preview URLs are listed, but not an environment history, manual approvals or rollbacks, which GitHub Environments and GitLab Environments both provide.
  Done when: a job targeting `environment: prod` waits for a named approver and the deployment shows in an environment timeline with its commit.

- **Add a security dashboard with dependency and SAST scanning** — FEATURE — value 3 — effort L — risk med — category: security
  Why: the pack has push secret-scanning only. GitHub's Dependabot and code scanning and GitLab's security dashboard are headline features of the "all features" request.
  Done when: pushing a lockfile with a known-vulnerable package raises an alert on the repo's Security tab, with an advisory link.

- **Add automated dependency update PRs** — FEATURE — value 3 — effort M — risk low — category: automation
  Why: assumption: nothing in the pack covers Renovate- or Dependabot-style updates, and the CI and PR machinery they depend on is already planned.
  Done when: a config file enables updates and a stale dependency yields an auto-opened PR whose CI result is visible.

- **Add a built-in wiki and discussions per repo** — FEATURE — value 3 — effort M — risk low — category: collaboration
  Why: the pack has issues and boards but no long-form docs or Q&A. The wiki can be stored as a git repo, which fits the git-first design.
  Done when: a repo's wiki can be cloned as `<repo>.wiki.git`, edited in the browser, and a discussion thread can mark an answer.

- **Add notifications with email, a web inbox and digest rules** — FEATURE — value 4 — effort M — risk low — category: notifications
  Why: the pack has no notification path for mentions, review requests or CI failures. A public multi-user forge needs one, and so does a self-hosted instance with an SMTP setting.
  Done when: an @mention produces an inbox item, and an email is sent only when SMTP is configured and the user's preferences allow it.

- **Add signed commits and tag verification** — FEATURE — value 3 — effort M — risk low — category: trust
  Why: passkeys and attestation appear in the pack, but not GPG/SSH signature verification, which shows a "Verified" badge on GitHub and GitLab and can be required by branch protection.
  Done when: a commit signed by an uploaded SSH key shows Verified, an unsigned one does not, and a protection rule can reject unsigned pushes.

- **Wild: Make the merge queue a git ref that anyone can `git fetch`** — FEATURE — value 3 — effort M — risk med — category: git-native
  Why: wild. Expose the speculative queue state as refs such as `refs/queue/N`, so a developer can fetch and test exactly what CI is testing, which no mainstream forge offers.
  Done when: `git fetch origin refs/queue/*` returns the speculative merge commit that CI is currently building.

- **Wild: Add a repo "health score" that fixes itself via PRs** — FEATURE — value 3 — effort M — risk low — category: automation
  Why: wild. Score the repo for a missing license, README, CI config and CODEOWNERS, then offer a one-click PR that adds the missing files.
  Done when: a bare repo shows a score below 100%, and clicking a suggestion opens a PR that raises the score once merged.

- **Wild: Add a flaky-test detector that quarantines tests across runs** — FEATURE — value 4 — effort M — risk low — category: CI
  Why: wild. Because Gitter owns the CI history, it can spot tests that flip between pass and fail on the same commit and mark them in the UI. Most forges leave that to third-party tools.
  Done when: a test that fails then passes on a retry of the same commit gets a "flaky" label and a rate in the run summary.

- **Wild: Add a read-only public API explorer with live "try it" on the user's own token** — FEATURE — value 2 — effort S — risk low — category: developer experience
  Why: wild. An OpenAPI spec is already planned, so a built-in explorer served from the binary costs little and makes the API discoverable without an external tool.
  Done when: the `/api/docs` page loads offline and can execute an authenticated call using the signed-in user's token.
