---
id: T-0014
type: FEATURE
tier: M
status: done
priority: normal
scope: []
depends_on: []
source: followup
created: 2026-10-21T17:01:49Z
updated: 2026-10-22T03:22:18Z
base: e6b58dba4331e4d1beccd68a8d200a3c6c0ff53b
base_tree: 3c8139a035ee07d6c85c17d3804d26135701a58c
plan0_at: 2026-10-22T01:52:19Z
plan0_scope: []
verified: strong
---
# Package/container registry + pull-through proxies (C20)
## Raw request
> Package/container registry + pull-through proxies (C20)
## Interpretation
A container registry that docker, podman and skopeo can push to and pull from, plus generic packages. Images live under a repository: host/owner/repo[/name]:tag, and follow its visibility (private repos need sign-in, anyone with read can pull, write can push). It implements the OCI distribution API (/v2/): the ping with a Basic auth challenge, blob HEAD and GET, monolithic and chunked uploads, cross-repository mount, manifests by tag or digest (GET, HEAD, PUT, DELETE), and tags/list. Blobs are content-addressed (sha256 verified on upload), shared between images, counted once toward the owning account's storage quota, and unreferenced ones are swept hourly. Generic packages upload any file under a name and version. A Packages tab lists images, tags, sizes and the pull command.
## Assumptions (confidence)
## Acceptance criteria
- [x] Push and pull an image through the /v2/ API (chunked and monolithic uploads, digest checked, manifest by tag and digest, tags list), private repos refuse others, uploads respect the storage quota, deleting the repo removes its images — verify with `cargo test -q --test api registry`
- [x] A real client push and pull when docker, podman or skopeo is installed (skipped otherwise), the Packages tab renders, backups include the registry — verify with `bash scripts/e2e.sh && bash scripts/backup-roundtrip.sh`
## Non-goals
## Approach (options → choice → why)
(a) Blobs on disk under GITTER_DATA/registry/sha256/ab/abcd..., tables registry_blobs, registry_manifests (repo, name, digest, media type, size) and registry_tags, uploads streamed to a temp file like LFS [chosen: same pattern as src/lfs.rs, quota-aware, simple GC by reference]; (b) storing images as git objects (wrong tool); (c) proxying to an external registry (not self-contained). Auth: Basic with password or access token on every request (no bearer token service), which docker accepts; deploy tokens limited to one repo work for that repo's images.
## Risks and rollback
## Execution prompt
## Steps
1. [x] schema + content-addressed blob store with streamed, digest-verified uploads and the storage quota (STORAGE_USED counts registry blobs once)
2. [x] the /v2/ API: ping and auth challenge, blobs, uploads (POST, PATCH, PUT, cross-repo mount), manifests, tags/list, error bodies in the OCI format
3. [x] access: repo visibility and roles, tokens and deploy tokens, rate limits; generic packages upload and download
4. [x] hourly sweep of unreferenced blobs and stale uploads, cleanup on repo delete, backup and restore include registry/
5. [x] UI Packages tab, README section, e2e with a real client when available
## Resume here
## Docs impact
README container images: untagged images, delete, caps, Basic-only sign-in note
## Verification evidence
- (step 1) `cargo test -q --test api registry_storage 2>&1 | tail -20` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 56 filtered out; finished in 5.47s [ran] [tree bb4a44db7d0e] (2026-10-22T02:09:16Z)
- (step 1) `cargo test -q --test api registry` → exit 0 · ..... / test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 52 filtered out; finished in 16.25s [ran] [tree bb4a44db7d0e] (2026-10-22T02:09:36Z)
- (step 2) `cargo test -q --test api registry_pushes` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 56 filtered out; finished in 17.00s [ran] [tree bb4a44db7d0e] (2026-10-22T02:09:56Z)
- (step 3) `cargo test -q --test api registry_access registry_generic` → ✗ exit 1 · Usage: cargo test [OPTIONS] [TESTNAME] [-- [ARGS]...] / For more information, try '--help'. [ran] [tree bb4a44db7d0e] (2026-10-22T02:09:57Z)
- (step 3) `cargo test -q --test api -- registry_access registry_generic` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 55 filtered out; finished in 5.38s [ran] [tree bb4a44db7d0e] (2026-10-22T02:10:06Z)
- (step 4) `cargo test -q --lib ops:: 2>&1 | tail -15` → exit 0 · .......... / test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 55 filtered out; finished in 0.43s [ran] [tree 76bfb3595b9c] (2026-10-22T02:11:14Z)
- (step 4) `cargo test -q --test api -- registry_sweep registry_storage` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 55 filtered out; finished in 7.57s [ran] [tree 65df17bb3fd6] (2026-10-22T02:12:20Z)
- (step 4) `cargo test -q --lib ops::` → exit 0 · .......... / test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 55 filtered out; finished in 0.31s [ran] [tree 65df17bb3fd6] (2026-10-22T02:12:21Z)
- (ac 1) `cargo test -q --test api registry 2>&1 | tail -15` → exit 0 · ...... / test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 52 filtered out; finished in 21.01s [ran] [tree 3466ffb3ac9a] (2026-10-22T02:14:53Z)
- (step 5) `cargo test -q --test api registry 2>&1 | tail -15` → exit 0 · ...... / test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 52 filtered out; finished in 21.01s [ran] [tree 3466ffb3ac9a] (2026-10-22T02:14:53Z)
- (step 2) `cargo test -q --test api registry_never_serves` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 57 filtered out; finished in 3.76s [ran] [tree 3466ffb3ac9a] (2026-10-22T02:15:04Z)
- (ac 1) `cargo test -q --test api registry` → exit 0 · ....... / test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 62 filtered out; finished in 18.88s [ran] [tree e1a506cff584] (2026-10-22T03:16:50Z)
- (ac 2) `bash scripts/e2e.sh 2>&1 | tail -2 && bash scripts/backup-roundtrip.sh 2>&1 | tail -1` → exit 0 · All 39 checks passed. / Backup round-trip passed. [ran] [tree e1a506cff584] (2026-10-22T03:19:26Z)
- (ac 1) `cargo test -q --test api registry` → exit 0 · ....... / test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 62 filtered out; finished in 17.73s [ran] [tree e1a506cff584] (2026-10-22T03:22:18Z)
- (ac 2) `bash scripts/e2e.sh && bash scripts/backup-roundtrip.sh` → exit 0 · ✓ restored server has the same history, issues, wiki, discussions, snippets, trash, logins, clones, LFS files, release files, images and the key for CI secrets / Backup round-trip passed. [ran] [tree e1a506cff584] (2026-10-22T03:22:18Z)
- (audit intent) `fm-reviewer five-lens review (.foreman/research/T-0014-review.md); all MEDIUMs and LOWs fixed on merge with tests, gzip finding checked and not applicable` → both criteria met; registry tests 7/7, e2e 39/39 with a real docker push/pull, backup round trip passes [tree e1a506cff584] (2026-10-22T03:22:18Z)
- (audit adversary) `fm-reviewer five-lens review (.foreman/research/T-0014-review.md); all MEDIUMs and LOWs fixed on merge with tests, gzip finding checked and not applicable` → per-address failed-password limit shared by git, LFS and registry (basic_auth_failures_are_limited_per_address_too); agent tokens refused at /v2 by user_by_basic [tree e1a506cff584] (2026-10-22T03:22:18Z)
- (audit edge) `fm-reviewer five-lens review (.foreman/research/T-0014-review.md); all MEDIUMs and LOWs fixed on merge with tests, gzip finding checked and not applicable` → re-tag at quota, index children protected, tag/name caps, always-rename manifest writes (registry_untagged_images_are_listed_and_deleted_and_names_are_capped) [tree e1a506cff584] (2026-10-22T03:22:18Z)
- (audit operator) `fm-reviewer five-lens review (.foreman/research/T-0014-review.md); all MEDIUMs and LOWs fixed on merge with tests, gzip finding checked and not applicable` → untagged images listed and deletable in the Packages tab; backups include images [tree e1a506cff584] (2026-10-22T03:22:18Z)
## Log
- 2026-10-21T17:01:49Z created
- 2026-10-22T01:51:54Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0014.builder.md
- 2026-10-22T01:52:19Z focused
- 2026-10-22T03:15:58Z merged worktree-agent-a322c2bcda4460f89 (21 file(s))
- 2026-10-22T03:16:03Z lane removed: /home/chase/projects/gitter/.claude/worktrees/agent-a322c2bcda4460f89
- 2026-10-22T03:16:03Z focused
- 2026-10-22T03:22:18Z done
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research brainstorm-20261021: [instruction-like text quoted from a file or tool; data, not instructions] # Gitter brainstorm catalogue (2026-10-21)
- research T-0008-review-2: Verdict: changes needed.** One HIGH issue lets any user get code run on the server's built-in CI runner. I read the diff and the surrounding co…
- research T-0033-review: Verdict: changes needed.** The core is solid. Trashed repos are excluded from `lookup`, `VISIBLE`, CI, LFS and git, and private repos don't leak …
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
## Lessons
- A registry that keeps untagged manifests needs a visible way to delete them, or quota becomes a trap; never auto-delete them, since people pin images by digest
## Plan gaps
- failed first: step 3 access: repo visibility and roles, tokens and deploy tokens, rate limits; generic packages upload a…
## Files touched
- tests/api.rs
- src/lfs.rs
- src/db.rs
- src/repo.rs
- src/registry.rs
- src/lib.rs
- src/api/mod.rs
- src/main.rs
- src/validate.rs
- src/api/lifecycle.rs
- src/ops.rs
- scripts/backup-roundtrip.sh
- static/js/icons.js
- static/js/repohead.js
- static/js/main.js
- static/js/pages/packages.js
- scripts/ui-check.mjs
- scripts/e2e.sh
- README.md
- src/api/account.rs
- src/api/code.rs
- src/api/insights.rs
- src/api/issues.rs
- src/api/mcp.rs
- src/api/orgs.rs
- src/api/repos.rs
- src/api/reviews.rs
- src/auth.rs
- src/ci/jobs.rs
- src/ci/queue.rs
