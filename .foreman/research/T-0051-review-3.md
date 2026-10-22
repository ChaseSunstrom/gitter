## lens: maintainer — changes needed

Pre-audit flags: the secret-looking values in `scripts/ui-check.mjs` and `tests/forks.rs` are test passwords and runtime tokens, and the `console.log` calls are that script's normal output. I dismiss all four. Every hunk serves the feature, though the plan's steps name only 5 of the 19 files.

- CRITICAL — src/api/issues.rs:1019 (create_pull), enforced at src/api/issues.rs:251 and src/smart_http.rs:163 — `create_pull` only requires that the opener can read the head repo. `maintainer_can_modify` is then trusted, and `maintainer_branches` grants push rights to the head repo from the base repo's write access. So anyone can push to any public repo.
  - Reproducer: bob forks alice/hello and makes a branch in bob/hello that is behind alice's `main`. He POSTs `/api/repos/bob/hello/pulls` with `{head:"main", head_repo:"alice/hello", base:<that branch>, maintainer_can_modify:true}`. `maintainer_branches(alice/hello, bob)` then returns `["main"]` because bob can write bob/hello, so `git push` to alice/hello main succeeds. Only branch rules can stop it.
  - The same path lets a base-repo writer push to a third party's fork: alice opens a PR from bob/hello:feature with the flag set. The tests never try either (tests/forks.rs:1156 and :1095).
  - Fix: require `Repo::writable` on the head repo when `maintainer_can_modify` is set, ideally for any fork head as GitHub does. Add tests for both cases.
  - file: src/api/issues.rs; line: 1019; short: Anyone can grant themselves push to others' repos; category: authorization-bypass; verdict: CONFIRMED

- MEDIUM — src/api/code.rs:139-144 — All compares from one fork share the ref `refs/gitter-compare/<fork id>`. Concurrent requests for different branches (head=a, head=b) race: A fetches, B overwrites the ref, and A resolves B's sha, so the diff is for the wrong branch. A lock clash instead gives a misleading 400 "has no branch".
  - Fix: resolve the sha in the fork (`st.git.resolve(&f.path, refs/heads/<head>)`, as `create_pull` does). Use the shared ref only to keep the objects.
  - file: src/api/code.rs; line: 139; short: Shared compare ref races across branches; category: race-condition; verdict: PLAUSIBLE

- MEDIUM — src/api/issues.rs:657-668 and :207-232 — `sync_fork_pulls` only follows open PRs. If the fork's branch is pushed while the PR is closed and the PR is then reopened, `refs/pull/N/head` is stale and the page and merge use old commits.
  - Reproducer: open, close, push to the fork branch, reopen, then read `head_sha`. It does not match the fork's tip.
  - Fix: on reopen of a `from_fork` PR, call `fetch_fork_head`.
  - file: src/api/issues.rs; line: 668; short: Reopened fork PR keeps stale head; category: stale-state; verdict: CONFIRMED

- LOW — src/api/mergequeue.rs:76, src/api/reviews.rs:510, src/api/checks.rs:203, src/protect.rs:448, src/api/repos.rs:587, src/api/lifecycle.rs:753, src/ci/trigger.rs:190 — These behaviour changes have no test: queue refusal, apply-suggestion refusal, auto-merge on a fork PR, branch list hiding, bundle exclusion, and the agent join. `check_refs` (src/protect.rs:674) is a pure function with no unit test. Fix: add one short test per change.
  - file: src/mergequeue.rs; line: 76; short: Fork guards untested; category: test-coverage; verdict: CONFIRMED

- LOW — src/api/issues.rs:107, src/api/checks.rs:203, src/protect.rs:448 — The `refs/pull/N/head` ref is built in SQL three times and in Rust by `head_ref()`. The fork-fetch command line is copied in `fetch_fork_head`, `create_pull` and `compare`. Fix: one SQL fragment and one fetch helper that takes a destination ref.
  - file: src/api/issues.rs; line: 107; short: Duplicated ref and fetch logic; category: duplication; verdict: CONFIRMED

- LOW — src/repo.rs:409 — `push_only: vec![],` is misindented in the test fixture. Fix: run rustfmt.

## Acceptance criteria
Not listed in the brief, so I judged the stated feature only. Opening, following, merging and edits-off work as tested. "Only that one branch" for maintainers is not met: see the CRITICAL finding.

## Implicit decisions
- A fork's PR head is copied to `refs/pull/N/head` and shown to git clients, while `refs/gitter-compare/*` is hidden.
- Maintainer edits are off unless requested, and the UI checkbox is on by default.
- Fork PRs get no CI and no merge queue.
- Family means parent, child or sibling.

## Not checked
Running the tests or the UI script; the imports.rs interaction with `refs/pull/*`; webhook payloads for `refs/pull` updates.

Noticed: after_push in src/smart_http.rs:509 diffs all refs, so a concurrent server-side `refs/pull/*` update can show up as a user push in webhooks.
