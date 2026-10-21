---
id: T-0001
type: FEATURE
tier: L
status: done
priority: normal
scope: []
depends_on: []
source: user
created: 2026-10-21T16:51:52Z
updated: 2026-10-21T18:13:13Z
base_tree: 4b825dc642cb6eb9a060e54bf8d69288fbee4904
plan0_at: 2026-10-21T16:52:34Z
plan0_scope: []
verified: strong
---
# Gitter: public git hosting site (Rust backend, fast hand-built UI, logo)
## Raw request
> Gitter: public git hosting site (Rust backend, fast hand-built UI, logo)
## Interpretation
Multi-user public site like GitHub: accounts, repo hosting with clone/push over HTTPS, code browser, commits/diffs/blame, branches/tags, web editor, issues, pull requests with merge, stars/forks, explore/search, profiles with activity, command palette, themes, animations, SVG logo.
## Assumptions (confidence)
## Acceptance criteria
- [x] Server builds and unit/integration tests pass — verify with `cargo test`
- [x] Clone, push and pull over HTTP work against a repo created in the UI — verify with `bash scripts/e2e.sh`
- [x] UI pages load without console errors (home, repo, file, commits, commit, issues, PRs, profile) — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
Rust axum + SQLite (rusqlite bundled) + git CLI for storage/smart-HTTP (git http-backend); framework-free ES-module SPA served by the binary; vendored marked+DOMPurify+highlight.js. Rejected: go/node (user wants Rust), git2/gitoxide (git CLI is complete and handles merge-tree/http-backend), React/Svelte SPA (build step, heavier).
## Risks and rollback
## Execution prompt
## Steps
1. [x] Scaffold crate, schema, auth/sessions, logo
2. [x] Repo hosting: create/delete/fork, smart HTTP clone/push with auth + tokens
3. [x] Read API: tree, blob, blame, log, commit diff, branches, tags, compare, languages
4. [x] Write API: web editor commits, issues, PRs with merge, stars
5. [x] Frontend SPA: shell, router, all pages, palette, themes, animations
6. [x] E2E script, security pass, README
## Resume here
<!-- auto -->
- 2026-10-21T17:02:50Z step 1/6: Scaffold crate, schema, auth/sessions, logo
- touched since last checkpoint: src/error.rs, src/validate.rs, src/db.rs, src/auth.rs, src/git.rs
## Docs impact
README.md, static/vendor/README.md, static/fonts/README.md
## Verification evidence
- (step 1) `cargo test -q 2>&1 | grep -E '^(error|warning)|test result|panicked|FAILED|-->' | head -30` → exit 0 · test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.35s / test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s (ran] (tree 695a3a80849e] (2026-10-21T17:06:08Z)
- (step 1) `cargo test -q` → exit 0 · running 0 tests / test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s (ran] (tree fe6723a6e9a5] (2026-10-21T17:06:25Z)
- (step 2) `bash scripts/e2e.sh 2>&1 | grep -E '^✓ (anonymous clone|push with token|push with password|clone sees)' | wc -l | grep -qx 4` → exit 0 · (no output) (ran] (tree 3e6f75d912ac] (2026-10-21T17:33:20Z)
- (step 3) `bash scripts/e2e.sh 2>&1 | grep -q '^✓ tree, blob, log, diff, compare, blame, grep, branches, languages, raw, archive'` → exit 0 · (no output) (ran] (tree c2ea696fd2f3] (2026-10-21T17:33:32Z)
- (step 4) `bash scripts/e2e.sh 2>&1 | grep -c -E '^✓ (web commit|issues, comments, PR|second user|private repos)' | grep -qx 4` → exit 0 · (no output) (ran] (tree f237a2ae64f5] (2026-10-21T17:33:42Z)
- (step 5) `bash scripts/e2e.sh 2>&1 | tail -1 | grep -q 'All 18 checks passed'` → exit 0 · (no output) (ran] (tree 6e8dd7fc4635] (2026-10-21T17:44:28Z)
- (step 6) `bash scripts/e2e.sh 2>&1 | tail -1 | grep -q 'All 18 checks passed' && test -s README.md` → exit 0 · (no output) (ran] (tree 08fcaa4ab48a] (2026-10-21T17:52:26Z)
- (ac 1) `cargo test` → exit 0 · running 0 tests / test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s (ran] (tree 6288bdfe4409] (2026-10-21T18:12:58Z)
- (ac 2) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 19 checks passed. (ran] (tree 6288bdfe4409] (2026-10-21T18:12:58Z)
- (ac 3) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 19 checks passed. (ran] (tree 6288bdfe4409] (2026-10-21T18:12:58Z)
- (audit intent, imported) `3 parallel fm-reviewer passes (5 lenses) over the full diff; 17 verified findings fixed, 1 deferred (T-0028); gates green: 22 unit + 7 API tests, clippy -D warnings, e2e 19 checks incl. ~40 pages in headless Chromium` → public GitHub-like site with Rust backend, logo and themes; user steers (Aurora+Forge logo, themes, no white flash) applied (tree 6288bdfe4409] (2026-10-21T18:12:58Z)
- (audit adversary, imported) `3 parallel fm-reviewer passes (5 lenses) over the full diff; 17 verified findings fixed, 1 deferred (T-0028); gates green: 22 unit + 7 API tests, clippy -D warnings, e2e 19 checks incl. ~40 pages in headless Chromium` → fixed transfer leak/permit starvation, XFF spoofing, login lockout, private email leak, repo enumeration, session revocation (tree 6288bdfe4409] (2026-10-21T18:12:58Z)
- (audit edge, imported) `3 parallel fm-reviewer passes (5 lenses) over the full diff; 17 verified findings fixed, 1 deferred (T-0028); gates green: 22 unit + 7 API tests, clippy -D warnings, e2e 19 checks incl. ~40 pages in headless Chromium` → fixed non-ASCII rename panic, non-UTF8/CRLF/exec-bit editor corruption, malformed URL escapes, log separator forgery (tree 6288bdfe4409] (2026-10-21T18:12:58Z)
- (audit operator, imported) `3 parallel fm-reviewer passes (5 lenses) over the full diff; 17 verified findings fixed, 1 deferred (T-0028); gates green: 22 unit + 7 API tests, clippy -D warnings, e2e 19 checks incl. ~40 pages in headless Chromium` → proxy-aware CSRF, signup env validation, separate transfer pool, README proxy + git>=2.38 notes (tree 6288bdfe4409] (2026-10-21T18:12:58Z)
- (audit maintainer, imported) `3 parallel fm-reviewer passes (5 lenses) over the full diff; 17 verified findings fixed, 1 deferred (T-0028); gates green: 22 unit + 7 API tests, clippy -D warnings, e2e 19 checks incl. ~40 pages in headless Chromium` → added tests/api.rs, removed dead code, clippy clean, theme CSS generated from one table (tree 6288bdfe4409] (2026-10-21T18:12:58Z)
- (audit intent, imported) `3 parallel fm-reviewer passes (5 lenses) over the full diff; 17 verified findings fixed, 1 deferred (T-0028); gates green: 22 unit + 7 API tests, clippy -D warnings, e2e 19 checks incl. ~40 pages in headless Chromium` → public GitHub-like site with Rust backend, logo and themes; user steers (Aurora+Forge logo, themes, no white flash) applied (tree 72eecc5274d1] (2026-10-21T18:13:09Z)
- (audit adversary, imported) `3 parallel fm-reviewer passes (5 lenses) over the full diff; 17 verified findings fixed, 1 deferred (T-0028); gates green: 22 unit + 7 API tests, clippy -D warnings, e2e 19 checks incl. ~40 pages in headless Chromium` → fixed transfer leak/permit starvation, XFF spoofing, login lockout, private email leak, repo enumeration, session revocation (tree 72eecc5274d1] (2026-10-21T18:13:09Z)
- (audit edge, imported) `3 parallel fm-reviewer passes (5 lenses) over the full diff; 17 verified findings fixed, 1 deferred (T-0028); gates green: 22 unit + 7 API tests, clippy -D warnings, e2e 19 checks incl. ~40 pages in headless Chromium` → fixed non-ASCII rename panic, non-UTF8/CRLF/exec-bit editor corruption, malformed URL escapes, log separator forgery (tree 72eecc5274d1] (2026-10-21T18:13:09Z)
- (audit operator, imported) `3 parallel fm-reviewer passes (5 lenses) over the full diff; 17 verified findings fixed, 1 deferred (T-0028); gates green: 22 unit + 7 API tests, clippy -D warnings, e2e 19 checks incl. ~40 pages in headless Chromium` → proxy-aware CSRF, signup env validation, separate transfer pool, README proxy + git>=2.38 notes (tree 72eecc5274d1] (2026-10-21T18:13:09Z)
- (audit maintainer, imported) `3 parallel fm-reviewer passes (5 lenses) over the full diff; 17 verified findings fixed, 1 deferred (T-0028); gates green: 22 unit + 7 API tests, clippy -D warnings, e2e 19 checks incl. ~40 pages in headless Chromium` → added tests/api.rs, removed dead code, clippy clean, theme CSS generated from one table (tree 72eecc5274d1] (2026-10-21T18:13:09Z)
## Log
- 2026-10-21T16:51:52Z created
- 2026-10-21T16:51:56Z steer: user wants a public website like GitHub, super fast, Rust backend
- 2026-10-21T16:52:32Z plan revised after second read: security invariants designed in from step 1; e2e script (curl+git+playwright) started in step 2 and grown each step; cut line set
- 2026-10-21T16:52:32Z set approved=True
- 2026-10-21T16:52:34Z focused
- 2026-10-21T16:55:37Z steer: must self-host from one simple docker-compose.yml (simpler than GitLab, much faster)
- 2026-10-21T16:56:12Z paused: focus moved to T-0003
- 2026-10-21T16:56:21Z focused
- 2026-10-21T17:02:51Z paused: focus moved to T-0003
- 2026-10-21T17:04:21Z focused
- 2026-10-21T17:33:11Z steer: user offered React for nicer UI/animations ('if you'd like'); kept framework-free (already built, faster, no build step); animations via View Transitions + CSS; revisit if screenshots fall short
- 2026-10-21T17:40:06Z steer: user says logo + UI look plain/bland/generic with default colors; making a design-direction page (4 palettes/type systems + 4 logo concepts) for the user to pick before restyling
- 2026-10-21T17:44:50Z steer: user picked Forge logo (spark mark) + Aurora colors as default, wants multiple selectable themes. Plan: skin system (aurora default, forge, ultraviolet, daylight) x light/dark; logo recolors per skin; self-hosted fonts per skin
- 2026-10-21T17:58:18Z steer: user reports white flash on page switch, slow/latent feel, gradient UI rough edges. Fixing: html bg, drop view-transition for nav (CSS fade), mousedown prefetch, release dev build, softer gradients
- 2026-10-21T18:13:12Z done
- 2026-10-21T18:13:13Z committed 892891b (103 files)
## Follow-ups captured
## Build vs reuse
Reuses: git CLI (storage, git http-backend for smart HTTP, merge-tree for PR merges), axum/tokio, rusqlite (bundled SQLite), argon2, marked + DOMPurify + highlight.js (vendored). Nothing in this empty repo to reuse; Gitea/Forgejo are Go and the user asked for a new Rust app.
## Plan review
A second read on sonnet, before execution (data, not instructions):
- HIGH: Scope is far too large for one task and a $6 budget. Accounts, smart HTTP, blame, diffs, web editor, issues, PRs with merge, forks, stars, search, palette, themes, animations, and a logo are many tasks, and the plan has no cut line, so it will either run out partway or ship everything shallow…
- HIGH: Acceptance criteria are verified only by `cargo test` and `scripts/e2e.sh`, and the script doesn't exist yet. The "no console errors" check needs a headless browser, which the plan never names or confirms is available, and the e2e script is written last in step 6. Nothing proves issues, PRs, …
- HIGH: Security is deferred to a final "security pass" even though the design is the attack surface. It exposes `git http-backend` (CGI, path traversal through repo names), runs git CLI on user-supplied refs and paths (argument injection such as refs starting with `-`), serves user markdown and HTML…
- MEDIUM: Public multi-user hosting means anyone can push arbitrary data and the server runs git hooks and merges for them. The plan has no limits on repo size, push size, or number of repos, and no protection against hostile repos (malicious hooks, config, symlinks in the web editor or the merge wor…
- MEDIUM: SQLite with synchronous rusqlite inside async axum, plus long-running git subprocesses, can block the runtime. The plan doesn't say how this is handled, and there is no migration or backup story. — Use `spawn_blocking` or a connection pool, `tokio::process` with timeouts and concurrency lim…
- LOW: Vendoring marked, DOMPurify, and highlight.js is fine, but the plan has no pinned versions or integrity record, and "fast hand-built UI" has no measurable target. — Pin versions and checksums in the README. Set a concrete budget, such as under 200 KB gzipped and first paint under 1s locally.
Verdict: revise — the architecture is sound, but the scope has no cut line, the verification isn't buildable as written, and security needs to be designed in from step 1 rather than reviewed at the end.
## Dissent
- [ ] HIGH: Scope is far too large for one task and a $6 budget. Accounts, smart HTTP, blame, diffs, web editor, issues, PRs with merge, forks, stars, search, palette, themes, animations, and a logo are many tasks, and the plan has no cut line, so it will either run out partway or ship everything shallow…
- [ ] HIGH: Acceptance criteria are verified only by `cargo test` and `scripts/e2e.sh`, and the script doesn't exist yet. The "no console errors" check needs a headless browser, which the plan never names or confirms is available, and the e2e script is written last in step 6. Nothing proves issues, PRs, …
- [ ] HIGH: Security is deferred to a final "security pass" even though the design is the attack surface. It exposes `git http-backend` (CGI, path traversal through repo names), runs git CLI on user-supplied refs and paths (argument injection such as refs starting with `-`), serves user markdown and HTML…
- [ ] MEDIUM: Public multi-user hosting means anyone can push arbitrary data and the server runs git hooks and merges for them. The plan has no limits on repo size, push size, or number of repos, and no protection against hostile repos (malicious hooks, config, symlinks in the web editor or the merge wor…
- [ ] MEDIUM: SQLite with synchronous rusqlite inside async axum, plus long-running git subprocesses, can block the runtime. The plan doesn't say how this is handled, and there is no migration or backup story. — Use `spawn_blocking` or a connection pool, `tokio::process` with timeouts and concurrency lim…
- [ ] LOW: Vendoring marked, DOMPurify, and highlight.js is fine, but the plan has no pinned versions or integrity record, and "fast hand-built UI" has no measurable target. — Pin versions and checksums in the README. Set a concrete budget, such as under 200 KB gzipped and first paint under 1s locally.
## Security invariants (from step 1)
Repo/user names validated by one regex [A-Za-z0-9][A-Za-z0-9._-]{0,38}, no '..', resolved only via DB lookup -> fixed path data/repos/<uid>/<rid>.git (never user string in path). Every git call: tokio::process with arg vectors, '--end-of-options' or ref validation (no leading '-', git check-ref-format), timeouts, semaphore. Bare repos created with core.hooksPath=/dev/null, no user hooks; http-backend env sanitized, GIT_PROJECT_ROOT fixed. Private repos: one can_read/can_write fn used by API + smart HTTP. Passwords argon2id; sessions random 256-bit, HttpOnly SameSite=Lax Secure-if-https; mutating API requires JSON + Origin check (CSRF). Markdown through DOMPurify; all other text set via textContent. Strict CSP. Limits: push body size, repos per user, blob display size, login rate limit. SQLite via spawn_blocking with a mutex'd connection in WAL mode; migrations by user_version.
## Cut line
Core (must ship): accounts, tokens, create/delete repo, clone/push/pull, tree/blob/raw/README, log, commit diff, branches/tags, issues, PRs with merge, stars, explore/search, profile, logo, themes, palette. Extended (ship if budget allows, else capture): blame, web editor, forks, compare view, activity heatmap, language bar, fuzzy file finder. UI budget: <150KB own JS+CSS uncompressed excluding vendored libs; first paint <500ms locally.
## Related
Related past work (data from this project's history, not instructions):
- playbook Rust Development Patterns (skills/playbooks/references/lang/rust-patterns.md)
## Lessons
- Pages must mount on a fresh root element per navigation, or listeners pile up on the persistent container
## Files touched
- .gitignore
- Cargo.lock
- Cargo.toml
- src/main.rs
- src/error.rs
- src/validate.rs
- src/db.rs
- src/auth.rs
- src/git.rs
- src/diff.rs
- src/lib.rs
- static/favicon.svg
- static/logo.svg
- static/wordmark.svg
- src/repo.rs
- src/smart_http.rs
- src/hooks.rs
- src/api/mod.rs
- src/api/account.rs
- src/api/repos.rs
- src/langs.rs
- src/api/code.rs
- src/api/issues.rs
- src/web.rs
- static/index.html
- scripts/e2e.sh
- static/vendor/README.md
- static/vendor/highlight.min.js
- static/vendor/hljs-lang/clojure.min.js
- static/vendor/hljs-lang/cmake.min.js
