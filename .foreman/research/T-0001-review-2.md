**Verdict:** changes needed. I read the diff and spot-checked `src/`, `scripts/` and `static/js` for line numbers. I did not run cargo or the e2e script, because this was a read-only pass with no shell.

**Pre-audit dismissals:** the "secret" hits are test passwords and field names. The `console.log` hits are CLI output in scripts or third-party vendor code. The "generated file" hits are lockfiles and vendor files, which were not hand-edited. Unnamed files all serve the one feature.

## lens: intent: changes needed
- MEDIUM — `src/auth.rs:44`, `src/api/code.rs:576`, `src/api/repos.rs:86`, `src/api/issues.rs:653` — Web edits, README creation and PR merges write the user's private email into public commits. This contradicts `auth.js:63` ("Never shown publicly") and `settings.js:19` ("Private"). Repro: sign up with `x@y.z`, edit a file in a public repo, then `git clone && git log --format=%ae` shows it. The noreply fallback never fires because email is required. Fix: default to a noreply address and add an opt-in commit email. — high
- MEDIUM — `src/api/code.rs:190`, `:572`, `static/js/pages/editor.js:155` — The web editor corrupts files silently. `from_utf8_lossy` turns invalid UTF-8 into U+FFFD, and only NUL bytes count as binary. `Change::Write(..., false)` drops the exec bit and symlink mode. A textarea also turns CRLF into LF. Repro: commit `printf 'caf\xe9\n'` or a mode-755 script, open it in `/edit/...`, save, and the diff shows corrupted bytes or "Mode changed". Fix: refuse editing unless the bytes are valid UTF-8, LF-only and mode 100644. — high
- LOW — `scripts/e2e.sh:173-177` — If `playwright-core` is missing, the UI check is skipped and the script still exits 0, so criterion 3 can pass without checking anything. Criterion 2 also differs from the request. The repo is created through the API, not the UI, and "pull" is never run, only a fresh clone (`:206`). Fix: fail or flag loudly when skipped, and add a `git pull` step. — high

## lens: operator: changes needed
- HIGH — `src/lib.rs:52`, `src/smart_http.rs:79,150-159`, `src/api/code.rs:262` — One global 64-permit semaphore covers every git call. `permit()` waits with no timeout, and clone and archive streams hold a permit while the client reads. `tx.send().await` has no timeout, and the archive holds its permit for up to 600s. Repro: signup is open, so push a multi-MB repo, then run 64 `curl --limit-rate 1 /archive/o/r/main.zip` requests. Every tree, blob and commit API call then hangs. Fix: add a write timeout, per-IP caps, and a bounded permit wait that returns 503. — high
- MEDIUM — `src/smart_http.rs:65,70` — Anonymous requests get 401 for a private repo but 404 for a missing one, so private repo names can be enumerated. README line 111 claims "not found" for git. Repro: compare `curl -w %{http_code} $B/alice/secret.git/info/refs?service=git-upload-pack` with the same call on a missing repo. Fix: send the challenge in both cases. — high
- MEDIUM — `src/api/mod.rs:78-80`, `src/api/account.rs:97` — With `GITTER_TRUST_PROXY=true`, the limiter key is the client-controlled leftmost `X-Forwarded-For` value, so random headers bypass the sign-up and login IP limits. With the default `false` behind a proxy, every user shares one bucket, so 30 bad logins lock out everyone. Fix: use the rightmost trusted hop. — high
- MEDIUM — `src/auth.rs:176,187` — The git Basic-auth limiter key `basic:{name}` is case-sensitive, but usernames match case-insensitively. Repro: `Alice`, `aLice` and similar variants each get 10 tries per minute against the same account. There is no per-IP cap, and every attempt runs an Argon2 check. Fix: lowercase the key and add an IP limit. — high
- LOW — `src/api/account.rs:174-179` — `update_me` doesn't check that an email is unique, while signup does (`:69`). Repro: bob sets his email to alice's; email login then uses `query_row` (`auth.rs:187`), which returns one arbitrary matching row. Fix: add a uniqueness check or a unique index. — medium
- LOW — `src/db.rs:43`, `src/api/repos.rs:234` — Repo ids can be reused (no AUTOINCREMENT), and they are on-disk directory names. If `remove_dir_all` fails (only logged), the next repo gets the same id and `git init` reinitializes the old directory with its refs. Fix: use AUTOINCREMENT and fail if the path exists. — low
- LOW — `scripts/ui-check.mjs:7`, `scripts/e2e.sh:173` — The Chromium path is hard-coded to `chromium-1248`, and the README only says `npm install`. On a fresh machine the check crashes instead of skipping. — high
- LOW — `src/main.rs:17` — Only the exact string `closed` closes sign-up; `Closed` silently leaves it open. — high
- LOW — `src/web.rs:109`, README — The CSRF check compares Origin with the Host header, which breaks behind nginx defaults. The README also omits the git version requirement (≥2.38 for `merge-tree --write-tree`, `git.rs`). — medium
- LOW — `Cargo.toml` — `tempfile` and tower-http `limit` and `set-header` are unused (grep found no use). — high

**Implicit decisions:** first account becomes admin and admins read all private repos; binds `0.0.0.0` with open sign-up; `img-src https:` allows remote markdown images; CSRF via Origin/Host; bundled SQLite; Argon2 defaults; 30-day sessions; numeric on-disk repo ids; 64 git processes.

**Not checked:** test results; `DUMMY_HASH` validity; CSS; fonts; vendor files; the pages home, profile, explore, commits, compare, insights, labels and newissue.

**Noticed:**
- `repos.rs:96` returns from `create` without `drop_row` when the README commit fails, leaving an orphan row.
- `CompressionLayer` (`lib.rs:78`) recompresses archives.
- `pull_status` writes to the DB during a GET.
- There is no `tests/` directory despite the "integration tests" criterion.
