---
id: T-0076
type: FEATURE
tier: M
status: active
priority: normal
scope: []
depends_on: []
source: followup
created: 2026-10-22T03:31:47Z
updated: 2026-10-22T15:40:21Z
builder: 2026-10-22T15:21:11Z
builder_proc: archbox-hardened|pid:[4026531836]|4747|8111
lane: /home/chase/projects/gitter/.claude/worktrees/agent-afc5f07f48dba2abf
lane_branch: worktree-agent-afc5f07f48dba2abf
base: 8de42a03af2f3a7e062d2d11777dffda5fe1b57b
base_tree: d1109834611a09273b5b3830c64be35d37b4597d
plan0_at: 2026-10-22T15:21:47Z
plan0_scope: []
---
# Notifications inbox: participating, mentions, assignments, watching; unread count
## Raw request
> Notifications inbox (split from T-0050): /notifications for participating, mentioned, assigned and watched threads; watch/unwatch/ignore a repo, subscribe/unsubscribe a thread; mark read/done; unread count in the top bar; @mentions in issue, PR, comment and discussion bodies notify the mentioned person if they can read the repo; email digest optional
## Interpretation
An inbox at /notifications for threads (issues, pull requests, discussions, CI runs that fail on your pushes) you take part in, are @mentioned or assigned in, or watch. Watch, unwatch or ignore a repository (watching = all its new issues/PRs/discussions); subscribe or unsubscribe a single thread. Mark read, mark done, mark all read; filters (unread, participating, mentions, repository). Unread count in the top bar (cheap: one indexed count, polled or refreshed on navigation). @username mentions in issue, PR, review comment, comment and discussion bodies notify that person only if they can read the repository (never leak private repos), with a per-author rate limit so mentions can't spam. Agents' actions notify like their owner's. Email delivery is out of scope unless SMTP config already exists.
## Assumptions (confidence)
## Acceptance criteria
- [ ] Mentions, participation, assignment and watching create one notification per thread per person (only for people who can read the repo); read/done/unsubscribe/ignore work; unread count is right; private repos never notify outsiders — verify with `cargo test -q --test notifications`
- [ ] The /notifications page with filters and the top-bar unread badge render without console errors; marking read updates the badge — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
(a) a notifications table (user_id, thread kind/id, repo_id, reason, unread, updated_at, last_actor) upserted per thread per user when an event happens, plus thread_subscriptions and repo_watches tables [chosen: GitHub's model, one row per thread per user]; (b) computing the inbox at read time from events (no writes, but expensive queries and no read state). Fan-out happens in one place called from issue/comment/review/discussion/merge handlers; readability is checked with repo::access per recipient.
## Risks and rollback
## Execution prompt
## Steps
1. [x] schema + fan-out helper with access checks
2. [x] hook fan-out into issues, comments, reviews, discussions, merges, CI failures
3. [x] API: list/filter/mark read/done, watch/subscribe
4. [x] UI: inbox page, badge, watch and subscribe buttons
5. [x] README, e2e
## Resume here
## Docs impact
## Verification evidence
- (step 1) `CARGO_INCREMENTAL=0 cargo test -q --test notifications 2>&1 | grep -v "^\s*$" | tail -40` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.04s [ran] [tree 11ca177af0e7] (2026-10-22T15:32:58Z)
- (step 1) `CARGO_INCREMENTAL=0 cargo test -q --test notifications` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.78s [ran] [tree 11ca177af0e7] (2026-10-22T15:33:39Z)
- (step 2) `CARGO_INCREMENTAL=0 cargo test -q --test notifications` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.91s [ran] [tree 11ca177af0e7] (2026-10-22T15:33:46Z)
- (step 3) `CARGO_INCREMENTAL=0 cargo test -q --lib notifications` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 91 filtered out; finished in 0.01s [ran] [tree 11ca177af0e7] (2026-10-22T15:33:46Z)
- (step 4) `CARGO_INCREMENTAL=0 cargo test -q --test notifications 2>&1 | grep -v "^\s*$" | tail -20` → exit 0 · .... / test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.06s [ran] [tree 806a346f021d] (2026-10-22T15:39:41Z)
- (step 4) `node --check static/js/pages/notifications.js` → exit 0 · (no output) [ran] [tree 755c2e037f72] (2026-10-22T15:40:18Z)
- (step 5) `node --check scripts/ui-check.mjs` → exit 0 · (no output) [ran] [tree 755c2e037f72] (2026-10-22T15:40:21Z)
## Log
- 2026-10-22T03:31:47Z created
- 2026-10-22T15:21:06Z planned from capture
- 2026-10-22T15:21:11Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0076.builder.md
- 2026-10-22T15:21:47Z focused
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research brainstorm-20261021: [instruction-like text quoted from a file or tool; data, not instructions] # Gitter brainstorm catalogue (2026-10-21)
- research T-0016-review: HIGH — src/api/snippets.rs:137,261 and src/api/wiki.rs:148 — snippets and wikis bypass the storage quota [stale: src/api/discussions.rs, src/api/…
- T-0050 [FEATURE M, done, 5 steps] Issue tracking depth: milestones, issue/PR templates, assignees — lesson: Builders writing tests into their own test file (tests/issues…
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
- the similar finished task took 5 steps
