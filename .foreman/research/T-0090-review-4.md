**Verdict: changes needed.** One HIGH (the comments cap) and a few MEDIUMs. The other fixes (3 token and overflow, 4, 6, 7, 8) hold up.

## lens: adversary + edge

- HIGH — `src/imports.rs:905` (with `save` at 428) — The GitLab comments cap counts steps, not pages.
  - Scenario: `gitlab_notes` returns `Flow::Stay` once per issue or MR, and `save` does `pages + 1` each time. The check at 905 is also unconditional, so it fires even when `p.next` is `None`.
  - Result: a project with about 2000 or more issues plus MRs fails at "comments" with "its comments go on past 2000 pages". Nothing is looping.
  - Fix: reset `pages` when the cursor moves to a new issue, and apply the cap only when `p.next.is_some()`. Both belong in the `Stay` handling in `save`.
  - Confidence: high.

- MEDIUM — `src/imports.rs:579-603` — Re-import jumps to "labels" even when the first import never cloned.
  - Scenario: the first import creates the repo and its `import_map` row, then the clone fails (retries exhausted, out of storage). Re-importing finds the repo, passes the admin check, and skips the clone. The repo stays empty and pulls get NULL SHAs.
  - Fix: jump only when the repo already has refs, for example via `st.git.refs`. Otherwise return `Flow::Next`.
  - Confidence: high.

- MEDIUM — `src/imports.rs:358-359` with `src/mirrors.rs:35,140` — Fairness is per item, not per user or job, and a clone blocks the single worker for up to 1800 s.
  - Scenario: one user's 200-item job against a slow-drip remote costs 200 × 30 min × up to 5 clone tries. Another user's single item then gets a step roughly once per full round.
  - Fix: run clones on a small semaphore-limited pool, or pick the next item by the job's least-recent step. A shorter clone timeout for imports also helps.
  - Confidence: medium.

- MEDIUM — `src/imports.rs:163-168, 434, 441` — A provider that always answers 429 (or 403 with `retry-after`) never fails the item.
  - `Flow::Wait` doesn't touch `tries` or `pages`, and `save` still sets the job's `updated_at`, so the one-day idle sweep never fires. The user's one-running-job slot stays held until they cancel.
  - Alternating A→B→A is bounded by MAX_PAGES. MAX_PAGES × 10 s is acceptable only if a per-job age cap exists.
  - Fix: in `sweep`, also fail running jobs where `created_at < t - 7d` (or a similar cap).
  - Confidence: high.

- LOW — `src/imports.rs:552, 601` — A crash or failed `save` after the `repo_id` UPDATE sends a re-import to `Flow::Next` on re-entry. The clone then runs with `--force --prune` (`mirrors.rs:290`) and overwrites local pushes. Fix: set the target stage in the same UPDATE as `repo_id`. Confidence: medium.

- LOW — `src/imports.rs:359` — `next_at` has one-second resolution, so ties break by the lowest id. A fast item can take every step within a second. Fix: use ms, or add a tiebreak on last-step time. Confidence: medium.

## Checked and fine

- Item 3, retry shift: `tries + 1 < 5` caps the shift at `30 << 3`, so it can't overflow.
- Item 3, token in error text: `get` redacts, and `mirrors::clean` strips the password and the base64 form. `step` doesn't redact `Ok(Retry)`, but both sources are already clean.
- Item 3, clone retries: "out of storage" fails instantly, so five retries over about 7.5 min only delay the message. Acceptable.
- Item 4, `recover()`: safe for one process (`main.rs:231`).
- Item 5, comments via `import_map`: correct for GitHub.
- Item 6, unique index: the constraint is mapped to 409, and the transaction rolls back.
- Item 7, `sweep`: setting `updated_at` first keeps freshly failed jobs out of the delete. Cascade strands nothing, since `import_map` is keyed by repo.
- Item 8, `present`: input is hex-validated, and `cat-file --batch-check` prints only full lowercase SHAs, so abbreviated or uppercase input is treated as missing. That is safe.

Noticed: `src/net.rs:168` — a body cut off by the 10 s global timeout becomes a permanent "wasn't JSON (or was over 8 MB)" failure in `imports.rs:178`, not a retry.
