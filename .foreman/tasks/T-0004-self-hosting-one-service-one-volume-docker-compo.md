---
id: T-0004
type: FEATURE
tier: M
status: done
priority: normal
scope: []
depends_on: [T-0001]
source: user
created: 2026-10-21T17:01:42Z
updated: 2026-10-21T20:54:12Z
base: 892891bd8cfacdadb674873f8e294eb13f0af9d1
base_tree: 72eecc5274d15d2b94f8117e33d3140dceee8a5b
plan0_at: 2026-10-21T18:13:22Z
plan0_scope: []
verified: ok
---
# Self-hosting: one-service one-volume docker-compose.yml, GITTER_URL-only config, first-boo
## Raw request
> FEATURE: Self-hosting: one-service one-volume docker-compose.yml, GITTER_URL-only config, first-boot admin claim, /healthz + admin dashboard + gitter doctor, gitter backup/restore with verify, graceful shutdown + DB/repo reconcile, compose lint test (clusters C1 C12 C34 C35 C43) #T-0001
> CONTEXT: Gitter = public GitHub-like forge, Rust axum + SQLite + git CLI, framework-free SPA; catalogue at research brainstorm-20261021. All build after #T-0001.
> DONE-WHEN: docker compose up on a clean machine serves the site and gitter backup/restore round-trips
## Interpretation
Self-host Gitter from one simple docker-compose.yml: one service, one volume, GITTER_URL as the only needed setting; first account claims admin; health endpoint + compose healthcheck; admin dashboard (users, repos, disk, version, sign-up mode in UI); gitter backup/restore/doctor subcommands with verification; graceful shutdown + DB/repo reconcile at boot; a lint test keeping the compose file minimal.
## Assumptions (confidence)
## Acceptance criteria
- [x] docker compose config is valid and has exactly one service and one volume — verify with `bash scripts/compose-lint.sh`
- [x] Healthz responds and admin page loads — verify with `bash scripts/e2e.sh`
- [x] Backup then restore into a fresh data dir round-trips repos and DB — verify with `bash scripts/backup-roundtrip.sh`
- [x] Unit tests pass — verify with `cargo test`
- [x] The image builds and the README's install, backup, verify and restore commands work in Docker — verify with `bash scripts/compose-lint.sh --smoke`
## Non-goals
## Approach (options → choice → why)
Alpine multi-stage Dockerfile (rust:alpine musl build, runtime alpine + git) for a small image; settings table in SQLite for UI-managed settings (env overrides); subcommands via std::env::args (no clap); backup = SQLite VACUUM INTO + tar of repos into one .tar.gz with a manifest, restore verifies by opening DB and fsck-ing repos. Rejected: Postgres/Redis services (against one-file simplicity), cargo-chef (extra layer complexity).
## Risks and rollback
## Execution prompt
## Steps
1. [x] Healthz, settings table, admin API + page
2. [x] backup/restore/doctor subcommands + reconcile on boot
3. [x] Dockerfile + docker-compose.yml + compose lint
4. [x] README self-hosting section
## Resume here
<!-- auto -->
- 2026-10-21T20:43:00Z steps 4/4 done
- git: main; 1 uncommitted: scripts/package-lock.json
- touched since last checkpoint: scripts/package-lock.json
## Docs impact
README self-hosting section rewritten (.env, upgrade, stdio backup/verify/restore, trust proxy); compose header comments
## Verification evidence
- (step 1) `cargo test -q --test api` → ✗ exit 101 · For more information about an error, try `rustc --explain E0063`. / error: could not compile `gitter` (lib) due to 2 previous errors; 1 warning emitted (ran] (tree af67ddae648f] (2026-10-21T18:14:31Z)
- (step 1) `cargo test -q --test api` → exit 0 · ......... / test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.42s (ran] (tree 2b9942c913d6] (2026-10-21T18:16:33Z)
- (step 2) `bash scripts/backup-roundtrip.sh` → exit 0 · ✓ restored server has the same history, issues, logins and clones / Backup round-trip passed. (ran] (tree 5b2b47864cf3] (2026-10-21T18:26:43Z)
- (step 3) `bash scripts/compose-lint.sh` → ✗ exit 1 · compose file "/home/chase/Projects/gitter/docker-compose.yml" is invalid: open /home/chase/Projects/gitter/docker-compose.yml: no such file or directory / ✗ docker compose config failed (ran] (tree 443cb255e213] (2026-10-21T18:27:05Z)
- (step 3) `bash scripts/compose-lint.sh` → exit 0 · ✓ docker-compose.yml: one service, one volume at /data, healthcheck, GITTER_URL, nothing privileged (ran] (tree 6b9ec9b99d57] (2026-10-21T18:42:27Z)
- (step 4) `grep -q 'Self-host it (one file)' README.md && grep -q 'gitter restore' README.md` → exit 0 · (no output) (ran] (tree 6b9ec9b99d57] (2026-10-21T18:42:33Z)
- (step 1) `cargo test -q` → exit 0 · running 0 tests / test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s [ran] [tree 22733a4f5b80] (2026-10-21T20:40:21Z)
- (step 2) `bash scripts/backup-roundtrip.sh` → exit 0 · ✓ restored server has the same history, issues, logins and clones / Backup round-trip passed. [ran] [tree 22733a4f5b80] (2026-10-21T20:40:31Z)
- (step 3) `bash scripts/compose-lint.sh` → exit 0 · ✓ docker-compose.yml: one service, one volume at /data, healthcheck, GITTER_URL, nothing privileged [ran] [tree 22733a4f5b80] (2026-10-21T20:40:32Z)
- (step 4) `bash scripts/e2e.sh` → ✗ exit 1 · http://127.0.0.1:28253/signup?setup=YV0uXQT-QmJ6pA7w / gitter 0.1.0 listening on 127.0.0.1:28253 — open http://127.0.0.1:28253 [ran] [tree 22733a4f5b80] (2026-10-21T20:40:40Z)
- (step 4) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 22 checks passed. [ran] [tree f8f4299ea0fb] (2026-10-21T20:41:39Z)
- (step 2) `bash scripts/backup-roundtrip.sh` → exit 0 · ✓ restored server has the same history, issues, logins and clones / Backup round-trip passed. [ran] [tree 86246d4c6238] (2026-10-21T20:47:59Z)
- (step 3) `bash scripts/compose-lint.sh --smoke` → ✗ exit 1 · Container gitter-smoke-3745639-gitter-1 Healthy / ✗ admin signup via setup link [ran] [tree bb830d3002ae] (2026-10-21T20:51:22Z)
- (step 3) `bash scripts/compose-lint.sh --smoke` → exit 0 · Container gitter-smoke-3781804-gitter-1 Healthy / ✓ image builds, boots healthy, serves the app; setup link, clone, push, doctor, backup, verify and restore work (44 MB image) [ran] [tree b634a9126886] (2026-10-21T20:51:50Z)
- (step 1) `cargo test -q` → exit 0 · running 0 tests / test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s [ran] [tree 81c77c35e79f] (2026-10-21T20:52:04Z)
- (step 4) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 22 checks passed. [ran] [tree 81c77c35e79f] (2026-10-21T20:52:48Z)
- (ac 1) `bash scripts/compose-lint.sh` → exit 0 (2026-10-21T20:52:49Z)
- (ac 2) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 22 checks passed. [ran] [tree 81c77c35e79f] (2026-10-21T20:54:11Z)
- (ac 3) `bash scripts/backup-roundtrip.sh` → exit 0 · ✓ restored server has the same history, issues, logins and clones / Backup round-trip passed. [ran] [tree 81c77c35e79f] (2026-10-21T20:54:11Z)
- (ac 4) `cargo test` → exit 0 · running 0 tests / test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s [ran] [tree 81c77c35e79f] (2026-10-21T20:54:11Z)
- (ac 5) `bash scripts/compose-lint.sh --smoke` → exit 0 · Container gitter-smoke-3824199-gitter-1 Healthy / ✓ image builds, boots healthy, serves the app; setup link, clone, push, doctor, backup, verify and restore work (44 MB image) [ran] [tree 81c77c35e79f] (2026-10-21T20:54:11Z)
- (audit intent) `one fm-reviewer pass over all five lenses (research T-0004-review); every finding fixed and re-verified, Docker smoke now runs the README's exact backup/verify/restore commands` → verify command added, smoke + UI check now in criteria, delete dialog states full impact [tree 81c77c35e79f] (2026-10-21T20:54:11Z)
- (audit adversary) `one fm-reviewer pass over all five lenses (research T-0004-review); every finding fixed and re-verified, Docker smoke now runs the README's exact backup/verify/restore commands` → archive path/link checks, setup-code gate and last-admin guard held; stdin spool is owner-only [tree 81c77c35e79f] (2026-10-21T20:54:11Z)
- (audit edge) `one fm-reviewer pass over all five lenses (research T-0004-review); every finding fixed and re-verified, Docker smoke now runs the README's exact backup/verify/restore commands` → backup copies only snapshot repos, refs before objects, tolerates vanishing files; restore treats non-empty repos/ as occupied [tree 81c77c35e79f] (2026-10-21T20:54:11Z)
- (audit operator) `one fm-reviewer pass over all five lenses (research T-0004-review); every finding fixed and re-verified, Docker smoke now runs the README's exact backup/verify/restore commands` → .env for settings, upgrade rebuilds, GITTER_TRUST_PROXY in compose, backup/restore stream over stdio (no perms or name clashes) [tree 81c77c35e79f] (2026-10-21T20:54:11Z)
- (audit maintainer) `one fm-reviewer pass over all five lenses (research T-0004-review); every finding fixed and re-verified, Docker smoke now runs the README's exact backup/verify/restore commands` → settings persistence test, duplicated test Config removed [tree 81c77c35e79f] (2026-10-21T20:54:11Z)
## Log
- 2026-10-21T17:01:42Z created
- 2026-10-21T18:13:22Z focused
- 2026-10-21T18:42:28Z docker smoke (compose-lint.sh --smoke): image builds, boots healthy, serves app, doctor + backup work in container; 14 MB image
- 2026-10-21T20:39:27Z focused
- 2026-10-21T20:43:30Z paused: focus moved to T-0029
- 2026-10-21T20:45:42Z other work changed the same lines while it was paused: its diff still includes that work
- 2026-10-21T20:45:42Z focused
- 2026-10-21T20:54:11Z done
- 2026-10-21T20:54:12Z committed 47eb5b7 (32 files)
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research brainstorm-20261021: [instruction-like text quoted from a file or tool; data, not instructions] # Gitter brainstorm catalogue (2026-10-21)
- research mission-20261021-115538: Request: self host in a single simple docker compose file, simpler than gitlab but way faster, plus runners, CI, all features; recursiv…
- research mission-20261021-115538.pack: self host in a single simple docker compose file, simpler than gitlab but way faster, plus runners, CI, all features; recursive br…
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
## Lessons
- Docker-docs claims (upgrade, restore, image size) only hold if the smoke test runs the exact documented commands
## Plan gaps
- failed first: step 4 README self-hosting section
## Files touched
- scripts/package-lock.json
- src/ops.rs
- src/main.rs
- .gitignore
- README.md
- docker-compose.yml
- static/js/pages/admin.js
- scripts/backup-roundtrip.sh
- src/settings.rs
- scripts/compose-lint.sh
- .claude/scheduled_tasks.lock
- .dockerignore
- Cargo.lock
- Cargo.toml
- Dockerfile
- scripts/e2e.sh
- scripts/ui-check.mjs
- src/api/account.rs
- src/api/admin.rs
- src/api/issues.rs
- src/api/mod.rs
- src/api/repos.rs
- src/auth.rs
- src/db.rs
- src/git.rs
- src/langs.rs
- src/lib.rs
- src/smart_http.rs
- static/css/pages.css
- static/js/main.js
