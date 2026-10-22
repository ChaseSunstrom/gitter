---
id: T-0095
type: FIX
tier: M
status: done
priority: urgent
scope: []
depends_on: []
source: user
created: 2026-10-22T17:46:23Z
updated: 2026-10-22T18:06:17Z
base: 6d2fc6bdba6070a3b06551fb8e4c8cc9ebfc9632
base_tree: f3262676322615443582537851331c6fadf9434a
plan0_at: 2026-10-22T17:48:27Z
plan0_scope: []
verified: strong
---
# FIX!: repo page layout breaks at desktop width: About sidebar overlaps the file list and '
## Raw request
> FIX!: repo page layout breaks at desktop width: About sidebar overlaps the file list and 'Add file', commit count overlaps the sidebar, repo tabs cut off ('Settir'), last-commit column stays as skeleton bars (user screenshot, 1767px wide, mirror of a 1,511-commit repo)
## Interpretation
Pages built on .stack (a CSS grid with an auto-sized column) grow to their widest unbreakable content — a long commit message in the file list, a long code line or table in a README — so the main column runs under the About sidebar and off the page. The repository tabs also overflow their row at 1280px with no hint (Settings shows as 'Settir'). Fix the layout so long content truncates or scrolls inside its box, and the tabs that don't fit go into a More menu.
## Assumptions (confidence)
## Acceptance criteria
- [x] At 1767, 1360 and 1024 px the repo page with long commit messages and a wide README keeps the main column left of the sidebar, and every repo tab is visible or in the More menu — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
(a) .stack column as minmax(0,1fr) plus min-width:0 on split columns [chosen: one root-cause rule fixes every page]; (b) per-component max-width/overflow patches (misses other pages); tabs: (a) a More menu for tabs that don't fit, measured on render and resize [chosen: every tab reachable, no hidden scroll]; (b) visible scrolling (easy to miss); (c) smaller padding (still breaks with more tabs).
## Risks and rollback
## Execution prompt
## Steps
1. [x] browser check that fails on overlap and hidden tabs
2. [x] CSS fix for .stack/.split; More menu for repo tabs
## Resume here
## Docs impact
none: layout bug fix
## Verification evidence
- (step 1) `bash scripts/e2e.sh` → ✗ exit 1 · git over SSH on 127.0.0.1:33681 / gitter 0.1.0 listening on 127.0.0.1:33680 — open http://127.0.0.1:33680 [ran] [tree 6e4dbdea82d7] (2026-10-22T17:51:28Z)
- (step 2) `grep -q 'ok long commit messages' /tmp/claude-1000/-home-chase-projects-gitter/2e122f62-61e6-4fe5-b944-f5546ee8dfc9/scratchpad/t0095-e2e.log` → exit 0 · (no output) [ran] [tree ab0b2719c3e7] (2026-10-22T17:57:10Z)
- (ac 1) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 44 checks passed. [ran] [tree ab0b2719c3e7] (2026-10-22T18:00:00Z)
- (audit intent) `screenshots at 1767/1360/1024 px of a wide README repo and the user's own repo mirrored into a demo` → met: main column stays left of the sidebar, long lines truncate or scroll inside their boxes, every repo tab is visible or under More [tree c8b3c402f915] (2026-10-22T18:00:30Z)
- (audit edge) `read the CSS and fitTabs paths; ui-check measures 4 widths incl. 800px single column` → root-cause .stack rule fixes every page using it; tabs refit on resize and after fonts load; the active tab is never hidden; if even that can't fit, the row still scrolls [tree c8b3c402f915] (2026-10-22T18:00:30Z)
- (ac 1) `node scripts/ui-check.mjs layout check via bash scripts/e2e.sh` → ✗ exit 1 · } / Node.js v26.10.0 [ran] [tree c8b3c402f915] (2026-10-22T18:00:39Z)
- (ac 1) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 44 checks passed. [ran] [tree c8b3c402f915] (2026-10-22T18:03:12Z)
- (ac 1) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 44 checks passed. [ran] [tree c8b3c402f915] (2026-10-22T18:06:16Z)
- (audit intent) `self-review with screenshots at four widths, including the user's own repository mirrored into a demo; ui-check layout block red before, green after` → met — the reported overlap, cut-off tabs and placeholder column all render correctly [tree c8b3c402f915] (2026-10-22T18:06:16Z)
- (audit edge) `self-review with screenshots at four widths, including the user's own repository mirrored into a demo; ui-check layout block red before, green after` → one root-cause rule for every .stack page; tabs refit on resize and font load; active tab always shown [tree c8b3c402f915] (2026-10-22T18:06:16Z)
## Log
- 2026-10-22T17:46:23Z created
- 2026-10-22T17:46:35Z set type=FIX, priority=urgent
- 2026-10-22T17:48:27Z focused
- 2026-10-22T18:06:17Z done
- 2026-10-22T18:06:17Z committed 4f2566e (15 files)
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- playbook Motion: choreography, timing and reduced motion (skills/playbooks/references/ui/motion.md)
- research T-0006-review-2: Verdict: changes needed.** The feature matches the request and the README. The operator-facing gaps are the unbounded built-in runner access, a…
- research T-0021-review: Verdict: changes needed.** The route allow-list, repo scoping, token handling, JSON-RPC parsing and the default-branch rule held up against every…
- research T-0006-review-1: Verdict: changes needed.** Four HIGH findings, each shown by a code path. I found them by reading, not by running anything. [stale: src/api/ci.…
## Regression test
scripts/ui-check.mjs 'Layout' block (run by bash scripts/e2e.sh): failed before the fix (step 1 evidence, exit 1; the repro measured content running to 2554px past the sidebar at 1220px) and passes after (step 2 and criterion evidence). fm doesn't count .mjs browser checks as test files.
## Why not caught
the browser checks only looked for console errors and expected text, never for elements overlapping; the new ui-check layout block measures boxes at four widths with long content
## Lessons
- A CSS grid without explicit columns sizes its one column to the widest unbreakable content: give single-column grids minmax(0, 1fr)
## Plan gaps
- failed first: step 1 browser check that fails on overlap and hidden tabs
## Files touched
- scripts/ui-check.mjs
- static/css/app.css
- static/js/repohead.js
