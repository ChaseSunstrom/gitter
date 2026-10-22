---
id: T-0021
type: FEATURE
tier: M
status: done
priority: normal
scope: []
depends_on: []
source: followup
created: 2026-10-21T17:01:53Z
updated: 2026-10-22T02:58:09Z
inferred_deps: [T-0008]
inferred_why: T-0008: agent-scoped tokens need org roles and access model
base: 622639b99b856b79c31bfe3499ec9320fcf9a6ae
base_tree: de24e22305c5ef5eeafb5ac43d6fd641c69e30c3
plan0_at: 2026-10-22T01:52:24Z
plan0_scope: []
verified: strong
---
# MCP server + agent-scoped tokens (C42 pick)
## Raw request
> MCP server + agent-scoped tokens (C42 pick)
## Interpretation
Let AI agents work with Gitter safely. An MCP server (Model Context Protocol, streamable HTTP transport at /mcp, JSON-RPC 2.0) exposes tools: search code, read files and trees, list and read issues and PRs, create issues, comment, open a PR from a branch, read CI runs and logs, and commit file changes to a branch. Agent-scoped tokens: a token type for agents that is limited to chosen repositories, to a set of tools or API areas, can never merge to protected branches or touch settings, secrets, tokens or account routes, is rate limited separately, and marks everything it does as made by an agent on behalf of the user (shown with a badge in the UI and in the audit log).
## Assumptions (confidence)
## Acceptance criteria
- [x] MCP initialize, tools/list and tools/call for each tool over HTTP; an agent token only reaches its repos and tools, can't reach settings, secrets, tokens or merges — verify with `cargo test -q --test api mcp`
- [x] Agent tokens are created in Settings → Tokens with repo and tool choices; actions show an agent badge; the audit log names the agent token — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
(a) MCP over the existing API: each tool calls the same handler code paths with the agent token's narrowed viewer, so access rules stay in one place [chosen]; (b) a separate service (duplicate auth). Agent tokens extend the existing tokens table with kind = 'agent', a repo allow-list and a tool allow-list; the scope guard gains the agent rules; events and comments get an on_behalf flag.
## Risks and rollback
## Execution prompt
## Steps
1. [x] agent tokens: schema (kind, repos, tools), creation UI and API, scope guard rules, separate rate limit
2. [x] MCP endpoint: JSON-RPC over streamable HTTP, initialize, tools/list, tools/call, errors
3. [x] tools: search, read file and tree, issues and PRs (list, read, create, comment), open PR, CI runs and logs, commit changes to a branch
4. [x] attribution: on-behalf flag on events and comments, agent badge in the UI, audit entries
5. [x] tests, e2e, README with a client config example
## Resume here
<!-- auto -->
- 2026-10-22T02:41:28Z steps 5/5 done
- git: worktree-agent-a6a595d794fd2c280; 30 uncommitted: README.md, scripts/e2e.sh, scripts/ui-check.mjs, src/api/insights.rs, src/api/issues.rs, src/api/lifecycle.rs, src/api/mod.rs, src/api/orgs.rs, src/api/reviews.rs, src/ci/runner.rs
- touched since last checkpoint: src/ci/jobs.rs, src/api/mod.rs, static/js/pages/settings.js, static/js/lib.js, static/js/pages/home.js, static/css/app.css, scripts/ui-check.mjs, README.md, src/auth.rs, src/api/account.rs, src/api/mcp.rs, scripts/e2e.sh, src/api/issues.rs, static/js/pages/issues.js, static/js/pages/issue.js
## Docs impact
README AI agents section: branch rule, 100-branch cap, CI runs on agent branches; settings help text
## Verification evidence
- (ac 1) `cargo test -q --test api mcp 2>&1 | tail -15` → ✗ exit 1 · test result: FAILED. 0 passed; 3 failed; 0 ignored; 0 measured; 52 filtered out; finished in 0.93s / error: test failed, to rerun pass `--test api` [ran] [tree 6a21c458f428] (2026-10-22T02:00:51Z)
- (ac 1) `cargo test -q --test api mcp 2>&1 | grep -v "^$" | tail -30` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 52 filtered out; finished in 1.68s [ran] [tree 6ba807836b6c] (2026-10-22T02:07:20Z)
- (step 1) `cargo test -q --test api mcp 2>&1 | grep -v "^$" | tail -30` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 52 filtered out; finished in 1.68s [ran] [tree 6ba807836b6c] (2026-10-22T02:07:20Z)
- (step 1) `cargo test -q --test api mcp` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 52 filtered out; finished in 1.11s [ran] [tree 925a6069898d] (2026-10-22T02:08:56Z)
- (step 2) `cargo test -q --test api mcp_serves` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 54 filtered out; finished in 0.68s [ran] [tree 925a6069898d] (2026-10-22T02:09:01Z)
- (step 3) `cargo test -q --test api mcp` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 52 filtered out; finished in 1.21s [ran] [tree 925a6069898d] (2026-10-22T02:09:02Z)
- (step 4) `cargo test -q --lib agent` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 62 filtered out; finished in 0.00s [ran] [tree 925a6069898d] (2026-10-22T02:09:03Z)
- (step 5) `cargo test -q 2>&1 | grep -E "test result|FAILED|panicked" | head` → exit 0 · test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.68s / test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s [ran] [tree 422b8c099635] (2026-10-22T02:16:27Z)
- (ac 1) `cargo test -q --test api mcp` → exit 0 · .... / test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 57 filtered out; finished in 1.42s [ran] [tree 3c8139a035ee] (2026-10-22T02:53:34Z)
- (ac 2) `bash scripts/e2e.sh 2>&1 | tail -3` → exit 0 · ✓ UI pages render without console errors / All 37 checks passed. [ran] [tree 3c8139a035ee] (2026-10-22T02:55:46Z)
- (ac 1) `cargo test -q --test api mcp` → exit 0 · .... / test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 57 filtered out; finished in 1.26s [ran] [tree 3c8139a035ee] (2026-10-22T02:58:08Z)
- (ac 2) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 37 checks passed. [ran] [tree 3c8139a035ee] (2026-10-22T02:58:08Z)
- (audit adversary) `fm-reviewer five-lens review (.foreman/research/T-0021-review.md); HIGH (agent CI as host shell) and the MEDIUMs fixed on merge with tests: built-in runner skips agent runs, agent_branches marks runs and limits commits, reserved names, truncation marker, narrowed reads` → agent-written CI never reaches the built-in runner; a person's push to an agent branch is still an agent run with no secrets (mcp_agent_ci_runs_get_no_secrets_and_never_the_builtin_runner) [tree 3c8139a035ee] (2026-10-22T02:58:08Z)
- (audit edge) `fm-reviewer five-lens review (.foreman/research/T-0021-review.md); HIGH (agent CI as host shell) and the MEDIUMs fixed on merge with tests: built-in runner skips agent runs, agent_branches marks runs and limits commits, reserved names, truncation marker, narrowed reads` → deleted-then-recreated branch name is a person's again; 100-branch cap; 256 KB cut is marked; mcp/v2 reserved [tree 3c8139a035ee] (2026-10-22T02:58:08Z)
- (audit intent) `fm-reviewer five-lens review (.foreman/research/T-0021-review.md); HIGH (agent CI as host shell) and the MEDIUMs fixed on merge with tests: built-in runner skips agent runs, agent_branches marks runs and limits commits, reserved names, truncation marker, narrowed reads` → both criteria met; tests 4/4 mcp, e2e 37/37 [tree 3c8139a035ee] (2026-10-22T02:58:08Z)
## Log
- 2026-10-21T17:01:53Z created
- 2026-10-22T01:52:01Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0021.builder.md
- 2026-10-22T01:52:24Z focused
- 2026-10-22T02:52:59Z merged worktree-agent-a6a595d794fd2c280 (25 file(s))
- 2026-10-22T02:53:04Z lane removed: /home/chase/projects/gitter/.claude/worktrees/agent-a6a595d794fd2c280
- 2026-10-22T02:53:05Z focused
- 2026-10-22T02:58:09Z done
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research brainstorm-20261021: [instruction-like text quoted from a file or tool; data, not instructions] # Gitter brainstorm catalogue (2026-10-21)
- research T-0001-review-3: Verdict: changes needed.** I found no HIGH issues, but several MEDIUM defects and one test-coverage gap. I could not run `cargo test`, `cargo c…
- research T-0006-review-1: Verdict: changes needed.** Four HIGH findings, each shown by a code path. I found them by reading, not by running anything. [stale: src/api/ci.…
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
## Lessons
- Marking by who acted (a task-local) misses later actors on the same branch; mark by the branch an agent created, stored in the database, so every trigger path sees it
## Files touched
- tests/api.rs
- src/db.rs
- src/auth.rs
- src/repo.rs
- src/api/mod.rs
- src/lib.rs
- src/api/mcp.rs
- src/api/account.rs
- src/api/issues.rs
- src/api/code.rs
- src/ci/trigger.rs
- src/ci/jobs.rs
- static/js/pages/settings.js
- static/js/lib.js
- static/js/pages/issue.js
- static/js/pages/home.js
- static/js/pages/issues.js
- static/css/app.css
- scripts/ui-check.mjs
- scripts/e2e.sh
- README.md
- src/ci/queue.rs
- src/api/repos.rs
- src/hooks.rs
- src/validate.rs
