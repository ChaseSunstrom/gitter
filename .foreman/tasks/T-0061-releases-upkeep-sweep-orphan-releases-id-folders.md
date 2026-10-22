---
id: T-0061
type: CLEAN
tier: S
status: captured
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T00:57:55Z
updated: 2026-10-22T01:37:08Z
---
# Releases upkeep: sweep orphan releases/<id> folders in reconcile and log removal failures,
## Raw request
> Releases upkeep: sweep orphan releases/<id> folders in reconcile and log removal failures, delete a just-made tag when the release insert loses a race, map only ref-exists to 409 in make_tag, DB-only lookups for upload/update/delete_asset, defer release file deletion so backups never miss a file (from T-0044 review)
## Interpretation
Small robustness fixes the releases review found. Boot-time reconcile (src/ops.rs) and gitter doctor report and remove releases/<id> folders whose release row is gone; release file removals log failures instead of ignoring them; when creating a published release makes a new tag but the row insert then loses a race, the just-made tag is deleted again; make_tag maps only a ref-already-exists failure to 409 and reports other git errors as errors; upload, update and delete_asset look the release up with a DB-only query instead of the git-backed load; deleting a release file defers the file removal to the next sweep (like LFS) so a backup running at that moment never archives a row without its file.
## Assumptions (confidence)
## Acceptance criteria
- [ ] Orphan release folders are swept, a lost insert race removes its new tag, non-race git errors aren't 409, and deleted files survive until the sweep so backups stay consistent — verify with `cargo test -q --test api releases`
- [ ] Backups and doctor stay green — verify with `bash scripts/backup-roundtrip.sh`
## Non-goals
## Approach (options → choice → why)
(a) Reuse the existing reconcile and sweep patterns in src/ops.rs and src/lfs.rs: a pending-deletes list (rows marked removed, files removed by the hourly sweep) [chosen: mirrors LFS, which already solved the backup race]; (b) a lock between backups and deletes (couples unrelated paths). The DB-only lookup is a small helper next to load in src/api/releases.rs.
## Risks and rollback
## Execution prompt
## Steps
1. [ ] reconcile and doctor: orphan releases/<id> folders found, reported and removed; removal failures logged  <- CURRENT
2. [ ] make_tag: delete the new tag when the release insert fails; only ref-exists is a 409
3. [ ] DB-only release lookup for upload, update and delete_asset; deferred file deletion swept hourly; tests
## Resume here
## Docs impact
## Verification evidence
## Log
- 2026-10-22T00:57:55Z created
## Follow-ups captured
