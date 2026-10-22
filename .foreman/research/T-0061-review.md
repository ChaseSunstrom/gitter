**Verdict:** approve with nits. The builder's two claims check out, and I found no data-loss path in normal operation.

I had no Bash, so I read the worktree files instead of running the diff.

**Deviation on tag removal: right.**
- A failed insert deletes the new tag only when no release row exists for that repo and tag.
- `make_tag` creates the ref with `update-ref` against `EMPTY_OID`. A concurrent creator that loses that race gets the 409 and never reaches the insert. The only race winner that can hold our tag is one that saw the tag already there, skipped making it, and inserted first.
- Deleting the tag then would orphan that winner's release, so keeping it is correct.
- If the held-check query errors, `.unwrap_or(true)` keeps the tag, which is the safe choice.
- A small window remains between the held-check and `update-ref -d`. It needs a DB insert failure and a racing create at the same moment. I'm not reporting it as a finding.

**Sweep against an in-flight upload: verified safe.**
- `Db::call` runs under a single `Mutex<Connection>` (`src/db.rs:960`).
- `upload` does the row insert, `rename` into `releases/<rid>/<aid>`, and `tx.commit()` inside one `call` (`releases.rs:462-490`).
- `sweep` also runs inside one `call` (`releases.rs:151-164`), so the two cannot interleave. The sweep never sees a file whose row is uncommitted.
- A crash after the rename but before the commit leaves an orphan file that the sweep removes.
- `releases/tmp` isn't numeric, so `orphans` skips it.

## lens: adversary
- MEDIUM — src/api/releases.rs:156 — `sweep` runs `remove_dir_all` and `remove_file` while holding the DB mutex. Every API request stalls while a deleted release with large assets is unlinked. — List the orphans under the lock, then delete after `call` returns. This is safe because `releases.id` and `release_assets.id` are AUTOINCREMENT (`db.rs:368,386`). An orphan path can never become valid, and an upload only writes files for new ids that aren't in the list. — 60%

## lens: edge
- LOW — src/api/releases.rs:279 — `refname.trim_start_matches("refs/tags/")` strips the prefix repeatedly. `validate::ref_name` allows a tag named `refs/tags/a`. In that case the held-check queries tag `a`, finds no row, and deletes a tag a racing winner holds. — Clone `tag` before it moves into the insert closure, or use `strip_prefix` once. — 85%

## lens: operator
- LOW — src/api/releases.rs:148, src/ops.rs:235 — "Backups never miss a file" is overstated. `backup` snapshots the rows with `VACUUM INTO`, then copies files for as long as the data takes. If a release is deleted after the snapshot and the sweep fires before that file is copied, the file is missing. The backup prints "missing" and carries on, so the restored DB has a dangling asset row. Repo purge and account delete (`lifecycle.rs:345,537`) still remove files immediately. — Have `sweep` skip while a `.backup-tmp-*` directory exists in `data_dir`. `tempdir_in` creates it before the snapshot and removes it after (`ops.rs:167,259`). — 75%

`scripts/backup-roundtrip.sh` never deletes a release, so deferred deletion doesn't affect it.

## lens: intent / maintainer
- The reconcile and doctor changes, the error logging, the DB-only lookups and the 409 mapping all match T-0061. The tests cover orphan dirs, deferred deletion, and the tag taken back after a failed insert.

**Acceptance criteria**
- Reconcile and doctor report or remove orphan `releases/<id>` folders: met (`ops.rs:650,706`).
- Removal failures are logged: met.
- A just-made tag is taken back when the insert fails: met, with the deviation judged correct.
- `make_tag` maps only ref-already-exists to 409: met.
- `upload`, `update` and `delete_asset` look the release up DB-only: met.
- File deletion is deferred to the hourly sweep: met for release and asset deletes. Purge and account delete still delete immediately.

**Implicit decisions**
- The sweep runs at boot and then every hour.
- Doctor reports stray folders as OK rather than as a problem.
- Reconcile removes only whole orphan folders, not stray files inside live release folders. The sweep handles those.
- Non-numeric names under `releases/` are ignored.
- Deleting a release frees its quota immediately but its disk space only at the sweep.

**Not checked:** I didn't run the tests, and I didn't read the rest of `lifecycle.rs`.

**Noticed:**
- `releases.rs:365-378`: publishing a draft makes the tag with `make_tag`, and if the later `UPDATE` fails the tag is left behind. It is the same bug as the create path, in `update`.
- `releases.rs:321-325`: a ref conflict like `v3` blocking `v3/rc` returns 500 where a 4xx is better, and a test pins that 500.
