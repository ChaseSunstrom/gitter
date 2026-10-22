## Verdict
Approve with nits. I found no security defect. I did not run anything: the review is from reading the diff and the worktree, so the passing tests recorded in the task are taken as reported, not re-run.

## Findings

## lens: intent
- LOW — task evidence — the task's second criterion (`bash scripts/e2e.sh && bash scripts/backup-roundtrip.sh`, plus the `ui-check.mjs` additions) has no recorded run. Only `cargo test` runs are logged. — Run both scripts and `ui-check` and attach the evidence before closing. — high

## lens: adversary
- None. These all held up when I traced them:
  - **Private repos:** every handler goes through `Repo::readable` or `writable`, so a private repo gives 404.
  - **Limited tokens:** read-scope tokens are refused by `access()` and `scope_guard`, and a deploy token for a different repo is refused too.
  - **Drafts:** they are filtered in `load`, `list`, `download`, `summary` and `latest`. A draft's new tag is only created at publish.
  - **File names:** they never reach a path. Files are stored by `release id/asset id`, and `asset_name` allows only `[A-Za-z0-9.+_-]`.
  - **Downloads:** the content type is always `application/octet-stream`, with `attachment`, the sandboxed CSP and `nosniff`.
  - **Quota:** the in-transaction recount after the insert (`releases.rs:596-603`) closes the parallel-upload race, and the test covers it.
  - **Cross-site writes:** the CSRF origin check covers the new routes.
  - **Tag objects:** `mktag` input is built only from validated tag, sha and filtered tagger name.
  - **Temp files and cleanup:** the temp-file guard cleans error paths, and repo and user deletion remove release files.

## lens: edge
- LOW — `src/api/releases.rs:483-485` — a draft made on an existing tag stores `target=''`. If that tag is later deleted by a push, publishing calls `check_target("")` and silently creates a new tag at the default branch tip, which is not the commit the author meant. — If `target` is empty and `sha` is null, return 409 "the tag no longer exists". — medium
- LOW — `src/api/releases.rs:409-426` — a non-draft create makes the tag before the DB insert. If the insert fails (a unique-constraint race against a concurrent draft), the tag stays with no release. — Accept and document, or delete the tag you just created on failure. — low

## lens: operator
- MEDIUM — `src/ops.rs:181-190` — the comment says release files are "never rewritten", but they are deleted immediately, unlike LFS objects, which are only removed at the next start. If an asset is deleted between the DB snapshot and the file copy of a long backup, the code warns and continues. The archive then holds a row with no file. `verify` flags it, and a restore leaves a dangling row. — Defer file removal (an LFS-style sweep), or fail or retry the backup when a snapshot file is missing. — medium
- LOW — `src/ops.rs:185-189` — `is_file()` followed by `append_path_with_name` aborts the whole backup if the file vanishes between the two calls. — Use the existing `append_tree` helper, which tolerates NotFound. — high
- LOW — `src/api/releases.rs:118,661` — `let _ = remove_dir_all` and `remove_file` hide failures. `reconcile` never sweeps `releases/<id>` folders that have no row, so a failure or a crash leaks disk without any signal. — Log with `eprintln!` as `repos::remove` does, and have `reconcile` or `check` report orphan release folders. — medium
- LOW — `src/api/releases.rs:560,474,656` — `upload`, `update` and `delete_asset` call `load`, which runs `git for-each-ref` (`add_shas`) just to count assets or check existence. That is one git subprocess per upload. — Use a DB-only query for these three. — medium

## lens: maintainer
- LOW — `src/api/releases.rs:453-456` — `make_tag` turns every `update_ref` failure into "just created by someone else", so a real git error (for example an unreadable repo) is misreported as a race. — Return the 409 only on the ref-exists case. — low

## Acceptance criteria
- **Create, edit, publish, assets, quota, download counts and latest via the API** (`cargo test -q --test api releases`): met. The task log shows these tests passing, and the tests cover each part.
- **Pages and sidebar render, backups include assets:** not verifiable from the code. The e2e, backup-roundtrip and ui-check edits look right, but there is no run evidence.

## Implicit decisions
- Content type is stored but never served; downloads are always `octet-stream`.
- Draft tag creation is deferred to publish.
- Publishing a draft sets `published_at` to now; making a release a draft clears it.
- Asset names are lossily sanitized, so non-ASCII characters become `.`.
- 1000 assets per release and 20 releases per page.
- The audit log records only release deletion.
- Compression is now off for all `application/octet-stream` responses, including raw files.

## Not checked
- The markdown sanitizer (`md.js`).
- `Slot` and rate-limit behaviour beyond reading.
- Collaborators and orgs (T-0008 is on main, not on this base).
- The CI artifact path.
- I ran no tests or scripts.

**Noticed:** On merge with main, re-verify releases against the collaborator and org access rules (T-0008), including `Repo::writable` and the org-deletion paths, which must also call `releases::ids_of` and `remove_files`.
