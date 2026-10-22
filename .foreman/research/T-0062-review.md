**Verdict — changes needed.** The logic is sound and I found no HIGH issues. The UI copy is still stale in seven places, and lowering `trash_days` has a data-loss edge that needs a warning.

I had no shell here, so I read the worktree files directly instead of running `git diff`. Nothing was compiled or run. Line numbers refer to the worktree.

## lens: intent
- MEDIUM — static/js/pages/settings.js:80,84; reposettings.js:14,146,285; admin.js:44 — The UI still says "7 days". With `trash_days=30` users are told 7 and the repo survives 30; with 2 they are told 7 and it is purged after 2. Fix: add `trash_days` and `transfer_offer_days` to `instance_info` (src/api/account.rs:1054) and to the `state.instance = {…}` assignment at admin.js:300. That assignment otherwise drops them after an admin saves. Then change the copy:
  - settings.js:80 → "Offers end after ${state.instance.transfer_offer_days} days."
  - settings.js:84 → "stay here for ${state.instance.trash_days} days".
  - reposettings.js:14 → "the offer ends after N days".
  - reposettings.js:146 and :285 → "for N days" / "restore it for N days".
  - admin.js:44 → "Each is deleted for good ${N} days after it was deleted", or just drop it, since each row already shows `purge_at`.
  - Use a plural helper so "1 day" reads correctly.
  - — high
- LOW — src/api/lifecycle.rs:3,451,459,503; src/api/repos.rs:382; src/db.rs:443 — The doc comments and the log line "deleted over a week ago" are stale. Fix: reword to "past the retention window". — high

## lens: adversary
- No finding on settings validation. A missing field takes `default_week` (7), not 0 (settings.rs:49,52). 0, and anything over 365, return 400 at settings.rs:142. A value that doesn't fit `u32` fails deserialisation. The only exposure is that a client omitting the field silently resets a custom value to 7, which is the existing pattern for the other fields.
- No finding on the removed constants. I grepped `src/`, `static/` and `tests/` in the worktree and no `TRASH_SECS` or `OFFER_SECS` remain.

## lens: edge
- MEDIUM — src/api/lifecycle.rs:463-468 and admin.js:176 — Trash expiry is computed at sweep time (`deleted_at < now − days`), so the setting applies to repos already in the trash. If an admin sets 30 to 1, the next hourly sweep irreversibly purges every repo deleted more than a day ago. Transfer offers behave the other way: they store `expires_at` at offer time (lifecycle.rs:163), so changing the setting only affects new offers. Both are defensible. Fix: add a hint on the trash field, "Applies to repositories already deleted; shortening it purges them at the next hourly sweep."
- `can_transfer` is correct for org-owned repos. It requires `can_admin` and then `owns()`, which allows the owner, a site admin or an org member with role `owner`. That is the same predicate as `must_own`, so an org admin or an admin collaborator no longer sees a Transfer button that would 403. Tests cover the owner, a site admin, an admin collaborator and a stranger, but not an org owner.
- LOW — src/api/lifecycle.rs:590-595 — The rate limiter runs before the disk check, so a 507 uses one of the 10 daily export tries. The disk check also runs before the "still being prepared" 409. Fix: move the disk check ahead of the limiter, or accept it.

## lens: operator
- LOW — src/api/lifecycle.rs:595 — `free_space` runs `df` with no timeout, so a hung mount stalls the request. It is already called this way from the admin overview. `used` includes trashed repos, which makes the check conservative, not wrong.

## lens: maintainer
- LOW — src/api/lifecycle.rs:443 — The restore rollback via `trash_sql("")` stamps `deleted_at` with `now()`, so a failed restore resets the retention clock. This is the same as the old behaviour and is fine; a comment would help.

## Acceptance criteria
- Export disk check: met (`check_room`, with a unit test).
- `trash_days` and `transfer_offer_days` settings, default 7, range 1–365: met.
- One `trash_sql` helper: met.
- `can_transfer` in the repo JSON and Transfer hidden otherwise: met.
- `refresh_size` skips trashed repos: met.
- UI copy follows the settings: not met (the first finding above).

## Implicit decisions
- Settings fields use `#[serde(default)]` to 7, not required.
- Trash expiry is computed at sweep time; offer expiry is stored.
- 507 for low disk; the 2× multiplier is hard-coded.
- The `df` helper is reused, and unknown free space lets the export through.

## Not checked
Compilation and test runs, e2e scripts, and the org-owner path at runtime.

**Noticed:**
- `restore` (lifecycle.rs:441) doesn't call `refresh_size`, so a repo whose size was already zeroed by the old race stays at 0 against the quota until the next boot's backfill.
- `backfill_sizes` (hooks.rs:87) still selects trashed repos with `size_bytes = 0`; add `AND deleted_at IS NULL`.
