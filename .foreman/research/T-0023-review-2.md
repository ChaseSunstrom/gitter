## Verdict
Changes needed. The four acceptance criteria are met on a reading of the code. I found two medium defects (an unauthenticated rate-limit and lookup-budget exhaustion, and remote-person row spam) and four low ones. I could not run anything, so nothing is confirmed by a run.

I dismissed the pre-audit flags:
- The "secret" values are the test password `correct horse`, the same as in the existing e2e and UI checks. The `[REDACTED]` text appears only in the diff file, not the source.
- The `ui-check.mjs` `console.log` lines follow the file's existing ok/✗ reporting.
- The Cargo change pins `ed25519-dalek` 3.0.0, which `Cargo.lock` already lists.

## intent: ok
- LOW — static/js/pages/newrepo.js:149-153 — If the fork step fails (typo, or the other server has federation off), an empty repo stays behind with only a toast. The copy path sends the user to `settings/mirrors` to retry; the fork path has no retry page. — Navigate to a retry spot, or check the other server (`/.well-known/gitter` plus the repo) before creating the repo. — CONFIRMED
  - Repro: choose "fork from another Gitter server", enter `http://x/alice/nope`. `alice/nope` is created and left empty.
- LOW — src/api/issues.rs:157-181 — A reopened remote pull request never catches up with its branch. `may_copy` and `refresh_fork_head` join `repos` on `head_repo_id`, which is NULL for remote pull requests. `update` also refuses closed pull requests, so pushes made while it was closed are lost. — In `refresh_fork_head`, look up `remote_pulls` and call the federation `refetch`. — CONFIRMED
  - Repro: close the pull request on A, push to the branch on B, reopen on A. `head_sha` stays at the old commit until the next push.

## operator: changes needed
- MEDIUM — src/api/federation.rs:126-134 (also :98) — The per-server rate limit is counted for the *claimed* `server` before the signature is checked. A failed check then forces a fresh lookup (`identity(.., true)`) that spends the global `federation-lookups` budget (120/min) and makes an outbound request.
  - Repro, part 1: POST `/api/federation/pulls` 120 times with header `server="https://b.example", ts="<now>", sig="<64 zero bytes in base64>"`. B's real signed requests then get 429, so `pushed` updates are dropped and never retried.
  - Repro, part 2: repeat with a different forged `server` each time. The forks of legitimate users fail with "Too many other servers looked up".
  - Fix: rate-limit unverified calls by IP, count the per-server budget only after the signature verifies, and throttle fresh key fetches per server. — CONFIRMED
- MEDIUM — src/api/federation.rs:426 (`remote_person`) — The `users` row is created before the branch is fetched or the pull request opened. A peer can send valid-looking signed requests with `author.username` = x1, x2, … and a nonexistent branch. Each one returns an error but leaves a `name@host` user behind, up to 120 a minute per peer.
  - Fix: fetch first, then create the person inside the success path (or delete it on failure). — CONFIRMED
- LOW — src/main.rs:14 with src/api/federation.rs:39 — `GITTER_URL` defaults to `http://localhost:3000`. If federation is switched on without it set, `/.well-known/gitter` advertises localhost and peers see "says it's http://localhost:3000". Nothing warns the admin. — Refuse or warn in `Settings::validate` and the admin UI when the URL host is localhost. — CONFIRMED
- LOW — src/ops.rs:194-197 — `backup` includes `secret.key` but not `federation_ed25519`. After a restore the server's identity changes. Peers recover through the fresh refetch, which is one more lookup. — Include the file or document it. — CONFIRMED
- LOW — README.md:10 — The README says the key is "made on first start". `federation::key` creates it lazily, on the first `/.well-known/gitter` hit or the first outbound call. — Fix the wording. — CONFIRMED
- LOW — src/api/federation.rs:300 — `pushed` notifies every `outgoing_pulls` number for a branch. Closed or merged ones get 404 and a log line on every push, and a failed notification is never retried. — Skip closed pull requests, and retry once. — PLAUSIBLE

## Acceptance criteria
1. Ed25519 identity, signing, and checks for time, body, key and replay: met. `signer` (federation.rs:117) and the lib tests cover it. The replay set is in memory, so a restart reopens the 5-minute window.
2. Fork, pull request from bob@B, and push updates it: met, via `tests/federation.rs`.
3. Refusals (federation off, private repo, wrong signer, bad signature; remote people can't sign in or get email): met. Their `pass_hash` is `'!'` and `email` is `''`.
4. New-repository page, the fork page's propose button, and the pull request page showing the remote author and source: met on a reading of the code (`newrepo.js`, `repohead.js`, `issue.js`) and the `ui-check` selectors. Not verifiable without Playwright.

## Implicit decisions
- Any server can federate; there is no allowlist.
- A limit of 10 open pull requests per peer per repository, and a 256 MB cap per fetch.
- Remote usernames are `user@host:port`, and `http` and `https` on the same host map to the same user.
- Federation and private-address access are admin settings that default to off.

## Not checked
I did not compile or run any tests, did not read the playbook checklists (the glob found none), and did not run a full adversary pass.

Noticed: no peer allowlist or blocklist, and no total byte budget per peer (repeated forced `update` fetches of up to 256 MB leave unreachable objects when the owner has no storage quota).
Noticed: the migration index `remote_pulls_source` includes `issue_id`, so it isn't unique despite its name.
Noticed: a repeat `/federation/fork` on a repo that was forked and later emptied hits the `remote_forks` primary key and returns 500.
