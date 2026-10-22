## design: changes needed

**Verdict: approve with nits.** `static/css/account.css` is clean, with three small rules (lines 97-99) that reuse the existing tokens. The problems are in the markup and JS the CSS serves.

**MEDIUM**
- `static/js/pages/admin.js:1364` (diff 1364-1376, used again at diff 1453)
  - file: static/js/pages/admin.js
  - line: 1364
  - summary: `providerFields()` uses fixed ids (`pv-name`, `pv-base`, `pv-id`, `pv-secret`, `pv-kind`). It renders both in the "Add a provider" form and in the Edit modal, which is appended to `document.body` while that page form is still in the DOM.
  - short_summary: Duplicate form ids between add form and edit modal
  - failure_scenario: Open Admin > Sign-in > Edit on a provider, then click the "Client ID" label in the modal. `label[for=pv-id]` resolves to the first element with that id in the document, which is the page form's input behind the scrim. Focus lands behind the modal, and the modal's inputs have no programmatic label for a screen reader.
  - category: accessibility-duplicate-id
  - verdict: CONFIRMED (`modal()` at lib.js:230 appends the scrim to `document.body`)
  - fix: Pass an id prefix into `providerFields(p, 'pe')`. Use `pv-` for the add form and `pe-` for the modal, and update the `#pv-name`, `#pv-base` and `#pv-kind` lookups to match.
  - reproducer: In the admin Sign-in tab, click Edit, then click a field label. `document.activeElement` is the add-form input.

- `static/js/lib.js:1272` and `static/js/lib.js:1299` (diff lines)
  - file: static/js/lib.js
  - line: 1299
  - summary: In the reauth modal, `modal()` focuses `#ra-pw` (autofocus, lib.js:233) at open. When `/me/identities` returns with `has_password` false, `#ra-pw-f` is hidden, so the focused element disappears. No replacement receives focus.
  - short_summary: Reauth modal loses focus for passwordless accounts
  - failure_scenario: A passwordless OAuth-only user triggers a sensitive action. The modal opens with focus in the password field, then the field vanishes. Focus drops to `<body>`, and the "Confirm with GitHub" buttons injected afterwards are not focused. A keyboard or screen-reader user gets no cue about the change, since `#ra-why` isn't a live region. The password field also flashes before the fetch resolves.
  - category: focus-management
  - verdict: CONFIRMED
  - fix: After rendering, call `box.querySelector('button')?.focus()` when `!m.has_password`. Better, render `#ra-pw-f` hidden until the fetch resolves, or add `aria-live="polite"` to `#ra-why`.
  - reproducer: Sign in with a provider only, open any reauth-gated action, and observe `document.activeElement === document.body`.

**LOW**
- `static/js/pages/settings.js:1597` and `static/js/pages/admin.js:1395-1396`
  - file: static/js/pages/settings.js
  - line: 1597
  - summary: The per-provider buttons "Link", "Unlink", "Edit" and "Turn off/on" repeat with identical accessible names across rows. Only the delete button has an `aria-label`.
  - short_summary: Repeated row buttons lack provider-specific names
  - failure_scenario: With GitHub, GitLab and OIDC configured, a screen-reader button list reads "Link, Link, Unlink". The user can't tell which provider each belongs to.
  - category: accessibility-name
  - verdict: CONFIRMED
  - fix: Add `aria-label="Link ${p.name}"`, `"Unlink ${p.name}"`, `"Edit ${p.name}"` and `"Turn off ${p.name}"`.

- `static/js/pages/settings.js:1663`
  - file: static/js/pages/settings.js
  - line: 1663
  - summary: The Link handler runs `startOAuth()` in `try/finally busy(b,false)`. `startOAuth` resolves right after `location.assign`, so the button re-enables while the page is leaving.
  - short_summary: Link button re-enabled during navigation
  - failure_scenario: A fast double-click on "Link" sends a second `/start` request before navigation. This creates a second state row and the first provider tab races the second.
  - category: feedback-state
  - verdict: PLAUSIBLE
  - fix: Only call `busy(b,false)` in a `catch`, as `auth.js` and the reauth modal already do.

**Art director:** nothing to flag. The OAuth buttons are text-only `.btn.lg` buttons with no provider mark. That is acceptable and consistent with the rest of the app.

**Accessibility, other:** `.btn.sm` is 26px tall, which meets the 24px minimum. The global `:focus-visible` ring applies to the new buttons. `--muted` on `--surface` at 12px (`.or-sep`) uses existing palette tokens, so it is no worse than the rest of the app.

**First-time user:** the sign-in flow is clear: the provider buttons come first, then "or", then the password form. Errors from the provider return as an `oauth_error` toast or `#form-err` with `role="alert"`.

## Not checked
- Rendered appearance and contrast per theme (no screenshots named).
- Rust OAuth logic in `src/api/oauth.rs`, `tests/oauth.rs`, `src/net.rs`, `src/db.rs`. That belongs to the adversary lens (SSRF, state/PKCE, account linking) and was not reviewed here.
- The Playwright check in `scripts/ui-check.mjs`. The pre-audit "secret-looking" hits there look like the test fixture, not real secrets, but I did not confirm.
- Mobile layout at narrow widths for the new admin rows.

**Noticed:** `afterOAuth()` skips cleaning `oauth_error` on `/login` and `/signup`, so the error stays in the URL there and reappears on reload.
