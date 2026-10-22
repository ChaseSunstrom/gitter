**Verdict:** changes needed (one MEDIUM on restart behaviour; the rest are nits)

I read the diff (all 1971 lines) and spot-checked `src/mirrors.rs` and `src/imports.rs`. I did not run anything, so every reproducer below is a lead until someone runs it.

## lens: intent

- MEDIUM — src/imports.rs:333 (claim), :34 (LEASE=900) — A restart does not resume promptly. `claim` skips items with `running_until >= now`, and nothing resets leases at boot. Items mid-step when the server dies sit idle for up to 15 minutes, while the README says "a restart carries on where it stopped". The test only simulates a restart by resetting `stage` and `cursor` (tests/import.rs:1898), never a live lease. Reproducer: start an import, kill the server during a step, restart; the item stays idle. Fix: run `UPDATE import_items SET running_until = 0` once before `imports::run` starts, and add a test that leaves a live lease. Confidence: high.
  - file=src/imports.rs, line=333, category=resume-lease, verdict=CONFIRMED
  - short_summary: Restart waits out a 15-minute lease
  - failure_scenario: the server is killed mid-clone with `running_until = now+900`; after restart nothing is claimable for up to 900s.

- LOW — src/imports.rs:399 — Any error, even a transient 502 or timeout on page 40, sets the item to `failed` for good. The only recovery is a re-import, which clones again. Fix: retry a few times with backoff (set `next_at`) for 5xx and network errors before failing. Confidence: medium.

- LOW — src/imports.rs:228 — `list` silently stops at 200 repositories, and `start` only accepts repos from that list. An organization with more than 200 can't be imported past the first 200, and the page doesn't say so. Fix: return a `truncated` flag and show it, or page the picker. Confidence: high.

- LOW — src/imports.rs:606, :744 — If an issue number is already taken locally (for example the owner filed an issue in the half-imported repo), the imported issue is skipped silently. Comments are then attached by number (`SELECT id FROM issues WHERE number`), so they land on the unrelated local issue. Fix: look comments up through `import_map`, and record a skipped item. Confidence: medium.

- LOW — src/imports.rs:906 — The "one import at a time" check runs before the slow `list` call and has no unique index. Two concurrent POSTs can both pass it. Fix: add a partial unique index on `import_jobs(user_id) WHERE state='running'`. Confidence: medium.

## lens: operator

- LOW — src/validate.rs:16 — Adding `import` to `RESERVED` means an existing user or org named `import` is now shadowed by the `/import` route. There is no migration check. Fix: check for such accounts at startup, or note it in the README. Confidence: medium.
- LOW — src/auth.rs:399 — Git basic auth now also accepts the username `oauth2`. This was added so the GitLab-style clone can run against the test's Gitter source. It is undocumented and widens a production auth path. Fix: mention it in the README, or confine it to the test. Confidence: high.
- LOW — src/db.rs:963 — `import_jobs` and `import_items` are never pruned. `claim` joins them on every 2-second poll. The growth is small. Fix: delete finished jobs after N days in the sweep. Confidence: low.

## Pre-audit items

- **`[REDACTED]` markers:** these are in the diff file only. The real source has `token = [REDACTED] (src/imports.rs:406, 434, 985). Dismissed.
- **`console.log` in `scripts/ui-check.mjs`:** this matches the script's existing reporting style. Dismissed.
- **`clean` and `pull` "removed":** neither was removed. `clean` became `pub` and `pull` gained an `extra` parameter. The only other caller is `mirrors.rs:262`, which is updated. Dismissed.
- **`src/mirrors.rs` vs `src/api/mirrors.rs`:** `api/mirrors.rs` does not call the changed functions, so it needs no edit.

## Acceptance criteria

- **Mock-API import (tests/import.rs):** all parts are covered by tests I read, but not run by me, so not verified:
  - labels, milestones, issues, pull requests, comments and releases;
  - pagination;
  - a 403 rate-limit wait;
  - re-import adds nothing;
  - the token is wiped and never appears in output;
  - private addresses refused.
  - Two gaps: the restart case is simulated rather than real (the MEDIUM above), and GitLab merge-request numbers are offset rather than kept (documented).
- **Import page renders without console errors (`scripts/ui-check.mjs`):** the check is in place. It filters console errors matching "status of 502", which could hide a real 502 from the preview. Not run by me.

## Implicit decisions

- One worker handles all users' imports one step at a time.
- Remote authors are written into the text, and every record is owned by the importer.
- Imported `refs/pull/*` and `refs/merge-requests/*` are advertised to every clone, since `hideRefs` only hides the merge queue.
- Label descriptions are dropped.
- Existing default labels keep their own colour.
- GitLab "internal" visibility becomes private.
- Plain http is allowed only with `allow_private`, which only site admins can set.

## Not checked

The `html` helper's escaping, rustfmt and clippy, the real `scripts/e2e.sh`, and the `token.trim()` handling (it sits behind a redaction marker in the diff).

**Noticed:** none
