---
id: T-0041
type: FIX
tier: S
status: done
priority: normal
scope: []
depends_on: []
source: user
created: 2026-10-21T22:48:01Z
updated: 2026-10-21T22:49:06Z
base: 227e6fbf976f44e97b1b6257de947c13d77c3039
base_tree: 73c7dc89cd75b31e33f8047e80451740b6cca154
plan0_at: 2026-10-21T22:48:01Z
plan0_scope: []
verified: strong
---
# Stop tracking builder worktrees
## Raw request
> Stop tracking builder worktrees
## Interpretation
## Assumptions (confidence)
## Acceptance criteria
- [x] git ls-files .claude/worktrees prints nothing — verify with `test -z "$(git ls-files .claude/worktrees)"`
## Non-goals
## Approach (options → choice → why)
## Risks and rollback
## Execution prompt
## Steps
1. [x] untrack worktree gitlinks and ignore .claude/worktrees/
## Resume here
## Docs impact
## Verification evidence
- (step 1) `test -z "$(git ls-files .claude/worktrees)"` → exit 0 · (no output) [ran] [tree 4c21dfbab8ae] (2026-10-21T22:48:12Z)
- (ac 1) `test -z "$(git ls-files .claude/worktrees)"` → exit 0 · (no output) [ran] [tree 4c21dfbab8ae] (2026-10-21T22:48:12Z)
- (audit intent) `self` → only index entries removed; worktree folders on disk untouched; .gitignore stops re-adding [tree 4c21dfbab8ae] (2026-10-21T22:48:17Z)
- (ac 1) `test -z "$(git ls-files .claude/worktrees)"` → exit 0 · (no output) [ran] [tree 4c21dfbab8ae] (2026-10-21T22:48:17Z)
- (audit self) `self-review: index-only change` → no findings [tree 4c21dfbab8ae] (2026-10-21T22:48:17Z)
- (audit self) `self` → index-only change plus .gitignore line; folders on disk untouched [tree cb11f6223c09] (2026-10-21T22:48:23Z)
- (audit self) `self-review: index-only change` → no findings [tree cb11f6223c09] (2026-10-21T22:48:23Z)
- (audit self) `self` → index-only change plus .gitignore line; folders on disk untouched [tree b5756747856b] (2026-10-21T22:48:28Z)
- (audit self) `self-review: index-only change` → no findings [tree b5756747856b] (2026-10-21T22:48:28Z)
- (audit self) `self` → index-only change plus .gitignore line [tree 1f4ca986a368] (2026-10-21T22:48:43Z)
- (audit self) `self` → index-only change plus .gitignore line [tree 8ed730d04e04] (2026-10-21T22:48:51Z)
- (audit self) `self-review: index-only change` → no findings [tree 8ed730d04e04] (2026-10-21T22:48:51Z)
- (audit self) `self` → index-only change plus .gitignore line [tree 1b305cb2ca83] (2026-10-21T22:49:05Z)
- (audit self) `self-review: index-only change` → no findings [tree 1b305cb2ca83] (2026-10-21T22:49:05Z)
## Log
- 2026-10-21T22:48:01Z created
- 2026-10-21T22:48:01Z focused
- 2026-10-21T22:49:06Z done
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- playbook Terminal and mod UI (skills/playbooks/references/ui/terminal-ui.md)
- research T-0031-review: Verdict: changes needed.** Two verified scope escapes. The smart HTTP, `/raw` and `/archive` routes are fine, and so are the path tricks. [stale:…
- research brainstorm-20261021: [instruction-like text quoted from a file or tool; data, not instructions] # Gitter brainstorm catalogue (2026-10-21)
- research T-0004-review: Verdict: changes needed.** None of these block the four acceptance scripts. Findings 1, 2 and 3 break the documented Docker workflow. I verified …
## Regression test
none: repo hygiene (index entries), checked by the acceptance command; .gitignore prevents recurrence
## Lessons
- Builder worktrees under .claude/worktrees must be gitignored before any commit that stages broadly.
## Files touched
- .gitignore
- .claude/worktrees/agent-a0dc1e40f9e40af81
