---
id: T-0012
type: FEATURE
tier: M
status: captured
priority: normal
scope: []
depends_on: []
source: user
created: 2026-10-21T17:01:42Z
updated: 2026-10-21T17:01:42Z
inferred_deps: [T-0001, T-0006, T-0009]
inferred_why: T-0001: named in its request; T-0006: CI extras extend the core pipeline and runner.; T-0009: SSH auth needs scoped tokens and deploy keys.
group: CI pipelines, runners, and gating
---
# CI extras and SSH: CI cache, JUnit/coverage annotations, flaky-test detection, pipeline te
## Raw request
> FEATURE: CI extras and SSH: CI cache, JUnit/coverage annotations, flaky-test detection, pipeline templates, SSH clone/push via in-process server (C7-rest C15)
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
