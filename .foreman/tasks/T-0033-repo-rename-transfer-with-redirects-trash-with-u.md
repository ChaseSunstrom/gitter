---
id: T-0033
type: FEATURE
tier: M
status: captured
priority: normal
scope: []
depends_on: []
source: followup
created: 2026-10-21T21:11:21Z
updated: 2026-10-21T23:40:55Z
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
## Risks and rollback
## Execution prompt
## Steps
1. [ ] redirects table + resolution in Repo::load fallback (web/API 301, git served in place)  <- CURRENT
2. [ ] transfer offers: create/accept/decline/expire, quota + repo count checks, audit
3. [ ] trash: soft delete, listing/access exclusion, restore, purge in upkeep, LFS collect on purge, backups include trash/
4. [ ] data export job + download; self-delete with Sudo
5. [ ] UI: Settings → Repositories (offers, recently deleted), Account (export, delete) + tests
## Resume here
## Docs impact
## Verification evidence
## Log
- 2026-10-21T21:11:21Z created
- 2026-10-21T23:40:52Z set tier=M
## Follow-ups captured
