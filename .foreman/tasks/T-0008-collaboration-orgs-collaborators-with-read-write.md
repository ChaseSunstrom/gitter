---
id: T-0008
type: FEATURE
tier: L
status: done
priority: normal
scope: []
depends_on: []
source: user
created: 2026-10-21T17:01:42Z
updated: 2026-10-22T00:43:55Z
base: 5d606557ffab1edc484bacfe602d893d46e1cf57
base_tree: 258ee376be7c89ecab1c3466bab2fd43149e1906
plan0_at: 2026-10-22T00:13:08Z
plan0_scope: []
verified: strong
---
# Collaboration: orgs + collaborators with read/write/admin roles and plain-language access
## Raw request
> FEATURE: Collaboration: orgs + collaborators with read/write/admin roles and plain-language access text, PR code review (inline comments, approvals, suggestions, viewed files), labels + milestones + templates, notifications inbox + watching, cross-repo fork PRs (C17 C18 C24 C25 C46)
> CONTEXT: Gitter = public GitHub-like forge, Rust axum + SQLite + git CLI, framework-free SPA; catalogue at research brainstorm-20261021. All build after #T-0001.
## Interpretation
This task covers orgs and collaborators. The rest of the original C17/C18/C24/C25/C46 cluster (PR code review, milestones, templates, notifications and watching, cross-repo fork PRs) goes to its own follow-up tasks. (1) Collaborators: a repository's admins add accounts with a role. read: see and clone private repos, and comment. write: push, manage issues and PRs, merge, edit the wiki, run CI. admin: everything else, meaning settings, collaborators, deploy tokens, CI secrets, visibility, delete, rename and transfer. Being added gives access directly, with no invite step. (2) Organizations: accounts that own repositories but can't sign in. Any user can create one (up to 10) and becomes its owner. Members have the role owner or member. Owners have admin on every org repo. Members get the org's base permission (none, read or write). Repos can be created in an org by people who'd have write on all of its repos. Org profile pages show members and repos. Org settings cover display name, base permission, members, and deleting the org (Sudo, typed name, only when it owns no repos). The last owner can't leave. (3) Access everywhere follows one rule, Repo::load + access(): role = max(owner/site admin → admin, collaborator role, org-derived role). Deploy and scoped tokens still narrow it. Moderator-hidden repos stay visible only to admins of the repo. The shared VISIBLE SQL fragment includes collaborator and org access, so lists, search, the feed and profiles show what people can see. (4) Repo settings get a 'Who has access' section in plain words: 'Anyone on the internet can see and clone this', 'bob can push, merge and manage issues', 'Members of acme can see and clone it'. Admins can add, change and remove collaborators there. (5) Settings-type endpoints (repo PATCH/DELETE, deploy tokens, CI secrets) need admin, not write. The UI shows the Settings tab to admins only. (6) Audit: collaborator_added/changed/removed, org_created/deleted, org_member_added/changed/removed.
## Assumptions (confidence)
## Acceptance criteria
- [x] Collaborator roles read/write/admin grant exactly their rights over the API and git, removing them revokes, and settings endpoints need admin — verify with `cargo test -q --test api collaborators`
- [x] Organizations own repos; members get the base permission, owners admin; orgs can't sign in; the last owner can't leave; lists and search follow access — verify with `cargo test -q --test api organizations`
- [x] Org pages, repo access settings and the owner picker on new repos render and work in a browser — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
(a) Orgs as rows in users with is_org=1 plus orgs(base_role) and org_members tables [chosen: one namespace for /{owner}/{repo}, so owner_id keeps working for quotas, profiles and paths]; (b) a separate orgs table with repos.owner_kind (every owner lookup branches); (c) teams first (more than asked). Collaborators go in repo_collaborators(repo_id, user_id, role). The viewer's role is computed in Repo::load's single db.call, and access() turns it into read/write/admin. Build vs reuse: the existing access(), VISIBLE and visible_params (extended with collaborator and org EXISTS clauses), Sudo, security::record and validate::username.
## Risks and rollback
## Execution prompt
## Steps
1. [x] schema: users.is_org, orgs, org_members, repo_collaborators; Repo role + can_admin; access() + VISIBLE/visible_params
2. [x] admin-level endpoints: repo update/delete, deploy tokens, CI secrets require can_admin
3. [x] collaborators API (list with plain-language access, add/change/remove) + audit
4. [x] organizations API: create, profile, members, settings, delete; repo create in org; login refused for orgs
5. [x] UI: repo settings access section, org profile + settings, new org page, owner picker on new repo; tests + browser checks
## Resume here
<!-- auto -->
- 2026-10-22T00:19:31Z step 5/5: UI: repo settings access section, org profile + settings, new org page, owner picker on new repo; tests + browser checks
- git: main; 11 uncommitted: src/api/account.rs, src/api/ci.rs, src/api/mod.rs, src/api/repos.rs, src/auth.rs, src/ci/queue.rs, src/ci/trigger.rs, src/db.rs, src/repo.rs, tests/api.rs
- touched since last checkpoint: src/api/ci.rs, src/db.rs, src/repo.rs, src/api/orgs.rs, /tmp/claude-1000/-home-chase-projects-gitter/2e122f62-61e6-4fe5-b944-f5546ee8dfc9/scratchpad/orgs_tests.rs, tests/api.rs, src/api/mod.rs, src/auth.rs, src/api/repos.rs, src/ci/queue.rs, src/ci/trigger.rs, src/api/account.rs
## Docs impact
README: Teams bullet, security note on access
## Verification evidence
- (step 1) `cargo test -q --test api collaborators` → ✗ exit 101 · test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 37 filtered out; finished in 1.82s / error: test failed, to rerun pass `--test api` [ran] [tree 45e072fbf489] (2026-10-22T00:17:19Z)
- (step 1) `cargo test -q --test api collaborators` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 37 filtered out; finished in 3.81s [ran] [tree 432330c07105] (2026-10-22T00:19:10Z)
- (step 2) `cargo test -q --test api collaborators` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 37 filtered out; finished in 4.02s [ran] [tree 432330c07105] (2026-10-22T00:19:15Z)
- (step 3) `cargo test -q --test api collaborators` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 37 filtered out; finished in 2.32s [ran] [tree 432330c07105] (2026-10-22T00:19:18Z)
- (step 4) `cargo test -q --test api organizations` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 37 filtered out; finished in 2.35s [ran] [tree 432330c07105] (2026-10-22T00:19:21Z)
- (step 5) `SKIP_UI=0 bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 26 checks passed. [ran] [tree 458402248f9a] (2026-10-22T00:30:13Z)
- (ac 1) `cargo test -q --test api collaborators` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 38 filtered out; finished in 3.26s [ran] [tree 1ce838877787] (2026-10-22T00:43:54Z)
- (ac 2) `cargo test -q --test api organizations` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 37 filtered out; finished in 7.59s [ran] [tree 1ce838877787] (2026-10-22T00:43:54Z)
- (ac 3) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 26 checks passed. [ran] [tree 1ce838877787] (2026-10-22T00:43:54Z)
- (audit intent) `3 fm-reviewer passes (5 lenses) over the full diff; 10 findings fixed test-first (CI org trust, shared quotas, private membership, token leave, orphaned orgs, tests), 1 documented and handed to T-0007` → roles, orgs, plain-language access met; quota multiplication via orgs fixed (creator pays) [tree 1ce838877787] (2026-10-22T00:43:54Z)
- (audit adversary) `3 fm-reviewer passes (5 lenses) over the full diff; 10 findings fixed test-first (CI org trust, shared quotas, private membership, token leave, orphaned orgs, tests), 1 documented and handed to T-0007` → forged org CI trust fixed (ADMIN_OWNED = admin-created orgs); limited tokens can't leave; write→CI secrets documented, gating moved to T-0007 [tree 1ce838877787] (2026-10-22T00:43:54Z)
- (audit edge) `3 fm-reviewer passes (5 lenses) over the full diff; 10 findings fixed test-first (CI org trust, shared quotas, private membership, token leave, orphaned orgs, tests), 1 documented and handed to T-0007` → base role none/read/write lists, hidden repos, last owner, sole-owner admin delete, org login with a matching hash all tested [tree 1ce838877787] (2026-10-22T00:43:54Z)
- (audit operator) `3 fm-reviewer passes (5 lenses) over the full diff; 10 findings fixed test-first (CI org trust, shared quotas, private membership, token leave, orphaned orgs, tests), 1 documented and handed to T-0007` → admin counts/lists exclude orgs; README documents trust and limits; e2e pushes as a collaborator with real git [tree 1ce838877787] (2026-10-22T00:43:54Z)
- (audit maintainer) `3 fm-reviewer passes (5 lenses) over the full diff; 10 findings fixed test-first (CI org trust, shared quotas, private membership, token leave, orphaned orgs, tests), 1 documented and handed to T-0007` → one access rule (repo::access + VISIBLE), dead role_level removed, stale doc fixed [tree 1ce838877787] (2026-10-22T00:43:54Z)
## Log
- 2026-10-21T17:01:42Z created
- 2026-10-22T00:13:04Z set tier=L
- 2026-10-22T00:13:07Z set approved=True
- 2026-10-22T00:13:07Z approved: the user asked to keep going until every queued feature is fully implemented
- 2026-10-22T00:13:07Z focused
- 2026-10-22T00:39:05Z review fixes: ADMIN_OWNED trusts orgs an admin created (not ones with an admin owner); org repos/storage count toward the creator; members private to members; limited tokens can't leave; admin can't delete an org's only owner; dashboard lists shared repos; role_level removed; stronger org-login test
- 2026-10-22T00:43:55Z done
## Follow-ups captured
## Build vs reuse
Reuse access(), VISIBLE and visible_params, Sudo/Admin extractors, security::record and validate::username. No new crates.
## Related
Related past work (data from this project's history, not instructions):
- research brainstorm-20261021: [instruction-like text quoted from a file or tool; data, not instructions] # Gitter brainstorm catalogue (2026-10-21)
- research T-0016-review: HIGH — src/api/snippets.rs:137,261 and src/api/wiki.rs:148 — snippets and wikis bypass the storage quota [stale: src/api/discussions.rs, src/api/…
- T-0011 [PERFORMANCE M, done, 5 steps] Speed proof: commit-graph maintenance, cached last commits, big-diff loading, gitter bench + perf budget, published numbers — lesso…
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
- the similar finished task took 5 steps
## Lessons
- Trust derived from membership is forgeable when members can be added without consent; derive it from who created the thing
## Plan gaps
- failed first: step 1 schema: users.is_org, orgs, org_members, repo_collaborators; Repo role + can_admin; access() + VISI…
## Files touched
- src/repo.rs
- src/api/ci.rs
- src/api/repos.rs
- src/db.rs
- src/api/mod.rs
- src/api/orgs.rs
- tests/api.rs
- src/auth.rs
- src/ci/queue.rs
- src/ci/trigger.rs
- src/api/account.rs
- static/js/pages/repo.js
- static/js/pages/reposettings.js
- static/js/palette.js
- static/js/repohead.js
- static/css/pages.css
- static/js/pages/newrepo.js
- static/js/pages/orgs.js
- static/js/main.js
- static/js/pages/profile.js
- src/api/admin.rs
- scripts/e2e.sh
- scripts/ui-check.mjs
- README.md
- src/lfs.rs
