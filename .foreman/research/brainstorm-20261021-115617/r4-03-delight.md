# Brainstorm — lens: delight

- **Show a "your data stays here" badge with live egress ledger** — FEATURE — value 4 — effort M — risk low — category: privacy delight
  Why: pack flags privacy as thin; many ideas add outbound calls (webhooks, ACME, SMTP, mirrors, import) with no visible accounting.
  Done when: admin footer shows a counter of outbound connections by destination since boot, and a fresh instance with no integrations shows "0 external calls".

- **Animate a push as a visible object travelling into the repo** — FEATURE — value 3 — effort M — risk low — category: motion
  Why: "push-progress ribbon" and "push-to-green toast" exist but only as separate pieces; assumption: unify into one choreographed push → CI → merge journey.
  Done when: pushing from a terminal while the repo page is open shows one continuous ribbon that morphs from "received" to "building" to a green or red result without layout shift.

- **Make the empty-state first-push card detect the user's actual OS and remote** — FEATURE — value 4 — effort S — risk low — category: onboarding
  Why: "guided first-push card" is listed generically; assumption: pre-filling real URL, user and token removes the copy-edit step.
  Done when: a new empty repo shows copy-ready commands with the correct host and username, and one click copies a full sequence that succeeds when pasted.

- **Give each CI step a tactile "click" with reduced-motion parity** — FEATURE — value 2 — effort S — risk low — category: accessibility motion
  Why: finish animations are proposed but accessibility parity isn't stated; assumption: a delight layer must degrade cleanly.
  Done when: with prefers-reduced-motion set, every animation has a static equivalent that conveys the same state, verified by a Playwright test.

- **Offer a "quiet mode" that mutes all celebration at once** — FEATURE — value 3 — effort S — risk low — category: polish
  Why: many micro-moments, toasts, mascots and sounds are proposed; assumption: users on a serious team need one switch.
  Done when: one setting disables confetti, mascot, toasts and haptics instance-wide or per user, and a test confirms none render.

- **Show diff stats as a tiny sparkline beside every commit and PR** — FEATURE — value 3 — effort S — risk low — category: information density
  Why: assumption: commit lists are the most-viewed page; a glanceable size signal reduces opening commits to judge them.
  Done when: each commit row shows an add/delete bar computed from numstat, cached by OID.

- **Let hovering a CI step's duration show where its time went** — FEATURE — value 3 — effort M — risk low — category: CI insight
  Why: job-start latency budgets and per-phase tracing are proposed, but not surfaced to the viewer.
  Done when: hovering a step shows queue, checkout, cache and run phases summing to the step time.

- **Wild: Offer a "tour guide" ghost that walks a newcomer through the instance using real clicks** — FEATURE — value 3 — effort L — risk med — category: onboarding (wild)
  Why: assumption: first-hour onboarding checklist exists, but a moving, skippable guided cursor would be the version people demo to friends.
  Done when: starting the tour moves a visible pointer through create repo, push, and view CI, and Esc stops it at any step.

- **Wild: Play the commit history as generative ambient audio, off by default** — FEATURE — value 2 — effort M — risk low — category: delight (wild)
  Why: assumption: a playful, opt-in extra people show others; pairs with the time-lapse repo page.
  Done when: pressing play on a repo's history emits tones mapped to commit size and author, entirely client-side with no network audio fetch.

- **Wild: Print the instance's first-run log as a styled ASCII welcome with the admin link and logo** — FEATURE — value 3 — effort S — risk low — category: first impression (wild)
  Why: the first thing a self-hoster sees is `docker compose up` output; a memorable, clear banner with the one next step builds trust.
  Done when: first boot logs a banner showing the logo mark, URL, and a single-use admin claim link, and subsequent boots print a short one-line status instead.

- **Make local-only features show a lock icon explaining nothing leaves the box** — FEATURE — value 3 — effort S — risk low — category: privacy delight
  Why: privacy is thin; local LLM hooks, "ask this repo", and search would benefit from visible reassurance.
  Done when: each local-processing feature carries a tooltip stating where computation happens, and tests fail if such a feature makes an external request.

- **Tint the browser tab icon with live CI status** — FEATURE — value 3 — effort S — risk low — category: ambient feedback
  Why: assumption: people leave PR tabs open waiting for builds; a favicon dot saves tab switching.
  Done when: the favicon of a PR or commit page reflects pending, passing or failing within 2 seconds of a status change.

- **Show a gentle "you're caught up" end-of-inbox state with a tiny illustration** — FEATURE — value 2 — effort S — risk low — category: empty states
  Why: the notification inbox and "my work" home are proposed; the empty state is where tone is set.
  Done when: an empty inbox renders an illustrated message with the next suggested action, and no layout shift occurs when items arrive.
