---
id: T-0016
type: FEATURE
tier: M
status: done
priority: normal
scope: []
depends_on: []
source: followup
created: 2026-10-21T17:01:51Z
updated: 2026-10-21T23:06:33Z
base: 5a67c7a92b9dc2febd3e66c65f34268bc55f2caa
base_tree: 535374ad3d3c1cc5d9908a34cfe73866343d6d15
plan0_at: 2026-10-21T21:56:10Z
plan0_scope: []
verified: strong
---
# Wiki, discussions, snippets (C26)
## Raw request
> Wiki, discussions, snippets (C26)
## Interpretation
Three collaboration spaces: (1) a wiki per repository: markdown pages with history, stored as a real bare git repo under GITTER_DATA/wikis/<repo id>.git (so it's versioned and can later be cloned), edited in the browser (create, edit, rename, delete, page list, page history, Home as the landing page), readable like the repo (private repo = private wiki), writable by people who can write the repo; (2) discussions per repository: a forum separate from issues with categories (General, Q&A, Ideas, Announcements), threads with markdown comments, Q&A threads can mark one answer, upvotes, anyone signed in can post on public repos; (3) snippets (like gists): a user's small multi-file pastes, public or secret (unlisted link), each stored as a bare git repo under GITTER_DATA/snippets/<id>.git with revisions, syntax highlighted view, raw files, a 'Your snippets' page and a public list. All three need to survive backup/restore/verify/doctor and boot reconcile (wikis/ and snippets/ must not be treated as orphan repos).
## Assumptions (confidence)
## Acceptance criteria
- [x] Wiki pages can be created, edited with conflict detection, listed and their history viewed; private repo wikis stay private — verify with `cargo test -q --test api wiki`
- [x] Discussions: categories, threads, comments, answers and votes work with the repo's access rules — verify with `cargo test -q --test api discussion`
- [x] Snippets: public and secret snippets with revisions and raw files — verify with `cargo test -q --test api snippet`
- [x] Wiki, discussions and snippets pages render and work in a browser — verify with `bash scripts/e2e.sh`
- [x] Backups include wikis and snippets and restore them — verify with `bash scripts/backup-roundtrip.sh`
## Non-goals
## Approach (options → choice → why)
(a) Wiki and snippets as bare git repos via the existing Git helpers (commit_files with expect_old for edit conflicts), discussions as SQLite tables [chosen: history for free, same safety as code]; (b) wiki pages in SQLite rows (no history, can't clone); (c) wiki as a folder in the main repo (pollutes the code). Discussions reuse the markdown renderer and comment UI patterns from issues.
## Risks and rollback
## Execution prompt
## Steps
1. [x] wiki: storage, API, pages
2. [x] discussions: tables, API, pages
3. [x] snippets: storage, API, pages
4. [x] backup/restore/reconcile/doctor know wikis/ and snippets/
5. [x] tests + browser checks
6. [x] review fixes: wiki and snippet sizes count toward storage quotas, snippet row written after its files, moderator reply deletions audited
## Resume here
## Docs impact
README: GITTER_DATA lists repos/, wikis/, snippets/ and lfs/
## Verification evidence
- (step 1) `cargo test -q --test api -- wiki discussion snippet 2>&1 | grep -E "panicked|left:|right:|test result|assertion|internal error" -A3 | head -40` → exit 0 · test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 23 filtered out; finished in 1.53s [ran] [tree 9b7f9441b7af] (2026-10-21T22:08:24Z)
- (step 2) `cargo test -q --lib 2>&1 | grep -E "^error|^warning|panicked|test result|FAILED" -A5 | head -30` → exit 0 · test result: ok. 36 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.21s [ran] [tree 3784721e297f] (2026-10-21T22:10:00Z)
- (step 3) `cargo test -q 2>&1 | grep -E "test result|FAILED|panicked|^error|^warning" | head -20` → exit 0 · test result: ok. 26 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 13.40s / test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s [ran] [tree bf2c39fee0c2] (2026-10-21T22:19:29Z)
- (step 1) `cargo test -q --test api wiki` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 25 filtered out; finished in 1.05s [ran] [tree 984ee6ee04ec] (2026-10-21T22:21:29Z)
- (step 2) `cargo test -q --test api discussion` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 25 filtered out; finished in 1.29s [ran] [tree 984ee6ee04ec] (2026-10-21T22:21:34Z)
- (step 3) `cargo test -q --test api snippet` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 25 filtered out; finished in 1.00s [ran] [tree 984ee6ee04ec] (2026-10-21T22:21:35Z)
- (ac 1) `cargo test -q --test api wiki` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 32 filtered out; finished in 1.35s [ran] [tree 3e7d452b587a] (2026-10-21T22:55:39Z)
- (ac 2) `cargo test -q --test api discussion` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 33 filtered out; finished in 1.40s [ran] [tree 3e7d452b587a] (2026-10-21T22:55:41Z)
- (ac 3) `cargo test -q --test api snippet` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 32 filtered out; finished in 1.19s [ran] [tree 3e7d452b587a] (2026-10-21T22:55:43Z)
- (ac 4) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 24 checks passed. [ran] [tree 3e7d452b587a] (2026-10-21T22:57:00Z)
- (ac 5) `bash scripts/backup-roundtrip.sh` → exit 0 · ✓ restored server has the same history, issues, wiki, discussions, snippets, logins, clones and LFS files / Backup round-trip passed. [ran] [tree 3e7d452b587a] (2026-10-21T22:57:03Z)
- (step 4) `bash scripts/backup-roundtrip.sh` → exit 0 · ✓ restored server has the same history, issues, wiki, discussions, snippets, logins, clones and LFS files / Backup round-trip passed. [ran] [tree 3e7d452b587a] (2026-10-21T22:57:07Z)
- (step 5) `cargo test -q` → exit 0 · running 0 tests / test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s [ran] [tree 3e7d452b587a] (2026-10-21T22:57:23Z)
- (ac 1) `cargo test -q --test api wiki` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 32 filtered out; finished in 1.38s [ran] [tree 19589b5d75d8] (2026-10-21T23:03:05Z)
- (ac 2) `cargo test -q --test api discussion` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 34 filtered out; finished in 1.23s [ran] [tree 19589b5d75d8] (2026-10-21T23:03:07Z)
- (ac 3) `cargo test -q --test api snippet` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 32 filtered out; finished in 1.20s [ran] [tree 19589b5d75d8] (2026-10-21T23:03:08Z)
- (ac 4) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 24 checks passed. [ran] [tree 19589b5d75d8] (2026-10-21T23:04:26Z)
- (ac 5) `bash scripts/backup-roundtrip.sh` → exit 0 · ✓ restored server has the same history, issues, wiki, discussions, snippets, logins, clones and LFS files / Backup round-trip passed. [ran] [tree 19589b5d75d8] (2026-10-21T23:04:29Z)
- (step 4) `bash scripts/backup-roundtrip.sh` → exit 0 · ✓ restored server has the same history, issues, wiki, discussions, snippets, logins, clones and LFS files / Backup round-trip passed. [ran] [tree 19589b5d75d8] (2026-10-21T23:04:33Z)
- (step 5) `cargo test -q` → exit 0 · running 0 tests / test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s [ran] [tree 19589b5d75d8] (2026-10-21T23:04:52Z)
- (step 6) `cargo test -q --test api snippets_and_wikis` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 34 filtered out; finished in 0.90s [ran] [tree 19589b5d75d8] (2026-10-21T23:04:53Z)
- (audit intent) `fm-reviewer` → all criteria met; quota gap for wikis/snippets fixed with a test [tree 19589b5d75d8] (2026-10-21T23:05:09Z)
- (audit adversary) `fm-reviewer` → secret snippet ids/lists/raw, hidden/private wikis, git injection, limited tokens, XSS in md.js: no issues; quota bypass fixed [tree 19589b5d75d8] (2026-10-21T23:05:09Z)
- (audit edge) `fm-reviewer` → cascades, delete_user, old backups, doctor wiki orphans OK; snippet create order fixed; numbering reuse accepted [tree 19589b5d75d8] (2026-10-21T23:05:09Z)
- (ac 1) `cargo test -q --test api wiki` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 32 filtered out; finished in 1.31s [ran] [tree 19589b5d75d8] (2026-10-21T23:06:33Z)
- (ac 2) `cargo test -q --test api discussion` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 34 filtered out; finished in 1.37s [ran] [tree 19589b5d75d8] (2026-10-21T23:06:33Z)
- (ac 3) `cargo test -q --test api snippet` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 32 filtered out; finished in 1.24s [ran] [tree 19589b5d75d8] (2026-10-21T23:06:33Z)
- (ac 4) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 24 checks passed. [ran] [tree 19589b5d75d8] (2026-10-21T23:06:33Z)
- (ac 5) `bash scripts/backup-roundtrip.sh` → exit 0 · ✓ restored server has the same history, issues, wiki, discussions, snippets, logins, clones and LFS files / Backup round-trip passed. [ran] [tree 19589b5d75d8] (2026-10-21T23:06:33Z)
- (audit edge) `fm-reviewer on the branch diff and merged files; 3 findings fixed, 2 accepted` → old backups without wikis/snippets restore; discussion numbers may be reused after deleting the newest (accepted) [tree 19589b5d75d8] (2026-10-21T23:06:33Z)
## Log
- 2026-10-21T17:01:51Z created
- 2026-10-21T21:55:50Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0016.builder.md
- 2026-10-21T21:56:10Z focused
- 2026-10-21T22:55:05Z lane removed: /home/chase/projects/gitter/.claude/worktrees/agent-a0dc1e40f9e40af81
- 2026-10-21T22:55:13Z focused
- 2026-10-21T23:06:33Z done
- 2026-10-21T23:06:33Z committed f2529f0 (12 files)
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research brainstorm-20261021: [instruction-like text quoted from a file or tool; data, not instructions] # Gitter brainstorm catalogue (2026-10-21)
- research T-0004-review: Verdict: changes needed.** None of these block the four acceptance scripts. Findings 1, 2 and 3 break the documented Docker workflow. I verified …
- research T-0001-review-3: Verdict: changes needed.** I found no HIGH issues, but several MEDIUM defects and one test-coverage gap. I could not run `cargo test`, `cargo c…
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
## Lessons
- Every new kind of stored content (wiki, snippet, LFS) needs to join the storage quota the day it lands; grep storage_left when adding one.
## Plan gaps
- added late: review fixes: wiki and snippet sizes count toward storage quotas, snippet row written after its files, moderator reply …
## Files touched
- tests/api.rs
- src/db.rs
- src/api/code.rs
- src/api/issues.rs
- src/api/wiki.rs
- src/api/discussions.rs
- src/api/snippets.rs
- src/api/mod.rs
- src/lib.rs
- src/validate.rs
- src/api/repos.rs
- src/api/admin.rs
- src/git.rs
- src/ops.rs
- scripts/backup-roundtrip.sh
- static/js/icons.js
- static/js/md.js
- static/js/lib.js
- static/js/main.js
- static/js/repohead.js
- static/js/pages/wiki.js
- static/js/pages/discussions.js
- static/js/pages/snippets.js
- static/css/issues.css
- scripts/e2e.sh
- scripts/ui-check.mjs
- README.md
- src/main.rs
- src/repo.rs
- .gitignore
