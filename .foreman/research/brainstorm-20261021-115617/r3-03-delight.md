# Brainstorm — lens: delight

- **Apply every action optimistically, with an animated rollback** — FEATURE — value 4 — effort M — risk low — category: responsiveness
  Why: the user asked for "more responsive" and "faster" than GitHub. Instant local feedback is what makes a page feel fast beyond measured latency. Assumption: the SPA has a central mutation layer where this can live.
  Done when: with the server delayed 2s, starring, commenting and closing an issue all update the UI in under 50ms, and a forced server error animates the change back with an inline message.

- **Linkify stack traces to the exact line at that commit** — FEATURE — value 4 — effort M — risk low — category: CI UX
  Why: CI logs and issues are where people paste traces, and the pack's ideas stop at ANSI colour and step folding. Frames like `src/foo.rs:42` should resolve against the run's commit.
  Done when: a CI log containing `src/main.rs:12` renders a link that opens the blob at that run's SHA with line 12 highlighted.

- **Scrub a file through its history** — FEATURE — value 4 — effort M — risk med — category: code viewing
  Why: blame and the time-travel repo page are listed, but a per-file slider is missing. Dragging it morphs the file between versions, with changed lines animated using view transitions. Assumption: file history is cheap enough to precompute on push (see the browse-index idea).
  Done when: a slider on the blob page switches between versions of a 10-commit file without a page reload, and changed lines visibly animate.

- **Mark files as viewed in a diff, with a progress ring** — FEATURE — value 3 — effort S — risk low — category: review
  Why: this is a review-flow detail that makes large PRs feel finishable. Reviewed files collapse smoothly, the ring fills, and the state survives new pushes when a file's diff is unchanged.
  Done when: marking a file viewed collapses it, the ring updates, and after a push that doesn't touch that file it stays viewed.

- **Autosave every draft and restore it across devices** — FEATURE — value 4 — effort S — risk low — category: trust
  Why: assumption: losing a long comment or editor buffer to a refresh or session timeout is the quickest way to lose trust. Drafts are stored server-side per user, keyed by context.
  Done when: typing a PR comment, closing the tab, and opening the same PR in another browser restores the text with a "restored draft" chip.

- **Peek at any reference on hover** — FEATURE — value 3 — effort M — risk low — category: navigation
  Why: this extends prefetch-on-hover. `#12`, `abc1234`, `@user` and `path/file.rs` in markdown open a small card with the title, status and CI dot, served from prefetched data.
  Done when: hovering `#12` in a comment shows a card with its title and state in under 100ms, with no network request if it was prefetched.

- **Guided three-way conflict resolver** — FEATURE — value 4 — effort L — risk med — category: PRs
  Why: the plan merges with `merge-tree`, so conflicts will happen. Resolving them in the browser, with "take ours/theirs/both" chips and a live preview, fits the "Edit in browser then open PR" flow.
  Done when: a PR with a one-hunk conflict can be resolved and merged entirely from the web UI, and the resulting merge commit has the expected content.

- **Drag a folder onto a repo page to commit it** — FEATURE — value 3 — effort M — risk low — category: web editor
  Why: the web editor is planned, but bulk upload isn't mentioned. Dropping files shows a tree preview, then a progress animation, then one commit.
  Done when: dropping a folder of 20 files creates a single commit containing them all, and a cancelled upload leaves no commit.

- **Let the owner theme the instance** — FEATURE — value 3 — effort S — risk low — category: branding
  Why: self-hosters want their forge to look like theirs. Accent colour, name and logo upload get a live preview in settings, and the favicon and share cards are regenerated from them.
  Done when: changing the accent colour in the settings page restyles the open tab instantly and persists after restart.

- **Wild: Generate a private "Gitter Wrapped" recap** — FEATURE — value 3 — effort M — risk low — category: delight
  Why: wild. A yearly (or on-demand) shareable card of commits, streaks, busiest repos and CI green rate, computed locally from the instance's own data. It also gives people a reason to open the site in December.
  Done when: `/me/wrapped` renders an animated story and a downloadable image built only from local data.

- **Wild: Record in-repo code tours that play as an animated walkthrough** — FEATURE — value 3 — effort L — risk med — category: onboarding
  Why: wild. An author annotates a sequence of files and lines in a `.gitter/tours/*.md` file. The UI plays it as a guided fly-through with a spotlight, so a new contributor's first hour needs no docs.
  Done when: a tour file with three steps plays in the UI, each step scrolls to and highlights its target, and a stale line reference shows a warning.

- **Wild: Render the repo as a walkable treemap city** — FEATURE — value 2 — effort L — risk med — category: visualization
  Why: wild. Directories become districts and files become buildings. Height is size, colour is churn, and glow is "red CI touched here". It's pointless in the best way, and the sort of thing people show others. Assumption: it can be drawn with canvas and no 3D library.
  Done when: a repo-level toggle shows the treemap at 60fps for a 5,000-file repo, and clicking a building opens the file.

- **Reserve layout space so nothing ever jumps** — FIX — value 3 — effort S — risk low — category: polish
  Why: assumption: layout shift on async content such as CI dots, avatars and diff expansion is what makes a fast site feel janky. Ideas in the pack cover latency but not visual stability.
  Done when: a Playwright check on the repo, PR and commit pages measures a cumulative layout shift of 0 on a throttled connection.
