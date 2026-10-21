---
id: T-0015
type: FEATURE
tier: M
status: captured
priority: normal
scope: []
depends_on: []
source: followup
created: 2026-10-21T17:01:50Z
updated: 2026-10-21T21:51:34Z
builder: 2026-10-21T21:51:34Z
builder_proc: archbox-hardened|pid:[4026531836]|3700487|2540304
---
# Git LFS (C22)
## Raw request
> Git LFS (C22)
## Interpretation
Serve Git LFS from the same binary so 'git lfs push/pull' just work against a Gitter remote: the LFS batch API at /{owner}/{repo}.git/info/lfs/objects/batch (same auth as git over HTTP: password, token, deploy token; read for download, write for upload), object upload/download endpoints that verify the sha256 while streaming, content-addressed storage under GITTER_DATA/lfs shared across repos (a table records which repo holds which object so access is per repo), objects included in gitter backup/restore/verify, a per-object size limit, LFS pointer files shown in the web UI as 'Stored with Git LFS' with size and a download link, and /raw serving the real file for pointers. Locking API answers 404 (git-lfs then skips lock checks).
## Assumptions (confidence)
## Acceptance criteria
- [ ] Batch API, upload and download work with real auth and reject a wrong hash — verify with `cargo test -q --test api lfs`
- [ ] A real git-lfs client pushes and clones a repo with LFS files (skipped if git-lfs isn't installed) — verify with `bash scripts/e2e.sh`
- [ ] Backups include LFS objects and restore them — verify with `bash scripts/backup-roundtrip.sh`
## Non-goals
## Approach (options → choice → why)
(a) LFS endpoints in smart_http-style handlers under the git URL space, objects at lfs/aa/bb/<oid>, table lfs_objects(repo_id, oid, size) [chosen: dedupes forks, per-repo access check stays in Repo::load]; (b) per-repo object folders (copies on fork); (c) an external LFS server (against one service). Upload streams to a temp file while hashing, renamed into place only if the sha256 and size match.
## Risks and rollback
## Execution prompt
## Steps
1. [ ] migration + storage + batch/upload/download endpoints  <- CURRENT
2. [ ] web UI: pointer files show LFS info; raw serves the object
3. [ ] backup/restore/verify include lfs/
4. [ ] tests: API, git-lfs e2e, backup round-trip
## Resume here
## Docs impact
## Verification evidence
## Log
- 2026-10-21T17:01:50Z created
- 2026-10-21T21:51:34Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0015.builder.md
## Follow-ups captured
