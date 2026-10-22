---
id: T-0094
type: FIX
tier: M
status: done
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T16:20:09Z
updated: 2026-10-22T19:10:39Z
base: 9ba422fe1b5640c1fa2b697817544eae1ed460be
base_tree: a1fae384c61815b8719852363a90f2f0c02a6a86
plan0_at: 2026-10-22T18:44:44Z
plan0_scope: []
verified: strong
---
# Notifications follow-ups (T-0076 review): mentions in 4-space-indented list items are skip
## Raw request
> Notifications follow-ups (T-0076 review): mentions in 4-space-indented list items are skipped as code; review_text reads only 200 line comments; prune old read/done rows and bound the repo facet query; fan-out holds the DB connection per watcher; tests for moderator-hidden issues, suspended users and the mention cap
## Interpretation
Follow-ups from the T-0076 review: (1) a mention in an indented list item ('    - cc @bob') is treated as an indented code block and doesn't notify: only skip 4-space-indented lines that start a code block (after a blank line, outside a list); (2) review_text reads only the first 200 line comments of a review, so later mentions are lost: read them all (they're capped elsewhere) or at least the mentions; (3) read and done notifications are kept forever: prune done ones older than 90 days and read ones older than 180 days in an hourly sweep, and bound the repository facet query; (4) tests for a moderator-hidden issue (no notification to non-moderators), a suspended user (never notified), and the per-author mention cap (MENTIONS_PER_HOUR).
## Assumptions (confidence)
## Acceptance criteria
- [x] Indented-list mentions notify, mentions past 200 review comments notify, old read/done rows are pruned, hidden issues and suspended users never notify, and the mention cap holds — verify with `cargo test -q --test notifications`
## Non-goals
## Approach (options → choice → why)
(a) small fixes in src/api/notifications.rs plus an hourly prune hooked into an existing upkeep loop [chosen: localized]; (b) a separate notifications worker (unneeded). Fan-out holding the DB per watcher stays as the ponytail note says.
## Risks and rollback
## Execution prompt
## Steps
1. [x] failing tests for the four follow-ups
2. [x] fixes and the hourly prune
## Resume here
## Docs impact
none: internal behavior; README's notifications section still holds
## Verification evidence
- (step 1) `CARGO_INCREMENTAL=0 cargo test -q --test notifications` → ✗ exit 101 · For more information about this error, try `rustc --explain E0425`. / error: could not compile `gitter` (test "notifications") due to 2 previous errors [ran] [tree 09f11912e75b] (2026-10-22T18:49:24Z)
- (step 2) `CARGO_INCREMENTAL=0 cargo test -q --test notifications` → exit 0 · ......... / test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.66s [ran] [tree 9c8d07d94cb4] (2026-10-22T18:52:05Z)
- (ac 1) `cargo test -q --test notifications` → exit 0 · ......... / test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.02s [ran] [tree 742a33e7c3f0] (2026-10-22T19:10:34Z)
- (step 2) `cargo test -q --test notifications` → exit 0 · ......... / test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.94s [ran] [tree 742a33e7c3f0] (2026-10-22T19:10:36Z)
- (ac 1) `cargo test -q --test notifications` → exit 0 · ......... / test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.84s [ran] [tree 742a33e7c3f0] (2026-10-22T19:10:39Z)
- (audit intent) `self-review of the 318-line lane diff (mention parsing for lists and code, per-comment review mentions, hourly sweep in batches, bounded repo facet, five tests); merged with main and the full suite passed in the lane` → met — the four T-0076 review follow-ups are fixed with tests [tree 742a33e7c3f0] (2026-10-22T19:10:39Z)
- (audit edge) `self-review of the 318-line lane diff (mention parsing for lists and code, per-comment review mentions, hourly sweep in batches, bounded repo facet, five tests); merged with main and the full suite passed in the lane` → sweep works in 10k-id batches and is idempotent; code indented inside list items now counts as prose (ponytail note); per-repo counts cover the newest 1,000 open notifications [tree 742a33e7c3f0] (2026-10-22T19:10:39Z)
## Log
- 2026-10-22T16:20:09Z created
- 2026-10-22T18:44:35Z set type=FIX
- 2026-10-22T18:44:36Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0094.builder.md
- 2026-10-22T18:44:44Z focused
- 2026-10-22T19:09:48Z merged worktree-agent-af9ba7a803b494cfb (3 file(s))
- 2026-10-22T19:09:49Z lane removed: /home/chase/projects/gitter/.claude/worktrees/agent-af9ba7a803b494cfb
- 2026-10-22T19:10:30Z focused
- 2026-10-22T19:10:39Z done
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research T-0076-review: Verdict: approve with nits.** I found no leak of private repository content, no cross-user access to notifications, and no way around the mention…
- research T-0049-review-2: Verdict: changes needed.** Two MEDIUM findings, one in the apply path and one in pending-review visibility. The rest are LOW. I did not run any…
- research T-0024-recon: 1. **Answer [stale: src/api/issues.rs, src/db.rs, src/hooks.rs changed since it was written]
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
## Why not caught
mentions() had no tests for nested lists or reviews past 200 comments; it has now
## Lessons
- Markdown-aware text scanning needs the list/paragraph context a line sits in, not just its indent
## Plan gaps
- failed first: step 1 failing tests for the four follow-ups
## Files touched
- tests/notifications.rs
- src/api/notifications.rs
- src/main.rs
- README.md
- src/api/ci.rs
- src/api/releases.rs
- src/api/repos.rs
- src/ci/compat.rs
- src/ci/config.rs
- src/ci/mod.rs
- src/ci/trigger.rs
- src/git.rs
- static/js/pages/ci.js
- tests/api.rs
- tests/ci.rs
