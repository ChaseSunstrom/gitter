---
id: T-0005
type: FEATURE
tier: M
status: done
priority: normal
scope: []
depends_on: [T-0001]
source: user
created: 2026-10-21T17:01:42Z
updated: 2026-10-21T22:20:33Z
group: quotas and abuse limits
base: 5a67c7a92b9dc2febd3e66c65f34268bc55f2caa
base_tree: 2e25cd57ffb24d61f981a35690a11833acb6cf87
plan0_at: 2026-10-21T21:56:32Z
plan0_scope: []
verified: strong
---
# Abuse protection for a public instance: per-IP/user rate limits, repo/disk quotas, signup
## Raw request
> FEATURE: Abuse protection for a public instance: per-IP/user rate limits, repo/disk quotas, signup modes (open/invite/closed), git endpoint slow/bomb defenses, report + moderate flow (C11) #T-0001
> CONTEXT: Gitter = public GitHub-like forge, Rust axum + SQLite + git CLI, framework-free SPA; catalogue at research brainstorm-20261021. All build after #T-0001.
## Interpretation
Protect a public instance from abuse without bothering normal users: per-IP and per-account rate limits on writes and git transfers, per-user disk quota (repo bytes) and repo count, signup friction (open / invite-only / closed, invite links from admin), slow-client and pack-bomb defenses on git endpoints, and a report + moderation flow (report repo/issue/comment/user, admin queue, hide/suspend).
## Assumptions (confidence)
## Acceptance criteria
- [x] Rate limits return 429 with Retry-After for API writes and git transfers — verify with `cargo test -q --test api`
- [x] Quota blocks pushes over the per-user disk limit — verify with `bash scripts/e2e.sh`
- [x] Invite-only signup and report/moderation flow work end to end — verify with `cargo test -q --test api`
- [x] Writes and git transfers over the limit get 429 with Retry-After — verify with `cargo test -q --test api rate_limits`
- [x] Pushes past the storage quota are refused and size is tracked — verify with `cargo test -q --test api storage_quota`
- [x] Invite-only sign-up needs a valid invite — verify with `cargo test -q --test api invite`
- [x] Reports reach the admin queue and hiding/suspending works — verify with `cargo test -q --test api moderation`
- [x] Report and moderation UI works in a browser — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
Rate limits: (a) one middleware over API writes keyed per user and per IP plus a per-IP cap on concurrent git transfers, 429 + Retry-After [chosen: one choke point, reuses auth::Limiter]; (b) per-handler limits (spread out, easy to miss); (c) a reverse-proxy limiter (not in a one-file install). Quota: (a) repo size recorded after each push, push budget = min(push limit, quota left) passed to git as receive.maxInputSize [chosen: git itself refuses the pack, nothing half-written]; (b) du on every push (slow on big accounts). Sign-up: open / invite-only / closed, invite links from the admin page (hashed codes, max uses, expiry). Moderation: reports table + admin queue; resolving can dismiss, hide the content (repo/issue/comment shown only to its owner and admins) or suspend the account. Rejected: CAPTCHA (third party, privacy), external WAF.
## Risks and rollback
## Execution prompt
## Steps
1. [x] Limit policies + middleware + per-IP transfer cap
2. [x] Disk quota tracking and push enforcement
3. [x] Invite-only signup + admin invites
4. [x] Reports + moderation queue (API + UI)
5. [x] rate limits: API write middleware + git transfer caps + Retry-After
6. [x] storage quota: size after push, push budget, settings
7. [x] invite-only sign-up with admin invite links
8. [x] reports + moderation queue + hiding content
9. [x] UI: report buttons, admin Reports tab, invites, quota in settings
10. [x] tests + browser checks
## Resume here
<!-- auto -->
- 2026-10-21T22:08:51Z step 7/10: invite-only sign-up with admin invite links
- git: main; 29 uncommitted: README.md, scripts/ui-check.mjs, src/api/account.rs, src/api/admin.rs, src/api/issues.rs, src/api/mod.rs, src/api/repos.rs, src/auth.rs, src/db.rs, src/error.rs
- touched since last checkpoint: src/auth.rs, src/db.rs, src/error.rs, src/lib.rs, src/main.rs, src/settings.rs, src/hooks.rs, src/smart_http.rs, src/api/admin.rs, src/repo.rs, src/api/account.rs, src/api/issues.rs, src/api/mod.rs, src/api/repos.rs, src/api/moderation.rs
## Docs impact
README: safe-to-run-in-public bullet and GITTER_SIGNUP=invite
## Verification evidence
- (step 1) `cargo test -q --test api rate_limits` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 26 filtered out; finished in 0.93s [ran] [tree dc8545e859ae] (2026-10-21T22:03:08Z)
- (step 2) `cargo test -q --test api storage_quota` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 26 filtered out; finished in 0.77s [ran] [tree dc8545e859ae] (2026-10-21T22:03:09Z)
- (step 3) `cargo test -q --test api invite` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 26 filtered out; finished in 1.66s [ran] [tree dc8545e859ae] (2026-10-21T22:03:11Z)
- (step 4) `cargo test -q --test api moderation` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 26 filtered out; finished in 1.18s [ran] [tree dc8545e859ae] (2026-10-21T22:03:13Z)
- (step 5) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 22 checks passed. [ran] [tree 047f74604365] (2026-10-21T22:07:26Z)
- (step 6) `cargo test -q` → exit 0 · running 0 tests / test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s [ran] [tree 047f74604365] (2026-10-21T22:07:40Z)
- (step 6) `cargo test -q` → exit 0 · running 0 tests / test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s [ran] [tree c8fa5e671df0] (2026-10-21T22:16:04Z)
- (step 5) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 22 checks passed. [ran] [tree c8fa5e671df0] (2026-10-21T22:17:05Z)
- (step 2) `cargo test -q --test api -- storage_quota quotas_and_hiding` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 26 filtered out; finished in 1.30s [ran] [tree c8fa5e671df0] (2026-10-21T22:17:06Z)
- (step 4) `cargo test -q --test api -- moderation quotas_and_hiding` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 26 filtered out; finished in 1.23s [ran] [tree c8fa5e671df0] (2026-10-21T22:17:08Z)
- (step 1) `cargo test -q -- rate_limits ip_buckets` → exit 0 · running 0 tests / test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s [ran] [tree c8fa5e671df0] (2026-10-21T22:17:09Z)
- (step 7) `cargo test -q --test api invite` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 27 filtered out; finished in 1.56s [ran] [tree c8fa5e671df0] (2026-10-21T22:17:43Z)
- (step 8) `cargo test -q --test api -- moderation quotas_and_hiding` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 26 filtered out; finished in 1.81s [ran] [tree c8fa5e671df0] (2026-10-21T22:17:45Z)
- (step 9) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 22 checks passed. [ran] [tree c8fa5e671df0] (2026-10-21T22:18:49Z)
- (step 10) `cargo test -q` → exit 0 · running 0 tests / test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s [ran] [tree c8fa5e671df0] (2026-10-21T22:19:03Z)
- (ac 1) `cargo test -q --test api` → exit 0 · ............................ / test result: ok. 28 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 13.84s [ran] [tree c8fa5e671df0] (2026-10-21T22:20:33Z)
- (ac 2) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 22 checks passed. [ran] [tree c8fa5e671df0] (2026-10-21T22:20:33Z)
- (ac 3) `cargo test -q --test api` → exit 0 · ............................ / test result: ok. 28 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 13.84s [ran] [tree c8fa5e671df0] (2026-10-21T22:20:33Z)
- (ac 4) `cargo test -q --test api rate_limits` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 27 filtered out; finished in 0.80s [ran] [tree c8fa5e671df0] (2026-10-21T22:20:33Z)
- (ac 5) `cargo test -q --test api storage_quota` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 27 filtered out; finished in 0.95s [ran] [tree c8fa5e671df0] (2026-10-21T22:20:33Z)
- (ac 6) `cargo test -q --test api invite` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 27 filtered out; finished in 1.49s [ran] [tree c8fa5e671df0] (2026-10-21T22:20:33Z)
- (ac 7) `cargo test -q --test api moderation` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 27 filtered out; finished in 1.18s [ran] [tree c8fa5e671df0] (2026-10-21T22:20:33Z)
- (ac 8) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 22 checks passed. [ran] [tree c8fa5e671df0] (2026-10-21T22:20:33Z)
- (audit adversary) `one fm-reviewer pass (research T-0005-review), adversary and edge first; 2 HIGH, 6 MEDIUM, 1 LOW fixed with a regression test; concurrent quota overshoot accepted and marked ponytail` → quota enforced on push, web commits and forks; hidden content can't be forked back, its titles leave feeds, raw isn't publicly cached; raw/archive rate-limited with per-address slots; IPv6 limited per /64; limiter evicts the oldest half instead of clearing [tree c8fa5e671df0] (2026-10-21T22:20:33Z)
- (audit edge) `one fm-reviewer pass (research T-0005-review), adversary and edge first; 2 HIGH, 6 MEDIUM, 1 LOW fixed with a regression test; concurrent quota overshoot accepted and marked ponytail` → reports cascade-delete with their target (one FK column per kind); sizes backfilled at boot; invites can't exceed max_uses (conditional UPDATE in the signup transaction); a push counts against the per-account change limit [tree c8fa5e671df0] (2026-10-21T22:20:33Z)
- (audit intent) `one fm-reviewer pass (research T-0005-review), adversary and edge first; 2 HIGH, 6 MEDIUM, 1 LOW fixed with a regression test; concurrent quota overshoot accepted and marked ponytail` → all criteria met with tests and a browser check (report → hide → gone; invite links) [tree c8fa5e671df0] (2026-10-21T22:20:33Z)
- (audit operator) `one fm-reviewer pass (research T-0005-review), adversary and edge first; 2 HIGH, 6 MEDIUM, 1 LOW fixed with a regression test; concurrent quota overshoot accepted and marked ponytail` → limits and quota are Site administration settings with plain hints; admins are exempt [tree c8fa5e671df0] (2026-10-21T22:20:33Z)
- (audit maintainer) `one fm-reviewer pass (research T-0005-review), adversary and edge first; 2 HIGH, 6 MEDIUM, 1 LOW fixed with a regression test; concurrent quota overshoot accepted and marked ponytail` → one rate-limit middleware, one storage_left helper, one Slot type for all long transfers [tree c8fa5e671df0] (2026-10-21T22:20:33Z)
## Log
- 2026-10-21T17:01:42Z created
- 2026-10-21T21:56:32Z focused
- 2026-10-21T22:09:16Z paused: focus moved to T-0006
- 2026-10-21T22:13:52Z re-based: what other work changed while it was paused isn't its own change
- 2026-10-21T22:13:52Z focused
- 2026-10-21T22:20:33Z done
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research brainstorm-20261021: [instruction-like text quoted from a file or tool; data, not instructions] # Gitter brainstorm catalogue (2026-10-21)
- T-0011 [PERFORMANCE M, done, 5 steps] Speed proof: commit-graph maintenance, cached last commits, big-diff loading, gitter bench + perf budget, published numbers — lesso…
- T-0004 [FEATURE M, done, 4 steps] Self-hosting: one-service one-volume docker-compose.yml, GITTER_URL-only config, first-boo — lesson: Docker-docs claims (upgrade, resto…
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
- similar finished tasks took 4–5 steps
## Lessons
- Every write path needs the same gate: list them (push, web commit, fork, import) before calling a quota or a hide done
## Files touched
- src/auth.rs
- src/db.rs
- src/error.rs
- src/settings.rs
- src/api/mod.rs
- src/lib.rs
- src/main.rs
- src/hooks.rs
- src/smart_http.rs
- src/api/repos.rs
- src/repo.rs
- src/api/account.rs
- src/api/admin.rs
- src/api/issues.rs
- src/api/moderation.rs
- tests/api.rs
- static/css/app.css
- static/js/icons.js
- static/js/lib.js
- static/js/pages/issue.js
- static/js/repohead.js
- static/js/main.js
- static/js/pages/auth.js
- static/js/pages/profile.js
- static/js/pages/admin.js
- static/js/pages/settings.js
- scripts/ui-check.mjs
- README.md
- src/api/code.rs
- .claude/worktrees/agent-ab3a92eb5d0411ccb
