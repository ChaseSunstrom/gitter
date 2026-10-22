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
updated: 2026-10-22T16:00:50Z
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
1. [ ] Merge T-0076 notifications  <- CURRENT
2. [ ] Merge T-0083 mirrors
3. [ ] Merge T-0012 CI extras
4. [ ] Final full gates
## Resume here
## Docs impact
## Verification evidence
## Log
- 2026-10-22T15:59:18Z created
- 2026-10-22T15:59:22Z approved: the user asked to keep going until every queued feature is fully implemented
- 2026-10-22T15:59:25Z set approved=True
- 2026-10-22T16:00:50Z plan: order T-0076 → T-0083 → T-0012; each lane gets the next migration number at its turn (v25, v26, v27), with db version assertions in tests updated; per lane both gates (cargo test -q, bash scripts/e2e.sh) run in the lane worktree after merging main and fixing findings, before fm lane merge; main's pre-merge commit noted for reset; each lane's ui-check block exercises its feature (notifications, mirrors, CI report/setup); findings listed in the lane task's audit with fixed/captured(T-id) per finding; a flaky e2e is investigated, not retried to green
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
