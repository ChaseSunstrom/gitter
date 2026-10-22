---
id: T-0083
type: FEATURE
tier: M
status: done
priority: normal
scope: []
depends_on: []
source: followup
created: 2026-10-22T15:21:29Z
updated: 2026-10-22T17:07:15Z
base: fd653bc9e8d0d76b1e81784c3e5a1e0e3a0bfe1f
base_tree: 9e22ea0c6380fd41e858e828ab84cb65fde16228
plan0_at: 2026-10-22T15:22:07Z
plan0_scope: []
verified: strong
---
# Repository mirrors: scheduled pull mirrors and push mirrors with a Sync page (split from T-0024)
## Raw request
> Repository mirrors: scheduled pull mirrors and push mirrors with a Sync page (split from T-0024)
## Interpretation
Pull mirrors: a repository created as (or turned into) a mirror of a remote URL fetches all branches and tags on a schedule (per-repo interval, at least 10 minutes, backoff on errors up to a day), is read-only in Gitter (pushes, web edits and merges refused with a clear message), and has 'sync now'. Push mirrors: any repository can push to one or more remote URLs after every change (triggered from hooks::on_push, queued, retried with backoff, coalesced so a burst of pushes is one mirror push). Credentials (username + token/password) are stored encrypted with security::seal like CI secrets and passed to git only through the environment (never in the URL, argv, logs or error messages; remote URLs with embedded credentials are refused). Remote URLs go through the SSRF rules in src/net.rs (no private addresses unless the admin allows). A per-repo Settings → Mirrors page shows each mirror's status, last success, last error and a 'sync now' button; admins see all failing mirrors.
## Assumptions (confidence)
## Acceptance criteria
- [x] Pull mirror fetches new commits on schedule and on 'sync now', refuses pushes/web edits/merges, backs off on errors; push mirror pushes after a push and retries; credentials never appear in logs, errors, argv or API output; private-address remotes refused — verify with `cargo test -q --test mirrors`
- [x] Settings → Mirrors page and the mirror badge on repo pages render without console errors — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
(a) git CLI fetch/push with -c http.extraHeader from env (GIT_CONFIG_COUNT/KEY/VALUE) so the secret never hits argv [chosen]; (b) a credential helper process (more moving parts). Schedules in the existing upkeep loop with per-repo locks; mirror pushes as a small queue table worked by a background task.
## Risks and rollback
## Execution prompt
## Steps
1. [x] schema, encrypted credentials, URL validation through net.rs rules
2. [x] pull mirror fetch + schedule + read-only enforcement
3. [x] push mirror queue + retry
4. [x] Mirrors settings page, badge, README
5. [x] tests with local bare remotes, e2e
## Resume here
## Docs impact
README: Mirrors section, updated for the review fixes
## Verification evidence
- (step 1) `CARGO_INCREMENTAL=0 cargo test -q --test mirrors` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.39s [ran] [tree 645347f8bbea] (2026-10-22T15:34:03Z)
- (step 2) `CARGO_INCREMENTAL=0 cargo test -q --test mirrors` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.19s [ran] [tree 645347f8bbea] (2026-10-22T15:34:10Z)
- (step 3) `CARGO_INCREMENTAL=0 cargo test -q --test mirrors` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.24s [ran] [tree 645347f8bbea] (2026-10-22T15:34:14Z)
- (step 4) `CARGO_INCREMENTAL=0 bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 43 checks passed. [ran] [tree f652da1701db] (2026-10-22T15:50:32Z)
- (step 5) `CARGO_INCREMENTAL=0 cargo test -q --test mirrors` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.31s [ran] [tree f652da1701db] (2026-10-22T15:50:37Z)
- (ac 1) `cargo test -q --test mirrors` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.98s [ran] [tree ec6b4534d4e5] (2026-10-22T16:58:30Z)
- (ac 2) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 43 checks passed. [ran] [tree ec6b4534d4e5] (2026-10-22T17:01:35Z)
- (step 4) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 43 checks passed. [ran] [tree ec6b4534d4e5] (2026-10-22T17:04:18Z)
- (ac 1) `cargo test -q --test mirrors` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.95s [ran] [tree ec6b4534d4e5] (2026-10-22T17:07:15Z)
- (ac 2) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 43 checks passed. [ran] [tree ec6b4534d4e5] (2026-10-22T17:07:15Z)
- (audit intent) `fm-reviewer five-lens review of the lane diff (research T-0083-review): no SSRF bypass or credential leak; 6 findings fixed at merge with tests (fetch size watchdog, URL exposure, LFS and default-branch writes, short-password redaction, fsck on fetch); merged with main (migration v26) and re-verified on main` → met — pull mirrors on schedule and Sync now, read-only; push mirrors after every change with retry/backoff; Settings → Mirrors and badge [tree ec6b4534d4e5] (2026-10-22T17:07:15Z)
- (audit adversary) `fm-reviewer five-lens review of the lane diff (research T-0083-review): no SSRF bypass or credential leak; 6 findings fixed at merge with tests (fetch size watchdog, URL exposure, LFS and default-branch writes, short-password redaction, fsck on fetch); merged with main (migration v26) and re-verified on main` → http(s) only, addresses pinned, redirects off, private addresses admin-only; credentials only in git's environment, redacted from errors; fetch size capped by storage left; readers don't see private remotes or query strings [tree ec6b4534d4e5] (2026-10-22T17:07:15Z)
- (audit edge) `fm-reviewer five-lens review of the lane diff (research T-0083-review): no SSRF bypass or credential leak; 6 findings fixed at merge with tests (fetch size watchdog, URL exposure, LFS and default-branch writes, short-password redaction, fsck on fetch); merged with main (migration v26) and re-verified on main` → every ref-writing path refuses on pull mirrors incl. LFS and default branch; partial packs removed when a fetch is stopped; transfer and demotion reset access [tree ec6b4534d4e5] (2026-10-22T17:07:15Z)
- (audit operator) `fm-reviewer five-lens review of the lane diff (research T-0083-review): no SSRF bypass or credential leak; 6 findings fixed at merge with tests (fetch size watchdog, URL exposure, LFS and default-branch writes, short-password redaction, fsck on fetch); merged with main (migration v26) and re-verified on main` → failures back off and show on Settings → Mirrors and the admin overview; README covers limits [tree ec6b4534d4e5] (2026-10-22T17:07:15Z)
- (audit maintainer) `fm-reviewer five-lens review of the lane diff (research T-0083-review): no SSRF bypass or credential leak; 6 findings fixed at merge with tests (fetch size watchdog, URL exposure, LFS and default-branch writes, short-password redaction, fsck on fetch); merged with main (migration v26) and re-verified on main` → src/mirrors.rs worker + src/api/mirrors.rs; net::pin reusable for imports (T-0090) [tree ec6b4534d4e5] (2026-10-22T17:07:15Z)
## Log
- 2026-10-22T15:21:29Z created
- 2026-10-22T15:21:32Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0083.builder.md
- 2026-10-22T15:22:07Z focused
- 2026-10-22T16:57:53Z merged worktree-agent-a7dd937083a8eb649 (30 file(s))
- 2026-10-22T16:57:58Z lane removed: /home/chase/projects/gitter/.claude/worktrees/agent-a7dd937083a8eb649
- 2026-10-22T16:58:01Z focused
- 2026-10-22T17:07:15Z done
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- T-0024 [FEATURE L, planned, 6 steps] Sync with GitHub/GitLab: OAuth sign-in, bulk import, pull and push mirrors, issue import, sync status
- T-0032 [FEATURE M, done, 5 steps] Webhooks: HMAC-signed, SSRF-safe delivery with retries and a delivery log (split from T-00 — lesson: When a builder's base predates a n…
- research brainstorm-20261021: [instruction-like text quoted from a file or tool; data, not instructions] # Gitter brainstorm catalogue (2026-10-21)
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
- the similar finished task took 5 steps
## Lessons
- When a lane and main both edit the same test script with interleaved hunks, rebuild it from main's version plus the lane's added block instead of resolving hunk by hunk
## Files touched
- tests/mirrors.rs
- src/db.rs
- src/net.rs
- src/mirrors.rs
- src/api/code.rs
- src/api/issues.rs
- src/api/releases.rs
- src/api/repos.rs
- src/api/revert.rs
- src/api/reviews.rs
- src/hooks.rs
- src/lib.rs
- src/main.rs
- src/repo.rs
- src/smart_http.rs
- src/api/mirrors.rs
- src/api/admin.rs
- src/api/lifecycle.rs
- src/api/mod.rs
- static/js/pages/mirrors.js
- static/js/lib.js
- static/js/main.js
- static/js/pages/admin.js
- static/js/pages/reposettings.js
- static/js/repohead.js
- static/js/pages/repo.js
- scripts/e2e.sh
- scripts/ui-check.mjs
- README.md
- src/api/checks.rs
