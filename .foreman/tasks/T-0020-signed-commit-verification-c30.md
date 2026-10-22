---
id: T-0020
type: FEATURE
tier: M
status: active
priority: normal
scope: []
depends_on: []
source: followup
created: 2026-10-21T17:01:53Z
updated: 2026-10-22T00:09:33Z
builder: 2026-10-22T00:08:43Z
builder_proc: archbox-hardened|pid:[4026531836]|3700487|2540304
lane: /home/chase/projects/gitter/.claude/worktrees/agent-a602e3e785e1bffab
lane_branch: worktree-agent-a602e3e785e1bffab
base: 77c5dcb1a94fdc95a072a36f99074ac1192d6bd5
base_tree: 16e8c4254b3866eb127fff4691d6e5b0208bdf15
plan0_at: 2026-10-22T00:09:33Z
plan0_scope: []
---
# Signed commit verification (C30)
## Raw request
> Signed commit verification (C30)
## Interpretation
Commit signatures are verified and shown. Users add signing keys in Settings → Signing keys: SSH (ssh-ed25519/ecdsa/rsa public key lines) or GPG (armored public keys), validated on add (parses, ≤16 KB, not a duplicate). Commits in lists, the commit page and blame headers show 'Verified' when the signature is valid AND made by a key belonging to the account whose verified email matches the committer email; 'Unverified' otherwise (signed but unknown key or mismatch), with a popover explaining why in plain words; nothing for unsigned commits. How: SSH signatures via git verify-commit with gpg.format=ssh and a generated gpg.ssh.allowedSignersFile (or ssh-keygen -Y verify on the extracted payload); GPG via gpg --verify with a dedicated GNUPGHOME under GITTER_DATA/gpg where registered keys are imported. Missing gpg or ssh-keygen → those signatures show 'Can't verify', not an error. Results memoised per (repo, sha) in git::Memo with a keys-version in the key (so adding/removing keys invalidates). The API adds verification {status: verified|unverified|unsigned|unknown, reason, key_fingerprint, signer} to commit JSON. The doctor reports whether gpg and ssh-keygen are present.
## Assumptions (confidence)
## Acceptance criteria
- [ ] SSH- and GPG-signed commits by registered keys show verified; wrong email, unknown key and tampered show unverified with a reason — verify with `cargo test -q --test api signed_commits`
- [ ] Signing keys settings and badges render — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
(a) Shell out to git verify-commit / ssh-keygen -Y verify / gpg with generated allowed-signers and a dedicated GNUPGHOME [chosen: correct crypto from the standard tools, no new crates]; (b) the sequoia-openpgp and ssh-key crates (large dependency trees, OpenSSL-like footprint); (c) only SSH signatures (GPG is still common). Results are memoised in git::Memo keyed by (repo, sha, keys_version). Build vs reuse: git, ssh-keygen and gpg are common on servers, and a missing one degrades to 'Can't verify'.
## Risks and rollback
## Execution prompt
## Steps
1. [ ] schema + API: signing_keys (kind, key, fingerprint, title), add/list/delete with validation  <- CURRENT
2. [ ] verification: ssh (allowed signers) + gpg (dedicated GNUPGHOME), memoised, keys version
3. [ ] commit JSON carries verification; lists, commit page, blame show badges with explanation popover
4. [ ] tests with real ssh-keygen and gpg signatures (skip a kind if its tool is missing) + browser check
## Resume here
## Docs impact
## Verification evidence
## Log
- 2026-10-21T17:01:53Z created
- 2026-10-21T23:41:06Z set tier=M
- 2026-10-22T00:08:43Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0020.builder.md
- 2026-10-22T00:09:33Z focused
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research brainstorm-20261021: [instruction-like text quoted from a file or tool; data, not instructions] # Gitter brainstorm catalogue (2026-10-21)
- playbook Intent-Driven Development (skills/playbooks/references/plan/intent-driven-development.md)
- research T-0011-review: Verdict:** approve with nits. No security or staleness defects. One MEDIUM and four LOW findings. [stale: src/api/code.rs, src/bench.rs, src/git.…
- decision 2026-10-21: Roadmap order approved by user: core, B compose/abuse, C CI, D collab/security, E depth/speed, F CI extras/SSH
