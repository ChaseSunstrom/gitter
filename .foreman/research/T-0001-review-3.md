**Verdict: changes needed.** I found no HIGH issues, but several MEDIUM defects and one test-coverage gap. I could not run `cargo test`, `cargo check` or the e2e script (read-only), so everything below comes from reading the code.

## lens: maintainer

- MEDIUM — src/web.rs:109-120 (wired at src/lib.rs:69) — The CSRF check compares `Origin` to the `Host` header. Behind a default nginx `proxy_pass`, `Host` is `127.0.0.1:3000` while the browser sends `Origin: https://git.example.com`, so every cookie-authenticated POST, PATCH or DELETE returns 403. Login still works because it has no cookie yet. README never mentions the proxy requirement. — Compare against the host of `cfg.url` (or `X-Forwarded-Host` when `trust_proxy` is set), using `from_fn_with_state`. — 85%
- MEDIUM — src/api/code.rs:190 and static/js/pages/editor.js:155 — `blob` uses `String::from_utf8_lossy` and only flags a file as binary on a NUL byte. A Latin-1 README containing "café" (0xE9) opens in the editor with U+FFFD, and saving writes the replacement bytes back. This corrupts any file that isn't valid UTF-8, on an ordinary edit or a rename. The editor's textarea also turns CRLF into LF, so editing a CRLF file rewrites every line (I'm confident of this from the HTML spec but did not test it). — Return `binary:true` or `editable:false` when `String::from_utf8` fails, and make the editor refuse such files. — 90%
- MEDIUM — src/api/account.rs:178-195 — `update_me` never checks that the new email is unused, though `signup` does (line 68) and the schema has no UNIQUE on email (db.rs:20). If bob sets his email to alice's, `password_login` (auth.rs:187) takes the first matching row, so alice may be unable to log in by email. — Apply the same uniqueness check, or add a unique index on `email` where `email != ''`. — 85%
- MEDIUM — src/api/mod.rs:112 — `CARD_SELECT.fork_of` has no visibility check. bob forks alice's public repo, then alice makes it private, and bob's public fork card on explore, profile, search and stars still shows "Forked from alice/proj". `repos::show` (repos.rs:134) does filter this, and README promises that private repos stay hidden. — Join on `p.is_private = 0`, or apply `VISIBLE`. — 90%
- MEDIUM — static/js/main.js:46, 86 and 145 — `decodeURIComponent` throws `URIError` on a malformed `%`. In `navigate`, `ctxFor` runs outside the `try`, so opening `/alice/hello/blob/main/%E0%A4%A` at boot rejects before any view renders and leaves a blank page. A malformed hash makes `jumpToHash` throw inside `swap` and leaves the progress bar stuck. — Wrap the decodes in try/catch and fall back to the raw string. — 90%
- MEDIUM — src/api/repos.rs:177-182 — `update` runs `set_head` before the DB transaction. `PATCH {"name":"<existing repo>","default_branch":"dev"}` returns 409, but git HEAD now points at `dev` while the DB still says `main`. Clones and the UI disagree until the next push. — Do the DB update first, or fail the name check before touching git. — 90%
- MEDIUM — src/api/issues.rs, account.rs, code.rs, repos.rs — The handlers have no Rust tests, `Db::memory()` is used only by one unit test, and `tempfile` is an unused dev-dependency. `scripts/e2e.sh` is not part of `cargo test`. Nothing exercises edit/delete authorization on comments (issues.rs:478, 493), label write permission, token Bearer auth, squash or ff merges, or the rate limiter. — Add axum `oneshot` tests on `router()` over a memory DB, starting with the authorization cases. — 80%
- LOW — src/api/account.rs:97 — The login limiter is keyed only on the target username (10 per 5 minutes), counts successes, and has no IP component. Ten requests lock a victim out of web login. — Key on username plus IP, or count only failures. — 85%
- LOW — src/auth.rs:258 — Cleanup runs at 50,000 keys and keeps only entries younger than the current caller's window. A 60-second `basic:` call therefore wipes `signup:` entries that should live for an hour. — Store each entry's window. — 85%
- LOW — src/api/mod.rs:80 — With `GITTER_TRUST_PROXY` on, the code takes the leftmost `X-Forwarded-For` value, which a client can set. Rate limits are then bypassable. — Take the rightmost value, or count trusted hops. — 80%
- LOW — src/api/repos.rs:96-99 — If `commit_files` fails after `init_bare`, the repo row and directory stay, and a retry returns "already have a repository". The `init_bare` failure path (line 81) removes the row but not the directory. — Clean up on every error path, as `fork` does. — 80%
- LOW — src/api/repos.rs:300-319 — `branches` runs up to 200 `git rev-list` processes per request (the semaphore allows 64), and static/js/pages/repo.js:267 calls it only to read one sha. — Reuse the `sha` already returned by `blob`, and cap or batch the ahead/behind counts. — 75%
- LOW — static/js/pages/repo.js:262 and 311 — They call `navigator.clipboard.writeText` directly instead of `lib.copy()`. On a non-localhost `http://` deployment `clipboard` is undefined and the button throws. — Use `copy()`. — 85%
- LOW (dead code and duplication) — `load_by_id` (repos.rs:399) is never called. `total` (repo.js:85, 100) and `wide ? '' : ''` (repohead.js:138) do nothing. `colspan` has the ternary `? 2 : 2` (diffview.js:83). The `_user` parameter of `on_push` is unused (hooks.rs:9). The tower-http `set-header` and `limit` features are unused. The `RESERVED` list in auth.js:6 duplicates validate.rs:4 and has already diverged. — Delete or unify. — 95%
- LOW — src/api/repos.rs:113-118 — The `AND ?2 = ?2` clauses exist only to bind an unused parameter. — Use separate queries. — 90%

## Acceptance criteria
The brief lists none. Not verifiable.

## Implicit decisions
- Auth and git: the first account becomes admin. A Bearer token with no matching user gives 401 even when a valid cookie is present. Argon2id default parameters.
- Storage: `receive.maxInputSize` is set once at repo creation, so changing `GITTER_MAX_PUSH_MB` later does not affect existing repos. Single SQLite connection behind a mutex.
- Data and git requirements: `GET` of a PR can write to the DB (auto-marks it merged). `git merge-tree --write-tree` needs git 2.38 or newer, which README does not say.
- Front end: the CSP allows `img-src https:`.

## Not checked
Any build or test run, `static/css/*` (not read), `static/js/icons.js`, `scripts/package*.json`, `Cargo.lock`, and the generated `themes.css` against `gen-themes.py`. The pre-audit "secret" hits are fixtures and a constant dummy hash. The `console.log` hits are CLI output. Both are dismissed.

**Noticed:**
- `refs()` (git.rs:198) and `langs::stats` (langs.rs:83) read unbounded git output on nearly every page view. `files` caps it at 4 MB.
- `merge_pull` (issues.rs:678-697) moves the git ref before the DB write, so a failed DB write leaves a squash-merged PR open.
- The Basic-auth limiter (`basic:{name}`, auth.rs:176) counts successful password pushes and can be exhausted by an attacker for a known username.
