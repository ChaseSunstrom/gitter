---
id: T-0062
type: CLEAN
tier: S
status: captured
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T01:25:20Z
updated: 2026-10-22T01:37:15Z
---
# Lifecycle follow-ups: free-disk check before building an export (refuse below ~2x the acco
## Raw request
> Lifecycle follow-ups: free-disk check before building an export (refuse below ~2x the account size); the 7-day trash/offer windows as admin settings; one helper for the '~' || id trash rename; show the transfer button only to owners (meta.can_transfer); hooks.rs refresh_size on a repo trashed mid-push (from T-0033 review)
## Interpretation
Small follow-ups from the lifecycle review. Building a data export first checks free disk space and refuses when there's less than about twice the account's stored size; the trash and transfer-offer windows (7 days) become admin settings; the '~' || id trash rename lives in one helper used by trash, restore's rollback and the migration notes; repo settings only show Transfer ownership to people who may transfer (a can_transfer flag in the repo JSON: the owner, an owner of the owning organization, or a site admin); the size refresh after a push skips a repository that was trashed meanwhile so its quota isn't understated.
## Assumptions (confidence)
## Acceptance criteria
- [ ] Exports refuse on low disk, the trash and offer windows follow the settings, can_transfer is in the repo JSON only for owners, and a trashed repo's size isn't zeroed by a late push hook — verify with `cargo test -q --test api lifecycle_settings`
- [ ] Settings pages render and e2e stays green — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
(a) Free space from statvfs on the data dir through the libc crate, which is already in the dependency tree (add it as a direct dependency with the same version), skipping the check on non-unix [chosen: a few lines, no new download]; (b) no check (the status quo). The windows go through the existing settings struct and admin page like the other limits.
## Risks and rollback
## Execution prompt
## Steps
1. [ ] free-disk check before exports (unix statvfs), clear error  <- CURRENT
2. [ ] trash and offer windows as admin settings with bounds; admin settings UI
3. [ ] one helper for the trash rename; can_transfer in the repo JSON and the settings page; skip size refresh for trashed repos; tests
## Resume here
## Docs impact
## Verification evidence
## Log
- 2026-10-22T01:25:20Z created
## Follow-ups captured
