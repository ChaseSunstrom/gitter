**Verdict: approve with nits.** I found no SSRF bypass, credential leak or unguarded ref-writing path. There are two MEDIUM issues (unbounded fetch size, remote URL shown to every reader) and a few LOW ones. I read the code and did not run `cargo test`.

## lens: ssrf
Result: ok, no bypass found.
- Non-numeric hosts are resolved once and every address is checked (`net.rs:85-101`). Git is pinned to those addresses with `http.curloptResolve`, and `ops.rs:505` already requires git 2.38 or newer, so the option is supported.
- `http.followRedirects=false` and `protocol.allow=never` (`mirrors.rs:76-90`) close the redirect and `ext::`/`file://` routes.
- Only `http://` and `https://` URLs pass `check_url` (`net.rs:30`), so a URL starting with `-` or `ext::` cannot reach git's argv.
- `@` in the authority is rejected (`net.rs:34`).
- Loopback, private, link-local, IPv4-mapped IPv6, 6to4, Teredo and NAT64 addresses are all refused (`net.rs:43-75`).
- Numeric forms such as decimal, octal and hex are refused because `getaddrinfo` turns them into loopback.
- `git.rs:68` clears the environment, so `http_proxy` cannot redirect the traffic.
- Private addresses are for site admins only (`api/mirrors.rs:294`, `:341`). Transfer and demotion both reset `allow_private`.

## lens: credentials
Result: ok, with one LOW.
- The password reaches git only through the `GIT_CONFIG_*` environment, never argv or the URL.
- Errors go through `clean()`, audit entries record the host only, and the API returns `has_password` instead of the secret.
- Mirror paths are blocked for API tokens (`api/mod.rs:418`).
- LOW — `src/mirrors.rs:115` — `clean()` only redacts passwords of 4 or more characters, so a 1–3 character password echoed by a remote is stored in `last_error` — redact any non-empty password. Confidence: high.

## lens: read-only enforcement
Result: ref-changing paths are covered; gaps are LFS and the default branch.
- Covered: HTTP and SSH pushes (both go through `smart_http::open`, `smart_http.rs:165`), `write_contents` (which MCP `commit_files` calls), merge and auto-merge (via `merge_with`), revert, suggestion apply, `create_branch`, `delete_branch` and `make_tag`.
- LOW — `src/lfs.rs:197,308` — LFS batch upload and object PUT only check `can_write`. A writer can store LFS objects in a pull mirror, which the README presents as read-only — add `refuse_writes` there. Confidence: high.
- LOW — `src/api/repos.rs:328,339` — `PATCH` of `default_branch` is not refused. It rewrites HEAD and the stored default, and sync's `fix_head` only repairs it when that branch is gone — call `refuse_writes` when `req.default_branch` is set. Confidence: medium.

## lens: abuse and exposure
- MEDIUM — `src/mirrors.rs:233,126` — the storage quota is checked only before the fetch, and nothing caps its size. Scenario: any user with an empty repo adds a pull mirror to a host they control that serves a very large pack. Git writes at full speed for up to 30 minutes, four fetches can run at once, and the temporary pack files are left behind when the timeout kills git. This can fill the server's disk. Fix: in `exec`, poll the size of the repo's `objects/` directory and kill git once it passes the owner's `storage_left`.
- MEDIUM — `src/api/repos.rs:221` with `src/mirrors.rs:325` — `show` returns `mirror.url` to everyone who can read the repo. Scenario: an admin-created mirror of `http://10.0.0.5:3000/x.git` shows that internal host to anonymous viewers, and a `?token=…` in any mirror URL is shown too. Fix: return only scheme and host, or the full URL to repo admins only, and hide it when `allow_private` is set.
- Test gap: no test shows git actually honouring `http.curloptResolve` or accepting a public hostname. Every test that reaches git sets `allow_private: true`, and `pins_only_public_hosts` only tests refusals.

## Acceptance criteria
- Pull and push mirror behaviour, credentials never exposed, private remotes refused: met in code, as reviewed above except the findings.
- Settings → Mirrors page and badge render without console errors: not verifiable from the code; `scripts/ui-check.mjs` has the check but I did not run it.

## Implicit decisions
- Any repo admin, not just a site admin, can create mirrors to public hosts.
- A mirror URL may carry a query string.
- Pull mirrors replace everything, forced and pruned.
- Mirrors of a suspended or hidden repo wait.
- Syncs poll every 5 seconds.

## Not checked
Merge-queue code, the JS rendering and escaping, and a live run of the tests.

**Noticed:** fetched packs are not verified (`transfer.fsckObjects` is not set), so malformed objects from a remote are accepted. Worth a hardening task.
