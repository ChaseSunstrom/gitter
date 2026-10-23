**Verdict: changes needed.** Two gaps fall inside the acceptance criteria: the "no outgoing request" rule is not enforced on every path, and the budget is not enforced end to end. I read the code but did not run anything, so each reproducer below is by code path.

I dismissed the pre-audit items. The `ui-check.mjs` console.log is an existing line with only the step count changed. `tests/api.rs`, `registry.rs` and `packages.js` hold T-0073 hunks from the stale diff base; they are not in the working tree.

## lens: intent
- MEDIUM — src/api/federation.rs:82-93 — The daily budget covers only pull-request fetches (`incoming`, `update`, `refetch`). The fork import, which is the main thing a listed server brings in, goes through a mirror (src/api/federation.rs:301) and is never charged or stopped. The README and the admin UI say "one server's fetches stop for the day". Reproducer: after `charge(s, today, DAILY)`, `POST .../federation/fork` still imports from `s`. Fix: charge the import to the budget and refuse it past the limit, or reword the README, the UI hint and the AC to "pull-request fetches". — high
- LOW — static/js/pages/admin.js:379 — `mb()` rounds to whole MB, so a 300 KB fetch shows "0 MB today" next to a nonzero count. Fix: show KB below 1 MB. — high

## lens: adversary
- HIGH — src/api/federation.rs:560-573, 551-557, 82-93 — `reopened` → `refetch` → `fetch_from` has no `listed` check. Reproducer: server B has a pull request open here; add B to the block list; close the pull request and reopen it (`refresh_fork_head`, src/api/issues.rs:186-187). This server then runs `git fetch` against B's fork. That is an outgoing request to a blocked server, and the brief says the check must come "before any outgoing request". Fix: call `listed(st, server)?` first in `fetch_from`. That covers every caller, including `incoming` and `update`, and `reopened` already logs the error. — high
- MEDIUM — src/api/federation.rs:301 with src/mirrors.rs:195-251 — A fork's one-time import is a queued mirror that is retried with backoff until it succeeds. Block the server after `fork` but before the import lands (or while it keeps failing) and the mirror still fetches from the blocked server. Fix: in `claim`/`sync`, skip and delete a pull mirror whose `remote_forks.server` is not `federates_with`. — medium
- MEDIUM — src/settings.rs:366 with src/federation.rs:58-71 — The block list is evaded by spelling alone. `origin()` keeps `:443` and `:80`. A blocked server run at `GITTER_URL=https://spam.example:443` signs as `https://spam.example:443`. It passes `identity` (`says == origin`) and `host` gives `spam.example:443`, which does not match the entry `spam.example`. The unit test even asserts "the port is part of the name". Fix: drop default ports (and a trailing dot) in `origin()`. Also let a block entry without a port cover every port of that host. — medium
- MEDIUM — src/settings.rs:349 and :366 — An entry such as `https://spam.example/alice/notes` (a pasted repository URL) passes validation, but `host()` keeps the path, so it never matches. A block silently does nothing. In an allow list the same typo locks out every server. Fix: reject, or cut at the first `/`, in `validate`. — high
- LOW — src/api/federation.rs:182 — `identity()` makes a GET to a blocked host before `listed` runs, so a blocked host still receives requests from here. Anyone can trigger that by sending `server=<blocked>`. Fix: if answering before the signature check is acceptable, return the same generic 401 for a listed-out server without fetching. — medium
- LOW — src/api/federation.rs:61,72-79 — `FETCHED` never drops old days or servers. Entries are created per verified server, and a server can run many ports. Fix: `retain` the current day inside `charge`. — high
- LOW — src/api/federation.rs:84-91 — Check-then-charge is not atomic, so concurrent fetches all pass the check. Growth is measured as the difference in the size of the repository's `objects` directory, so a local push or another server's fetch into the same repository during the window is charged to this server. Fix: charge `min(growth, REMOTE_MAX)` and reserve before fetching. — medium

## lens: edge
- LOW — src/api/federation.rs:88-91 — `dir_size` walks the whole `objects` directory twice per fetch, on top of `refresh_size`'s walk. This is blocking work that grows with repository size on every `update`. Fix: charge the bytes the fetch actually wrote, which `exec` already tracks for its cap. — medium
- LOW — src/api/admin.rs:328 — `PUT /admin/settings` replaces the whole `Settings`, with `#[serde(default)]` on the new fields. An older client or script that omits them clears the block list without any error. Fix: keep the old lists when the keys are absent (or use `Option`). — medium

## lens: operator
- MEDIUM — README.md (federation bullet) — It promises "fetches stop for the day after 2 GB". That is false for fork imports and misleading for concurrent fetches (see intent and adversary). Fix: after those fixes, state exactly what is counted. — high

## lens: maintainer
- MEDIUM — tests/federation.rs:482-514 and src/api/federation.rs:622-633 — No test fails if the `spent >= DAILY` refusal in `fetch_from` is deleted. The unit test checks only the counter functions, and the integration test checks only `fetched_today > 0`. The second AC ("fetches stop past its budget") is therefore unverified. Also uncovered: `pushed()` skipping a blocked server, and `reopened`. Fix: make `DAILY` overridable in tests (or add a `charge` hook), then assert a 429 from `/api/federation/pulls/update` after charging the budget. Add a reopen test for a blocked server. — high

## Per-lens verdicts
- intent: changes needed
- adversary: changes needed
- edge: approve with nits
- operator: changes needed
- maintainer: changes needed

## Acceptance criteria
- AC1 (refused both ways, 403): met for signed requests, `look_up`, `propose` and `pushed`. Not met for `reopened`, and not met for queued fork mirrors.
- AC2 (admin page lists servers and today's fetches; budget stops fetches): the list is met (`servers()`, admin.js). The budget is only partly met (pull-request fetches only, not fork imports) and has no test of the refusal.

## Implicit decisions
- Lists match by host or host:port, case-insensitively, and ignore the scheme.
- The budget lives in memory and resets on restart.
- Growth of the `objects` directory is the size measure.
- The budget is checked after the signature, and rows appear only for servers with pull requests or forks.
- Lists are capped at 500 entries and 255 characters each.

## Not checked
I did not run the tests or the build, and I did not read the `html` helper's escaping. I did not read the rest of `mirrors::exec`.

Noticed: `src/api/federation.rs:143` — `identity()` counts the shared 120/min lookup limit before the cache check, so first-seen servers compete; this is worth its own look.
