## Verdict
Changes needed. I read every hunk and checked claims against the code; I ran nothing (read-only), so the reproducers come from reading the code.

## lens: intent

- HIGH — src/api/collab.rs:481 (`Watching::drop`; `state` at :254) — Leaving a session deletes the `Person`, including `typed`, so "everyone who typed is listed as Co-authored-by" fails for anyone who typed and then closed the tab. An SSE reconnect does the same, since the old stream drops (streams reach 0) before the new one enters with `typed=false`. — Keep a separate `typed: HashMap<i64, signature>` on the session that survives leave and rejoin. — confirmed
  - Reproducer: nina types and closes her tab, then alice's `GET /collab/{id}` returns `co_authors: []`.
- MEDIUM — src/api/collab.rs:341 — `join` looks the session up under one lock, awaits git, then inserts without checking again. Two people opening the invite link at once get two sessions for the same file, and they never converge (the AC fails). — Re-run the lookup under the second lock before inserting. — confirmed
- MEDIUM — src/api/collab.rs:461 and static/js/collab.js:130 — `committed` accepts any sha that is the branch head, with no check that it descends from `base`. Anyone in the session who POSTs `{sha: head}` after an outside push makes the stale session text the base, and the next commit silently overwrites the push. The only way to refresh `base` is this call, so a session otherwise stays wedged ("Someone else changed this branch") for everyone. The committer's `committed()` also swallows its own failure with `.catch(() => {})`, which wedges the others the same way. — Require `rev-parse sha^ == base`, surface the failure, and offer a rebase or reload. — confirmed
- MEDIUM — static/js/ot.js:64 — `diff()` can put half of an emoji into an insert. Select 😀 and type 😁 (same high surrogate), and the op is `[1,-1,"\uDE01"]`. serde_json rejects a lone surrogate (serde_json-1.0.154 read.rs:963), the server answers 400, and `resync` throws away the user's edit. — Widen the prefix and suffix to code-point boundaries in `diff`. — confirmed
- MEDIUM — static/js/collab.js:99 — After a `reset` (reconnect `init`, or a failed `sendOp`), `client` is unchanged. A POST that was already in flight still lands on the server, and its echo takes the "ours" branch, which only bumps `revision` and never applies the op. The server has the text and this tab doesn't. — Make `client` a `let` and regenerate it in `reset`, so stale echoes apply as remote ops. — confirmed
- LOW — static/js/collab.js:41 — `reset` replaces the textarea with the session text, so edits typed before clicking "Edit together" vanish without a warning. — Confirm first, or send the local text as an op. — confirmed
- LOW — static/js/collab.js:57-64 — `local()` never moves `people[].cursor`, so peers' markers drift from the text until they move again. — Run `moved` over the cursors in `local()`. — confirmed
- LOW — static/js/collab.js:65 — `remote()` overwrites `ta.value` during IME composition, which destroys the composition text. — Queue remote ops while `composing`. — confirmed
- LOW — static/js/collab.js:111 — The check `: ${ev.user} <` compares against the display name (auth.rs:120). Anyone with a display name triggers a `GET` for every remote op from them. — Compare on the username, or refetch only on `presence`. — confirmed
- LOW — static/js/collab.js:89 and :125 — `EventSource` has no `onerror`. If the stream never connects (409, or a buffering proxy), the textarea stays read-only with no message. — Add `onerror` with a timeout and a toast. — confirmed
- LOW — static/js/collab.js:49 — If `resync` fails, `ready` stays false and later typing is never sent. — Retry the resync or tell the user. — confirmed
- LOW — src/api/collab.rs:310-313 — Idle sessions are cleaned only inside `join`, and any open tab keeps a session alive forever ("end when idle"). — Run a periodic sweep, and let idleness end a session even if a tab is open. — confirmed
- MEDIUM — tests/collab.rs — Four AC points have no test: the 20-person cap, the session cap, idle expiry, and the commit refusing when the branch moved. `cargo test --test collab` therefore can't verify AC 2. — Add those four tests. — confirmed

## lens: operator

- MEDIUM — src/api/collab.rs:287 with src/api/account.rs:312 — The `Co-authored-by` line is built from another user's `display_name`, and `clip` only limits its length. A display name containing a newline or `<>` injects trailer lines into the committer's commit message. — Strip control characters and `<>` from the name, or use the username. — confirmed
- MEDIUM — src/api/collab.rs:33 and :412 with src/api/mod.rs:338 — History is capped by count (1000), not bytes. The body limit is 40 MB, so each op can be `[-1M, "x"×1M]` (about 2 MB resident). One writer can pin about 2 GB per session within about a minute (900 ops per minute) and across up to 200 sessions. — Cap history by total code units and drop the oldest entries. — confirmed
- MEDIUM — src/api/collab.rs:494 — Write access is checked only when the stream opens. A collaborator removed afterwards, even from a private repo, keeps receiving every text change. — Re-check `Repo::writable` periodically in the stream. — confirmed
- LOW — src/api/collab.rs:434 and src/api/mod.rs:418-420 — `presence` is exempt from the write limiter and has no limiter of its own, although the comment says it has "its own budget". A member flooding it overflows the 256-slot broadcast, which kicks all other members. — Call `st.limiter.check` in `presence`. — confirmed
- LOW — src/api/collab.rs:438 — `presence` calls `enter`, so a late presence POST after the stream closed re-adds a ghost with streams=0 that is never removed and counts toward the cap of 20. — Make `presence` reject non-members. — confirmed
- LOW — src/api/collab.rs:88-93 — Counts are added without overflow checks. `[9223372036854775807, 9223372036854775807, N]` wraps `base_len` in release (and `Build` merging panics in debug), and `apply` then panics on a slice. — Cap counts in `parse` and use `checked_add`. — confirmed
- LOW — src/api/mod.rs:415-417 — The `is_view_count` doc comment now sits on top of `is_collab_typing`'s. — Move the view-count lines back above `is_view_count`. — confirmed
- LOW — static/js/pages/repo.js:135 — The `mirror.once` polling change has nothing to do with collab. — Split it into its own task and commit. — confirmed
- The diff also carries committed T-0122 logout hunks (src/cli.rs, src/main.rs, tests/cli.rs, README line 214), because its base is stale.

**Pre-audit hits**
- The password fixture in `ui-check.mjs` carries `pragma: allowlist`, so I dismissed it.
- The `console.log` calls match the file's existing style, so I dismissed them.
- The `innerHTML` sites use `esc` or the `html` template, so I dismissed them.

## Acceptance criteria
1. Converge under concurrent edits: met on the server (tests/collab.rs; the ot-check model). The client breaks it in the cases above (surrogates, stale echo, the join race).
2. Write access, caps, idle, co-authors, refusal if the branch moved: partly met. The co-author credit is lost when someone leaves, the caps and idle expiry are untested, and refusal relies on the contents API.
3. Presence and two tabs ending with the same text: met by the ui-check scenario, which I did not run.

## Implicit decisions
- Sessions are in memory only.
- Positions count UTF-16 code units.
- The first-arriving insert goes first at a tie.
- The session id is the only handle.
- Rate limit of 900 ops per minute per user.
- History of 1000 ops.
- Idle timeout of one hour.
- Cap of 200 sessions.
- No-reply address comes from `User::signature`.

## Not checked
Actual execution (no tests or browser), CSS rendering and accessibility, and the `git.ok` output cap for huge blobs read in `join`.

**Noticed:** `static/js/collab.js` — `ta.value = …` in `remote()` clears the browser's undo history, so Ctrl-Z stops working in a session.
