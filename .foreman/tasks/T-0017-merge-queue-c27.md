---
id: T-0017
type: FEATURE
tier: M
status: done
priority: normal
scope: []
depends_on: []
source: followup
created: 2026-10-21T17:01:51Z
updated: 2026-10-22T16:18:07Z
inferred_deps: [T-0007]
inferred_why: T-0007: merge queue needs required checks and auto-merge
group: CI gating and pipeline features
base: 8de42a03af2f3a7e062d2d11777dffda5fe1b57b
base_tree: d1109834611a09273b5b3830c64be35d37b4597d
plan0_at: 2026-10-22T15:22:32Z
plan0_scope: []
verified: strong
---
# Merge queue: pull requests merge one at a time after passing checks on the combined result
## Raw request
> Merge queue (C27)
## Interpretation
For a branch whose rule turns the merge queue on, writers 'Merge when ready (queue)' instead of merging. Entries are tested one at a time in order: Gitter makes the commit the merge would produce on top of the current base (merge commit or squash), puts it on a hidden ref (refs/gitter-queue/<id>), and runs CI and waits for statuses on it; when the rule's required checks pass, the base moves to exactly that tested commit (no untested merge ever lands) and the pull request is recorded as merged by whoever queued it; when a check fails, the entry leaves the queue with a comment saying which check failed, and the next entry starts. If the base moved meanwhile (a direct push by an admin), the entry is tested again. The queue is visible on the pull request (position, state) and on a per-repo queue page; writers can remove entries.
## Assumptions (confidence)
## Acceptance criteria
- [x] Queued PRs are tested on the combined commit, land in order only when required checks pass on it, a failing one leaves the queue with a comment and the next proceeds, a moved base retests, removing an entry works, readers can't queue — verify with `cargo test -q --test api merge_queue`
- [x] Queue button and state on the PR page, the rule's merge-queue option in settings, and the queue page render without console errors — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
(a) serial queue, one test commit at a time, advanced from the same triggers as auto-merge (job finished, status posted) plus the upkeep tick [chosen: simple and never merges untested code]; (b) batched speculative queue (test N PRs stacked, bisect on failure) — faster on busy repos, much more state; later if needed. CI on the queue commit is attributed to the head branch's last pusher (T-0007 trust rule), and runs with event 'merge_queue'.
## Risks and rollback
## Execution prompt
## Steps
1. [x] schema + rule option + enqueue/dequeue API
2. [x] worker: build test commit, start CI, advance on checks, land or eject
3. [x] UI: PR queue state, settings option, queue page
4. [x] README, e2e
## Resume here
<!-- auto -->
- 2026-10-22T15:24:54Z step 1/4: schema + rule option + enqueue/dequeue API
- git: main; 4 uncommitted: src/api/checks.rs, src/api/issues.rs, src/db.rs, src/protect.rs
- touched since last checkpoint: src/api/issues.rs, src/api/checks.rs, src/db.rs, src/protect.rs
## Docs impact
README: Merge queue paragraph under Branch rules
## Verification evidence
- (step 1) `cargo test -q --test api merge_queue` → ✗ exit 101 · test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 71 filtered out; finished in 1.96s / error: test failed, to rerun pass `--test api` [ran] [tree 71f819f5afde] (2026-10-22T15:33:14Z)
- (step 1) `cargo test -q --test api merge_queue` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 71 filtered out; finished in 5.48s [ran] [tree d4311fc2ab4e] (2026-10-22T15:38:39Z)
- (ac 1) `cargo test -q --test api merge_queue` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 71 filtered out; finished in 4.54s [ran] [tree d4311fc2ab4e] (2026-10-22T15:38:47Z)
- (step 2) `cargo test -q --test api merge_queue` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 71 filtered out; finished in 5.01s [ran] [tree d4311fc2ab4e] (2026-10-22T15:38:53Z)
- (step 3) `node --check static/js/pages/issue.js && node --check static/js/pages/issues.js && node --check static/js/pages/reposettings.js && grep -q 'ok merge queue' /tmp/claude-1000/-home-chase-projects-gitter/2e122f62-61e6-4fe5-b944-f5546ee8dfc9/scratchpad/e2e-main-t0017.log` → exit 0 · (no output) [ran] [tree 660d85dadee4] (2026-10-22T15:52:09Z)
- (step 4) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 42 checks passed. [ran] [tree 660d85dadee4] (2026-10-22T15:55:21Z)
- (ac 2) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 42 checks passed. [ran] [tree 660d85dadee4] (2026-10-22T15:58:04Z)
- (ac 1) `cargo test -q --test api merge_queue` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 71 filtered out; finished in 3.28s [ran] [tree d1ce1ec35b2d] (2026-10-22T16:12:09Z)
- (ac 2) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 42 checks passed. [ran] [tree d1ce1ec35b2d] (2026-10-22T16:14:58Z)
- (ac 1) `cargo test -q --test api merge_queue` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 71 filtered out; finished in 3.66s [ran] [tree d1ce1ec35b2d] (2026-10-22T16:17:51Z)
- (ac 2) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 42 checks passed. [ran] [tree d1ce1ec35b2d] (2026-10-22T16:17:51Z)
- (audit intent) `one fm-reviewer pass over all five lenses (saved as research T-0017-review); 9 findings, all fixed with test coverage in merge_queue test; full cargo test --no-fail-fast and e2e green` → met — PRs land one at a time only as the exact commit that passed; admins may skip (audited); README matches [tree d1ce1ec35b2d] (2026-10-22T16:17:51Z)
- (audit adversary) `one fm-reviewer pass over all five lenses (saved as research T-0017-review); 9 findings, all fixed with test coverage in merge_queue test; full cargo test --no-fail-fast and e2e green` → queue now implies pull requests only; queue refs hidden from clients, push webhooks and exports; non-admins can't skip; CI start failure ejects instead of passing [tree d1ce1ec35b2d] (2026-10-22T16:17:51Z)
- (audit edge) `one fm-reviewer pass over all five lenses (saved as research T-0017-review); 9 findings, all fixed with test coverage in merge_queue test; full cargo test --no-fail-fast and e2e green` → base moves retest; head pushes eject; errors eject the entry instead of stalling; one-hour timeout; dequeue serialised with landing; ff refused [tree d1ce1ec35b2d] (2026-10-22T16:17:51Z)
- (audit operator) `one fm-reviewer pass over all five lenses (saved as research T-0017-review); 9 findings, all fixed with test coverage in merge_queue test; full cargo test --no-fail-fast and e2e green` → queue shown on the pulls page and PR merge box; ejections leave a timeline note; skips in the security log [tree d1ce1ec35b2d] (2026-10-22T16:17:51Z)
- (audit maintainer) `one fm-reviewer pass over all five lenses (saved as research T-0017-review); 9 findings, all fixed with test coverage in merge_queue test; full cargo test --no-fail-fast and e2e green` → merge split into prepare_merge/land_merge reused by merge, auto-merge and the queue; one module src/mergequeue.rs; ponytail note on the global lock [tree d1ce1ec35b2d] (2026-10-22T16:17:51Z)
- (audit intent) `one fm-reviewer pass over all five lenses (saved as research T-0017-review); 9 findings, all fixed with test coverage in merge_queue test; full cargo test --no-fail-fast and e2e green` → met — PRs land one at a time only as the exact commit that passed; admins may skip (audited); README matches [tree d1ce1ec35b2d] (2026-10-22T16:18:07Z)
- (audit adversary) `one fm-reviewer pass over all five lenses (saved as research T-0017-review); 9 findings, all fixed with test coverage in merge_queue test; full cargo test --no-fail-fast and e2e green` → queue now implies pull requests only; queue refs hidden from clients, push webhooks and exports; non-admins can't skip; CI start failure ejects instead of passing [tree d1ce1ec35b2d] (2026-10-22T16:18:07Z)
- (audit edge) `one fm-reviewer pass over all five lenses (saved as research T-0017-review); 9 findings, all fixed with test coverage in merge_queue test; full cargo test --no-fail-fast and e2e green` → base moves retest; head pushes eject; errors eject the entry instead of stalling; one-hour timeout; dequeue serialised with landing; ff refused [tree d1ce1ec35b2d] (2026-10-22T16:18:07Z)
- (audit operator) `one fm-reviewer pass over all five lenses (saved as research T-0017-review); 9 findings, all fixed with test coverage in merge_queue test; full cargo test --no-fail-fast and e2e green` → queue shown on the pulls page and PR merge box; ejections leave a timeline note; skips in the security log [tree d1ce1ec35b2d] (2026-10-22T16:18:07Z)
- (audit maintainer) `one fm-reviewer pass over all five lenses (saved as research T-0017-review); 9 findings, all fixed with test coverage in merge_queue test; full cargo test --no-fail-fast and e2e green` → merge split into prepare_merge/land_merge reused by merge, auto-merge and the queue; one module src/mergequeue.rs; ponytail note on the global lock [tree d1ce1ec35b2d] (2026-10-22T16:18:07Z)
## Log
- 2026-10-21T17:01:51Z created
- 2026-10-22T15:22:27Z planned from capture
- 2026-10-22T15:22:32Z focused
- 2026-10-22T16:18:07Z done
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research T-0029-review: Verdict:** approve with nits. The rollback logic in `merge_pull` looks correct. I found one real regression in the `refs()` change.
- research T-0001-review-3: Verdict: changes needed.** I found no HIGH issues, but several MEDIUM defects and one test-coverage gap. I could not run `cargo test`, `cargo c…
- T-0007 [FEATURE M, done, 5 steps] CI trust and gating: sandboxed jobs (no host docker socket, cpu/mem/time limits, network p — lesson: Trust must follow who wrote the co…
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
- the similar finished task took 5 steps
## Lessons
- Builder lanes sharing the session scratchpad can clobber a fixed log path: give e2e logs a task-specific name
## Plan gaps
- failed first: step 1 schema + rule option + enqueue/dequeue API
## Files touched
- src/api/issues.rs
- src/protect.rs
- src/api/checks.rs
- src/db.rs
- src/mergequeue.rs
- src/ci/trigger.rs
- src/lib.rs
- tests/api.rs
- src/api/mod.rs
- src/ci/jobs.rs
- src/ci/mod.rs
- src/hooks.rs
- src/smart_http.rs
- static/js/pages/reposettings.js
- static/js/pages/issue.js
- static/js/pages/issues.js
- scripts/ui-check.mjs
- README.md
- src/api/lifecycle.rs
