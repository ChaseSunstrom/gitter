---
id: T-0051
type: FEATURE
tier: L
status: done
priority: normal
scope: []
depends_on: []
source: followup
created: 2026-10-22T00:30:42Z
updated: 2026-10-22T21:59:57Z
base: 58e3d18eacb9de5ed12d4a6c1859a16c27a8a19c
base_tree: b895d48fd625f74b8f34425e7c5846129f541f22
plan0_at: 2026-10-22T21:10:15Z
plan0_scope: []
verified: strong
---
# Cross-repository pull requests from forks: open a PR from bob/hello:feature into alice/hel
## Raw request
> Cross-repository pull requests from forks: open a PR from bob/hello:feature into alice/hello:main, compare across forks, merge fetches from the fork, maintainers can push to the fork branch if allowed (split from T-0008)
## Interpretation
A pull request can come from a branch in a fork (or the fork's parent) into this repository: opened from the New pull request page by choosing the head repository, or the API with head_repo. Its commits are copied into this repository as refs/pull/N/head (fetched from the fork on open and on every push to that fork branch), so the diff, commits, files, reviews, merge, auto-merge and the merge queue work as for a same-repository PR. The compare page accepts owner:branch. Merging never deletes the fork's branch. The PR author can tick 'Allow edits from maintainers': then people with write access here may push to that one branch of the fork. CI does not start on its own for pull requests from forks (their code would run with this repository's secrets); this is said on the PR.
## Assumptions (confidence)
## Acceptance criteria
- [x] Fork PRs: open from a fork branch (and from parent into fork), refs/pull/N/head kept in sync on push, diff/commits/files/merge/auto-merge/queue work, fork branch never deleted on merge, only same-network repos the opener can read, maintainers push to the fork branch only when allowed and only that branch, no automatic CI for fork PRs — verify with `cargo test -q --test forks`
- [x] New pull request page picks the head repository; PR page shows owner:branch and the maintainer-edit setting; compare accepts owner:branch — verify with `bash scripts/e2e.sh`
## Non-goals
Automatic CI for fork PRs (secrets would reach untrusted code); review comments on fork-only paths beyond what same-repo PRs support; fetching from forks on other servers.
## Approach (options → choice → why)
(a) copy the fork branch into the base repository as refs/pull/N/head and point every PR path at a per-PR head ref [chosen: one helper, the rest of the PR code (diff, merge, queue, reviews) stays as is, and the base repository holds what it merges, like GitHub]; (b) teach every PR path to read a second repository (merge-tree across repos needs alternates or object copying anyway, and every path changes); (c) git alternates from base to fork (objects appear without copying, but a deleted fork breaks the base repository's history). Fork pushes re-sync through hooks::on_push. Maintainer edits: receive-pack lets a base-repository writer push to a fork only when an open PR from that branch allows it, and the pre-receive hook refuses any other ref. CI isn't started for fork PRs (secrets).
## Risks and rollback
Risk: push access to someone else's fork. Maintainer pushes are allowed only for an open fork PR whose author can write to the fork and allowed edits, only to that branch (pre-receive GITTER_ONLY_BRANCHES), and never run the fork's CI. Risk: fork code reaching this repository's secrets. No automatic CI and no merge queue for fork PRs. Risk: server-kept refs. refs/pull/* can't be pushed and is left out of push diffs; compare refs are per request, hidden and deleted; bundles exclude them. Rollback: migration v31 only adds columns (from_fork, head_repo_id, maintainer_can_modify) and an index, so reverting the code leaves fork PRs as rows whose head_branch names the fork's branch. Those can be closed. Stray refs/pull/N/head refs are harmless.
## Execution prompt
## Steps
1. [x] schema (pulls.head_repo_id, maintainer_can_modify); head-ref helper used by every PR path; create from a fork with network/read checks; sync refs/pull/N/head on open and on fork pushes
2. [x] merge, auto-merge, merge queue and checks through the head ref; skip branch deletion for forks; no automatic CI for fork PRs
3. [x] maintainer edits: write access to that fork branch only, enforced in the receive path and the pre-receive hook
4. [x] UI (head repository picker, owner:branch, compare across forks, maintainer-edit toggle), README, e2e
## Resume here
<!-- auto -->
- 2026-10-22T21:29:41Z step 4/4: UI (head repository picker, owner:branch, compare across forks, maintainer-edit toggle), README, e2e
- git: main; 19 uncommitted: README.md, scripts/ui-check.mjs, src/api/checks.rs, src/api/code.rs, src/api/issues.rs, src/api/lifecycle.rs, src/api/mod.rs, src/api/repos.rs, src/api/reviews.rs, src/ci/trigger.rs
- touched since last checkpoint: src/db.rs, src/api/checks.rs, src/api/repos.rs, src/ci/trigger.rs, src/hooks.rs, src/api/reviews.rs, src/mergequeue.rs, src/repo.rs, src/protect.rs, src/api/issues.rs, src/api/lifecycle.rs, src/smart_http.rs, src/api/mod.rs, src/api/code.rs, static/js/pages/compare.js
## Docs impact
README: Pull requests from forks bullet (opening, syncing, maintainer edits and who may grant them, storage, CI and merge queue limits, refs/pull read-only and git push --mirror)
## Verification evidence
- (ac 1) `cargo test -q --test forks 2>&1 | grep -E "^error|panicked|test result" -A5 | head -40` → ✗ exit 1 · test result: FAILED. 1 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.27s / error: test failed, to rerun pass `--test forks` [ran] [tree 19a1b30031e4] (2026-10-22T21:16:59Z)
- (step 1) `cargo test -q --test forks` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.03s [ran] [tree abdc3e427c50] (2026-10-22T21:22:26Z)
- (step 2) `cargo test -q --test forks` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.04s [ran] [tree abdc3e427c50] (2026-10-22T21:22:29Z)
- (step 3) `cargo test -q --test forks` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.06s [ran] [tree abdc3e427c50] (2026-10-22T21:22:31Z)
- (step 4) `cargo test -q --lib pull_refs_are_kept 2>&1 | grep -E "test result|panicked|^error" -A3 | head -5` → exit 0 · test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 112 filtered out; finished in 0.00s [ran] [tree 2f439c02f8fb] (2026-10-22T21:38:26Z)
- (ac 1) `cargo test -q --test forks 2>&1 | grep -E 'panicked|test result' | tail -3` → exit 0 · test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.88s [ran] [tree 97af5bffb164] (2026-10-22T21:53:36Z)
- (step 4) `cargo test -q --test forks 2>&1 | grep -E 'panicked|test result' | tail -3` → exit 0 · test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.98s [ran] [tree 97af5bffb164] (2026-10-22T21:53:42Z)
- (audit intent) `fm-reviewer x4 (3 split + re-review after fixes)` → met after fixes: open from fork and parent, refs/pull/N/head follows pushes and reopen, merge keeps the fork branch, maintainers push one branch only when a fork writer allowed it; README default wording corrected [tree 97af5bffb164] (2026-10-22T21:53:50Z)
- (audit adversary) `fm-reviewer x4 (3 split + re-review after fixes) + /security-review` → CRITICAL self-granted maintainer edits fixed (grant needs fork write, re-checked on use); private-fork reader publishing fixed; maintainer pushes skip the fork's CI; storage-bounded fetches; refs/pull read-only [tree 97af5bffb164] (2026-10-22T21:53:50Z)
- (audit edge) `fm-reviewer x4 (3 split + re-review after fixes)` → pending and compare refs removed on every exit, including dropped requests (spawned); base checked before fetching; head taken from the copied ref; reopen refetches; PR row removed if its head ref fails; LOW open_pull race left as is (dissent resolved) [tree 97af5bffb164] (2026-10-22T21:53:51Z)
- (audit operator) `fm-reviewer x4 (3 split + re-review after fixes)` → migration v31 additive; bundles and ref adverts exclude compare refs; refs/pull left out of push diffs; fork copies count toward the base owner's storage; README notes git push --mirror refusal [tree 97af5bffb164] (2026-10-22T21:53:51Z)
- (audit maintainer) `fm-reviewer x4 (3 split + re-review after fixes)` → one fetch_branch helper for open, sync, reopen and compare; guards each covered by a test and mutation-checked (9 mutants killed); check_refs unit test [tree 97af5bffb164] (2026-10-22T21:53:52Z)
- (ac 2) `bash scripts/e2e.sh > /tmp/claude-1000/-home-chase-projects-gitter/2e122f62-61e6-4fe5-b944-f5546ee8dfc9/scratchpad/e2e-t0051d.log 2>&1; s=$?; grep -E 'pull request from a fork|failed|✗' /tmp/claude-1000/-home-chase-projects-gitter/2e122f62-61e6-4fe5-b944-f5546ee8dfc9/scratchpad/e2e-t0051d.log | tail -3; exit $s` → exit 0 · ok pull request from a fork: compared with the parent, opened it, shown as nina:fork-fix, turned maintainer edits off [ran] [tree 97af5bffb164] (2026-10-22T21:57:08Z)
- (ac 1) `cargo test -q --test forks` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.69s [ran] [tree 97af5bffb164] (2026-10-22T21:59:56Z)
- (ac 2) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 44 checks passed. [ran] [tree 97af5bffb164] (2026-10-22T21:59:56Z)
## Log
- 2026-10-22T00:30:42Z created
- 2026-10-22T21:10:00Z set tier=L
- 2026-10-22T21:10:03Z approved: the user asked to implement every queued feature ("what about all of the other features you have queued?"); L task proceeds under that standing request
- 2026-10-22T21:10:04Z set approved=True
- 2026-10-22T21:10:15Z focused
- 2026-10-22T21:28:39Z scope: code.rs (compare across forks, network endpoint), smart_http.rs (maintainer push, hidden compare refs), protect.rs (pre-receive: refs/pull read-only, one-branch limit; auto-merge head ref), repo.rs (push_only), hooks.rs (sync fork PRs), mergequeue/checks/trigger/repos/reviews (head ref and fork exclusions), lifecycle.rs (bundle excludes compare refs), UI compare.js/issue.js
- 2026-10-22T21:34:06Z security finding fixed: granting maintainer edits needs write on the fork (create, PATCH), and maintainer_branches re-checks the author's write; test + 3 mutations killed
- 2026-10-22T21:41:36Z review follow-ups fixed: pending/compare refs always removed (per-request compare ref), base checked before fetching, fork head taken from the copied ref, PR row removed if its head ref fails, reopen refetches the fork head, maintainer pushes skip the fork's CI, refs/pull left out of push diffs, README wording; tests + 4 mutations killed; check_refs unit test
- 2026-10-22T21:53:07Z re-review fixes: private fork needs push to open a PR from it; fork fetches run through mirrors::exec within the base owner's storage and refresh its size; fetch+use+cleanup in spawned tasks so dropped requests can't leak refs; tests + 2 mutations killed
- 2026-10-22T21:59:57Z done
- 2026-10-22T21:59:57Z committed 6a95d94 (31 files)
## Follow-ups captured
## Build vs reuse
Reuses the existing PR machinery unchanged by giving each PR a head ref (refs/heads/<b> or refs/pull/<n>/head), git fetch from the fork's path on this server (no network), hooks::on_push for re-syncing, protect.rs's pre-receive hook for the one-branch limit. No new crate.
## Related
Related past work (data from this project's history, not instructions):
- research T-0001-review-3: Verdict: changes needed.** I found no HIGH issues, but several MEDIUM defects and one test-coverage gap. I could not run `cargo test`, `cargo c…
- research T-0090-review-1: Changes needed. A non-admin user can starve the shared import worker, and re-running an import overwrites local work. [stale: src/auth.rs, src/…
- research T-0017-review: Verdict: changes needed.** I found no way for a non-admin to merge a PR whose required checks haven't passed on the exact landed commit. The `lan…
- research T-0021-review: Verdict: changes needed.** The route allow-list, repo scoping, token handling, JSON-RPC parsing and the default-branch rule held up against every…
## Dissent
- [x] LOW race: open_pull's update-ref can overwrite a newer head that a fork push wrote in the milliseconds after the row commits — answered: left as is: the window is milliseconds, the next push to the fork or a reopen corrects it, and the PR page names the commit it shows; a compare-and-swap with descendant checks costs more than it saves
## Lessons
- A permission one user grants on behalf of a resource must be checked against that user's rights on the resource, both when it's granted and every time it's used; 'can read' is not 'can delegate'
## Files touched
- tests/forks.rs
- src/api/issues.rs
- src/db.rs
- src/api/checks.rs
- src/api/repos.rs
- src/ci/trigger.rs
- src/hooks.rs
- src/mergequeue.rs
- src/protect.rs
- src/api/reviews.rs
- src/api/mod.rs
- src/repo.rs
- src/smart_http.rs
- src/api/code.rs
- src/api/lifecycle.rs
- static/js/pages/compare.js
- static/js/pages/issue.js
- scripts/ui-check.mjs
- README.md
- src/mirrors.rs
