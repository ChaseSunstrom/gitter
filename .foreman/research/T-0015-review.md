**Verdict: changes needed.** The temp-file guard, the slow-upload check, the `Cache-Control` fixes and the batch limiter are sound. The storage-cap finding is not fully closed. I did not run anything; all findings come from reading the code.

## lens: intent

- HIGH — src/api/repos.rs:341 and src/lfs.rs:372 — Deleting a repo cascades its `lfs_objects` rows, so `STORAGE_USED` drops at once. The files stay on disk until the next boot, because `lfs::sweep` is only called at src/main.rs:143. One account can loop "create repo, PUT a unique 1 GB object, `DELETE` the repo". The quota never fills and the disk grows without limit between restarts. `delete_user` (src/api/admin.rs:273) has the same gap. This is the original "no cap on total LFS disk" finding, still open. — Collect right after repo and user deletion: gather the repo's oids first, then remove the files that no row holds any more (keep the age guard or accept the tiny rename-to-insert window). Also run `sweep` on an hourly interval. — high
- MEDIUM — src/lfs.rs:296 — The quota is read once before streaming and never checked again at commit. With `left` = L, 8 parallel PUTs of distinct L-sized objects all pass, giving 8L stored per IP bucket, and more across buckets. The slot cap limits the factor but does not enforce the quota. — After the rename, inside the `INSERT` `db.call`, recompute the owner's usage. If it is over quota, delete the row and the file (when no other row holds it) and return 413. Or keep a per-owner in-flight reservation. — high
- LOW — src/repo.rs:147-148 — The old doc line "Bytes an account may still store…" now sits above `STORAGE_USED`, next to a second doc line. — Move it back onto `storage_left`. — high

## lens: adversary

- MEDIUM — src/api/repos.rs:414 — The fork check compares only the source's `size_bytes` to `left`. The fork then copies the `lfs_objects` rows (line 453), so the forker is charged for LFS bytes that were never checked. A user with 0 MB left can fork a repo with 2 GB of LFS and go far over quota. No new disk is used, but the quota is bypassed and a fork can hold objects after the source is deleted. — Add the source's LFS bytes that the forker doesn't already hold to the size in that check. — high
- LOW — src/lfs.rs:243-250 — A download holds its slot correctly: the closure owns `slot` and the stream drops with the body. But LFS downloads have no send timeout, while smart_http has `SEND_TIMEOUT`. A client reading 1 byte/s keeps a slot, an fd and a 2 GB file handle open indefinitely. — Add a per-write timeout to the mapped stream, or wrap it with a deadline. — medium
- LOW — src/api/code.rs:356 — `/raw` streams the LFS object through `object_body` without taking a `Slot`, so it bypasses the transfer cap. — Take a slot there too. — high
- LOW — src/lfs.rs:323 — `too_slow` counts total bytes, so a 100 MB burst buys about 7 hours of one chunk per minute (the stall timer only needs a chunk every 60 s). — Use a sliding window, or accept the ceiling and document it. — medium
- Admins and `quota == 0` skip the quota in `storage_left`. Anyone with write access to an admin-owned repo is limited only by the per-file maximum. This is by design; I report it only as a known exposure.

## lens: edge

- MEDIUM — src/lfs.rs:218-220 — The size check comes before the `have` check. A user close to quota who re-announces an object the repo already holds (a new branch, `git lfs push --all`) gets 422 "only N MB left" for an object that needs no upload, so the push fails. Example: 400 KB left, a held 600 KB object. — Test `upload && have` before the size check. The same applies to objects held by another repo of the same owner, which are charged zero because of `DISTINCT`. — high
- LOW — src/lfs.rs:376-399 — The sweep does synchronous directory walks and one query per file on the DB thread, which stalls all DB work at boot for a large store. It also checks the age and row, then unlinks. If an upload renames the same oid in between, the unlink removes a file whose row has just been inserted. `held` then reports false and the client re-uploads, so it self-heals. Empty `ab/cd` directories are never removed. — Walk the files outside `db.call` and only query inside it, or batch the queries. — medium
- LOW — src/smart_http.rs:24 and src/lfs.rs:75 — The slot limit of 8 equals git-lfs's default of 8 concurrent transfers. Any other transfer from the same bucket (a second user on the NAT or IPv6 /64, a clone) gets 429 with `Retry-After: 5`. The limit is also taken before authentication. — Give LFS transfers their own bucket, or raise the limit. — medium

**Checked, no problem:**
- `DISTINCT l.oid, l.size` is correct. The size comes from the hash-verified upload and is copied by forks, so one oid has one size.
- The sweep cannot delete a fork's objects, since forks hold their own rows and the sweep tests any row by oid.
- Mid-upload objects are safe: a temp file's mtime is its last write and stalls last 60 s at most, and a renamed object keeps that fresh mtime.
- The `Tmp` guard covers every early return and cancellation. The only leak left is rename-then-insert failing or being cancelled, and the sweep reclaims that.
- The `Cache-Control` changes and the `pending` flag are correct.

**Not checked:** I did not run the test suite, the `ops.rs` backup and restore paths, or client-IP trust configuration.

**Noticed:** `delete_user` (src/api/admin.rs:273) and repo deletion have no LFS cleanup hook.
