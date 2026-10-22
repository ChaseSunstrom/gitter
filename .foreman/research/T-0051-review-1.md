**Verdict: changes needed.** Maintainers can be granted push access to branches of a repository the PR author has no rights on. I confirmed this by reading the code. I did not run it.

Pre-audit items: the "secret-looking" values in `scripts/ui-check.mjs` and `tests/forks.rs` are throwaway test credentials, so dismissed. The "debug leftovers" are the script's normal pass/fail output, so dismissed.

## lens: adversary
- CRITICAL — src/api/issues.rs:1016-1076 (+ :236-256, src/smart_http.rs:773-783) — `create_pull` and `update_pull` never check that the PR author can write to the head repo. `maintainer_branches` then grants every writer of the base repo a push to that head-repo branch. Reproducer: bob owns the fork `bob/hello`, and `alice/hello` has a commit his main lacks. Bob makes branch `old` in `bob/hello` at an older commit, then runs `POST /api/repos/bob/hello/pulls {head:"main", head_repo:"alice/hello", base:"old", maintainer_can_modify:true}`. `head_repo` accepts this because `r.forked_from == h.id`. `git push https://bob@host/alice/hello.git HEAD:main` is then allowed, because bob can write `bob/hello` and `push_only` is `["main"]`. Any reader of a public fork can do the same to push into that fork's branch. Only branch rules (`GITTER_RULES`), if configured, still bind. — Fix: require the author's `can_write` on the head repo to set `maintainer_can_modify` (at create and in `update_pull`). In `maintainer_branches`, also require that `i.author_id` still has write on `fork` (via `Repo::writable`). Add a test for the parent-to-fork direction. — high
- MEDIUM — src/api/issues.rs:352-356 — Early returns after the fetch skip `drop_pending()`: a missing base branch returns before it, and so do the `?` errors on `resolve` and `count`. `refs/pull/new-*/head` stays in the base repo and is advertised to clone and ls-remote (only `refs/gitter-compare` and `refs/gitter-queue` are hidden). Reproducer: a reader of a private fork posts `base:"nope"`. — Fix: delete the pending ref on every exit, for example with a drop guard, or fetch only after base is validated. — high
- MEDIUM — src/api/code.rs:139-144 — `refs/gitter-compare/<fork id>` is shared by every viewer and branch. Two concurrent compares of different branches of one fork can fetch, fetch, then resolve, so one viewer gets the other's sha and a wrong diff. Any anonymous viewer of a public fork can trigger a fetch of all its objects into the base repo, and nothing cleans them up. — Fix: use a per-request ref name and delete it afterwards (or resolve `FETCH_HEAD`), and rate-limit. — high
- MEDIUM — src/hooks.rs:18 with src/smart_http.rs:783 — A maintainer's push to the fork runs the fork's own CI (`ci::trigger::on_push`) with the maintainer as actor. Edits to `.gitter/ci.sh` therefore run with the fork owner's CI secrets. — Fix: skip CI, or require approval, for pushes made through `push_only`. — low (CI secret scoping not checked)

## lens: edge
- LOW — src/api/issues.rs:405-411 — The PR row is inserted before `update-ref`. If the fork branch moved between the resolve and the fetch, or the process dies, the PR exists with no `refs/pull/N/head`, and the client gets a 500. — Fix: update the ref before commit, or delete the PR on failure.
- LOW — src/api/issues.rs:687 — Reopening a closed fork PR doesn't re-fetch. `sync_fork_pulls` only follows open PRs, so the head is stale until the next push. — Fix: call `fetch_fork_head` on reopen.
- LOW — src/protect.rs:377 — Any ref under `refs/pull/` is refused. A `git push --mirror` of a GitHub clone, a common migration, fails entirely. — Fix: note it in the README or ignore those refs.

## Acceptance criteria
Not given in the brief. Not verifiable.

## Implicit decisions
- Any reader of the head fork may open a PR from it.
- The family is parent, child or sibling.
- A fork PR's head is a server-owned ref, `refs/pull/N/head`.
- Fork PRs get no CI, no merge queue, and no applying of suggestions.
- `maintainer_can_modify` defaults to false in the API but is checked in the UI.
- Fetch failures in `sync_fork_pulls` are only logged.

## Not checked
Whether the diff's hunks match plan steps (I didn't have the plan). Playwright and JS runtime behaviour, `tests/forks.rs` execution, CI secret scoping, and `validate::ref_name` Unicode cases.

**Noticed:** `src/webhooks.rs:146` reports only `head_branch` for fork PRs, so the payload has no head repo.
