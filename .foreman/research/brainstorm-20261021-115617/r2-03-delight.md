# Brainstorm — lens: delight

- **Show a push-progress ribbon in the browser** — FEATURE — value 4 — effort M — risk low — category: feedback
  Why: assumption: the existing "push-to-green toast" is terminal-only; the web UI has no live signal after `git push`. A ribbon at the top of the repo page reads "received → indexed → CI queued → green" over one SSE channel.
  Done when: pushing to a repo with the page open shows the ribbon advancing through each stage without a reload.

- **Make the diff view hover-explain moved code** — FEATURE — value 4 — effort M — risk low — category: diff UI
  Why: assumption: diffs are the most-viewed screen after the file tree. Detect moved blocks (`git diff --color-moved` data) and draw a soft connector between the old and new location on hover instead of showing delete plus add.
  Done when: a commit that moves a function between files shows it tagged "moved" with a connector, and a test fixture asserts the detection.

- **Add keyboard-first everything with a visible hint layer** — FEATURE — value 4 — effort M — risk low — category: keyboard
  Why: the palette idea exists, but not discoverability. Holding `?` or `g` overlays one-key hints on every link and button, Vimium-style, with j/k moving through files and commits.
  Done when: pressing `g` on a repo page labels each clickable element and typing its label navigates there; Playwright covers it.

- **Render empty repos as a guided first-push card** — FEATURE — value 4 — effort S — risk low — category: onboarding
  Why: assumption: an empty repo is the first thing every new user sees. Show the exact clone URL for the current host, a copy button that adapts to HTTPS/SSH, and a live "waiting for first push…" pulse that flips to the repo the moment a push lands.
  Done when: creating a repo shows the card with the correct host-specific commands, and a push turns it into the normal view with no manual refresh.

- **Add a per-repo "README as landing page" reading mode** — FEATURE — value 3 — effort S — risk low — category: reading
  Why: assumption: most visitors to a public site only read the README. Offer typographic reading mode with a table of contents rail that tracks scroll, and heading anchors with a copy-link flourish.
  Done when: a long README shows a sticky TOC highlighting the current section, and the heading link copies a working anchor.

- **Give every CI step a satisfying finish animation and sound-off haptics** — FEATURE — value 3 — effort S — risk low — category: CI feedback
  Why: assumption: CI is watched often, so small cues add up. Steps settle with a short check-draw, failed steps shake once and auto-expand to the first error line, and the tab favicon changes colour with run state. No sound by default.
  Done when: the favicon reflects queued/running/passed/failed on a pipeline page, and a failed step auto-expands to its first error.

- **Add a contributor "wall" with real commit-derived stats** — FEATURE — value 3 — effort M — risk low — category: social
  Why: assumption: public-site people like recognition. Each profile gets a year heatmap (already planned) plus streaks, first-commit-of-a-repo badges, and "merged your first PR" moments computed from history, never gamified with points.
  Done when: a profile shows derived badges, and recomputing from a fixture repo gives the same set deterministically.

- **Let the logo react to the instance's state** — FEATURE — value 2 — effort S — risk low — category: branding
  Why: the user asked for a minimal logo and to use it. Make the mark a live status light: it draws on at load, pulses gently while any of your pipelines run, and shows a dot when a review is waiting on you.
  Done when: the header mark changes state when a pipeline is running for the logged-in user, verified in a browser test.

- **Wild: Turn the commit graph into a navigable 3D-ish "river" you can scrub with the mouse wheel** — FEATURE — value 3 — effort L — risk med — category: visualization
  Why: wild. Branches flow as parallel streams, merges join them, and CI status colours the water. Scrubbing moves through time and opens the commit under the cursor. Falls back to the plain list under reduced-motion.
  Done when: the graph page renders the river for a repo with 500+ commits at 60fps, and `prefers-reduced-motion` shows the flat list.

- **Wild: Add "ghost cursors" in the web editor for people viewing the same file** — FEATURE — value 3 — effort L — risk med — category: collaboration
  Why: wild. Reuse the SSE channel to show who else is reading or editing a file as small named presence dots in the gutter, with no document syncing at first, only presence.
  Done when: two browser sessions on one file each show the other's presence dot within a second, and it disappears on tab close.

- **Wild: Let a PR page play a short "what changed" replay of the branch's commits** — FEATURE — value 3 — effort M — risk low — category: review
  Why: wild. Reviewers step through the PR commit by commit with a spacebar-driven autoplay, each commit's message appearing as a caption while its diff animates in, so a messy PR reads like a story.
  Done when: opening the replay on a PR with 5 commits steps through all five with play/pause and arrow keys, and ends on the combined diff.

- **Add celebratory but skippable micro-moments for milestones** — FEATURE — value 2 — effort S — risk low — category: polish
  Why: assumption: moments like first push, first merged PR, or first green build are memorable. A short confetti-free animation, such as the logo mark drawing a check, appears once, never again, and can be switched off in settings.
  Done when: the first green build for an account shows the animation exactly once, and the setting disables it.

- **Preview links unfurl into beautiful share cards** — FEATURE — value 4 — effort M — risk low — category: sharing
  Why: assumption: a public site gets shared in chats. Generate Open Graph images server-side for repos, issues, PRs and commits showing name, language bar, stars and CI state, cached by OID.
  Done when: fetching a repo's `og:image` URL returns a PNG with the repo name, and repeat requests are served from cache with an ETag.
