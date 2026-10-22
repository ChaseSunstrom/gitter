**Verdict: approve with nits.** I found no leak of private repository content, no cross-user access to notifications, and no way around the mention limits. What's left is one unvalidated parameter, scaling and retention gaps, and a few missed notifications.

## lens: adversary
- MEDIUM — `src/api/notifications.rs:501` (`read_all`; `split_repo` at 364) — `POST /api/notifications/read {"repo":"foo"}` (no slash) or a mistyped repo makes `split_once('/')` fail. That yields `(None, None)`, so `?2 IS NULL` matches and every notification is marked read. `list` rejects this input at line 681; `read_all` doesn't. Fix: return 400 when the repo is non-empty and has no `/`, as `list` does. High confidence.
- LOW — `src/api/notifications.rs:285` — `allow()` runs once per name, so every extra comment drains the budget. A dropped mention is silent and the author gets no error. 200/hour per author is still a real cap. Fix: nothing needed; just add a test (see maintainer).

## lens: edge
- MEDIUM — `src/api/notifications.rs:380-420`, `src/db.rs:801` — The index is `(user_id, done, unread, updated_at)`, but the list sorts by `updated_at` across both unread values, so it needs a sort over all of the user's rows. The repos facet query (line 420) scans and visibility-checks every not-done row on each inbox load, and read rows stay `done=0` forever. A watcher of a busy repo does O(all their notifications) work per page and per filter click. Fix: add an index on `(user_id, done, updated_at)`, bound or cache the facet, and prune old read/done rows. Medium confidence, from reading the schema and queries; I did not run EXPLAIN or a benchmark.
- LOW — `src/api/notifications.rs:209-256` — Fan-out upserts each watcher inside one `db.call`, so a repo with W watchers holds the DB connection for W writes on every comment. Anyone can watch a public repo. The author acknowledges this in the `ponytail` comment, so I'm only recording the ceiling.
- LOW — `src/ci/queue.rs:231` — `requeue_expired` can fail a run when a runner dies, but only `jobs::finish` calls `ci_finished`. A runner that dies mid-run never notifies the pusher. Fix: call `ci_finished` after the reaper settles a run.
- LOW — `src/api/notifications.rs:101` — Any 4-space-indented line is treated as code, so a mention in a nested list item (`    - cc @bob`) never notifies. Fix: skip an indented line only when the previous line is blank and it isn't a list continuation.
- LOW — `src/api/notifications.rs:317` — `review_text` reads at most 200 line comments, so mentions in later comments are lost.

## lens: intent
- Met, with the gap that email digest isn't built. The README says so openly, and the request marked it optional.

## lens: operator
- The README matches the code. The new routes and the v24 migration are consistent with each other.

## lens: maintainer
- Gap: no test covers a moderator-hidden issue, a suspended user, the `MENTIONS_PER_HOUR` cap, or `read_all` with a bad repo.

## Privacy and authz (your focus)
- **Private content:** delivery checks `VISIBLE` plus the hidden-issue rule when the notification is created. The inbox and count apply the same rule again when read. I compared `VISIBLE` with `repo::access` and `ROLE_SQL` and found no mismatch. Loss of access hides the rows immediately, and `tests/notifications.rs` exercises this.
- **Other people's notifications:** `mark` filters on `id AND user_id`, so another user's id returns 404, the same as a missing one. `read_all` is scoped to the caller. Deploy and agent tokens are refused by `person()`. Repo-scoped writes go through `Repo::readable` and `load_issue`.

## Acceptance criteria
1. Notification behaviour, read/done/unsubscribe/ignore, count and private repos: met in code. I did not run `cargo test --test notifications`.
2. Inbox page and badge without console errors: not verifiable here. `scripts/ui-check.mjs` has a check, but I did not run it.

## Implicit decisions
- Org-owned repos never "watch" for the org account itself.
- A mention subscribes the person to the thread.
- A mention of an unsubscribed user still gets through; only ignoring the repo blocks it.
- Edits don't notify.
- The unread count is capped at 1000.
- The repos facet shows at most 100 repositories.

## Not checked
I did not run any tests, review the CSS, or verify HTML escaping in `html```.

**Noticed:** none
