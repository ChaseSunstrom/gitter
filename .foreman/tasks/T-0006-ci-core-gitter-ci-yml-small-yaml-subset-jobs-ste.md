---
id: T-0006
type: FEATURE
tier: M
status: captured
priority: normal
scope: []
depends_on: [T-0002]
source: user
created: 2026-10-21T17:01:42Z
updated: 2026-10-21T18:11:59Z
---
# CI core: .gitter/ci.yml (small YAML subset: jobs, steps, needs, image, env, matrix, path f
## Raw request
> FEATURE: CI core: .gitter/ci.yml (small YAML subset: jobs, steps, needs, image, env, matrix, path filters, manual/cron), built-in runner in the same binary (gitter runner, auto-registered), leased jobs with heartbeat/requeue, live SSE logs with ANSI + folding + secret masking, encrypted secrets, artifacts, rerun failed (C2 C3 C6 C7-core C8) #T-0002
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
- 2026-10-21T18:11:59Z steer (user): 'wayy faster CI/pipeline runners, with able to use gitlab/github CI configurations, GUI management of them, etc.' -> run .github/workflows/*.yml and .gitlab-ci.yml natively (translate to internal graph), speed-first runners (warm pools, content-addressed caching, instant job start), full GUI: pipeline graph + live logs, runner management, secrets/variables, schedules, manual runs
## Follow-ups captured
