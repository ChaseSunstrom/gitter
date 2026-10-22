**Verdict:** approve with nits. I found no privacy leaks, no injection and no access holes. The migration conflict with main needs fixing at merge time. I read the code and did not run any tests.

## lens: adversary

- MEDIUM — `src/api/insights.rs:217,346` — `graph` runs `rev-list --topo-order --branches -n10001` and `history` runs `log --follow` with no cache and nothing narrower than the global git permit. On a large repo, a path touched once makes `--follow` walk all of history before it returns. The per-IP API limit is 1200 requests a minute, and each request can hold a git permit for up to 30s. A few clients on a public repo could use up the permits, so pushes and every other git endpoint get 503 "busy". Fix: cache graph pages and history by (repo id, sha of the branch heads, page) as `last_commit` does, and use a small separate semaphore for these two routes, or `permit_stream`. — confidence medium (not benchmarked).
- MEDIUM — `src/api/insights.rs:454-458,505-509` — The 100 name slots per repo per day go to whoever posts first. Any reader can POST 100 junk paths or referrer hosts, such as spam domains, and the real top lists stay empty for that day. The `path` field accepts any string starting with `/`. Fix: when the cap is hit, replace the lowest-count row instead of refusing the new one. Also accept only paths whose first segment is a known app route. — confidence high.
- LOW — `src/api/mod.rs:316` — `ends_with("/traffic/view")` matches more than the view route. The router strips the `/api` prefix, so `PATCH` or `DELETE /repos/traffic/view` (user `traffic`, repo `view`) also skips the per-account write limit. Fix: match exactly, with 5 segments and `segs[3]=="traffic"`, POST only. — confidence high.
- LOW — `src/api/insights.rs:408` — At 100,000 pending entries all counts are dropped, including clones, until the next flush. The flush then runs up to 100,000 statements on the single DB connection. A flood would need many distinct IP buckets, or `api_rate_per_min=0`. — confidence low.

Checked and fine:
- **Git arguments:** `path` follows `--`, and `GIT_LITERAL_PATHSPECS=1` is set in `git.rs:80`, so `:(top)`, globs and leading `-` are inert. The `rev` is validated and then resolved to a sha.
- **View route:** it goes through `Repo::readable`, so it returns 404 for private, trashed and hidden repos. CSRF is covered by the `web.rs` origin check, and the route is bounded by the per-IP API limit.
- **Traffic read:** `administrable` is used, so site admins, org owners and admin collaborators are included.
- **Privacy:** referrers are cut to a validated host. No raw IP is stored.

## lens: edge

- LOW — `src/api/insights.rs:299,350` — If the output cap cuts a record between a commit and its status line, the version is kept as "modified" with the wrong path. If the cap trims `git show`, commits silently vanish from the graph page (`filter_map` at line 236). Fix: return a `truncated` flag.
- LOW — `static/js/pages/history.js:937,924` — `?page=abc` gives NaN, so the server returns 400. On page 2 the label reads "version X of N" with X and N relative to that page, not the whole history. Fix: clamp the page number, and use the page offset in the label.
- LOW — `static/js/pages/insights.js:82` — `.catch(() => null)` makes the traffic panel disappear with no message on a 500. Fix: show an error line for the panel.

## lens: operator / maintainer

- MEDIUM — `src/db.rs:666` — Main already has a `v15` code-review migration, and this branch also appends `v15` at the end of the array. Resolve the merge by keeping main's migration first and putting traffic second, as `v16`. Swapping the order would make deployed databases (at `user_version` 15) skip a migration.
- LOW — `tests/api.rs:1298` — The `LIKE '%unknown%'` check on a hex hash can never fail. Assert instead that no stored hash equals `sha256_hex(ip_bucket(ip))`, and that the same address gives a different hash on a different day. The traffic test could also flake if it runs across UTC midnight.

## Acceptance criteria

1. **Met** — `cargo test --test api insights` shows 3 passed in the task evidence. The tests cover lane continuity across pages, rename following, admin-only traffic and 404 for private repos.
2. **Not verifiable** — I did not run `scripts/e2e.sh` or `ui-check`.

## Implicit decisions

- Time is UTC days.
- Visitors are identified by a hash of the `ip_bucket` (IPv6 /64) and a salt held only in memory. The hash is truncated to 128 bits.
- Referrers are host-only, with the site's own host dropped.
- Graph and history stop at 10,000 commits, and the graph covers branches only (not tags).
- Admins' own views are counted.
- Fetches count as clones.
- Traffic is kept for 14 days.

## Not checked

I did not run any tests, benchmark the git commands, read the commit-diff endpoint behind the scrubber, or read main's code-review changes.

**Noticed:**
- The history page for a directory path (`--follow` on a dir) gives odd status pairing. No UI link leads there.
- `referrer_host` treats an alias host that differs from `cfg.url` as an outside referrer.
