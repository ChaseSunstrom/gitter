---
id: T-0044
type: FEATURE
tier: M
status: planned
priority: normal
scope: []
depends_on: []
source: user
created: 2026-10-21T23:41:28Z
updated: 2026-10-21T23:41:28Z
---
# Releases with assets (from T-0010)
## Raw request
> Releases with assets (from T-0010)
## Interpretation
## Assumptions (confidence)
## Acceptance criteria
- [ ] Releases are created from a tag (or a new tag at a commit), edited, published or drafted, with uploaded assets counted toward storage, download counts and the latest release in the API — verify with `cargo test -q --test api releases`
- [ ] Releases pages and the repo sidebar's latest release render; backups include assets — verify with `bash scripts/e2e.sh && bash scripts/backup-roundtrip.sh`
## Non-goals
## Approach (options → choice → why)
## Risks and rollback
## Execution prompt
## Steps
1. [ ] schema + API: releases (tag, title, notes markdown, draft, prerelease, author), assets (name, size, content type, downloads) under GITTER_DATA/releases/<release id>/; writers create/edit/delete, readers see published  <- CURRENT
2. [ ] tags: create the tag at a chosen commit or branch when it doesn't exist (annotated, by update-ref with an expected old value); deleting a release keeps the tag
3. [ ] assets: streaming upload with the storage quota (STORAGE_USED gains release asset bytes), per-file cap = max_lfs_mb, download counts, safe Content-Disposition, private repos stay private
4. [ ] UI: Releases tab (list, release page with notes, assets, source archives), new/edit form with tag picker, latest release in the repo sidebar; backup/restore/verify include releases/
## Resume here
## Docs impact
## Verification evidence
## Log
- 2026-10-21T23:41:28Z created
## Follow-ups captured
