# Brainstorm — lens: unspoken needs

- **Ship a "stuck?" self-diagnosing first-push flow** — FEATURE — value 5 — effort M — risk low — category: onboarding
  Why: the user wants one simple compose file and "simpler than GitLab"; the first failure (wrong URL behind a proxy, HTTP vs HTTPS, token not accepted, push size limit) will be a git client error that tells them nothing. Assumption: most first-hour churn happens at the first `git push`.
  Done when: pushing with a bad token, a wrong remote URL or an oversized pack shows a server-side message in the git client output naming the cause and the fix, and the same cause appears in a "recent rejected pushes" list in the repo UI.

- **Provide a one-page "what is this instance doing right now" admin view** — FEATURE — value 4 — effort M — risk low — category: observability
  Why: the user asked for "wayyy faster" and "all features" on a single process; with git subprocesses, CI jobs, SQLite writes and the runner sharing one box, they will want to see which one is eating the machine. Assumption: no external Prometheus/Grafana is wanted given the one-file simplicity goal.
  Done when: /admin/live shows running git processes, active CI jobs, SQLite write queue depth, RAM and disk per component, and updates without reload.

- **Let the instance upgrade itself behind one admin button** — FEATURE — value 4 — effort M — risk med — category: operations
  Why: "docker compose pull" is the planned path, but a self-hoster who isn't at a terminal still needs to know a new version exists and trust it. Past ideas cover snapshot and rollback; none cover discovery and one-click consent. Assumption: the compose file can mount nothing extra, so this triggers a documented pull via a tiny updater mode of the same binary only if the user opts in.
  Done when: the admin page shows "new version available" with release notes, and clicking Upgrade snapshots, restarts into the new image, runs the health check, and rolls back on failure (verified in e2e with a deliberately broken image).

- **Explain every permission as a sentence on the page where it applies** — FEATURE — value 4 — effort S — risk low — category: clarity
  Why: the plan has orgs, tokens, branch protection, CI on forks and a public site. Users of a self-hosted public forge repeatedly misconfigure visibility. Assumption: nobody reads docs for this.
  Done when: every repo, token and CI-secret settings screen shows a live plain-language line such as "Anyone on the internet can read this and fork it; CI from forks runs without secrets", and a test asserts that line changes when the setting changes.

- **Warn before a push or a setting leaks something** — FEATURE — value 4 — effort M — risk low — category: safety
  Why: public site plus "private by default, publish later" is listed, but the moment of switching to public is the real risk. Assumption: users want a checklist at that moment, not a policy.
  Done when: flipping a repo to public shows a pre-flight page (secret scan hits, .env files, large files, CI secrets exposed to forks) and requires confirmation if any item is flagged.

- **Keep a "recently visited" and "pick up where I left off" home** — FEATURE — value 4 — effort S — risk low — category: user value
  Why: the user wants GitHub-like but "cleaner and more responsive"; GitHub's dashboard is a noted weak spot. Assumption: a returning user wants their open PRs, red builds and review requests in one list within one request.
  Done when: the logged-in home renders in a single query-backed request under the performance budget and lists failing CI on my branches, PRs awaiting my review and my recent repos.

- **Ship an in-product "same box, same repo" speed comparison on demand** — FEATURE — value 3 — effort M — risk med — category: performance
  Why: the user's headline claim is "wayyy faster"; a published benchmark exists as an idea, but nothing lets the owner verify it on their own hardware after setup. Assumption: owners want proof on their data rather than a blog chart.
  Done when: `gitter bench` runs clone, page-load and CI-start timings against the local instance and prints the numbers next to stored reference figures for the same operations.

- **Offer a phone-friendly review and approve mode** — FEATURE — value 4 — effort M — risk low — category: devices
  Why: the user stressed "responsive"; the most common mobile task on a forge is approving a PR or checking a red build, not browsing code. Assumption: layout is currently desktop-first.
  Done when: in Playwright at a 390px viewport a user can open a PR, read the diff, see check status, comment and merge without horizontal scrolling.

- **Wild: Let the instance email you a weekly "state of the forge" as plain text** — FEATURE — value 3 — effort S — risk low — category: delight
  Why: assumption: self-hosters forget their instance exists until it breaks; a short digest of disk growth, failed backups, slowest endpoints and pending upgrades keeps trust high. wild
  Done when: with SMTP configured, a weekly message arrives containing those four items, and without SMTP the same text appears on the admin home.

- **Wild: Add a "bus factor" panic switch that exports everything to a plain folder** — FEATURE — value 4 — effort M — risk low — category: trust
  Why: users moving off GitHub/GitLab will worry about lock-in again; import is covered but a clean, tool-free export is not. Assumption: a bare repos plus JSON/Markdown dump satisfies it. wild
  Done when: one command produces a directory containing bare repos, issues and PRs as Markdown/JSON, and CI logs, and a script rebuilds the readable content from it without Gitter running.

- **Wild: Make a "demo me" button that seeds a believable sample org with history and a red-then-green build** — FEATURE — value 3 — effort M — risk low — category: delight
  Why: a fresh instance is empty, which hides every feature the user asked for ("all features"); assumption: owners want to show colleagues something real in 30 seconds. wild
  Done when: one click creates a sample repo with commits, a PR, an issue and a CI run that fails then passes, and a second click removes all of it cleanly.

- **Wild: Let a CI run's failure open a pre-filled issue assigned to the pusher** — FEATURE — value 3 — effort S — risk low — category: workflow
  Why: assumption: red builds on the default branch are noticed late; the failure digest idea exists but no loop closes it into the tracker. wild
  Done when: enabling the option on a repo causes a failed default-branch run to create exactly one issue (deduplicated across re-runs) linking the run and the culprit commit.

- **Show disk and RAM headroom as a visible budget in the footer for admins** — FEATURE — value 3 — effort S — risk low — category: operations
  Why: the plan promises a published low-RAM number and a single volume; assumption: owners on small VPSes want early warning before the disk fills from CI artifacts and repos.
  Done when: the admin footer shows free disk and memory use, turns amber at 80% and red at 90%, and a test with a mocked low-disk state shows the banner.
