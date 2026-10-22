---
id: T-0053
type: FEATURE
tier: M
status: active
priority: normal
scope: []
depends_on: []
source: followup
created: 2026-10-22T00:30:43Z
updated: 2026-10-22T02:19:43Z
builder: 2026-10-22T01:51:55Z
builder_proc: archbox-hardened|pid:[4026531836]|3700487|2540304
lane: /home/chase/projects/gitter/.claude/worktrees/agent-a166a192716114d6d
lane_branch: worktree-agent-a166a192716114d6d
base: 2bb252e71be01028b31a177b03c5a288117f0a05
base_tree: 57244d31b37e6b4daafae41eed3fe73bdb336cca
plan0_at: 2026-10-22T01:52:22Z
plan0_scope: []
---
# Repository insights: commit graph (lanes), file history scrubber, contributors chart, traf
## Raw request
> Repository insights: commit graph (lanes), file history scrubber, contributors chart, traffic (views and clones per day) (split from T-0010)
## Interpretation
Three insight views the repo lacks. A commit graph: the commits page can show lanes and merge lines next to commits across all branches. File history: from a file view, a History button lists the commits that touched the file (following renames) with a scrubber to step through its versions and see each diff. Traffic: page views and clones per day for the last 14 days, unique visitors approximated by hashed address, top referrers and popular paths, visible to repo admins only.
## Assumptions (confidence)
## Acceptance criteria
- [ ] Graph lanes for a page of commits including merges, file history following renames, traffic counts for admins only (private repos stay private, nothing for readers) — verify with `cargo test -q --test api insights`
- [ ] Commits page graph toggle, file history with a scrubber, and the traffic panel render without console errors — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
(a) Graph lanes computed server-side from git log --format with parents for the requested page, returned as lane numbers and edges per row, drawn as SVG [chosen: one pass, cheap to page, no client-side git logic]; (b) client-side lane layout from parent lists (more JS, same data). Traffic counted in memory per repo and day and flushed to a traffic table every minute and at shutdown, so a page view doesn't cost a write; addresses are hashed with a daily salt so no raw IP is stored.
## Risks and rollback
## Execution prompt
## Steps
1. [x] API: graph lanes per commit row (lane, edges) for a page of git log across branches
2. [x] API: file history with --follow and per-version diffs; History button and scrubber page
3. [x] traffic: in-memory daily counters flushed every minute and at shutdown, hashed visitors with a daily salt, referrers and paths, admins only
4. [ ] UI: SVG graph on the commits page, history scrubber, traffic panel in insights; e2e + ui-check; README  <- CURRENT
## Resume here
## Docs impact
## Verification evidence
- (step 1) `cargo test -q --test api insights_graph` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 54 filtered out; finished in 1.81s [ran] [tree 99efce54f758] (2026-10-22T02:05:15Z)
- (step 3) `cargo test -q --test api insights_traffic` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 54 filtered out; finished in 1.39s [ran] [tree 99efce54f758] (2026-10-22T02:05:21Z)
- (step 2) `cargo test -q --test api insights_file_history` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 54 filtered out; finished in 1.59s [ran] [tree 8774b71dfdea] (2026-10-22T02:13:28Z)
- (ac 1) `cargo test -q --test api insights` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 52 filtered out; finished in 2.39s [ran] [tree 1c2fb5d90a92] (2026-10-22T02:19:43Z)
## Log
- 2026-10-22T00:30:43Z created
- 2026-10-22T01:51:55Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0053.builder.md
- 2026-10-22T01:52:22Z focused
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research brainstorm-20261021: [instruction-like text quoted from a file or tool; data, not instructions] # Gitter brainstorm catalogue (2026-10-21)
- research T-0001-review-1: Verdict: changes needed.** Nothing was executed (read-only), so every finding comes from reading the code. The first one is the most serious. […
- research T-0001-review-3: Verdict: changes needed.** I found no HIGH issues, but several MEDIUM defects and one test-coverage gap. I could not run `cargo test`, `cargo c…
- T-0011 [PERFORMANCE M, done, 5 steps] Speed proof: commit-graph maintenance, cached last commits, big-diff loading, gitter bench + perf budget, published numbers — lesso…
- the similar finished task took 5 steps
