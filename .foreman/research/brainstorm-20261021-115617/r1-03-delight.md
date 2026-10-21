# Brainstorm — lens: delight

- **Show a live "push-to-green" toast in the terminal** — FEATURE — value 5 — effort M — risk low — category: CI delight
  Why: user wants CI with streaming logs and status checks; `git push` output can carry a URL plus a live status line (assumption: the receive-pack wrapper can stream sideband messages).
  Done when: pushing to a repo with a pipeline prints a clickable run URL in the push output, and the final line shows pass/fail with duration.

- **Stream CI logs with ANSI color and step folding** — FEATURE — value 5 — effort M — risk low — category: CI UI
  Why: T-0002 asks for logs streaming; color and collapsible steps with auto-scroll that pauses on user scroll are what make logs pleasant.
  Done when: a running job shows colored output appended live over SSE, steps collapse and expand, and the failed step opens automatically.

- **Boot the whole stack with one command and a friendly first-run wizard** — FEATURE — value 5 — effort M — risk low — category: onboarding
  Why: the user insists on one simple docker-compose.yml, simpler than GitLab; the first run should end in a working site plus a registered runner without editing config.
  Done when: `docker compose up` on a clean machine serves the site, a first-visit page creates the admin, and a built-in runner shows as online with no manual tokens.

- **Animate commit graph and diffs with view transitions** — FEATURE — value 4 — effort M — risk low — category: motion
  Why: the user asked for beautiful UI and animations, and the planned SPA already uses view transitions; file-to-file navigation can morph the file tree row into the blob header.
  Done when: navigating tree→blob animates a shared element, and it degrades to an instant swap under `prefers-reduced-motion`.

- **Add instant-feel navigation with a visible speed budget** — PERFORMANCE — value 4 — effort S — risk low — category: speed
  Why: "wayyy faster" than GitLab is a headline claim; show server time in the footer and prefetch on hover so pages feel instant.
  Done when: the footer shows server render time, and the e2e check asserts that repo pages respond in under 50 ms locally.

- **Make the command palette do everything** — FEATURE — value 4 — effort M — risk low — category: keyboard UX
  Why: the plan already includes a palette and a fuzzy file finder; extend it to actions (create repo, switch theme, rerun the last pipeline, jump to PR).
  Done when: pressing `Cmd/Ctrl-K` and typing "rerun" lists the failed jobs and runs one with Enter.

- **Generate a unique identicon and a minimal animated logo mark** — FEATURE — value 3 — effort S — risk low — category: branding
  Why: the user asked for a minimalistic logo used throughout; deterministic repo and user avatars give the site personality with no uploads.
  Done when: every new user and repo shows a deterministic SVG identicon in the same visual style as the logo, and the logo animates once on load.

- **Show CI status as a heartbeat strip on commit lists** — FEATURE — value 4 — effort S — risk low — category: CI UI
  Why: status checks on commits/PRs are requested; a thin strip of colored cells over the last 30 runs per branch is glanceable.
  Done when: the branch page shows the last 30 runs as colored cells, and hovering one shows commit, duration and result.

- **Offer a one-click "Edit in browser then open PR" flow** — FEATURE — value 4 — effort M — risk med — category: editor
  Why: the web editor uses plumbing commits; a branch-and-PR path with a live diff preview lowers friction for casual contributors on a public site.
  Done when: editing a file on a protected branch auto-creates a branch and lands on a prefilled PR form.

- **Replay any CI run as a scrubbable timeline** — wild — FEATURE — value 3 — effort L — risk med — category: CI UI
  Why: assumption: logs are stored with timestamps; a timeline with a scrubber shows what ran in parallel and where time went.
  Done when: a finished run page has a Gantt-style timeline, and dragging the scrubber shows the log as it was at that moment.

- **Let the repo page play its own history as a time-lapse** — wild — FEATURE — value 2 — effort M — risk low — category: visualization
  Why: assumption: commit data is cheap to read via git plumbing; a gource-like animation is a share-worthy detail that people show others.
  Done when: a "Play history" button animates the file tree growing across commits at an adjustable speed, rendered on canvas.

- **Add a tiny pet mascot that reacts to build state** — wild — FEATURE — value 2 — effort S — risk low — category: playful
  Why: assumption: a playful touch fits the logo; a small SVG creature near the CI badge sleeps when idle, runs on builds, celebrates on green, and slumps on red.
  Done when: the mascot's state follows the latest run status, and a setting turns it off.

- **Ship a status badge and embeddable README card generator** — FEATURE — value 3 — effort S — risk low — category: sharing
  Why: it's a public site; shareable, well-designed SVG badges and social preview images make repos look good when linked.
  Done when: `/badge/<repo>/ci.svg` renders the live status, and each repo link yields an Open Graph image with name, language bar and stars.

- **Make themes follow time of day with a smooth crossfade** — FEATURE — value 2 — effort S — risk low — category: theming
  Why: themes are already planned; an automatic schedule plus an animated switch adds polish at low cost.
  Done when: the "auto" theme setting changes at sunset local time with a 300 ms crossfade and no flash on load.
