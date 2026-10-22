**Verdict: changes needed.** The route allow-list, repo scoping, token handling, JSON-RPC parsing and the default-branch rule held up against everything I tried. One HIGH remains: an agent can run shell commands on the Gitter host through CI.

I found no way to reach a repo outside the allow-list, whether by id, rename, transfer, fork, `refs/heads/main`, `main/` or case variants. Tool paths are built only from validated `owner/name`, integers and percent-encoded query values. I did not run the code or tests.

## lens: adversary
- HIGH — `src/ci/runner.rs:533-535`, `src/ci/jobs.rs:38`, `src/ci/trigger.rs:131-147` — An agent can run arbitrary shell on the Gitter host.
  - Scenario: with `commit_files`, the agent writes `.gitter/ci.yml` (or any script the pipeline calls) on a new branch. The push starts a run; with the default setting `ci=admins` and the built-in runner on, the step runs as a bare `sh -e -c` as the server user, with `DOCKER_HOST` passed through.
  - Impact: that shell can read `secret.key` and the DB, so "no CI secrets" is moot. The agent reads the output with `get_ci_log`.
  - Fix: hold agent-started runs as "awaiting approval" until a human with write access approves, or run them only in a no-mount, no-network container.
- MEDIUM — `src/ci/trigger.rs:98-108`, `src/api/ci.rs:145` — The no-secrets mark covers only runs the agent itself starts. A human pushing a fixup to the agent's branch, clicking Run on it, or opening a PR from it runs the agent's workflow with secrets. The README says "a later push by a person runs with secrets as usual". Fix: remember agent-created branches (or the commits) and mark any run on them until merged.
- MEDIUM — `src/api/code.rs:965` — Only the default branch is guarded, so an agent can overwrite any existing branch, including a human's feature branch or a `release` or `deploy` branch that triggers CD. Fix: let agents commit only to branches they created, or only `new_branch`.
- LOW — `src/api/mod.rs:319` — `agent_may` allows GETs the tools never use (`ci/jobs/*/artifacts`, `log/stream`, `compare`, `blame`). Narrow it to what the tools use.
- Review routes once merged (`src/api/mod.rs:164-169` on main): `agent_may` is default-deny, so POST `/pulls/{n}/comments`, `/reviews` and `/comments/{id}/apply` are blocked, and `/collaborators` stays blocked. GET `pulls/**` is open, so an agent can read the user's own pending drafts.
  - If a review tool is added, allow only `comment`-type reviews, never `approve` or `request_changes`; those would count as the user's decision.
  - `apply` calls `commit_files` directly (`reviews.rs:533`), bypassing the check in `write_contents`. Move the agent default-branch check into a shared helper before ever allowing it.

## lens: edge
- MEDIUM — `src/validate.rs:4` — `mcp` is not a reserved name. `GET /mcp` now returns 405, so a user or org named `mcp` loses its profile page on a direct load. Fix: add `mcp` to `RESERVED`.
- MEDIUM — `mcp.rs` `through_api` (about lines 321-349) — Replies over 256 KB are cut with no marker (only `get_ci_log` gets a hint). `read_file` returns truncated JSON with `isError:false`, and a stream error mid-body is ignored. Fix: append a "truncated" note and return `isError`.
- LOW — A moved repo shows the agent only "Failed (308)". Say where it moved to.

## lens: operator
- LOW — Denied calls (scope_guard 403, blocked default-branch commit) aren't audited, only successful writes. Those are the prompt-injection signals.
- LOW — No site-wide switch to turn MCP off. The rate limit of 120 calls a minute is a constant.
- LOW — The public Agent badge, feed and audit show the user's chosen token name. Warn in the UI.
- LOW — Commits are authored as the user with no agent marker in git history. Add a trailer.

## lens: maintainer
- MEDIUM — `ACTING_AGENT` is a task-local, so a `tokio::spawn` inside a handler silently drops attribution and the no-secrets mark. Pass the agent explicitly, or document it.
- LOW — The tool-to-path map and `agent_may` are two allow-lists that must agree. `kind` and `tools` are magic strings. The router is rebuilt per call and the "<1 ms" claim was never measured.

## lens: intent
Both criteria are met as written.

## Acceptance criteria
1. Met. The three `mcp` tests cover the tool calls and show an agent reaches only its repos and tools. They pass per the task evidence; I did not rerun them.
2. Met by code reading. The settings UI, badge and audit entry are present. I did not run `e2e.sh`.

Secrets are withheld on human re-runs, because `rerun` reuses the same run row. Pushes made inline through `write_contents` are also marked.

## Implicit decisions
- Agents get Write scope, not admin. A site admin's agent is not an admin.
- The user's role is recomputed on every call.
- Tokens are never-expiring if the user picks "Never".
- Batches are refused.
- A missing `Origin` header is allowed.
- `tools` is stored as comma-separated text.

## Not checked
Running anything; `ROLE_SQL`; org and transfer edge cases beyond `lookup` and `covers`; whether the default-branch DB column can drift from the git HEAD ref; the artifact download path; the CSS.

**Noticed:** the agent can create unlimited branches (bounded only by the 120-per-minute write limit); runners with docker get `DOCKER_HOST` passed into no-image jobs.
