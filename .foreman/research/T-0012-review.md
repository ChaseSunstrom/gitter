**Verdict: changes needed.** I found three MEDIUM issues and several LOW ones. I found no path traversal, symlink, XXE, auth or XSS holes.

I read the diff in full and, in the worktree, `cache.rs`, `reports.rs`, `runner.rs`, `http.rs`, `trigger.rs`, `lib.js` and the vendored `tar-0.4.46` source. I did not run any code.

## lens: adversary
- MEDIUM — `/home/chase/projects/gitter/.claude/worktrees/agent-a752748bdff2376bc/src/ci/runner.rs:756` (also `reports.rs:417-447`) — Test report messages and file text bypass the secret masking that the log pump applies (`runner.rs:332-342`). `/ci/runs/N/tests` and `/ci/report` serve them to every reader of a public repo. Scenario: a trusted run with secrets fails an assertion whose message contains `$DATABASE_URL`. The JUnit `<failure>` text goes in unmasked and the log shows `***`. — Fix: mask `l.secrets` in the report text on the runner before packing, or on the server in `store`. — confidence medium-high.
- MEDIUM — `reports.rs:393,417-423` — A report upload can force millions of cases. `FILE_CAP` is 16 MB, 1M elements per file and 64 MB in total, so about 3M `<testcase name="a"/>` entries parse into a `Vec<Case>` of roughly 0.6–1 GB. `store` then runs one `before.query_row` per passed case inside a single `db.call`, which holds the database thread for tens of seconds. Any user who can run CI, including an approved agent, can trigger this. — Fix: cap cases per upload (for example 50k). Fetch the failed names for the repo, sha and job in one query and look them up in a `HashSet`. — confidence medium (arithmetic, not measured).
- LOW — `reports.rs:446-447` — `ci_flaky_tests` takes up to 1000 attacker-named rows per job from `flakyFailure` entries. There is no retention or cap, and the API only shows 100. — Fix: cap the rows per repo, or only mark a case flaky from the server-side comparison. — confidence medium.
- LOW — `cache.rs:189,161` — Read-down and eviction cross trust levels. Agent runs get no secrets, but they can read ADMIN and USER caches written by runs that had secrets, if a cached path holds a credential file. Eviction is least-recently-used across all trust levels, so a low-trust run can flush trusted caches. — Fix: document it, or evict the writer's own trust level first. — confidence low-medium.

## lens: edge
- MEDIUM — `cache.rs:217-223` and `cache.rs:248` — A cache row whose file is missing blocks that key forever. `find` updates `used_at` before it checks the file exists, so the dead row never ages out. `store` then hits `INSERT OR IGNORE` and returns false, so the key is never saved again.
  - Triggers: a restart between the `INSERT` and the `rename`; restoring a backup (the README says backups exclude `ci/` files); and `ci_caches.id` without `AUTOINCREMENT` (`db.rs:782`) reusing ids. The id reuse works like this: eviction or sweep deletes the highest-id row, a new store takes the same id and writes `{id}.tar.gz`, then the delayed `remove_file` (`cache.rs:271-273`, `:297`) deletes the new file.
  - Fix: use `AUTOINCREMENT`; in `find`, delete the row and skip the `used_at` update when the file is missing. — confidence high for the backup and crash cases.
- LOW — `config.rs:699` — `restore_keys` are never interpolated, but `check_key` accepts `${{ matrix.x }}` in them (`cache.rs:68`). `restore-keys: [npm-${{ matrix.node }}-]` expands at runtime to `npm--matrix.node-` and never matches, with no warning. — Fix: interpolate them like `key`, or reject `matrix.` in `restore-keys`. — confidence high.
- LOW — `reports.rs:393` — A malformed or non-UTF-8 JUnit file (a DOCTYPE, ISO-8859-1) is silently dropped by `unwrap_or_default()`. The run shows no tests and the runner still logs "Sent N report file(s)". — Fix: record the parse error in the job log. — confidence high.

## lens: tests
- LOW — `tests/ci_extras.rs` — Nothing exercises `unpack_cache` with `..`, absolute, symlink or hardlink entries. `scope()`'s agent and admin trust derivation is only tested by inserting rows directly. — Fix: add one malicious-tar test and one agent-run lookup test. — confidence high.

Per-area results:
- Cache poisoning: no cross-trust hole. Scope comes from the run row, never from the runner. `ref` is server-side, so a branch cannot spoof the default branch. Keys are restricted to `[A-Za-z0-9._-]` and the storage path uses the integer id.
- Archive extraction: safe. The tar crate's `unpack_in` plus `create_new` (which unlinks an existing symlink) and the Regular/Directory-only filter stop path escapes. Total size and entry counts are capped.
- XML parsing: safe. `<!` is rejected, so no DTD or entity expansion. Depth and element counts are capped, and I found no panics or quadratic paths.
- Upload auth: the job token is checked before any body is read, and uploads are bound to the repo through the job. The 403 test covers it.
- XSS: safe. `html` escapes every interpolation, `data-n` is guarded by `Number.isInteger`, and hrefs are same-site, `encPath`-ed paths.

## Acceptance criteria
`cargo test -q --test ci_extras` and `bash scripts/e2e.sh` were not run by me, so I could not verify them. By reading, both appear met: the tests assert cache restore with default-branch fallback, eviction, JUnit failures, flaky marking and coverage. The e2e and UI scripts cover the run page, PR annotations and Set up CI.

## Implicit decisions
- Trust is taken from the run's actor, not from the code's author.
- Cache entries are immutable per (repo, ref, trust, key).
- A fixed 2 GB per-repo cap and a 7-day unused expiry.
- Coverage is summed across jobs. The JUnit and coverage XML reader is hand-written.
- Flaky means a failure on the same sha and job name, matched across runs.
- `restore-keys` prefixes beat an exact match on the default branch.

## Not checked
Merge-queue code, `issues.rs`, `reposettings.js`, SSH, the CSS, and the pre-audit hits (the e2e token is allowlisted and the `console.log` calls are intentional script output).

**Noticed:** ci_test_results and ci_flaky_tests have no retention or pruning. Artifacts are the only CI data that expires.
