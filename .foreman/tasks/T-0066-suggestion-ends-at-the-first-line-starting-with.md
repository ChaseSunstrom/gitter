---
id: T-0066
type: FIX
tier: S
status: dropped
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T02:17:09Z
updated: 2026-10-22T22:12:26Z
base: 6e1ef85012b118f0c31812348206b1ebc277b4cf
base_tree: f33bc7eef8ec3e3883ad8e7a3e322250ec4c6692
plan0_at: 2026-10-22T22:10:59Z
plan0_scope: []
---
# Review suggestions keep fenced code blocks: a suggestion ends at a closing fence as long as its opening one
## Raw request
> suggestion()` ends at the first line starting with a triple backtick, so a suggestion for a Markdown file with a fenced block is cut short
> (noticed by an agent; research T-0049-review-1)
## Interpretation
A review suggestion is a fenced block with the info string suggestion. The parser stops at the first line starting with three backticks, so a suggestion for Markdown containing its own fenced block is cut short. CommonMark: a fence closes only on a line of the same character, at least as long as the opener, with nothing else on it.
## Assumptions (confidence)
## Acceptance criteria
- [ ] a suggestion opened with four backticks can contain a triple-backtick block, and applying it commits all of it — verify with `cargo test -q --lib suggestions`
## Non-goals
## Approach (options → choice → why)
Options: (a) CommonMark fence matching in suggestion() (opener length n of backticks or tildes; closer: same char, length >= n, only whitespace after) [chosen: matches how the comment renders]; (b) render with the markdown library and read the code block back (heavier, a second parser); (c) document 'use tildes' (doesn't fix it). Check the web UI's suggestion rendering uses the same rule.
## Risks and rollback
## Execution prompt
## Steps
1. [x] test, then CommonMark fence matching in reviews::suggestion (and the UI's parser if it has one)
## Resume here
## Docs impact
## Verification evidence
- (step 1) `cargo test -q --lib suggestions` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 113 filtered out; finished in 0.00s [ran] [tree f33bc7eef8ec] (2026-10-22T22:12:01Z)
## Log
- 2026-10-22T02:17:09Z created
- 2026-10-22T22:09:59Z planned from capture
- 2026-10-22T22:10:31Z approved: the user asked to implement every queued feature ("what about all of the other features you have queued?"); proceeds under that standing request
- 2026-10-22T22:10:32Z set approved=True
- 2026-10-22T22:10:36Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0066.builder.md
- 2026-10-22T22:10:59Z focused
- 2026-10-22T22:12:26Z dropped: already handled: suggestion() closes only at a fence at least as long as its opener (since 982e7c7), covered by the suggestions_and_slices unit test
- 2026-10-22T22:12:26Z lane removed: /home/chase/projects/gitter/.claude/worktrees/agent-af6ac3350a97b8b0a
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research T-0049-review-1: Verdict:** changes needed [stale: src/api/reviews.rs, static/js/pages/issue.js, static/js/review.js changed since it was written]
- playbook Docs: task-first pages, examples that run (skills/playbooks/references/context/docs.md)
- research T-0006-review-1: Verdict: changes needed.** Four HIGH findings, each shown by a code path. I found them by reading, not by running anything. [stale: src/api/ci.…
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
