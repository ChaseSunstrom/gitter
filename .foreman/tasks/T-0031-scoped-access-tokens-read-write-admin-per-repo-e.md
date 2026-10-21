---
id: T-0031
type: SECURITY
tier: M
status: captured
priority: normal
scope: []
depends_on: []
source: followup
created: 2026-10-21T21:11:21Z
updated: 2026-10-21T21:30:58Z
---
# Scoped access tokens (read/write/admin, per-repo, expiry) and deploy keys (split from T-00
## Raw request
> Scoped access tokens (read/write/admin, per-repo, expiry) and deploy keys (split from T-0009)
## Interpretation
Tokens today can do anything their owner can. Give each token an access level (read only / read and write repositories / full account), an optional single repository it's limited to, and an optional expiry. Repository owners can make deploy tokens from repo settings: a token for exactly one repository, read-only or read-write, for CI servers and deploy boxes. Enforced for git over HTTP and the API alike.
## Assumptions (confidence)
## Acceptance criteria
- [ ] Read-only tokens can clone and read but not push or write; repo-limited tokens can't touch other private repos; expired tokens stop working — verify with `cargo test -q --test api scoped_tokens`
- [ ] Deploy tokens are created and listed in repo settings and work for that repo only — verify with `cargo test -q --test api deploy_tokens`
- [ ] Token and deploy-token forms work in a browser — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
(a) Columns on tokens (scope, repo_id, expires_at); the token's limits ride on the User value so the existing access() check and one API middleware enforce them [chosen: one choke point for repos, one for routes]; (b) a separate deploy_keys table (duplicates token auth); (c) per-route scope annotations (spread over 60 handlers, easy to miss one). SSH deploy keys come with SSH (T-0012).
## Risks and rollback
## Execution prompt
## Steps
1. [ ] migration + token limits on User + access() + API scope middleware  <- CURRENT
2. [ ] token form: access level, repository, expiry
3. [ ] deploy tokens in repo settings
4. [ ] tests + browser check
## Resume here
## Docs impact
## Verification evidence
## Log
- 2026-10-21T21:11:21Z created
## Follow-ups captured
