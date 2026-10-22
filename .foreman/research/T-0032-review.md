**Verdict: changes needed.** The SSRF core holds up and the checks are correct for the cases I traced. The blocker is the "owner/writer" gate, which is unsafe once main's write collaborators land. I couldn't find the ureq source, so I didn't confirm its resolver and redirect internals. I relied on the tests that exercise them (127.0.0.1, `localhost`, and 302 not followed).

## lens: adversary
- HIGH — src/api/webhooks.rs:401,420,462,496,506,530,544,571 (also static/js/pages/webhook.js:1450 and reposettings.js:1375) — every hook handler uses `Repo::writable`, and the UI checks `meta.can_write`. At this base that means owner or site admin. After merging main, any write collaborator can read hook URLs (which often carry tokens, as Slack/Discord URLs do) and delivery bodies. They can also PATCH the URL to their own server (the secret is kept) and receive every future payload — fix: switch all 8 handlers to `Repo::administrable` and the UI checks to `meta.can_admin` — high
- MEDIUM — src/net.rs:810 — the refusal message includes the resolved IP, and `HostNotFound` is reported separately. Any account that can add a hook can read both back in the delivery log (`error`). That lets them map the server's internal DNS (`db.corp` gives "10.0.0.5 is a private…") — fix: use one generic "address not allowed or not found" message and keep the IP out of it — high
- LOW — src/api/admin.rs:243 / webhooks.rs:1213 — `allow_private` survives when the owner is demoted or suspended, and `claim` never checks `users.suspended` or `repos.hidden`. A demoted admin's hook still reaches localhost and 169.254.x, and a suspended account's hooks keep firing — fix: clear the flag on demotion and skip suspended/hidden owners in `claim` — medium

## lens: edge
- MEDIUM — src/webhooks.rs:1304 — `join_all` over 8 deliveries makes every batch wait for its slowest one (10 s). One user with 20 hooks pointed at a stalling server, each with up to 100 queued, delays every other tenant's deliveries by minutes, repeating on each retry — fix: use a semaphore-bounded stream with a per-hook concurrency of 1 — high
- LOW — src/webhooks.rs:988 — events dropped at `MAX_WAITING` are logged only to stderr and leave no row in the delivery log, so the hook's owner never sees them — fix: insert a `failed` row, or show a "dropped" count in the UI — high
- LOW — src/webhooks.rs:1121 — payloads are only loosely bounded (issue/comment bodies up to 64 KB each, 20 commits × 8 KB, copied per hook). 20 hooks × 150 rows can reach hundreds of MB per repo, and this storage isn't in the quota — fix: truncate bodies in payloads to about 16 KB — medium

## lens: maintainer
- HIGH (merge blocker) — src/db.rs:603 — this branch adds a "v10" migration, but main already has v10 (organizations). `MIGRATIONS` is positional, so the second merge must renumber to v11. Otherwise one side's schema is skipped — fix: rename it to v11 and append it after main's entry — high
- LOW — README.md:19 — the documented `repository` object omits `description`, which webhooks.rs:1023 sends — fix: add it to the table — high

## lens: intent / operator
Findings: none beyond those above. There is no instance-wide off switch and no admin view of queue depth. A restart mid-send uses up one retry attempt. These are acceptable.

## Acceptance criteria
- AC1 (hooks created, signed, delivered, retried, logged, private refused; `cargo test -q --test api webhook`): met by the code and tests. The task's final `ac 1` evidence line is the early failing run (✗), but the later step runs show 3 of 3 passing on tree 48cd.
- AC2 (UI works; `bash scripts/e2e.sh`): not verifiable. No evidence line shows `e2e.sh` or `ui-check.mjs` being run.

## Implicit decisions
- Event queueing happens after the main commit in a separate transaction, so a crash in between loses the event. Only comment deletion is transactional.
- Failures of any kind, including 3xx and 4xx, are retried on the fixed 5-step backoff.
- ureq/rustls with the default resolver for admin-allowed hooks.
- Creating, changing and deleting a hook needs `Sudo`. Ping, redeliver and reads do not.
- Limited tokens, including deploy tokens, are refused on every hook route.

## Not checked
- ureq source.
- Actually running the tests and e2e.
- CI re-run edge: `hooked_at == finished_at` within the same second could drop an event. Unverified.
- Behaviour of merge conflicts in `issues.rs` and `hooks.rs`.

**Noticed:** none
