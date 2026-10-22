**Verdict: changes needed.** There is one real data-corruption path in apply-suggestion and one visibility hole on replies. The rest is minor.

Two of the pre-audit hits are dismissed. The `ui-check.mjs` "debug leftovers" are the script's normal pass/fail output. The `load_issue` and `pull_status` hits are just the new `pub(super)` visibility. innerHTML use goes through `html\`\`...\`.s`, which escapes interpolations. Markdown goes through `renderMarkdown`. I found no injection path.

## maintainer: changes needed

- MEDIUM — src/api/reviews.rs:222,624-632 — `apply` corrupts CRLF and non-UTF-8 files. `lines_at` runs `from_utf8_lossy(..).lines()`, which strips `\r` and turns invalid bytes into U+FFFD. `apply` then joins the whole file with `\n` and commits it. Reproducer: put `a\r\nb\r\n` on `feat`, add a suggestion on line 1, apply. The commit rewrites every line ending to LF. A Latin-1 file gets U+FFFD in place of its non-ASCII bytes. The web editor (`code.rs`) never does this. Fix: in `apply`, refuse (or splice at byte level) when the blob has `\r` or isn't valid UTF-8 (`String::from_utf8(..).ok()?`). Add a CRLF test. Confidence: high.
  - file: src/api/reviews.rs, line: 222, summary: apply rewrites CRLF files as LF and lossy-decodes non-UTF-8, so the commit corrupts the file, short_summary: apply-suggestion corrupts CRLF/non-UTF8 files, failure_scenario: CRLF file with a suggestion on line 1, apply, whole file is committed with LF endings, category: data-corruption, verdict: CONFIRMED

- MEDIUM — src/api/reviews.rs:273-284 — A reply's root lookup skips the `VISIBLE` filter that `find` and `list` use. Bob can POST `in_reply_to=<id of alice's pending comment>` (ids are sequential). He gets 200 instead of 404, and his reply has `review_id` NULL. It becomes public in alice's thread the moment she submits. Fix: reuse `VISIBLE`, joining `reviews v` and passing the user id, in that SELECT. Confidence: high.
  - file: src/api/reviews.rs, line: 273, summary: reply lookup ignores pending-review visibility, so strangers can attach replies to hidden threads, short_summary: reply to another user's pending thread allowed, failure_scenario: bob POSTs in_reply_to=alice's pending root id, gets 200 not 404, category: access-control, verdict: CONFIRMED

- MEDIUM — src/api/reviews.rs:201-213 — `pull()` calls `pull_status`, which runs `merge_base` and `merge_tree` for every open PR. `comment`, `list`, `submit` and `apply` only need the head and base shas. `list` runs on every PR page view, so each view adds one more merge-tree to the existing issue and diff calls. `pull_status` can also flip a PR to merged as a side effect. No before/after numbers were recorded. Fix: split out a sha-only helper in `issues.rs` and use it here. Confidence: medium.

- LOW — src/api/reviews.rs:463 — `submit` has no `limiter` check, unlike `comment` (line 266). A user with no pending review can insert unlimited "comment" reviews with a body, each with its own `record_event`. Fix: add the same limiter check. Confidence: high.

- LOW — src/api/reviews.rs:576 — Deleting the last comment of a pending review leaves an empty pending `reviews` row. The UI then shows "Add review comment" and "Discard 0 pending". Fix: delete the pending review when its last comment goes. Confidence: medium.

- LOW — static/js/pages/issue.js:834,840 — `.catch(() => null)` hides a failed reviews fetch, so the review UI vanishes with no signal. `rctx.mdCtx` duplicates the `mdCtx` defined a few lines later. Fix: log or toast on failure, and use `rctx = { api, n, isAuthor, mdCtx }`. Confidence: high.

- LOW — static/js/review.js:975,1009 — `lineText` only knows lines inside diff hunks. A shift-click range that spans a gap prefills the ```suggestion block with empty lines. Applying it would delete those lines. Fix: disable the suggest button when a line isn't found, or have the server prefill from `original`. Confidence: medium.

- LOW — tests/api.rs:1273 — `assert_ne!(.., OK)` is too loose. Applying twice should be asserted as `CONFLICT`. Missing tests: `DELETE reviews/pending`, reply to a pending thread, CRLF apply, anonymous GET, a literal pin of the JSON shape, and a reply cascade when its root is deleted (line 1243).

## Acceptance criteria
The brief states no criteria. Judged against the task title:
- Inline comments, threads, resolve, outdated and review submission (approve / request changes / comment): met, covered by `review_threads_states_and_outdated_comments`.
- Suggestions applied as commits: met, with the CRLF and non-UTF-8 gap above.
- Pending-review privacy: mostly met, with the reply hole above.
- Hunks in `issues.rs`, `mod.rs`, `db.rs`, the CSS, `issue.js`, `issues.js` and `tests/api.rs` serve the feature but no step names them. That is a plan gap, not scope creep.

## Implicit decisions
- Any reader of a repo can approve or request changes.
- Approvals are not invalidated by new pushes.
- Review state is informational only and does not gate merging.
- Comments are allowed on closed and merged PRs.
- Review rows are stored per submit, and the latest approve or changes decision per user wins.
- A suggestion applies the comment's current body, so an edit after posting changes what gets applied.
- Review cards in the timeline sort by `submitted_at`.

## Not checked
I did not run the test suite, the UI check or the migrations. I did not review `lib.js` escaping beyond reading `html()`. I did not check the diff endpoint's merge-base semantics or `validate::write_path`. I did not do a performance measurement.

Noticed: `pull_status` doing a `merge_tree` on every read (src/api/issues.rs:417) is a general hot-path cost worth a lighter variant.
