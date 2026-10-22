---
id: T-0061
type: CLEAN
tier: S
status: done
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T00:57:55Z
updated: 2026-10-22T20:01:50Z
base: 37fdf8fda25a5a79c3fe468fe7fe5a0db99c33b4
base_tree: 97916a1baaf7c43a261752d6641326bbbd18a0bd
plan0_at: 2026-10-22T19:38:28Z
plan0_scope: []
verified: strong
---
# Releases upkeep: sweep orphan releases/<id> folders in reconcile and log removal failures,
## Raw request
> Releases upkeep: sweep orphan releases/<id> folders in reconcile and log removal failures, delete a just-made tag when the release insert loses a race, map only ref-exists to 409 in make_tag, DB-only lookups for upload/update/delete_asset, defer release file deletion so backups never miss a file (from T-0044 review)
## Interpretation
Small robustness fixes the releases review found. Boot-time reconcile (src/ops.rs) and gitter doctor report and remove releases/<id> folders whose release row is gone; release file removals log failures instead of ignoring them; when creating a published release makes a new tag but the row insert then loses a race, the just-made tag is deleted again; make_tag maps only a ref-already-exists failure to 409 and reports other git errors as errors; upload, update and delete_asset look the release up with a DB-only query instead of the git-backed load; deleting a release file defers the file removal to the next sweep (like LFS) so a backup running at that moment never archives a row without its file.
## Assumptions (confidence)
## Acceptance criteria
- [x] Orphan release folders are swept, a lost insert race removes its new tag, non-race git errors aren't 409, and deleted files survive until the sweep so backups stay consistent — verify with `cargo test -q --test api releases`
- [x] Backups and doctor stay green — verify with `bash scripts/backup-roundtrip.sh`
## Non-goals
## Approach (options → choice → why)
(a) Reuse the existing reconcile and sweep patterns in src/ops.rs and src/lfs.rs: a pending-deletes list (rows marked removed, files removed by the hourly sweep) [chosen: mirrors LFS, which already solved the backup race]; (b) a lock between backups and deletes (couples unrelated paths). The DB-only lookup is a small helper next to load in src/api/releases.rs.
## Risks and rollback
## Execution prompt
## Steps
1. [x] reconcile and doctor: orphan releases/<id> folders found, reported and removed; removal failures logged
2. [x] make_tag: delete the new tag when the release insert fails; only ref-exists is a 409
3. [x] DB-only release lookup for upload, update and delete_asset; deferred file deletion swept hourly; tests
## Resume here
## Docs impact
none: internal upkeep, no user-facing behaviour or settings
## Verification evidence
- (step 1) `cargo test -q --lib reconcile_removes` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 109 filtered out; finished in 0.06s [ran] [tree b6ff3e08c5b9] (2026-10-22T19:44:13Z)
- (step 2) `cargo test -q --test api releases` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 72 filtered out; finished in 4.35s [ran] [tree b6ff3e08c5b9] (2026-10-22T19:44:18Z)
- (ac 1) `cargo test -q --test api releases 2>&1 | tail -15` → exit 0 · .... / test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 72 filtered out; finished in 3.02s [ran] [tree 386a83c5a60a] (2026-10-22T19:49:18Z)
- (step 3) `cargo test -q --test api releases 2>&1 | tail -15` → exit 0 · .... / test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 72 filtered out; finished in 3.02s [ran] [tree 386a83c5a60a] (2026-10-22T19:49:18Z)
- (ac 1) `cargo test -q --test api releases` → exit 0 · .... / test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 72 filtered out; finished in 2.38s [ran] [tree 22641e381c9e] (2026-10-22T20:00:45Z)
- (step 1) `cargo test -q --lib reconcile_removes` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 110 filtered out; finished in 0.03s [ran] [tree 22641e381c9e] (2026-10-22T20:01:01Z)
- (step 2) `cargo test -q --test api releases` → exit 0 · .... / test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 72 filtered out; finished in 1.54s [ran] [tree 22641e381c9e] (2026-10-22T20:01:03Z)
- (step 3) `cargo test -q --test api releases` → exit 0 · .... / test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 72 filtered out; finished in 1.62s [ran] [tree 22641e381c9e] (2026-10-22T20:01:05Z)
- (ac 2) `bash scripts/backup-roundtrip.sh` → exit 0 · ✓ restored server has the same history, issues, wiki, discussions, snippets, trash, logins, clones, LFS files, release files, images and the key for CI secrets / Backup round-trip passed. [ran] [tree 22641e381c9e] (2026-10-22T20:01:27Z)
- (ac 1) `cargo test -q --test api releases` → exit 0 · .... / test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 72 filtered out; finished in 2.62s [ran] [tree 22641e381c9e] (2026-10-22T20:01:42Z)
- (ac 2) `bash scripts/backup-roundtrip.sh` → exit 0 · ✓ restored server has the same history, issues, wiki, discussions, snippets, trash, logins, clones, LFS files, release files, images and the key for CI secrets / Backup round-trip passed. [ran] [tree 22641e381c9e] (2026-10-22T20:01:42Z)
- (audit intent) `fm-reviewer full review of the lane branch (intent/adversary/edge/operator/maintainer); 4 fixes applied in the lane (sweep outside the DB lock, single strip_prefix, sweep waits for backups, draft publish takes back its tag)` → all T-0061 points met; tag take-back keeps a race winner's tag (judged correct) [tree 22641e381c9e] (2026-10-22T20:01:42Z)
- (audit adversary) `fm-reviewer full review of the lane branch (intent/adversary/edge/operator/maintainer); 4 fixes applied in the lane (sweep outside the DB lock, single strip_prefix, sweep waits for backups, draft publish takes back its tag)` → sweep no longer holds the DB mutex while unlinking [tree 22641e381c9e] (2026-10-22T20:01:42Z)
- (audit edge) `fm-reviewer full review of the lane branch (intent/adversary/edge/operator/maintainer); 4 fixes applied in the lane (sweep outside the DB lock, single strip_prefix, sweep waits for backups, draft publish takes back its tag)` → tag names with a refs/tags/ prefix handled; publish failure takes back its tag [tree 22641e381c9e] (2026-10-22T20:01:42Z)
- (audit operator) `fm-reviewer full review of the lane branch (intent/adversary/edge/operator/maintainer); 4 fixes applied in the lane (sweep outside the DB lock, single strip_prefix, sweep waits for backups, draft publish takes back its tag)` → sweep skips while a backup runs; backup round-trip green [tree 22641e381c9e] (2026-10-22T20:01:42Z)
- (audit maintainer) `fm-reviewer full review of the lane branch (intent/adversary/edge/operator/maintainer); 4 fixes applied in the lane (sweep outside the DB lock, single strip_prefix, sweep waits for backups, draft publish takes back its tag)` → take_back_tag helper shared by create and update [tree 22641e381c9e] (2026-10-22T20:01:42Z)
- (audit self) `fm-reviewer full review of the lane branch (intent/adversary/edge/operator/maintainer); 4 fixes applied in the lane (sweep outside the DB lock, single strip_prefix, sweep waits for backups, draft publish takes back its tag)` → no findings [tree 22641e381c9e] (2026-10-22T20:01:42Z)
- (audit intent) `fm-reviewer full review of the lane branch (intent/adversary/edge/operator/maintainer); 4 fixes applied in the lane (sweep outside the DB lock, single strip_prefix, sweep waits for backups, draft publish takes back its tag)` → all T-0061 points met; tag take-back keeps a race winner's tag (judged correct) [tree 22641e381c9e] (2026-10-22T20:01:50Z)
- (audit adversary) `fm-reviewer full review of the lane branch (intent/adversary/edge/operator/maintainer); 4 fixes applied in the lane (sweep outside the DB lock, single strip_prefix, sweep waits for backups, draft publish takes back its tag)` → sweep no longer holds the DB mutex while unlinking [tree 22641e381c9e] (2026-10-22T20:01:50Z)
- (audit edge) `fm-reviewer full review of the lane branch (intent/adversary/edge/operator/maintainer); 4 fixes applied in the lane (sweep outside the DB lock, single strip_prefix, sweep waits for backups, draft publish takes back its tag)` → tag names with a refs/tags/ prefix handled; publish failure takes back its tag [tree 22641e381c9e] (2026-10-22T20:01:50Z)
- (audit operator) `fm-reviewer full review of the lane branch (intent/adversary/edge/operator/maintainer); 4 fixes applied in the lane (sweep outside the DB lock, single strip_prefix, sweep waits for backups, draft publish takes back its tag)` → sweep skips while a backup runs; backup round-trip green [tree 22641e381c9e] (2026-10-22T20:01:50Z)
- (audit maintainer) `fm-reviewer full review of the lane branch (intent/adversary/edge/operator/maintainer); 4 fixes applied in the lane (sweep outside the DB lock, single strip_prefix, sweep waits for backups, draft publish takes back its tag)` → take_back_tag helper shared by create and update [tree 22641e381c9e] (2026-10-22T20:01:50Z)
- (audit self) `fm-reviewer full review of the lane branch (intent/adversary/edge/operator/maintainer); 4 fixes applied in the lane (sweep outside the DB lock, single strip_prefix, sweep waits for backups, draft publish takes back its tag)` → no findings [tree 22641e381c9e] (2026-10-22T20:01:50Z)
## Log
- 2026-10-22T00:57:55Z created
- 2026-10-22T19:38:12Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0061.builder.md
- 2026-10-22T19:38:28Z focused
- 2026-10-22T19:59:30Z merged worktree-agent-a4dd17775a328ba04 (4 file(s))
- 2026-10-22T20:00:01Z lane removed: /home/chase/projects/gitter/.claude/worktrees/agent-a4dd17775a328ba04
- 2026-10-22T20:00:06Z focused
- 2026-10-22T20:01:50Z done
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research T-0044-review: Approve with nits. I found no security defect. I did not run anything: the review is from reading the diff and the worktree, so the passing tests…
- research T-0012-review: Verdict: changes needed.** I found three MEDIUM issues and several LOW ones. I found no path traversal, symlink, XXE, auth or XSS holes.
- research T-0016-review: HIGH — src/api/snippets.rs:137,261 and src/api/wiki.rs:148 — snippets and wikis bypass the storage quota [stale: src/api/discussions.rs, src/api/…
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
## Behaviour lock
none before edits (built in a builder lane); the existing releases tests (asset upload/delete, tag conflict) were kept and pass unchanged except the delete test now also checks the file stays until the sweep
## Lessons
- Deferred file deletion must also wait for backups in progress, not only for row commits.
## Files touched
- src/api/releases.rs
- src/main.rs
- src/ops.rs
- tests/api.rs
- README.md
- scripts/ui-check.mjs
- src/api/account.rs
- src/api/mcp.rs
- src/api/mod.rs
- src/api/oauth.rs
- src/api/security.rs
- src/db.rs
- src/net.rs
- static/css/account.css
- static/js/lib.js
- static/js/main.js
- static/js/pages/admin.js
- static/js/pages/auth.js
- static/js/pages/settings.js
- tests/oauth.rs
