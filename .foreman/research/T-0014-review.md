**Verdict: approve with nits.** I found no CRITICAL or HIGH defects, and the access-control focus areas all hold. Two MEDIUM operational gaps should be fixed or captured before merge. I read the code only and ran nothing.

Held up under review:
- Mount checks the source repo's read access (`registry.rs:514-519`).
- Blob HEAD and GET are per-repo (`held`), so there is no cross-repo digest oracle.
- Every completion path checks the digest (monolithic POST, PUT, manifest, package).
- Upload ids are 128-bit, bound to user and repo, and hex-validated before any filesystem use.
- Image components, package parts and digests are strictly validated.
- Mounted blobs are charged to the mounter's quota.
- Trashed repos are invisible, and read-only and deploy tokens are narrowed by `access()`.

## lens: adversary
- MEDIUM — `registry.rs:34`, `registry.rs:194`, `auth.rs:339` — Failed-password limiting is keyed per username (`basic:{name}`), and the registry allows 1200 requests per minute per IP bucket, against 120 for git. Rotating usernames makes each request run a full argon2 verify, about 20 per second per IP. A /48 gives 65k IPv6 /64 buckets. — Add a per-IP failed-auth counter in `open` and `ping_inner`, or lower the limit for password (non-token) requests. — medium
- LOW — `registry.rs:782` — Tags and image sub-names are unbounded per repo. A writer can create unlimited rows, and `list` silently truncates at 2000. — Cap tags and names per repo. — high
- LOW — `registry.rs:705` — `to_bytes` on a manifest PUT has no stall timeout and takes no Slot, unlike the other upload paths. — Wrap it in a timeout. — medium

## lens: edge
- LOW — `registry.rs:733-736` — The quota pre-check rejects a manifest PUT when remaining room is below the manifest size, even if the manifest is already held. At or over quota, re-tagging an unchanged image fails with 413. — Drop the pre-check, since the in-transaction `added` check already covers it. — high
- LOW — `registry.rs:739` — If the manifest file already exists, the write is skipped. A concurrent `collect` can then delete it before the rows commit. The transaction checks rows, not files, so `get_manifest` returns "file is missing" until the client re-pushes. The `place` rename plus `link` window has the same shape. — Always write via temp and rename, as `place` does. — low
- LOW — `registry.rs:429` — The artifact manifest type is accepted, but `references` needs `config` or `layers`, so it is always rejected as MANIFEST_INVALID. — Remove it from the allow-list or handle `blobs`. — high
- LOW — `lib.rs:151` — The gzip layer compresses manifest JSON, and tower-http drops Content-Length when it does. HEAD requests from non-Go clients that send Accept-Encoding lose their length. — Exclude registry content types from compression. — medium

## lens: operator
- MEDIUM — `registry.rs:782`, `registry.rs:1000-1003`, `registry.rs:802` — Re-pushing `:latest` leaves the old manifest in `registry_manifests`, and nothing deletes manifests except DELETE by digest. Its layers count against the quota forever. Untagged digests are not listed in the Packages tab or tags/list, and blob DELETE returns 405. A user can hit quota with no way out except deleting the repo. — Sweep untagged manifests that no index references and that are older than GRACE, and offer deletion in the UI. — high
- LOW/MEDIUM — `registry.rs:361-367` — The anonymous ping always returns a Basic 401 challenge. containerd and kubelet refuse to proceed without credentials on a Basic challenge, so I expect anonymous pulls of public images to fail for them (not verified here). The e2e only tests anonymous access with curl. — Document it, or add a token service later. — medium

## lens: maintainer (merge)
- MEDIUM — `db.rs` — The migration is v15 here and v15 on main (code review), so they will conflict. Renumber the registry one after main's. — high
- MEDIUM — `lib.rs:121-123` — `/v2` bypasses `scope_guard`, so main's agent and MCP token restrictions will not apply, and `user_by_basic` will accept the new token type. Re-check the policy after merge. — medium
- Other likely conflicts: the `purge` and `delete_account` tuples in `lifecycle.rs`, the `STORAGE_USED` constant, the `DATA_DIRS` entry in `ops.rs`, the end of `tests/api.rs`, and the route and tab lists.
- LOW — Existing accounts or orgs named `v2` would be shadowed by the new route. `RESERVED` only blocks new names. — Add a doctor check. — medium

## Acceptance criteria
- **AC1 (`registry` tests):** met on reading. The tests cover chunked and monolithic uploads, digests, tags, private repos, quota and repo deletion.
- **AC2 (e2e and backup scripts):** not verifiable here. The docker, podman and skopeo path is skipped when the client is absent, and I did not run the scripts. The Packages tab check sits in `ui-check.mjs`, and the backup test is in `ops.rs`.

## Implicit decisions
- Basic auth only, no token service.
- Only sha256 digests.
- Per-repo blob rows over a global file store.
- Manifest bytes stored as blobs.
- One-hour upload staleness, 24-hour blob grace, sweep every hour.
- Blob size limit is `max_lfs_mb`.
- Quota is charged to the repo owner's payer.
- DELETE by tag leaves the manifest.
- No blob DELETE and no Range on GET.
- Packages overwrite on the same name, version and file.
- Any write-scope token can publish packages.

## Not checked
- Real docker, podman or skopeo runs.
- Sweep performance with large tables.
- Org-deletion cleanup paths.
- `web::security_headers` beyond the passing CSP test.
- Whether the user-supplied `from` names in `link` are rate-limited.

**Noticed:** Repo names that are valid in Gitter but not valid OCI components (`a..b`, `a-.b`) cannot use the registry, yet the Packages tab shows a pull command for them. `foo.git` also resolves to repo `foo`, because `lookup` strips the suffix.
