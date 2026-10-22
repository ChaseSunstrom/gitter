**Verdict: approve with nits.** I checked each listed fix against the working tree and found no way around the CRITICAL one. The new findings are below.

Fixes that hold:
- **Maintainer grant:** `maintainer_branches` (issues.rs:175-199) re-checks the PR author's write access to the fork live. `open_pull` (issues.rs:1104) and `update_pull` both need the opener to have write access on the head repo. `pre_receive`'s `check_refs` is all-or-nothing and refuses deletes. The `u.limits.is_none()` guard works over SSH too, because `ssh.rs:439` calls the same `open`.
- **CI on maintainer pushes:** `hooks::on_push` is the only path to `ci::trigger::on_push`, so the skip is complete.
- **Reopen:** both state-change sites call `refresh_fork_head`.
- **Merge and auto-merge:** both resolve `refs/pull/N/head`. The `branch_pushers` lookup is limited to non-fork PRs, so a same-named base branch can't be mistaken for the fork's.
- **`ref_map`:** the only other caller is `mirrors::pull`, which is fine.

## lens: adversary
- MEDIUM — src/api/issues.rs:1031-1040, 1104 — `head_repo` only requires that the opener can read the fork. A read-only collaborator on a private fork can open a PR into the public parent. The fork's commits are then copied to `refs/pull/N/head` in the public repo, where anyone can clone them and where export bundles include them. Reproducer: bob makes `bob/hello` private and adds carol as a read collaborator. Carol POSTs `/api/repos/alice/hello/pulls` with `head_repo: "bob/hello"` and gets 200. An anonymous `git ls-remote alice/hello` then lists `refs/pull/N/head`. Fix: in `head_repo`, reject a private or hidden head repo unless `h.can_write`, or unless the base is at least as private as the fork. — confidence: medium. It matches the acceptance criterion literally ("opener can read"), but it lets a reader publish.

## lens: edge
- MEDIUM — src/api/code.rs:806-808 and src/api/issues.rs:1063-1071 — The temporary ref is deleted by inline `let _ = delete_ref`. If the client disconnects, the handler future is dropped and the delete never runs. A busy server (`permit()` times out after 15s) or a restart in between has the same result. The leaked `refs/gitter-compare/<id>-<rand>` or `refs/pull/new-*/head` pins the fork's objects permanently. The second one is also visible to git clients. Reproducer: `curl --max-time 0.05 '…/compare?...&head_repo=bob/hello'` in a loop, while parallel compares keep the git semaphore full. Then `git for-each-ref` shows the refs. Fix: run fetch and delete inside `tokio::spawn` and await the handle, as `land_merge` does. Also add a sweep that deletes `refs/gitter-compare/*` and `refs/pull/new-*` older than about 10 minutes, in `maintain` or the hourly job. — confidence: plausible.
- LOW — src/api/issues.rs:1134 — `open_pull` runs `update-ref refs/pull/N/head` after the row is committed. A push to the fork in that gap lets `sync_fork_pulls` write the newer head, and `open_pull` then overwrites it with the older sha. The PR is stale until the next push. Fix: pass the zero oid as the old value (create only), and treat "already exists" as success.

## lens: operator
- MEDIUM — src/api/issues.rs:1065, 165 and src/hooks.rs:16 — Fetching a fork's objects into the base repo has no `storage_left` check for the base owner. `refresh_size` runs only for the pushed fork, not for the base. A stranger can push large unique blobs to their own fork and open PRs into someone else's repo. That fills the victim's quota, and later pushes by the victim fail with "out of storage". Fix: before `fetch_branch` into a base, require `storage_left(base.owner_id)` to cover the fork's `size_bytes`, and call `refresh_size(base)` after the fetch. — confidence: plausible.

## lens: intent
No findings. The ui-check "secret" and "debug" pre-audit items are test credentials and the script's normal reporting, so I dismiss them.

## lens: maintainer
No findings. Test gap, LOW: nothing covers a parent→fork PR with `maintainer_can_modify: true` (the `fork.can_write` gate is only exercised by carol), and nothing covers a suspended PR author.

Not checked: `scripts/e2e.sh` and `cargo test` (read-only review), and branch rename and delete paths that bypass `hooks::on_push`.

**Noticed:** UI shows the "Allow edits" checkbox to a PR author who no longer has write access to the fork, so toggling it only produces a 403 toast (static/js/pages/issue.js:226).
