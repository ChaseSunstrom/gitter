**Verdict: approve with nits.** I found no CRITICAL or HIGH defect. I read the code but did not run it: I have no shell here, so I did not run `cargo test`, and I took the two passing `ref_clash` runs from the brief's evidence log. I compared against the main tree for `releases.rs`, `repos.rs` and `code.rs`, which are unmodified there. `issues.rs` is modified in main.

## lens: correctness
- LOW — src/git.rs:681-684 — `create_ref` turns any `update-ref` failure where no blocker is found into a 500. `create_branch` used to answer 409 for every failure. A transient `.lock` held by a concurrent push, or a blocker deleted between the failure and the lookup, now gives a 500. Reproducer: hold `refs/heads/x.lock`, then POST /branches `x`. Fix: if stderr matches "Unable to create .*\.lock" or "cannot lock ref", answer 409 "ref is busy, retry". — 60%
- LOW — src/api/issues.rs:1475-1477 — any `update-ref -d` failure is swallowed with `.is_ok()`, not only the intended "head moved" case. A lock error leaves the branch undeleted and unlogged. Fix: `eprintln!` the error unless stderr says the ref moved or is gone. — 70%
- Prefix and extension logic is correct. `skip(2)` leaves the `refs/heads` folder out and the parent prefixes in. The `for-each-ref` pattern match is exact-or-up-to-a-slash, so `v31` is not a clash. `refs/heads` and `refs/tags` are kept apart by the full path. `for-each-ref` reads packed refs. Case-insensitive filesystems don't apply on Linux and I did not review them.
- The check and the create don't race. `update-ref` with the zero OID is atomic, and the lookup runs only after a failure.
- The merge path is correct. `update-ref -d ref <head>` is atomic against pushes, and `head` is the sha the merge gate approved.

## lens: performance
- LOW — src/git.rs:689-697, src/api/code.rs:1042 — `ref_in_way` lists every ref under each parent prefix, up to 8 MB. `code.rs` now runs it on every web commit to a new branch, where it used to be one `rev-parse`. Reproducer: 10k `feature/*` branches, then a "propose changes" commit onto `feature/y`. Fix: in `code.rs`, skip the pre-check and let `create_ref` or `commit_files` report the clash. Alternatively check the parents with `cat-file --batch-check` and the exact ref and its children with `for-each-ref --count=1`. I took no measurements. — 55%

## lens: security
- No findings. Ref names are validated to `[A-Za-z0-9._/-]` and always start with `refs/`, so there is no option injection. The blocker name comes from the same namespace the caller can write to, so it leaks nothing, and the JS has no `innerHTML` use of error text.

## lens: silent-failures
- LOW — src/api/releases.rs:302-304,400-402 — `announce_tag` now runs after the DB save, so the old early `mirrors::changed` in `make_tag` is gone. If the client disconnects after the row commits, axum drops the handler: the release exists but no webhook or mirror goes out. `land_merge` spawns its critical section for this reason. Fix: `tokio::spawn` the announce. — 45%
- The `land_merge` item above also belongs here.

## lens: tests
- MEDIUM — tests/api.rs:2658 — nothing covers the new branch-moved-during-merge rule, the one non-trivial new logic. `land_merge` is `pub(crate)`, so `tests/` can't reach it. Reproducer: revert the `head` argument to `update-ref -d` and the tests still pass. Fix: add a unit test in `issues.rs` that pushes to the head branch between `prepare_merge` and `land_merge` and asserts the branch survives with no deletion webhook. — 75%
- The two `ref_clash` tests are not vacuous. They assert 409, the blocker name with a trailing space, the sibling and `v31` non-clashes, exactly 2 and 3 push events (once only), and that the pusher is forgotten.

## Acceptance criteria
- 409 naming `refs/heads/v3`: met. This is in the test and the code path, not run by me.
- Merge deletion and web tags run the push hooks: met for webhooks and the pusher, which the tests assert. CI and mirrors run via `on_push`, but no test asserts them, and CI ignores tags and deletions as it does for pushes.

## Implicit decisions
- The 409 message wording changed to "refs/heads/v3 already exists" from "A branch with that name already exists". No test or JS references the old text.
- Draft releases are not checked for clashes until they are published.
- The 8 MB cap on the ref list is silent.

## Not checked
Mirrors internals, the merge-queue path (it passes `delete_branch=false`), and case-insensitive filesystems.

**Noticed:**
- Main's uncommitted `land_merge` (src/api/issues.rs:1474) adds `deploy::pull_closed`, which this branch's hunk overlaps. Keep that call when integrating.
- A draft release with a clashing tag name (`v3/rc` while `v3` exists) is created fine and only fails with a 409 at publish. Check it at creation.
