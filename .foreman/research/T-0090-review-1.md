## Verdict
Changes needed. A non-admin user can starve the shared import worker, and re-running an import overwrites local work.

## lens: adversary

- HIGH — src/imports.rs:486 (also :233) — A provider that always sends `Link: <same URL>; rel="next"` makes paging endless, which starves every other user's imports. Cap the pages per stage and the rows per item, and fail when `next` equals the current URL or a URL already seen. — high
  - file: src/imports.rs
  - line: 486
  - summary: Paging follows `rel="next"` with no page cap and no cycle check.
  - short_summary: Endless paging starves the import worker
  - failure_scenario: A non-admin user runs an https server of their own that answers `/user`, `/user/repos` and `labels` with `[]` plus a `Link` header pointing to itself. They list and start an import against it. `advance` returns `Flow::Stay(next)` forever. `save` refreshes `updated_at`, so the one-day idle sweep never fires. `claim` orders by `next_at, id`, so this item (`next_at` stays 0) is always claimed first, and `run` is a single loop. Every other user's import waits indefinitely. The same loop in `list()` (:233) never ends when pages hold no valid rows, because the `out.len() < MAX_REPOS` check never trips. That ties up the HTTP handler and a blocking thread. The attacker never needs to cancel, and only an admin can.
  - category: resource-exhaustion

- HIGH — src/imports.rs:494 (`create`, reuse branch) with src/mirrors.rs:290 — Importing again into a repository an earlier import made runs `git fetch --prune --force` over it. Local pushes and branches are lost. Take the reuse path only when the repository has no commits newer than the import. Otherwise skip the clone stage or drop `--force`/`--prune` for reused repositories. Correct the README line "fills in only what's missing". — high
  - file: src/imports.rs
  - line: 494
  - summary: Re-import force-fetches over a repository that now has local work.
  - short_summary: Re-import overwrites local pushes and prunes branches
  - failure_scenario: The user imports octo/hello and pushes a commit to `main` and a new branch `wip` in alice/hello. They re-run the import to pick up new issues. `create` finds the `import_map` row and returns the existing repo. `mirrors::pull` then fetches `+refs/heads/*:refs/heads/*` with `--force --prune`. Local `main` is reset to the source and `wip` is deleted. No confirmation is asked, and branch protection is bypassed.
  - category: data-loss

- MEDIUM — src/imports.rs:908 (`start`) — Unlike `preview`, `start` has no rate limit, yet it calls `list()` against a user-chosen host before anything is stored. A user can send unlimited outbound requests, and the `listing_error` text (up to 300 characters of the remote's `message`) comes back in the 502. Apply the same `import-list:{uid}` limiter in `start`. — medium
  - file: src/imports.rs
  - line: 908
  - summary: `start` makes unthrottled outbound listing requests.
  - short_summary: No rate limit on start's outbound listing
  - failure_scenario: POST /api/imports in a loop with a bad `source`. Each call makes 2 or more fetches to any public host and reflects error text. The `busy` check passes because no job row exists.
  - category: missing-rate-limit

- LOW — src/auth.rs:399 — Basic auth now also accepts the username `oauth2` for any token, widening the accepted names. No bypass, since the secret must still be a valid token. Keep it, but mention it in the README or a test. — high

The token-exposure paths I traced were sound: token on cross-host cursors, token in errors, redirects (`followRedirects=false` and `max_redirects(0)`), the clone host pinned by `net::pin`, and `html` escaping in import.js.

## lens: edge

- MEDIUM — src/imports.rs:399 — Any transient error (timeout, 5xx, DNS blip, git stall) sets the item to `failed` permanently. Retry transient errors with backoff a few times through `next_at` before failing. Recovery is a manual re-import, which hits the HIGH data-loss finding above. — medium
- MEDIUM — src/imports.rs:333, 360 — After a crash or restart, items stay leased for 900 seconds, so imports resume up to 15 minutes late. README says "a restart carries on where it stopped". Zero `running_until` on startup, since there is a single worker. — medium
- LOW — src/imports.rs:611 — A numbered issue or merge request that collides with an existing local number is silently dropped (`added == 0`). The item still reports done. Count the skipped rows and put them in `error`. — medium
- LOW — src/imports.rs:908 — `req.repos` is not deduplicated (case-insensitive), so a repeated path becomes two items and is cloned twice. — high
- LOW — src/imports.rs (`Source::new`) — The `https://github.com` check is case-sensitive. `https://GitHub.com` or `www.` gets `/api/v3` and fails with a 404. Lowercase the host before comparing. — medium
- LOW — src/imports.rs:420 — One global worker serves one item at a time, so a large clone blocks every other user's imports. Fine for now, but undocumented.

## Acceptance criteria
No criteria were in the brief. The README steps match the code except two claims. "Fills in only what's missing" is false for the clone (HIGH above). "Cancel ends it at once" is true only for the next claim, since an in-flight page or clone completes.

## Implicit decisions
- One import per user at a time, and one global worker.
- Re-import reuses the repository through an `import_map` row keyed `web/path`.
- Only admins may allow private addresses.
- Remote times are clamped to now; authorship becomes text in the body.
- Merge requests are numbered after the highest issue.
- Failed means failed, with no retry.
- Draft releases are created when the tag is missing.

## Not checked
I did not run the tests or the ui-check, review `src/security.rs`, `owner_for`'s role logic in depth, markdown rendering of imported bodies, or the playwright and javascript card items. The `[REDACTED]` markers in the diff are redaction artifacts, so I could not read how the token is trimmed or sealed.

Noticed: README.md claims re-import "fills in only what's missing", and a test for re-import over local commits is missing.
Noticed: `Cancel` does not stop an in-flight clone.
Noticed: `import_items.error` and the job `error` can hold remote-controlled text (already redacted and stripped of control characters), so the UI relies on `html` escaping.
