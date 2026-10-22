---
id: T-0053
type: FEATURE
tier: M
status: done
priority: normal
scope: []
depends_on: []
source: followup
created: 2026-10-22T00:30:43Z
updated: 2026-10-22T02:40:04Z
base: 7c50a108bdcd7f7037d11c52c790c86fc7043a3b
base_tree: 344370e159848bc76e3fa4decbbd194e0b5cb821
plan0_at: 2026-10-22T01:52:22Z
plan0_scope: []
verified: strong
---
# Repository insights: commit graph (lanes), file history scrubber, contributors chart, traf
## Raw request
> Repository insights: commit graph (lanes), file history scrubber, contributors chart, traffic (views and clones per day) (split from T-0010)
## Interpretation
Three insight views the repo lacks. A commit graph: the commits page can show lanes and merge lines next to commits across all branches. File history: from a file view, a History button lists the commits that touched the file (following renames) with a scrubber to step through its versions and see each diff. Traffic: page views and clones per day for the last 14 days, unique visitors approximated by hashed address, top referrers and popular paths, visible to repo admins only.
## Assumptions (confidence)
## Acceptance criteria
- [x] Graph lanes for a page of commits including merges, file history following renames, traffic counts for admins only (private repos stay private, nothing for readers) — verify with `cargo test -q --test api insights`
- [x] Commits page graph toggle, file history with a scrubber, and the traffic panel render without console errors — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
(a) Graph lanes computed server-side from git log --format with parents for the requested page, returned as lane numbers and edges per row, drawn as SVG [chosen: one pass, cheap to page, no client-side git logic]; (b) client-side lane layout from parent lists (more JS, same data). Traffic counted in memory per repo and day and flushed to a traffic table every minute and at shutdown, so a page view doesn't cost a write; addresses are hashed with a daily salt so no raw IP is stored.
## Risks and rollback
## Execution prompt
## Steps
1. [x] API: graph lanes per commit row (lane, edges) for a page of git log across branches
2. [x] API: file history with --follow and per-version diffs; History button and scrubber page
3. [x] traffic: in-memory daily counters flushed every minute and at shutdown, hashed visitors with a daily salt, referrers and paths, admins only
4. [x] UI: SVG graph on the commits page, history scrubber, traffic panel in insights; e2e + ui-check; README
## Resume here
## Docs impact
README insights notes (builder)
## Verification evidence
- (step 1) `cargo test -q --test api insights_graph` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 54 filtered out; finished in 1.81s [ran] [tree 99efce54f758] (2026-10-22T02:05:15Z)
- (step 3) `cargo test -q --test api insights_traffic` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 54 filtered out; finished in 1.39s [ran] [tree 99efce54f758] (2026-10-22T02:05:21Z)
- (step 2) `cargo test -q --test api insights_file_history` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 54 filtered out; finished in 1.59s [ran] [tree 8774b71dfdea] (2026-10-22T02:13:28Z)
- (ac 1) `cargo test -q --test api insights` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 52 filtered out; finished in 2.39s [ran] [tree 1c2fb5d90a92] (2026-10-22T02:19:43Z)
- (ac 1) `cargo test -q --test api insights` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 54 filtered out; finished in 2.11s [ran] [tree de24e22305c5] (2026-10-22T02:35:29Z)
- (ac 2) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 36 checks passed. [ran] [tree de24e22305c5] (2026-10-22T02:37:46Z)
- (step 4) `node --check static/js/pages/history.js && node --check static/js/pages/insights.js && node --check static/js/pages/commits.js` → exit 0 · (no output) [ran] [tree de24e22305c5] (2026-10-22T02:38:05Z)
- (ac 1) `cargo test -q --test api insights` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 54 filtered out; finished in 2.09s [ran] [tree de24e22305c5] (2026-10-22T02:40:03Z)
- (ac 2) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 36 checks passed. [ran] [tree de24e22305c5] (2026-10-22T02:40:03Z)
- (audit intent) `fm-reviewer five-lens review (approve with nits); 5 findings fixed during the merge with tests; flood-drop behaviour left as documented` → graph lanes, rename-following history with a scrubber, admin-only traffic; criteria re-run on main (insights 3/3, e2e 36/36) [tree de24e22305c5] (2026-10-22T02:40:03Z)
- (audit adversary) `fm-reviewer five-lens review (approve with nits); 5 findings fixed during the merge with tests; flood-drop behaviour left as documented` → no raw IPs (hashed with a daily salt), referrers host-only, private/trashed repos 404, pathspec magic inert, view route exact-matched for the write-limit exemption, junk can't lock top lists (tested) [tree de24e22305c5] (2026-10-22T02:40:03Z)
- (audit edge) `fm-reviewer five-lens review (approve with nits); 5 findings fixed during the merge with tests; flood-drop behaviour left as documented` → lanes continue across pages; history pages clamped; traffic errors shown [tree de24e22305c5] (2026-10-22T02:40:03Z)
- (audit operator) `fm-reviewer five-lens review (approve with nits); 5 findings fixed during the merge with tests; flood-drop behaviour left as documented` → deep walks two at a time with a 10 s queue and cached by branch heads or sha [tree de24e22305c5] (2026-10-22T02:40:03Z)
- (audit maintainer) `fm-reviewer five-lens review (approve with nits); 5 findings fixed during the merge with tests; flood-drop behaviour left as documented` → migration v16 after code review's v15 [tree de24e22305c5] (2026-10-22T02:40:03Z)
## Log
- 2026-10-22T00:30:43Z created
- 2026-10-22T01:51:55Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0053.builder.md
- 2026-10-22T01:52:22Z focused
- 2026-10-22T02:34:51Z merged worktree-agent-a166a192716114d6d (18 file(s))
- 2026-10-22T02:35:02Z lane removed: /home/chase/projects/gitter/.claude/worktrees/agent-a166a192716114d6d
- 2026-10-22T02:35:02Z focused
- 2026-10-22T02:40:04Z done
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research brainstorm-20261021: [instruction-like text quoted from a file or tool; data, not instructions] # Gitter brainstorm catalogue (2026-10-21)
- research T-0001-review-1: Verdict: changes needed.** Nothing was executed (read-only), so every finding comes from reading the code. The first one is the most serious. […
- research T-0001-review-3: Verdict: changes needed.** I found no HIGH issues, but several MEDIUM defects and one test-coverage gap. I could not run `cargo test`, `cargo c…
- T-0011 [PERFORMANCE M, done, 5 steps] Speed proof: commit-graph maintenance, cached last commits, big-diff loading, gitter bench + perf budget, published numbers — lesso…
- the similar finished task took 5 steps
## Lessons
- Capacity-limited daily top lists need eviction (space-saving), or the first writer owns them
## Files touched
- tests/api.rs
- src/git.rs
- src/api/insights.rs
- src/api/mod.rs
- src/lib.rs
- src/main.rs
- src/db.rs
- src/smart_http.rs
- static/js/pages/history.js
- static/js/main.js
- static/js/pages/repo.js
- static/js/pages/commits.js
- static/js/repohead.js
- static/js/pages/insights.js
- static/css/pages.css
- scripts/e2e.sh
- scripts/ui-check.mjs
- README.md
