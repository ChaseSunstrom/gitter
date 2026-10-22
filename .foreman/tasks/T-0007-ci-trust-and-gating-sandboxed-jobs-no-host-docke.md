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
updated: 2026-10-22T00:48:37Z
group: CI gating and pipeline features
---
# CI trust and gating: sandboxed jobs (no host docker socket, cpu/mem/time limits, network p
## Raw request
> FEATURE: CI trust and gating: sandboxed jobs (no host docker socket, cpu/mem/time limits, network policy), fork-PR approval, commit/PR status checks, branch protection with required checks, auto-merge, status badges + unfurl cards (C4 C5 C40)
> CONTEXT: Gitter = public GitHub-like forge, Rust axum + SQLite + git CLI, framework-free SPA; catalogue at research brainstorm-20261021. All build after #T-0001.
## Interpretation
Make CI safe to offer and useful for gating merges. Jobs run sandboxed: in a container with no host Docker socket, CPU, memory, process and time limits, a read-only root with a writable work dir, and a network policy (on by default, off per job or site-wide). The built-in runner only takes jobs whose run was triggered by someone with admin on an admin-trusted repo (fixing the gap T-0008 found where write collaborators' pushes ran on the host); other runs wait for a remote runner. Pull requests from people without write (and from forks) wait for approval by a writer before their run starts, and never get secrets. Commit statuses: external systems post statuses (context, state, description, URL) to a commit, shown next to CI runs. Branch protection per branch pattern: required status checks, required approving reviews (once T-0049 lands, until then a count of zero), no force-push, no deletion, admins can bypass or not. Auto-merge: a PR merges itself once checks pass. Status badges (SVG per branch and workflow) and link unfurl cards (Open Graph tags on repo, issue and PR pages).
## Assumptions (confidence)
## Acceptance criteria
- [ ] Jobs run with resource limits and no host socket; fork and non-writer PR runs wait for approval and get no secrets; the built-in runner only runs runs triggered by admins — verify with `cargo test -q --test ci trust`
- [ ] Commit statuses post and list; branch protection blocks force-push, deletion and merges without required checks, over git and the web; auto-merge merges when checks pass — verify with `cargo test -q --test api protection`
- [ ] Badges render as SVG, pages carry Open Graph tags, the settings pages for rules and approvals render — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
(a) Sandbox by always running steps in docker with --cpus, --memory, --pids-limit, --read-only plus a tmpfs, --cap-drop ALL, --security-opt no-new-privileges, and --network none when the policy says so; shell steps without docker are refused unless the admin opts in [chosen: docker is already the runner's executor]; (b) bubblewrap/nsjail (another dependency, Linux only). Statuses in a commit_statuses table keyed by repo, sha, context; protection in a branch_rules table checked in the receive-pack pre-receive path (src/hooks.rs) and in the merge API, so git pushes and web merges follow one rule. Auto-merge is a flag on the pull that the CI finish path and status posts re-check.
## Risks and rollback
## Execution prompt
## Steps
1. [ ] sandboxed docker steps: limits, read-only root, cap-drop, no-new-privileges, network policy; shell steps only when an admin allows; tests  <- CURRENT
2. [ ] trust: runs record who triggered them; the built-in runner and secrets need an admin trigger on a trusted repo; approval gate for fork and non-writer PRs
3. [ ] commit statuses API and display beside CI runs
4. [ ] branch protection rules (patterns, required checks, no force-push or delete, admin bypass) enforced in the pre-receive hook and the merge API; settings UI
5. [ ] auto-merge flag re-checked when runs finish or statuses arrive; badges SVG; Open Graph unfurl tags; e2e + README
## Resume here
## Docs impact
## Verification evidence
## Log
- 2026-10-21T17:01:42Z created
- 2026-10-22T00:39:12Z from T-0008 review: write collaborators (and org members with write) on an admin-owned repo can push .gitter/ci.yml that runs on the built-in host runner with the repo's secrets. Documented for now; T-0007 should gate built-in-runner jobs and secrets by the pusher's role (or need approval for non-admin pushers)
## Follow-ups captured
