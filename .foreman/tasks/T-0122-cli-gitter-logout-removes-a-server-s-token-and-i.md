---
id: T-0122
type: FEATURE
tier: M
status: active
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T23:11:59Z
updated: 2026-10-22T23:12:48Z
batch: [T-0120, T-0121]
builder: 2026-10-22T23:12:33Z
builder_proc: archbox-hardened|pid:[4026531836]|4747|8111
lane: /home/chase/projects/gitter/.claude/worktrees/agent-a9ba505e6dbbdbc87
lane_branch: worktree-agent-a9ba505e6dbbdbc87
base: 6339a7eb39a0b3561f9b00937eaed4765e6da487
base_tree: a936b328b71e9b678215d2012065c3ae56b2ba86
plan0_at: 2026-10-22T23:12:48Z
plan0_scope: []
---
# CLI: gitter logout removes a server's token (and its git helper), and --help after a command prints that command's usage
## Raw request
> T-0120: no `gitter logout`, so a token cannot be removed from the config
> (noticed by an agent; research T-0064-review)
> T-0121: --help` always prints the global usage, not the command's own
> (noticed by an agent; research T-0064-review)
## Interpretation
There's no way to remove a saved token: add gitter logout. It drops the server from ~/.config/gitter/config.json (moving the default if needed) and removes the credential.<server>.helper entries setup-git added (only gitter's own, including the empty reset that precedes it). And --help after a command (or gitter help CMD) shows that command's usage, not the whole list.
## Assumptions (confidence)
## Acceptance criteria
- [ ] T-0120: gitter logout exists and removes a server's token — verify with `cargo test -q --test cli logout`
- [ ] T-0121: --help after a command prints that command's usage — verify with `cargo test -q --test cli help`
- [ ] gitter logout [SERVER] removes that server's saved token and the git credential helper entry setup-git wrote for it; with one server logged in, no argument is needed — verify with `cargo test -q --test cli logout`
- [ ] gitter <command> --help (and gitter help <command>) prints only that command's usage lines and exits 0 — verify with `cargo test -q --test cli help`
## Non-goals
## Approach (options → choice → why)
(a) logout edits config.json and runs git config --global --unset-all with a value regex matching gitter's helper [chosen: removes exactly what setup-git added]; (b) leave git config alone and tell the user (leaves a helper that answers nothing); (c) revoke the token on the server too (needs the token's id; captured: logout --revoke). Help: reuse usage_of(command).
## Risks and rollback
## Execution prompt
## Steps
1. [ ] T-0120: No `gitter logout`, so a token cannot be removed from the config  <- CURRENT
2. [ ] T-0121: --help` always prints the global usage, not the command's own
3. [ ] logout: tests first; remove the host from the config and unset credential.<server>.helper entries gitter wrote (leave others)
4. [ ] --help per command via the existing usage_of(); README and help text
## Resume here
## Docs impact
## Verification evidence
## Log
- 2026-10-22T23:11:59Z created
- 2026-10-22T23:12:13Z approved: the user asked to implement every queued feature ("what about all of the other features you have queued?"); proceeds under that standing request
- 2026-10-22T23:12:14Z set approved=True
- 2026-10-22T23:12:32Z criterion 1 verify: `` → `cargo test -q --test cli logout`
- 2026-10-22T23:12:32Z criterion 1 text: T-0120: No `gitter logout`, so a token cannot be removed from the config works → T-0120: gitter logout exists and removes a server's token
- 2026-10-22T23:12:32Z criterion 2 verify: `` → `cargo test -q --test cli help`
- 2026-10-22T23:12:32Z criterion 2 text: T-0121: --help` always prints the global usage, not the command's own works → T-0121: --help after a command prints that command's usage
- 2026-10-22T23:12:33Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0122.builder.md
- 2026-10-22T23:12:47Z focused
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research T-0064-review: Verdict:** changes needed (1 MEDIUM wrong-token bug, 1 MEDIUM terminal-state bug, several LOW nits). I had no shell, so this is a code reading. I…
- research T-0013-review: Verdict:** approve with nits. I found no token leak and no change to server-side runner behaviour. The findings below are two MEDIUM and several …
- T-0064 [FEATURE M, done, 3 steps] CLI: gitter auth setup-git credential helper, token prompt without echo, unknown flags refused per command — lesson: A credential helpe…
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
