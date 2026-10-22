---
id: T-0106
type: CLEAN
tier: S
status: done
priority: normal
scope: []
depends_on: []
source: user
created: 2026-10-22T19:56:42Z
updated: 2026-10-22T20:19:34Z
base: 37fdf8fda25a5a79c3fe468fe7fe5a0db99c33b4
base_tree: aab89a87e56025c1235c21a75acc9cb6b790248c
plan0_at: 2026-10-22T19:56:42Z
plan0_scope: []
verified: strong
---
# Integrate builder lanes T-0061 (releases upkeep) and T-0062 (lifecycle follow-ups)
## Raw request
> Integrate builder lanes T-0061 (releases upkeep) and T-0062 (lifecycle follow-ups)
## Interpretation
## Assumptions (confidence)
## Acceptance criteria
- [x] Both lanes merged into main with the full suite green — verify with `cargo test -q`
- [x] Browser checks stay green after the merge — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
## Risks and rollback
## Execution prompt
## Steps
1. [x] merge main into each lane, resolve conflicts, run tests there
2. [x] fm lane merge each, then close T-0061 and T-0062 with fresh evidence
## Resume here
## Docs impact
none: no behaviour beyond the two lanes, which have their own docs notes
## Verification evidence
- (ac 1) `cargo test -q` → exit 0 · running 0 tests / test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s [ran] [tree c531d63fd4fa] (2026-10-22T20:13:26Z)
- (step 1) `cargo test -q --test api releases lifecycle` → ✗ exit 1 · Usage: cargo test [OPTIONS] [TESTNAME] [-- [ARGS]...] / For more information, try '--help'. [ran] [tree c531d63fd4fa] (2026-10-22T20:13:27Z)
- (step 2) `cargo test -q --test api lifecycle_settings` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 76 filtered out; finished in 1.18s [ran] [tree c531d63fd4fa] (2026-10-22T20:13:28Z)
- (step 1) `cargo test -q --test api releases` → exit 0 · .... / test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 74 filtered out; finished in 1.50s [ran] [tree c531d63fd4fa] (2026-10-22T20:13:32Z)
- (ac 2) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 44 checks passed. [ran] [tree c531d63fd4fa] (2026-10-22T20:16:02Z)
- (ac 1) `cargo test -q` → exit 0 · running 0 tests / test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s [ran] [tree c531d63fd4fa] (2026-10-22T20:19:34Z)
- (ac 2) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 44 checks passed. [ran] [tree c531d63fd4fa] (2026-10-22T20:19:34Z)
- (audit intent) `integration only: each lane reviewed by fm-reviewer before merge; one import conflict in settings.js resolved by taking both import lists; full suite green in each lane and on main; e2e 44 checks on main` → both lanes landed with their review fixes [tree c531d63fd4fa] (2026-10-22T20:19:34Z)
- (audit edge) `integration only: each lane reviewed by fm-reviewer before merge; one import conflict in settings.js resolved by taking both import lists; full suite green in each lane and on main; e2e 44 checks on main` → T-0062 lane re-tested after T-0061 landed on main (combined tree green) [tree c531d63fd4fa] (2026-10-22T20:19:34Z)
- (audit self) `integration only: each lane reviewed by fm-reviewer before merge; one import conflict in settings.js resolved by taking both import lists; full suite green in each lane and on main; e2e 44 checks on main` → no findings [tree c531d63fd4fa] (2026-10-22T20:19:34Z)
## Log
- 2026-10-22T19:56:42Z created
- 2026-10-22T19:56:42Z focused
- 2026-10-22T20:00:06Z paused: focus moved to T-0061
- 2026-10-22T20:12:15Z re-based: what other work changed while it was paused isn't its own change
- 2026-10-22T20:12:15Z focused
- 2026-10-22T20:19:34Z done
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- T-0058 [CLEAN M, done, 1 steps] Integrate builder branches into main: releases, lifecycle, webhooks, signed commits (resolve conflicts, apply review fixes) — lesson: Mer…
- T-0062 [CLEAN S, active, 3 steps] Lifecycle follow-ups: free-disk check before building an export (refuse below ~2x the acco
- T-0084 [FEATURE L, done, 4 steps] Integrate builder batch 4 into main: notifications (T-0076), mirrors (T-0083), CI extras (T-0012) — lesson: Merge lanes one at a time a…
- T-0069 [CLEAN M, done, 1 steps] Integrate builder batch 2 into main: CLI, insights, MCP server, registry (resolve conflicts, apply review fixes) — lesson: Merge lanes on…
- similar past tasks were L: consider --tier L
## Behaviour lock
none: integration only; the full suite on main before and after both merges is the lock (green both times)
## Lessons
- Run lane integration commands from the main checkout with git -C; cd into a lane makes the guard treat the session as that builder.
## Plan gaps
- failed first: step 1 merge main into each lane, resolve conflicts, run tests there
## Files touched
- src/api/releases.rs
- src/main.rs
- src/ops.rs
- tests/api.rs
