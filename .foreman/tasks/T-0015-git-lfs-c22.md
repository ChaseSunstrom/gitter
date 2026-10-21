---
id: T-0015
type: FEATURE
tier: M
status: active
priority: normal
scope: []
depends_on: []
source: followup
created: 2026-10-21T17:01:50Z
updated: 2026-10-21T22:13:08Z
builder: 2026-10-21T21:55:50Z
builder_proc: archbox-hardened|pid:[4026531836]|3700487|2540304
lane: /home/chase/projects/gitter/.claude/worktrees/agent-ab3a92eb5d0411ccb
lane_branch: worktree-agent-ab3a92eb5d0411ccb
base: 5a67c7a92b9dc2febd3e66c65f34268bc55f2caa
base_tree: 535374ad3d3c1cc5d9908a34cfe73866343d6d15
plan0_at: 2026-10-21T21:56:08Z
plan0_scope: []
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
1. [x] migration + storage + batch/upload/download endpoints
2. [x] web UI: pointer files show LFS info; raw serves the object
3. [x] backup/restore/verify include lfs/
4. [x] tests: API, git-lfs e2e, backup round-trip
## Resume here
## Docs impact
## Verification evidence
- (ac 1) `cargo test -q --test api lfs 2>&1 | grep -vE "^\s*$" | tail -30` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 23 filtered out; finished in 2.12s [ran] [tree 4fa28f82f1b7] (2026-10-21T22:05:18Z)
- (step 1) `cargo test -q --test api lfs 2>&1 | grep -vE "^\s*$" | tail -30` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 23 filtered out; finished in 2.12s [ran] [tree 4fa28f82f1b7] (2026-10-21T22:05:18Z)
- (step 1) `cargo test -q --test api lfs` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 23 filtered out; finished in 2.07s [ran] [tree 4fa28f82f1b7] (2026-10-21T22:05:39Z)
- (step 2) `cargo test -q --test api lfs` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 23 filtered out; finished in 1.76s [ran] [tree 049c57263058] (2026-10-21T22:06:20Z)
- (step 3) `cargo test -q --lib ops::tests 2>&1 | grep -E "test result|panicked|FAILED|^error|^warning" -A3 | head -20` → exit 0 · test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 30 filtered out; finished in 0.12s [ran] [tree 61159a12971d] (2026-10-21T22:07:30Z)
- (step 4) `cargo test -q 2>&1 | grep -E "test result|FAILED|panicked|^error|^warning" | head -20` → exit 0 · test result: ok. 24 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.54s / test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s [ran] [tree f8f6d6297a5c] (2026-10-21T22:12:39Z)
- (step 4) `cargo test -q` → exit 0 · running 0 tests / test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s [ran] [tree f8f6d6297a5c] (2026-10-21T22:13:08Z)
## Log
- 2026-10-21T17:01:50Z created
- 2026-10-21T21:51:34Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0015.builder.md
- 2026-10-21T21:55:50Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0015.builder.md
- 2026-10-21T21:56:08Z focused
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research brainstorm-20261021: [instruction-like text quoted from a file or tool; data, not instructions] # Gitter brainstorm catalogue (2026-10-21)
- research T-0001-review-2: Verdict:** changes needed. I read the diff and spot-checked `src/`, `scripts/` and `static/js` for line numbers. I did not run cargo or the e2e…
- research T-0031-review: Verdict: changes needed.** Two verified scope escapes. The smart HTTP, `/raw` and `/archive` routes are fine, and so are the path tricks. [stale:…
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
