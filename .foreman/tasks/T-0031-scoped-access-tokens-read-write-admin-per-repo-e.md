---
id: T-0031
type: SECURITY
tier: M
status: done
priority: normal
scope: []
depends_on: []
source: followup
created: 2026-10-21T21:11:21Z
updated: 2026-10-21T21:55:25Z
base: 29c888265d5f758b743b6a0d3a43e85cdafc75aa
base_tree: 7255579380c54aa09fd23ef0f146d44fa616e057
plan0_at: 2026-10-21T21:42:03Z
plan0_scope: []
verified: strong
---
# Scoped access tokens (read/write/admin, per-repo, expiry) and deploy keys (split from T-00
## Raw request
> Scoped access tokens (read/write/admin, per-repo, expiry) and deploy keys (split from T-0009)
## Interpretation
Tokens today can do anything their owner can. Give each token an access level (read only / read and write repositories / full account), an optional single repository it's limited to, and an optional expiry. Repository owners can make deploy tokens from repo settings: a token for exactly one repository, read-only or read-write, for CI servers and deploy boxes. Enforced for git over HTTP and the API alike.
## Assumptions (confidence)
## Acceptance criteria
- [x] Read-only tokens can clone and read but not push or write; repo-limited tokens can't touch other private repos; expired tokens stop working — verify with `cargo test -q --test api scoped_tokens`
- [x] Deploy tokens are created and listed in repo settings and work for that repo only — verify with `cargo test -q --test api deploy_tokens`
- [x] Token and deploy-token forms work in a browser — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
(a) Columns on tokens (scope, repo_id, expires_at); the token's limits ride on the User value so the existing access() check and one API middleware enforce them [chosen: one choke point for repos, one for routes]; (b) a separate deploy_keys table (duplicates token auth); (c) per-route scope annotations (spread over 60 handlers, easy to miss one). SSH deploy keys come with SSH (T-0012).
## Risks and rollback
## Execution prompt
## Steps
1. [x] migration + token limits on User + access() + API scope middleware
2. [x] token form: access level, repository, expiry
3. [x] deploy tokens in repo settings
4. [x] tests + browser check
## Resume here
## Docs impact
README: token levels, deploy tokens and the 2FA-needs-a-token rule
## Verification evidence
- (step 1) `cargo test -q --test api -- scoped_tokens deploy_tokens access_tokens` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 19 filtered out; finished in 1.52s [ran] [tree 98464c91feae] (2026-10-21T21:45:16Z)
- (step 2) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 22 checks passed. [ran] [tree 0c832096b964] (2026-10-21T21:48:06Z)
- (step 3) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 22 checks passed. [ran] [tree 0c832096b964] (2026-10-21T21:49:00Z)
- (step 4) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 22 checks passed. [ran] [tree 0c832096b964] (2026-10-21T21:49:52Z)
- (step 1) `cargo test -q --test api -- scoped_tokens deploy_tokens access_tokens` → exit 0 · .... / test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 19 filtered out; finished in 1.84s [ran] [tree f051c092e165] (2026-10-21T21:52:52Z)
- (step 4) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 22 checks passed. [ran] [tree f051c092e165] (2026-10-21T21:53:51Z)
- (ac 1) `cargo test -q --test api scoped_tokens` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 22 filtered out; finished in 0.47s [ran] [tree f051c092e165] (2026-10-21T21:55:15Z)
- (ac 2) `cargo test -q --test api deploy_tokens` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 21 filtered out; finished in 1.42s [ran] [tree f051c092e165] (2026-10-21T21:55:15Z)
- (ac 3) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 22 checks passed. [ran] [tree f051c092e165] (2026-10-21T21:55:15Z)
- (audit adversary) `one fm-reviewer security pass (research T-0031-review); 2 HIGH and 2 LOW fixed with a regression test; smart HTTP, raw, archive and path tricks verified safe` → limited tokens can't reach account/admin/security routes; deploy-token writes must target their own repo (checked against the path's repo id); feed and lists treat deploy tokens as signed out; email hidden from limited tokens [tree f051c092e165] (2026-10-21T21:55:15Z)
- (audit intent) `one fm-reviewer security pass (research T-0031-review); 2 HIGH and 2 LOW fixed with a regression test; smart HTTP, raw, archive and path tricks verified safe` → read/write/all levels, one-repo limit, expiry and deploy tokens all enforced for git over HTTP and the API, each with a test [tree f051c092e165] (2026-10-21T21:55:15Z)
- (audit edge) `one fm-reviewer security pass (research T-0031-review); 2 HIGH and 2 LOW fixed with a regression test; smart HTTP, raw, archive and path tricks verified safe` → expired tokens 401; deleting a repo deletes its deploy tokens (FK cascade); unknown stored scope falls back to read [tree f051c092e165] (2026-10-21T21:55:15Z)
- (audit operator) `one fm-reviewer security pass (research T-0031-review); 2 HIGH and 2 LOW fixed with a regression test; smart HTTP, raw, archive and path tricks verified safe` → deploy tokens live in repo settings with creator, last use and expiry; README shows the x-access-token clone URL [tree f051c092e165] (2026-10-21T21:55:15Z)
- (audit maintainer) `one fm-reviewer security pass (research T-0031-review); 2 HIGH and 2 LOW fixed with a regression test; smart HTTP, raw, archive and path tricks verified safe` → limits ride on User so access() and one middleware enforce everything [tree f051c092e165] (2026-10-21T21:55:15Z)
- (audit adversary) `one fm-reviewer security pass (research T-0031-review); 2 HIGH and 2 LOW fixed with a regression test; smart HTTP, raw, archive and path tricks verified safe` → limited tokens can't reach account/admin/security routes; deploy-token writes must target their own repo (checked against the path's repo id); feed and lists treat deploy tokens as signed out; email hidden from limited tokens [tree bb3893e68440] (2026-10-21T21:55:24Z)
- (audit intent) `one fm-reviewer security pass (research T-0031-review); 2 HIGH and 2 LOW fixed with a regression test; smart HTTP, raw, archive and path tricks verified safe` → read/write/all levels, one-repo limit, expiry and deploy tokens all enforced for git over HTTP and the API, each with a test [tree bb3893e68440] (2026-10-21T21:55:24Z)
- (audit edge) `one fm-reviewer security pass (research T-0031-review); 2 HIGH and 2 LOW fixed with a regression test; smart HTTP, raw, archive and path tricks verified safe` → expired tokens 401; deleting a repo deletes its deploy tokens (FK cascade); unknown stored scope falls back to read [tree bb3893e68440] (2026-10-21T21:55:24Z)
- (audit operator) `one fm-reviewer security pass (research T-0031-review); 2 HIGH and 2 LOW fixed with a regression test; smart HTTP, raw, archive and path tricks verified safe` → deploy tokens live in repo settings with creator, last use and expiry; README explains how to clone with one [tree bb3893e68440] (2026-10-21T21:55:24Z)
- (audit maintainer) `one fm-reviewer security pass (research T-0031-review); 2 HIGH and 2 LOW fixed with a regression test; smart HTTP, raw, archive and path tricks verified safe` → limits ride on User so access() and one middleware enforce everything [tree bb3893e68440] (2026-10-21T21:55:24Z)
## Log
- 2026-10-21T21:11:21Z created
- 2026-10-21T21:42:03Z focused
- 2026-10-21T21:55:25Z done
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research brainstorm-20261021: [instruction-like text quoted from a file or tool; data, not instructions] # Gitter brainstorm catalogue (2026-10-21)
- research T-0009-review: Verdict: changes needed.** I found no way to sign in without the second factor through login, signup or git basic auth. Session revoke is scoped …
- research T-0001-review-1: Verdict: changes needed.** Nothing was executed (read-only), so every finding comes from reading the code. The first one is the most serious. […
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
## Lessons
- Scope checks keyed on URL shape aren't enough for per-repo tokens: writes must also be pinned to the token's own repo, and every list query must go through the visibility filter
## Files touched
- src/auth.rs
- src/db.rs
- src/repo.rs
- src/api/mod.rs
- src/lib.rs
- src/api/account.rs
- src/api/repos.rs
- tests/api.rs
- static/js/pages/settings.js
- static/js/pages/reposettings.js
- scripts/ui-check.mjs
- README.md
