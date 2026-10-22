---
id: T-0062
type: CLEAN
tier: S
status: done
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T01:25:20Z
updated: 2026-10-22T20:12:10Z
base: f60ed7d6e7b107cc762c5e21e9f709f2e1483dec
base_tree: 22641e381c9eba8db1ff7373e1173a54cdf8fd2b
plan0_at: 2026-10-22T19:38:46Z
plan0_scope: []
verified: strong
---
# Lifecycle follow-ups: free-disk check before building an export (refuse below ~2x the acco
## Raw request
> Lifecycle follow-ups: free-disk check before building an export (refuse below ~2x the account size); the 7-day trash/offer windows as admin settings; one helper for the '~' || id trash rename; show the transfer button only to owners (meta.can_transfer); hooks.rs refresh_size on a repo trashed mid-push (from T-0033 review)
## Interpretation
Small follow-ups from the lifecycle review. Building a data export first checks free disk space and refuses when there's less than about twice the account's stored size; the trash and transfer-offer windows (7 days) become admin settings; the '~' || id trash rename lives in one helper used by trash, restore's rollback and the migration notes; repo settings only show Transfer ownership to people who may transfer (a can_transfer flag in the repo JSON: the owner, an owner of the owning organization, or a site admin); the size refresh after a push skips a repository that was trashed meanwhile so its quota isn't understated.
## Assumptions (confidence)
## Acceptance criteria
- [x] Exports refuse on low disk, the trash and offer windows follow the settings, can_transfer is in the repo JSON only for owners, and a trashed repo's size isn't zeroed by a late push hook — verify with `cargo test -q --test api lifecycle_settings`
- [x] Settings pages render and e2e stays green — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
(a) Free space from statvfs on the data dir through the libc crate, which is already in the dependency tree (add it as a direct dependency with the same version), skipping the check on non-unix [chosen: a few lines, no new download]; (b) no check (the status quo). The windows go through the existing settings struct and admin page like the other limits.
## Risks and rollback
## Execution prompt
## Steps
1. [x] free-disk check before exports (unix statvfs), clear error
2. [x] trash and offer windows as admin settings with bounds; admin settings UI
3. [x] one helper for the trash rename; can_transfer in the repo JSON and the settings page; skip size refresh for trashed repos; tests
## Resume here
## Docs impact
Admin settings page explains the two new windows; no README section covers them
## Verification evidence
- (ac 1) `cargo test -q --test api lifecycle_settings 2>&1 | tail -25` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 74 filtered out; finished in 1.47s [ran] [tree 73d5b644fc2a] (2026-10-22T19:42:01Z)
- (step 1) `cargo test -q --test api lifecycle_settings 2>&1 | tail -25` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 74 filtered out; finished in 1.47s [ran] [tree 73d5b644fc2a] (2026-10-22T19:42:01Z)
- (ac 2) `bash scripts/e2e.sh` → ✗ exit 1 · git over SSH on 127.0.0.1:32677 / gitter 0.1.0 listening on 127.0.0.1:32676 — open http://127.0.0.1:32676 [ran] [tree 73d5b644fc2a] (2026-10-22T19:45:08Z)
- (step 2) `SKIP_UI=1 bash scripts/e2e.sh` → exit 0 · · UI check skipped (SKIP_UI=1) / All 43 checks passed. [ran] [tree 73d5b644fc2a] (2026-10-22T19:45:42Z)
- (step 3) `cargo test -q --test api lifecycle_settings` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 74 filtered out; finished in 1.22s [ran] [tree 73d5b644fc2a] (2026-10-22T19:45:47Z)
- (step 2) `SKIP_UI=1 bash scripts/e2e.sh` → exit 0 · · UI check skipped (SKIP_UI=1) / All 43 checks passed. [ran] [tree 73d5b644fc2a] (2026-10-22T19:46:07Z)
- (step 1) `cargo test -q --test api lifecycle_settings_export` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 75 filtered out; finished in 0.00s [ran] [tree 73d5b644fc2a] (2026-10-22T19:46:11Z)
- (ac 1) `cargo test -q --test api lifecycle_settings 2>&1 | tail -15` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 74 filtered out; finished in 2.73s [ran] [tree 3c3f2e8ad547] (2026-10-22T19:49:43Z)
- (ac 2) `bash scripts/e2e.sh` → ✗ exit 1 · git over SSH on 127.0.0.1:38621 / gitter 0.1.0 listening on 127.0.0.1:38620 — open http://127.0.0.1:38620 [ran] [tree 3c3f2e8ad547] (2026-10-22T19:51:05Z)
- (ac 2) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 44 checks passed. [ran] [tree 3c3f2e8ad547] (2026-10-22T19:53:47Z)
- (ac 1) `cargo test -q --test api lifecycle_settings` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 76 filtered out; finished in 2.27s [ran] [tree c531d63fd4fa] (2026-10-22T20:05:39Z)
- (step 1) `cargo test -q --test api lifecycle_settings` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 76 filtered out; finished in 2.24s [ran] [tree c531d63fd4fa] (2026-10-22T20:05:45Z)
- (step 2) `cargo test -q --test api lifecycle_settings` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 76 filtered out; finished in 2.21s [ran] [tree c531d63fd4fa] (2026-10-22T20:05:48Z)
- (step 3) `cargo test -q --test api lifecycle_settings` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 76 filtered out; finished in 2.27s [ran] [tree c531d63fd4fa] (2026-10-22T20:05:51Z)
- (ac 2) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 44 checks passed. [ran] [tree c531d63fd4fa] (2026-10-22T20:09:30Z)
- (ac 1) `cargo test -q --test api lifecycle_settings` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 76 filtered out; finished in 2.23s [ran] [tree c531d63fd4fa] (2026-10-22T20:12:10Z)
- (ac 2) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 44 checks passed. [ran] [tree c531d63fd4fa] (2026-10-22T20:12:10Z)
- (audit intent) `fm-reviewer full review of the lane branch; 6 items fixed in the lane (copy follows settings with plural days, trash-shortening hint, retention wording, disk check before the rate limiter, backfill/restore sizes, org-owner can_transfer test)` → all points met; UI copy now follows trash_days/transfer_offer_days [tree c531d63fd4fa] (2026-10-22T20:12:10Z)
- (audit adversary) `fm-reviewer full review of the lane branch; 6 items fixed in the lane (copy follows settings with plural days, trash-shortening hint, retention wording, disk check before the rate limiter, backfill/restore sizes, org-owner can_transfer test)` → settings bounded 1-365, missing fields default to 7; removed constants gone [tree c531d63fd4fa] (2026-10-22T20:12:10Z)
- (audit edge) `fm-reviewer full review of the lane branch; 6 items fixed in the lane (copy follows settings with plural days, trash-shortening hint, retention wording, disk check before the rate limiter, backfill/restore sizes, org-owner can_transfer test)` → shortening trash_days purges older trash at the next sweep (hinted on the field); offers keep their stored expiry [tree c531d63fd4fa] (2026-10-22T20:12:10Z)
- (audit operator) `fm-reviewer full review of the lane branch; 6 items fixed in the lane (copy follows settings with plural days, trash-shortening hint, retention wording, disk check before the rate limiter, backfill/restore sizes, org-owner can_transfer test)` → df reused (no new dependency); unknown free space lets exports through [tree c531d63fd4fa] (2026-10-22T20:12:10Z)
- (audit maintainer) `fm-reviewer full review of the lane branch; 6 items fixed in the lane (copy follows settings with plural days, trash-shortening hint, retention wording, disk check before the rate limiter, backfill/restore sizes, org-owner can_transfer test)` → one trash_sql helper; doc comments say retention window [tree c531d63fd4fa] (2026-10-22T20:12:10Z)
- (audit self) `fm-reviewer full review of the lane branch; 6 items fixed in the lane (copy follows settings with plural days, trash-shortening hint, retention wording, disk check before the rate limiter, backfill/restore sizes, org-owner can_transfer test)` → no findings [tree c531d63fd4fa] (2026-10-22T20:12:10Z)
## Log
- 2026-10-22T01:25:20Z created
- 2026-10-22T19:38:31Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0062.builder.md
- 2026-10-22T19:38:45Z focused
- 2026-10-22T20:04:56Z merged worktree-agent-a786761f8550c2a77 (9 file(s))
- 2026-10-22T20:04:59Z lane removed: /home/chase/projects/gitter/.claude/worktrees/agent-a786761f8550c2a77
- 2026-10-22T20:04:59Z focused
- 2026-10-22T20:12:10Z done
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research T-0033-review: Verdict: changes needed.** The core is solid. Trashed repos are excluded from `lookup`, `VISIBLE`, CI, LFS and git, and private repos don't leak …
- T-0033 [FEATURE M, done, 5 steps] Repo rename/transfer with redirects, trash with undo, account data export and self-delete — lesson: A new ownership model (orgs) touche…
- research T-0032-review: Verdict: changes needed.** The SSRF core holds up and the checks are correct for the cases I traced. The blocker is the "owner/writer" gate, whic…
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
## Behaviour lock
none before edits (built in a builder lane); existing lifecycle tests (trash, restore, transfer offers, export) kept and pass unchanged
## Lessons
- When a constant becomes a setting, grep the UI copy for the old value too: the review found it in six places.
## Files touched
- src/api/lifecycle.rs
- src/settings.rs
- static/js/pages/admin.js
- src/api/repos.rs
- src/hooks.rs
- static/js/pages/reposettings.js
- tests/api.rs
- src/api/account.rs
- static/js/pages/settings.js
