---
id: T-0054
type: FEATURE
tier: M
status: done
priority: normal
scope: []
depends_on: []
source: followup
created: 2026-10-22T00:30:44Z
updated: 2026-10-22T14:13:22Z
base: c98598a3f97854ba515454171266c58bb57112be
base_tree: 70ab0046da16893dbc169e9f38fcdde3e2502d10
plan0_at: 2026-10-22T03:32:32Z
plan0_scope: []
verified: strong
---
# Polish: rich viewers, web revert, drag-and-drop upload, PWA, gitter demo
## Raw request
> Polish: rich viewers (CSV table, PDF, notebook, 3D/STL, audio/video), revert a commit or PR from the web, drag-and-drop upload anywhere, PWA manifest, demo seed mode (gitter demo) (split from T-0010)
## Interpretation
Polish that makes the forge pleasant. Rich file viewers in the blob view: CSV/TSV as a sortable table (capped rows), PDF inline in a sandboxed frame, Jupyter notebooks rendered (markdown cells via the existing sanitized renderer, code cells highlighted, text and image outputs; never run scripts or HTML outputs unsanitized), STL (ascii and binary) 3D models with orbit controls drawn with plain WebGL or canvas (no three.js unless justified), audio and video players, image diffs with side-by-side, swipe and onion-skin modes. Revert a commit or a merged PR from the web: creates a revert branch and opens a revert PR. Drag and drop files anywhere on a repo code page to upload (goes through the existing upload/commit flow). A PWA manifest and icons so it installs as an app. 'gitter demo' seeds an empty data dir with sample users, repos, issues, PRs, CI runs and wiki pages.
## Assumptions (confidence)
## Acceptance criteria
- [x] Revert a commit and a merged PR from the web: a revert branch and PR appear with the inverse change; conflicts give a clear error — verify with `cargo test -q --test polish`
- [x] PWA manifest and icons served; gitter demo seeds users, repos, issues, PRs and wiki into an empty data dir and refuses a non-empty one — verify with `cargo test -q --test polish demo`
- [x] CSV table, PDF frame, notebook, STL viewer, audio/video and image diff modes render without console errors; drag-and-drop upload works — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
Viewers are client-side modules loaded on demand by file extension, fed by the existing raw endpoint (CSP-safe, no inline scripts). Revert via git (revert on a temp index with commit_files/merge-tree style plumbing, no working tree). Demo seed reuses the bench seed helpers if present.
## Risks and rollback
## Execution prompt
## Steps
1. [x] viewers: CSV, PDF, notebook, media
2. [x] STL viewer and image diff modes
3. [x] revert commit/PR API + UI
4. [x] drag-and-drop upload, PWA manifest
5. [x] gitter demo, README, e2e
## Resume here
## Docs impact
README rich viewers, revert, PWA, gitter demo
## Verification evidence
- (ac 1) `cargo test -q --test polish 2>&1 | tail -60` → exit 0 · ....... / test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.42s [ran] [tree 982cce2be039] (2026-10-22T03:43:44Z)
- (step 1) `cargo test -q --test polish 2>&1 | tail -60` → exit 0 · ....... / test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.42s [ran] [tree 982cce2be039] (2026-10-22T03:43:44Z)
- (step 3) `cargo test -q --test polish revert` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 1.97s [ran] [tree 982cce2be039] (2026-10-22T03:43:52Z)
- (step 4) `bash scripts/e2e.sh > /tmp/claude-1000/-home-chase-projects-gitter/2e122f62-61e6-4fe5-b944-f5546ee8dfc9/scratchpad/t0054-e2e.log 2>&1` → exit 0 · (no output) [ran] [tree 099c9c54f4e7] (2026-10-22T03:50:03Z)
- (step 5) `cargo test -q --test polish demo` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 5.11s [ran] [tree c16979ccc3b5] (2026-10-22T03:52:36Z)
- (step 4) `cargo test -q --test polish` → exit 0 · ....... / test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.99s [ran] [tree c16979ccc3b5] (2026-10-22T03:52:48Z)
- (step 2) `cargo test -q --test api 2>&1 | tail -5` → exit 0 · . / test result: ok. 69 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 87.14s [ran] [tree c16979ccc3b5] (2026-10-22T03:54:49Z)
- (ac 1) `cargo test -q --test polish 2>&1 | tail -2` → exit 0 · test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.88s [ran] [tree 11876dac3df6] (2026-10-22T03:58:36Z)
- (ac 1) `cargo test -q --test polish` → exit 0 · ....... / test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.89s [ran] [tree 8791274d97c6] (2026-10-22T14:07:55Z)
- (ac 2) `cargo test -q --test polish demo` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 3.08s [ran] [tree 8791274d97c6] (2026-10-22T14:07:58Z)
- (ac 3) `bash scripts/e2e.sh 2>&1 | tail -1` → exit 0 · All 39 checks passed. [ran] [tree 8791274d97c6] (2026-10-22T14:10:25Z)
- (step 1) `node --check static/js/viewers.js` → exit 0 · (no output) [ran] [tree 8791274d97c6] (2026-10-22T14:10:25Z)
- (step 2) `node --check static/js/diffview.js` → exit 0 · (no output) [ran] [tree 8791274d97c6] (2026-10-22T14:10:25Z)
- (step 3) `cargo test -q --test polish` → exit 0 · ....... / test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.28s [ran] [tree 8791274d97c6] (2026-10-22T14:10:46Z)
- (step 4) `cargo test -q --test polish` → exit 0 · ....... / test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.47s [ran] [tree 8791274d97c6] (2026-10-22T14:10:51Z)
- (step 5) `cargo test -q --test polish` → exit 0 · ....... / test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.65s [ran] [tree 8791274d97c6] (2026-10-22T14:10:56Z)
- (ac 1) `cargo test -q --test polish` → exit 0 · ....... / test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.07s [ran] [tree 8791274d97c6] (2026-10-22T14:13:21Z)
- (ac 2) `cargo test -q --test polish demo` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 2.72s [ran] [tree 8791274d97c6] (2026-10-22T14:13:21Z)
- (ac 3) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 39 checks passed. [ran] [tree 8791274d97c6] (2026-10-22T14:13:21Z)
- (audit intent) `fm-reviewer five-lens review: changes needed; fixed in the lane merge (PDF only when real, tighter CSP; CSV column cap; demo bound to localhost with 96-bit password; revert cleanup, 12-char names, confirm dialogs); e2e cleanup race fixed` → all three criteria pass on main: polish 7/7, demo 3/3, e2e 39/39 [tree 8791274d97c6] (2026-10-22T14:13:21Z)
- (audit adversary) `fm-reviewer five-lens review: changes needed; fixed in the lane merge (PDF only when real, tighter CSP; CSV column cap; demo bound to localhost with 96-bit password; revert cleanup, 12-char names, confirm dialogs); e2e cleanup race fixed` → no viewer runs pushed content; PDFs inline only when the bytes start with %PDF- and the CSP forbids forms and base; the andon was answered by keeping inline PDFs (Chrome can't show sandboxed ones) with those limits [tree 8791274d97c6] (2026-10-22T14:13:21Z)
- (audit edge) `fm-reviewer five-lens review: changes needed; fixed in the lane merge (PDF only when real, tighter CSP; CSV column cap; demo bound to localhost with 96-bit password; revert cleanup, 12-char names, confirm dialogs); e2e cleanup race fixed` → CSV 1000 rows x 200 columns; STL 64 MB / 1M triangles; revert conflicts give a clear 409 with no branch left [tree 8791274d97c6] (2026-10-22T14:13:21Z)
## Log
- 2026-10-22T00:30:44Z created
- 2026-10-22T03:31:36Z planned from capture
- 2026-10-22T03:32:04Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0054.builder.md
- 2026-10-22T03:32:32Z focused
- 2026-10-22T14:07:21Z merged worktree-agent-a80bb66476735a10e (25 file(s))
- 2026-10-22T14:07:27Z lane removed: /home/chase/projects/gitter/.claude/worktrees/agent-a80bb66476735a10e
- 2026-10-22T14:07:29Z focused
- 2026-10-22T14:13:22Z done
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- T-0010 [FEATURE M, dropped, 0 steps] Depth: import by URL + GitHub issues, pull/push mirrors, releases with assets, commit grap
- research brainstorm-20261021: [instruction-like text quoted from a file or tool; data, not instructions] # Gitter brainstorm catalogue (2026-10-21)
- research T-0049-review-3: Verdict: changes needed.** There is one real data-corruption path in apply-suggestion and one visibility hole on replies. The rest is minor. [s…
- playbook code-reviewer (skills/playbooks/references/review/code-reviewer.md)
## Lessons
- A cleanup trap must wait for the server it killed before deleting its data folder, or background CI writes race the rm
## Files touched
- tests/polish.rs
- src/git.rs
- src/api/revert.rs
- src/api/mod.rs
- src/api/code.rs
- src/web.rs
- ANDON.md
- static/icons/icon-192.png
- static/icons/icon-512.png
- static/icons/apple-touch-icon.png
- static/manifest.webmanifest
- src/lib.rs
- static/index.html
- src/demo.rs
- src/main.rs
- static/js/viewers.js
- static/js/pages/repo.js
- static/js/diffview.js
- static/js/pages/history.js
- static/js/pages/compare.js
- static/js/pages/issue.js
- static/js/pages/commit.js
- static/css/code.css
- scripts/ui-check.mjs
- README.md
- src/api/checks.rs
- src/api/ci.rs
- src/api/issues.rs
- src/api/lifecycle.rs
- src/api/milestones.rs
