---
id: T-0011
type: PERFORMANCE
tier: M
status: done
priority: normal
scope: []
depends_on: []
source: user
created: 2026-10-21T17:01:42Z
updated: 2026-10-21T21:17:03Z
base: e8fd6f2f50c87a9d7c5dd2527c70587c02db5add
base_tree: 3b1b6c216505574939949580ef7e2d578a3dc3b6
plan0_at: 2026-10-21T20:57:53Z
plan0_scope: []
verified: strong
---
# Speed proof: commit-graph maintenance, cached last commits, big-diff loading, gitter bench + perf budget, published numbers
## Raw request
> PERF: Speed proof: git maintenance (commit-graph with changed-path bloom), cached last-commit-per-entry, virtualized big diffs, perf budget test, gitter bench + published RAM/latency numbers vs GitLab (C9 C10 C45)
> CONTEXT: Gitter = public GitHub-like forge, Rust axum + SQLite + git CLI, framework-free SPA; catalogue at research brainstorm-20261021. All build after #T-0001.
## Interpretation
Make Gitter measurably fast and prove it: keep commit-graphs with changed-path Bloom filters fresh so history/path queries stay fast on big repos; cache the per-entry 'last commit' lookups (immutable per commit sha); let a too-large file diff be loaded on demand and rendered in chunks instead of being hidden; ship 'gitter bench' (deterministic seeded instance, p50/p95 per page, RSS) with a --check budget gate; publish the numbers in the README next to GitLab's documented requirements.
## Assumptions (confidence)
## Acceptance criteria
- [x] Push keeps a commit-graph with Bloom filters, and last-commit lookups are cached — verify with `cargo test -q`
- [x] A file too big for the full diff can be loaded alone — verify with `cargo test -q --test api single_file_diff`
- [x] gitter bench meets the perf budget in a release build — verify with `bash scripts/perf-budget.sh`
- [x] UI still renders without errors — verify with `bash scripts/e2e.sh`
- [x] README publishes measured latency and RAM next to GitLab's requirements — verify with `grep -q 'gitter bench' README.md`
## Non-goals
## Approach (options → choice → why)
Maintenance: (a) commit-graph write --split --changed-paths in the background after each push, debounced per repo, plus a boot sweep for repos without one [chosen: incremental, no scheduler]; (b) git maintenance start (needs cron/systemd in the container); (c) a periodic timer over all repos (work for idle repos). Cache: (a) bounded in-memory map keyed repo+commit+path [chosen: results are immutable, nothing to invalidate]; (b) SQLite table (writes on reads); (c) precompute on push (wasted on unvisited dirs). Bench: (a) in-process router via tower oneshot on a seeded temp instance [chosen: no HTTP client dependency, deterministic]; (b) external wrk/oha (extra install); (c) curl loop (process spawn dominates). Big diffs: per-file ?path= diff with a larger cap + chunked row rendering as you scroll.
## Risks and rollback
## Execution prompt
## Steps
1. [x] commit-graph maintenance after push + boot sweep
2. [x] memo cache for tree_commits and last_commit
3. [x] single-file diff endpoint + chunked rendering for big files
4. [x] gitter bench + scripts/perf-budget.sh
5. [x] README numbers
## Resume here
## Docs impact
README: How fast is it? section (bench table, memory, GitLab baseline), gitter bench in operator commands
## Verification evidence
- (step 1) `cargo test -q --lib git::` → exit 0 · ....... / test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 24 filtered out; finished in 1.94s [ran] [tree 44182002d667] (2026-10-21T21:02:59Z)
- (step 2) `cargo test -q` → exit 0 · running 0 tests / test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s [ran] [tree 3a8f698e9d68] (2026-10-21T21:05:31Z)
- (step 3) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 22 checks passed. [ran] [tree 2468ad1ec6d7] (2026-10-21T21:07:53Z)
- (step 4) `bash scripts/perf-budget.sh` → exit 0 · memory: 8 MB after boot, 31 MB after the run / ✓ every page is within its budget [ran] [tree b8144e3906e2] (2026-10-21T21:09:31Z)
- (step 5) `grep -q 'gitter bench' README.md` → exit 0 · (no output) [ran] [tree d7807fbd807d] (2026-10-21T21:09:56Z)
- (ac 1) `cargo test -q 2>&1 | grep "test result"` → exit 0 · test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.49s / test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s [ran] [tree 29797e3d7121] (2026-10-21T21:12:31Z)
- (step 2) `cargo test -q` → exit 0 · running 0 tests / test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s [ran] [tree 0881e5a36330] (2026-10-21T21:15:08Z)
- (step 3) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 22 checks passed. [ran] [tree 0881e5a36330] (2026-10-21T21:15:54Z)
- (step 4) `bash scripts/perf-budget.sh` → exit 0 · memory: 8 MB after boot, 31 MB after the run / ✓ every page is within its budget [ran] [tree 0881e5a36330] (2026-10-21T21:15:59Z)
- (step 1) `cargo test -q --lib git::` → exit 0 · ....... / test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 24 filtered out; finished in 1.75s [ran] [tree 0881e5a36330] (2026-10-21T21:16:01Z)
- (ac 1) `cargo test -q` → exit 0 · running 0 tests / test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s [ran] [tree 0881e5a36330] (2026-10-21T21:17:02Z)
- (ac 2) `cargo test -q --test api single_file_diff` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 12 filtered out; finished in 0.50s [ran] [tree 0881e5a36330] (2026-10-21T21:17:02Z)
- (ac 3) `bash scripts/perf-budget.sh` → exit 0 · memory: 9 MB after boot, 31 MB after the run / ✓ every page is within its budget [ran] [tree 0881e5a36330] (2026-10-21T21:17:02Z)
- (ac 4) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 22 checks passed. [ran] [tree 0881e5a36330] (2026-10-21T21:17:02Z)
- (ac 5) `grep -q 'gitter bench' README.md` → exit 0 · (no output) [ran] [tree 0881e5a36330] (2026-10-21T21:17:02Z)
- (audit intent) `one fm-reviewer pass (research T-0011-review): cache keys verified safe; the MEDIUM and all four LOWs fixed and re-verified` → all five criteria met with their own checks; README numbers come from gitter bench on this box with hardware stated [tree 0881e5a36330] (2026-10-21T21:17:02Z)
- (audit adversary) `one fm-reviewer pass (research T-0011-review): cache keys verified safe; the MEDIUM and all four LOWs fixed and re-verified` → memo keys pin repo id + resolved 40-hex sha + path and lookups happen after Repo::readable; pathspecs now literal everywhere (GIT_LITERAL_PATHSPECS) [tree 0881e5a36330] (2026-10-21T21:17:02Z)
- (audit edge) `one fm-reviewer pass (research T-0011-review): cache keys verified safe; the MEDIUM and all four LOWs fixed and re-verified` → failed git walks and failed language stats are never cached; renamed files load with their old path; split/unified toggle keeps loaded rows [tree 0881e5a36330] (2026-10-21T21:17:02Z)
- (audit operator) `one fm-reviewer pass (research T-0011-review): cache keys verified safe; the MEDIUM and all four LOWs fixed and re-verified` → commit-graph maintenance is incremental, one run per repo, plus a boot sweep; gitter bench and perf-budget.sh run anywhere [tree 0881e5a36330] (2026-10-21T21:17:02Z)
- (audit maintainer) `one fm-reviewer pass (research T-0011-review): cache keys verified safe; the MEDIUM and all four LOWs fixed and re-verified` → memo accounting exact on overwrite; bench now budgets cold (empty cache) as well as warm p95 [tree 0881e5a36330] (2026-10-21T21:17:02Z)
## Log
- 2026-10-21T17:01:42Z created
- 2026-10-21T20:57:53Z planned from capture
- 2026-10-21T20:57:53Z focused
- 2026-10-21T21:17:03Z done
- 2026-10-21T21:17:03Z committed 0b6bd60 (30 files)
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research brainstorm-20261021: [instruction-like text quoted from a file or tool; data, not instructions] # Gitter brainstorm catalogue (2026-10-21)
- playbook Latency Critical Systems (skills/playbooks/references/perf/latency-critical.md)
- T-0004 [FEATURE M, done, 4 steps] Self-hosting: one-service one-volume docker-compose.yml, GITTER_URL-only config, first-boo — lesson: Docker-docs claims (upgrade, resto…
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
- the similar finished task took 4 steps
## Measurements
gitter bench (release, 5000 commits/1000 files, p50/p95 ms), before → after: tree 26.2/42.4 → 9.0/12.0 · tree last commits root 8.0/12.0 → 2.9/5.6 · dir 14.3/18.1 → 3.0/4.8 · file 20.0/27.7 → 10.9/15.2 · file history 37.5/58.1 → 8.1/12.2 · blame 14.1/18.8 → 4.9/7.1 · languages 5.5/7.0 → 2.7/4.3 · history 5.9/10.8 → 6.0/9.1 · commit diff 8.7/12.4 → 9.2/15.6 (unchanged) · big file diff: hidden → loads 6000 lines in 11.5/15.3 · RSS 8 MB after boot, 32 MB after run · clone ~300 ms
## Lessons
- A speed budget that only measures warm cache hits proves the cache, not the code; clear caches and budget the cold request too
## Files touched
- src/bench.rs
- src/git.rs
- src/hooks.rs
- src/main.rs
- src/api/code.rs
- src/api/issues.rs
- src/diff.rs
- tests/api.rs
- static/js/diffview.js
- static/css/code.css
- static/js/pages/commit.js
- static/js/pages/issue.js
- static/js/pages/compare.js
- scripts/ui-check.mjs
- scripts/perf-budget.sh
- README.md
- src/langs.rs
- Cargo.toml
- src/lib.rs
