---
id: T-0020
type: FEATURE
tier: M
status: captured
priority: normal
scope: []
depends_on: []
source: followup
created: 2026-10-21T17:01:53Z
updated: 2026-10-21T23:41:19Z
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
## Follow-ups captured
