**Verdict: changes needed.** None of these block the four acceptance scripts. Findings 1, 2 and 3 break the documented Docker workflow. I verified each by reading the code. I did not run Docker.

## lens: adversary — ok
The setup-code gate, archive path checks, suspension filters and last-admin guard held up. A forged archive in `restore` (`..`, absolute path, symlink, hardlink, stray name) is refused before anything is written. The only adversary-adjacent issue is finding 2.

## lens: operator — changes needed
- MEDIUM — `README.md:16,28` — The README tells the user to run `GITTER_URL=https://… docker compose up -d` (README.md:16). Compose reads the variable only from the shell or a `.env` file, and the repo's `.gitignore` has no `.env` entry. The next plain `docker compose up -d`, such as the documented upgrade at README.md:28, recreates the container with the default `http://localhost:3000`. Clone URLs and secure cookies then silently revert. Fix: document `echo GITTER_URL=https://… > .env` and add `.env` to `.gitignore`. Confidence: high.
- MEDIUM — `README.md:28` — The upgrade command `docker compose pull && docker compose up -d` does not upgrade. The service has `build: .` and `image: gitter` (docker-compose.yml:8), so there is no registry image to pull, and `up -d` reuses the existing `gitter` image without rebuilding. Fix: document `git pull && docker compose up -d --build`. Confidence: high.
- MEDIUM — `docker-compose.yml:13-14` and `src/api/account.rs:53` — Compose passes only `GITTER_URL`, but the README recommends Caddy, which needs `GITTER_TRUST_PROXY`. Without it every client appears as the Docker gateway IP. The sign-up limit `signup:{ip}` allows 5 per hour, so it becomes 5 per hour for the whole instance. It also lets one attacker block the setup-link claim. Fix: add `GITTER_TRUST_PROXY: ${GITTER_TRUST_PROXY:-false}` to the compose environment (the lint allows it) and say so in the README. Confidence: high.
- MEDIUM — `src/ops.rs:405` — `owner_only_file` uses `create_new`, so a repeated backup to the same path fails. The suggested command in `static/js/pages/admin.js:36` and the compose header comment use a fixed name, `/data/backup.tar.gz`, so a cron job on that name stops after the first run. Fix: use a timestamped name in those hints, or add `--force`. Confidence: high.
- MEDIUM (PLAUSIBLE) — `README.md:26` — A backup is created with mode 0600. `docker compose cp` into the container creates root-owned files and keeps the mode. The `gitter` user (uid 1000) then can't open `/data/restore.tar.gz`, and the documented restore fails with Permission denied. Fix: have the README tell the user to `chmod 644` the file on the host before copying it in, or restore from a bind mount. Confidence: medium.

## lens: edge — changes needed
- MEDIUM (PLAUSIBLE) — `src/ops.rs:88` — A live backup can capture an inconsistent repository. `append_dir_all` walks directories in arbitrary order while a push is running.
  - If `objects/` is read before `refs/`, the backup can contain a ref whose objects are missing.
  - A temp file that vanishes mid-walk aborts the backup and deletes the output.
  - The README calls backup "safe while running". Fix: archive `HEAD`, `refs` and `packed-refs` first, then the rest, and tolerate files that disappear. Confidence: medium.
- MEDIUM — `src/ops.rs:217,344` — The database snapshot is taken before the repository walk. A repo created in between shows up as an orphan, and `check()` counts orphans as problems (line 344). `restore` has by then already swapped the data into place (line 211), so it returns an error after completing. Fix: treat orphans as a warning in the restore path. Confidence: high.
- LOW — `src/ops.rs:161,211` — `occupied` looks only at `gitter.db`. If `repos/` is non-empty but the database is missing, the database is renamed into place first and then renaming `repos` fails with ENOTEMPTY, leaving a half-restored directory. Fix: also treat a non-empty `repos/` as occupied. Confidence: high.

## lens: intent — changes needed
- LOW — `scripts/e2e.sh:23-36` — The criterion "admin page loads" is checked in `e2e.sh` only through `/api/admin/overview`. The page itself is checked in `ui-check.mjs`, which is not part of the criterion. `compose-lint.sh` only builds and boots the image with `--smoke`, which is not in the criterion either. Fix: add `--smoke` and `ui-check` to the criteria.
- LOW — `src/api/admin.rs:277` — Deleting a user cascades to their issues, PRs and comments in other people's repos (`db.rs` `author_id … ON DELETE CASCADE`). The confirm dialog (admin.js:114) mentions only repositories.
- LOW — The request asked for "backup/restore with verify", but `inspect_archive` runs only inside `restore`. There is no way to verify an archive without restoring it.

## lens: maintainer — ok with nits
- LOW — `src/settings.rs` `Live::load` has no test, although "the UI value wins over the env var after restart" is a headline claim. Add a save-then-load test.
- LOW — `src/ops.rs` builds the same `Config` literal in the reconcile test and in `cfg_for`.

## Not checked
- Docker build and run, and the browser UI.
- `scripts/ui-check.mjs` execution.
- Findings that would need a running Docker daemon.

**Noticed:** none.
