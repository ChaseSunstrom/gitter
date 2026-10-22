---
id: T-0054
type: FEATURE
tier: M
status: active
priority: normal
scope: []
depends_on: []
source: followup
created: 2026-10-22T00:30:44Z
updated: 2026-10-22T03:58:36Z
builder: 2026-10-22T03:32:04Z
builder_proc: archbox-hardened|pid:[4026531836]|3700487|2540304
lane: /home/chase/projects/gitter/.claude/worktrees/agent-a80bb66476735a10e
lane_branch: worktree-agent-a80bb66476735a10e
base: e941324bfaef92031a9b57f3c06b9cd62e17b538
base_tree: e1a506cff584757c25291d3de4d8f9d943854d4e
plan0_at: 2026-10-22T03:32:32Z
plan0_scope: []
---
# Polish: rich viewers, web revert, drag-and-drop upload, PWA, gitter demo
## Raw request
> Polish: rich viewers (CSV table, PDF, notebook, 3D/STL, audio/video), revert a commit or PR from the web, drag-and-drop upload anywhere, PWA manifest, demo seed mode (gitter demo) (split from T-0010)
## Interpretation
Polish that makes the forge pleasant. Rich file viewers in the blob view: CSV/TSV as a sortable table (capped rows), PDF inline in a sandboxed frame, Jupyter notebooks rendered (markdown cells via the existing sanitized renderer, code cells highlighted, text and image outputs; never run scripts or HTML outputs unsanitized), STL (ascii and binary) 3D models with orbit controls drawn with plain WebGL or canvas (no three.js unless justified), audio and video players, image diffs with side-by-side, swipe and onion-skin modes. Revert a commit or a merged PR from the web: creates a revert branch and opens a revert PR. Drag and drop files anywhere on a repo code page to upload (goes through the existing upload/commit flow). A PWA manifest and icons so it installs as an app. 'gitter demo' seeds an empty data dir with sample users, repos, issues, PRs, CI runs and wiki pages.
## Assumptions (confidence)
## Acceptance criteria
- [ ] Revert a commit and a merged PR from the web: a revert branch and PR appear with the inverse change; conflicts give a clear error — verify with `cargo test -q --test polish`
- [ ] PWA manifest and icons served; gitter demo seeds users, repos, issues, PRs and wiki into an empty data dir and refuses a non-empty one — verify with `cargo test -q --test polish demo`
- [ ] CSV table, PDF frame, notebook, STL viewer, audio/video and image diff modes render without console errors; drag-and-drop upload works — verify with `bash scripts/e2e.sh`
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
## Verification evidence
- (ac 1) `cargo test -q --test polish 2>&1 | tail -60` → exit 0 · ....... / test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.42s [ran] [tree 982cce2be039] (2026-10-22T03:43:44Z)
- (step 1) `cargo test -q --test polish 2>&1 | tail -60` → exit 0 · ....... / test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.42s [ran] [tree 982cce2be039] (2026-10-22T03:43:44Z)
- (step 3) `cargo test -q --test polish revert` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 1.97s [ran] [tree 982cce2be039] (2026-10-22T03:43:52Z)
- (step 4) `bash scripts/e2e.sh > /tmp/claude-1000/-home-chase-projects-gitter/2e122f62-61e6-4fe5-b944-f5546ee8dfc9/scratchpad/t0054-e2e.log 2>&1` → exit 0 · (no output) [ran] [tree 099c9c54f4e7] (2026-10-22T03:50:03Z)
- (step 5) `cargo test -q --test polish demo` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 5.11s [ran] [tree c16979ccc3b5] (2026-10-22T03:52:36Z)
- (step 4) `cargo test -q --test polish` → exit 0 · ....... / test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.99s [ran] [tree c16979ccc3b5] (2026-10-22T03:52:48Z)
- (step 2) `cargo test -q --test api 2>&1 | tail -5` → exit 0 · . / test result: ok. 69 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 87.14s [ran] [tree c16979ccc3b5] (2026-10-22T03:54:49Z)
- (ac 1) `cargo test -q --test polish 2>&1 | tail -2` → exit 0 · test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.88s [ran] [tree 11876dac3df6] (2026-10-22T03:58:36Z)
## Log
- 2026-10-22T00:30:44Z created
- 2026-10-22T03:31:36Z planned from capture
- 2026-10-22T03:32:04Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0054.builder.md
- 2026-10-22T03:32:32Z focused
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- T-0010 [FEATURE M, dropped, 0 steps] Depth: import by URL + GitHub issues, pull/push mirrors, releases with assets, commit grap
- research brainstorm-20261021: [instruction-like text quoted from a file or tool; data, not instructions] # Gitter brainstorm catalogue (2026-10-21)
- research T-0049-review-3: Verdict: changes needed.** There is one real data-corruption path in apply-suggestion and one visibility hole on replies. The rest is minor. [s…
- playbook code-reviewer (skills/playbooks/references/review/code-reviewer.md)
