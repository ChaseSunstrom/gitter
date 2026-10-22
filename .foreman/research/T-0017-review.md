**Verdict: changes needed.** I found no way for a non-admin to merge a PR whose required checks haven't passed on the exact landed commit. The `land_merge` compare-and-swap and the per-pass checks hold. The problems are fail-open spots around that, plus two queue-skip and leak paths. I couldn't run anything, so none of these were run; each comes from reading the code.

## lens: adversary
- MEDIUM — src/protect.rs:104,114 — `merge_queue` doesn't imply `require_pr`, so a write collaborator can skip the queue and its checks. With a rule `{require_pr:false, merge_queue:true}`, `protect::direct` and `check_rules` allow direct pushes and web commits. The new test does exactly this: bob commits to `main` at tests/api.rs:1183-1184 and gets 200. — Fix: `a.require_pr |= a.merge_queue` in `effective`, or reject the combination in `create_rule`/`update_rule`; adjust the test to move the base another way. — high
- MEDIUM — src/smart_http.rs:465 — `ref_map` runs an unfiltered `for-each-ref`, so `refs/gitter-queue/*` shows up in `after_push`. A queue ref created or deleted during someone's push becomes a `RefUpdate`. `webhooks::push` (src/webhooks.rs:246) then sends it, leaking the hidden test sha and its ref name. — Fix: filter `REF_PREFIX` out of `ref_map`. — medium
- LOW — src/mergequeue.rs:103-121 — `dequeue` doesn't take `LOCK`. Dequeuing while `step` is at `land_merge` returns 200 and the PR still merges. `merge_queue.id` has no AUTOINCREMENT, so a stale `e.id` can also delete a re-added entry's row. — Fix: take `LOCK` in `dequeue`, and use AUTOINCREMENT. — medium
- Agent/secrets: no finding. The third COALESCE term in the agent-mark SQL (src/ci/trigger.rs:111-113) marks queue runs through `pr_number`, and agents have no queue tool in MCP.

## lens: edge
- MEDIUM — src/mergequeue.rs:319-321 — an `on_queue` error is only printed, and the entry is already `testing`. If the rule names no checks, `verdict` sees no checks and returns `Pass`, so the PR lands with no CI run, e.g. after a transient `pipeline_at` or `start` error. — Fix: on `Err`, reset the entry (state `queued`, `tested_base` NULL) or eject it. — high
- LOW — src/mergequeue.rs:229,310 — `step(..)?` aborts the whole pass. A persistent error on a front entry (e.g. `update-ref` failing at line 310) never ejects it, and it blocks later entries in the same repo. — Fix: eject or time out on error. — low
- LOW — src/smart_http.rs:212 — `hideRefs` keeps external systems from seeing the test commit. Required checks that come from outside status contexts never arrive, so the entry is ejected after an hour and holds the line meanwhile. `ff` entries behind another one also always eject, from `prepare_merge`'s "Can't fast-forward". — Fix: document it, or fail fast when a required check can't come from CI. — medium
- LOW — src/hooks.rs:21 — every push in every repo spawns `advance`, which takes the global lock. The same happens at src/api/checks.rs:162 and src/ci/jobs.rs:316 for every status and job finish. — Fix: a cheap `EXISTS` on `merge_queue` for the repo before taking the lock. — medium

## lens: intent
- LOW — src/api/issues.rs:1087 with src/protect.rs:259 — `can_bypass` needs a non-empty `missing`. An admin on a green PR can't skip the queue, and with a queue-only rule never can. README line 15 says admins "may still merge directly", and no test covers the admin skip. — Fix: compute the bypass from the queue condition, or reword the README, and add a test. — medium

## lens: operator
ok. The README matches the code except the admin-skip claim above. The `ui-check.mjs` console.log lines are the script's normal output, not leftovers.

## lens: maintainer
No findings beyond the missing admin-skip test above. The `MergePlan`/`land_merge` split reuses the existing merge path without duplicating it.

## Acceptance criteria
- Queue criterion (combined commit, order, failure ejects with a comment, retest, removal, readers blocked): met. tests/api.rs:1088-1228 covers each case; I did not run it.
- UI criterion (`scripts/e2e.sh`): not verifiable from the code. The ui-check block is present.

## Implicit decisions
- One global lock for all queues.
- A one-hour test timeout.
- Queue entries are removed silently when their PR closes.
- The queue merges as whoever queued it.
- Test commits live in a hidden `refs/gitter-queue/<id>` namespace.
- Any writer can dequeue any entry.

## Not checked
- Whether upload-pack honours `hideRefs` for remote runners fetching by sha.
- Which plan step each hunk serves.
- The effect of `refs/gitter-queue` on `bundle --all` exports (src/api/lifecycle.rs:728).

Noticed: account export bundles with `--all` (src/api/lifecycle.rs:728) include the `refs/gitter-queue/*` refs. Orphaned queue refs, left when a `delete_ref` fails or `start_test` races a dequeue, are never pruned.
