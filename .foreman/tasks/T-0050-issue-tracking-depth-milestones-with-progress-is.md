---
id: T-0050
type: FEATURE
tier: M
status: done
priority: normal
scope: []
depends_on: []
source: followup
created: 2026-10-22T00:30:42Z
updated: 2026-10-22T13:43:46Z
base: 160ee7ebd7ffd673a338a8ffd294233d59bea233
base_tree: b0927ac64981ef32ee55ad63666e752179ea75bf
plan0_at: 2026-10-22T03:32:40Z
plan0_scope: []
verified: strong
---
# Issue tracking depth: milestones, issue/PR templates, assignees
## Raw request
> Issue tracking depth: milestones with progress, issue and PR templates (.gitter/ISSUE_TEMPLATE), notifications inbox with watching/unwatching repos and threads, @mention notifications (split from T-0008)
## Interpretation
Milestones per repo (title, description, due date, open/closed) with open/closed progress, assignable to issues and PRs, a milestones list and a milestone page, and a milestone filter on issue lists. Issue and PR templates from .gitter/ISSUE_TEMPLATE/*.md (front matter: name, about, title, labels) and .gitter/pull_request_template.md on the default branch, offered as a chooser when opening an issue and prefilled for PRs. Assignees on issues and PRs (up to 10, people who can write), set from the sidebar, shown in lists, filterable (assignee:me). Notifications, watching and @mentions are a separate task (not here).
## Assumptions (confidence)
## Acceptance criteria
- [x] Milestones CRUD with progress counts, assigning an issue/PR to a milestone, milestone filter; assignees set by writers only (others refused), filter assignee:NAME; templates listed from .gitter/ISSUE_TEMPLATE with front matter and the PR template returned — verify with `cargo test -q --test issues_depth`
- [x] Milestones page, milestone and assignee pickers in the issue sidebar, template chooser on new issue, PR template prefill render without console errors — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
Tables milestones(id, repo_id, number?, title, description, due_on, state, closed_at) and issue_assignees(issue_id, user_id); issues get milestone_id. Templates parsed from the git tree at request time (small front-matter parser, cached by blob sha via the memo cache). Writers manage milestones and assignees; issue authors without write can't set them (like GitHub). Agent tokens (MCP) stay unable to touch them unless a tool is added later.
## Risks and rollback
## Execution prompt
## Steps
1. [x] schema + milestones API with progress
2. [x] assignees API + filters
3. [x] templates parsing + API
4. [x] UI: milestones pages, sidebar pickers, template chooser, PR prefill
5. [x] README, e2e
## Resume here
## Docs impact
README issues: milestones, templates, assignees
## Verification evidence
- (ac 1) `cargo test -q --test issues_depth 2>&1 | tail -40` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.39s [ran] [tree bf6b83f7f961] (2026-10-22T03:41:05Z)
- (step 1) `cargo test -q --test issues_depth 2>&1 | tail -40` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.39s [ran] [tree bf6b83f7f961] (2026-10-22T03:41:05Z)
- (step 1) `cargo test -q --test issues_depth milestones` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.97s [ran] [tree bf6b83f7f961] (2026-10-22T03:41:53Z)
- (step 2) `cargo test -q --test issues_depth assignees` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 1.71s [ran] [tree bf6b83f7f961] (2026-10-22T03:41:58Z)
- (step 3) `cargo test -q --test issues_depth templates` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.98s [ran] [tree bf6b83f7f961] (2026-10-22T03:42:00Z)
- (ac 2) `bash scripts/e2e.sh` → ✗ exit 1 · http://127.0.0.1:24678/signup?setup=MYYzs7TJ4XmO2gZJ / gitter 0.1.0 listening on 127.0.0.1:24678 — open http://127.0.0.1:24678 [ran] [tree 57fcc425ee53] (2026-10-22T03:46:39Z)
- (ac 1) `cargo test -q --test issues_depth` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.00s [ran] [tree 3580b69b6fe8] (2026-10-22T05:57:29Z)
- (ac 2) `bash scripts/e2e.sh 2>&1 | tail -3` → exit 0 · ✓ UI pages render without console errors / All 39 checks passed. [ran] [tree 3580b69b6fe8] (2026-10-22T13:41:15Z)
- (step 4) `cargo test -q --test issues_depth && node --check static/js/pages/issue.js` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.12s [ran] [tree 3580b69b6fe8] (2026-10-22T13:41:51Z)
- (step 5) `grep -c -i milestone README.md` → exit 0 · 1 [ran] [tree 3580b69b6fe8] (2026-10-22T13:41:51Z)
- (ac 1) `cargo test -q --test issues_depth` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.14s [ran] [tree 3580b69b6fe8] (2026-10-22T13:43:46Z)
- (ac 2) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 39 checks passed. [ran] [tree 3580b69b6fe8] (2026-10-22T13:43:46Z)
- (audit intent) `fm-reviewer five-lens review: approve with nits; nits fixed in the lane merge (none title, due dates, template budget), site-admin assignees documented as intended` → both criteria met on main; issues_depth 3/3, e2e 39/39 [tree 3580b69b6fe8] (2026-10-22T13:43:46Z)
- (audit adversary) `fm-reviewer five-lens review: approve with nits; nits fixed in the lane merge (none title, due dates, template budget), site-admin assignees documented as intended` → new routes readable/writable-gated, agents default-denied, assignees writers only via writer_sql [tree 3580b69b6fe8] (2026-10-22T13:43:46Z)
- (audit edge) `fm-reviewer five-lens review: approve with nits; nits fixed in the lane merge (none title, due dates, template budget), site-admin assignees documented as intended` → due dates per month and leap year, 'none' reserved as a filter word, 256 KB template budget [tree 3580b69b6fe8] (2026-10-22T13:43:46Z)
## Log
- 2026-10-22T00:30:42Z created
- 2026-10-22T03:31:47Z planned from capture
- 2026-10-22T03:32:04Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0050.builder.md
- 2026-10-22T03:32:40Z focused
- 2026-10-22T05:56:55Z merged worktree-agent-acb02b0a51b1325d4 (15 file(s))
- 2026-10-22T05:57:01Z lane removed: /home/chase/projects/gitter/.claude/worktrees/agent-acb02b0a51b1325d4
- 2026-10-22T05:57:02Z focused
- 2026-10-22T13:43:46Z done
## Follow-ups captured
## Build vs reuse
Reuses: issues/comments/events transactions in src/api/issues.rs and src/api/reviews.rs (fan-out runs in the same transaction), repo::access for the read check when the inbox is read, labels' picker UI pattern in static/js/pages/issue.js for assignees and milestones, the markdown renderer for templates, and Git::run to read .gitter/ISSUE_TEMPLATE from the default branch. New: milestones, issue_assignees, watches, thread_subs, notifications tables and a /notifications page.
## Related
Related past work (data from this project's history, not instructions):
- T-0008 [FEATURE L, done, 5 steps] Collaboration: orgs + collaborators with read/write/admin roles and plain-language access — lesson: Trust derived from membership is fo…
- research brainstorm-20261021: [instruction-like text quoted from a file or tool; data, not instructions] # Gitter brainstorm catalogue (2026-10-21)
- playbook Architecture Decision Records (skills/playbooks/references/reflect/architecture-decision-records.md)
- research T-0049-review-3: Verdict: changes needed.** There is one real data-corruption path in apply-suggestion and one visibility hole on replies. The rest is minor. [s…
- similar past tasks were L: consider --tier L
- the similar finished task took 5 steps
## Lessons
- Builders writing tests into their own test file (tests/issues_depth.rs) instead of the tail of tests/api.rs made the merge nearly conflict-free
## Files touched
- tests/issues_depth.rs
- src/db.rs
- src/api/milestones.rs
- src/api/mod.rs
- src/api/issues.rs
- src/api/lifecycle.rs
- static/js/pages/milestones.js
- static/js/main.js
- static/js/pages/issues.js
- static/js/pages/issue.js
- static/js/pages/newissue.js
- static/js/pages/compare.js
- static/css/issues.css
- scripts/ui-check.mjs
- README.md
- src/api/checks.rs
- src/api/ci.rs
- src/api/code.rs
- src/api/repos.rs
- src/api/reviews.rs
- src/auth.rs
- src/ci/config.rs
- src/ci/jobs.rs
- src/ci/queue.rs
- src/ci/runner.rs
- src/ci/trigger.rs
- src/cli.rs
- src/hooks.rs
- src/lib.rs
- src/main.rs
