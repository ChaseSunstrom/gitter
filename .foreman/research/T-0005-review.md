**Verdict: changes needed.** Two HIGH findings: the quota bypass and the fork that undoes a hide.

## lens: adversary

- HIGH — src/api/code.rs:830 — `write_contents` has no `storage_left` check. Only smart_http push and fork enforce the quota. `POST /api/repos/u/r/contents` takes up to 25 MB of base64 per call (cap at line 870). At the default 120 writes/min that adds about 3 GB/min per account, and a 2 GB quota never blocks it. — Call `storage_left` before committing and refuse when the decoded size exceeds it (same for merge/import paths). — high
- HIGH — src/api/repos.rs:401 — The owner of a hidden repo can still read it, so `fork` accepts it. The fork row gets `hidden=0` and copies `src.private` (line 421), so a hidden public repo comes back as a visible public copy. Scenario: hide `spam/x`, owner POSTs `/repos/spam/x/fork {"name":"y"}`, and the content is live again. — Refuse to fork a hidden repo, or inherit `hidden`. — high
- MEDIUM — src/api/account.rs:586 — `events()` only checks repo visibility. `record_event` stores the issue title for "issue", "pull" and "comment" events (issues.rs:250, 551, 764). After an issue is hidden, its title still shows in the anonymous `/api/feed`, in profile events, and in the heatmap (account.rs:644, which also ignores `hidden`). — Hide the events when the issue or comment is hidden: filter on the target issue's `hidden` flag, or delete the events in `resolve`. — high
- MEDIUM — src/lib.rs:115, src/api/code.rs:407 — `/archive` and `/raw` sit outside `rate_limit` and `Slot`. One IP can open 32 slow-reading archive requests and hold every `stream_sem` permit for up to 300 s. Every clone, fetch and push then fails after the 10 s permit wait. — Apply the per-IP `Slot` and the rate limit to these routes. — high
- MEDIUM — src/auth.rs:506 — When the limiter map hits 100k entries and none have expired, it calls `m.clear()`. Rate-limit keys are now `api:{ip}`, `git:{ip}` and `write-ip:{ip}`. Keys are the full IPv6 address, so an attacker with a /64 can rotate addresses and wipe every counter, including login lockouts and signup limits. — Key on the /64 for IPv6, and evict the oldest entries instead of clearing. — medium
- MEDIUM — src/api/repos.rs:183-196 — `show` filters the fork parent only on `!is_private`. A visible fork of a hidden repo still exposes `fork_of` with the hidden parent's name. — Also exclude hidden parents, as `CARD_SELECT` now does. — high
- MEDIUM — src/api/code.rs:363 — Only `r.private` switches off `public, max-age=31536000, immutable`. An owner or admin fetching a hidden public repo's raw SHA URL gets a publicly cacheable response. — Use `r.private || r.hidden`. — medium
- LOW — src/db.rs:90 — `comments`, `issues` and `users` use `INTEGER PRIMARY KEY` without AUTOINCREMENT, and `reports.target_id` has no cleanup. Scenario: a comment with the highest id is reported and then deleted, a new comment takes the same id, and the admin's hide or suspend lands on an unrelated user. — Close a report when its target is deleted. — medium

## lens: edge

- MEDIUM — src/api/repos.rs:407, src/smart_http.rs:195, src/hooks.rs:16 — Quota checks read the stale `size_bytes` sum, and `refresh_size` only runs after a push or fork completes. N concurrent forks, or pushes to different repos from different IPs, all see the same `left`. Migration v5 (db.rs:171) has no backfill, so every existing repo counts as 0 bytes until its next push. — Run a size backfill at startup and reserve the bytes at check time. — high

## Acceptance criteria

- 429 with Retry-After on API writes and git transfers: met (`rate_limits` test). `/archive` and `/raw` are unthrottled.
- Quota blocks pushes: met for git push only. The contents API bypasses it.
- Invite-only signup: met. The `uses` increment is a conditional UPDATE inside the signup transaction, so it can't exceed `max_uses`, and a failed signup rolls it back.
- Reports and moderation end to end: met. Hiding is partly defeated by the fork and event findings above.
- Browser UI: not verifiable from the code.

## Implicit decisions

- The quota is per owner and admins are exempt.
- `hidden` is a separate boolean from `private`; hiding only denies reads, so the owner can still write.
- Rate limits are a fixed window per full IP string.
- Report caps: 20 per hour per user; 30 repo creations per hour.

## Not checked

I did not run the tests, e2e or the UI. I did not review CSS or README text.

XSS in the admin report queue: not vulnerable. The `html` tag escapes all five characters, and the link URLs are built server-side and start with `/`.

**Noticed:** the "Changes per minute" setting says it covers pushes, but `rate_limit` only wraps `/api`. Git pushes are limited only by `TRANSFER_STARTS_PER_MIN` per IP.
