---
id: T-0119
type: FIX
tier: M
status: done
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T22:55:46Z
updated: 2026-10-23T05:35:14Z
group: issues status read path
base: 48f35a58644ee68d48423053c84b1f1942cb3619
base_tree: 451753b883e72a281c6875c3800c9f34cf5fe458
plan0_at: 2026-10-23T04:43:08Z
plan0_scope: []
verified: strong
---
# Hand-merged pull requests are marked merged when the base branch moves, not by a read
## Raw request
> - `issues.rs:625-648`: a read-only status GET writes merged state, so the hand-merge path has side effects. Worth its own task
> (noticed by an agent; research T-0018-review-3)
## Interpretation
pull_status (src/api/issues.rs, the issue/status GET) notices a pull request whose head is already in its base (someone merged it by hand) and writes it as merged, with deploy::pull_closed. A read with side effects means nothing happens until someone looks, and every look pays for a merge-base. Do it when the base branch moves instead.
## Assumptions (confidence)
## Acceptance criteria
- [x] a pull request whose head is pushed into its base by hand is marked merged by that push (merged_sha, a timeline entry, previews retired), without anyone opening it — verify with `cargo test -q --test api hand_merged`
- [x] reading a pull request (GET issue or status) writes nothing — verify with `cargo test -q --test api hand_merged`
## Non-goals
## Approach (options → choice → why)
Options: (1) in hooks::on_push, for each moved branch, find open pull requests into it in this repository and mark those whose head is now contained as merged (same writes as today), then keep the read side pure; (2) keep the read-time write but guard it; (3) a periodic sweep. Choice: 1: it's where every branch move (git push, web edits, merges, mirror syncs via after_import) already goes; the read keeps reporting merged-looking state without writing.
## Risks and rollback
## Execution prompt
## Steps
1. [x] test: push the head into main with git, the PR is merged without a GET, and a GET of an untouched PR changes no rows; fix: on_push checks open pull requests into the pushed branch (hooks::on_push), and the status read only reports
## Resume here
## Docs impact
none: pull requests merged by hand now show as merged on their own, as people expect
## Verification evidence
- (step 1) `cargo test -q --test api hand_merged` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 89 filtered out; finished in 0.45s [ran] [tree 64d27a6494cd] (2026-10-23T04:50:28Z)
- (step 1) `cargo test -q --test api hand_merged` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 89 filtered out; finished in 0.42s [ran] [tree db7f652ba986] (2026-10-23T04:58:37Z)
- (step 1) `cargo test -q --test api hand_merged && cargo test -q --test deploy && cargo test -q --test api merge` → exit 0 · ........ / test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 83 filtered out; finished in 3.32s [ran] [tree e9d101f79d94] (2026-10-23T05:04:17Z)
- (ac 1) `cargo test -q --test api hand_merged` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 89 filtered out; finished in 0.41s [ran] [tree e9d101f79d94] (2026-10-23T05:04:18Z)
- (ac 2) `cargo test -q --test api hand_merged` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 89 filtered out; finished in 0.41s [ran] [tree e9d101f79d94] (2026-10-23T05:04:19Z)
- `cargo test -q --test api hand_merged` → ✗ exit 101 · test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 89 filtered out; finished in 0.84s / error: test failed, to rerun pass `--test api` (start tree + this task's tests) [ran] [tree e9d101f79d94] (2026-10-23T05:04:59Z)
- `cargo test -q --test api hand_merged` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 89 filtered out; finished in 0.84s [ran] [tree e9d101f79d94] (2026-10-23T05:04:59Z)
- (audit intent) `fm-reviewer (branch review)` → hand merges marked by the push with the same follow-ups as a merge (webhook, notification, activity) through shared helpers; reads write nothing [tree e9d101f79d94] (2026-10-23T05:05:14Z)
- (audit edge) `fm-reviewer` → forks and other servers via head_ref; land_merge race closed by an open-state guard (409 and the base moved back); a head that falls into the base without the base moving is marked on the next base push (accepted) [tree e9d101f79d94] (2026-10-23T05:05:15Z)
- (audit operator) `fm-reviewer` → for-each-ref limited to the open pull requests' heads; query errors logged [tree e9d101f79d94] (2026-10-23T05:05:15Z)
- (ac 1) `cargo test -q --test api hand_merged` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 89 filtered out; finished in 0.43s [ran] [tree e9d101f79d94] (2026-10-23T05:35:14Z)
- (ac 2) `cargo test -q --test api hand_merged` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 89 filtered out; finished in 0.43s [ran] [tree e9d101f79d94] (2026-10-23T05:35:14Z)
## Log
- 2026-10-22T22:55:46Z created
- 2026-10-23T04:42:37Z planned from capture
- 2026-10-23T04:42:38Z approved: the user asked to implement every queued feature (standing request)
- 2026-10-23T04:42:38Z set approved=True
- 2026-10-23T04:42:48Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0119.builder.md
- 2026-10-23T04:43:08Z focused
- 2026-10-23T05:03:26Z merged worktree-agent-adcdf74251722d93a (3 file(s))
- 2026-10-23T05:03:33Z lane removed: /home/chase/projects/gitter/.claude/worktrees/agent-adcdf74251722d93a
- 2026-10-23T05:03:36Z focused
- 2026-10-23T05:35:14Z done
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research T-0018-review-3: Verdict:** changes needed. No HIGH findings. Findings 1 and 2 break README promises and finding 3 is a test gap on a security rule. I confirmed…
- research T-0018-review-1: Verdict: changes needed.** There are no HIGH findings, and the sandbox and the secret scoping hold. But the "closing or merging takes the previ…
- research T-0018-review-2: Verdict** — changes needed. The diff file T-0018.diff is out of date against the working tree. I reviewed the working tree, so my line numbers …
- research T-0049-review-3: Verdict: changes needed.** There is one real data-corruption path in apply-suggestion and one visibility hole on replies. The rest is minor. [s…
## Why not caught
none: the review that found it (T-0018) was the catch; no test asserted reads are pure until now
## Lessons
- A read that writes hides work until someone looks: do it where the change happens (the push hook)
## Files touched
- tests/api.rs
- src/hooks.rs
- src/api/issues.rs
