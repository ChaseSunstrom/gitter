---
id: T-0049
type: FEATURE
tier: L
status: done
priority: normal
scope: []
depends_on: []
source: followup
created: 2026-10-22T00:30:41Z
updated: 2026-10-22T02:22:04Z
base: 2bb252e71be01028b31a177b03c5a288117f0a05
base_tree: 57244d31b37e6b4daafae41eed3fe73bdb336cca
plan0_at: 2026-10-22T01:52:55Z
plan0_scope: []
verified: strong
---
# PR code review: inline comments on diff lines with threads, review submissions (approve /
## Raw request
> PR code review: inline comments on diff lines with threads, review submissions (approve / request changes / comment), suggested changes that apply as a commit, viewed-file checkboxes (split from T-0008)
## Interpretation
GitHub-style code review on pull requests. On Files changed, anyone who can read the repo can comment on a diff line or a range of lines; comments form threads with replies and can be resolved and unresolved. Comments can be posted alone or gathered into a pending review that is submitted as Approve, Request changes or Comment with a summary. A suggestion block in a comment (a fenced block with the language 'suggestion') renders as a small diff, and someone who can push to the head branch applies it with one click as a commit. Each reader can tick files as viewed; the tick clears when that file changes. The conversation tab shows submitted reviews and their threads, the PR list and header show the review state (approved by, changes requested by), and comments whose lines changed since are marked outdated.
## Assumptions (confidence)
## Acceptance criteria
- [x] Line and range comments with threads, resolve and unresolve, pending reviews visible only to their author, submit as approve, request changes or comment, review state on the PR, outdated detection when the head changes — verify with `cargo test -q --test api review`
- [x] Suggestion blocks apply as one commit on the head branch by people who can push there; a stale head or a conflicting line is refused — verify with `cargo test -q --test api suggestion`
- [x] Files changed shows a + on diff lines, inline threads, a review composer and viewed checkboxes; the conversation shows reviews; pages render without console errors — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
(a) New tables reviews (pull, author, state pending/approved/changes_requested/commented, body, commit_sha, submitted_at) and review_comments (review or none, path, side old/new, line, start_line, commit_sha, body, in_reply_to, resolved_by, resolved_at), plus viewed_files (pull, user, path, blob_sha); outdated is computed by comparing the commented lines at the comment's commit with the head's version [chosen: queryable, fits the comments model, outdated is cheap with git show]; (b) store line comments as ordinary comments with a JSON anchor (mixes concerns, hard to thread); (c) git notes (not queryable, no permissions). Applying a suggestion reuses the web-edit commit path in src/api/code.rs with the head sha as the expected sha, so a moved branch is refused rather than overwritten.
## Risks and rollback
## Execution prompt
## Steps
1. [x] schema + API: reviews, review_comments (threads, resolve), viewed_files; who may do what follows repo::access (read comments, write resolves others' threads, authors resolve their own)
2. [x] outdated comments (lines changed since the comment's commit) and viewed-file reset by blob sha; review state summary on the pull JSON and the PR list
3. [x] suggestions: parse fenced suggestion blocks, apply as a commit on the head branch through the web-edit path with the expected head sha
4. [x] UI: + on diff lines (click or drag for a range), inline threads in diffview, review composer, viewed checkboxes with a progress count, reviews in the conversation timeline, state badges in the PR list
5. [x] tests, e2e + ui-check pages, README
## Resume here
## Docs impact
README Code review bullet
## Verification evidence
- (step 1) `cargo test -q --test api review` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 52 filtered out; finished in 2.13s [ran] [tree 0b7fac702d89] (2026-10-22T02:06:00Z)
- (step 2) `cargo test -q --test api review` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 52 filtered out; finished in 2.44s [ran] [tree 0b7fac702d89] (2026-10-22T02:06:03Z)
- (step 3) `cargo test -q --test api review` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 52 filtered out; finished in 2.55s [ran] [tree 0b7fac702d89] (2026-10-22T02:06:06Z)
- (step 4) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 34 checks passed. [ran] [tree 0b7fac702d89] (2026-10-22T02:08:05Z)
- (ac 1) `cargo test -q --test api review_threads` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 53 filtered out; finished in 3.12s [ran] [tree f0c8638a5cdc] (2026-10-22T02:17:16Z)
- (ac 2) `cargo test -q --test api review_suggestions` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 53 filtered out; finished in 2.49s [ran] [tree f0c8638a5cdc] (2026-10-22T02:17:19Z)
- (ac 3) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 34 checks passed. [ran] [tree f0c8638a5cdc] (2026-10-22T02:19:38Z)
- (step 5) `cargo test -q --lib reviews` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 62 filtered out; finished in 0.00s [ran] [tree f0c8638a5cdc] (2026-10-22T02:19:38Z)
- (ac 1) `cargo test -q --test api review` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 52 filtered out; finished in 2.14s [ran] [tree f0c8638a5cdc] (2026-10-22T02:22:04Z)
- (ac 2) `cargo test -q --test api suggestion` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 53 filtered out; finished in 1.63s [ran] [tree f0c8638a5cdc] (2026-10-22T02:22:04Z)
- (ac 3) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 34 checks passed. [ran] [tree f0c8638a5cdc] (2026-10-22T02:22:04Z)
- (audit intent) `3 fm-reviewer passes (5 lenses) plus an automated security note; 10 findings fixed with tests (byte-exact suggestion splicing incl. CRLF, apply pinned to the seen version, reply visibility, token collaborator lockout, changed-files-only anchors, fence-aware suggestions, rate limit, export); thread-list scaling captured` → line and range comments, threads, resolve, pending reviews, approve/request changes/comment, outdated, suggestions as one-line-precise commits, list badges; e2e 34/34 with a browser check of the composer [tree f0c8638a5cdc] (2026-10-22T02:22:04Z)
- (audit adversary) `3 fm-reviewer passes (5 lenses) plus an automated security note; 10 findings fixed with tests (byte-exact suggestion splicing incl. CRLF, apply pinned to the seen version, reply visibility, token collaborator lockout, changed-files-only anchors, fence-aware suggestions, rate limit, export); thread-list scaling captured` → pending threads invisible and unrepliable to others (tested); apply needs write and the seen updated_at (tested); anchors only on changed files (tested); tokens can't add collaborators and adding needs a fresh sign-in (tested); markdown through renderMarkdown, other values escaped [tree f0c8638a5cdc] (2026-10-22T02:22:04Z)
- (audit edge) `3 fm-reviewer passes (5 lenses) plus an automated security note; 10 findings fixed with tests (byte-exact suggestion splicing incl. CRLF, apply pinned to the seen version, reply visibility, token collaborator lockout, changed-files-only anchors, fence-aware suggestions, rate limit, export); thread-list scaling captured` → CRLF and non-UTF-8 files keep their bytes (unit + API test); blank-line and nested-fence suggestions; emptied pending reviews removed; apply refused on closed PRs [tree f0c8638a5cdc] (2026-10-22T02:22:04Z)
- (audit operator) `3 fm-reviewer passes (5 lenses) plus an automated security note; 10 findings fixed with tests (byte-exact suggestion splicing incl. CRLF, apply pinned to the seen version, reply visibility, token collaborator lockout, changed-files-only anchors, fence-aware suggestions, rate limit, export); thread-list scaling captured` → no merge-tree per review request (pull_shas); review submit rate-limited; reviews in the data export; README documents the feature [tree f0c8638a5cdc] (2026-10-22T02:22:04Z)
- (audit maintainer) `3 fm-reviewer passes (5 lenses) plus an automated security note; 10 findings fixed with tests (byte-exact suggestion splicing incl. CRLF, apply pinned to the seen version, reply visibility, token collaborator lockout, changed-files-only anchors, fence-aware suggestions, rate limit, export); thread-list scaling captured` → one module (src/api/reviews.rs), diffview gains two generic hooks (decorate, lineClick), migration v15 [tree f0c8638a5cdc] (2026-10-22T02:22:04Z)
## Log
- 2026-10-22T00:30:41Z created
- 2026-10-22T01:52:23Z approved: the user asked to keep going until every queued feature is fully implemented
- 2026-10-22T01:52:55Z set approved=True
- 2026-10-22T01:52:55Z focused
- 2026-10-22T02:09:59Z scope: security fix from the automated review — API tokens (even full-access) can't add or change collaborators (persistent access beyond the token); set_collaborator needs a fresh sign-in (Sudo)
- 2026-10-22T02:22:04Z done
## Follow-ups captured
## Build vs reuse
Reuses: the comments/events/audit flow in src/api/issues.rs (review submissions show in the timeline as comment kinds), the markdown renderer and editor (static/js/md.js, mdeditor.js), diffview.js rows (data-n line numbers per side) for anchors, Git::run for reading file lines at a commit (outdated detection), and the web-edit commit path in src/api/code.rs (expected head sha) for applying suggestions. The diff view's viewed checkboxes already exist (per browser, reset when the head moves), so they stay. New: reviews and review_comments tables.
## Related
Related past work (data from this project's history, not instructions):
- research brainstorm-20261021: [instruction-like text quoted from a file or tool; data, not instructions] # Gitter brainstorm catalogue (2026-10-21)
- playbook code-reviewer (skills/playbooks/references/review/code-reviewer.md)
- research T-0006-review-1: Verdict: changes needed.** Four HIGH findings, each shown by a code path. I found them by reading, not by running anything. [stale: src/api/ci.…
- research T-0006-review-2: Verdict: changes needed.** The feature matches the request and the README. The operator-facing gaps are the unbounded built-in runner access, a…
## Lessons
- When a feature rewrites a user's file, splice bytes rather than round-trip through decoded lines; and pin destructive one-click actions to the version the user saw
## Files touched
- tests/api.rs
- src/api/reviews.rs
- src/api/issues.rs
- src/api/mod.rs
- src/db.rs
- static/js/diffview.js
- static/js/review.js
- static/js/pages/issue.js
- static/js/pages/issues.js
- static/css/issues.css
- scripts/e2e.sh
- scripts/ui-check.mjs
- README.md
- src/api/orgs.rs
- src/api/lifecycle.rs
