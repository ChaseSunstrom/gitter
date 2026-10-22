# Gitter

Fast, simple git hosting. One Rust binary serves the web app, the API and git over HTTPS, and keeps its state in SQLite plus plain bare repositories.

- **Code:** browse repositories, view files with syntax colors and linkable line ranges, blame, history, search, download archives.
- **Insights:** Commits → Graph draws every branch with its merges, lane by lane. A file's History button steps through each version of it, following renames, with a slider and the diff of each. The Insights tab shows contributors, weekly commit activity and languages, and to the repository's admins its traffic: views and clones (fetches count too) per day for two weeks, unique visitors, referring sites and popular pages.
- **Releases:** publish a version from a tag (or a new tag at a branch), with markdown notes, files to download (with download counts) and the source code as zip and tar.gz. Drafts stay with the maintainers, pre-releases are marked, and the repository page shows the latest release.
- **Collaboration:** issues, labels, pull requests with server-side merge (merge commit, squash or fast-forward), comments in markdown, stars and forks.
- **Milestones, assignees, templates:** milestones (a title, a due date and a description) gather issues and pull requests and show how many are done; the Milestones page lists them with their progress, and issue lists filter by milestone and by assignee (Assigned to me). People with write access set an issue's or pull request's milestone and up to 10 assignees from its sidebar, and only people with write access can be assigned: someone who loses it stops showing as assigned (and comes back if it's given back). Markdown files in `.gitter/ISSUE_TEMPLATE/` on the default branch are offered when opening an issue, with optional front matter (`name`, `about`, `title`, `labels`; the labels apply whoever opens it), and `.gitter/pull_request_template.md` starts every pull request's description.
- **Code review:** click a line number in a pull request's Files changed (shift-click for a range) to comment there. Comments form threads with replies that can be resolved; ones whose lines changed since are marked outdated. Gather comments into a review and submit it as Approve, Request changes or Comment (not on your own pull request); the latest decision of each reviewer shows on the pull request and in the list. A ```` ```suggestion ```` block proposes replacement lines, and anyone who can push applies it as a commit with one click (refused if those lines changed meanwhile).
- **Teams:** add collaborators to a repository as Read (see and clone), Write (push, manage issues and pull requests) or Admin (settings, access, deploy tokens, CI secrets), and see in plain words who can do what in Settings → Who has access. Organizations own repositories for a group: owners manage everything, and members get the organization's base permission (none, read or write) on all of its repositories. An organization shares its creator's repository limit and storage quota, and who belongs to it is visible to its members only. Write access includes running CI: pushes run the repository's pipelines with its secrets, so give Write to people you'd trust with those.
- **Wikis, discussions, snippets:** every repository has a markdown wiki with page history (its own git repository, private when the repo is) and a discussions forum with General, Q&A, Ideas and Announcements categories, marked answers and upvotes. Snippets share a few files, public or secret (only the link finds it), with every edit kept as a revision.
- **Editing:** edit, create, rename, delete and upload files in the browser, committing straight to a branch or to a new branch for a pull request.
- **Moving and undoing:** renaming a repository or transferring it to another account or an organization (the recipient, or an owner of that organization, accepts the offer in Settings → Repositories; deploy tokens, CI secrets and webhooks stay behind) keeps the old address working: pages and the API redirect, and git clones and pushes at the old URL as before. A deleted repository waits 7 days in Settings → Repositories → Recently deleted, where you can restore it, then it's removed for good (it counts toward your storage until then; admins can remove it sooner). Settings → Account exports your data (profile, a git bundle of each repository and wiki, your snippets, what you wrote as JSON, your security log; Git LFS files and release files aren't included, download those separately) and deletes your account (not while you're an organization's only owner).
- **Speed:** pages prefetch on hover, the UI has no framework, and diffs render lazily, so even huge commits stay smooth.
- **Command palette:** press `Ctrl K` or `/` to jump to any repository, file, branch or action. Press `?` for every shortcut.
- **Account security:** optional two-factor sign-in (any authenticator app, with recovery codes), a list of where you're signed in with one-click sign-out, a password re-check before dangerous actions (deleting a repo, making a private repo public, creating tokens), and a tamper-evident security log for you and for admins (`gitter doctor` verifies it). Code views mark hidden Unicode ("Trojan Source" bidi tricks, zero-width characters) and look-alike letters from other alphabets, with a warning above the file or diff.
- **Signed commits:** add SSH or GPG public keys in Settings → Signing keys, and commits you sign show a Verified badge in history, on the commit page and in blame (Unverified, with the reason, when the key is unknown, the email doesn't match or the commit was changed after signing).
- **Safe to run in public:** per-address and per-account rate limits (429 with Retry-After), limits on concurrent git transfers, per-account storage quotas enforced by git itself on push, open / invite-only / closed sign-up with invite links, and a report button on repositories, issues, comments and accounts that feeds an admin queue where moderators dismiss, hide or suspend.
- **CI built in:** put a `.gitter/ci.yml` (or just a `.gitter/ci.sh`) in a repository and every push and pull request runs it, on a runner inside the server or on your own machines with `gitter runner`. Live logs with colors and a fold per step, secrets, artifacts, matrices, schedules, re-runs, and a pass/fail mark on commits and pull requests.
- **Themes:** Aurora (the default), Forge, Ultraviolet and Daylight, each in light and dark. Pick one in Settings → Appearance or from the palette.

## Self-host it (one file)

```sh
git clone <this repository> gitter && cd gitter
echo GITTER_URL=https://git.example.com > .env   # the address people will use
docker compose up -d                             # builds a ~70 MB image the first time
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

Signed commits: add your public key in Settings → Signing keys (an SSH `.pub` line or a GPG `--armor --export` block), then sign as usual (`git config gpg.format ssh`, `git config user.signingkey ~/.ssh/id_ed25519.pub`, `git commit -S`). A commit is **Verified** when its signature is good, the key is on an account, and the committer email is that account's email or its no-reply address (`<id>+<name>@users.noreply.gitter`). Accounts have one email and Gitter doesn't confirm it by mail, so Verified means "signed by this account's key", not "this person owns that address"; the badge names that account (**Verified · alice**). Several accounts may add the same public key (adding someone else's gains nothing: it only verifies commits whose committer email is yours). Checking needs `ssh-keygen` (OpenSSH 8.4+) and `gpg` on the server (`gitter doctor` says which are there); without one, those commits show "Can't verify". The Docker image has both. Only detached signatures count: a signed message pasted into a commit's signature header doesn't verify it. Registered GPG keys are kept in a keyring under `GITTER_DATA/gpg`, refilled from the database after each start, so backups don't need it.

Release files upload by streaming, so they can be as big as Git LFS files (the same limit), and they count toward the owner's storage quota too. They're stored under `GITTER_DATA/releases`, always download as attachments, stay as private as their repository, and backups include them. A draft's new tag is made when it's published, and deleting a release deletes its files but keeps its tag. From a script: `curl -H "Authorization: Bearer $TOKEN" --data-binary @app.tar.gz "$URL/api/repos/OWNER/REPO/releases/ID/assets?name=app.tar.gz"`.

Container images: every repository is also an image registry for docker, podman and skopeo (the OCI distribution API at `/v2/`). `docker login HOST` with your username and password or an access token (a token if you use two-factor sign-in; a deploy token works for its repository), then `docker push HOST/owner/repo:tag` or `HOST/owner/repo/name:tag` (names in lowercase). Images follow their repository: whoever can read it pulls, whoever can push to it publishes, and they're gone with it (the trash hides them; purging frees them). Layers are stored once by content under `GITTER_DATA/registry` (one layer in several images or repositories counts once toward the owner's storage quota), each is limited like a Git LFS file, blobs no image names are removed after a day and abandoned uploads after an hour, and backups include them. The repository's Packages tab lists images, tags, sizes and the pull command, plus untagged images (left behind when a tag moves on; still pullable by digest and still using storage), and writers can delete tags and images there. A repository holds up to 100 image names and 1,000 tags. Sign-in is HTTP Basic only (no token service): docker, podman and skopeo pull public images anonymously, but containerd and Kubernetes nodes may need credentials configured even for public ones. Docker accepts plain HTTP only for `localhost`/`127.0.0.1`; anywhere else serve Gitter over HTTPS. Generic packages hold any file under a name and version: `curl -H "Authorization: Bearer $TOKEN" --upload-file tool.tar.gz "$URL/api/repos/OWNER/REPO/packages/generic/tool/1.0.0/tool.tar.gz"`, downloaded from the same URL.

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

**Runners.** The server runs up to 2 jobs at once itself, only for repositories owned by admins and only for runs a site admin started: their push or merge, their press of Run, or a schedule on a branch they last pushed. A collaborator's push to an admin's repository waits for a separate runner. A job on the built-in runner without an `image` runs as the server's own user and can read its data folder. To add machines, make a token in **Site administration → CI runners**, then run on the machine. It needs git, and docker for jobs with an `image`:

```sh
GITTER_RUNNER_TOKEN=grn_… gitter runner --url https://git.example.com --labels gpu --jobs 4
```

**Sandbox.** A job with an `image` runs in a container that gets only its workspace mounted (never the docker socket), no new privileges, a few capabilities dropped (raw sockets, device nodes, chroot), at most 4096 processes, and the site's CPU and memory limits (2 CPUs and 4 GB unless the admin changes them; 0 is no limit). `network: false` on a job, or turning the network off site-wide, gives the container no network. Jobs without an `image` run as plain processes on the runner, so register runners on machines you're happy to have run your repositories' code.

**Approval.** A run started by an agent's changes (see AI agents) waits until someone with write access reads the changes and presses **Approve and run**. It then goes to a separate runner, without secrets.

Jobs are leased with a heartbeat. A job whose runner dies goes back to the queue, and a runner that lost its lease can't report over the new attempt. Stopping a runner (Ctrl-C or SIGTERM) kills its running steps, and their jobs go back to the queue.

**Branch rules, checks and auto-merge.** In a repository's **Settings → Branch rules**, admins protect the branches matching a pattern (`main`, `release/*`; `*` stays within one part of the name, `**` spans parts, and `main` doesn't cover `fix/main`). A rule can:

- require pull requests: no direct pushes or web edits;
- require approving reviews of the pull request's latest commit (a later push needs a new approval) from people with write access, any of whom can also block by requesting changes;
- require checks to pass on the pull request's head: CI job names (a job skipped because one it needs failed counts as failed), or contexts of commit statuses that other systems post with `POST /api/repos/OWNER/REPO/statuses/SHA` (`{"state": "success", "context": "deploy", "target_url": "https://…"}`; states are pending, success, failure and error). Anyone with write access can post statuses, so a status named like a CI job can only make that check worse;
- allow or refuse force-pushes and deleting the branch (refused unless allowed);
- merge through a queue (below).

Rules apply to git pushes (through a pre-receive hook, before anything moves), web edits, branch deletion and merges. Repository admins may bypass them unless the rule says it applies to admins too, and a bypassed merge has to be asked for. Merging past the rules is in the security log. **Merge when ready** on a pull request merges it by itself, with the method chosen, once its checks pass and its reviews are in; a push to the branch by anyone else than whoever asked cancels it. A CI badge for a README: `![CI](https://git.example.com/api/repos/OWNER/REPO/ci/badge.svg)` (add `?branch=NAME` for another branch). Links to public repositories, issues and pull requests unfurl in chat apps.

**Merge queue.** On a branch whose rule has a merge queue, pull requests aren't merged directly: **Add to merge queue** puts one in line once its reviews are in. The first in line is merged onto the branch's current tip as a test commit (kept on the server, not shown to clients), and its CI runs there as the pull request's own runs would (pipelines with a `pull_request` trigger matching the branch, as `GITTER_EVENT=merge_queue`). When the rule's required checks pass on that commit (or, if it names none, every check that ran there), the branch moves to that exact commit and the next one is tested on top. A failed check, a push to the pull request, a conflict, a new change request or checks unfinished after an hour take it out of the line with a note on its timeline. If the branch moves some other way, the one being tested is tested again on the new tip. It merges as whoever queued it, who must still have write access. A queue implies pull requests only, and the queue merges with a merge commit or a squash (fast-forward can't work behind another entry). Test commits aren't pushes, so outside systems that post statuses find them through the queue listing (`test_sha`). Admins the rule isn't enforced on may still merge directly, skipping the line; that goes in the security log. The line is shown on the pull requests page; over the API: `POST`/`DELETE /api/repos/OWNER/REPO/pulls/N/queue` (`{"method": "merge"}`, or `squash` or `ff`) and `GET /api/repos/OWNER/REPO/merge-queue`.

**Who can run CI** is set in Site administration:

- off;
- admins' repositories (the default);
- everyone: anyone's jobs go to separate runners, never the built-in one. Use disposable runner machines for this.

Turning CI off stops new runs, re-runs and queued jobs.

**Storage.** Logs and artifacts live under `GITTER_DATA/ci`. Artifacts are deleted 30 days after their job, and files of deleted repositories within the hour. Backups don't include CI logs and artifacts; they do include the key for secrets.

**Run it locally.** `gitter ci run` runs `.gitter/ci.yml` on your machine with the same parser and step runner, no server needed. Jobs run one at a time in `needs` order, and a job whose needs failed is skipped. Each job gets a fresh copy of the work tree as it is now: tracked files and untracked ones that aren't ignored, without `.git`. Use `--here` to run in the folder itself, for example when steps need git history. `HOME` is a fresh folder, and no secrets are set. Triggers, `paths` and `runs-on` don't apply, and jobs with an `image` need docker. It exits 1 when a job fails.

## Command-line client

The same `gitter` binary is a client for any Gitter server, like `gh`:

```sh
gitter login https://git.example.com       # asks for an access token (or --token, or GITTER_TOKEN)
gitter repo create notes --private && gitter repo clone alice/notes && cd notes
gitter issue create --title "Crash on start" --body "…"
git switch -c fix && git commit -am "Fix the crash"
gitter pr create                           # pushes the branch first, then opens the pull request
gitter pr list; gitter pr checkout 7; gitter pr merge 7 --method squash
gitter ci status; gitter ci logs 12        # follows a running job; gitter ci rerun 12
```

`gitter login` saves the server and token to `~/.config/gitter/config.json`, readable only by you. Inside a clone, commands act on the repository the git remote points at. Elsewhere, pass `-R owner/name`. `--json` prints the server's answer as JSON. Git operations get the token through the environment, only for that server's URLs, so it is never written to `.git/config`. `gitter <command> --help` lists every command.

## Webhooks

A repository's admins add webhooks under **Settings → Webhooks** (signed in on the web, with a recent password confirmation). Each event is a `POST` of JSON to the payload URL with these headers:

| Header | Value |
|---|---|
| `X-Gitter-Event` | `ping`, `push`, `issues`, `issue_comment`, `pull_request` or `ci_run` |
| `X-Gitter-Delivery` | A UUID for the delivery. Retries and redeliveries keep it, so receivers can skip repeats. |
| `X-Gitter-Signature-256` | `sha256=` and the hex HMAC-SHA256 of the body, keyed with the webhook's secret. Compare it in constant time. |

Every payload has `repository` (`id`, `full_name`, `owner`, `name`, `description`, `private`, `default_branch`, `url`, `clone_url`) and `sender` (`id`, `username`, or null for scheduled CI runs). Besides those:

- `ping`: `hook` (`id`, `events`). Sent when the webhook is added, and by its **Send a ping** button.
- `push`: `ref`, `before` and `after` (null when the ref was created or deleted), `created`, `deleted`, `commits` (up to 20 the push added, newest first; a new branch lists only its head) and `head_commit`. Each commit has `id`, `message`, `timestamp`, `author` (`name`, `email`) and `url`. A push that moves more than 20 refs sends events for the first 20.
- `issues` and `pull_request`: `action` (`opened`, `edited`, `closed`, `reopened`, or `merged` for pull requests) and `issue` or `pull_request`: `number`, `title`, `body`, `state`, `user`, `labels`, `created_at`, `updated_at`, `closed_at`, `url`; pull requests add `head`, `base`, `merged` and `merge_commit_sha`.
- `issue_comment`: `action` (`created`, `edited`, `deleted`), `issue` (as above, plus `is_pull_request`) and `comment` (`id`, `body`, `user`, `created_at`, `updated_at`).
- `ci_run`: `action` `completed` and `run`: `number`, `event`, `ref`, `sha`, `pr_number`, `title`, `status` (`success`, `failure`, `cancelled` or `skipped`), `error`, `created_at`, `started_at`, `finished_at`, `url`.

Times are Unix seconds. Any 2xx answer counts as delivered. Anything else, no answer within 10 seconds, or a refused address is tried again after 1 minute, 5 minutes, 30 minutes, 2 hours and 6 hours, then given up. Redirects aren't followed, and only the first 64 KB of an answer is read. The queue is in the database, so retries survive restarts (a restored backup sends what was still waiting). Each webhook's page lists its last 50 deliveries with the request, the response code and the first 2 KB of the answer, and can send any of them again.

Deliveries only go to public internet addresses: the host name is looked up once and the connection goes to the checked address, so loopback, private, link-local, shared (100.64/10), multicast and reserved addresses are refused, DNS rebinding included. A site admin can tick **Allow private addresses** on a webhook to reach the server's own network; it switches off again if someone who isn't an admin changes the URL, or when its repository's owner stops being an admin. Hooks of a suspended account or a moderator-hidden repository don't send. Each receiver gets one delivery at a time, so a slow one doesn't hold up the others; issue, pull request and comment bodies are cut at 16 KB, and when 100 deliveries are already waiting for a receiver, new events are dropped and the drop is logged on the webhook's page. The secret is encrypted at rest with `GITTER_DATA/secret.key`.

## AI agents (MCP)

Gitter is an [MCP](https://modelcontextprotocol.io) server at `/mcp` (JSON-RPC 2.0 over the streamable HTTP transport, plain JSON replies). In **Settings → Access tokens**, pick **Agent (MCP)**, the repositories (up to 20) and the tools the agent may use, then add the server to your agent. For Claude Code, `.mcp.json`:

```json
{
  "mcpServers": {
    "gitter": {
      "type": "http",
      "url": "https://git.example.com/mcp",
      "headers": { "Authorization": "Bearer gtr_your_agent_token" }
    }
  }
}
```

or `claude mcp add --transport http gitter https://git.example.com/mcp --header "Authorization: Bearer gtr_…"`.

Tools: `list_repos` (always on), `search_code`, `read_file`, `list_tree`, `list_issues`, `get_issue`, `create_issue`, `comment`, `open_pull_request`, `list_ci_runs`, `get_ci_run`, `get_ci_log` and `commit_files`. Each call is an ordinary API request made as you, so the agent can never do more than you can, and less:

- It reaches only the repositories you picked (not even other public ones) and only the tools you ticked.
- It can't change settings, collaborators or webhooks, see CI secrets, tokens or your account, or merge pull requests. `commit_files` starts a new branch (`new_branch`) or adds to one an agent started, never the default branch or a person's branch; then `open_pull_request`. Agents hold at most 100 branches per repository (deleting one frees its place).
- Any CI run of an agent's branch, whoever pushed, waits for your approval, gets no CI secrets (the agent wrote the workflow and reads the log), and never runs on the built-in runner (the server itself): once approved it goes to a runner you registered. Once a person merges the work, it runs like anyone's.
- The token works only at `/mcp`: not for git, the REST API or the web.
- It has its own rate limit (120 calls a minute) and its changes count apart from yours.
- Issues, pull requests, comments and feed entries it makes show an **Agent** badge next to your name, and each change is in your security log as "*token name*: *tool* *repository* …".

## Security scanning

**Push protection.** Every push (over HTTPS or SSH) and every commit made on the web (editing, uploading, applying a review suggestion, an agent's `commit_files`) is scanned before any branch moves. The lines it adds are checked for AWS access keys, private keys (a PEM header followed by key material), Gitter's own tokens (`gtr_`, `grn_`, `gcj_`), GitHub, GitLab, Slack, Stripe live, Google, npm, PyPI, OpenAI and Anthropic keys, and random-looking values assigned to names like `password`, `secret`, `token` or `api_key`. A push that adds one is refused with each finding's file, line and kind and a masked form (`AKIA…7Q`); the secret itself is never stored, logged or printed (findings are kept masked, with a fingerprint keyed by the server's secret key). Values that are URLs or paths don't count. A one-time allowance is used up only by a push that goes through. The scan reads at most 512 MB or two minutes of a push, and lets the rest through (the server log says so):

```
remote: Push refused: it adds something that looks like a secret.
remote:   conf/aws.env:2  AWS access key ID  AKIA…2T  (commit 3f2a1c9)
remote: If it's a false positive, a test value, or a secret you'll revoke, allow it once here and push again:
remote:   https://git.example.com/alice/app/security?allow=4be1…
```

Take the secret out of the commits (removing it in a later commit isn't enough), or open the link, say why, and **Allow once**: the next push or commit by you that has that finding (same kind, file and secret) goes through, within 3 hours, and the allowance is used up. Only the person who pushed can allow their finding, never an agent token, and each allowance is in the audit log. Web commits show the same findings in a dialog. Pushes are scanned up to 512 MB of changes; merge commits' own changes aren't.

**Security tab.** Repository admins can scan every file on the default branch on demand and see the findings. Everyone who can read the repository sees its **dependencies**, read from the default branch's lockfiles (`Cargo.lock`, `package-lock.json`, `yarn.lock`, `pnpm-lock.yaml`, `go.sum`, `requirements.txt` pins, `poetry.lock`, `Gemfile.lock`; not under `node_modules`, `vendor` or `third_party`), the **vulnerabilities** known for them, and an **SBOM** download as SPDX 2.3 or CycloneDX 1.5 JSON (`GET /api/repos/OWNER/REPO/sbom?format=spdx|cyclonedx`).

**Vulnerability data** comes from an [OSV](https://osv.dev) snapshot the site admin imports, so it works offline: on any Security tab (or `POST /api/admin/osv` with the file as the body, up to 512 MB), upload one OSV JSON record, a JSON array of them, or an ecosystem's zip from `https://osv-vulnerabilities.storage.googleapis.com/<ecosystem>/all.zip` (e.g. `crates.io`, `npm`, `Go`, `PyPI`, `RubyGems`). Importing again updates advisories by id; withdrawn ones are removed. Versions are matched against each advisory's version list and its SEMVER/ECOSYSTEM ranges with a simple dotted-number comparison, so exotic version schemes can be off.

## Git over SSH

Off by default. Set `GITTER_SSH_ADDR` (for example `0.0.0.0:2222`) and Gitter also serves clone, fetch and push over SSH, from its own built-in SSH server: no system `sshd` to set up. Its host key is made on first start in `GITTER_DATA/ssh_host_ed25519_key` (back it up, or clients will warn that the host changed). Clone URLs use the host name in `GITTER_URL` and the port listened on; when that port is mapped to another one (a container's `2222` published as `22`), set `GITTER_SSH_PORT` to the public port. On port 22 URLs look like `git@git.example.com:alice/hello.git`, otherwise `ssh://git@git.example.com:2222/alice/hello.git`. With Docker Compose, add `- "22:2222"` under `ports` and `GITTER_SSH_ADDR: 0.0.0.0:2222` and `GITTER_SSH_PORT: 22` under `environment`.

Add your public key in **Settings → SSH keys** (Ed25519, ECDSA, or RSA of 2048 bits or more; these sign you in, apart from signing keys). A key belongs to one account. Then the clone menu shows the SSH URL too.

- The SSH user name doesn't matter (`git@` is usual): the key says who you are. Access is exactly git over HTTP's: private repositories are "not found" without access, pushing needs write access, renamed and transferred repositories answer at their old path, and suspended accounts are refused. Pushes run the same post-push work (CI, webhooks, the feed, size and storage checks).
- Only `git-upload-pack`, `git-receive-pack` and `git-upload-archive` run, on an `owner/repository` path. There's no shell, no other command, no port or agent forwarding, and no environment except `GIT_PROTOCOL`.
- Branch rules apply to SSH pushes exactly as to HTTPS ones, and a removed key or suspended account stops at the next command.
- Limits: each git command takes the same per-address transfer limits as HTTP (transfers at once, starts per minute); an address opens at most 120 connections a minute, at most 512 are open in all, a connection must sign in and start its command within 30 seconds, unknown keys are counted per address (a registered key always gets in), a command with nothing moving for 2 minutes is stopped, and a shared connection (`ControlMaster`) can run one command after another.
- Deploy keys aren't supported yet: use a deploy token over HTTPS.

## Viewers, reverts, uploads and the demo

- **Rich file views:** CSV and TSV files show as a table (the first 1,000 rows and 200 columns; click a column to sort, numbers as numbers), Jupyter notebooks render their markdown, highlighted code and text and image outputs, STL models (ASCII or binary, up to a million triangles) turn in 3D (drag to orbit, scroll to zoom, double-click to reset), PDFs open in the page, and audio and video files play. **Code** next to **Preview** shows the source of the text ones. Nothing pushed runs: notebook HTML and script outputs show as their source, markdown is sanitized, and files come from `/raw` with a sandbox (files that really are PDFs go without one, since browsers won't show a PDF in a sandbox; the browser's PDF viewer keeps a PDF's own scripts apart from the site, and only Gitter's pages may frame them).
- **Image diffs:** a changed picture in a commit, comparison or pull request shows the two versions side by side (2-up), as a swipe, or as onion skin.
- **Revert from the web:** **Revert** on a commit page, or on a merged pull request, puts the inverse change on a new branch (`revert-<sha>` or `revert-pr-<n>`) and opens a pull request into the branch it landed on, so it goes through review like anything else. When later changes touch the same lines, the error names the files: revert it locally with `git revert` instead.
- **Drag and drop:** drop files or folders anywhere on a folder or file page to upload them into that folder (with the usual commit message and new-branch choice).
- **Install it as an app:** Gitter has a web app manifest, so browsers offer to install it (Chrome and Edge: the install icon in the address bar; iOS: Share → Add to Home Screen).
- **`gitter demo`:** seeds an empty `GITTER_DATA` with three people (alice is the admin, plus bob and carol, all with one password it prints), repositories with history, a fork and a file for each viewer, issues with comments, an open pull request with an approving review and a merged one, CI runs (queued until the server starts), wiki pages and a discussion, then starts the server, reachable from this machine only unless `GITTER_ADDR` is set. It refuses a directory that already has anything in it.

  ```sh
  GITTER_DATA=/tmp/gitter-demo gitter demo
  ```

## Notifications

The bell in the top bar opens **/notifications**, one row per thread (issue, pull request, discussion, failed CI run) with why you got it, newest first. Filter by **Unread**, **Participating** (not just watching), **Mentions**, **Done** or a repository; mark one read (opening it does too), mark it **done** (it comes back only when there's news), **unsubscribe** from its thread, or mark everything read. The bell shows the unread count, refreshed as you move around and every minute.

Who hears of what:

- **Taking part.** The author of an issue, pull request or discussion, everyone who commented or reviewed, its assignees, and people @mentioned in it hear of new comments, reviews, closes, reopens and merges.
- **@mentions** of `@username` in issue, pull request, comment, review and discussion bodies notify that person and subscribe them to the thread. Mentions in code (spans, fenced or indented blocks) and email addresses don't count; a body counts at most 50, and at most 200 of an author's mentions an hour count. Edits don't notify.
- **Assignments** tell the person assigned (only them).
- **Failed CI runs** tell whoever pushed or started the run.
- **Watching.** The **Watch** menu in a repository's header: *Participating and @mentions* (the default), *All activity* (every new thread and everything in it; a repository's owner watches it by default) or *Ignore* (nothing, not even @mentions). On an issue, pull request or discussion, **Subscribe** / **Unsubscribe** overrides that for the one thread; a new @mention still reaches you after unsubscribing.

You're never notified of what you did yourself, and an agent acts as its owner, so its owner isn't told of its work. Only people who can read the repository now are notified, under exactly the rule that governs viewing it (private repositories, collaborators and org roles, moderator-hidden repositories and issues, the trash); the inbox and the count check it again when they're read, so losing access to a repository hides its notifications. Deploy and agent tokens can't read the inbox. There's no email delivery (Gitter has no mail settings).

API: `GET /api/notifications?filter=all|unread|participating|mentions|done&repo=OWNER/REPO&page=N`, `GET /api/notifications/count`, `POST /api/notifications/{id}/read|done|unsubscribe`, `POST /api/notifications/read` (`{"repo": "OWNER/REPO"}` optional), `GET|PUT /api/repos/OWNER/REPO/subscription` (`{"level": "participating|watching|ignoring"}`), `GET|PUT /api/repos/OWNER/REPO/issues/N/subscription` and `…/discussions/N/subscription` (`{"subscribed": true|false}`).

## Configuration

Everything is optional. The defaults are tuned for a single small server, and the sign-up mode and limits can be changed later in Site administration (the UI value wins over the variable).

| Variable | Default | Meaning |
|---|---|---|
| `GITTER_URL` | `http://localhost:3000` | Public address, used in clone URLs. `https://` also turns on secure cookies. |
| `GITTER_ADDR` | `0.0.0.0:3000` | Address to listen on. |
| `GITTER_DATA` | `./data` | Where the database and the bare git repositories live (`repos/`, `wikis/`, `snippets/`, deleted repositories in `trash/`, plus Git LFS files in `lfs/`, release files in `releases/` and container image layers in `registry/`; data exports wait a day in `exports/`). Back this folder up, or use `gitter backup`, which includes all of them. |
| `GITTER_SIGNUP` | `open` | `invite` needs an invite link from an admin; `closed` stops new sign-ups. The first (admin) account is always created through the setup link. |
| `GITTER_MAX_PUSH_MB` | `500` | Largest single push. |
| `GITTER_MAX_REPOS` | `200` | Repositories per user (admins are exempt). |
| `GITTER_TRUST_PROXY` | `false` | Behind a reverse proxy, set this so rate limits use the client address from `X-Forwarded-For` (the last hop, which your proxy adds). |

Behind nginx or Caddy, set `GITTER_URL` to the public address. Requests whose `Origin` matches it are accepted even when the proxy rewrites `Host`.

## How it works

- **`src/`** is the server, built on axum and tokio.
  - It stores accounts, issues and pull requests in SQLite (WAL mode, run on the blocking pool).
  - It reads and writes code by calling the `git` CLI with argument vectors, a scrubbed environment, no hooks, timeouts and a concurrency cap.
  - Clone, fetch and push go through `git http-backend`, or over SSH (`src/ssh.rs`, russh) straight to `git upload-pack` and `git receive-pack`, with the same checks.
  - Web edits and merges use plumbing commands (`read-tree`, `merge-tree --write-tree`, `commit-tree`, `update-ref` with an expected old value), so no working copies are needed and concurrent changes are detected.
- **`static/`** is the web app. It is plain ES modules embedded into the binary, with a small router, a safe `html` template tag that escapes everything by default, and per-page modules loaded on demand.
  - Asset URLs carry a content hash, so browsers cache them forever.
  - Fonts and the third-party libraries (marked, DOMPurify, highlight.js) are vendored. Their versions and checksums are in `static/vendor/README.md` and `static/fonts/README.md`.
- **`scripts/gen-themes.py`** generates `static/css/themes.css` from one palette table.

## Security notes

- Repository and user names are checked against a strict pattern, and repositories, wikis and snippets live on disk under their numeric or random id, so no user input ever becomes a path. Secret snippets have 128-bit random ids.
- Private repositories answer "not found" to everyone without access: through the web, the API, raw files, archives, search and git.
- Access is one rule for every route: owners and site admins can do everything; collaborators and organization members get their role; a token can only narrow it (read-only tokens never write, only full-access tokens administer, a deploy token sees other repositories as if signed out, and an agent token sees only its own). Organizations can't sign in.
- Sessions are random 256-bit tokens, stored hashed and sent as `HttpOnly; SameSite=Lax` cookies. Writes made with a session cookie must come from this site's own pages, which blocks cross-site request forgery.
- Passwords use Argon2id. Login and sign-up are rate limited.
- User markdown is sanitized with DOMPurify, and the app ships a strict Content Security Policy.
- Raw files are served as inert text or images inside a sandboxing CSP.
- Traffic stores no addresses: a visitor is a hash of the address with a random salt that changes every day and is never written down, so it can't be traced back or followed across days. Only the referring site's host is kept, and traffic older than 14 days is deleted.

## Development

```sh
cargo test                     # unit tests
cd scripts && npm install      # once, for the browser checks (uses Playwright's Chromium, or set CHROME=/path/to/chrome)
bash scripts/e2e.sh            # real server + git clone/push + CI + every page in headless Chromium
bash scripts/ci-e2e.sh         # CI on an external `gitter runner`
```
