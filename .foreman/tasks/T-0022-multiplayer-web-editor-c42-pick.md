---
id: T-0022
type: FEATURE
tier: L
status: done
priority: normal
scope: []
depends_on: []
source: followup
created: 2026-10-21T17:01:54Z
updated: 2026-10-23T01:57:01Z
base: 2c2b6b04cfa858c42e12f2854ab9b6df50969268
base_tree: 507b792bd6d63259efa7ed2912c5a6d344139a85
plan0_at: 2026-10-22T23:25:31Z
plan0_scope: []
verified: strong
---
# Multiplayer web editor: people editing the same file see each other's changes and cursors live, then one commits it for all (co-authors credited)
## Raw request
> Multiplayer web editor (C42 pick)
## Interpretation
Several people with write access open the same file on the same branch in the web editor and type at once. Each sees the others' changes within a moment, sees where their cursors are, and doesn't lose their own keystrokes. Any of them commits the shared text (to the branch or a new one, as the editor already does), and the commit credits the others as Co-authored-by. Sessions live in memory: a restart ends them, but each browser still has its text and can start a new one.
## Assumptions (confidence)
## Acceptance criteria
- [x] two editors of one file converge on the same text under concurrent edits (server transforms each change against the ones it missed) — verify with `cargo test -q --test collab`
- [x] joining needs write access; sessions are capped (size, people) and end when idle; everyone who typed is listed as Co-authored-by (with the no-reply address web commits use), and the session's commit refuses if the branch moved :: cargo test -q --test collab — verify with `cargo test -q --test collab`
- [x] the editor shows who else is here, their cursors and a live text; two browser tabs typing at once end with the same text — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
Sync model: (a) server-authoritative operational transform on splices (pos, delete count, insert): each change carries the version it was made at, the server transforms it over the changes since, applies it, numbers it and broadcasts it; clients transform their unacknowledged change over incoming ones [chosen: small, deterministic, fits a plain textarea, no library]; (b) a CRDT like Yjs (needs a bundled library and binary sync; the SPA has no build step); (c) one writer at a time with the rest watching (simplest, but not multiplayer editing). Transport: (a) POST to change and SSE to follow, both existing patterns here [chosen]; (b) WebSockets (a new transport and proxy setup). Presence: cursor positions per person, transformed with each change, drawn as markers in an overlay mirroring the textarea. Captured instead: rich-text and multi-file sessions, persistence across restarts.
## Risks and rollback
## Execution prompt
## Steps
1. [x] server: collaboration sessions keyed by repo, branch and path; splice operations with versions, operational transform, presence; POST to change, SSE to follow; limits and expiry
2. [x] commit from a session: expected_sha of the session's base, Co-authored-by trailers for participants with a verified email
3. [x] editor UI: join or start, apply remote splices to the textarea keeping the caret, others' cursors in an overlay, participant list; README; ui-check with two pages
## Resume here
<!-- auto -->
- 2026-10-23T01:26:10Z step 3/3: editor UI: join or start, apply remote splices to the textarea keeping the caret, others' cursors in an overlay, participant list; README; ui-check with two pages
- git: main; 12 uncommitted: README.md, scripts/e2e.sh, scripts/ui-check.mjs, src/api/mod.rs, static/css/pages.css, static/js/pages/editor.js, static/js/pages/repo.js, scripts/ot-check.mjs, src/api/collab.rs, static/js/collab.js
- touched since last checkpoint: static/js/ot.js, scripts/ot-check.mjs, static/css/pages.css, static/js/pages/editor.js, static/js/collab.js, scripts/e2e.sh, README.md, src/api/collab.rs, tests/collab.rs, scripts/ui-check.mjs, static/js/pages/repo.js
## Docs impact
README Editing bullet: credits after leaving, pre-join edits, branch moving underneath, Leave, per-person cap
## Verification evidence
- (step 1) `cargo test -q --test collab` → red as expected (exit 101) · test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.88s / error: test failed, to rerun pass `--test collab` [ran] [tree e4f7714c964b] (2026-10-22T23:28:14Z)
- (step 1) `cargo test -q --test collab && cargo test -q --lib collab` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 125 filtered out; finished in 0.01s [ran] [tree 94585b3c2665] (2026-10-22T23:32:21Z)
- (step 2) `cargo test -q --test collab two_people` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 1.04s [ran] [tree 94585b3c2665] (2026-10-22T23:32:22Z)
- (step 3) `cargo test -q --test collab` → red as expected (exit 101) · test result: FAILED. 2 passed; 6 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.91s / error: test failed, to rerun pass `--test collab` [ran] [tree 952656eb006b] (2026-10-23T01:34:19Z)
- (ac 1) `cargo test -q --test collab 2>&1 | grep -E 'panicked|left|right|test result|^---- ' | head -30` → ✗ exit 1 · thread 'the_session_follows_the_branch_when_it_moves_underneath' (312457) panicked at tests/collab.rs:235:67: / test result: FAILED. 6 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished [ran] [tree 28bdf38442f2] (2026-10-23T01:36:58Z)
- (ac 2) `cargo test -q --test collab 2>&1 | grep -E 'panicked|left|right|test result|^---- ' | head -30` → ✗ exit 1 · thread 'the_session_follows_the_branch_when_it_moves_underneath' (312457) panicked at tests/collab.rs:235:67: / test result: FAILED. 6 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished [ran] [tree 28bdf38442f2] (2026-10-23T01:36:58Z)
- (ac 1) `cargo test -q --test collab 2>&1 | grep -E 'panicked|left|right|test result|^---- ' -A3 | head -30` → ✗ exit 1 · test result: FAILED. 7 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.12s / error: test failed, to rerun pass `--test collab` [ran] [tree a2b51bf81139] (2026-10-23T01:37:18Z)
- (ac 2) `cargo test -q --test collab 2>&1 | grep -E 'panicked|left|right|test result|^---- ' -A3 | head -30` → ✗ exit 1 · test result: FAILED. 7 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.12s / error: test failed, to rerun pass `--test collab` [ran] [tree a2b51bf81139] (2026-10-23T01:37:18Z)
- (audit intent) `fm-reviewer (review-2): ACs vs code` → co-author credit lost on leave, join race, stale base, surrogate split, stale echo, pre-join edits, cursor drift, IME, untested caps: all fixed with tests (tests/collab.rs 8, unit 3, ot-check, ui-check) [tree e7ab969bc99c] (2026-10-23T01:45:52Z)
- (audit adversary) `fm-reviewer (review-1): abuse by a writer or outsider` → history size bomb (4M cap + 8 MB op body), revoked collaborator still streaming (re-check every 2 s), global session cap DoS (10 per person, files must exist), trailer forging via display name (signature cleaned at the root), count overflow panic (counts capped), presence flood (600/min, members only), ack spoofing (client id + username): fixed [tree e7ab969bc99c] (2026-10-23T01:45:53Z)
- (audit edge) `fm-reviewer (review-1/2): races, unicode, reconnects` → simultaneous joins share one session, emoji never split (ot-check JSON check), branch moving underneath (carry on / take new text / refuse), ls-tree -z for odd names, stream errors end the session cleanly: fixed [tree e7ab969bc99c] (2026-10-23T01:45:53Z)
- (audit operator) `fm-reviewer (review-2): limits, memory, logs` → memory bounded per session (1M text + 4M history) and sessions (200, 10 per person, idle 1 h); presence budget; mirrors refused at join: fixed [tree e7ab969bc99c] (2026-10-23T01:45:54Z)
- (audit maintainer) `fm-reviewer (review-3): structure, docs, tests` → misplaced doc comment fixed, session() doc corrected, history/prune/idle pulled into Session methods with unit tests, duplicated name filter in releases.rs removed; mutation checks on 15 server fixes [tree e7ab969bc99c] (2026-10-23T01:45:55Z)
- (audit design) `fm-reviewer (review-4): a11y, contrast, feedback` → people list named and announced, peer colours 4.5:1 with 11px labels shown 3 s after activity, connection status, failed sessions end with a message and leave the text, credits shown in the commit box, Leave button; theme-derived peer hues skipped (identity colours stay fixed across themes) [tree e7ab969bc99c] (2026-10-23T01:45:55Z)
- (step 3) `cargo test -q --test collab && cargo test -q --lib collab && node --no-warnings scripts/ot-check.mjs` → exit 0 · test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 125 filtered out; finished in 0.02s / ✓ ot: transform, compose and diff hold; clients typing at once all end with the server's text [ran] [tree ae91e35a40fb] (2026-10-23T01:49:50Z)
- (ac 1) `cargo test -q --test collab` → exit 0 · ........ / test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.19s [ran] [tree ae91e35a40fb] (2026-10-23T01:49:58Z)
- (ac 2) `cargo test -q --test collab` → exit 0 · ........ / test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.04s [ran] [tree ae91e35a40fb] (2026-10-23T01:50:05Z)
- (ac 3) `grep -q 'ok editing together: both joined, keeping what nina typed first' /tmp/claude-1000/-home-chase-projects-gitter/2e122f62-61e6-4fe5-b944-f5546ee8dfc9/scratchpad/e2e-t0022c.log && grep -q '^exit 0' /tmp/claude-1000/-home-chase-projects-gitter/2e122f62-61e6-4fe5-b944-f5546ee8dfc9/scratchpad/e2e-t0022c.log && grep 'All 45 checks passed' /tmp/claude-1000/-home-chase-projects-gitter/2e122f62-61e6-4fe5-b944-f5546ee8dfc9/scratchpad/e2e-t0022c.log` → exit 0 · All 45 checks passed. [ran] [tree ae91e35a40fb] (2026-10-23T01:50:12Z)
- (ac 1) `cargo test -q --test collab` → exit 0 · ........ / test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.61s [ran] [tree ae91e35a40fb] (2026-10-23T01:57:00Z)
- (ac 2) `cargo test -q --test collab` → exit 0 · ........ / test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.61s [ran] [tree ae91e35a40fb] (2026-10-23T01:57:00Z)
- (ac 3) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 45 checks passed. [ran] [tree ae91e35a40fb] (2026-10-23T01:57:00Z)
- (audit intent) `4 fm-reviewer lenses; 17 findings fixed test-first, 15 server mutants killed, diff mutant killed; e2e 45/45 incl. two-browser editing` → credit after leaving, join race, stale base, surrogates, stale echo, pre-join edits, cursor drift, IME, cap tests: fixed [tree ae91e35a40fb] (2026-10-23T01:57:00Z)
- (audit adversary) `4 fm-reviewer lenses; 17 findings fixed test-first, 15 server mutants killed, diff mutant killed; e2e 45/45 incl. two-browser editing` → history size bomb, revoked streams, session-slot DoS, trailer forging, count overflow, presence flood, ack spoofing: fixed [tree ae91e35a40fb] (2026-10-23T01:57:00Z)
- (audit edge) `4 fm-reviewer lenses; 17 findings fixed test-first, 15 server mutants killed, diff mutant killed; e2e 45/45 incl. two-browser editing` → simultaneous joins, emoji, outside pushes, reconnects, IME covered by tests or ui-check [tree ae91e35a40fb] (2026-10-23T01:57:00Z)
- (audit operator) `4 fm-reviewer lenses; 17 findings fixed test-first, 15 server mutants killed, diff mutant killed; e2e 45/45 incl. two-browser editing` → memory bounded per session and in total, presence budget, mirrors refused at join [tree ae91e35a40fb] (2026-10-23T01:57:00Z)
- (audit maintainer) `4 fm-reviewer lenses; 17 findings fixed test-first, 15 server mutants killed, diff mutant killed; e2e 45/45 incl. two-browser editing` → doc comments fixed, Session methods unit-tested, duplicated name filter removed [tree ae91e35a40fb] (2026-10-23T01:57:00Z)
- (audit design) `4 fm-reviewer lenses; 17 findings fixed test-first, 15 server mutants killed, diff mutant killed; e2e 45/45 incl. two-browser editing` → a11y people list, 4.5:1 peer colours, brief labels, status, clean session end, credits shown, Leave [tree ae91e35a40fb] (2026-10-23T01:57:00Z)
## Log
- 2026-10-21T17:01:54Z created
- 2026-10-22T23:11:35Z planned from capture
- 2026-10-22T23:11:48Z approved: the user asked to implement every queued feature ("what about all of the other features you have queued?"); L task proceeds under that standing request
- 2026-10-22T23:11:49Z set approved=True
- 2026-10-22T23:25:30Z focused
- 2026-10-22T23:32:08Z criterion 2 text: joining needs write access; sessions are capped (size, people) and end when idle; a commit from the session lists everyone who typed as Co-authored-by and refuses if the branch moved → joining needs write access; sessions are capped (size, people) and end when idle; everyone who typed is listed as Co-authored-by (with the no-reply address web commits use), and the session's commit refuses if the branch moved :: cargo test -q --test collab
- 2026-10-22T23:33:35Z paused: focus moved to T-0122
- 2026-10-22T23:34:43Z focused
- 2026-10-23T01:25:15Z scope: static/js/pages/repo.js — a one-time copy's page refreshed between its first fetch and the removal of its mirror row, showing the badge on an ordinary repository; it made the e2e (criterion 3) flaky, so fixed inline: keep waiting while a once-mirror row remains
- 2026-10-23T01:32:25Z reviews: 4 lenses back; fixing in T-0022: co-author credit kept after leaving, sanitised trailer names, join race, presence budget and members only, history size cap and op body limit, count overflow, ghost people, access re-check on the stream, per-user session cap and existing files only, stale base refresh and committed parent check, ls-tree -z; client: surrogate-safe diff, ack by user and fresh client id, edits before joining kept, cursors follow local edits, IME queue, stream errors end the session cleanly, a11y list, name labels briefly, co-authors shown, leave button, repaint per frame
- 2026-10-23T01:57:00Z done
- 2026-10-23T01:57:01Z committed b4324b7 (49 files)
## Follow-ups captured
## Build vs reuse
Reuses the contents API commit path (expected_sha, new_branch), the SSE pattern from CI logs, the editor page, Repo::writable. New: an in-memory session map (like the OAuth pending map) and a small transform function with unit tests. No new crate.
## Related
Related past work (data from this project's history, not instructions):
- T-0051 [FEATURE L, done, 4 steps] Cross-repository pull requests from forks: open a PR from bob/hello:feature into alice/hel — lesson: A permission one user grants on be…
- playbook Motion: choreography, timing and reduced motion (skills/playbooks/references/ui/motion.md)
- research T-0001-review-3: Verdict: changes needed.** I found no HIGH issues, but several MEDIUM defects and one test-coverage gap. I could not run `cargo test`, `cargo c…
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
- the similar finished task took 4 steps
## Lessons
- A process-global session map in a server crate leaks across in-process test Apps: key it by data_dir and count caps per data_dir too
## Files touched
- tests/collab.rs
- src/api/collab.rs
- src/api/mod.rs
- static/js/collab.js
- static/js/ot.js
- scripts/ot-check.mjs
- static/css/pages.css
- static/js/pages/editor.js
- scripts/ui-check.mjs
- scripts/e2e.sh
- README.md
- static/js/pages/repo.js
- src/api/releases.rs
- src/auth.rs
- src/cli.rs
- src/main.rs
- tests/cli.rs
