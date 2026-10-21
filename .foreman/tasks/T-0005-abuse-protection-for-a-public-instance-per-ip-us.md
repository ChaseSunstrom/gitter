---
id: T-0005
type: FEATURE
tier: M
status: captured
priority: normal
scope: []
depends_on: [T-0001]
source: user
created: 2026-10-21T17:01:42Z
updated: 2026-10-21T21:50:34Z
group: quotas and abuse limits
---
# Abuse protection for a public instance: per-IP/user rate limits, repo/disk quotas, signup
## Raw request
> FEATURE: Abuse protection for a public instance: per-IP/user rate limits, repo/disk quotas, signup modes (open/invite/closed), git endpoint slow/bomb defenses, report + moderate flow (C11) #T-0001
> CONTEXT: Gitter = public GitHub-like forge, Rust axum + SQLite + git CLI, framework-free SPA; catalogue at research brainstorm-20261021. All build after #T-0001.
## Interpretation
Protect a public instance from abuse without bothering normal users: per-IP and per-account rate limits on writes and git transfers, per-user disk quota (repo bytes) and repo count, signup friction (open / invite-only / closed, invite links from admin), slow-client and pack-bomb defenses on git endpoints, and a report + moderation flow (report repo/issue/comment/user, admin queue, hide/suspend).
## Assumptions (confidence)
## Acceptance criteria
- [ ] Rate limits return 429 with Retry-After for API writes and git transfers — verify with `cargo test -q --test api`
- [ ] Quota blocks pushes over the per-user disk limit — verify with `bash scripts/e2e.sh`
- [ ] Invite-only signup and report/moderation flow work end to end — verify with `cargo test -q --test api`
- [ ] Writes and git transfers over the limit get 429 with Retry-After — verify with `cargo test -q --test api rate_limits`
- [ ] Pushes past the storage quota are refused and size is tracked — verify with `cargo test -q --test api storage_quota`
- [ ] Invite-only sign-up needs a valid invite — verify with `cargo test -q --test api invite`
- [ ] Reports reach the admin queue and hiding/suspending works — verify with `cargo test -q --test api moderation`
- [ ] Report and moderation UI works in a browser — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
Rate limits: (a) one middleware over API writes keyed per user and per IP plus a per-IP cap on concurrent git transfers, 429 + Retry-After [chosen: one choke point, reuses auth::Limiter]; (b) per-handler limits (spread out, easy to miss); (c) a reverse-proxy limiter (not in a one-file install). Quota: (a) repo size recorded after each push, push budget = min(push limit, quota left) passed to git as receive.maxInputSize [chosen: git itself refuses the pack, nothing half-written]; (b) du on every push (slow on big accounts). Sign-up: open / invite-only / closed, invite links from the admin page (hashed codes, max uses, expiry). Moderation: reports table + admin queue; resolving can dismiss, hide the content (repo/issue/comment shown only to its owner and admins) or suspend the account. Rejected: CAPTCHA (third party, privacy), external WAF.
## Risks and rollback
## Execution prompt
## Steps
1. [ ] Limit policies + middleware + per-IP transfer cap  <- CURRENT
2. [ ] Disk quota tracking and push enforcement
3. [ ] Invite-only signup + admin invites
4. [ ] Reports + moderation queue (API + UI)
5. [ ] rate limits: API write middleware + git transfer caps + Retry-After
6. [ ] storage quota: size after push, push budget, settings
7. [ ] invite-only sign-up with admin invite links
8. [ ] reports + moderation queue + hiding content
9. [ ] UI: report buttons, admin Reports tab, invites, quota in settings
10. [ ] tests + browser checks
## Resume here
## Docs impact
## Verification evidence
## Log
- 2026-10-21T17:01:42Z created
## Follow-ups captured
