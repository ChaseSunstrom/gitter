**Verdict:** approve with nits. The rollback logic in `merge_pull` looks correct. I found one real regression in the `refs()` change.

## lens: adversary / edge
- MEDIUM — `/home/chase/projects/gitter/src/git.rs:462-473` (reached via `refs()` at git.rs:292) — `run_capped` discards the exit status and sends stderr to null. Before this change, `refs()` used `ok()`, which turned a non-zero exit into a 500. Now a failing `for-each-ref` returns `Ok(vec![])`. Fix: make `run_records` fail on a non-zero exit when `cut` is false. Capture `child.wait().await` in `run_capped` and return `AppError::internal` on a non-zero code. Don't check the code after a deliberate `kill`. Confidence: high on the code path, not run.
  - Scenario: a repo with a corrupt `packed-refs`, or a git process killed under load, makes `for-each-ref` exit 128. `api/code.rs:87-95` (`tree`) then sees "no branches" and returns `{"empty": true}`, so the repo looks empty. `api/code.rs:809-815` (`has_branches`) is false in the same situation, and `split_rev_path` (code.rs:345) 404s every branch path.
  - `langs::stats` is unaffected, because it already swallowed errors by returning `vec![]`.

## lens: maintainer
- LOW — `/home/chase/projects/gitter/src/api/issues.rs:879-906` — `update_ref` (879) and `db.call` (906) are two separate awaits. If the client disconnects, axum drops the handler future, and the base branch has moved with the PR still open. The rollback only runs on a returned `Err`, so a dropped future skips it. The fix is to run the move-and-record step in `tokio::spawn`, or to record first and move the ref second. Confidence: plausible. I haven't reproduced it, so treat it as a lead.
- LOW — `/home/chase/projects/gitter/tests/api.rs:132-172` — the "rollback itself fails because a push landed" branch (`issues.rs:911-913`) is untested. That branch leaves the PR open with its commits already in base, and the only trace is an `eprintln!`. Fix: test it, or accept it explicitly.
- LOW — `/home/chase/projects/gitter/src/git.rs:292` — the 8 MB cap on `refs()` sorts newest-first, so older refs can be dropped. That includes a default branch with an old creator date. `split_rev_path` and `tree` then act on a partial list. The `ponytail:` comment documents this, and it needs more than 8 MB of ref data to trigger.

## Acceptance criteria
- **A merge whose record fails leaves the branch unmoved and the PR open:** met. The test loops over squash, merge and ff, asserts the failure status and that main is unchanged, then checks the issue state is "open". The rollback passes the old SHA with `Some(&new_sha)` as the expected value, so a concurrent push is not undone (`issues.rs:911`).
- **`refs()` and language stats read capped output:** met. `run_records` truncates at the last separator, and `capped_reads_keep_whole_records` checks a mid-record cut. I confirmed `ls-tree -z` is split on NUL and `parse_refs` on lines.
- **"Still works":** `refs()` loses its error behaviour on a git failure, as in the finding above.
- I did not run the tests.

## Implicit decisions
- Caps are 8 MB for refs and 32 MB for ls-tree. Over the cap, the output is truncated silently.
- A rollback failure is logged with `eprintln!` and the original DB error is returned.
- The failing-DB test uses a SQLite `BEFORE UPDATE` trigger on `pulls`.

## Not checked
I did not run cargo, check clippy or fmt, or review the `.foreman/` files and lockfile.

**Noticed:** `run_capped` has no `kill_on_drop` visible, so on the 30s timeout path (git.rs:462-467) the child may be left running. Check whether `command()` sets `kill_on_drop`.
