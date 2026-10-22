## Verdict
Changes needed. One likely HIGH in the GPG path, and the Verified guarantee is weak while emails are unconfirmed and keys need no proof of ownership. I could not run anything, so every finding comes from reading the code.

## Findings

## lens: adversary
- HIGH — src/signing.rs:594 — The GPG check runs `gpg --verify <sigfile> -` and never checks that the file is a detached signature. gpg ignores the stdin payload for a non-detached signed message (it only warns "not a detached signature; file was NOT verified") and still prints GOODSIG/VALIDSIG for the embedded text. A pusher takes any armored `gpg --sign` message by the victim, re-labels the armor as `PGP SIGNATURE` and puts it in `gpgsig`. They then set the committer email to the victim's. The result is Verified as the victim, whose key is registered. The 4-line `starts_with` test at :562 does not stop this. Fix: check the first dearmored packet is tag 2 (signature) and reject any other packet type. As a backstop, treat stderr containing "not a detached signature" as Bad. Also require gpg 2.2.8 or newer (status-line injection fix). Confidence: medium. I read the code path but did not run gpg.
- MEDIUM — src/api/account.rs:147, src/signing.rs:716 — Keys have no proof of possession and `fingerprint` is globally unique. Mallory can register Bob's public key (GitHub publishes SSH keys). Bob then cannot register it, and his real commits show "Signed with mallory's key, but the committer email isn't mallory's". Fix: require a signature over a server challenge when adding a key. Short of that, require a GPG user-id email that matches the account email.
- MEDIUM — src/api/account.rs:322-377, static/js/lib.js:1036 — Email is neither confirmed nor re-authenticated (the README admits this). A pusher sets their account email to an address nobody has claimed, registers their own key, and commits as `Bob <bob@corp>`. The list shows a green "Verified" with no signer; the signer appears only in the popover. Fix: show the signer in the badge (`Verified · mallory`), or confirm emails before they count.
- MEDIUM — src/signing.rs:675-684, 716 — `parse_key` imports into the shared keyring before the duplicate check, the 50-key limit and the "one key at a time" check. Rejected submissions still grow the keyring, with no cap. A rejected re-upload of a registered key (for example a revocation) changes the keyring without calling `key_added()`, so cached Good results stay stale. Fix: parse into a throwaway homedir, and import into the real one only after the DB insert.

## lens: edge
- MEDIUM — src/signing.rs:596,655-669 — The gpg exit status and stderr are never read. A locked keyring or an unwritable homedir produces no status lines, which `parse_gpg` maps to `Bad`. That is memoised as "commit may have been changed after signing" until restart. Fix: if there is no SIG-related status line and the exit is non-zero, return `Fail::Broken` (not cached).
- LOW — src/signing.rs:766 — A cached Good for a GPG key never re-checks when the key expires later.

## lens: operator
- MEDIUM — src/signing.rs:896, 516 — `git.run` has no output cap, and payloads are copied again with `to_vec`. A pusher's multi-MB commit messages are read in full on every cache-miss page view (blame can ask for thousands of shas). Fix: use `--batch-check` first and skip commits over about 1 MB as `unknown`.
- LOW — src/signing.rs:803 — Timeouts are not memoised, so a pathological signature costs 10 s per view (8 at a time) and holds git permits. Fix: negative-cache for 60 s.
- LOW — src/signing.rs:594 — Add `--no-auto-key-retrieve --no-auto-key-locate --keyserver none`; this relies on gpg's defaults today. Stale `sig-*` files left by a crash are never cleaned up.

## lens: intent / maintainer
No further findings. The UI escapes `data-sig` and the reason text. Private repos are guarded because `annotate` runs after `Repo::readable`.

## Acceptance criteria
- AC1, SSH and GPG tests: met per the recorded run (3 passed). The GPG test self-skips when gpg is absent, so whether it ran is not verifiable. No test covers the namespace, multiple gpgsig headers, or a non-detached signature.
- AC2, `bash scripts/e2e.sh`: not verifiable. The task shows no e2e or ui-check run.

## Implicit decisions
- Verified means "an account's key plus a matching unconfirmed email".
- The committer is checked, not the author.
- The no-reply address counts as an email match.
- Check results live only in memory and are lost on restart.
- Suspended users' keys still verify.

## Not checked
Real gpg/ssh-keygen behaviour (no execution), email validation (`validate::email`), rate limits, CSS, and the Dockerfile build.

**Noticed:** add negative tests for the SSH namespace, a non-detached GPG signature, and multiple `gpgsig` headers.
