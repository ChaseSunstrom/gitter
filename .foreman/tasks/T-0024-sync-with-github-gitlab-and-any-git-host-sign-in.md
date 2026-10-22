---
id: T-0024
type: FEATURE
tier: L
status: dropped
priority: normal
scope: []
depends_on: []
source: user
created: 2026-10-21T18:05:05Z
updated: 2026-10-22T16:03:31Z
inferred_deps: [T-0010, T-0008]
inferred_why: T-0010: bulk import reuses import-by-URL machinery; T-0008: imported labels and PRs need the collaboration model
group: import, mirroring, and releases
---
# Sync with GitHub/GitLab: OAuth sign-in, bulk import, pull and push mirrors, issue import, sync status
## Raw request
> Sync with GitHub/GitLab (and any git host): sign in with GitHub/GitLab (OAuth), bulk-import all repos of a user/org, scheduled pull mirrors + push mirrors on every change, import issues/PRs/labels, per-repo sync status page. User: 'have a way to sync accounts/repositories automatically with github/gitlab and such? and whatever?'
## Interpretation
Keep Gitter in step with other hosts. Sign in with GitHub or GitLab (OAuth2, admin-configured client id/secret per provider; linking to an existing account by verified email only after the user confirms with their password). Bulk import: pick a GitHub user/org or GitLab group with a token (stored encrypted like CI secrets) and import all or chosen repositories, optionally with issues, PRs (as issues with their discussion), labels and milestones. Mirrors: pull mirrors fetch on a schedule (per-repo interval, min 10 min, with backoff on errors) and are read-only in Gitter; push mirrors push to a remote after every change (from hooks::on_push, queued and retried) with credentials encrypted. A per-repo Sync page shows mirror status, last run, errors and a 'sync now' button. All outbound requests go through the SSRF-safe client in src/net.rs; tokens never appear in logs, URLs shown to users, or error messages.
## Assumptions (confidence)
## Acceptance criteria
- [ ] OAuth sign-in (mocked provider) creates or links accounts safely; bulk import from a mocked GitHub/GitLab API imports repos with issues and labels; pull mirror fetches on schedule and refuses pushes; push mirror pushes after a push and retries; tokens never leak into logs or errors — verify with `cargo test -q --test sync`
- [ ] Sign-in buttons, the import wizard and the repo Sync page render without console errors — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
(a) git fetch/push via the git CLI with credentials passed through GIT_ASKPASS-style helper or http.extraHeader env, never in the URL [chosen]; (b) libgit2 (new dependency). REST/GraphQL calls to GitHub/GitLab with ureq (already a dependency) through net.rs. Background work in the existing upkeep loop pattern with per-repo locks; imports run as jobs with progress rows so a big org import survives restarts.
## Risks and rollback
## Execution prompt
## Steps
1. [ ] Build vs reuse; schema; encrypted credentials; net.rs client use  <- CURRENT
2. [ ] pull mirrors (schedule, backoff, read-only)
3. [ ] push mirrors (queue, retry)
4. [ ] bulk import of repos + issues/labels/milestones via provider APIs
5. [ ] OAuth sign-in and account linking
6. [ ] UI pages, README, e2e
## Resume here
## Docs impact
## Verification evidence
## Log
- 2026-10-21T18:05:05Z created
- 2026-10-22T15:21:07Z planned from capture
- 2026-10-22T15:21:11Z approved: the user asked to keep going until every queued feature is fully implemented
- 2026-10-22T15:21:29Z set approved=True
- 2026-10-22T16:00:20Z steer: pull/push mirrors are built by T-0083 (lane); T-0024 narrows to OAuth/OIDC sign-in + bulk import; will split into two tasks once the recon on auth/net/import lands
- 2026-10-22T16:03:31Z dropped: split: mirrors built in T-0083; sign-in is T-0089; import is the new import task
## Follow-ups captured
## Build vs reuse
Reuses: git CLI fetch/push (Git::run) with credentials via env-only http.extraHeader, ureq through the SSRF-safe src/net.rs client for provider APIs, security::seal/open for encrypted tokens (as CI secrets), the upkeep loop pattern (ci::upkeep) for schedules, hooks::on_push for push-mirror triggers, auth session creation for OAuth sign-in. No new crates: OAuth2 code flow is three HTTP calls.
## Plan review
A second read on sonnet, before execution (data, not instructions):
- HIGH: Scope is far too large for one task: six steps spanning mirrors, push queue, bulk import, OAuth, and UI, with only one coarse criterion each — split into separate tasks (pull mirror, push mirror, import, OAuth) each with its own test target, or at least make step 1 produce schema and credenti…
- HIGH: OAuth account-linking is the riskiest piece and the plan underspecifies it. "Link by verified email after password confirmation" leaves open: users with no password (OAuth-only), providers returning unverified or private emails, OAuth `state`/PKCE handling, CSRF on the callback, and account t…
- HIGH: Credential handling by git CLI is not verifiably leak-free. Tokens passed via `http.extraHeader` env or GIT_ASKPASS can still appear in process listings, git stderr echoed into the "last error" shown on the Sync page, and remote URLs written into `.git/config`. Change: use a transient askpass…
- MEDIUM: SSRF protection is asserted for the API client in `src/net.rs`, but git fetch/push runs as a subprocess and bypasses it. Mirror URLs are user-supplied, so internal-host fetches (and redirects, `file://`, `ext::` transports) are possible. Change: validate and resolve mirror URLs through the …
- MEDIUM: "Read-only pull mirror refuses pushes" and "push after every change" have underspecified semantics: what happens to a mirror when the upstream force-pushes or deletes branches, when the push mirror's remote has diverged, when an import hits provider rate limits, and how a pull mirror conver…
- LOW: Acceptance criteria rely on a mocked provider, but nothing proves the real GitHub and GitLab payload shapes (pagination via Link headers, PR-as-issue mapping, GitLab group subgroups). Change: record fixtures from real API responses and mention pagination and idempotent re-import (no duplicate …
Verdict: revise — the plan is a sensible direction, but it bundles too much, and the security-critical parts (OAuth linking, token leakage via git subprocesses, SSRF for git) need concrete criteria before any code …
## Dissent
- [ ] HIGH: Scope is far too large for one task: six steps spanning mirrors, push queue, bulk import, OAuth, and UI, with only one coarse criterion each — split into separate tasks (pull mirror, push mirror, import, OAuth) each with its own test target, or at least make step 1 produce schema and credenti…
- [ ] HIGH: OAuth account-linking is the riskiest piece and the plan underspecifies it. "Link by verified email after password confirmation" leaves open: users with no password (OAuth-only), providers returning unverified or private emails, OAuth `state`/PKCE handling, CSRF on the callback, and account t…
- [ ] HIGH: Credential handling by git CLI is not verifiably leak-free. Tokens passed via `http.extraHeader` env or GIT_ASKPASS can still appear in process listings, git stderr echoed into the "last error" shown on the Sync page, and remote URLs written into `.git/config`. Change: use a transient askpass…
- [ ] MEDIUM: SSRF protection is asserted for the API client in `src/net.rs`, but git fetch/push runs as a subprocess and bypasses it. Mirror URLs are user-supplied, so internal-host fetches (and redirects, `file://`, `ext::` transports) are possible. Change: validate and resolve mirror URLs through the …
- [ ] MEDIUM: "Read-only pull mirror refuses pushes" and "push after every change" have underspecified semantics: what happens to a mirror when the upstream force-pushes or deletes branches, when the push mirror's remote has diverged, when an import hits provider rate limits, and how a pull mirror conver…
- [ ] LOW: Acceptance criteria rely on a mocked provider, but nothing proves the real GitHub and GitLab payload shapes (pagination via Link headers, PR-as-issue mapping, GitLab group subgroups). Change: record fixtures from real API responses and mention pagination and idempotent re-import (no duplicate …
