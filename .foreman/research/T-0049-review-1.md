**Verdict:** changes needed

I read the diff and the surrounding code. I did not run the tests.

## lens: adversary
- HIGH — src/api/reviews.rs:222,624-631 — `apply` rebuilds the whole file from `from_utf8_lossy(..).lines()` and joins it with `\n`. A CRLF file loses every `\r`, so one suggestion rewrites every line. Any non-UTF-8 byte (Latin-1 text) becomes U+FFFD. Fix: refuse files that are not valid UTF-8, or splice the suggestion into the raw bytes by line offsets and keep each line's original ending. Add a CRLF test. Confidence: high.
  - Proof: a blob `a\r\nb\r\n` with a suggestion on line 1 commits `X\nb\n`. A blob containing byte `0xE9` commits `EF BF BD` in its place.
- MEDIUM — src/api/reviews.rs:605-644,564 — `apply` commits the comment's current body, not the one the writer saw. A read-only user can edit a harmless suggestion after a writer opens the page (the UI shows no change until refresh), then the writer's click commits the new text under the writer's name. Fix: the client sends `updated_at` (or a body hash) with apply, and the server returns 409 on mismatch. Confidence: high.
  - Repro: bob POSTs a suggestion, carol (write) GETs the reviews, bob PATCHes the body to `evil`, carol POSTs apply. The commit contains `evil`.
- MEDIUM — src/api/reviews.rs:295-303 — The "That file isn't in this pull request" check only tests that the file exists at head or the merge base. A reader can anchor a comment, including a suggestion, on any file such as a CI config. That thread never shows in the diff, so a writer sees it only in the Conversation tab. Fix: require the path in `git diff --name-only mb head`. Confidence: medium.
- LOW — src/api/reviews.rs:273-284 — The `in_reply_to` lookup skips the `VISIBLE` check. Carol can reply to Bob's pending thread by guessing its id. The id lookup acts as an oracle (200 or 400), and her public reply lands in his draft. Fix: join `reviews` and apply `VISIBLE` in that query. Confidence: high.

## lens: edge
- LOW — src/api/reviews.rs:231-241,625 — An empty suggestion and a one-blank-line suggestion both return `Some("")`, and `"".lines()` yields nothing. Suggesting a blank line therefore deletes the line, and trailing blank lines are dropped. Fix: return `Vec<String>` from `suggestion` and keep blank lines. Confidence: high.
- LOW — static/js/pages/issue.js:834 — `.catch(() => null)` hides a failed reviews fetch. The review UI, reviewers block and threads vanish with no message. Fix: toast on error. Confidence: high.
- LOW — src/api/reviews.rs:417-420,359 — `list` runs one `git cat-file` per distinct (sha, path) with an outdated thread. It has no pagination and no cap on comments per PR. Fix: use one `git diff`/batch call, and cap or paginate. Confidence: medium.
- LOW — static/js/review.js:974-977,1009,1024 — A shift-click range that spans lines outside the rendered hunks makes `lineText` return `''` for those lines. "Suggest a change" then pre-fills blank lines, and posting it would replace real lines with blanks. Fix: disable the suggest button when a line is missing from the hunks. Confidence: medium.
- LOW — static/js/review.js:1066 — `ap.disabled=true` is never reset on error, so the button stays dead until refresh. Fix: re-enable it in a `finally`. Confidence: high.

## Acceptance criteria
No criteria were in the brief beyond the task title. Inline comments, threads, review states, suggestions and the list badge are implemented, and tests/api.rs exercises them. Verified only by reading.

## Implicit decisions
- Any reader can comment and approve; nothing gates merging.
- Outdated means the text at the same line numbers differs.
- Reviews are allowed on closed and merged PRs.
- Deleting a thread's first comment deletes the whole thread; deleting a user deletes their comments, including others' replies under them.
- A comment is rate-limited to 60 per 10 minutes per user.
- Suggestions apply under the applier's name.

## Not checked
CSS, the ui-check and e2e runs, mdeditor/markdown sanitization (`renderMarkdown`), the Rust compile, and the account-export coverage of review rows.

Noticed: account export and user deletion (T-0033) may not cover the new `reviews` and `review_comments` tables.
Noticed: `suggestion()` ends at the first line starting with a triple backtick, so a suggestion for a Markdown file with a fenced block is cut short.
