1. **Answer**

Auth is cookie sessions plus hashed bearer tokens. Every login path ends in `create_session`, so an OAuth callback can reuse it. The main tree's `src/net.rs` can only POST. The mirrors lane adds `net::pin` and git-through-env helpers that import should reuse. Issues, PRs, comments, labels, milestones and releases map cleanly, but author columns are NOT NULL and cascade, so imported authors need placeholder users. No import or migrate feature exists yet.

2. **Findings**

**Auth today**
- `User` is `{id, username, display_name, email, is_admin, limits}`. `limits` carries token scope, a deploy-token repo, and an agent. — `src/auth.rs:23-32,62-77` — high
- Sessions: a 32-byte random value is stored as a sha256 hash. The cookie is `gitter_session`, HttpOnly, SameSite=Lax, 30 days. — `auth.rs:180-204,226-240` — high
- `create_session` is called from signup (`src/api/account.rs:135`), login (`:172`), 2FA login (`:216`) and `:420`. — high
- Signup: the first account becomes admin only with the one-time `?setup=` code (`account.rs:84-97`, printed at `src/main.rs:205`). Otherwise `closed` means forbidden and `invite` needs an invite code (`:98-114`). The email unique check is case-insensitive (`:116`). — high
- Password login is rate-limited per account+IP and per IP (`account.rs:147-184`). 2FA (TOTP plus recovery codes) uses a `challenges` store with 5 attempts (`security.rs:150-160`, `account.rs:194`). OAuth login must stay behind the same 2FA gate. — high
- Settings: one JSON blob in `settings(key='instance')` (`src/settings.rs:172-212`). `signup` is "open", "invite" or "closed" (`:118`). The admin API is `get_settings`/`put_settings` (`src/api/admin.rs:317-337`) and the UI is in `static/js/pages/admin.js:145-294`. `GET` returns the whole struct, so provider client secrets should not go in it. — high

**Outbound HTTP and sealing**
- The client is `ureq = "3"` with `default-features = false, features = ["rustls","json"]` (`Cargo.toml:31`). There is no reqwest. — high
- Main `src/net.rs` has only `post` (`:101`). It has `check_url` (http/https only, no userinfo, `:28`) and `is_public` (`:43`). A `PublicOnly` resolver checks every address and ureq connects to exactly those (`:77-97`). Redirects and proxies are off, the timeout is 10 s, and replies are capped at 64 KB (`:16-18`). — high
- Webhooks pass `allow_private` through `net::post` (`src/webhooks.rs:375`). — high
- Secrets: `security::seal`/`open` use ChaCha20-Poly1305 with a random nonce (`security.rs:278-296`). `seal_text`/`open_text` give `sealed:<hex>` (`:167-178`). The key is `GITTER_DATA/secret.key`, held as `st.key` (`:238`, `src/lib.rs:78,96`). `ci_secrets.value` is a BLOB (`db.rs:318`) and webhook secrets are sealed too (`api/webhooks.rs:58`). There is no AAD, so bind ownership in the row. — high

**Import and mirrors**
- Main has no import or mirror feature; greps found only `hooks::after_import` (`src/hooks.rs:71`), which refreshes size, languages and maintenance. — high
- Mirrors lane, reusable pieces (the worktree path is below):
  - `net::pin(url)` looks the host up once, checks every address, and returns a curl entry `host:port:ip,...` (`net.rs:85-102`).
  - `mirrors::git()` (`mirrors.rs:72-100`) sets `protocol.allow=never` with http/https only, `http.followRedirects=false`, low-speed limits and `-c http.curloptResolve=<pin>`. The credential goes in `GIT_CONFIG_COUNT/KEY_0/VALUE_0` as `http.extraHeader: Authorization: [REDACTED] so it never appears in argv or the URL.
  - `clean()` blanks the password and its base64 form in errors (`:112`).
  - A lease/`next_at`/`failures` queue with `backoff()` is in `:137-202`.
  - `exec` runs under a 30-minute timeout (`:126`).
- Import can call `mirrors::git(...)` then `git fetch --prune <url> '+refs/*:refs/*'`, then `hooks::after_import`. — high

**Data model (`src/db.rs`)**
- `users` (`:17`): `pass_hash NOT NULL`. An OAuth-only account needs an unusable hash. Email is unique and case-insensitive. `is_org` was added later (`:341`).
- `issues` (`:65`): one table for issues and PRs, `UNIQUE(repo_id, number)`, `author_id NOT NULL ... ON DELETE CASCADE`, `is_pr`, `state`, `created_at`, `updated_at`, `closed_at`, and `milestone_id` added later (`:695`). `pulls` (`:80`) holds branches, SHAs and `merged_*`. Numbers are allocated as `max(number)+1` (`src/api/issues.rs:255-272`), so an importer can insert explicit numbers.
- `comments` (`:90`): `author_id` NOT NULL, a `kind` column, and timestamps. `labels` (`:100`) has name and color, `issue_labels` joins them. `milestones` (`:681`) has a number, `due_on` as a date, state and timestamps.
- `releases` (`:370`): `author_id` is nullable, with `draft`, `prerelease` and `published_at`. Assets are files on disk plus a row (`:385`). Reviews and `review_comments` exist (`:500-511`) but are best deferred.
- Migrations are an append-only array applied via `PRAGMA user_version` (`:14,817`).

**External docs** (all primary tier; none of the pages shows a date, so treat them as current at fetch)
- GitHub web flow: https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps — authorize `https://github.com/login/oauth/authorize`, token `POST https://github.com/login/oauth/access_token`. "the `plain` code challenge method is not supported" (S256 only). Send `Accept: application/json`. The code expires after 10 minutes. — high
- GitHub scopes: https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/scopes-for-oauth-apps. `read:user`, `user:email` and `read:org` are read-only. `repo` gives full access to public and private repositories, including write. GitHub has no read-only private-repo scope, so say so in the UI. — high
- GitHub issues list: https://docs.github.com/en/rest/issues/issues#list-repository-issues — "GitHub's REST API considers every pull request an issue". Use the `pull_request` key, `state=all` and `since`. — high
- GitHub pagination: https://docs.github.com/en/rest/using-the-rest-api/using-pagination-in-the-rest-api — "For most endpoints, the maximum value of `per_page` is `100`." Follow `Link rel="next"`. — high
- GitHub rate limits: https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api — "All of these requests count towards your personal rate limit of 5,000 requests per hour." On 403/429, honor `retry-after`, else `x-ratelimit-reset`, else wait at least a minute. Secondary limits cap content creation and concurrency at 100. — high
- GitLab OAuth: https://docs.gitlab.com/api/oauth2/ — `/oauth/authorize` with `code_challenge` and `code_challenge_method=S256`, `/oauth/token` with `grant_type=authorization_code` and `code_verifier`. The verifier is "a random string, between 43 and 128 characters in length". Self-managed uses the instance hostname. Access tokens last 7200 s and refresh tokens rotate. — high
- GitLab scopes: https://docs.gitlab.com/integration/oauth_provider/ — `read_api` "Grants read access to the API". `read_user` is the profile. `read_repository` is git over HTTP. `openid`, `profile` and `email` cover OIDC. — high
- GitLab pagination: https://docs.gitlab.com/api/rest/ — offset is the default, `per_page` max is 100, "Be sure to use these links instead of generating your own URLs." — high
- OIDC discovery: https://openid.net/specs/openid-connect-discovery-1_0.html — fetch `{issuer}/.well-known/openid-configuration`. Required fields are `issuer`, `authorization_endpoint`, `jwks_uri` and (unless implicit-only) `token_endpoint`. `userinfo_endpoint` is only recommended. The issuer must match exactly, including the `iss` claim. — high

3. **Recommended design (14 bullets)**
1. Add `oauth_providers(id, kind github|gitlab|oidc, name, base_url, client_id, secret BLOB sealed, enabled)`, edited on an admin page. This is separate from `Settings`, whose GET returns everything.
2. Add `identities(provider_id, subject, user_id, email_at_link, UNIQUE(provider_id, subject))`. Match on the provider's stable subject id, never on email.
3. `GET /api/auth/oauth/{p}/start` creates state and a PKCE verifier, kept in a server-side store like `challenges`. It sets a short-lived HttpOnly cookie, and the callback requires both to match.
4. The callback exchanges the code on a blocking thread through a net.rs GET/POST that has `pin` and `PublicOnly` (admin-only `allow_private` for self-hosted GitLab). For OIDC, check `iss`, `aud` and `exp`. Prefer the userinfo call to JWT signature checks, which is the minimum needed.
5. Known identity: if the user has 2FA, return the existing `challenge`; otherwise call `create_session`.
6. Unknown identity, with `signup` allowed (`closed` and `invite` still apply): create the user with an unusable `pass_hash`. Choose a free username, and use the email only if the provider marks it verified (GitHub `/user/emails` primary+verified, GitLab `confirmed_at`).
7. Linking to an existing account: if the verified email matches, never auto-link. Require the user to log in locally with their password or a session plus sudo, then confirm. Also offer "connect" from account settings.
8. An OAuth-only account has no password, so keep `password_login` from accepting the placeholder hash. Admin and `suspended` checks stay unchanged.
9. Import tokens: use a separate "import" authorization requesting `repo`/`read_org` for GitHub and `read_api read_repository` for GitLab. Store them in `import_jobs.token` with `seal`. Delete the token when the job finishes, fails or is cancelled. Never write sign-in tokens down.
10. Tables `import_jobs(id, user_id, provider, host, source, state, error, created_at)` and `import_items(job_id, remote_id, repo_id, stage, cursor, error)`. A worker loop in the style of `mirrors::run` leases items so the import resumes after a restart.
11. Per-repo stages: create the row, `fetch` through `mirrors::git`, `after_import`, then labels, milestones, issues and PRs (with `state=all` and `since`), then comments and releases. Store the pagination cursor per stage.
12. Mapping: keep remote numbers explicitly, since GitHub shares one number space between issues and PRs, as Gitter does. Create `ghost` users such as `gh-<login>` with a locked state. Put "originally by @login" in the body and keep the original timestamps. Imported PRs become issues with `is_pr=1` plus a `pulls` row. Closed and merged states carry over; reviews do not.
13. Rate limiting: read `x-ratelimit-remaining`, `x-ratelimit-reset` and `retry-after`, and sleep or reschedule the item. Use one worker per token, follow `Link rel=next`, and cap `per_page` at 100.
14. Defer to a follow-up: reviews and inline comments, release assets, a generic "any git host" bulk listing (this is just URL-by-URL clone), refresh-token rotation, OIDC JWKS signature validation, and webhooks for continuous sync.

4. **Risks / surprises**
- The `net.rs` and `mirrors.rs` in the main tree lack `pin`. The worktree lane has not merged, so import depends on that branch merging first (T-0010 is the likely carrier). The main tree is also dirty, including `src/hooks.rs` and `src/db.rs`.
- `net::post` caps replies at 64 KB. Provider JSON lists need a GET with a larger limit, around 5-10 MB.
- Session-cookie SameSite=Lax is fine for the callback redirect. Verify `secure_cookies` behind a proxy.
- Email is "NOT NULL DEFAULT ''" with a unique-where-nonempty index. OAuth users without a verified email will have an empty email.
- Worktree `net::pin` does a DNS lookup then lets curl reconnect, which is pinned via resolve. Redirects are disabled for git only, so provider REST calls must keep `max_redirects(0)`.

5. **Not checked**
- Whether `Admin`, `Sudo` and `Fresh` extractors work for the new pages.
- `static/js` routing for the login page, and the existing tests and e2e harness.
- GitLab rate-limit pages and the GitHub or GitLab GraphQL APIs.
- GitHub's `/user/emails` and GitLab's `confirmed_at` details beyond what I recall.
- Release asset handling and quota on import (`storage_left`).

Worktree files: `/home/chase/projects/gitter/.claude/worktrees/agent-a7dd937083a8eb649/src/mirrors.rs` and `.../src/net.rs`.

**Noticed:**
- `net::post` truncates replies silently at 64 KB, with no way to tell a cut-off body from a complete one.
- `security::seal` has no AAD, so a sealed blob can be swapped between rows without detection.
- `get_settings` returns the full settings struct, so adding secrets there would leak them to the browser.
