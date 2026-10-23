**Verdict: changes needed** (design lens only; one MEDIUM and several LOW findings).

The diff file is stale: it still includes the already-merged `src/cli.rs` and `main.rs` logout work. I judged the current working tree. No screenshots were named, so these are findings from reading the source.

## lens: design

### Accessibility reviewer
- MEDIUM — `static/js/pages/editor.js:226` — The "who's here" list can't be read by assistive technology. Each person is a bare `<span title>` wrapping `avatar()`, which renders `alt=""` (`lib.js:191`). The container `#ed-people` has no label or live region, so joins and leaves are never announced. Fix: give each entry `role="img" aria-label="${user} is editing"`, and put `role="group" aria-label="People editing"` and `aria-live="polite"` on `#ed-people`. — confidence high.
  - file: `static/js/pages/editor.js`, line 226
  - summary: Presence avatars have no accessible name and changes aren't announced.
  - short_summary: Presence list invisible to screen readers
  - failure_scenario: With a screen reader, nina joins. `#ed-people` gets `<span title="nina"><img alt="">`, so nothing is read and nobody can tell who else is editing.
  - category: accessibility-name
  - verdict: CONFIRMED
- LOW — `static/css/pages.css:136` — The cursor label is 10px white text on `var(--peer)`. Computed contrast with #fff is below 4.5:1 for six of the eight colours: #f76b15 about 3.0, #12a594 about 3.1, #30a46c about 3.2, #978365 about 3.7, #e5484d about 3.9, #d6409f about 4.1. Fix: use dark text on the light-ish colours, or darken the palette, and raise the font to 11px or more. — confidence high.
  - file: `static/css/pages.css`, line 136
  - summary: Peer name labels fail 4.5:1 for most palette colours.
  - short_summary: Peer label contrast below 4.5:1
  - failure_scenario: A peer gets colour 3 (orange) and their label is white 10px on #f76b15, about 3.0:1.
  - category: contrast
  - verdict: CONFIRMED
- LOW — `static/css/pages.css:136` — The label sits at `top:-13px`, 12px tall, inside the previous line's 20px box. The layer has `z-index:1` and is above the text, so it covers the lower half of the line above the peer's cursor, permanently. On line 1 it is also clipped by the `overflow:auto` scroller. Fix: fade the label to the caret alone after about 2s idle, or draw it beside the caret. — confidence medium.
  - file: `static/css/pages.css`, line 136
  - summary: The always-on peer label covers the previous line's text.
  - short_summary: Peer label hides the line above
  - failure_scenario: A peer's cursor is on line 5 and the label covers the bottom of line 4's glyphs at that column for as long as the peer stays there.
  - category: overlay-obscures-content
  - verdict: PLAUSIBLE

### First-time user
- MEDIUM — `static/js/collab.js:50` — The stream has no error handler (nothing in `static/js` handles `onerror` except `ci.js`). When a resync fails, the code toasts the error and then also toasts "Caught up with the others (…)", which is false. `ready` stays false, so `local()` ignores every further keystroke. Edits silently stop syncing while the avatars still show. Fix: toast the "why" message only when the reset succeeds. Add `es.onerror` that shows a persistent "Disconnected, your text isn't shared" state, and keep the textarea read-only or retry. — confidence high.
  - file: `static/js/collab.js`, line 50
  - summary: A failed resync reports success and leaves the session silently dead.
  - short_summary: Failed resync says "Caught up" and stops syncing
  - failure_scenario: Alice and nina are in a session and the server restarts. Alice types, so `POST /ops` returns 404 and `sendOp` calls `resync`. `GET` returns 404 and she sees "Not Found" followed by "Caught up with the others (Not Found)". `ready` stays false and later typing is never sent.
  - category: misleading-feedback
  - verdict: CONFIRMED
- LOW — `static/js/collab.js:123` — After joining, `reset(joined)` is followed by `ta.readOnly = true; ready = false` until `init` arrives. With no `onerror`, a stream that never connects (a buffering proxy, say) leaves the textarea read-only with no message. Fix: show "Connecting…" and a timeout error. — confidence medium.
- LOW — `static/js/pages/editor.js:216` — Co-author trailers are appended invisibly to the commit message, so the committer isn't told who gets credit. The path input also goes read-only at line 230 with no explanation, and there is no way to leave a session. Fix: show "Co-authors: nina" in the commit box and add a hint on the path input. — confidence high.

### Art director
- LOW — `static/css/pages.css:139` — The Radix-style hues (#e5484d, #3e63dd, …) are hard-coded and ignore the app's skin and theme variables. Fix: derive them from the theme tokens. — confidence low.

## Not checked
- Rendered layout and screenshots. Whether the 1px clip at the top of line 1 happens in a real browser. Dark-theme contrast of the avatar rings.
- The OT and server correctness lenses, and the pre-audit flags: the `ui-check.mjs` password fixture carries a `pragma: allowlist secret` comment, and the console logs are intentional test output.

Noticed: `paintPeers` at `collab.js:22` rebuilds the whole layer's `innerHTML` on every keystroke and remote op, which is O(file size) and up to 1M characters.
Noticed: the uncommitted `ui-check.mjs` and `e2e.sh` edits mix in unrelated changes (T-0123/T-0124 work).
