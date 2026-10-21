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
updated: 2026-10-21T17:01:42Z
inferred_deps: [T-0001, T-0006]
inferred_why: T-0001: named in its request; T-0006: trust gating needs CI core jobs and runners first.
group: CI pipelines, runners, and gating
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
## Follow-ups captured
