---
id: T-0033
type: FEATURE
tier: M
status: active
priority: normal
scope: []
depends_on: []
source: followup
created: 2026-10-21T21:11:21Z
updated: 2026-10-22T00:43:00Z
builder: 2026-10-22T00:08:38Z
builder_proc: archbox-hardened|pid:[4026531836]|3700487|2540304
lane: /home/chase/projects/gitter/.claude/worktrees/agent-a9e62c8d122156171
lane_branch: worktree-agent-a9e62c8d122156171
base: 77c5dcb1a94fdc95a072a36f99074ac1192d6bd5
base_tree: 16e8c4254b3866eb127fff4691d6e5b0208bdf15
plan0_at: 2026-10-22T00:09:22Z
plan0_scope: []
---
# Repo rename/transfer with redirects, trash with undo, account data export and self-delete
## Raw request
> Repo rename/transfer with redirects, trash with undo, account data export and self-delete (split from T-0009)
## Interpretation
Four account and repository lifecycle features. (1) Renaming a repository (the existing PATCH) and transferring it to another account leave redirects: a repo_redirects(owner_name, repo_name → repo_id) table, so old web URLs, API paths and git remotes (info/refs, upload-pack, receive-pack, LFS) keep working. The web and API answer 301 to the new place. Git over HTTP is served directly at the old URL, because git clients don't follow redirects on push. A redirect is dropped when a new repo takes that name. (2) Transfer: an owner offers a repo to another user (Sudo, typed confirm). The recipient sees the offer in Settings → Repositories and accepts or declines. Accepting moves ownership and checks the recipient's repo count and storage quota. Offers expire after 7 days. Audited. (3) Trash: deleting a repository moves it to a trash for 7 days (the row is kept with deleted_at; the folder moves to GITTER_DATA/trash/<id>.git). It vanishes from every listing and access check, can be restored from Settings → Repositories → Recently deleted (when the name is still free), and is purged by the upkeep loop after 7 days. Purging runs the existing LFS collection and audit. Trashed repos still count toward storage until purged. Admins can purge right away. (4) Account data export: Settings → Account → Export your data builds, in the background, a .tar.gz with profile.json, a git bundle of each owned repository (and wiki), issues, PRs, comments and discussions you wrote as JSON, snippets, and the security log. It's downloadable for 24 hours, and there's one export at a time per account. (5) Self-delete: Settings → Account → Delete account (Sudo, typed username). It refuses for the last admin, deletes the account like admin delete_user (repos go to trash first, then are purged), and signs out.
## Assumptions (confidence)
## Acceptance criteria
- [ ] Renamed and transferred repos redirect for web/API and still clone/push at the old git URL; transfers need acceptance — verify with `cargo test -q --test api rename_transfer`
- [ ] Deleted repos go to a 7-day trash, can be restored, and are purged after (LFS collected) — verify with `cargo test -q --test api trash`
- [ ] Data export contains bundles and JSON; self-delete works and refuses the last admin — verify with `cargo test -q --test api export_and_self_delete`
- [ ] Pages render in a browser; backups include trash — verify with `bash scripts/e2e.sh && bash scripts/backup-roundtrip.sh`
## Non-goals
## Approach (options → choice → why)
Redirects: (a) a repo_redirects table checked when Repo::load misses [chosen: one lookup, covers web, API and git]; (b) an alias column on repos (only one old name); (c) proxy rules (not per repo). Trash: (a) a deleted_at column plus moving the folder to trash/ [chosen: restore is a rename; access() and listings filter on deleted_at]; (b) a separate archive (harder to restore). Export: a background tokio task writing GITTER_DATA/exports/<user>-<id>.tar.gz with git bundle create --all per repo; a row tracks status and expiry. Build vs reuse: the tar, flate2 and git CLI are already in place, and admin::delete_user's logic is reused for self-delete.
## Risks and rollback
## Execution prompt
## Steps
1. [x] redirects table + resolution in Repo::load fallback (web/API 301, git served in place)
2. [x] transfer offers: create/accept/decline/expire, quota + repo count checks, audit
3. [x] trash: soft delete, listing/access exclusion, restore, purge in upkeep, LFS collect on purge, backups include trash/
4. [x] data export job + download; self-delete with Sudo
5. [x] UI: Settings → Repositories (offers, recently deleted), Account (export, delete) + tests
## Resume here
## Docs impact
## Verification evidence
- (step 1) `cargo test -q 2>&1 | grep -E "test result|FAILED|panicked|failed|warning" | head -30` → exit 0 · test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.56s / test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s [ran] [tree 6c1bf466395a] (2026-10-22T00:31:41Z)
- (step 1) `cargo test -q --test api rename_transfer` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 37 filtered out; finished in 2.37s [ran] [tree 6c1bf466395a] (2026-10-22T00:31:47Z)
- (step 2) `cargo test -q --test api rename_transfer` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 37 filtered out; finished in 2.42s [ran] [tree 6c1bf466395a] (2026-10-22T00:31:53Z)
- (step 3) `cargo test -q --test api trash` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 37 filtered out; finished in 3.20s [ran] [tree 6c1bf466395a] (2026-10-22T00:31:56Z)
- (step 4) `cargo test -q --test api export_and_self_delete` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 37 filtered out; finished in 1.61s [ran] [tree 6c1bf466395a] (2026-10-22T00:31:59Z)
- (step 4) `cargo test -q --test api export_and_self_delete` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 37 filtered out; finished in 1.51s [ran] [tree e6e9bb916028] (2026-10-22T00:42:41Z)
- (step 5) `cargo test -q --lib reconcile 2>&1 | tail -4` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 49 filtered out; finished in 0.04s [ran] [tree ffea2b0d0d43] (2026-10-22T00:43:00Z)
## Log
- 2026-10-21T21:11:21Z created
- 2026-10-21T23:40:52Z set tier=M
- 2026-10-22T00:08:38Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0033.builder.md
- 2026-10-22T00:09:22Z focused
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research T-0016-review: HIGH — src/api/snippets.rs:137,261 and src/api/wiki.rs:148 — snippets and wikis bypass the storage quota [stale: src/api/discussions.rs, src/api/…
- research T-0015-review: Verdict: changes needed.** The temp-file guard, the slow-upload check, the `Cache-Control` fixes and the batch limiter are sound. The storage-cap…
- research T-0004-review: Verdict: changes needed.** None of these block the four acceptance scripts. Findings 1, 2 and 3 break the documented Docker workflow. I verified …
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
