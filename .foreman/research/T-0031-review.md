**Verdict: changes needed.** Two verified scope escapes. The smart HTTP, `/raw` and `/archive` routes are fine, and so are the path tricks.

## lens: adversary

- HIGH — `src/api/account.rs:656` — `feed` returns all of the owner's repositories, private ones included, to a repo-limited token. Scenario: a deploy token for public or private repo A calls `GET /api/feed`. `token_may` allows the GET, `Viewer` returns the owner with `limits.repo_id = A`, and `mine = repo_cards("WHERE r.owner_id = ?", u.id)` skips `VISIBLE` and `visible_params`. The `repos` array lists private repo B's name, description, language and stars. The `visible_params` fix covers only the queries that go through `VISIBLE`. Fix: build `mine` with `VISIBLE` and `visible_params(Some(&u))`, or use the anonymous branch when `u.limits.repo_id` is set. Confidence: high (read, not run).

- HIGH — `src/api/mod.rs:163-176` with `src/api/issues.rs:218,483,569,593,699` and `src/api/repos.rs:350,372` — a write-scoped deploy token reaches repos other than its own. `token_may` checks only the shape `/repos/*/*/{thing}`, not which repo. `issues::create`, `comment`, `update`, `edit_comment`, `delete_comment`, `create_pull`, `star` and `unstar` gate on `Repo::readable`, not `writable`. For another repo B, `access()` only blocks private repos. Scenario: a deploy token with `write` for A sends `POST /api/repos/{anyowner}/{publicB}/issues`, `/issues/1/comments`, `/pulls` or `/star`. The action is recorded as the owner's account on any public repo, and `star` is an account-level change. Fix: in `scope_guard`, when `repo_id` is set and the method is not GET, resolve `{owner}/{repo}` from the path and require its id to equal `repo_id`. Or add a `Repo::acting(viewer)` check that returns 403 when the limits' repo differs, and use it in those handlers. Confidence: high (read, not run).

- LOW — `src/api/account.rs:268` — `GET /api/me` is allowed for every token, so a deploy token reads the owner's email, bio and display name through `profile`. Fix: blank `email` in `me` when `u.limits.is_some()`, or add `/me` to the account-only routes with an exact match.

- LOW — `src/api/mod.rs:172` — a token with scope `all` plus a `repo` set (accepted by `create_token`) can never write. `token_may` requires `Scope::Write`, while `access()` would allow the write. It fails closed, so the API contradicts the form and the doc. Fix: allow `Scope::All` there as well, or reject `all` with a repo at `account.rs:462`.

Checked and fine:
- `/raw`, `/archive` and smart HTTP all go through `Repo::load` and `access()`. Read-scoped tokens can't push, and deploy tokens get 404 on other private repos.
- Encoded slashes, trailing or double slashes and case all fall through to the 404 fallback. `ACCOUNT_ROUTES` matches exact paths and `/…/` prefixes, and axum matches the raw path.
- Writes with 3 segments are blocked: `PATCH` and `DELETE /repos/o/r`, and `POST /repos`.
- `fork` and `deploy-tokens` are excluded by name.
- Deploy-token creation and deletion need a `Sudo` session plus `Repo::writable`, so a token can't create tokens.
- `explore`, `search`, `profile` and `user_stars` use `visible_params`, which treats a repo-limited token as signed out.
- Tokens are cascade-deleted with the repo, and `AUTOINCREMENT` stops id reuse.

## Acceptance criteria
Not run (read-only). Reading the code, "read tokens can't push", "expired tokens stop working" and "repo-limited tokens can't read other private repos" hold for repo access. The feed endpoint breaks the third one, and the repo-limited write case is only partly met (second HIGH).

## Implicit decisions
- A token with unknown stored scope text falls back to `Read` at `auth.rs:255`.
- A token with scope `all` and no repo is treated as unlimited.
- Writes are allowed only under `/repos/{o}/{r}/…`.

## Not checked
The JS files, the tests, and the build. I did not run the code to confirm the feed leak.

Noticed: none
