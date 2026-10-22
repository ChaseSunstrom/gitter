---
id: T-0096
type: FEATURE
tier: M
status: done
priority: normal
scope: []
depends_on: []
source: user
created: 2026-10-22T17:46:23Z
updated: 2026-10-22T18:27:32Z
base: d0ddee41bfb70b9153ce31ea0bbf0e344e065abb
base_tree: ec2be356b1dacd247cc5328c4b8e724c3efd1b39
plan0_at: 2026-10-22T18:10:49Z
plan0_scope: []
verified: strong
---
# FEATURE: New repository page can mirror or import a remote URL directly (creates the repo
## Raw request
> FEATURE: New repository page can mirror or import a remote URL directly (creates the repo empty and starts the first sync at once; one-time import or keep in sync), so users don't have to know to untick the README first
## Interpretation
On the New repository page, 'Start with' offers a README (default), nothing, or 'Copy from a URL': a remote URL, optional user name and password/token, an admin-only 'allow private addresses', and 'Keep it in sync' (a read-only pull mirror). Copying creates the repository empty and adds a pull mirror whose first sync starts at once; without 'Keep it in sync' the mirror is a one-time import that removes itself after its first successful sync, leaving an ordinary repository. The repository page shows 'importing from …' or 'mirror of …' and refreshes itself until the first sync lands.
## Assumptions (confidence)
## Acceptance criteria
- [x] A pull mirror added with once:true syncs, then removes itself so the repository is writable; a normal one stays; the badge says importing vs mirror — verify with `cargo test -q --test mirrors`
- [x] New repository → Copy from a URL creates the repository, it fills in by itself, and with Keep it in sync off it ends up an ordinary repository — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
(a) two existing calls from the page (create empty repo, add pull mirror) plus a 'once' flag on mirrors that the worker drops after the first successful sync [chosen: reuses the mirror worker, pinning and credential handling; survives the user leaving the page]; (b) a server-side clone at create time (blocks the request on a big fetch, duplicates mirror safety code); (c) client deletes the mirror after the sync (breaks if the page is closed).
## Risks and rollback
## Execution prompt
## Steps
1. [x] failing mirror test for once
2. [x] schema v28 once column, API flag, worker removes once mirrors after first sync, badge
3. [x] New repository 'Start with' options and copy form; repo page refreshes until synced
4. [x] README, ui-check block, e2e
## Resume here
## Docs impact
README: Mirrors section explains Start with → A copy of another repository
## Verification evidence
- (step 1) `cargo test -q --test mirrors one_time` → ✗ exit 101 · test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 3 filtered out; finished in 0.59s / error: test failed, to rerun pass `--test mirrors` [ran] [tree 8f99102fc28a] (2026-10-22T18:11:27Z)
- (step 2) `cargo test -q --test mirrors` → exit 0 · .... / test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.50s [ran] [tree 1b9f931f028a] (2026-10-22T18:12:27Z)
- (step 3) `grep -q 'ok New repository copies a URL' /tmp/claude-1000/-home-chase-projects-gitter/2e122f62-61e6-4fe5-b944-f5546ee8dfc9/scratchpad/t0096-e2e.log` → exit 0 · (no output) [ran] [tree 5f00eab5599a] (2026-10-22T18:21:44Z)
- (step 4) `grep -q 'ok New repository copies a URL' /tmp/claude-1000/-home-chase-projects-gitter/2e122f62-61e6-4fe5-b944-f5546ee8dfc9/scratchpad/t0096-e2e.log` → exit 0 · (no output) [ran] [tree 5f00eab5599a] (2026-10-22T18:21:45Z)
- (ac 1) `cargo test -q --test mirrors` → exit 0 · .... / test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.36s [ran] [tree 5f00eab5599a] (2026-10-22T18:21:53Z)
- (ac 2) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 44 checks passed. [ran] [tree 5f00eab5599a] (2026-10-22T18:24:38Z)
- (step 1) `cargo test -q --test mirrors one_time` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 3 filtered out; finished in 0.83s [ran] [tree 5f00eab5599a] (2026-10-22T18:24:57Z)
- (ac 1) `cargo test -q --test mirrors` → exit 0 · .... / test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.85s [ran] [tree 5f00eab5599a] (2026-10-22T18:27:31Z)
- (ac 2) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 44 checks passed. [ran] [tree 5f00eab5599a] (2026-10-22T18:27:31Z)
- (audit intent) `self-review of the diff plus a real run: New repository → copy of https://github.com/ChaseSunstrom/foreman into a demo server filled in after ~6 s and ended an ordinary repository; mirrors tests and e2e green` → met — copying or mirroring a URL is one step on the New repository page; the first sync starts at once and the page fills itself in [tree 5f00eab5599a] (2026-10-22T18:27:31Z)
- (audit adversary) `self-review of the diff plus a real run: New repository → copy of https://github.com/ChaseSunstrom/foreman into a demo server filled in after ~6 s and ended an ordinary repository; mirrors tests and e2e green` → goes through the existing mirror API (recent password, admin-only private addresses, pinned addresses, credentials only in git's environment); a one-time copy's row and sealed password are deleted after the first sync; sync errors shown only to the repository's admins, already redacted [tree 5f00eab5599a] (2026-10-22T18:27:31Z)
- (audit edge) `self-review of the diff plus a real run: New repository → copy of https://github.com/ChaseSunstrom/foreman into a demo server filled in after ~6 s and ended an ordinary repository; mirrors tests and e2e green` → a failed first sync stays visible with its error and retries with backoff; push mirrors can't be one-time; the page drops its cached empty file list before re-rendering (that crash was found and fixed in the real run) [tree 5f00eab5599a] (2026-10-22T18:27:31Z)
## Log
- 2026-10-22T17:46:23Z created
- 2026-10-22T18:10:48Z set type=FEATURE
- 2026-10-22T18:10:49Z focused
- 2026-10-22T18:27:32Z done
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research T-0083-review: Verdict: approve with nits.** I found no SSRF bypass, credential leak or unguarded ref-writing path. There are two MEDIUM issues (unbounded fetch…
- research T-0024-recon: 1. **Answer [stale: src/api/issues.rs, src/db.rs, src/hooks.rs changed since it was written]
- T-0083 [FEATURE M, done, 5 steps] Repository mirrors: scheduled pull mirrors and push mirrors with a Sync page (split from T-0024) — lesson: When a lane and main both ed…
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
- the similar finished task took 5 steps
## Lessons
- A page that polls for a change must invalidate the client GET cache for that resource before re-rendering, or it renders the stale cached state
## Plan gaps
- failed first: step 1 failing mirror test for once
## Files touched
- tests/mirrors.rs
- src/api/mirrors.rs
- src/db.rs
- src/mirrors.rs
- static/js/pages/newrepo.js
- static/js/pages/mirrors.js
- static/js/pages/repo.js
- static/js/repohead.js
- static/css/app.css
- README.md
- scripts/ui-check.mjs
