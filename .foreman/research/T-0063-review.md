**Verdict: changes needed.** Nothing exploitable turned up in the adversary lens, but the 60 s timeout memo is partly dead and migration v32 collides with T-0018 in main. I could only read files. I had no shell, so the diff came from reading the worktree and no tests, `gpg` or `ssh-keygen` were run.

## Findings

## lens: correctness
- MEDIUM — src/signing.rs:658,678 — `at: now` takes `now` from the start of `try_verify`, but a request can run about 130 s (100 checks, 8 at a time, 10 s each). Reproducer: a hung `gpg` with more than 48 uncached signed commits on one page. Every result after about 60 s is stored with `at` already older than `TIMEOUT_MEMO`, so `fresh()` rejects it straight away and those commits are re-run on the next view. — Set `at: crate::db::now()` when calling `memo.put` (leave `now` for the `fresh()` lookups). — high
- MEDIUM — src/db.rs:1017 — this branch adds the signing-key migration as v32. The uncommitted `src/db.rs` in main (T-0018, deployments) already has its own v32 at the same array index. A dev DB already at `user_version` 32 would skip this migration, and `proved_at` queries would then fail with "no such column". — Renumber this one to v33 when integrating, and take T-0018's v32 first. — high
- LOW — src/api/account.rs:746 — the `DELETE FROM signing_challenges` doesn't check how many rows it removed, so "one use" isn't atomic. Two parallel adds of different keys, both signed over the same challenge, both succeed. This needs both private keys, so it isn't an escalation. — Require exactly 1 row deleted, otherwise return the "no challenge" error. — high
- LOW — src/api/account.rs:676 — the settings page POSTs the challenge endpoint on every render. In the last 10 minutes of a challenge's life that replaces it, so a signature being made in another tab or on the CLI is silently invalidated. — Replace it only once expired, or also accept the previous challenge for a few minutes. — medium

## lens: adversary
- LOW — src/signing.rs:392-413 — `only_signature_packets` stops at the first `-----END` and ignores everything after it. gpg can read concatenated armor blocks, so a trailing inline-signed block would go unchecked, and `prove` and `check` share this function. The only remaining guard is the `PLAINTEXT` check in `parse_gpg`. I did not reproduce this. — Reject any non-whitespace text after the END line. — low

## lens: edge cases
- LOW — src/signing.rs:241 — a client clock ahead of the server gives a future-dated GPG signature. I expect gpg to refuse it, which would show the generic "isn't this key's signature" 400. I didn't test this. — Pass `--ignore-time-conflict` for the proof check only; the one-use challenge already limits replay. — low

## lens: test coverage
- MEDIUM — tests/api.rs — nothing tests the "proved key takes over" retain logic (src/signing.rs:760-763). Nothing tests challenge expiry or the 10-minute renewal either. Nothing checks that read-scope or agent tokens are refused on `/me/signing-keys/challenge`. — Add a test with one proved and one unproved owner of the same fingerprint, a test that moves `expires_at` into the past, and a token test. — high

## lens: performance
- LOW — src/signing.rs:323-330 — each uncached Good GPG check now runs a second `gpg --list-keys`. The code comment accepts this and results are memoised. — Make no change unless it shows up in measurements. — medium

## Probes that came back clean
- **Replay across accounts or sites:** the challenge is per user, includes the username, site URL and 128 random bits, and is deleted on success.
- **Another key's signature:** the signature's fingerprint must equal the pasted key's, which blocks other keys and the `git` SSH namespace. For GPG a subkey signature is accepted, and its primary fingerprint (VALIDSIG field 10) must match.
- **Inline or clearsigned messages:** these are rejected by the armor-header check, `only_signature_packets` and the `PLAINTEXT` check.
- **Keyring isolation:** a scratch GNUPGHOME holds only the one key and is removed by `TempDir`'s `Drop`.
- **Argument injection:** arguments are fixed and the sig file name is random hex.
- **Tokens:** adding a key needs `Sudo`, which refuses tokens. The challenge endpoint is blocked for limited and agent tokens by `token_may`: a POST outside `/repos` is refused, and `agent_may` allows only `/repos`.
- **Takeover griefing:** claiming or griefing a victim's key needs that key's private half. The one exception is a deliberately shared key, where a proved holder displaces a legacy unproved entry.

## Acceptance criteria
- **1. Cached Good GPG result expires with the key:** met by the code (`fresh`, `key_expiry`, tests at src/signing.rs:932 and tests/api.rs:1948), but not evidenced. Every recorded `cargo test -q signing` run says "0 tests, 2 filtered out".
- **2. Timeout remembered for a minute:** partly met. It works for small batches and is defeated for later ones by the `at` bug above.
- **3. Signing key needs a signed challenge, and the key page and badge say so:** met (account.rs, signing.rs `prove`, settings.js:149, lib.js:300), with the race and coverage gaps above.

## Implicit decisions
- Legacy unproved keys still produce a "verified" badge, with `proved: false` in the API and "never proved" in the UI.
- With `ssh-keygen` or `gpg` missing, no new key of that kind can be added (it fails closed).
- The challenge is one per account, reused for up to 50 minutes of its 1 h life.
- The SSH proof namespace is the shared constant `gitter`.
- A key's expiry is changed only by deleting and re-adding it.

## Not checked
README text, the settings.js layout, the T-0018 merge beyond the v32 collision, and gpg's behaviour on concatenated armor blocks and future-dated signatures.

**Noticed:** the first `sync()` deletes `sig-*` and `parse-*` entries from the gpg home, which can remove files a concurrent request is using, so a first request can fail spuriously (src/signing.rs:269-276).
