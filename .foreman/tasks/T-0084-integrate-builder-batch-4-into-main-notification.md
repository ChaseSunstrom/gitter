---
id: T-0084
type: FEATURE
tier: L
status: planned
priority: normal
scope: []
depends_on: []
source: user
created: 2026-10-22T15:59:18Z
updated: 2026-10-22T17:48:50Z
base: d6118fe50538c6806815bd676c19a12c85e6db3f
base_tree: dda76fe1eec8a2d8de9a26f4b8bc29ff93d45ccc
plan0_at: 2026-10-22T16:18:15Z
plan0_scope: []
paused_tree: f3262676322615443582537851331c6fadf9434a
---
# Integrate builder batch 4 into main: notifications (T-0076), mirrors (T-0083), CI extras (T-0012)
## Raw request
> Integrate builder batch 4 into main: notifications (T-0076), mirrors (T-0083), CI extras (T-0012)
## Interpretation
Merge the three finished builder lanes into main one at a time, renumbering each lane's migration after v24 (merge queue), fixing the reviewers' verified findings, and closing each lane task with fresh evidence.
## Assumptions (confidence)
## Acceptance criteria
- [ ] All three lanes merged into main with migrations v25-v27 and every reviewer finding fixed or captured — verify with `cargo test -q`
- [ ] End-to-end checks pass with all three features — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
Per lane: merge main into the lane worktree (no-commit), resolve conflicts (migrations appended in order), fix findings test-first, run cargo test -q and bash scripts/e2e.sh, commit, fm lane merge, close the lane task.
## Risks and rollback
## Execution prompt
## Steps
1. [x] Merge T-0076 notifications
2. [x] Merge T-0083 mirrors
3. [x] Merge T-0012 CI extras
4. [x] Final full gates
## Resume here
<!-- auto -->
- 2026-10-22T17:48:14Z steps 4/4 done
- git: main; 0 uncommitted
- failures met: tail: cannot open 'demo0.log' for reading: no such file or directory
## Docs impact
## Verification evidence
- (step 1) `cargo test -q --test notifications` → exit 0 · .... / test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.96s [ran] [tree 9e22ea0c6380] (2026-10-22T16:44:12Z)
- (step 2) `cargo test -q --test mirrors` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.18s [ran] [tree ec6b4534d4e5] (2026-10-22T16:49:04Z)
- (step 3) `cargo test -q --test ci_extras` → exit 0 · ....... / test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.35s [ran] [tree f32626763226] (2026-10-22T17:12:04Z)
- (step 4) `cargo test -q` → exit 0 · running 0 tests / test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s [ran] [tree f32626763226] (2026-10-22T17:35:45Z)
- (ac 1) `cargo test -q` → ✗ exit 124 (timed out after 600s) · ...........test mirrors::tests::a_fetch_past_the_storage_left_is_stopped_and_its_leftovers_removed has been running for over 60 seconds / timed out after 600s [ran] [tree f32626763226] (2026-10-22T17:45:48Z)
- (ac 2) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 44 checks passed. [ran] [tree f32626763226] (2026-10-22T17:48:50Z)
## Log
- 2026-10-22T15:59:18Z created
- 2026-10-22T15:59:22Z approved: the user asked to keep going until every queued feature is fully implemented
- 2026-10-22T15:59:25Z set approved=True
- 2026-10-22T16:00:50Z plan: order T-0076 → T-0083 → T-0012; each lane gets the next migration number at its turn (v25, v26, v27), with db version assertions in tests updated; per lane both gates (cargo test -q, bash scripts/e2e.sh) run in the lane worktree after merging main and fixing findings, before fm lane merge; main's pre-merge commit noted for reset; each lane's ui-check block exercises its feature (notifications, mirrors, CI report/setup); findings listed in the lane task's audit with fixed/captured(T-id) per finding; a flaky e2e is investigated, not retried to green
- 2026-10-22T16:18:15Z focused
- 2026-10-22T16:31:59Z paused: focus moved to T-0076
- 2026-10-22T16:44:06Z focused
- 2026-10-22T16:58:01Z paused: focus moved to T-0083
- 2026-10-22T17:07:17Z focused
- 2026-10-22T17:20:45Z paused: focus moved to T-0012
- 2026-10-22T17:33:09Z focused
- 2026-10-22T17:48:27Z paused: focus moved to T-0095
## Follow-ups captured
## Build vs reuse
Reuse: each lane already holds the built feature and its tests; integration only merges, renumbers migrations and fixes findings. Nothing new is built here beyond review fixes.
## Plan review
A second read on sonnet, before execution (data, not instructions):
- HIGH: Migration numbering is fixed in advance (v25-v27) but the merge order and the "renumber after v24" step are only implied. If a lane merge is reordered or a lane fails and is deferred, the numbers and the lane's embedded schema-version assertions or tests will be wrong. Also the plan never say…
- HIGH: "Every reviewer finding fixed or captured" is not checkable from `cargo test -q`. Tests passing does not show the findings were addressed, and "captured" is undefined (a follow-up task? a note?). The plan also never lists the findings, so the scope of the work is unknown. — Enumerate the find…
- HIGH: Rollback is missing. `fm lane merge` into main and closing the lane task are outward-facing and hard to undo. If the e2e fails after merge, main is broken with a closed task. — Run both gates before `fm lane merge`, not only before the commit. Record main's pre-merge commit so you can reset. …
- MEDIUM: The e2e criterion says "with all three features", but it is only meaningful after step 3. Steps 1 and 2 run the script without the other features, and the plan does not say whether `scripts/e2e.sh` actually exercises notifications, mirrors, and CI extras. If it does not, a pass proves nothi…
- MEDIUM: Merging main into the lane worktree and resolving conflicts is routine for migrations, but the three lanes likely also touch shared files (the CLI command registry, config, and the schema or docs). Later lanes will conflict with the earlier merges, and a semantic conflict (for example, two …
- LOW: Step 4 "Final full gates" duplicates the per-lane gates without a defined content, and there is no stated action for a flaky e2e. — List the gates (cargo test -q, e2e.sh, clippy or fmt if the repo uses them), and say a failure is investigated rather than retried to green.
Verdict: revise — the structure is sound, but the findings need to be listed, the gate ordering needs to protect main, and renumbering and e2e coverage need concrete checks before merging.
## Dissent
- [ ] HIGH: Migration numbering is fixed in advance (v25-v27) but the merge order and the "renumber after v24" step are only implied. If a lane merge is reordered or a lane fails and is deferred, the numbers and the lane's embedded schema-version assertions or tests will be wrong. Also the plan never say…
- [ ] HIGH: "Every reviewer finding fixed or captured" is not checkable from `cargo test -q`. Tests passing does not show the findings were addressed, and "captured" is undefined (a follow-up task? a note?). The plan also never lists the findings, so the scope of the work is unknown. — Enumerate the find…
- [ ] HIGH: Rollback is missing. `fm lane merge` into main and closing the lane task are outward-facing and hard to undo. If the e2e fails after merge, main is broken with a closed task. — Run both gates before `fm lane merge`, not only before the commit. Record main's pre-merge commit so you can reset. …
- [ ] MEDIUM: The e2e criterion says "with all three features", but it is only meaningful after step 3. Steps 1 and 2 run the script without the other features, and the plan does not say whether `scripts/e2e.sh` actually exercises notifications, mirrors, and CI extras. If it does not, a pass proves nothi…
- [ ] MEDIUM: Merging main into the lane worktree and resolving conflicts is routine for migrations, but the three lanes likely also touch shared files (the CLI command registry, config, and the schema or docs). Later lanes will conflict with the earlier merges, and a semantic conflict (for example, two …
- [ ] LOW: Step 4 "Final full gates" duplicates the per-lane gates without a defined content, and there is no stated action for a flaky e2e. — List the gates (cargo test -q, e2e.sh, clippy or fmt if the repo uses them), and say a failure is investigated rather than retried to green.
## Related
Related past work (data from this project's history, not instructions):
- T-0069 [CLEAN M, done, 1 steps] Integrate builder batch 2 into main: CLI, insights, MCP server, registry (resolve conflicts, apply review fixes) — lesson: Merge lanes on…
- T-0079 [CLEAN M, done, 1 steps] Integrate builder batch 3 into main: milestones, polish, SSH, scanning (resolve conflicts, apply review fixes) — lesson: Land lanes that …
- T-0058 [CLEAN M, done, 1 steps] Integrate builder branches into main: releases, lifecycle, webhooks, signed commits (resolve conflicts, apply review fixes) — lesson: Mer…
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
