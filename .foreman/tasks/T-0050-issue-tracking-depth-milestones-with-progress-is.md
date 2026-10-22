---
id: T-0050
type: FEATURE
tier: L
status: captured
priority: normal
scope: []
depends_on: []
source: followup
created: 2026-10-22T00:30:42Z
updated: 2026-10-22T02:08:37Z
---
# Issue tracking depth: milestones with progress, issue and PR templates (.gitter/ISSUE_TEMP
## Raw request
> Issue tracking depth: milestones with progress, issue and PR templates (.gitter/ISSUE_TEMPLATE), notifications inbox with watching/unwatching repos and threads, @mention notifications (split from T-0008)
## Interpretation
Issue tracking depth. Milestones per repo with a due date, description and open/closed progress, assignable to issues and PRs, with a milestone page. Issue and PR templates from .gitter/ISSUE_TEMPLATE/*.md (front matter name, about, title, labels) and .gitter/pull_request_template.md, offered as a chooser when opening one. Assignees on issues and PRs (people with write). Notifications: an inbox at /notifications for things you participate in, are mentioned or assigned in, or watch; watch, unwatch or ignore a repo, subscribe or unsubscribe a thread; mark read and done; unread count in the top bar. @mentions in issue, PR, comment and discussion bodies notify the mentioned person if they can read the repo.
## Assumptions (confidence)
## Acceptance criteria
- [ ] Milestones with progress and assignment, assignees, templates parsed from .gitter/ISSUE_TEMPLATE and the PR template — verify with `cargo test -q --test api milestones`
- [ ] Notifications for participation, mentions, assignment and watched repos; watch levels and thread subscriptions; read and done; no notifications about repos the user can't read — verify with `cargo test -q --test api notifications`
- [ ] Milestone pages, template chooser, inbox and unread badge render without console errors — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
(a) Tables milestones, issue_assignees, watches (user, repo, level), thread_subs (user, issue), notifications (user, repo, issue, reason, updated_at, read_at, done_at) with one row per user and thread updated in place, written in the same transaction as the event that causes it [chosen: cheap reads, natural grouping like GitHub]; (b) compute notifications from events on read (expensive, hard to mark read). Mentions parsed server-side from markdown text outside code spans; the reader's access is re-checked when the inbox is read, so losing access hides old notifications.
## Risks and rollback
## Execution prompt
## Steps
1. [ ] milestones and assignees: schema, API, filters in issue lists  <- CURRENT
2. [ ] templates: parse .gitter/ISSUE_TEMPLATE/*.md and the PR template from the default branch; chooser on new issue and PR
3. [ ] notifications: schema, fan-out in the same transaction as the triggering event, mentions, watch levels, thread subscriptions, access re-check on read
4. [ ] UI: milestone pages and pickers, assignee picker, template chooser, /notifications inbox with filters, unread count in the top bar, watch button on repos
5. [ ] e2e + ui-check, README
## Resume here
## Docs impact
## Verification evidence
## Log
- 2026-10-22T00:30:42Z created
## Follow-ups captured
## Build vs reuse
Reuses: issues/comments/events transactions in src/api/issues.rs and src/api/reviews.rs (fan-out runs in the same transaction), repo::access for the read check when the inbox is read, labels' picker UI pattern in static/js/pages/issue.js for assignees and milestones, the markdown renderer for templates, and Git::run to read .gitter/ISSUE_TEMPLATE from the default branch. New: milestones, issue_assignees, watches, thread_subs, notifications tables and a /notifications page.
