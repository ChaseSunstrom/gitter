**Verdict:** approve with nits. No security or staleness defects. One MEDIUM and four LOW findings.

**Cache-key safety (checked, no finding).**
- Keys are `prefix:{repo id}:{full 40-hex sha}:{path}`. The sha comes from `resolve_rev` (`git.rs:309`), which runs `rev-parse ^{commit}`, so a branch name never ends up in a key.
- Repo ids come from `INTEGER PRIMARY KEY AUTOINCREMENT` (`db.rs:45`), so a deleted repo's id is never reused. One repo's entries cannot serve another's.
- The handlers I read (`tree_commits`, `commit`, `compare`, `blame`, `languages`) call `Repo::readable` before any memo lookup. A viewer who has lost access gets denied before the cache is touched.
- `maintain_soon` (`git.rs:654-678`) is race-correct: the flag is re-checked under the lock before removal.

## lens: adversary / edge / maintainer

- MEDIUM — src/api/code.rs:214-242 (also :296-307) — a failed `git log` is cached as a complete empty result. `last_commits` never checks the exit status. If git dies (OOM kill, repo mid-gc), the stream ends, `work` finishes, `complete` is true, and the empty map is stored under `tc:{id}:{sha}:{path}`. That folder then shows no last-commit info for that sha until restart or a 64 MB cache clear. `langs::stats` does the same (`langs.rs:91-93` returns `vec![]` on error, and `languages` caches it under `lang:`). Fix: cache only when `want.is_empty()` or the git exit status is 0, and don't cache an error-derived empty `stats`. — confidence: high on the path, medium on how often it triggers.

- LOW — static/js/diffview.js:245 with src/git.rs:483-495 — Load diff on a renamed file shows the wrong diff. The JS sends only `new_path`, and `git diff -M … -- newpath` can't pair the rename without the old path. A renamed and heavily edited file over 3,000 lines loads as 100% added, while its header still says "renamed" with the old counts. Fix: add an `old` query param and pass both paths as pathspecs. — confidence: high.

- LOW — src/git.rs:493 — the pathspec is not literal (no `--literal-pathspecs`). `GET /commit/{sha}?path=*` returns the whole commit through `parse_one` with a 40k cap per file. The JS takes `x.diff.files[0]`, so a name like `pages/[id].tsx` can load a neighbouring file. Fix: set `GIT_LITERAL_PATHSPECS=1` on this call. — confidence: medium.

- LOW — src/bench.rs:423-441 and 553 — the perf budget only gates warm p95. The first sample is shown as "cold ms" but never compared against a budget. For the memoized pages (tree-commits, blame, languages, repo home), the 40 timed samples are all cache hits. A regression in the commit-graph or the last-commit walk would still pass `perf-budget.sh`. The README does say "warm cache". Fix: give the cold sample its own budget, or clear `st.git.memo` before the first request of each page. — confidence: high.

- LOW — static/js/diffview.js:265 with :208-214 — toggling Split/Unified (or pressing `u`) re-renders every file from row 1. A loaded file you had scrolled through collapses back to 1,000 rows and the scroll position jumps. Fix: remember how many chunks were rendered and restore that many. — confidence: high.

- LOW — src/git.rs:722-724 — memo size accounting drifts. Overwriting a key subtracts only `old.len()`, not the key length. Concurrent identical misses each add `key.len()`. A single entry larger than `MEMO_CAP` also overshoots it. The effect is small: the cache just clears earlier. Fix: subtract `old.len() + key.len()`. — confidence: high.

## Acceptance criteria
- Commit-graph with Bloom filters, and cached last commits: met in code. The `git.rs` test checks for the BIDX chunk. I did not run `cargo test`.
- Single-file diff: met. Test `single_file_diff…` covers it, including the `../` rejection.
- Perf budget: not verifiable here. Cold-path regressions would not fail it (see the bench finding).
- UI renders without errors: not run. `ui-check.mjs` adds a big-diff check.
- README publishes latency and RAM: met textually. The numbers and the GitLab figures cannot be verified from the code.

## Implicit decisions
- Whole-cache clear when full instead of LRU (marked with a ponytail comment).
- The memo serializes to JSON on every put and deserializes on every get.
- `tower` is a production dependency just for the bench.
- The bench seeds repo id 1 and writes to a PID-named directory under the system temp dir.
- Commit-graph runs use `--split` with a 600 s timeout.

## Not checked
- I ran no tests, no `cargo build` and no `ui-check`.
- Pre-audit "debug leftover" hits in `ui-check.mjs` are intentional test output, not leftovers.
- Memory and latency numbers in the README.
- A stale `.lock` file left behind if a 600 s `commit-graph write` is killed (speculative, no reproducer).

**Noticed:** `hooks::after_import` (`hooks.rs:19`) is only called by the bench, although its comment says "an import".
