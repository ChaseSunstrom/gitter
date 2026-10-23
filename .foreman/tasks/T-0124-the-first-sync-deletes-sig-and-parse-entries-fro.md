---
id: T-0124
type: FIX
tier: L
status: dropped
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T23:24:22Z
updated: 2026-10-22T23:24:36Z
---
# The first `sync()` deletes `sig-*` and `parse-*` entries from the gpg home, which can remo
## Raw request
> the first `sync()` deletes `sig-*` and `parse-*` entries from the gpg home, which can remove files a concurrent request is using, so a first request can fail spuriously (src/signing.rs:269-276)
> (noticed by an agent; research T-0063-review)
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
- 2026-10-22T23:24:22Z created
- 2026-10-22T23:24:36Z dropped: fixed in T-0063 (review item 8): clean_leftovers removes sig-*/parse-* files only once they're over 10 minutes old, tested by leftovers_are_removed_only_once_they_are_old
## Follow-ups captured
