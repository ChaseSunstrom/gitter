# Brainstorm — lens: simplicity

- **Cap the compose file at one service and five env vars** — FEATURE — value 5 — effort S — risk low — category: simplicity budget
  Why: user asked for "a single simple docker compose file, more simple than gitlab"; prior ideas add sidecars (runner, ACME, Litestream) that could creep back into complexity. Assumption: a hard budget is the only thing that keeps it simple.
  Done when: a test parses docker-compose.yml and fails if it has more than one service, more than five environment keys, or more than 25 lines; the e2e script boots from that file alone.

- **Derive every setting from a single `GITTER_URL`** — FEATURE — value 5 — effort S — risk low — category: config
  Why: prior ideas include a wizard, settings in UI, HTTPS via ACME, and SSH; each needs a hostname and port. Assumption: one public URL can imply cookie domain, CSRF origin, clone URLs, ACME domain, and CORS.
  Done when: starting with only `GITTER_URL=https://git.example.com` produces correct clone URLs, cookie flags, Origin checks and an HTTPS listener, verified by a test per derived value.

- **Make the server binary the only image, with runner as `gitter runner` subcommand** — CLEAN — value 4 — effort M — risk med — category: packaging
  Why: pack lists "ship one binary that is also the runner" but not the consequence: one image, one tag, one version, so server and runner can never drift. Assumption: the image stays small (static musl, scratch or distroless base).
  Done when: `docker image ls` shows a single gitter image under 50 MB, and the runner starts from `command: gitter runner` with no separate build.

- **Keep all state under one `/data` volume with a manifest** — FEATURE — value 4 — effort S — risk low — category: storage layout
  Why: backup, restore, upgrade and `doctor` all need to know where things are; scattered paths multiply bugs. Assumption: SQLite, repos, logs, artifacts, certs and secrets can all live under one directory.
  Done when: deleting the container and remounting only `/data` restores a fully working instance, and a test checks that no file is written outside `/data`.

- **Default to the simplest CI config: a `.gitter/ci.sh` file** — FEATURE — value 4 — effort S — risk low — category: CI simplicity
  Why: T-0002 asks for pipelines defined in repo; prior ideas say "as minimal as a shell script" but not that it be the zero-config default, with YAML only when you need matrix or multiple jobs. Assumption: most small repos need one script and an image.
  Done when: pushing a repo containing only an executable `.gitter/ci.sh` runs it in the default image and shows a green check on the commit with no other configuration.

- **Fail loudly at boot with a one-line fix for every misconfiguration** — FIX — value 4 — effort S — risk low — category: error messages
  Why: simplicity is lost when a user hits a cryptic failure on a Docker volume permission, port conflict or missing socket. Assumption: top failures are a small enumerable set.
  Done when: each of ten injected misconfigurations (read-only volume, port in use, bad URL, no docker socket, etc.) exits with a message naming the cause and the exact fix, covered by tests.

- **Hide advanced features behind feature flags that default to off** — FEATURE — value 3 — effort M — risk low — category: progressive disclosure
  Why: the idea list contains registry, pages, LFS, SSO, boards, federation; showing all of them in the UI on day one makes it GitLab-shaped. Assumption: a new instance should show repos, issues, PRs and CI only.
  Done when: a fresh instance's nav has at most five entries, and each extra feature appears only after toggling it in settings.

- **Wild: Print a QR code in the first-run log that logs you in as admin on your phone** — FEATURE — value 3 — effort S — risk med — category: first run
  Why: assumption: the first-run admin setup is the biggest friction, and a one-time token in the log is already planned; a QR makes it zero typing. wild
  Done when: the first boot prints a QR encoding a single-use setup URL that expires after 15 minutes and works once.

- **Wild: Ship `gitter` as a self-updating compose file via `docker compose pull` only** — FEATURE — value 4 — effort M — risk med — category: upgrades
  Why: assumption: an upgrade should be exactly one command, with migration and snapshot done on boot, so no upgrade guide is needed. wild
  Done when: changing the image tag and running `docker compose up -d` upgrades across a schema change, with an automatic snapshot and rollback on migration failure, tested from the previous release.

- **Wild: Make the whole admin panel a single page of plain-language toggles with no jargon** — FEATURE — value 3 — effort M — risk low — category: admin UX
  Why: assumption: GitLab's admin area is a major source of its complexity; one page with sentences like "Let anyone sign up" is the opposite. wild
  Done when: every instance setting appears on one scrollable page, each with a one-sentence description, and a test confirms there is no second settings page.

- **Wild: Let the instance run with zero Docker by using the host sandbox it already runs in** — FEATURE — value 3 — effort L — risk high — category: runner
  Why: assumption: the Docker socket is the main source of both complexity and risk; running jobs in a nested user-namespace sandbox inside the one container removes it. wild
  Done when: a compose file with no socket mount runs a CI job isolated by user namespaces, and a test verifies the job cannot see the host or the server's `/data`.

- **Count and publish the "moving parts" number in `gitter doctor`** — CLEAN — value 2 — effort S — risk low — category: simplicity metric
  Why: the pack's simplicity bet is single binary and SQLite, and a measured number keeps it honest. Assumption: parts means processes, volumes, ports and required env vars.
  Done when: `gitter doctor` prints the count and a CI check fails if it grows past the recorded budget.
