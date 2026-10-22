---
id: T-0032
type: FEATURE
tier: M
status: captured
priority: normal
scope: []
depends_on: []
source: followup
created: 2026-10-21T21:11:21Z
updated: 2026-10-21T23:40:35Z
---
# Webhooks: HMAC-signed, SSRF-safe delivery with retries and a delivery log (split from T-00
## Raw request
> Webhooks: HMAC-signed, SSRF-safe delivery with retries and a delivery log (split from T-0009)
## Interpretation
Repository webhooks, managed in repo Settings → Webhooks by writers: payload URL, secret, events (push, issues, issue_comment, pull_request, ci_run, release when it exists; or all), active toggle, and an optional 'allow private addresses' that only admins can tick. Each delivery POSTs JSON with headers X-Gitter-Event, X-Gitter-Delivery (uuid) and X-Gitter-Signature-256: sha256=<hex HMAC-SHA256(secret, body)>. Delivery is SSRF-safe through the shared src/net.rs helper: https or http only, DNS resolved once and the connection pinned to that IP, loopback, private, link-local, CGNAT, multicast and unspecified addresses refused unless allowed, no redirects, 10 s timeout, response capped at 64 KB. Failed deliveries retry 5 times with backoff (1 m, 5 m, 30 m, 2 h, 6 h) from a background loop that survives restarts (a deliveries table holds the state). A delivery log per hook shows the last 50 deliveries (status, duration, request headers and body, response code and the first 2 KB of the body) with a Redeliver button. A ping is sent on create. Payload secrets are encrypted at rest with security::seal and the server key. Events are emitted from hooks::on_push, issue, PR and comment creation and updates, and CI run completion.
## Assumptions (confidence)
## Acceptance criteria
- [ ] Hooks are created, signed with HMAC, delivered, retried with backoff and logged, and private addresses are refused by default — verify with `cargo test -q --test api webhook`
- [ ] The webhook settings and delivery log render and work in a browser — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
## Risks and rollback
## Execution prompt
## Steps
1. [ ] net: use src/net.rs safe_post (pinned IP, private ranges refused, no redirects, caps)  <- CURRENT
2. [ ] schema + API: webhooks, deliveries (sealed secret), CRUD for writers, ping, redeliver
3. [ ] events: push, issues, comments, pull requests, CI runs; payload shapes documented in README
4. [ ] delivery loop: retries with backoff, survives restart, log of 50 per hook
5. [ ] UI: Settings → Webhooks (list, form, delivery log with redeliver) + e2e and browser checks
## Resume here
## Docs impact
## Verification evidence
## Log
- 2026-10-21T21:11:21Z created
- 2026-10-21T23:40:33Z set tier=M
## Follow-ups captured
