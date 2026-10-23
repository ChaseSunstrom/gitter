---
id: T-0085
type: FIX
tier: S
status: done
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T15:59:34Z
updated: 2026-10-22T23:18:24Z
base: 6339a7eb39a0b3561f9b00937eaed4765e6da487
base_tree: a936b328b71e9b678215d2012065c3ae56b2ba86
plan0_at: 2026-10-22T23:08:14Z
plan0_scope: []
verified: strong
---
# UI markup: the Labels sidebar block gets its opening div, the Star button's aria-pressed says true or false
## Raw request
> UI markup fixes: issue.js Labels sidebar block lacks its opening side-block div; repohead.js aria-pressed=${meta.starred} renders empty because html drops booleans
## Interpretation
## Assumptions (confidence)
## Acceptance criteria
- [x] the issue sidebar's Labels block is its own side-block and the Star button has aria-pressed true/false — verify with `grep -q 'ok markup' /tmp/claude-1000/-home-chase-projects-gitter/2e122f62-61e6-4fe5-b944-f5546ee8dfc9/scratchpad/e2e-t0085.log && grep -q '^exit 0' /tmp/claude-1000/-home-chase-projects-gitter/2e122f62-61e6-4fe5-b944-f5546ee8dfc9/scratchpad/e2e-t0085.log`
- [x] the PR author who can't push to the fork sees no maintainer-edit toggle, the one who can does — verify with `cargo test -q --test forks`
## Non-goals
## Approach (options → choice → why)
## Risks and rollback
## Execution prompt
## Steps
1. [x] fix both in issue.js and repohead.js; a ui-check assertion for each
## Resume here
## Docs impact
none: markup and a hidden toggle; the README already says only someone who can push to the fork can allow edits
## Verification evidence
- (step 1) `cargo test -q --test forks maintainers` → red as expected (exit 101) · test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 3 filtered out; finished in 3.94s / error: test failed, to rerun pass `--test forks` [ran] [tree 1317051fbd23] (2026-10-22T23:09:05Z)
- (ac 1) `grep -q 'ok markup' /tmp/claude-1000/-home-chase-projects-gitter/2e122f62-61e6-4fe5-b944-f5546ee8dfc9/scratchpad/e2e-t0085.log && grep -q '^exit 0' /tmp/claude-1000/-home-chase-projects-gitter/2e122f62-61e6-4fe5-b944-f5546ee8dfc9/scratchpad/e2e-t0085.log` → exit 0 · (no output) [ran] [tree a7bedbe86fc2] (2026-10-22T23:17:25Z)
- (ac 2) `cargo test -q --test forks` → exit 0 · .... / test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.68s [ran] [tree a7bedbe86fc2] (2026-10-22T23:17:28Z)
- (step 1) `cargo test -q --test forks maintainers` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 3 filtered out; finished in 2.28s [ran] [tree a7bedbe86fc2] (2026-10-22T23:17:31Z)
- (ac 1) `grep -q 'ok markup' /tmp/claude-1000/-home-chase-projects-gitter/2e122f62-61e6-4fe5-b944-f5546ee8dfc9/scratchpad/e2e-t0085.log && grep -q '^exit 0' /tmp/claude-1000/-home-chase-projects-gitter/2e122f62-61e6-4fe5-b944-f5546ee8dfc9/scratchpad/e2e-t0085.log` → exit 0 · (no output) [ran] [tree a7bedbe86fc2] (2026-10-22T23:17:34Z)
- (ac 2) `cargo test -q --test forks` → exit 0 · .... / test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.65s [ran] [tree a7bedbe86fc2] (2026-10-22T23:17:34Z)
- (audit adversary) `self: markup fixes checked in a real browser by a new ui-check block (e2e exit 0); the toggle rule computed server-side per viewer (author and can push to the fork) and tested for the author who can, a maintainer, and an author who can't` → can_allow_edits is display only; update_pull still refuses a grant from someone who can't push to the fork (T-0051) [tree a7bedbe86fc2] (2026-10-22T23:17:34Z)
- (audit self) `self: markup fixes checked in a real browser by a new ui-check block (e2e exit 0); the toggle rule computed server-side per viewer (author and can push to the fork) and tested for the author who can, a maintainer, and an author who can't` → no findings [tree a7bedbe86fc2] (2026-10-22T23:17:34Z)
- `cargo test -q --test forks maintainers` → ✗ exit 101 · test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 3 filtered out; finished in 1.21s / error: test failed, to rerun pass `--test forks` (start tree + this task's tests) [ran] [tree a7bedbe86fc2] (2026-10-22T23:18:18Z)
- `cargo test -q --test forks maintainers` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 3 filtered out; finished in 2.51s [ran] [tree a7bedbe86fc2] (2026-10-22T23:18:18Z)
- (audit adversary) `self: markup fixes checked in a real browser by a new ui-check block (e2e exit 0); the toggle rule computed server-side per viewer (author and can push to the fork) and tested for the author who can, a maintainer, and an author who can't` → can_allow_edits is display only; update_pull still refuses a grant from someone who can't push to the fork (T-0051) [tree a7bedbe86fc2] (2026-10-22T23:18:24Z)
- (audit self) `self: markup fixes checked in a real browser by a new ui-check block (e2e exit 0); the toggle rule computed server-side per viewer (author and can push to the fork) and tested for the author who can, a maintainer, and an author who can't` → no findings [tree a7bedbe86fc2] (2026-10-22T23:18:24Z)
## Log
- 2026-10-22T15:59:34Z created
- 2026-10-22T23:08:13Z planned from capture
- 2026-10-22T23:08:14Z focused
- 2026-10-22T23:08:34Z scope: also T-0115 (same issue page: hide the 'Allow edits from maintainers' toggle from an author who can't push to the fork), one e2e run for both
- 2026-10-22T23:17:25Z criterion 1 verify: `node scripts/ui-check.mjs is covered by bash scripts/e2e.sh` → `grep -q 'ok markup' /tmp/claude-1000/-home-chase-projects-gitter/2e122f62-61e6-4fe5-b944-f5546ee8dfc9/scratchpad/e2e-t0085.log && grep -q '^exit 0' /tmp/claude-1000/-home-chase-projects-gitter/2e122f62-61e6-4fe5-b944-f5546ee8dfc9/scratchpad/e2e-t0085.log`
- 2026-10-22T23:18:24Z done
- 2026-10-22T23:18:24Z committed 713cf0e (17 files)
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research T-0089-review-4: Verdict: approve with nits.** `static/css/account.css` is clean, with three small rules (lines 97-99) that reuse the existing tokens. The probl…
- playbook Interaction: feedback, keyboard, forms, words and the phone (skills/playbooks/references/ui/interaction.md)
- research T-0049-review-2: Verdict: changes needed.** Two MEDIUM findings, one in the apply path and one in pending-review visibility. The rest are LOW. I did not run any…
- research T-0001-review-3: Verdict: changes needed.** I found no HIGH issues, but several MEDIUM defects and one test-coverage gap. I could not run `cargo test`, `cargo c…
## Files touched
- tests/forks.rs
- src/api/issues.rs
- static/js/pages/issue.js
- static/js/repohead.js
- scripts/ui-check.mjs
