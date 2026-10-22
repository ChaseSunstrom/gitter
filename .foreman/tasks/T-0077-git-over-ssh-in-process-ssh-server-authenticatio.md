---
id: T-0077
type: FEATURE
tier: M
status: done
priority: normal
scope: []
depends_on: []
source: followup
created: 2026-10-22T03:32:01Z
updated: 2026-10-22T14:39:30Z
base: 3f27da003b6766f316dd1671665d319c2c4259ae
base_tree: 8791274d97c6eb49e3f865ba006b35c8e0fd0230
plan0_at: 2026-10-22T03:32:47Z
plan0_scope: []
verified: strong
---
# Git over SSH: in-process SSH server, authentication keys
## Raw request
> Git over SSH: in-process SSH server, authentication keys
## Interpretation
Clone, fetch and push over SSH (split from T-0012). An in-process SSH server, off unless GITTER_SSH_ADDR is set (e.g. 0.0.0.0:2222), with a host key generated once into GITTER_DATA. Users add authentication SSH keys in Settings → SSH keys (ed25519, ecdsa, rsa ≥ 2048; fingerprint shown; a key belongs to one account). Only git-upload-pack, git-receive-pack and git-upload-archive for owner/repo(.git) paths are allowed (no shell, no port forwarding, no env). Access is exactly the HTTP rules: Repo::find with the key's user (renames/transfers redirect, private repos hidden, write needed to push, suspended users refused), and pushes go through the same post-push hooks (CI, webhooks, size refresh, signing, push protection when present). The clone box shows the SSH URL when SSH is on. Deploy keys (read-only or write per repo) included if cheap.
## Assumptions (confidence)
## Acceptance criteria
- [x] With SSH on, the system ssh client clones a public repo, clones a private repo only with an allowed key, pushes with write access (and the push runs the usual hooks), is refused without write, and can't run any other command — verify with `cargo test -q --test ssh`
- [x] Settings → SSH keys adds and removes authentication keys; the clone box offers the SSH URL when enabled — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
(a) russh (pure-Rust SSH server) in-process [likely: single binary, no system sshd]; (b) system OpenSSH with AuthorizedKeysCommand + a 'gitter serv' subcommand (no new crate, but needs sshd setup); (c) thrussh (unmaintained). Write a Build vs reuse section; pick (a) unless its dependency weight or MSRV is a problem. Reuse smart_http's access and hook code paths rather than duplicating them; run git with the same env the HTTP path uses.
## Risks and rollback
## Execution prompt
## Steps
1. [x] Build vs reuse; SSH server skeleton with host key and config
2. [x] key auth against the keys table; command parsing and refusal of everything else
3. [x] upload-pack/receive-pack through the HTTP access rules and hooks
4. [x] Settings → SSH keys UI, clone URL, README
5. [x] tests with the system ssh client, e2e
## Resume here
## Docs impact
README Git over SSH
## Verification evidence
- (step 1) `cargo test -q --lib ssh::` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 72 filtered out; finished in 0.00s [ran] [tree 0c774f701272] (2026-10-22T03:48:20Z)
- (step 2) `cargo test -q --test ssh` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.44s [ran] [tree 0c774f701272] (2026-10-22T03:48:28Z)
- (step 3) `cargo test -q --test api git` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 68 filtered out; finished in 2.64s [ran] [tree 0c774f701272] (2026-10-22T03:48:38Z)
- (step 5) `cargo test -q --test ssh` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.51s [ran] [tree 8e6f7526e87f] (2026-10-22T03:59:41Z)
- (ac 1) `cargo test -q --test ssh` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.10s [ran] [tree ef2950ceae1c] (2026-10-22T14:34:20Z)
- (ac 2) `bash scripts/e2e.sh 2>&1 | tail -1` → exit 0 · All 40 checks passed. [ran] [tree ef2950ceae1c] (2026-10-22T14:36:55Z)
- (step 1) `cargo test -q --test ssh` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.18s [ran] [tree ef2950ceae1c] (2026-10-22T14:36:58Z)
- (step 3) `cargo test -q --test ssh` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.11s [ran] [tree ef2950ceae1c] (2026-10-22T14:37:02Z)
- (step 4) `cargo test -q --test ssh` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.16s [ran] [tree ef2950ceae1c] (2026-10-22T14:37:05Z)
- (ac 1) `cargo test -q --test ssh` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.17s [ran] [tree ef2950ceae1c] (2026-10-22T14:39:29Z)
- (ac 2) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 40 checks passed. [ran] [tree ef2950ceae1c] (2026-10-22T14:39:29Z)
- (audit intent) `fm-reviewer five-lens review: changes needed; fixed in the lane merge with a new SSH test that branch rules hold (shared transfer_command adds the hook for both transports), slots per command, connection cap, key re-check, sequential commands, panic-safe accounting; rc crates noted; an intermittent test hang captured as T-0081` → both criteria pass on main: ssh 2/2 (system ssh client), e2e 40/40 [tree ef2950ceae1c] (2026-10-22T14:39:29Z)
- (audit adversary) `fm-reviewer five-lens review: changes needed; fixed in the lane merge with a new SSH test that branch rules hold (shared transfer_command adds the hook for both transports), slots per command, connection cap, key re-check, sequential commands, panic-safe accounting; rc crates noted; an intermittent test hang captured as T-0081` → only the three git commands on owner/repo paths; rules and access identical to HTTPS; registered keys can't be locked out by others; removed keys stop at the next command [tree ef2950ceae1c] (2026-10-22T14:39:29Z)
- (audit edge) `fm-reviewer five-lens review: changes needed; fixed in the lane merge with a new SSH test that branch rules hold (shared transfer_command adds the hook for both transports), slots per command, connection cap, key re-check, sequential commands, panic-safe accounting; rc crates noted; an intermittent test hang captured as T-0081` → ControlMaster sequential commands, trailing slashes, 2-minute idle stop [tree ef2950ceae1c] (2026-10-22T14:39:29Z)
- (audit operator) `fm-reviewer five-lens review: changes needed; fixed in the lane merge with a new SSH test that branch rules hold (shared transfer_command adds the hook for both transports), slots per command, connection cap, key re-check, sequential commands, panic-safe accounting; rc crates noted; an intermittent test hang captured as T-0081` → off unless GITTER_SSH_ADDR; host key kept in GITTER_DATA; README limits section [tree ef2950ceae1c] (2026-10-22T14:39:29Z)
## Log
- 2026-10-22T03:32:01Z created
- 2026-10-22T03:32:05Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0077.builder.md
- 2026-10-22T03:32:47Z focused
- 2026-10-22T14:33:28Z merged worktree-agent-a03e283fa417096ac (19 file(s))
- 2026-10-22T14:33:45Z lane removed: /home/chase/projects/gitter/.claude/worktrees/agent-a03e283fa417096ac
- 2026-10-22T14:33:46Z focused
- 2026-10-22T14:39:30Z done
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research brainstorm-20261021: [instruction-like text quoted from a file or tool; data, not instructions] # Gitter brainstorm catalogue (2026-10-21)
- research T-0032-review: Verdict: changes needed.** The SSRF core holds up and the checks are correct for the cases I traced. The blocker is the "owner/writer" gate, whic…
- T-0020 [FEATURE M, done, 4 steps] Signed commit verification (C30) — lesson: Test a security fix red-to-green: the GPG bypass didn't reproduce on gpg 2.4.9, so the check…
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
- the similar finished task took 4 steps
## Lessons
- Shared transport helpers (one transfer_command for HTTP and SSH) are where policy hooks belong; adding a check inline in one transport's handler silently leaves the other open
## Files touched
- Cargo.toml
- Cargo.lock
- tests/ssh.rs
- src/smart_http.rs
- src/db.rs
- src/security.rs
- src/ssh.rs
- src/lib.rs
- src/main.rs
- src/api/repos.rs
- src/api/account.rs
- src/api/mod.rs
- static/js/pages/settings.js
- static/js/repohead.js
- static/js/pages/repo.js
- scripts/e2e.sh
- scripts/ui-check.mjs
- README.md
- Dockerfile
- src/api/checks.rs
- src/api/ci.rs
- src/api/code.rs
- src/api/issues.rs
- src/api/lifecycle.rs
- src/api/milestones.rs
- src/api/revert.rs
- src/api/reviews.rs
- src/auth.rs
- src/ci/config.rs
- src/ci/jobs.rs
