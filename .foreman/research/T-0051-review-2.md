## Verdict
Changes needed. There is one privilege-escalation bug, and it is easy to trigger.

## lens: intent
- HIGH — src/api/issues.rs:1019,1065 (`edits`) and :251 (`maintainer_branches`) — `maintainer_can_modify` is accepted from anyone who can open a PR. The opener is never required to have write access to the head repo. A maintainer of the base repo therefore gets push rights to a branch the PR author doesn't control. — Set `edits` only when the opener passes `Repo::writable` on the head repo, and enforce the same in `update_pull`. In `maintainer_branches`, also require that the PR author can still write to the fork. — confidence: high (CONFIRMED by code path)
  - Reproducer: mallory (no rights on alice/hello) forks it as mallory/hello. alice/hello has any commit mallory/hello lacks, e.g. a feature branch or a new push to main.
  - Mallory sends `POST /api/repos/mallory/hello/pulls {head:"main", head_repo:"alice/hello", base:"<behind branch>", maintainer_can_modify:true}`.
  - `head_repo()` at :315 accepts the parent. Nothing checks mallory's rights on alice/hello.
  - `smart_http::open` then calls `maintainer_branches(alice/hello, mallory)`, which finds that PR, sees mallory writable on the base, and returns `["main"]`. The push to alice/hello:main succeeds.
  - Fix test: add this case to tests/forks.rs; it currently covers only the fork owner as author.
  - ReportFindings: file=src/api/issues.rs line=1065 category=authorization-bypass verdict=CONFIRMED.
- LOW — src/api/issues.rs:1029-1062 — The pending ref `refs/pull/new-*/head` leaks on the early returns after the fetch: the `?` and "target branch doesn't exist" paths at :352-356, the `count` error at :357, and the `update-ref` failure at :406-410. — Call `drop_pending` on every exit, for example with a guard or a single cleanup block. — confidence: high
  - Reproducer: POST a fork PR with `base:"nope"`. `refs/pull/new-…/head` stays, because `refs/pull/*` is not hidden from clients.
- LOW — src/api/code.rs:139-144 — The compare ref is keyed only by fork id, so two concurrent compares of different branches of one fork race between fetch and resolve. One caller then gets the other's head. — Include a per-request nonce in the ref name and delete it after use. — confidence: medium
- LOW — src/api/code.rs:134-146 — A GET (including an anonymous one) runs `git fetch` into the base repo and stores the fork's objects there. When the fork is private and the viewer is a collaborator on it, those objects land in the possibly public base repo. Refs are never cleaned up, for example when the fork is deleted. — Compute the compare in a temporary ref, or use alternates and delete the ref afterwards. — confidence: medium (PLAUSIBLE)
- LOW — README.md:9 versus src/api/issues.rs:988-990 — The README says edits from maintainers are "on by default". The API default is false, and only the compare page's checkbox starts checked. — Say "the compare page offers it checked" or make the API default true. — confidence: high

## lens: operator
- ok. The migration v31 is additive with defaults. Both `bundle` and `ref_map` exclude `refs/gitter-compare`. `refs/pull/*` pushes are refused in pre-receive. Imports fetch with `git fetch`, so they are unaffected. No install or uninstall scripts are touched.
- The pre-audit "secret" hits are test and e2e fixture passwords with a `pragma: allowlist secret` marker. The "debug leftover" hits are the check's own pass and fail reporting. Both are dismissed.

## Acceptance criteria
- Fork PRs, verified with `cargo test -q --test forks`: partly met. I did not run the tests. They cover open, sync, merge, no CI, and the allowed and not-allowed push cases. They do not cover the opener-authority hole above, so "maintainers push only when allowed" is not met.
- New PR page, PR page and compare, verified with `bash scripts/e2e.sh`: not verifiable from the code alone. `scripts/ui-check.mjs` has a flow for it, but I did not run it. `compare.js` and `issue.js` implement all three behaviours.

## Implicit decisions
- Fork PRs cannot use the merge queue, and review suggestions on them are refused.
- Any fork-family member the opener can read is allowed, including siblings.
- The new-PR checkbox defaults to on, and the API defaults to off.
- Compare ref `refs/gitter-compare/<fork id>`, ref `refs/pull/<n>/head`, and a limit of 100 repos in the network list.

## Not checked
I did not run the build or the tests. I did not review `Repo::readable` internals, the fork-of-a-fork `forked_from` semantics, or webhooks.

Noticed: the network endpoint at src/api/code.rs:92 shows only one level of forks, so a fork of a fork has no way to compare.
