---
id: T-0090
type: FEATURE
tier: L
status: planned
priority: normal
scope: []
depends_on: [T-0084]
source: user
created: 2026-10-22T16:03:31Z
updated: 2026-10-22T16:04:34Z
---
# Bulk import from GitHub and GitLab: repositories with labels, milestones, issues, pull requests, comments and releases
## Raw request
> Bulk import from GitHub and GitLab: repositories with labels, milestones, issues, pull requests, comments and releases
## Interpretation
A signed-in user picks GitHub (or a GitLab instance), pastes a personal access token (sealed, deleted when the job ends), chooses an owner (themselves or an org they admin here) and a source user/org/group, sees its repositories and imports all or some. Each repository is cloned through the mirrors lane's pinned, credential-in-env git path, then its labels, milestones, issues and pull requests (keeping their numbers, states and timestamps), comments and releases (no assets) are imported. Remote authors who aren't local users are credited in the text ('originally by @login on GitHub') under the importing user. A job runs in the background, survives restarts, honours rate limits and shows progress per repository; re-running skips what already exists.
## Assumptions (confidence)
## Acceptance criteria
- [ ] Against a mock GitHub and GitLab API: repos import with labels, milestones, issues, PRs (numbers kept), comments and releases; pagination and rate-limit waits honoured; a restart resumes; re-import doesn't duplicate; the token is gone after the job and never appears in errors or responses; private addresses refused — verify with `cargo test -q --test import`
- [ ] Import page (provider, token, source, repo picker, progress) renders without console errors — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
(a) REST over net::get_json with Link-header pagination, per-repo stages with a stored cursor, leased like mirrors [chosen: reuses mirrors' worker and git path]; (b) GraphQL (fewer calls but two different schemas and harder mocking); (c) ghost users per remote author (pollutes users table; text attribution is enough for now). Personal access token instead of an OAuth import grant: no provider app needed; the OAuth grant can come later with T-0089.
## Risks and rollback
## Execution prompt
## Steps
1. [ ] schema (import_jobs, import_items), job API with sealed token, provider listing (GitHub, GitLab) with pagination and rate limits  <- CURRENT
2. [ ] worker: clone through mirrors::git, then labels, milestones, issues/PRs, comments, releases with cursors; resume and idempotence
3. [ ] UI import page with progress, README, e2e
## Resume here
## Docs impact
## Verification evidence
## Log
- 2026-10-22T16:03:31Z created
- 2026-10-22T16:03:39Z approved: the user asked to keep going until every queued feature is fully implemented
- 2026-10-22T16:03:41Z set approved=True
- 2026-10-22T16:04:34Z plan decisions (from second read): every request URL — the GitLab instance, clone URLs and each Link 'next' — must be on the job's original host (same scheme+host+port) and pass net::check_url + pin each call, redirects off; clone uses the source host's URL rebuilt from owner/name, not the API's clone_url. Token sealed in import_jobs and wiped (set NULL) on every terminal state — done, failed, cancelled — and by an hourly sweep for jobs idle over 24 h; a Cancel button/endpoint exists; responses never include it and errors pass through a redactor. Mapping table import_map(job_id, kind, remote_id, local_id) UNIQUE(repo_id, kind, remote_id) keeps re-import idempotent; GitHub numbers (shared space) are kept; GitLab issues keep iid, merge requests get numbers after the highest issue iid (noted in their text), and only into a freshly created empty repository so numbers never collide. PRs import as records (title, body, state, merged/closed times, branches as text); refs/pull/*/head for GitHub and refs/merge-requests/*/head for GitLab are fetched into refs/pull/N/head where available; head_sha/base_sha set when the commits exist. Rate limits: GitHub x-ratelimit-* and secondary 403/429 with retry-after; GitLab RateLimit-* and 429; the item is rescheduled (not a sleeping worker). Limits: one running job per user, at most 200 repositories per job, storage_left checked before each clone and the fetch size watchdog from the mirrors fix. Remote text is stored as markdown with the login in inline code (no @-mention, no notification) and timestamps clamped to now. Owner must be the user or an org they own/admin — tested.
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
- [ ] HIGH: SSRF and credential-leak surface is under-specified. "Private addresses refused" is only an acceptance line, but GitLab takes a user-supplied instance URL, and the clone URL and every pagination `Link` header come from the remote. A malicious or compromised host can redirect to internal addre…
- [ ] HIGH: Token lifecycle contradicts "survives restarts" and "resume." The token must persist (sealed) until the job ends, but then must be deleted on every terminal path: success, failure, cancel, and a job abandoned after repeated crashes. The plan has no expiry/TTL, no cancel step, and no check tha…
- [ ] HIGH: Idempotence and number preservation are only asserted, with no mechanism. GitHub issues and PRs share one number space, GitLab issues and MRs have separate ones (iid), and local numbering may already have rows or a counter. Re-import skipping "what exists" needs a stable remote-ID-to-local ma…
- [ ] MEDIUM: PR import scope is unclear. Importing PRs as records without their branches fails when the head is a deleted fork branch. Closed or merged PRs have no live refs. GitHub's `refs/pull/*` aren't cloned by default, and the issues endpoint also returns PRs. — State the behavior: PRs are imported…
- [ ] MEDIUM: Rate-limit and abuse handling is thin. It covers GitHub's `X-RateLimit-*` but not secondary limits (`Retry-After`, 403/429) or GitLab's `RateLimit-*`/429 headers. Nothing bounds per-user concurrent jobs, repo count, or total storage, so a user can fill disk or hold workers. Waiting on a lea…
- [ ] LOW: Attribution text and timestamps from remote content are untrusted. "originally by @login" must be escaped and must not mention or notify local users who happen to share the login. Authorization on owner choice ("org they admin") is also a test gap. — Render the login as plain text without link…
