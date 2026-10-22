---
id: T-0007
type: FEATURE
tier: M
status: captured
priority: normal
scope: []
depends_on: []
source: user
created: 2026-10-21T17:01:42Z
updated: 2026-10-22T00:39:12Z
group: CI gating and pipeline features
---
# CI trust and gating: sandboxed jobs (no host docker socket, cpu/mem/time limits, network p
## Raw request
> FEATURE: CI trust and gating: sandboxed jobs (no host docker socket, cpu/mem/time limits, network policy), fork-PR approval, commit/PR status checks, branch protection with required checks, auto-merge, status badges + unfurl cards (C4 C5 C40)
> CONTEXT: Gitter = public GitHub-like forge, Rust axum + SQLite + git CLI, framework-free SPA; catalogue at research brainstorm-20261021. All build after #T-0001.
## Interpretation
## Assumptions (confidence)
## Acceptance criteria
## Non-goals
## Approach (options → choice → why)
## Risks and rollback
## Execution prompt
## Steps
## Resume here
## Docs impact
## Verification evidence
## Log
- 2026-10-21T17:01:42Z created
- 2026-10-22T00:39:12Z from T-0008 review: write collaborators (and org members with write) on an admin-owned repo can push .gitter/ci.yml that runs on the built-in host runner with the repo's secrets. Documented for now; T-0007 should gate built-in-runner jobs and secrets by the pusher's role (or need approval for non-admin pushers)
## Follow-ups captured
