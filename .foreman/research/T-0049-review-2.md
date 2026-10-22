**Verdict: changes needed.** Two MEDIUM findings, one in the apply path and one in pending-review visibility. The rest are LOW. I did not run any tests or scripts.

## lens: intent

- MEDIUM — `src/api/reviews.rs:624-631` (via `lines_at` at 222) — Applying a suggestion rewrites the whole file from lossy-decoded, newline-normalised lines — apply against the original bytes (splice only the changed lines), or refuse files that contain `\r` or invalid UTF-8 — high
  - Scenario: `lines_at` uses `from_utf8_lossy(..).lines()`, which drops `\r` and turns invalid UTF-8 into U+FFFD. `apply` then rebuilds the file with `out.join("\n")`.
  - Reproducer: put `"a\r\nb\r\n"` on the head branch, suggest `x` on line 1, apply. The commit turns the file into `"x\nb\n"`, rewriting every line ending. A Latin-1 file gets every non-UTF-8 byte replaced.
  - The acceptance criteria ask for "one commit"; this is one commit with a whole-file diff.
- MEDIUM — `src/api/reviews.rs:271-285` — A reply can target another user's pending thread, because the `in_reply_to` lookup doesn't use the `VISIBLE` filter that `find` uses — add `AND (review_id IS NULL OR EXISTS(pending-visible to u))` to that query — high
  - Reproducer: bob adds a pending comment with id X. carol POSTs `{in_reply_to: X, body:"hi"}` and gets 200 instead of "no such thread".
  - Consequences: carol can probe ids for hidden comments, and she can write into bob's unsubmitted review. The criterion says pending reviews are visible only to their author.
- LOW — `src/api/reviews.rs:429` — `can_apply` ignores the pull request's state, so merged and closed PRs show "Apply suggestion" and the click returns 400 ("isn't open") — add `&& issue.state == "open"` — high
- LOW — `static/js/review.js:1008-1009,1024` — "Suggest a change" pre-fills from diff hunks only, so a range that spans lines outside the hunks gets blank lines — applying that unedited replaces those real lines with empty ones — disable the button for a range with gaps, or fetch the real lines — medium
- LOW — `src/api/lifecycle.rs:763-771` — The account data export covers issue comments but not `reviews` or `review_comments`, so the new content is missing from "everything I wrote" — add two `rows(...)` sections — medium
- LOW — `static/js/pages/issue.js:834` — `.catch(() => null)` on the reviews fetch hides a failure: the page renders with no review UI and no message — toast the error, or let it surface — medium

## lens: operator

- The v15 migration is appended last, and `MIGRATIONS.len()` matches the `user_version` assertion at `src/db.rs:597`. This is fine.
- The README entry matches the behaviour: own-PR approval is refused, outdated threads are marked, apply is refused on stale lines.
- Pre-audit items:
  - The `console.log` calls in `scripts/ui-check.mjs` are dismissed. The file has 33 of them as its normal pass/fail output.
  - "Removed `load_issue`/`pull_status`" is dismissed. They were only made `pub(super)`, not removed.
  - The `innerHTML` use is dismissed. Interpolations go through the escaping `html` tag, and `raw` only wraps `renderMarkdown` output, as elsewhere in the repo.
- Hunks with no step named (`e2e.sh`, `issues.rs`, `mod.rs`, `db.rs`, css, `issue.js`, `issues.js`, `tests/api.rs`) all serve the feature, so the plan was incomplete rather than the diff scoped too wide.

## Acceptance criteria
1. Threads, resolve, pending visibility, submit, review state and outdated detection: met in code and covered by `review_threads_states_and_outdated_comments`. The reply leak above is the exception. I didn't run the tests.
2. Apply as one commit, refused on a stale head or a conflicting line: partly met. The `expect_old` check and the original-lines check are present and tested, but the CRLF and non-UTF-8 rewrite breaks "one commit" as a minimal change.
3. Files-changed UI and `scripts/e2e.sh`: not verifiable without running it. The viewed checkboxes already existed in `diffview.js:171`; they are client-side (localStorage) and not part of this change.

## Implicit decisions
- Users with only read access can approve, and the approval counts toward the badge.
- Approvals survive new pushes; nothing gets dismissed.
- Authors can't approve or request changes on their own PR.
- Outdated is judged by comparing line text at the same line number, so inserting lines above a comment marks it outdated.
- Comments and reviews are allowed on closed and merged PRs.
- Review comments are not covered by moderator hide/report.

## Not checked
Test runs, the Playwright UI check, the `lib.js` `html` escaping internals, and the `commit_files` implementation.

**Noticed:**
- Moderation (`src/api/moderation.rs`) can't hide or report review comments; worth its own task.
- `src/api/reviews.rs:517-525` — `submit` has no rate limit, unlike `comment`.
