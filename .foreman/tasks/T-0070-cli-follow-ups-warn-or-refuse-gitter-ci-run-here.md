---
id: T-0070
type: FIX
tier: S
status: planned
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T02:26:50Z
updated: 2026-10-22T22:01:03Z
base: 826af23e438a44d0c2084f736ba5e0fe7a940ca3
base_tree: 037fd34f1871545ffb4f5b4d67011a8399123f77
plan0_at: 2026-10-22T22:00:31Z
plan0_scope: []
paused_tree: 037fd34f1871545ffb4f5b4d67011a8399123f77
---
# CLI safety follow-ups: ci run --here refuses image jobs, pr create -R elsewhere needs --head, warn on plain-http logins to other machines
## Raw request
> CLI follow-ups: warn or refuse 'gitter ci run --here' for image jobs (it mounts and chowns the real tree); 'pr create -R other/repo' should require --head when -R doesn't match the cwd remote; warn on non-loopback http:// logins (from T-0013 review)
## Interpretation
## Assumptions (confidence)
## Acceptance criteria
- [ ] the three cases behave as stated — verify with `cargo test -q --test cli`
## Non-goals
## Approach (options → choice → why)
## Risks and rollback
## Execution prompt
## Steps
1. [ ] tests for the three cases, then the checks in src/cli.rs  <- CURRENT
## Resume here
<!-- auto -->
- 2026-10-22T22:01:02Z step 1/1: tests for the three cases, then the checks in src/cli.rs
- git: main; 0 uncommitted
## Docs impact
## Verification evidence
## Log
- 2026-10-22T02:26:50Z created
- 2026-10-22T22:00:31Z planned from capture
- 2026-10-22T22:00:31Z focused
- 2026-10-22T22:01:03Z paused: focus moved to T-0116
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research T-0013-review: Verdict:** approve with nits. I found no token leak and no change to server-side runner behaviour. The findings below are two MEDIUM and several …
- research T-0001-review-2: Verdict:** changes needed. I read the diff and spot-checked `src/`, `scripts/` and `static/js` for line numbers. I did not run cargo or the e2e…
- research T-0004-review: Verdict: changes needed.** None of these block the four acceptance scripts. Findings 1, 2 and 3 break the documented Docker workflow. I verified …
- research T-0083-review: Verdict: approve with nits.** I found no SSRF bypass, credential leak or unguarded ref-writing path. There are two MEDIUM issues (unbounded fetch…
