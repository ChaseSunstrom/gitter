**Answer**

Most of the plumbing exists. `src/net.rs` gives SSRF-safe outbound HTTP, and `mirrors::git` plus `mirrors::exec` give a pinned, storage-capped `git fetch` from a URL. Fork PRs already copy the head into `refs/pull/N/head`. Settings already hold booleans. There is no remote-person concept, no `/.well-known` route, no Ed25519 signing for HTTP, and `ring` is not a direct dependency. Latest migration is v33.

**Findings**

1. **Fetch from a URL**
- `net::check_url` accepts only http(s) with a host and no `user@`. `src/net.rs:45-55`. High.
- `net::is_public(IpAddr)` blocks private, loopback, CGNAT, 6to4, Teredo, NAT64 and the like. `src/net.rs:60-92`. High.
- `net::pin(url)` resolves once, checks every address, and returns a curl resolve entry. It rejects a trailing dot. `src/net.rs:102-119`. High.
- `mirrors::git(st, repo, &Remote)` builds the git command. It sets `protocol.*`, `http.followRedirects=false` and a low-speed stall limit (1 KB/s for 60 s). It pins the host with `http.curloptResolve`. It puts credentials in `GIT_CONFIG_*` as an `http.extraHeader` Basic header, never in argv. `src/mirrors.rs:61-108`. High.
- `Remote { url, username, password, allow_private }` is at `src/mirrors.rs:61-67`. `mirrors::clean` redacts credentials from git errors at `src/mirrors.rs:112`. High.
- `mirrors::exec(st, cmd, cap: Option<(&Path, u64)>)` takes a git permit and runs git in its own process group. It has a 1800 s timeout (`:35`). It polls pack-directory growth every 2 s and kills git past `cap`. It returns a `"Stopped…"` error and deletes `tmp_` packs. `src/mirrors.rs:127-173`. High.
- The pull-mirror fetch is at `src/mirrors.rs:290-292`. It uses `fetch.fsckObjects=true --prune --force "+refs/heads/*:refs/heads/*"` and caps at `storage_left`. High.
- `api/mirrors.rs::create` handles "copy from URL" (one-time import with `once`, pull only, empty repo only). It uses `check_url` (`:38-45`, 2048 chars) and allows `allow_private` for admins only (`:134`). `src/api/mirrors.rs:117-166`. High.
- `validate::` has only `web_link, name, username, repo_name, rev, ref_name, path, write_path, sha, email`. `src/validate.rs:42-120`. There is no URL or SSRF validator there. High.
- `imports.rs:112-127` has `check_scheme` (https required unless `allow_private`) and `check_base` (no `?` or `#`). High.

2. **Forks and fork PRs**
- `repos.forked_from INTEGER REFERENCES repos(id) ON DELETE SET NULL` is at `src/db.rs:52`. It is a local repo id only, so there is no remote reference. High.
- `api/repos.rs::fork` runs `insert_repo(..., Some(src.id))` then `git.clone_bare` from the local path. `src/api/repos.rs:463-514`. High.
- v31 adds `pulls.from_fork`, `pulls.head_repo_id` (FK, `ON DELETE SET NULL`) and `pulls.maintainer_can_modify`, plus an index. `src/db.rs:1009-1016`. `pulls.head_branch` is the plain branch name (`src/db.rs:82`). High.
- `issues.rs::create_pull` (`:1093`) calls `head_repo()` (`:1082`, a local `owner/name` in the same fork family). It then calls `fetch_branch(st, base_id, fork_path, branch, dest)` into a temp `refs/pull/new-<rand>/head`. `open_pull` (`:1157`) inserts the issue and pull rows, then runs `update-ref refs/pull/{num}/head`. High.
- `fetch_branch` (`src/api/issues.rs:113-130`) fetches from a local filesystem path with `git fetch --no-tags … +refs/heads/{branch}:{dest}` through `mirrors::exec`, capped at the base owner's `storage_left`. It does not use `mirrors::git`, so for a remote head you would build the command with `mirrors::git` instead. High.
- `issues.rs::head_ref(number, branch, from_fork)` returns `refs/pull/N/head` for fork PRs (`:107`). `may_copy` is at `:134`. High.

3. **PR and user model**
- `issues(id, repo_id, number, author_id INTEGER NOT NULL REFERENCES users ON DELETE CASCADE, title, body, is_pr, state, …)` is at `src/db.rs:65-78`. `pulls` is at `:80-89`. `comments.author_id` is NOT NULL too (`:93`). High.
- `users` is at `src/db.rs:17-28`: `username` UNIQUE, `pass_hash NOT NULL`, `display_name`, `email`. v2 adds `suspended` (`:128`). v10 adds `is_org` (`:341`): "An org is a users row that can't sign in". Login filters `is_org = 0` and `suspended = 0` (`src/auth.rs:435`). High.
- There is no ghost or deleted-user concept. Grep for "ghost" finds nothing in `src`. High.
- Agents are tokens with `kind='agent'`, not users (`src/db.rs:557`; `auth.rs:302`). The agent's name is stored via `insert_issue(..., agent)` (`src/api/issues.rs:1167,1185`) and surfaced as `"agent"` in comment JSON (`:552`). High.
- The issue JSON author is `u.username` (`ISSUE_SELECT`, `src/api/issues.rs:60`; `author_id` is also selected). Medium: I did not read the serializer.

4. **Crypto dependencies**
- `Cargo.toml` direct dependencies are `base64 0.22`, `hmac 0.12`, `sha2`, `sha1`, `hex`, `rand 0.8`, `chacha20poly1305`, `argon2`, `ureq 3` (rustls) and `russh 0.64` with the `ring` feature. `Cargo.toml:10,15,22,31`. High.
- `ring` is not a direct dependency. A comment at `Cargo.toml:21` says it comes in "already here for rustls". `Cargo.lock:1977`. High.
- `ed25519-dalek` is in `Cargo.lock:734` only, transitively. High.
- Ed25519 is used only for the SSH host key, via `russh::keys::ssh_key::private::Ed25519Keypair::from_seed`. `src/ssh.rs:84-100`. The key is stored OpenSSH-PEM in `GITTER_DATA/ssh_host_ed25519_key`. High.
- HMAC-SHA256 is used for webhook signing (`src/webhooks.rs:41`, header `X-Gitter-Signature-256`), at `src/lib.rs:101` and for scan fingerprints (`src/scan.rs:309`). High.
- Nothing is served under `/.well-known`. The only mention is an outbound OIDC discovery call (`src/api/oauth.rs:142`). Routes are at `src/lib.rs:126-175`, with `/robots.txt` and `/healthz` as precedent and a `web::index` fallback. High.

5. **Admin settings**
- The `settings` table is `(key TEXT PK, value TEXT)` (`src/db.rs:127`). One row, key `'instance'`, holds the whole `Settings` struct as JSON. `src/settings.rs:12-54,186-227`. High.
- Reads use `st.settings.get().field` (a cloned `RwLock` snapshot). Writes go through `Live::save`, which calls `validate()` and upserts. `GET`/`PUT` live in `src/api/admin.rs:319-339`, with a `settings_changed` audit event. High.
- A boolean already exists: `ci_network` with `#[serde(default = "default_true")]` (`src/settings.rs:46-47`). It is initialised in `from_config` at `:108`. A new field needs a serde default, a `from_config` entry and optionally validation. `load` overlays the saved JSON on the defaults, so older rows are fine. High.

6. **Migrations**
- `const MIGRATIONS: &[&str]` at `src/db.rs:14` is an append-only array. The latest is v33 (`:1058-1068`). Apply order comes from `PRAGMA user_version` (`:1086-1090`). A test asserts `user_version == MIGRATIONS.len()` (`:1135`). To add one, append a `r#"…"#` entry with a `// v34:` comment. High.

7. **Outbound HTTP**
- `ureq` 3 is the only HTTP client (`Cargo.toml:31`); there is no reqwest. The git CLI is used for repo fetches. High.
- `net::fetch_lenient` builds the agent with a 10 s global timeout, `max_redirects(0)`, no proxy and a `Gitter/<ver>` user agent. `src/net.rs:162-188`. High.
- Unless `allow_private`, the agent uses a custom `PublicOnly` resolver that refuses the lot if any resolved address is non-public. The check and the connection use the same addresses, so DNS rebinding can't get through. `src/net.rs:121-139,171-175`. High.
- `net::fetch(url, headers, body, allow_private, cap)` errors if the reply passes `cap`. `net::post` is lenient and defaults to `MAX_REPLY` (64 KB). `src/net.rs:144-159`. Both are blocking, so call them from `spawn_blocking`. High.
- Webhook delivery is a `webhook_deliveries` table used as queue and log, with retries from `BACKOFF` (`src/webhooks.rs:21`). It sends through `spawn_blocking(net::post(url, headers, body, allow))`. `src/webhooks.rs:1-4,375-376`. High.

**Risks / surprises**
- `fetch_branch` assumes a local path, and `head_repo` requires a local repo in the same fork family. A remote head needs a new head-repo representation, because `head_repo_id` is an FK to `repos`.
- `author_id` is NOT NULL with a `users` FK on `issues` and `comments`, so bob@B needs a `users` row. That row would need a non-login marker, and `username` is UNIQUE NOCASE, so `bob@B` would have to be stored as the username. The existing pattern is `is_org` (can't sign in) plus `pass_hash NOT NULL`.
- Federation URLs from A to B would run through the same `pin`/`is_public` path. That blocks local test instances unless `allow_private` is used (admin-only today).
- The `exec` stop detection depends on error strings starting with `"Stopped"` (`src/api/issues.rs:124`).

**Not checked**
- The `insert_issue` body, the `Repo::readable` visibility rules, the `/mcp` agent code, and the front-end rendering of authors.
- Rate limiting for unauthenticated endpoints, which a signed-request inbound route would need.
- The CLI.

**Noticed:** none
