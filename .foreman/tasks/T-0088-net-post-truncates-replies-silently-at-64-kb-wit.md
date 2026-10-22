---
id: T-0088
type: FIX
tier: S
status: done
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T16:03:15Z
updated: 2026-10-22T23:04:34Z
base: 6ecd753ce4e251474b93b69db9db5591a7fdee43
base_tree: 2a91d3173d93850c20a99ecb30aee87bc2d91eba
plan0_at: 2026-10-22T22:42:18Z
plan0_scope: []
verified: strong
---
# net::fetch/post say when a reply was cut off at the size cap, and callers treat it as an error
## Raw request
> - `net::post` truncates replies silently at 64 KB, with no way to tell a cut-off body from a complete one
> (noticed by an agent; research T-0024-recon)
## Interpretation
net::fetch reads at most cap bytes and silently drops the rest, so a JSON reply cut at 64 KB parses as an error at best, or as wrong data. Callers can't tell. Make the cut visible: Reply.truncated, or an error, and have callers that parse the body refuse a cut-off one.
## Assumptions (confidence)
## Acceptance criteria
- [x] a reply larger than the cap comes back marked truncated, and OAuth/import callers refuse it with a clear error instead of parsing a cut-off body — verify with `cargo test -q --lib net`
## Non-goals
## Approach (options → choice → why)
(a) fetch returns Err('the reply was larger than N KB') when more than cap arrives [chosen: callers already handle Err; no silent partial bodies anywhere]; (b) Reply.truncated flag every caller must check (easy to forget); (c) raise caps (doesn't fix it). Keep a cap per caller as today.
## Risks and rollback
## Execution prompt
## Steps
1. [x] test with a local server sending more than the cap; Reply gains truncated (or fetch errors past the cap); callers updated
## Resume here
## Docs impact
## Verification evidence
- (step 1) `cargo test -q --lib net` → exit 0 · .... / test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 111 filtered out; finished in 0.01s [ran] [tree aff61500854c] (2026-10-22T22:44:14Z)
- (ac 1) `cargo test -q --lib net` → exit 0 · .... / test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 114 filtered out; finished in 0.00s [ran] [tree 0b1cf4611819] (2026-10-22T23:04:21Z)
- (step 1) `cargo test -q --lib net` → exit 0 · .... / test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 114 filtered out; finished in 0.00s [ran] [tree 0b1cf4611819] (2026-10-22T23:04:21Z)
- (ac 1) `cargo test -q --lib net` → exit 0 · .... / test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 114 filtered out; finished in 0.00s [ran] [tree 0b1cf4611819] (2026-10-22T23:04:29Z)
- (audit edge) `self-review of the lane diff: only OAuth and imports parse replies and both now get Err past the cap; webhooks keep the lenient post (deliveries still count, their logged reply is trimmed); imports treat it as permanent; exact-cap boundary tested` → exactly-the-cap reply accepted, cap+1 refused [tree 0b1cf4611819] (2026-10-22T23:04:29Z)
- (audit self) `self-review of the lane diff: only OAuth and imports parse replies and both now get Err past the cap; webhooks keep the lenient post (deliveries still count, their logged reply is trimmed); imports treat it as permanent; exact-cap boundary tested` → no findings [tree 0b1cf4611819] (2026-10-22T23:04:29Z)
- (audit edge) `self-review of the lane diff: only OAuth and imports parse replies and both now get Err past the cap; webhooks keep the lenient post (deliveries still count, their logged reply is trimmed); imports treat it as permanent; exact-cap boundary tested` → exactly-the-cap reply accepted, cap+1 refused [tree 0b1cf4611819] (2026-10-22T23:04:34Z)
- (audit adversary) `self-review of the lane diff: only OAuth and imports parse replies and both now get Err past the cap; webhooks keep the lenient post (deliveries still count, their logged reply is trimmed); imports treat it as permanent; exact-cap boundary tested` → a hostile provider or import source sending a huge or endless body now gets an error after cap+1 bytes (no partial JSON parsed); the SSRF checks (check_url, PublicOnly resolver) are unchanged [tree 0b1cf4611819] (2026-10-22T23:04:34Z)
- (audit self) `self-review of the lane diff: only OAuth and imports parse replies and both now get Err past the cap; webhooks keep the lenient post (deliveries still count, their logged reply is trimmed); imports treat it as permanent; exact-cap boundary tested` → no findings [tree 0b1cf4611819] (2026-10-22T23:04:34Z)
## Log
- 2026-10-22T16:03:15Z created
- 2026-10-22T22:42:03Z planned from capture
- 2026-10-22T22:42:04Z approved: the user asked to implement every queued feature ("what about all of the other features you have queued?"); proceeds under that standing request
- 2026-10-22T22:42:05Z set approved=True
- 2026-10-22T22:42:05Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0088.builder.md
- 2026-10-22T22:42:18Z focused
- 2026-10-22T23:03:46Z merged worktree-agent-a241349dbdcd0846e (2 file(s))
- 2026-10-22T23:03:52Z lane removed: /home/chase/projects/gitter/.claude/worktrees/agent-a241349dbdcd0846e
- 2026-10-22T23:03:57Z focused
- 2026-10-22T23:04:34Z done
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research T-0024-recon: 1. **Answer [stale: src/api/account.rs, src/api/issues.rs, src/auth.rs changed since it was written]
- research T-0006-review-3: Verdict:** changes needed. Three MEDIUM issues, one in each of `src/api/ci.rs`, `src/ci/queue.rs` and `static/js/pages/ci.js`. The rest are LOW…
- research T-0049-review-1: Verdict:** changes needed [stale: src/api/reviews.rs, static/js/pages/issue.js, static/js/review.js changed since it was written]
- research T-0049-review-3: Verdict: changes needed.** There is one real data-corruption path in apply-suggestion and one visibility hole on replies. The rest is minor. [s…
## Regression test
src/net.rs unit test a_reply_past_the_cap_is_an_error_for_fetch_and_marked_for_post (in the source file, so fm's test-file check doesn't see it): on the code before 05f0ea7, fetch returned a 1000-byte body for a 2000-byte reply, which this test refuses.
## Files touched
- src/net.rs
- src/imports.rs
