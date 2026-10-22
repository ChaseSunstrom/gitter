# Gitter

Fast, simple git hosting. One Rust binary serves the web app, the API and git over HTTPS, and keeps its state in SQLite plus plain bare repositories.

- **Code:** browse repositories, view files with syntax colors and linkable line ranges, blame, history, search, download archives.
- **Collaboration:** issues, labels, pull requests with server-side merge (merge commit, squash or fast-forward), comments in markdown, stars and forks.
- **Wikis, discussions, snippets:** every repository has a markdown wiki with page history (its own git repository, private when the repo is) and a discussions forum with General, Q&A, Ideas and Announcements categories, marked answers and upvotes. Snippets share a few files, public or secret (only the link finds it), with every edit kept as a revision.
- **Editing:** edit, create, rename, delete and upload files in the browser, committing straight to a branch or to a new branch for a pull request.
- **Speed:** pages prefetch on hover, the UI has no framework, and diffs render lazily, so even huge commits stay smooth.
- **Command palette:** press `Ctrl K` or `/` to jump to any repository, file, branch or action. Press `?` for every shortcut.
- **Account security:** optional two-factor sign-in (any authenticator app, with recovery codes), a list of where you're signed in with one-click sign-out, a password re-check before dangerous actions (deleting a repo, making a private repo public, creating tokens), and a tamper-evident security log for you and for admins (`gitter doctor` verifies it). Code views mark hidden Unicode ("Trojan Source" bidi tricks, zero-width characters) and look-alike letters from other alphabets, with a warning above the file or diff.
- **Safe to run in public:** per-address and per-account rate limits (429 with Retry-After), limits on concurrent git transfers, per-account storage quotas enforced by git itself on push, open / invite-only / closed sign-up with invite links, and a report button on repositories, issues, comments and accounts that feeds an admin queue where moderators dismiss, hide or suspend.
- **CI built in:** put a `.gitter/ci.yml` (or just a `.gitter/ci.sh`) in a repository and every push and pull request runs it, on a runner inside the server or on your own machines with `gitter runner`. Live logs with colors and a fold per step, secrets, artifacts, matrices, schedules, re-runs, and a pass/fail mark on commits and pull requests.
- **Themes:** Aurora (the default), Forge, Ultraviolet and Daylight, each in light and dark. Pick one in Settings → Appearance or from the palette.

## Self-host it (one file)

```sh
git clone <this repository> gitter && cd gitter
echo GITTER_URL=https://git.example.com > .env   # the address people will use
docker compose up -d                             # builds a ~45 MB image the first time
docker compose logs gitter                       # open the one-time setup link it prints
```

That's the whole install: one service, one volume, one setting. Settings live in `.env` so every later `docker compose` command keeps them (`GITTER_PORT` changes the host port if 3000 is taken). The first link creates the admin account. After that, everything else lives in **Site administration** in the web UI: sign-up open/closed, repository and push limits, an announcement banner, and account management.

| Task | Command |
|---|---|
| Back up (safe while running) | `docker compose exec -T gitter gitter backup - > gitter-$(date +%F).tar.gz` |
| Check a backup | `docker compose exec -T gitter gitter verify - < gitter-2026-10-21.tar.gz` |
| Restore | `docker compose stop gitter && docker compose run --rm -T gitter restore - --force < gitter-2026-10-21.tar.gz && docker compose start gitter` (the old data is kept in `/data/before-restore-…`) |
| Health check | `docker compose exec gitter gitter doctor` (add `--fsck` to verify every repository) |
| Upgrade | `git pull && docker compose up -d --build` (data migrates automatically) |

Behind a reverse proxy, point it at port 3000 and add `GITTER_TRUST_PROXY=true` to `.env` next to the public `https://` `GITTER_URL`, so rate limits see each visitor's address instead of the proxy's. Caddy example:

```
git.example.com {
    reverse_proxy localhost:3000
}
```

## How fast is it?

`gitter bench` boots a throwaway instance, seeds a repository with 5,000 commits and 1,000 files, and times the pages people open most (release build, warm cache, through the full HTTP stack; measured on an 8-core Ryzen desktop, so expect a small VPS to be a few times slower):

| Page | p50 | p95 |
|---|---|---|
| Repository home (file list, README, last commit) | 9 ms | 12 ms |
| Last commit for every entry in a folder | 3 ms | 6 ms |
| A file | 11 ms | 15 ms |
| History of one file | 8 ms | 12 ms |
| A commit's diff | 9 ms | 16 ms |
| Blame | 5 ms | 7 ms |
| `git clone` of the whole history over HTTP | 330 ms | |

Memory: **8 MB** after boot and about **32 MB** after the run. For comparison, GitLab's documented baseline for a single node is 16 GB of RAM and 8 vCPU (8 GB at the very least).

What keeps it there: commit-graphs with changed-path Bloom filters refreshed after every push, a cache for answers that can't change for a given commit (last commits, blame, language stats), and a diff view that renders huge files a chunk at a time. Run `gitter bench` on your own box; `scripts/perf-budget.sh` (`gitter bench --check`) fails if any page goes over its budget.

## Run it without Docker

Needs `git` 2.38 or newer (for server-side merges).

```sh
cargo run --release
# open the setup link printed on first start
```

Clone and push work like on any other host:

```sh
git clone http://localhost:3000/alice/project.git
git push   # username + password, or username + an access token from Settings → Access tokens
```

Access tokens can be read-only, read and write, or full access, can be limited to one repository, and can expire. For a CI server or deploy box, make a **deploy token** in the repository's settings: it works for that repository only (read-only unless you allow pushing), with any username:

```sh
git clone https://git.example.com/alice/project.git   # username: anything, password: the deploy token
```

Accounts with two-factor authentication need a token for git over HTTPS; the password alone is refused.

Git LFS works with no setup: `git lfs track '*.psd'`, commit and push as usual. Files are stored once by content under `GITTER_DATA/lfs` (forks share them), each repository can only download the files pushed to it, they count toward the owner's storage quota, the largest file is set in Site administration (2 GB by default), and backups include them. Files no repository uses any more are removed at the next start. The web view shows LFS files as "Stored with Git LFS" with a download link, and raw links serve the real file. File locking and resumable (Range) downloads aren't supported; git-lfs skips locking and restarts an interrupted download.

The same binary has the operator commands: `gitter backup <file>`, `gitter verify <file>`, `gitter restore <file> [--force]`, `gitter doctor [--fsck]`, `gitter bench [--check]` (`-` as the file means stdout/stdin), and `gitter runner` for CI machines.

## CI

Add `.gitter/ci.yml`:

```yaml
on:
  push:
    branches: [main, "release/*"]
  pull_request:
  schedule: ["0 3 * * *"]        # UTC, on the default branch
  manual:                        # a Run button on the CI tab
jobs:
  test:
    image: rust:1                # optional: run the steps in this docker image
    matrix: { toolchain: [stable, beta] }
    env: { RUSTUP_TOOLCHAIN: "${{ matrix.toolchain }}" }
    steps:
      - name: Tests
        run: cargo test
  docs:
    paths: ["docs/**"]           # only when these files change
    steps: [make docs]
    artifacts: ["site/**"]       # downloadable from the run
  deploy:
    needs: [test]
    timeout: 10m
    steps:
      - run: ./deploy.sh "$DEPLOY_KEY"
```

- Without `on:`, pipelines run on pushes, pull requests and the Run button. A plain `.gitter/ci.sh` is a one-job pipeline that runs that script.
- Every step runs with `sh -e` in a fresh checkout of the commit. It gets `CI=true` and `GITTER_REPOSITORY`, `GITTER_SHA`, `GITTER_REF`, `GITTER_BRANCH`, `GITTER_EVENT`, `GITTER_RUN_NUMBER`, plus the job's `env`, `MATRIX_*` and the repository's secrets.
- Secrets live under the repository's **Settings → CI secrets**. They are encrypted at rest with `GITTER_DATA/secret.key`, which backups include, and their values are masked in logs.
- `needs` waits for other jobs and skips when one of them fails. `runs-on: [gpu]` sends a job to runners with those labels. The default `timeout` is 30 minutes.
- The YAML is a documented subset: block maps and lists, `|` and `>` text, quotes, and `[a, b]` / `{a: b}`. Anchors and tags give a clear error.

**Runners.** The server runs up to 2 jobs at once itself, for repositories owned by admins only. A job there runs as the server's own user and can read its data folder, so treat pushing to an admin's repository as server access. To add machines, make a token in **Site administration → CI runners**, then run on the machine. It needs git, and docker for jobs with an `image`:

```sh
GITTER_RUNNER_TOKEN=grn_… gitter runner --url https://git.example.com --labels gpu --jobs 4
```

Jobs are leased with a heartbeat. A job whose runner dies goes back to the queue, and a runner that lost its lease can't report over the new attempt. Stopping a runner (Ctrl-C or SIGTERM) kills its running steps, and their jobs go back to the queue.

**Who can run CI** is set in Site administration:

- off;
- admins' repositories (the default);
- everyone: anyone's jobs go to separate runners, never the built-in one. Use disposable runner machines for this.

Turning CI off stops new runs, re-runs and queued jobs.

**Storage.** Logs and artifacts live under `GITTER_DATA/ci`. Artifacts are deleted 30 days after their job, and files of deleted repositories within the hour. Backups don't include CI logs and artifacts; they do include the key for secrets.

## Webhooks

People who can write to a repository add webhooks under **Settings → Webhooks** (signed in on the web, with a recent password confirmation). Each event is a `POST` of JSON to the payload URL with these headers:

| Header | Value |
|---|---|
| `X-Gitter-Event` | `ping`, `push`, `issues`, `issue_comment`, `pull_request` or `ci_run` |
| `X-Gitter-Delivery` | A UUID for the delivery. Retries and redeliveries keep it, so receivers can skip repeats. |
| `X-Gitter-Signature-256` | `sha256=` and the hex HMAC-SHA256 of the body, keyed with the webhook's secret. Compare it in constant time. |

Every payload has `repository` (`id`, `full_name`, `owner`, `name`, `private`, `default_branch`, `url`, `clone_url`) and `sender` (`id`, `username`, or null for scheduled CI runs). Besides those:

- `ping`: `hook` (`id`, `events`). Sent when the webhook is added, and by its **Send a ping** button.
- `push`: `ref`, `before` and `after` (null when the ref was created or deleted), `created`, `deleted`, `commits` (up to 20 the push added, newest first; a new branch lists only its head) and `head_commit`. Each commit has `id`, `message`, `timestamp`, `author` (`name`, `email`) and `url`. A push that moves more than 20 refs sends events for the first 20.
- `issues` and `pull_request`: `action` (`opened`, `edited`, `closed`, `reopened`, or `merged` for pull requests) and `issue` or `pull_request`: `number`, `title`, `body`, `state`, `user`, `labels`, `created_at`, `updated_at`, `closed_at`, `url`; pull requests add `head`, `base`, `merged` and `merge_commit_sha`.
- `issue_comment`: `action` (`created`, `edited`, `deleted`), `issue` (as above, plus `is_pull_request`) and `comment` (`id`, `body`, `user`, `created_at`, `updated_at`).
- `ci_run`: `action` `completed` and `run`: `number`, `event`, `ref`, `sha`, `pr_number`, `title`, `status` (`success`, `failure`, `cancelled` or `skipped`), `error`, `created_at`, `started_at`, `finished_at`, `url`.

Times are Unix seconds. Any 2xx answer counts as delivered. Anything else, no answer within 10 seconds, or a refused address is tried again after 1 minute, 5 minutes, 30 minutes, 2 hours and 6 hours, then given up. Redirects aren't followed, and only the first 64 KB of an answer is read. The queue is in the database, so retries survive restarts (a restored backup sends what was still waiting). Each webhook's page lists its last 50 deliveries with the request, the response code and the first 2 KB of the answer, and can send any of them again.

Deliveries only go to public internet addresses: the host name is looked up once and the connection goes to the checked address, so loopback, private, link-local, shared (100.64/10), multicast and reserved addresses are refused, DNS rebinding included. A site admin can tick **Allow private addresses** on a webhook to reach the server's own network; it switches off again if someone who isn't an admin changes the URL. The secret is encrypted at rest with `GITTER_DATA/secret.key`.

## Configuration

Everything is optional. The defaults are tuned for a single small server, and the sign-up mode and limits can be changed later in Site administration (the UI value wins over the variable).

| Variable | Default | Meaning |
|---|---|---|
| `GITTER_URL` | `http://localhost:3000` | Public address, used in clone URLs. `https://` also turns on secure cookies. |
| `GITTER_ADDR` | `0.0.0.0:3000` | Address to listen on. |
| `GITTER_DATA` | `./data` | Where the database and the bare git repositories live (`repos/`, `wikis/`, `snippets/`, plus Git LFS files in `lfs/`). Back this folder up, or use `gitter backup`, which includes all of them. |
| `GITTER_SIGNUP` | `open` | `invite` needs an invite link from an admin; `closed` stops new sign-ups. The first (admin) account is always created through the setup link. |
| `GITTER_MAX_PUSH_MB` | `500` | Largest single push. |
| `GITTER_MAX_REPOS` | `200` | Repositories per user (admins are exempt). |
| `GITTER_TRUST_PROXY` | `false` | Behind a reverse proxy, set this so rate limits use the client address from `X-Forwarded-For` (the last hop, which your proxy adds). |

Behind nginx or Caddy, set `GITTER_URL` to the public address. Requests whose `Origin` matches it are accepted even when the proxy rewrites `Host`.

## How it works

- **`src/`** is the server, built on axum and tokio.
  - It stores accounts, issues and pull requests in SQLite (WAL mode, run on the blocking pool).
  - It reads and writes code by calling the `git` CLI with argument vectors, a scrubbed environment, no hooks, timeouts and a concurrency cap.
  - Clone, fetch and push go through `git http-backend`.
  - Web edits and merges use plumbing commands (`read-tree`, `merge-tree --write-tree`, `commit-tree`, `update-ref` with an expected old value), so no working copies are needed and concurrent changes are detected.
- **`static/`** is the web app. It is plain ES modules embedded into the binary, with a small router, a safe `html` template tag that escapes everything by default, and per-page modules loaded on demand.
  - Asset URLs carry a content hash, so browsers cache them forever.
  - Fonts and the third-party libraries (marked, DOMPurify, highlight.js) are vendored. Their versions and checksums are in `static/vendor/README.md` and `static/fonts/README.md`.
- **`scripts/gen-themes.py`** generates `static/css/themes.css` from one palette table.

## Security notes

- Repository and user names are checked against a strict pattern, and repositories, wikis and snippets live on disk under their numeric or random id, so no user input ever becomes a path. Secret snippets have 128-bit random ids.
- Private repositories answer "not found" to everyone without access: through the web, the API, raw files, archives, search and git.
- Sessions are random 256-bit tokens, stored hashed and sent as `HttpOnly; SameSite=Lax` cookies. Writes made with a session cookie must come from this site's own pages, which blocks cross-site request forgery.
- Passwords use Argon2id. Login and sign-up are rate limited.
- User markdown is sanitized with DOMPurify, and the app ships a strict Content Security Policy.
- Raw files are served as inert text or images inside a sandboxing CSP.

## Development

```sh
cargo test                     # unit tests
cd scripts && npm install      # once, for the browser checks (uses Playwright's Chromium, or set CHROME=/path/to/chrome)
bash scripts/e2e.sh            # real server + git clone/push + CI + every page in headless Chromium
bash scripts/ci-e2e.sh         # CI on an external `gitter runner`
```
