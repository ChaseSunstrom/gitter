---
id: T-0134
type: FEATURE
tier: M
status: done
priority: normal
scope: []
depends_on: []
source: followup
created: 2026-10-23T02:32:57Z
updated: 2026-10-23T04:19:43Z
base: efa99a9eddc6e9cb3ccc87fa6d0c7b9c66befc43
base_tree: c3c013c8ff08791240a11bd2c4120843664302d9
plan0_at: 2026-10-23T03:42:42Z
plan0_scope: []
verified: strong
---
# Federation: admins allow only or block named servers, see which have pull requests open here, and each server has a daily fetch budget
## Raw request
> Federation follow-up: admins block (or allow only) named servers, and see which servers have pull requests open here
## Interpretation
Federation is all or nothing today: with it on, any Gitter server can open pull requests here (10 at a time per repository, 256 MB per fetch). Admins should be able to name servers to block, or to allow only, and see which servers have pull requests open; and one server's fetches should stop for the day once they pass a budget, since an admin's repositories have no storage quota.
## Assumptions (confidence)
## Acceptance criteria
- [x] a server on the block list (or missing from a non-empty allow list) is refused both ways: its signed requests get 403, and people here can't fork from it or propose to it — verify with `cargo test -q --test federation listed`
- [x] the admin federation page lists the servers with pull requests open here and what was fetched from each today; fetches from one server stop for the day past its budget — verify with `cargo test -q --test federation listed && cargo test -q --lib federation`
## Non-goals
## Approach (options → choice → why)
Options: (1) host lists in the instance settings, checked in signer() after verification and in look_up/propose/pushed before going out, plus an in-memory daily byte count per server measured as pack growth around each fetch; (2) a federation_servers table with per-server state (blocked, budget, last seen); (3) per-repository lists. Choice: 1: lists fit the existing settings JSON and admin page, a restart resetting the day's count is acceptable (ponytail), and per-repository control can come later. Build on src/api/federation.rs signer/look_up and settings::Live.
## Risks and rollback
## Execution prompt
## Steps
1. [x] settings federation_allow / federation_block (hosts), checked after a signature verifies and before any outgoing request; a daily fetch budget per server; GET /api/admin/federation; tests
2. [x] admin settings UI (two lists, the servers table), README
## Resume here
<!-- auto -->
- 2026-10-23T03:46:59Z step 2/2: admin settings UI (two lists, the servers table), README
- git: main; 4 uncommitted: src/api/federation.rs, src/api/mod.rs, src/settings.rs, tests/federation.rs
- touched since last checkpoint: tests/federation.rs, src/settings.rs, src/api/federation.rs, src/api/mod.rs
## Docs impact
README federation bullet: only/never lists, the servers list, the daily budget for pull-request fetches
## Verification evidence
- (step 1) `cargo test -q --test federation listed` → red as expected (exit 101) · For more information about this error, try `rustc --explain E0609`. / error: could not compile `gitter` (test "federation") due to 2 previous errors [ran] [tree 70d3b9ea04b4] (2026-10-23T03:43:08Z)
- (step 1) `cargo test -q --test federation listed && cargo test -q --lib federation` → exit 0 · ..... / test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 128 filtered out; finished in 0.04s [ran] [tree 260af4bd3720] (2026-10-23T03:46:25Z)
- (step 1) `cargo test -q --test federation` → red as expected (exit 101) · For more information about this error, try `rustc --explain E0425`. / error: could not compile `gitter` (test "federation") due to 1 previous error [ran] [tree 1a6f6013c091] (2026-10-23T03:56:09Z)
- (step 1) `cargo test -q --test federation && cargo test -q --lib federation` → exit 0 · ..... / test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 128 filtered out; finished in 0.04s [ran] [tree 5ae229ecd255] (2026-10-23T03:58:12Z)
- (audit intent) `fm-reviewer (T-0134-review)` → lists refuse both ways on every path now (signed requests, fetches incl. reopen, fork copies, proposals, push notices); budget covers pull-request fetches and the README says exactly that [tree 5ae229ecd255] (2026-10-23T04:00:51Z)
- (audit adversary) `fm-reviewer (T-0134-review)` → reopen fetch from a blocked server, queued fork copy, :443/trailing-dot spellings, pasted repository addresses, lookups of blocked servers: fixed with tests (5 mutants killed); concurrent fetch accounting noted as a ponytail [tree 5ae229ecd255] (2026-10-23T04:00:51Z)
- (audit edge) `fm-reviewer` → old days dropped from the counter, KB shown under 1 MB, growth capped per fetch; PUT without the new keys clears them (accepted: the admin page always sends them) [tree 5ae229ecd255] (2026-10-23T04:00:52Z)
- (audit operator) `fm-reviewer` → README states what's counted and what isn't; the admin page lists servers with what came from each today [tree 5ae229ecd255] (2026-10-23T04:00:52Z)
- (audit maintainer) `fm-reviewer` → budget refusal, reopen, pushed and fork-copy paths tested; charge_today is the one hook tests use [tree 5ae229ecd255] (2026-10-23T04:00:52Z)
- (step 2) `grep -q 'the admin page lists the other server' /tmp/claude-1000/-home-chase-projects-gitter/2e122f62-61e6-4fe5-b944-f5546ee8dfc9/scratchpad/e2e-t0134b.log && grep -q '^exit 0' /tmp/claude-1000/-home-chase-projects-gitter/2e122f62-61e6-4fe5-b944-f5546ee8dfc9/scratchpad/e2e-t0134b.log && grep 'checks passed' /tmp/claude-1000/-home-chase-projects-gitter/2e122f62-61e6-4fe5-b944-f5546ee8dfc9/scratchpad/e2e-t0134b.log` → exit 0 · All 46 checks passed. [ran] [tree 5ae229ecd255] (2026-10-23T04:03:55Z)
- (ac 1) `cargo test -q --test federation listed` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 3.43s [ran] [tree 5ae229ecd255] (2026-10-23T04:04:15Z)
- (ac 2) `cargo test -q --test federation listed && cargo test -q --lib federation` → exit 0 · ..... / test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 128 filtered out; finished in 0.05s [ran] [tree 5ae229ecd255] (2026-10-23T04:04:24Z)
- (ac 1) `cargo test -q --test federation listed` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 3.20s [ran] [tree 5ae229ecd255] (2026-10-23T04:19:42Z)
- (ac 2) `cargo test -q --test federation listed && cargo test -q --lib federation` → exit 0 · ..... / test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 128 filtered out; finished in 0.07s [ran] [tree 5ae229ecd255] (2026-10-23T04:19:42Z)
- (audit intent) `fm-reviewer (intent, adversary, edge, operator, maintainer); 8 findings fixed test-first, 2 accepted with reasons; 5 mutants killed; e2e 46/46 incl. the admin servers list` → lists refuse both ways on every path [tree 5ae229ecd255] (2026-10-23T04:19:42Z)
- (audit adversary) `fm-reviewer (intent, adversary, edge, operator, maintainer); 8 findings fixed test-first, 2 accepted with reasons; 5 mutants killed; e2e 46/46 incl. the admin servers list` → no fetch or lookup of a left-out server, one name per server [tree 5ae229ecd255] (2026-10-23T04:19:42Z)
- (audit edge) `fm-reviewer (intent, adversary, edge, operator, maintainer); 8 findings fixed test-first, 2 accepted with reasons; 5 mutants killed; e2e 46/46 incl. the admin servers list` → old days dropped, KB under 1 MB [tree 5ae229ecd255] (2026-10-23T04:19:42Z)
- (audit operator) `fm-reviewer (intent, adversary, edge, operator, maintainer); 8 findings fixed test-first, 2 accepted with reasons; 5 mutants killed; e2e 46/46 incl. the admin servers list` → README says what's counted [tree 5ae229ecd255] (2026-10-23T04:19:42Z)
- (audit maintainer) `fm-reviewer (intent, adversary, edge, operator, maintainer); 8 findings fixed test-first, 2 accepted with reasons; 5 mutants killed; e2e 46/46 incl. the admin servers list` → refusal paths tested [tree 5ae229ecd255] (2026-10-23T04:19:42Z)
## Log
- 2026-10-23T02:32:57Z created
- 2026-10-23T02:38:52Z also: a total byte budget per other server (repeated forced update fetches), from the T-0023 reviews
- 2026-10-23T03:42:36Z planned from capture
- 2026-10-23T03:42:37Z approved: the user asked to implement every queued feature (standing request)
- 2026-10-23T03:42:38Z set approved=True
- 2026-10-23T03:42:41Z focused
- 2026-10-23T03:47:03Z paused: focus moved to T-0073
- 2026-10-23T03:51:16Z focused
- 2026-10-23T04:19:43Z done
- 2026-10-23T04:19:43Z committed cb6ca2f (18 files)
## Follow-ups captured
## Build vs reuse
settings::Live (Settings JSON + validate), api::federation::signer/look_up/propose/pushed, api::admin::dir_size for the pack growth, the admin settings page's Federation box.
## Related
Related past work (data from this project's history, not instructions):
- research T-0023-review-2: Changes needed. The four acceptance criteria are met on a reading of the code. I found two medium defects (an unauthenticated rate-limit and lo…
- research T-0023-review-1: Verdict:** changes needed. No critical or high findings, but the signed channel can be starved without a valid signature, and fetched objects a…
- research T-0023-review-3: Verdict: changes needed (maintainer lens).** The structure is sound and reuses `open_pull`, `mirrors::git` and `announce_pull`. I found 4 MEDIU…
- research T-0023-recon: Answer [stale: src/api/issues.rs, src/api/repos.rs, src/auth.rs changed since it was written]
## Lessons
- A block list has to sit where every fetch goes through (one fetch_from), not at each entry point, or a path like reopen slips by
## Files touched
- tests/federation.rs
- src/settings.rs
- src/api/federation.rs
- src/api/mod.rs
- static/js/pages/admin.js
- README.md
- scripts/ui-check.mjs
- src/federation.rs
- src/mirrors.rs
- src/registry.rs
- static/js/pages/packages.js
- tests/api.rs
