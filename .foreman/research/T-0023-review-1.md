**Verdict:** changes needed. No critical or high findings, but the signed channel can be starved without a valid signature, and fetched objects are not fsck'd. I did not build or run anything.

Pre-audit items I dismissed:
- The "secret" values are test passwords carrying allowlist pragmas.
- The `ui-check.mjs` logging is its normal output.
- `open_pull` was only made `pub(crate)`, not removed.
- The `Cargo.lock` change is a single line that cargo produced.

## lens: adversary
- MEDIUM — src/api/federation.rs:126,98,134 — The per-server rate limit runs before the signature check. The retry after a failed check, and every first-seen server, also draw on one global 120/min lookup budget. — Reproducer: POST `/api/federation/pulls` with `gitter-federation: server="http://b.example", ts="<now>", sig="<any 64 bytes, base64>"` about 2 times a second. Real B requests then get 429, new-server lookups and `fork` (which looks the server up afresh) get 429, and a cached key expiring after 1h makes real B requests fail. — Fix: rate-limit by IP before the check, and count the per-server bucket only after it passes. Spend the global lookup budget only when the server isn't cached. — high
- MEDIUM — src/api/issues.rs:140 (via :125) — The remote branch fetch skips `fetch.fsckObjects`. The mirror pull sets it (mirrors.rs:290, "checked like pushed ones"), so malformed objects from another server are stored and served at `refs/pull/N/head`. — Reproducer: a malicious "server" serves a branch whose tree has a `.git` entry, then open a pull request from it. — Fix: in `fetch_remote_branch`, add `cmd.args(["-c","fetch.fsckObjects=true"])` before `copy_branch`, and `--no-auto-gc` too. — high
- MEDIUM — src/api/federation.rs:35,423, src/api/issues.rs:131, src/repo.rs:322 — Anyone running a server can fill disk and spam an admin's public repo. Servers are unlimited (one host on many ports), the open cap is per server per repository, and `REMOTE_MAX` is per fetch, not cumulative. Site-admin-owned repos have no quota. Each `update` can fetch up to 256 MB, and each new pull request notifies and emails watchers. There is no allow or block list for servers. — Fix: add a server allow/block list, a total open cap per repository, and a cumulative size cap. — medium
- MEDIUM — src/api/federation.rs:426 — `remote_person` inserts the user row before the branch is fetched. — Reproducer: a signed request with distinct `author.username` values and a nonexistent branch gives one new `users` row each (120/min), and they show up in user search (account.rs:1095). — Fix: create the person only after the head is fetched, or inside the `open_pull` transaction. — high
- LOW — src/api/federation.rs:384-388, src/federation.rs:69 — `host()` drops the scheme, so `http://x.example` and `https://x.example` both map to `u@x.example`. On a conflict the `SELECT … remote IS NOT NULL` returns the other server's row. — Fix: select with `AND remote = ?profile`, otherwise 409. — medium
- LOW — src/api/federation.rs:82,132 — With `federation_private` on, unauthenticated callers can use `server=http://10.0.0.5:6379` to probe internal hosts through the echoed fetch error. — Fix: return a generic 401 to unauthenticated callers. — medium
- LOW — src/api/federation.rs:123 — `(t - h.ts).abs()` overflows on an untrusted `ts`. `ts="-9223372036854775808"` panics a build with overflow checks on (such as a debug build). — Fix: use `t.abs_diff(h.ts) > SKEW as u64`. — medium

## lens: edge
- LOW — src/federation.rs:35 — The key is written with `create_new` and then `write_all`, so a crash or a full disk leaves a short file and every later `key()` fails with "isn't a 32-byte key". `security::write_new_secret` (security.rs:260) already writes to a temp file and links it into place. The key is also not in the backup (ops.rs:194,333). — Fix: reuse that helper and add the file to backup and restore. — high
- LOW — src/api/federation.rs:231-236 — If the once-mirror is removed before it finishes, the repo is empty again but its `remote_forks` row remains. A second fork then hits the primary-key conflict and the whole transaction fails. — Fix: delete the old row first, or use `INSERT OR REPLACE`. — plausible
- LOW — src/api/issues.rs:149 — A network, TLS or 401 failure on the fetch becomes `Ok(None)`, so the user sees "source branch doesn't exist" and git's reason is dropped. — Fix: keep git's stderr for non-"Stopped" failures. — medium
- LOW — src/api/federation.rs:445 — `open_from` ignores `base`, so proposing the same branch to a different base silently returns the existing pull request. — Fix: key it on base too, or say so in the reply. — medium
- LOW — src/federation.rs:62 (`message`) — The signed format is tested only by round trip. — Fix: add a golden vector. — high

## Acceptance criteria
No criteria were in the brief. The README claims match the code, and the loopback tests cover the happy path, replay, tampering and staleness, a wrong or unrelated key, the 10-open cap, and federation turned off. Not verifiable from the code: that the e2e and UI scripts pass.

## Implicit decisions
- Default ports are not normalized.
- The replay set is in memory, so it is lost on restart.
- The key and the lookup cache last 1h.
- A pull request is deduplicated by branch.
- The author's name is the remote server's own username plus `@host`.
- The ed25519 key sits in the data folder and is not backed up.

## Not checked
Whether the Playwright, e2e and cargo runs pass; the exact limiter caps; the mirrors delete handler.

**Noticed:** no way for an admin to block a server (a follow-up task if finding 3 is deferred); no test for a second remote person, nor for two scheme-colliding servers.
