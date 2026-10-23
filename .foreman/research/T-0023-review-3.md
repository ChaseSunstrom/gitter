**Verdict: changes needed (maintainer lens).** The structure is sound and reuses `open_pull`, `mirrors::git` and `announce_pull`. I found 4 MEDIUM and 3 LOW defects. The code matches the diff; line numbers are from the working tree.

## lens: maintainer

- MEDIUM — src/api/issues.rs:157-198 — A reopened remote pull request keeps a stale head. `refresh_fork_head` calls `may_copy`, which joins `repos` on `head_repo_id`. That is NULL for remote pull requests, so `may_copy` returns false and nothing is fetched. While the request is closed, `update` returns 404 because `open_from` only matches open requests. — Repro: close #1 on A, commit to the branch on B, reopen #1 on A, then check `pull.head_sha`; it is the old sha. — Fix: in `refresh_fork_head`, look up `remote_pulls` and call `fetch_remote_branch` (make `source` `pub(crate)`). — confidence: high. (category: stale-state, CONFIRMED)
- MEDIUM — src/federation.rs:81 with src/api/federation.rs:139 — A same-second repeat of a request is refused as a replay, so a push can be lost. The message has a one-second `ts` and no nonce, and Ed25519 is deterministic. Two pushes to one branch in the same second send identical `update` bodies, so the second gets "already made" (401). If A fetched before the second push, the pull request stays on the first commit. — Repro: two `contents` commits to `bob/notes` main within one second, then poll A's `head_sha`; the update call for the second push is rejected. — Fix: put a random nonce in the header and the signed message, and key `SEEN` on it. — confidence: medium. (replay-key-collision, PLAUSIBLE)
- MEDIUM — src/api/federation.rs:406-409 and :445-458 — `incoming` ignores `base` when it finds an open pull request. `open_pull` allows one open request per (branch, base), but `open_from` matches on server, fork and branch only. — Repro: propose `feat` into `main`, which gives #1. Then propose `feat` into `dev`. A answers `{number: 1}` and opens no pull request into `dev`. A later `update` for a second request would also 404, because `query_row` returns the first row and the number filter then fails. — Fix: add `base_branch` to `open_from` for `incoming`. — confidence: high. (wrong-match, CONFIRMED)
- MEDIUM — src/api/federation.rs:426 — `remote_person` runs before the fetch and before the pull request is created. Any signing server can create one `x@host` user row per request, up to 120 a minute, and the rows stay even when the fetch or `open_pull` fails. The rows show up in user search (account.rs:1095). — Repro: send signed requests with a nonexistent branch and a new `author.username` each time. — Fix: create the person after the head fetch succeeds, or inside the `open_pull` transaction. — confidence: high. (unbounded-rows, CONFIRMED)
- LOW — src/api/federation.rs:126 and :98 — The rate-limit keys use the claimed `h.server` before any signature check. Forged headers can use up a real server's 120-per-minute budget, or the global `federation-lookups` budget that fork creation and key refresh need. The per-IP limiter in `rate_limit` (api/mod.rs:467-482) limits this but does not stop several IPs. — Fix: limit per client IP before verification, and per server only after it. — confidence: medium. (PLAUSIBLE)
- LOW — src/api/federation.rs:307-324 — `outgoing_pulls` rows are never removed or marked closed. Every push to that branch sends one signed call for each stale row, and each 404s with a log line. After a fork is renamed or transferred, `repo.full_name()` no longer matches A's `remote_pulls.repo`, so updates stop with only a log line. — Fix: delete the row on a 404 reply, and store a fork id or re-send the original fork name. — confidence: high. (CONFIRMED)
- LOW — src/db.rs:1086 — The `UNIQUE` index `remote_pulls_source` includes the primary key `issue_id`, so it enforces nothing. — Fix: make it a plain index, or enforce uniqueness in the query. — confidence: high.
- LOW — tests/federation.rs:1467 and :1560 — `let _ = first;` and `let _ = c;` are leftovers. Nothing tests the following:
  - `propose` rejecting a private fork, a repository that is not a remote fork, or a missing branch.
  - `GET federation/pulls`.
  - The key-rotation refetch.
  - `federation_private=false`.
  - The `remote IS NULL` clause in `password_login` (auth.rs:434). The login test at :1499 passes without it, because `pass_hash` is `'!'`.

  — confidence: high.

## Pre-audit items
- **Dismissed:**
  - The "secret" values are the test password (`correct horse`) in `scripts/ui-check.mjs` and `String::new()` in `src/api/federation.rs:368`.
  - The `console.log` calls in `ui-check.mjs` match the file's existing pattern.
  - `Cargo.lock` only gained the dependency line; `ed25519-dalek` 3.0.0 was already in the tree.
  - The `.foreman` notes that still name `open_pull` are history.
- **Plan gap:** several touched files have no step naming them (UI pages, `e2e.sh`, `auth.rs`, `db.rs`). All of them serve the feature; the plan under-lists files, which is not scope creep.

## Acceptance criteria
No acceptance criteria were in the brief or the file I was pointed to. I did not run the tests.

## Implicit decisions
- The signed message omits the sender's origin.
- Servers are identified by scheme, host and port, but `remote_person` names users by host and port only.
- Trust in a server's key is fetched on first use and cached for an hour.
- Repository owners get one global on/off switch, with no per-server blocklist.
- Any fetch failure is reported as "branch missing".
- A site served under a URL path is silently unsupported (`me()` falls back to the raw URL).

## Not checked
I did not review the security lens beyond the items above, the web UI visuals, or the e2e and ui-check scripts at runtime.

Noticed: repo owners have no way to refuse or block a specific federated server; one can open 10 pull requests, each up to 256 MB, in every public repository.
