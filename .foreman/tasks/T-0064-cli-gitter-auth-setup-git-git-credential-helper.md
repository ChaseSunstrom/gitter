---
id: T-0064
type: FEATURE
tier: M
status: done
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T02:08:37Z
updated: 2026-10-22T23:06:45Z
base: 0821f639082fdff6f6b52cbcb9c4aaa04e6fbfdc
base_tree: 0b1cf46118191286ae19768b6c2111f862d989fa
plan0_at: 2026-10-22T22:10:53Z
plan0_scope: []
verified: strong
---
# CLI: gitter auth setup-git credential helper, token prompt without echo, unknown flags refused per command
## Raw request
> CLI: 'gitter auth setup-git' git credential helper so plain git push works after gitter repo clone (like gh auth setup-git); token prompt without echo; reject unknown flags per command (from T-0013 builder)
## Interpretation
After gitter login, git itself still needs credentials. Add gitter auth setup-git (writes credential.<server>.helper = '!gitter auth git-credential' for each logged-in server, like gh auth setup-git) and gitter auth git-credential (git's helper protocol: answers get for those servers with username x-access-token and the saved token; ignores store/erase). The login prompt reads the token without echo on a terminal, and any flag a command doesn't take is refused with that command's usage.
## Assumptions (confidence)
## Acceptance criteria
- [x] after gitter login and gitter auth setup-git, a plain git clone/push to that server works with no token in the URL — verify with `cargo test -q --test cli`
- [x] the login token prompt doesn't echo what's typed (a terminal), and still reads a piped token — verify with `cargo test -q --test cli`
- [x] a flag a command doesn't take is refused with the command's usage — verify with `cargo test -q --test cli`
## Non-goals
## Approach (options → choice → why)
Options: (a) a credential helper in the gitter binary, set per server with git config --global [chosen: what gh does, no token in remote URLs or files git owns]; (b) rewrite remotes with the token in the URL (leaks into git config and logs); (c) an http.extraHeader in the global config (applies to every host with that prefix and is visible to all git commands). No echo via termios (libc already a dependency? else rpassword-free: stty -echo through std::process). Per-command flags: a table command → accepted flags, checked in one place before dispatch.
## Risks and rollback
## Execution prompt
## Steps
1. [x] gitter auth setup-git and gitter auth git-credential (git's helper protocol: get/store/erase), scoped to logged-in servers (test first)
2. [x] no-echo token prompt
3. [x] per-command flag lists; unknown flags refused; README and help
## Resume here
## Docs impact
README: gitter auth setup-git, the per-server helper, the hidden prompt
## Verification evidence
- (ac 1) `cargo test -q --test cli 2>&1 | grep -E "^test |panicked|test result|error" | head -30` → ✗ exit 1 · test result: FAILED. 3 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.62s / error: test failed, to rerun pass `--test cli` [ran] [tree 848168ed8e72] (2026-10-22T22:13:43Z)
- (ac 2) `cargo test -q --test cli 2>&1 | grep -E "^test |panicked|test result|error" | head -30` → ✗ exit 1 · test result: FAILED. 3 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.62s / error: test failed, to rerun pass `--test cli` [ran] [tree 848168ed8e72] (2026-10-22T22:13:43Z)
- (ac 3) `cargo test -q --test cli 2>&1 | grep -E "^test |panicked|test result|error" | head -30` → ✗ exit 1 · test result: FAILED. 3 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.62s / error: test failed, to rerun pass `--test cli` [ran] [tree 848168ed8e72] (2026-10-22T22:13:43Z)
- (ac 1) `cargo test -q --test cli 2>&1 | grep -E "^test |panicked|test result|^error|^warning" -A3 | head -40` → ✗ exit 1 · test result: FAILED. 4 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.57s / error: test failed, to rerun pass `--test cli` [ran] [tree eb3763f7d9f8] (2026-10-22T22:15:00Z)
- (ac 2) `cargo test -q --test cli 2>&1 | grep -E "^test |panicked|test result|^error|^warning" -A3 | head -40` → ✗ exit 1 · test result: FAILED. 4 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.57s / error: test failed, to rerun pass `--test cli` [ran] [tree eb3763f7d9f8] (2026-10-22T22:15:00Z)
- (ac 3) `cargo test -q --test cli 2>&1 | grep -E "^test |panicked|test result|^error|^warning" -A3 | head -40` → ✗ exit 1 · test result: FAILED. 4 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.57s / error: test failed, to rerun pass `--test cli` [ran] [tree eb3763f7d9f8] (2026-10-22T22:15:00Z)
- (ac 1) `cargo test -q --test cli 2>&1 | grep -E "^test |panicked|test result|^error|^warning" -A3 | head -40` → ✗ exit 1 · test result: FAILED. 5 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.60s / error: test failed, to rerun pass `--test cli` [ran] [tree 5419f95f0232] (2026-10-22T22:15:24Z)
- (ac 2) `cargo test -q --test cli 2>&1 | grep -E "^test |panicked|test result|^error|^warning" -A3 | head -40` → ✗ exit 1 · test result: FAILED. 5 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.60s / error: test failed, to rerun pass `--test cli` [ran] [tree 5419f95f0232] (2026-10-22T22:15:24Z)
- (ac 3) `cargo test -q --test cli 2>&1 | grep -E "^test |panicked|test result|^error|^warning" -A3 | head -40` → ✗ exit 1 · test result: FAILED. 5 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.60s / error: test failed, to rerun pass `--test cli` [ran] [tree 5419f95f0232] (2026-10-22T22:15:24Z)
- (ac 1) `cargo test -q --test cli 2>&1 | grep -E "^test |panicked|test result|^error|^warning" -A3 | head -40` → exit 0 · test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.84s [ran] [tree 3d59e7069b9d] (2026-10-22T22:15:35Z)
- (ac 2) `cargo test -q --test cli 2>&1 | grep -E "^test |panicked|test result|^error|^warning" -A3 | head -40` → exit 0 · test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.84s [ran] [tree 3d59e7069b9d] (2026-10-22T22:15:35Z)
- (ac 3) `cargo test -q --test cli 2>&1 | grep -E "^test |panicked|test result|^error|^warning" -A3 | head -40` → exit 0 · test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.84s [ran] [tree 3d59e7069b9d] (2026-10-22T22:15:35Z)
- (step 1) `cargo test -q --test cli 2>&1 | grep -E "^test |panicked|test result|^error|^warning" -A3 | head -40` → exit 0 · test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.84s [ran] [tree 3d59e7069b9d] (2026-10-22T22:15:35Z)
- (step 1) `cargo test -q --test cli` → exit 0 · ...... / test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.75s [ran] [tree 1529f6bf8380] (2026-10-22T22:16:32Z)
- (step 2) `cargo test -q --test cli` → exit 0 · ...... / test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.63s [ran] [tree 1529f6bf8380] (2026-10-22T22:16:36Z)
- (step 3) `cargo test -q --test cli` → exit 0 · ...... / test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.59s [ran] [tree 1529f6bf8380] (2026-10-22T22:16:40Z)
- (ac 1) `cargo test -q --test cli 2>&1 | grep -E "panicked|test result|^error|^warning" -A4 | head -40` → ✗ exit 1 · test result: FAILED. 3 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.91s / error: test failed, to rerun pass `--test cli` [ran] [tree e12f21c9b7a2] (2026-10-22T22:22:43Z)
- (ac 2) `cargo test -q --test cli 2>&1 | grep -E "panicked|test result|^error|^warning" -A4 | head -40` → ✗ exit 1 · test result: FAILED. 3 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.91s / error: test failed, to rerun pass `--test cli` [ran] [tree e12f21c9b7a2] (2026-10-22T22:22:43Z)
- (ac 3) `cargo test -q --test cli 2>&1 | grep -E "panicked|test result|^error|^warning" -A4 | head -40` → ✗ exit 1 · test result: FAILED. 3 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.91s / error: test failed, to rerun pass `--test cli` [ran] [tree e12f21c9b7a2] (2026-10-22T22:22:43Z)
- (step 3) `cargo test -q --test cli` → exit 0 · ...... / test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.70s [ran] [tree 4ade16116bdd] (2026-10-22T22:26:16Z)
- (ac 1) `cargo test -q --test cli` → exit 0 · ...... / test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.65s [ran] [tree a936b328b71e] (2026-10-22T23:05:33Z)
- (ac 2) `cargo test -q --test cli` → exit 0 · ...... / test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.70s [ran] [tree a936b328b71e] (2026-10-22T23:05:37Z)
- (ac 3) `cargo test -q --test cli` → exit 0 · ...... / test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.68s [ran] [tree a936b328b71e] (2026-10-22T23:05:42Z)
- (step 1) `cargo test -q --test cli && cargo test -q --lib cli::` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 116 filtered out; finished in 0.00s [ran] [tree a936b328b71e] (2026-10-22T23:06:08Z)
- (step 2) `cargo test -q --test cli && cargo test -q --lib cli::` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 116 filtered out; finished in 0.00s [ran] [tree a936b328b71e] (2026-10-22T23:06:13Z)
- (step 3) `cargo test -q --test cli && cargo test -q --lib cli::` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 116 filtered out; finished in 0.00s [ran] [tree a936b328b71e] (2026-10-22T23:06:19Z)
- (ac 1) `cargo test -q --test cli` → exit 0 · ...... / test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.45s [ran] [tree a936b328b71e] (2026-10-22T23:06:44Z)
- (ac 2) `cargo test -q --test cli` → exit 0 · ...... / test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.45s [ran] [tree a936b328b71e] (2026-10-22T23:06:44Z)
- (ac 3) `cargo test -q --test cli` → exit 0 · ...... / test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.45s [ran] [tree a936b328b71e] (2026-10-22T23:06:44Z)
- (audit intent) `fm-reviewer on the lane (five lenses), all findings fixed test-first by the builder and re-run here on main (cargo test --test cli: 6 passed; lib cli: 3 passed)` → plain git clone/push work after setup-git with no token in URLs; hidden prompt; per-command flags refused with usage [tree a936b328b71e] (2026-10-22T23:06:44Z)
- (audit adversary) `fm-reviewer on the lane (five lenses), all findings fixed test-first by the builder and re-run here on main (cargo test --test cli: 6 passed; lib cli: 3 passed)` → each helper entry names its server so only that server's token is given; scheme/host/path matched; http downgrade refused; newline-free tokens; oversized input answered with nothing; flag errors don't echo values [tree a936b328b71e] (2026-10-22T23:06:44Z)
- (audit edge) `fm-reviewer on the lane (five lenses), all findings fixed test-first by the builder and re-run here on main (cargo test --test cli: 6 passed; lib cli: 3 passed)` → signals restore the saved terminal state (exit 130/143/129); stty failure refuses; missing script(1) skips the tty test [tree a936b328b71e] (2026-10-22T23:06:44Z)
- (audit operator) `fm-reviewer on the lane (five lenses), all findings fixed test-first by the builder and re-run here on main (cargo test --test cli: 6 passed; lib cli: 3 passed)` → setup-git rolls back on failure, reports per-server failures, hints after logging in to a new server [tree a936b328b71e] (2026-10-22T23:06:44Z)
- (audit maintainer) `fm-reviewer on the lane (five lenses), all findings fixed test-first by the builder and re-run here on main (cargo test --test cli: 6 passed; lib cli: 3 passed)` → COMMANDS table checked against USAGE by a unit test [tree a936b328b71e] (2026-10-22T23:06:44Z)
## Log
- 2026-10-22T02:08:37Z created
- 2026-10-22T22:09:59Z planned from capture
- 2026-10-22T22:10:29Z approved: the user asked to implement every queued feature ("what about all of the other features you have queued?"); proceeds under that standing request
- 2026-10-22T22:10:30Z set approved=True
- 2026-10-22T22:10:35Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0064.builder.md
- 2026-10-22T22:10:53Z focused
- 2026-10-22T23:04:55Z merged worktree-agent-af33ad732a83d815a (4 file(s))
- 2026-10-22T23:05:01Z lane removed: /home/chase/projects/gitter/.claude/worktrees/agent-af33ad732a83d815a
- 2026-10-22T23:05:04Z focused
- 2026-10-22T23:06:45Z done
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research T-0013-review: Verdict:** approve with nits. I found no token leak and no change to server-side runner behaviour. The findings below are two MEDIUM and several …
- research T-0024-recon: 1. **Answer [stale: src/api/account.rs, src/api/issues.rs, src/auth.rs changed since it was written]
- research T-0009-review: Verdict: changes needed.** I found no way to sign in without the second factor through login, signup or git basic auth. Session revoke is scoped …
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
## Lessons
- A credential helper registered per server must carry the server in its own command line: git's default request has no path, so matching on what git sends can't tell two logins on one host apart
## Files touched
- tests/cli.rs
- src/cli.rs
- src/main.rs
- README.md
