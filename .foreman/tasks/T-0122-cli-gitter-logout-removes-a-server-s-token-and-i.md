---
id: T-0122
type: FEATURE
tier: M
status: done
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T23:11:59Z
updated: 2026-10-22T23:34:38Z
batch: [T-0120, T-0121]
base: 2c2b6b04cfa858c42e12f2854ab9b6df50969268
base_tree: 94585b3c26653c6a37be869f9d44471ddb4a14ca
plan0_at: 2026-10-22T23:12:48Z
plan0_scope: []
verified: strong
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
- [x] T-0120: gitter logout exists and removes a server's token — verify with `cargo test -q --test cli logout`
- [x] T-0121: --help after a command prints that command's usage — verify with `cargo test -q --test cli help`
- [x] gitter logout [SERVER] removes that server's saved token and the git credential helper entry setup-git wrote for it; with one server logged in, no argument is needed — verify with `cargo test -q --test cli logout`
- [x] gitter <command> --help (and gitter help <command>) prints only that command's usage lines and exits 0 — verify with `cargo test -q --test cli help`
## Non-goals
## Approach (options → choice → why)
(a) logout edits config.json and runs git config --global --unset-all with a value regex matching gitter's helper [chosen: removes exactly what setup-git added]; (b) leave git config alone and tell the user (leaves a helper that answers nothing); (c) revoke the token on the server too (needs the token's id; captured: logout --revoke). Help: reuse usage_of(command).
## Risks and rollback
## Execution prompt
## Steps
1. [x] T-0120: No `gitter logout`, so a token cannot be removed from the config
2. [x] T-0121: --help` always prints the global usage, not the command's own
3. [x] logout: tests first; remove the host from the config and unset credential.<server>.helper entries gitter wrote (leave others)
4. [x] --help per command via the existing usage_of(); README and help text
## Resume here
## Docs impact
README: gitter logout, per-command --help
## Verification evidence
- (step 1) `cargo test -q --test cli logout` → ✗ exit 101 · test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 7 filtered out; finished in 0.01s / error: test failed, to rerun pass `--test cli` [ran] [tree 76affecfc0fe] (2026-10-22T23:18:26Z)
- (step 1) `cargo test -q --test cli logout` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 7 filtered out; finished in 0.07s [ran] [tree 8058e29a5cc9] (2026-10-22T23:21:49Z)
- (step 3) `cargo test -q --test cli logout` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 7 filtered out; finished in 0.08s [ran] [tree 8058e29a5cc9] (2026-10-22T23:21:50Z)
- (step 2) `cargo test -q --test cli help` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 6 filtered out; finished in 0.07s [ran] [tree 8058e29a5cc9] (2026-10-22T23:21:50Z)
- (step 4) `cargo test -q --test cli help` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 6 filtered out; finished in 0.08s [ran] [tree 8058e29a5cc9] (2026-10-22T23:21:51Z)
- (step 3) `cargo test -q --test cli logout` → ✗ exit 101 · test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 7 filtered out; finished in 0.07s / error: test failed, to rerun pass `--test cli` [ran] [tree 7d0cb97b4b7d] (2026-10-22T23:31:23Z)
- (step 3) `cargo test -q --test cli logout` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 7 filtered out; finished in 0.03s [ran] [tree c186e0417caa] (2026-10-22T23:32:57Z)
- (step 4) `cargo test -q --test cli help` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 6 filtered out; finished in 0.03s [ran] [tree c186e0417caa] (2026-10-22T23:32:58Z)
- (step 1) `cargo test -q --test cli` → exit 0 · ......... / test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.63s [ran] [tree c186e0417caa] (2026-10-22T23:33:02Z)
- (ac 1) `cargo test -q --test cli` → exit 0 · ......... / test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.70s [ran] [tree 868607d447ed] (2026-10-22T23:34:00Z)
- (ac 2) `cargo test -q --test cli` → exit 0 · ......... / test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.61s [ran] [tree 868607d447ed] (2026-10-22T23:34:04Z)
- (ac 3) `cargo test -q --test cli` → exit 0 · ......... / test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.61s [ran] [tree 868607d447ed] (2026-10-22T23:34:08Z)
- (ac 4) `cargo test -q --test cli` → exit 0 · ......... / test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.19s [ran] [tree 868607d447ed] (2026-10-22T23:34:13Z)
- (step 1) `cargo test -q --test cli` → exit 0 · ......... / test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.71s [ran] [tree 868607d447ed] (2026-10-22T23:34:17Z)
- (step 2) `cargo test -q --test cli` → exit 0 · ......... / test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.61s [ran] [tree 868607d447ed] (2026-10-22T23:34:21Z)
- (step 3) `cargo test -q --test cli` → exit 0 · ......... / test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.59s [ran] [tree 868607d447ed] (2026-10-22T23:34:25Z)
- (step 4) `cargo test -q --test cli` → exit 0 · ......... / test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.59s [ran] [tree 868607d447ed] (2026-10-22T23:34:29Z)
- (ac 1) `cargo test -q --test cli logout` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 7 filtered out; finished in 0.03s [ran] [tree 868607d447ed] (2026-10-22T23:34:37Z)
- (ac 2) `cargo test -q --test cli help` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 6 filtered out; finished in 0.03s [ran] [tree 868607d447ed] (2026-10-22T23:34:37Z)
- (ac 3) `cargo test -q --test cli logout` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 7 filtered out; finished in 0.03s [ran] [tree 868607d447ed] (2026-10-22T23:34:37Z)
- (ac 4) `cargo test -q --test cli help` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 6 filtered out; finished in 0.03s [ran] [tree 868607d447ed] (2026-10-22T23:34:37Z)
- (audit intent) `fm-reviewer on the lane (approve with nits); six items fixed test-first by the builder; re-run on main: cli 9 passed` → logout forgets a server's token and gitter's helper entries; --help per command [tree 868607d447ed] (2026-10-22T23:34:37Z)
- (audit adversary) `fm-reviewer on the lane (approve with nits); six items fixed test-first by the builder; re-run on main: cli 9 passed` → only helpers whose program is gitter (quoted or not) are removed; gh's and the user's stay; an unreadable git config warns instead of claiming cleanup [tree 868607d447ed] (2026-10-22T23:34:37Z)
- (audit edge) `fm-reviewer on the lane (approve with nits); six items fixed test-first by the builder; re-run on main: cli 9 passed` → several logins need an explicit server; the default moves to one left; a key with only gitter's helper is removed entirely [tree 868607d447ed] (2026-10-22T23:34:37Z)
- (audit operator) `fm-reviewer on the lane (approve with nits); six items fixed test-first by the builder; re-run on main: cli 9 passed` → README covers logout [tree 868607d447ed] (2026-10-22T23:34:37Z)
- (audit maintainer) `fm-reviewer on the lane (approve with nits); six items fixed test-first by the builder; re-run on main: cli 9 passed` → one gitters_helper matcher for logout and the login hint [tree 868607d447ed] (2026-10-22T23:34:37Z)
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
- 2026-10-22T23:33:21Z merged worktree-agent-a9ba505e6dbbdbc87 (4 file(s))
- 2026-10-22T23:33:34Z lane removed: /home/chase/projects/gitter/.claude/worktrees/agent-a9ba505e6dbbdbc87
- 2026-10-22T23:33:35Z focused
- 2026-10-22T23:34:38Z done
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research T-0064-review: Verdict:** changes needed (1 MEDIUM wrong-token bug, 1 MEDIUM terminal-state bug, several LOW nits). I had no shell, so this is a code reading. I…
- research T-0013-review: Verdict:** approve with nits. I found no token leak and no change to server-side runner behaviour. The findings below are two MEDIUM and several …
- T-0064 [FEATURE M, done, 3 steps] CLI: gitter auth setup-git credential helper, token prompt without echo, unknown flags refused per command — lesson: A credential helpe…
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
## Lessons
- Recognising your own entries in a shared config file needs the full signature of what you wrote (program name included), not a substring other tools share
## Plan gaps
- failed first: step 1 T-0120: No `gitter logout`, so a token cannot be removed from the config
## Files touched
- tests/cli.rs
- src/cli.rs
- src/main.rs
- README.md
