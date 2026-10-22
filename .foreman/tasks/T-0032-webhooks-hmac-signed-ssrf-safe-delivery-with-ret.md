---
id: T-0032
type: FEATURE
tier: M
status: done
priority: normal
scope: []
depends_on: []
source: followup
created: 2026-10-21T21:11:21Z
updated: 2026-10-22T01:10:39Z
base: b1f7cb8f0153f235bc35b9789deadec3d070eb57
base_tree: dc25a316821902367fcf5c19d130f573c26845f9
plan0_at: 2026-10-22T00:09:12Z
plan0_scope: []
verified: strong
---
# Webhooks: HMAC-signed, SSRF-safe delivery with retries and a delivery log (split from T-00
## Raw request
> Webhooks: HMAC-signed, SSRF-safe delivery with retries and a delivery log (split from T-0009)
## Interpretation
Repository webhooks, managed in repo Settings → Webhooks by writers: payload URL, secret, events (push, issues, issue_comment, pull_request, ci_run, release when it exists; or all), active toggle, and an optional 'allow private addresses' that only admins can tick. Each delivery POSTs JSON with headers X-Gitter-Event, X-Gitter-Delivery (uuid) and X-Gitter-Signature-256: sha256=<hex HMAC-SHA256(secret, body)>. Delivery is SSRF-safe through the shared src/net.rs helper: https or http only, DNS resolved once and the connection pinned to that IP, loopback, private, link-local, CGNAT, multicast and unspecified addresses refused unless allowed, no redirects, 10 s timeout, response capped at 64 KB. Failed deliveries retry 5 times with backoff (1 m, 5 m, 30 m, 2 h, 6 h) from a background loop that survives restarts (a deliveries table holds the state). A delivery log per hook shows the last 50 deliveries (status, duration, request headers and body, response code and the first 2 KB of the body) with a Redeliver button. A ping is sent on create. Payload secrets are encrypted at rest with security::seal and the server key. Events are emitted from hooks::on_push, issue, PR and comment creation and updates, and CI run completion.
## Assumptions (confidence)
## Acceptance criteria
- [x] Hooks are created, signed with HMAC, delivered, retried with backoff and logged, and private addresses are refused by default — verify with `cargo test -q --test api webhook`
- [x] The webhook settings and delivery log render and work in a browser — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
(a) Deliveries table as a durable queue drained by a background loop (like ci::upkeep) [chosen: survives restarts, retries are rows, the log is the same table]; (b) fire-and-forget tokio tasks (lost on restart, no log); (c) an external queue (another service). HTTP via ureq in spawn_blocking with a resolver that pins the checked IP (ureq Agent with a custom resolver, or connect to the IP with the Host header and SNI). Build vs reuse: ureq and hmac/sha2 are already dependencies. Put SSRF checks in src/net.rs so imports and mirrors can reuse them later.
## Risks and rollback
## Execution prompt
## Steps
1. [x] net: use src/net.rs safe_post (pinned IP, private ranges refused, no redirects, caps)
2. [x] schema + API: webhooks, deliveries (sealed secret), CRUD for writers, ping, redeliver
3. [x] events: push, issues, comments, pull requests, CI runs; payload shapes documented in README
4. [x] delivery loop: retries with backoff, survives restart, log of 50 per hook
5. [x] UI: Settings → Webhooks (list, form, delivery log with redeliver) + e2e and browser checks
## Resume here
## Docs impact
README Webhooks section updated
## Verification evidence
- (ac 1) `cargo test -q --test api webhook 2>&1 | tail -30` → ✗ exit 1 · error: test failed, to rerun pass `--test api` / ✗ exit 101 (30.6 s; last 27 of 27 lines) [ran] [tree fb712b18adc6] (2026-10-22T00:21:47Z)
- (step 1) `cargo test -q --test api webhook` → ✗ exit 101 · test result: FAILED. 0 passed; 3 failed; 0 ignored; 0 measured; 35 filtered out; finished in 1.10s / error: test failed, to rerun pass `--test api` [ran] [tree fb712b18adc6] (2026-10-22T00:21:51Z)
- (step 1) `cargo test -q --lib net::` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 52 filtered out; finished in 0.00s [ran] [tree 48cd5419d5d2] (2026-10-22T00:28:45Z)
- (step 2) `cargo test -q --test api webhook` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 35 filtered out; finished in 2.49s [ran] [tree 48cd5419d5d2] (2026-10-22T00:28:48Z)
- (step 3) `cargo test -q --test api webhook` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 35 filtered out; finished in 2.13s [ran] [tree 48cd5419d5d2] (2026-10-22T00:28:51Z)
- (step 4) `cargo test -q --lib webhooks::` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 53 filtered out; finished in 0.00s [ran] [tree 48cd5419d5d2] (2026-10-22T00:28:52Z)
- (step 5) `cargo test -q 2>&1 | grep "test result"` → exit 0 · test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.49s / test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s [ran] [tree 52e47a647f02] (2026-10-22T00:42:31Z)
- (ac 1) `cargo test -q --test api webhook` → exit 0 · .... / test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 41 filtered out; finished in 1.39s [ran] [tree 309ada7ee2ed] (2026-10-22T01:06:57Z)
- (ac 2) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 28 checks passed. [ran] [tree 309ada7ee2ed] (2026-10-22T01:08:46Z)
- (ac 1) `cargo test -q --test api webhook` → exit 0 · .... / test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 41 filtered out; finished in 1.55s [ran] [tree 309ada7ee2ed] (2026-10-22T01:10:38Z)
- (ac 2) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 28 checks passed. [ran] [tree 309ada7ee2ed] (2026-10-22T01:10:38Z)
- (audit intent) `fm-reviewer five-lens review of the branch (changes needed); all 8 findings fixed with tests during the merge with main` → criteria re-run on main: webhook API tests 4/4, e2e 28/28 [tree 309ada7ee2ed] (2026-10-22T01:10:38Z)
- (audit adversary) `fm-reviewer five-lens review of the branch (changes needed); all 8 findings fixed with tests during the merge with main` → hooks admin-only (collaborator test), generic refused-address errors without IPs (tested), suspended/hidden repos don't send (tested), demotion clears private-address hooks [tree 309ada7ee2ed] (2026-10-22T01:10:38Z)
- (audit edge) `fm-reviewer five-lens review of the branch (changes needed); all 8 findings fixed with tests during the merge with main` → one delivery per receiver at a time; test tick drains rounds; drops logged as failed rows (tested) [tree 309ada7ee2ed] (2026-10-22T01:10:38Z)
- (audit operator) `fm-reviewer five-lens review of the branch (changes needed); all 8 findings fixed with tests during the merge with main` → worker pool of 8 with per-receiver serialization; README documents limits, drops and pauses [tree 309ada7ee2ed] (2026-10-22T01:10:38Z)
- (audit maintainer) `fm-reviewer five-lens review of the branch (changes needed); all 8 findings fixed with tests during the merge with main` → migration renumbered v12; payload bodies clipped in one helper [tree 309ada7ee2ed] (2026-10-22T01:10:38Z)
## Log
- 2026-10-21T21:11:21Z created
- 2026-10-21T23:40:33Z set tier=M
- 2026-10-22T00:08:38Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0032.builder.md
- 2026-10-22T00:09:12Z focused
- 2026-10-22T01:06:31Z merged worktree-agent-af0821cc1108311ce (18 file(s))
- 2026-10-22T01:06:38Z lane removed: /home/chase/projects/gitter/.claude/worktrees/agent-af0821cc1108311ce
- 2026-10-22T01:06:39Z focused
- 2026-10-22T01:10:39Z done
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research brainstorm-20261021: [instruction-like text quoted from a file or tool; data, not instructions] # Gitter brainstorm catalogue (2026-10-21)
- research T-0006-review-2: Verdict: changes needed.** The feature matches the request and the README. The operator-facing gaps are the unbounded built-in runner access, a…
- research T-0006-review-1: Verdict: changes needed.** Four HIGH findings, each shown by a code path. I found them by reading, not by running anything. [stale: src/api/ci.…
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
## Lessons
- When a builder's base predates a new role model, grep its handlers for writable/owner checks during the merge; reviewers told about main's changes flag them reliably
## Plan gaps
- failed first: step 1 net: use src/net.rs safe_post (pinned IP, private ranges refused, no redirects, caps)
## Files touched
- src/net.rs
- tests/api.rs
- src/lib.rs
- src/webhooks.rs
- src/db.rs
- src/api/webhooks.rs
- src/api/mod.rs
- src/hooks.rs
- src/main.rs
- src/api/issues.rs
- scripts/e2e.sh
- scripts/ui-check.mjs
- static/js/pages/webhook.js
- static/js/main.js
- static/js/lib.js
- static/js/pages/reposettings.js
- README.md
- src/api/admin.rs
