**Verdict: changes needed.** Nothing was executed (read-only), so every finding comes from reading the code. The first one is the most serious.

## lens: adversary
- HIGH — src/smart_http.rs:149-175 — On client disconnect, `tx.send` fails and the loop breaks. `stdout` is still owned by the task, so git blocks on the full pipe and `child.wait()` at :170 never returns. The permit and process leak. Repro: start a clone of a repo with a pack over about 1 MB and Ctrl-C it. After 64 aborts, every `permit()` call hangs. — `drop(stdout)` and `child.start_kill()` on early exit, and wrap `wait` in a timeout — high
- MEDIUM — src/smart_http.rs:159, src/api/code.rs:262-270 — A client that connects and never reads holds a permit. In smart_http the `send` has no timeout, so the hold is unbounded. In archive it is capped at 600 s. Anonymous slow readers of public repos can exhaust all 64 permits. — add a send/write timeout and a per-IP concurrency cap — medium
- MEDIUM — src/api/mod.rs:76-83 — With `trust_proxy` on, the code takes the leftmost `X-Forwarded-For` entry, which the client controls. A different value per request bypasses the signup limit (5/h) and the login-ip limit. — use the rightmost entry or a hop count — high
- MEDIUM — src/auth.rs:44 with src/api/code.rs:576 — Web commits embed the user's private account email as the author email. It is public through `author_email` in the API and through clones, while settings.js:19 and auth.js:63 promise it is private or "never shown publicly". — default to the noreply address, or add a separate public email field — high
- LOW — src/smart_http.rs:65-70 — An anonymous fetch of a private repo gets 401, but a nonexistent repo gets 404, so private repo names can be enumerated. This contradicts README.md:55, and scripts/e2e.sh:150 locks the behaviour in. — return 401 for both — high
- LOW — src/main.rs:17 — Only the exact string `closed` closes signups. `Closed`, `off` and `false` leave them open without any warning. — reject unknown values at startup — high
- LOW — src/api/account.rs:209-222 — Changing the password does not delete other sessions or tokens, so a stolen session survives for 30 days. — delete the user's sessions on change — high
- LOW — src/git.rs:461,477 — `\x1e` and `\x1f` in a commit body inject forged commit records into `parse_log`. — use NUL separators (`-z`) — medium
- LOW — src/auth.rs:254-267 — `retain` only removes expired entries. Spoofed keys (`basic:{name}`) grow the map without bound, and every request above 50k entries scans it under the mutex. — cap the map size or evict the oldest entries — medium
- LOW — src/api/account.rs:172-201 — `update_me` does not check email uniqueness. Setting someone else's email makes `query_row` in auth.rs:187 ambiguous and can lock that person out of email login. — enforce a unique index — medium

## lens: edge
- MEDIUM — src/diff.rs:106-111 — `&rest[..half]` can slice mid-character. Repro: `git mv éé ab`. The header is `a/éé b/ab` (11 bytes), so `half` is 5, inside the second `é`. The commit, compare and PR-diff handlers panic and the connection resets. — use `rest.get(..half)` or `is_char_boundary` — high
- MEDIUM — src/api/code.rs:572 — Web edits always write mode 100644, which strips `+x` and turns symlinks into regular files. — preserve the existing mode from `ls-tree` — high
- MEDIUM — src/api/code.rs:190, static/js/pages/editor.js:155 — The editor loads the file with `from_utf8_lossy`, and textarea values normalise CRLF to LF. Saving rewrites Latin-1 bytes as U+FFFD and converts every CRLF line ending. — refuse to edit non-UTF-8 or CRLF files, or round-trip the original bytes — medium
- LOW — src/web.rs:109-119 — The CSRF check compares Origin with the `Host` header. Behind a proxy that rewrites Host (nginx default), every write by a cookie user returns 403. — also accept the host from `cfg.url` — medium
- LOW — static/js/main.js:46,86 — `decodeURIComponent` throws outside the `try`. Visiting `/%` leaves a blank page. — wrap `ctxFor` in the `try` — high
- LOW — src/diff.rs:120 — `parse_range("")` panics if the 8 MiB cut lands right after `@@ -1,2 `. — guard the slice — plausible

## Acceptance criteria
No criteria were in the brief. Against README.md:55, the claim that private repos answer "not found" over git is not met.

## Implicit decisions
- Admins write to every repo.
- The first account becomes admin even when signups are closed.
- Tokens have full scope.
- Sessions last 30 days with no sliding expiry.
- Commits fall back to the noreply email only when no account email exists.

## Not checked
- Vendored files, fonts and CSS.
- JS pages: commit, commits, branches, labels, newissue, newrepo, explore and theme.js.
- Whether `git http-backend` takes the first or last `service=` parameter.
- The pre-audit scope-creep list; the secret flags are test credentials.

**Noticed:** Pre-audit "secret" hits (scripts/e2e.sh, ui-check.mjs) are test credentials, so dismiss them. There is no test for aborted clones or non-ASCII renames. Forks have no disk quota or rate limit (src/api/repos.rs:265).
