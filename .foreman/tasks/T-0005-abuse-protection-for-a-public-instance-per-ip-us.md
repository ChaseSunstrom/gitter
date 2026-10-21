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
updated: 2026-10-21T18:40:56Z
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
## Non-goals
## Approach (options → choice → why)
Reuse auth::Limiter with named policies in one table (limits.rs) applied by a middleware for API writes and in smart_http for transfers (per-IP concurrent transfer cap); disk quota computed after push (dir size cached in repos.size_bytes, refuse pushes over quota via receive.maxInputSize budget); invites table + signup mode 'invite'; reports table + admin queue page. Rejected: CAPTCHA (third-party, privacy), external WAF (against one-file simplicity).
## Risks and rollback
## Execution prompt
## Steps
1. [ ] Limit policies + middleware + per-IP transfer cap  <- CURRENT
2. [ ] Disk quota tracking and push enforcement
3. [ ] Invite-only signup + admin invites
4. [ ] Reports + moderation queue (API + UI)
## Resume here
## Docs impact
## Verification evidence
## Log
- 2026-10-21T17:01:42Z created
## Follow-ups captured
