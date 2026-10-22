---
id: T-0090
type: FEATURE
tier: L
status: done
priority: normal
scope: []
depends_on: [T-0084]
source: user
created: 2026-10-22T16:03:31Z
updated: 2026-10-22T21:08:42Z
base: 9e408687d37bd18c30c5896a7dc5eac98b32fb31
base_tree: 638a50040442be6a040723ef0a7c11354794169c
plan0_at: 2026-10-22T20:19:41Z
plan0_scope: []
verified: strong
---
# Bulk import from GitHub and GitLab: repositories with labels, milestones, issues, pull requests, comments and releases
## Raw request
> Bulk import from GitHub and GitLab: repositories with labels, milestones, issues, pull requests, comments and releases
## Interpretation
A signed-in user picks GitHub (or a GitLab instance), pastes a personal access token (sealed, deleted when the job ends), chooses an owner (themselves or an org they admin here) and a source user/org/group, sees its repositories and imports all or some. Each repository is cloned through the mirrors lane's pinned, credential-in-env git path, then its labels, milestones, issues and pull requests (keeping their numbers, states and timestamps), comments and releases (no assets) are imported. Remote authors who aren't local users are credited in the text ('originally by @login on GitHub') under the importing user. A job runs in the background, survives restarts, honours rate limits and shows progress per repository; re-running skips what already exists.
## Assumptions (confidence)
## Acceptance criteria
- [x] Against a mock GitHub and GitLab API: repos import with labels, milestones, issues, PRs (numbers kept), comments and releases; pagination and rate-limit waits honoured; a restart resumes; re-import doesn't duplicate; the token is gone after the job and never appears in errors or responses; private addresses refused — verify with `cargo test -q --test import`
- [x] Import page (provider, token, source, repo picker, progress) renders without console errors — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
(a) REST over net::get_json with Link-header pagination, per-repo stages with a stored cursor, leased like mirrors [chosen: reuses mirrors' worker and git path]; (b) GraphQL (fewer calls but two different schemas and harder mocking); (c) ghost users per remote author (pollutes users table; text attribution is enough for now). Personal access token instead of an OAuth import grant: no provider app needed; the OAuth grant can come later with T-0089.
## Risks and rollback
Rollback: git revert the commit; migration v30 only adds tables (old code ignores them), and imported repositories stay ordinary repositories. Risks: one worker serves all imports (a slow clone delays others; noted in a ponytail comment); a provider's text is stored as markdown and rendered through the usual sanitizer.
## Execution prompt
## Steps
1. [x] schema (import_jobs, import_items), job API with sealed token, provider listing (GitHub, GitLab) with pagination and rate limits
2. [x] worker: clone through mirrors::git, then labels, milestones, issues/PRs, comments, releases with cursors; resume and idempotence
3. [x] UI import page with progress, README, e2e
## Resume here
## Docs impact
README: Import from GitHub or GitLab; token usernames for git over HTTPS; trash window is a site setting
## Verification evidence
- (step 1) `cargo test -q --test import` → exit 0 · ..... / test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.74s [ran] [tree cf8da3a3df26] (2026-10-22T20:35:15Z)
- (step 2) `cargo test -q --test import` → exit 0 · ..... / test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.73s [ran] [tree cf8da3a3df26] (2026-10-22T20:35:17Z)
- (ac 1) `cargo test -q --test import` → exit 0 · ..... / test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.74s [ran] [tree cf8da3a3df26] (2026-10-22T20:35:19Z)
- (ac 1) `cargo test -q --test import 2>&1 | grep -E "^error|panicked|test result" -A4 | head -40` → ✗ exit 1 · test result: FAILED. 6 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.25s / error: test failed, to rerun pass `--test import` [ran] [tree df0d92df0a4a] (2026-10-22T20:45:31Z)
- (step 1) `cargo test -q --test import` → exit 0 · ........ / test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.79s [ran] [tree 39b4bdf85936] (2026-10-22T20:54:03Z)
- (step 2) `cargo test -q --test import` → exit 0 · ........ / test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.04s [ran] [tree 39b4bdf85936] (2026-10-22T20:54:11Z)
- (ac 1) `cargo test -q --test import` → exit 0 · ........ / test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.05s [ran] [tree 39b4bdf85936] (2026-10-22T20:54:19Z)
- (audit intent) `fm-reviewer review-2 (intent/operator) + re-check` → met after fixes: restart releases leases, transient errors retry, 200-cap shown, re-run fills in data and leaves code alone (clones if empty), taken numbers skipped and counted [tree 39b4bdf85936] (2026-10-22T20:56:50Z)
- (audit adversary) `fm-reviewer review-1 (adversary) + automated security review + re-check` → fixed: page loops/caps and per-job turns, re-run needs repo admin, start rate-limited, unique running-job index, always-rate-limited jobs end after a week; token redaction and same-host pinning verified [tree 39b4bdf85936] (2026-10-22T20:56:51Z)
- (audit edge) `fm-reviewer review-1/re-check (edge)` → fixed: GitLab comments budget per issue, never-cloned re-run copies, comments via import_map, duplicates collapsed, github.com case-insensitive, cut-off answers retried, missing PR commits not recorded [tree 39b4bdf85936] (2026-10-22T20:56:51Z)
- (audit operator) `fm-reviewer review-2 (operator)` → migration v30 additive; finished jobs pruned after 30 days; oauth2 token username documented; 'import' name reservation noted on T-0075 for the doctor check [tree 39b4bdf85936] (2026-10-22T20:56:52Z)
- (audit maintainer) `fm-reviewer review-3 (maintainer)` → tests added for sweep, taken name, cancel by others, unreadable token, org re-import; cat-file batch-check; create/create_empty duplication kept (two call sites, six lines) [tree 39b4bdf85936] (2026-10-22T20:56:52Z)
- (ac 2) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 44 checks passed. [ran] [tree 39b4bdf85936] (2026-10-22T21:00:08Z)
- (step 3) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 44 checks passed. [ran] [tree 39b4bdf85936] (2026-10-22T21:02:58Z)
- (ac 1) `cargo test -q --test import` → exit 0 · ........ / test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.48s [ran] [tree 39b4bdf85936] (2026-10-22T21:07:47Z)
- (ac 2) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 44 checks passed. [ran] [tree 39b4bdf85936] (2026-10-22T21:07:47Z)
- (audit intent) `three fm-reviewer lenses on the full diff, an automated security review, then a fresh adversary/edge re-check of the fixes; 17 findings fixed test-first and each fix mutation-checked; 8 import tests against mock GitHub/GitLab APIs with a real Gitter git source; e2e 44 checks incl. the import page` → criteria met; re-run fills data, leaves code alone [tree 39b4bdf85936] (2026-10-22T21:07:47Z)
- (audit adversary) `three fm-reviewer lenses on the full diff, an automated security review, then a fresh adversary/edge re-check of the fixes; 17 findings fixed test-first and each fix mutation-checked; 8 import tests against mock GitHub/GitLab APIs with a real Gitter git source; e2e 44 checks incl. the import page` → same-host pinned requests, page loops capped, per-job turns, admin needed to re-import, token sealed and wiped [tree 39b4bdf85936] (2026-10-22T21:07:47Z)
- (audit edge) `three fm-reviewer lenses on the full diff, an automated security review, then a fresh adversary/edge re-check of the fixes; 17 findings fixed test-first and each fix mutation-checked; 8 import tests against mock GitHub/GitLab APIs with a real Gitter git source; e2e 44 checks incl. the import page` → GitLab comment budget per issue, retries for transient errors, taken numbers skipped and counted [tree 39b4bdf85936] (2026-10-22T21:07:47Z)
- (audit intent) `three fm-reviewer lenses on the full diff, an automated security review, then a fresh adversary/edge re-check of the fixes; 17 findings fixed test-first and each fix mutation-checked; 8 import tests against mock GitHub/GitLab APIs with a real Gitter git source; e2e 44 checks incl. the import page` → criteria met; re-run fills data, leaves code alone [tree 407dfc4b2ffb] (2026-10-22T21:08:31Z)
- (audit adversary) `three fm-reviewer lenses on the full diff, an automated security review, then a fresh adversary/edge re-check of the fixes; 17 findings fixed test-first and each fix mutation-checked; 8 import tests against mock GitHub/GitLab APIs with a real Gitter git source; e2e 44 checks incl. the import page` → same-host pinned requests, page loops capped, per-job turns, admin needed to re-import, token sealed and wiped [tree 407dfc4b2ffb] (2026-10-22T21:08:31Z)
- (audit edge) `three fm-reviewer lenses on the full diff, an automated security review, then a fresh adversary/edge re-check of the fixes; 17 findings fixed test-first and each fix mutation-checked; 8 import tests against mock GitHub/GitLab APIs with a real Gitter git source; e2e 44 checks incl. the import page` → GitLab comment budget per issue, retries for transient errors, taken numbers skipped and counted [tree 407dfc4b2ffb] (2026-10-22T21:08:31Z)
- (audit operator) `fm-reviewer review-2 (operator); last change is a test-only pragma comment` → migration v30 additive; finished jobs pruned after 30 days; oauth2 token username documented; 'import' reservation noted on T-0075 [tree 407dfc4b2ffb] (2026-10-22T21:08:40Z)
- (audit maintainer) `fm-reviewer review-3 (maintainer); last change is a test-only pragma comment` → tests cover sweep, taken name, cancel by others, unreadable token, org re-import; one cat-file batch-check [tree 407dfc4b2ffb] (2026-10-22T21:08:40Z)
- (audit intent) `three fm-reviewer lenses on the full diff, an automated security review, then a fresh adversary/edge re-check of the fixes; 17 findings fixed test-first and each fix mutation-checked; 8 import tests against mock GitHub/GitLab APIs with a real Gitter git source; e2e 44 checks incl. the import page` → criteria met; re-run fills data, leaves code alone [tree 407dfc4b2ffb] (2026-10-22T21:08:41Z)
- (audit adversary) `three fm-reviewer lenses on the full diff, an automated security review, then a fresh adversary/edge re-check of the fixes; 17 findings fixed test-first and each fix mutation-checked; 8 import tests against mock GitHub/GitLab APIs with a real Gitter git source; e2e 44 checks incl. the import page` → same-host pinned requests, page loops capped, per-job turns, admin needed to re-import, token sealed and wiped [tree 407dfc4b2ffb] (2026-10-22T21:08:41Z)
- (audit edge) `three fm-reviewer lenses on the full diff, an automated security review, then a fresh adversary/edge re-check of the fixes; 17 findings fixed test-first and each fix mutation-checked; 8 import tests against mock GitHub/GitLab APIs with a real Gitter git source; e2e 44 checks incl. the import page` → GitLab comment budget per issue, retries for transient errors, taken numbers skipped and counted [tree 407dfc4b2ffb] (2026-10-22T21:08:41Z)
## Log
- 2026-10-22T16:03:31Z created
- 2026-10-22T16:03:39Z approved: the user asked to keep going until every queued feature is fully implemented
- 2026-10-22T16:03:41Z set approved=True
- 2026-10-22T16:04:34Z plan decisions (from second read): every request URL — the GitLab instance, clone URLs and each Link 'next' — must be on the job's original host (same scheme+host+port) and pass net::check_url + pin each call, redirects off; clone uses the source host's URL rebuilt from owner/name, not the API's clone_url. Token sealed in import_jobs and wiped (set NULL) on every terminal state — done, failed, cancelled — and by an hourly sweep for jobs idle over 24 h; a Cancel button/endpoint exists; responses never include it and errors pass through a redactor. Mapping table import_map(job_id, kind, remote_id, local_id) UNIQUE(repo_id, kind, remote_id) keeps re-import idempotent; GitHub numbers (shared space) are kept; GitLab issues keep iid, merge requests get numbers after the highest issue iid (noted in their text), and only into a freshly created empty repository so numbers never collide. PRs import as records (title, body, state, merged/closed times, branches as text); refs/pull/*/head for GitHub and refs/merge-requests/*/head for GitLab are fetched into refs/pull/N/head where available; head_sha/base_sha set when the commits exist. Rate limits: GitHub x-ratelimit-* and secondary 403/429 with retry-after; GitLab RateLimit-* and 429; the item is rescheduled (not a sleeping worker). Limits: one running job per user, at most 200 repositories per job, storage_left checked before each clone and the fetch size watchdog from the mirrors fix. Remote text is stored as markdown with the login in inline code (no @-mention, no notification) and timestamps clamped to now. Owner must be the user or an org they own/admin — tested.
- 2026-10-22T20:19:41Z focused
- 2026-10-22T20:38:56Z scope: net.rs (response headers), mirrors.rs (pull takes extra refspecs, clean pub), repos.rs (owner_for/create_empty factored), auth.rs (accept GitLab's oauth2 username for tokens), validate.rs + auth.js (reserve 'import'), main.js route, newrepo.js link
- 2026-10-22T21:08:41Z done
- 2026-10-22T21:08:42Z committed 802b8e6 (29 files)
## Follow-ups captured
## Build vs reuse
Reuses mirrors::git (pinned, credential via GIT_CONFIG_* env, protocol.allow=never) and its leased worker/backoff pattern from T-0083, hooks::after_import, security::seal for the token, net::get_json from T-0089 (or added here first), issues/pulls/comments/labels/milestones/releases tables as they are. No new crate.
## Plan review
A second read on sonnet, before execution (data, not instructions):
- HIGH: SSRF and credential-leak surface is under-specified. "Private addresses refused" is only an acceptance line, but GitLab takes a user-supplied instance URL, and the clone URL and every pagination `Link` header come from the remote. A malicious or compromised host can redirect to internal addre…
- HIGH: Token lifecycle contradicts "survives restarts" and "resume." The token must persist (sealed) until the job ends, but then must be deleted on every terminal path: success, failure, cancel, and a job abandoned after repeated crashes. The plan has no expiry/TTL, no cancel step, and no check tha…
- HIGH: Idempotence and number preservation are only asserted, with no mechanism. GitHub issues and PRs share one number space, GitLab issues and MRs have separate ones (iid), and local numbering may already have rows or a counter. Re-import skipping "what exists" needs a stable remote-ID-to-local ma…
- MEDIUM: PR import scope is unclear. Importing PRs as records without their branches fails when the head is a deleted fork branch. Closed or merged PRs have no live refs. GitHub's `refs/pull/*` aren't cloned by default, and the issues endpoint also returns PRs. — State the behavior: PRs are imported…
- MEDIUM: Rate-limit and abuse handling is thin. It covers GitHub's `X-RateLimit-*` but not secondary limits (`Retry-After`, 403/429) or GitLab's `RateLimit-*`/429 headers. Nothing bounds per-user concurrent jobs, repo count, or total storage, so a user can fill disk or hold workers. Waiting on a lea…
- LOW: Attribution text and timestamps from remote content are untrusted. "originally by @login" must be escaped and must not mention or notify local users who happen to share the login. Authorization on owner choice ("org they admin") is also a test gap. — Render the login as plain text without link…
Verdict: revise — the design is reasonable, but SSRF, token lifecycle, and the idempotence/number mapping need concrete mechanisms and tests before step 1 starts.
## Dissent
- [x] HIGH: SSRF and credential-leak surface is under-specified. "Private addresses refused" is only an acceptance line, but GitLab takes a user-supplied instance URL, and the clone URL and every pagination `Link` header come from the remote. A malicious or compromised host can redirect to internal addre… — answered: addressed in the plan decisions and covered by tests/import.rs (same-host + pinned requests, sealed token wiped on every end and by the sweep, import_map idempotence and kept numbers, PRs as records …
- [x] HIGH: Token lifecycle contradicts "survives restarts" and "resume." The token must persist (sealed) until the job ends, but then must be deleted on every terminal path: success, failure, cancel, and a job abandoned after repeated crashes. The plan has no expiry/TTL, no cancel step, and no check tha… — answered: addressed in the plan decisions and covered by tests/import.rs (same-host + pinned requests, sealed token wiped on every end and by the sweep, import_map idempotence and kept numbers, PRs as records …
- [x] HIGH: Idempotence and number preservation are only asserted, with no mechanism. GitHub issues and PRs share one number space, GitLab issues and MRs have separate ones (iid), and local numbering may already have rows or a counter. Re-import skipping "what exists" needs a stable remote-ID-to-local ma… — answered: addressed in the plan decisions and covered by tests/import.rs (same-host + pinned requests, sealed token wiped on every end and by the sweep, import_map idempotence and kept numbers, PRs as records …
- [x] MEDIUM: PR import scope is unclear. Importing PRs as records without their branches fails when the head is a deleted fork branch. Closed or merged PRs have no live refs. GitHub's `refs/pull/*` aren't cloned by default, and the issues endpoint also returns PRs. — State the behavior: PRs are imported… — answered: addressed in the plan decisions and covered by tests/import.rs (same-host + pinned requests, sealed token wiped on every end and by the sweep, import_map idempotence and kept numbers, PRs as records …
- [x] MEDIUM: Rate-limit and abuse handling is thin. It covers GitHub's `X-RateLimit-*` but not secondary limits (`Retry-After`, 403/429) or GitLab's `RateLimit-*`/429 headers. Nothing bounds per-user concurrent jobs, repo count, or total storage, so a user can fill disk or hold workers. Waiting on a lea… — answered: addressed in the plan decisions and covered by tests/import.rs (same-host + pinned requests, sealed token wiped on every end and by the sweep, import_map idempotence and kept numbers, PRs as records …
- [x] LOW: Attribution text and timestamps from remote content are untrusted. "originally by @login" must be escaped and must not mention or notify local users who happen to share the login. Authorization on owner choice ("org they admin") is also a test gap. — Render the login as plain text without link… — answered: addressed in the plan decisions and covered by tests/import.rs (same-host + pinned requests, sealed token wiped on every end and by the sweep, import_map idempotence and kept numbers, PRs as records …
## Related
Related past work (data from this project's history, not instructions):
- research T-0024-recon: 1. **Answer [stale: src/api/account.rs, src/api/issues.rs, src/db.rs changed since it was written]
- T-0024 [FEATURE L, dropped, 6 steps] Sync with GitHub/GitLab: OAuth sign-in, bulk import, pull and push mirrors, issue import, sync status
- research brainstorm-20261021: [instruction-like text quoted from a file or tool; data, not instructions] # Gitter brainstorm catalogue (2026-10-21)
- research T-0062-review: Verdict — changes needed.** The logic is sound and I found no HIGH issues. The UI copy is still stale in seven places, and lowering `trash_days` …
## Lessons
- A resume test must leave the job able to resume (token restored): with the token wiped, the 'no duplicates' check passed without re-reading anything. Assert the pages were actually fetched again.
## Files touched
- tests/import.rs
- src/auth.rs
- src/net.rs
- src/api/repos.rs
- src/mirrors.rs
- src/imports.rs
- src/db.rs
- src/api/mod.rs
- src/lib.rs
- src/main.rs
- static/js/pages/import.js
- src/validate.rs
- static/js/main.js
- static/js/pages/auth.js
- static/js/pages/newrepo.js
- scripts/ui-check.mjs
- README.md
