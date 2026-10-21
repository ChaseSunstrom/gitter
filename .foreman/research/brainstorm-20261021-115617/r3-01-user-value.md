# Brainstorm — lens: user value

- **Ship a "migrate in 10 minutes" wizard with a parity report** — FEATURE — value 5 — effort L — risk med — category: adoption
  Why: pack lists import from GitHub/GitLab, but nothing that tells a switcher what they lose or gain. A report listing what moved (repos, issues, PRs, secrets needing re-entry, workflows translated vs. needing manual edit) is what decides whether anyone actually leaves GitHub.
  Done when: importing a fixture GitHub org yields a page listing every item as moved/translated/skipped with a reason, and a re-run imports only the delta.

- **Add a "my work" home dashboard** — FEATURE — value 5 — effort M — risk low — category: daily use
  Why: assumption: pack covers repo pages and inboxes but not the landing screen. One page showing PRs awaiting my review, my PRs with check state, failing builds on my branches, and assigned issues is the daily-return surface.
  Done when: a logged-in user sees those four lists on `/`, each loading from one request in under 50 ms on a seeded 1k-repo instance.

- **Provide per-repo "contribute in 60 seconds" flow for newcomers** — FEATURE — value 4 — effort M — risk low — category: contributors
  Why: public site; strangers need fork, edit, PR without learning git. Combine the web editor, auto-fork and PR creation into a single guided path that handles the fork silently.
  Done when: a fresh account can change a file on someone else's repo and have an open PR in three clicks, with no manual fork step.

- **Write a "migrating from GitLab CI/Actions" translator with explain-output** — FEATURE — value 4 — effort L — risk med — category: CI adoption
  Why: pack proposes native Actions syntax or a tiny YAML subset but not what to do on unsupported features. Output a converted file plus line-by-line notes ("`services:` unsupported, use sidecar X") so failure is informative, not silent.
  Done when: running `gitter ci convert` on three sample real-world workflows outputs a valid pipeline plus a notes list naming each unsupported key.

- **Offer a hosted-free "try before install" public sandbox with expiring accounts** — FEATURE — value 3 — effort M — risk med — category: evaluation
  Why: user wants to share a public site; evaluators want to click around without self-hosting. Distinct from the listed instance-in-a-box demo mode: this is a reset-nightly shared instance with throwaway logins.
  Done when: one button creates a temporary user pre-seeded with a sample repo and a green CI run, and all data is wiped after 24h.

- **Show a cost/capacity forecast in admin ("this box fits ~N users, M builds/hr")** — FEATURE — value 4 — effort M — risk low — category: operations
  Why: the user's core promise is lightweight and fast; self-hosters need to know when it stops being true. Derive from live RAM, disk growth rate and CI queue wait. Assumption: metrics already exist per the health endpoints idea.
  Done when: the admin page shows projected days until disk full and a queue-wait warning when p95 exceeds a configured threshold.

- **Add repo "release train" automation: tag push builds, signs, uploads assets, writes changelog** — FEATURE — value 4 — effort M — risk low — category: releases
  Why: pack has releases and assets separately from CI. The common user job is "tag, and it ships"; linking them with a changelog generated from merged PR titles closes it.
  Done when: pushing tag `v1.2.0` to a sample repo yields a release page with built artifacts, checksums and a changelog, without manual steps.

- **Add a unified cross-repo "everything I follow" feed with mute rules** — FEATURE — value 3 — effort M — risk low — category: discovery
  Why: pack has notifications and explore, but no timeline built from stars and follows. Public-site stickiness comes from a reason to return.
  Done when: following two users and starring a repo shows their pushes, releases and merged PRs in time order, and a mute rule hides a chosen repo.

- **Wild: Let a failed CI run open a pre-filled issue/PR draft with the log excerpt and suspect commit** — FEATURE — value 4 — effort M — risk low — category: CI workflow
  Why: wild. Pack has a "why is my build red" digest and a fork-and-fix flow, but nothing that turns red into the next action. Use the first bad commit from the run history to assign blame candidates.
  Done when: clicking "Report" on a red run produces a draft issue containing the failing step's last 50 lines, the suspect commit, and the run link.

- **Wild: Build a "sponsor a runner minute" flow where visitors donate CI capacity to a public project** — FEATURE — value 2 — effort L — risk high — category: community
  Why: wild. Public projects on a small box starve for CI; a visitor can attach their own runner to a specific repo through a scoped, sandboxed token. Related to the volunteer-laptop idea but per-project, with an explicit opt-in from the maintainer.
  Done when: a maintainer enables donated runners for one repo, a second user's runner picks up only that repo's jobs, and revoking removes access immediately.

- **Wild: Add a "bus factor and bit-rot" report for each repo** — FEATURE — value 3 — effort S — risk low — category: insight
  Why: wild. Derive from existing git history: who owns what, which files nobody touched in two years, which have no tests nearby. Cheap to compute, interesting enough that people will share the page.
  Done when: a repo's Insights tab lists the top concentrated-ownership directories and the stalest directories, computed from a fixture repo with known answers.

- **Add per-repo size and clone-speed hints with one-click shallow/partial clone commands** — FEATURE — value 3 — effort S — risk low — category: git UX
  Why: assumption: large repos will be hosted, and "wayyy faster" should reach the clone step. Show real clone size and offer `--filter=blob:none` and `--depth=1` copy buttons, with server-side partial-clone support enabled.
  Done when: the clone dialog shows measured sizes for full, shallow and blobless clones, and a blobless clone against the test server succeeds.

- **Add a changelog "what's new in Gitter" in-app note after each upgrade, plus a rollback hint** — FEATURE — value 2 — effort S — risk low — category: operations
  Why: pack covers zero-downtime upgrades and snapshots but not telling the admin what changed or how to undo it.
  Done when: after upgrading the fixture instance, the admin sees a dismissible note listing the version changes and the exact restore command for the pre-upgrade snapshot.
