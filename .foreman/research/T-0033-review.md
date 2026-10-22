**Verdict: changes needed.** The core is solid. Trashed repos are excluded from `lookup`, `VISIBLE`, CI, LFS and git, and private repos don't leak through redirects. I found one real leak, one caching bug and two data-safety gaps. I read the diff and the worktree but did not run anything.

## lens: adversary
- MEDIUM — src/api/mod.rs:238 — `ACCOUNT_ROUTES` lacks `/me/trash` and `/me/transfers`. `token_may` lets any limited token GET them, and `my_trash` and `my_transfers` take plain `Authed`. A deploy token for one repo can therefore list the owner's trashed private repos (names, descriptions) and offered private repos. Fix: add both prefixes to `ACCOUNT_ROUTES`. Confidence: high.
- MEDIUM — src/web.rs:82 (`redirect`) — The 301/308 responses carry no `Cache-Control`. Browsers cache 301 indefinitely. After `claim_name` drops a redirect (or the repo moves again), `/alice/proj` and `/api/repos/alice/proj/*` keep jumping to the old target. A shared cache could also serve a private-viewer's redirect to others. Fix: add `Cache-Control: no-store` (and `Vary: Cookie`). Confidence: high.
- MEDIUM — src/repo.rs:91 and lifecycle.rs:198 — After a transfer, the former owner can create `alice/lib` again. `claim_name` drops the redirect, so anyone pulling from the old URL gets the old owner's new code (repo-jacking). Fix: for owner-changing redirects, refuse to reuse the name (409) instead of deleting the redirect. Confidence: medium; this follows the stated drop rule, so it is a design call.
- LOW — lifecycle.rs:202 — Accept deletes deploy tokens but not `ci_secrets`, so the new owner can print the old owner's secrets from a workflow. Fix: `DELETE FROM ci_secrets WHERE repo_id`.

## lens: edge
- MEDIUM — src/ops.rs:152-161 — If a repo moves in or out of the trash between the snapshot and the tar, its folder is "missing". The backup only warns and omits it, so a just-deleted, restorable repo is lost from the archive. Fix: fall back to the sibling store's folder, as `reconcile` does.
- MEDIUM — lifecycle.rs:256 — In `trash()`, `cancel(c, run)?` runs after the DB commit and before `move_dir`. An error leaves the row trashed with the folder still in `repos/`. `purge` (lifecycle.rs:274) only removes `trash_path` and the wiki, so that folder stays on disk. Fix: make the cancel best-effort or run it before the commit, and make `purge` also remove `repo_path(id)`.
- LOW — lifecycle.rs:57,358 — `move_dir` treats NotFound as success. Restoring a repo whose trash folder is missing returns OK and leaves a live row with no folder. Fix: fail on restore.
- LOW — lifecycle.rs:491 — An export stuck as `running` after a task panic blocks new exports until a restart. Fix: allow a replace after a staleness timeout.
- LOW — lifecycle.rs:~600 (`build_export`) — Bundles contain no LFS objects, so the export is incomplete. Document it or include them.

## lens: operator
- LOW — lifecycle.rs:~445 — Exports are never counted against quota and there is no free-disk check. Ready files persist for 24 hours, and the bundle plus tar roughly double a repo's size while building. Fix: refuse when free space is below about 2× the size.
- LOW — lifecycle.rs:20-24 — The 7-day windows are hard-coded, not settings.

## lens: maintainer
- LOW — The `'~' || id` rename SQL appears in `trash`, in the `restore` rollback and in the migration comment. Keep it in one helper.
- LOW — `follow_moved` (web.rs) rewrites only `/api/repos`, `/raw` and `/archive` paths. Any future owner/repo route gets a silent 404 on old URLs, so add a comment or a test for this.

## Merge flags (owner-only assumptions)
- `Repo::writable` gates `offer`, `offer_transfer` and `cancel_transfer` (lifecycle.rs:70,100,130), plus `update` and `remove` in repos.rs. A write-role collaborator could transfer or trash the repo. Switch these to `administrable`, or to owner-only for transfer.
- reposettings.js:27 checks `meta.can_write`.
- `trashed()` and `restore` filter on `owner_id = uid`, so an admin collaborator can't restore and org repos have no trash view.
- `accept`, `delete_account` and `export_json` assume personal owners. Decide how collaborator rows behave on transfer and on account delete.

## Acceptance criteria
- Rename/transfer redirects and acceptance: met by the `rename_transfer` test (not run here); the cache and repo-jacking issues above still apply.
- Trash, restore, purge with LFS collected: met by the `trash` test (not run here); see the `trash()` ordering finding.
- Export and self-delete: met by `export_and_self_delete`. Sudo means a fresh session, not a token, and the last-admin guard is covered.
- e2e.sh and backup-roundtrip.sh: not verifiable from the code.

## Implicit decisions
- The tar-sibling filenames in the export are `repositories/<name>.bundle`, `wikis/<name>.bundle` and `snippets/<id>.bundle`.
- Self-delete purges immediately instead of going through the trash.
- At most 20 redirects are kept per repo.
- Export build concurrency is capped at 2 for the whole instance.
- Offers last 7 days. Each repo holds one offer, and a new offer replaces the old one.

## Not checked
I did not compile or run any tests. I did not review the full API handler set (`api/code.rs`, `issues.rs`, `ci.rs`) beyond grepping for `repos` queries. I did not check the JS HTML escaping beyond a skim.

**Noticed:**
- hooks.rs:21 `refresh_size` can zero `size_bytes` for a repo trashed mid-push, which would understate quota.
- account.rs:646 heatmap counts events from trashed public repos.
