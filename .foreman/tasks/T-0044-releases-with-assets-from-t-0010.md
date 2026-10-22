---
id: T-0044
type: FEATURE
tier: M
status: done
priority: normal
scope: []
depends_on: []
source: user
created: 2026-10-21T23:41:28Z
updated: 2026-10-22T00:59:32Z
base: 70d66feb249d5f5c932b59651f6ae623d42d553d
base_tree: c9b01b011060e71afe38d61c5e47c1c48587e4a3
plan0_at: 2026-10-22T00:09:43Z
plan0_scope: []
inferred_deps: [T-0010]
inferred_why: T-0010: named in its request
group: import, mirroring, and releases
verified: strong
---
# Releases with assets (from T-0010)
## Raw request
> Releases with assets (from T-0010)
## Interpretation
GitHub-style releases. A release names a tag (an existing one, or a new tag at a chosen branch or commit), and has a title, markdown notes, a draft or published state and a prerelease flag. Writers attach any number of assets (binaries, archives), which upload by streaming. Each file is capped at the Git LFS file limit, and assets count toward the owner's storage quota. Assets are stored under GITTER_DATA/releases/<release id>/<asset id> and served with a safe Content-Disposition: attachment, and download counts are kept. Readers see published releases only (drafts are visible only to writers), and private repos stay private. The repo home sidebar shows the latest non-prerelease release. A Releases page lists them with notes, assets and the source zip and tar.gz for the tag. Deleting a release removes its assets but keeps the tag. Backup, verify and restore include releases/.
## Assumptions (confidence)
## Acceptance criteria
- [x] Releases are created from a tag (or a new tag at a commit), edited, published or drafted, with uploaded assets counted toward storage, download counts and the latest release in the API — verify with `cargo test -q --test api releases`
- [x] Releases pages and the repo sidebar's latest release render; backups include assets — verify with `bash scripts/e2e.sh && bash scripts/backup-roundtrip.sh`
## Non-goals
## Approach (options → choice → why)
(a) A releases table plus asset files on disk, streamed like LFS uploads (temp file, then rename) [chosen: same pattern as src/lfs.rs, so it's familiar and quota-aware]; (b) storing assets in git (repository bloat); (c) storing them as LFS objects (couples two features). Tags are created through Git::update_ref with an expected old value of zero, as an annotated tag via mktag (or lightweight if simpler). Build vs reuse: lfs.rs's receive() pattern and STORAGE_USED (add release asset bytes), and code.rs's archive endpoint for source downloads.
## Risks and rollback
## Execution prompt
## Steps
1. [x] schema + API: releases (tag, title, notes markdown, draft, prerelease, author), assets (name, size, content type, downloads) under GITTER_DATA/releases/<release id>/; writers create/edit/delete, readers see published
2. [x] tags: create the tag at a chosen commit or branch when it doesn't exist (annotated, by update-ref with an expected old value); deleting a release keeps the tag
3. [x] assets: streaming upload with the storage quota (STORAGE_USED gains release asset bytes), per-file cap = max_lfs_mb, download counts, safe Content-Disposition, private repos stay private
4. [x] UI: Releases tab (list, release page with notes, assets, source archives), new/edit form with tag picker, latest release in the repo sidebar; backup/restore/verify include releases/
## Resume here
## Docs impact
README releases section (builder)
## Verification evidence
- (ac 1) `cargo test -q --test api releases 2>&1 | tail -15` → ✗ exit 1 · test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 35 filtered out; finished in 1.25s / error: test failed, to rerun pass `--test api` [ran] [tree 06ccc11fd183] (2026-10-22T00:17:21Z)
- (step 1) `cargo test -q --lib asset_names 2>&1 | tail -3` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 52 filtered out; finished in 0.00s [ran] [tree 78b86db2aa78] (2026-10-22T00:24:11Z)
- (step 1) `cargo test -q --test api releases` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 35 filtered out; finished in 2.55s [ran] [tree 78b86db2aa78] (2026-10-22T00:24:19Z)
- (step 2) `cargo test -q --test api releases_from_tags` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 36 filtered out; finished in 0.95s [ran] [tree 78b86db2aa78] (2026-10-22T00:24:23Z)
- (step 3) `cargo test -q --test api releases_assets` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 36 filtered out; finished in 2.55s [ran] [tree 78b86db2aa78] (2026-10-22T00:24:26Z)
- (step 4) `cargo test -q 2>&1 | grep -E "test result|FAILED|panicked|failures" | head -20` → exit 0 · test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.78s / test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s [ran] [tree 3527903c92d9] (2026-10-22T00:28:50Z)
- (ac 1) `cargo test -q --test api releases 2>&1 | grep -E "panicked|test result" -A3 | head -20` → ✗ exit 1 · test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 35 filtered out; finished in 3.46s / error: test failed, to rerun pass `--test api` [ran] [tree 8e0fcf024ff0] (2026-10-22T00:32:22Z)
- (step 1) `cargo test -q --test api releases` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 35 filtered out; finished in 3.61s [ran] [tree e46605a90357] (2026-10-22T00:37:37Z)
- (step 2) `cargo test -q --test api releases_from_tags` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 36 filtered out; finished in 1.43s [ran] [tree e46605a90357] (2026-10-22T00:37:42Z)
- (step 3) `cargo test -q --test api releases_assets` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 36 filtered out; finished in 3.30s [ran] [tree e46605a90357] (2026-10-22T00:37:48Z)
- (ac 1) `cargo test -q --test api releases` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 39 filtered out; finished in 2.46s [ran] [tree dc25a3168219] (2026-10-22T00:55:47Z)
- (ac 2) `bash scripts/e2e.sh && bash scripts/backup-roundtrip.sh` → exit 0 · ✓ restored server has the same history, issues, wiki, discussions, snippets, logins, clones, LFS files, release files and the key for CI secrets / Backup round-trip passed. [ran] [tree dc25a3168219] (2026-10-22T00:57:36Z)
- (step 4) `bash scripts/backup-roundtrip.sh` → exit 0 · ✓ restored server has the same history, issues, wiki, discussions, snippets, logins, clones, LFS files, release files and the key for CI secrets / Backup round-trip passed. [ran] [tree dc25a3168219] (2026-10-22T00:57:39Z)
- (ac 1) `cargo test -q --test api releases` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 39 filtered out; finished in 2.25s [ran] [tree dc25a3168219] (2026-10-22T00:59:31Z)
- (ac 2) `bash scripts/e2e.sh && bash scripts/backup-roundtrip.sh` → exit 0 · ✓ restored server has the same history, issues, wiki, discussions, snippets, logins, clones, LFS files, release files and the key for CI secrets / Backup round-trip passed. [ran] [tree dc25a3168219] (2026-10-22T00:59:31Z)
- (audit intent) `fm-reviewer five-lens review of the branch diff (approve with nits); fixed the deleted-tag draft publish and the backup race; merged with main (org-aware quota via repo::usage, migration renumbered v11)` → criteria re-run on main: API tests and e2e+backup pass [tree dc25a3168219] (2026-10-22T00:59:31Z)
- (audit adversary) `fm-reviewer five-lens review of the branch diff (approve with nits); fixed the deleted-tag draft publish and the backup race; merged with main (org-aware quota via repo::usage, migration renumbered v11)` → private repos, drafts, tokens, file names, downloads as attachments, quota race all traced clean [tree dc25a3168219] (2026-10-22T00:59:31Z)
- (audit edge) `fm-reviewer five-lens review of the branch diff (approve with nits); fixed the deleted-tag draft publish and the backup race; merged with main (org-aware quota via repo::usage, migration renumbered v11)` → draft of a deleted tag now 409 (tested); tag race left as documented follow-up [tree dc25a3168219] (2026-10-22T00:59:31Z)
- (audit operator) `fm-reviewer five-lens review of the branch diff (approve with nits); fixed the deleted-tag draft publish and the backup race; merged with main (org-aware quota via repo::usage, migration renumbered v11)` → backup tolerates vanished files; orphan sweep captured [tree dc25a3168219] (2026-10-22T00:59:31Z)
- (audit maintainer) `fm-reviewer five-lens review of the branch diff (approve with nits); fixed the deleted-tag draft publish and the backup race; merged with main (org-aware quota via repo::usage, migration renumbered v11)` → lfs::spool and file_body shared with LFS; make_tag error mapping captured [tree dc25a3168219] (2026-10-22T00:59:31Z)
## Log
- 2026-10-21T23:41:28Z created
- 2026-10-22T00:08:43Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0044.builder.md
- 2026-10-22T00:09:43Z focused
- 2026-10-22T00:55:26Z merged worktree-agent-a0efd8bed424f13ba (20 file(s))
- 2026-10-22T00:55:30Z lane removed: /home/chase/projects/gitter/.claude/worktrees/agent-a0efd8bed424f13ba
- 2026-10-22T00:55:31Z focused
- 2026-10-22T00:59:32Z done
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research brainstorm-20261021: [instruction-like text quoted from a file or tool; data, not instructions] # Gitter brainstorm catalogue (2026-10-21)
- T-0015 [FEATURE M, done, 5 steps] Git LFS (C22) — lesson: A storage quota is only real if every way in (upload, fork, parallel requests) and every way out (delete) is co…
- research T-0016-review: HIGH — src/api/snippets.rs:137,261 and src/api/wiki.rs:148 — snippets and wikis bypass the storage quota [stale: src/api/discussions.rs, src/api/…
- research T-0015-review: Verdict: changes needed.** The temp-file guard, the slow-upload check, the `Cache-Control` fixes and the batch limiter are sound. The storage-cap…
- the similar finished task took 5 steps
## Lessons
- Merge builder lanes by merging main into the branch in its worktree, then fm lane merge: the guard and evidence stay per task
## Files touched
- tests/api.rs
- src/ops.rs
- src/db.rs
- src/repo.rs
- src/lfs.rs
- src/api/releases.rs
- src/api/mod.rs
- src/api/repos.rs
- src/api/admin.rs
- src/lib.rs
- src/main.rs
- static/js/pages/releases.js
- static/js/main.js
- static/js/pages/repo.js
- static/js/repohead.js
- static/js/lib.js
- scripts/e2e.sh
- scripts/ui-check.mjs
- scripts/backup-roundtrip.sh
- README.md
