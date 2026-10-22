---
id: T-0022
type: FEATURE
tier: L
status: planned
priority: normal
scope: []
depends_on: []
source: followup
created: 2026-10-21T17:01:54Z
updated: 2026-10-22T23:11:49Z
inferred_deps: [T-0010]
inferred_why: T-0010: multiplayer editor extends existing web editing flows
---
# Multiplayer web editor: people editing the same file see each other's changes and cursors live, then one commits it for all (co-authors credited)
## Raw request
> Multiplayer web editor (C42 pick)
## Interpretation
Several people with write access open the same file on the same branch in the web editor and type at once. Each sees the others' changes within a moment, sees where their cursors are, and doesn't lose their own keystrokes. Any of them commits the shared text (to the branch or a new one, as the editor already does), and the commit credits the others as Co-authored-by. Sessions live in memory: a restart ends them, but each browser still has its text and can start a new one.
## Assumptions (confidence)
## Acceptance criteria
- [ ] two editors of one file converge on the same text under concurrent edits (server transforms each change against the ones it missed) — verify with `cargo test -q --test collab`
- [ ] joining needs write access; sessions are capped (size, people) and end when idle; a commit from the session lists everyone who typed as Co-authored-by and refuses if the branch moved — verify with `cargo test -q --test collab`
- [ ] the editor shows who else is here, their cursors and a live text; two browser tabs typing at once end with the same text — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
Sync model: (a) server-authoritative operational transform on splices (pos, delete count, insert): each change carries the version it was made at, the server transforms it over the changes since, applies it, numbers it and broadcasts it; clients transform their unacknowledged change over incoming ones [chosen: small, deterministic, fits a plain textarea, no library]; (b) a CRDT like Yjs (needs a bundled library and binary sync; the SPA has no build step); (c) one writer at a time with the rest watching (simplest, but not multiplayer editing). Transport: (a) POST to change and SSE to follow, both existing patterns here [chosen]; (b) WebSockets (a new transport and proxy setup). Presence: cursor positions per person, transformed with each change, drawn as markers in an overlay mirroring the textarea. Captured instead: rich-text and multi-file sessions, persistence across restarts.
## Risks and rollback
## Execution prompt
## Steps
1. [ ] server: collaboration sessions keyed by repo, branch and path; splice operations with versions, operational transform, presence; POST to change, SSE to follow; limits and expiry  <- CURRENT
2. [ ] commit from a session: expected_sha of the session's base, Co-authored-by trailers for participants with a verified email
3. [ ] editor UI: join or start, apply remote splices to the textarea keeping the caret, others' cursors in an overlay, participant list; README; ui-check with two pages
## Resume here
## Docs impact
## Verification evidence
## Log
- 2026-10-21T17:01:54Z created
- 2026-10-22T23:11:35Z planned from capture
- 2026-10-22T23:11:48Z approved: the user asked to implement every queued feature ("what about all of the other features you have queued?"); L task proceeds under that standing request
- 2026-10-22T23:11:49Z set approved=True
## Follow-ups captured
## Build vs reuse
Reuses the contents API commit path (expected_sha, new_branch), the SSE pattern from CI logs, the editor page, Repo::writable. New: an in-memory session map (like the OAuth pending map) and a small transform function with unit tests. No new crate.
