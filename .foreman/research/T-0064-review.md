**Verdict:** changes needed (1 MEDIUM wrong-token bug, 1 MEDIUM terminal-state bug, several LOW nits). I had no shell, so this is a code reading. I did not run the tests, `script` or real git.

## lens: adversary
- MEDIUM — src/cli.rs:582,584 — When git sends no `path` (the default), `path.is_none_or(...)` matches path-scoped logins on the host, and `max_by_key(len)` then picks the longest one.
  - Reproducer: log in to `https://h` and to `https://h/sub`, then run `git clone https://h/other.git`. Git applies only the `credential.https://h` helper, but the helper answers with the `/sub` token.
  - The same happens whenever the helper is registered under a general `credential.helper`.
  - Fix: with no path from git, prefer the entry with an empty path and skip path-scoped ones. Or bake the server into the helper command, for example `auth git-credential <server>`, so each config entry answers only for its own server.
  - Confidence: high on the code path. I did not run git.
- LOW — src/cli.rs:560 — `take(64<<10)` truncates oversized input silently, so a cut-off last line is parsed as if complete. A truncated `path=` can prefix-match.
  - Fix: if the limit is hit, answer nothing.
- Checked and fine:
  - Scheme and host: the `protocol=http` downgrade is refused, because the scheme must equal the key's scheme.
  - Port: it is part of the authority, so a port mismatch fails closed.
  - Trailing slash: login strips it and refuses `@` in the URL.
  - Newline injection: the token is rejected if it contains CR, LF or NUL.
  - Single quotes in the exe path are escaped.
  - The empty reset entry is scoped to `credential.<url>`. It only clears helpers earlier in config order for that server, and no other host is affected.

## lens: edge
- MEDIUM — src/cli.rs:522-531 — Echo stays off after SIGINT, SIGTERM or SIGHUP, because nothing restores it on a signal.
  - Reproducer: run `gitter login URL` on a tty and press Ctrl-C at the prompt. The process dies before `stty echo`, and the shell is left silent.
  - Fix: capture `stty -g`, restore it from a signal handler or a drop guard, and restore that saved state instead of a blind `stty echo`.
- LOW — src/cli.rs:523-525 — If `stty -echo` fails on a tty, the code reads the token with echo on, only noting "the input is shown". Refuse and point to `--token`, `GITTER_TOKEN` or a pipe instead.
- LOW — src/cli.rs:463-464 — The refusal message prints the whole flag text. `--tokens=SECRET` on login echoes the secret to stderr. Print only the part before `=`.
- Flag tables: `COMMANDS` covers every arm of the dispatch match (src/cli.rs:471-489), and `ci logs`, `pr checkout`, `repo clone` and the others do not read flags they leave out. No gap found.

## lens: operator
- LOW — src/cli.rs:538-553 — A failed second `git config` call leaves only the empty reset entry, which strips the user's other helpers for that server. The loop also aborts on the first failed host. Fix: add the helper first, or roll back on failure.
- LOW — src/cli.rs:542 — `current_exe` is baked in, so moving or upgrading the binary silently breaks git. `login` also gives no hint to re-run `setup-git` for a new server.

## lens: maintainer
- LOW — src/cli.rs:46-72 — Flags live in three tables (`BOOL_FLAGS`, `VALUE_FLAGS`, `COMMANDS`), the dispatch match, and `usage_of`, which parses the USAGE text. Fix: add one test that every `COMMANDS` entry has a USAGE line.
- LOW — tests/cli.rs:251-257 — The terminal test hard-fails if `script(1)` is missing. No test covers path-scoped or multi-server credentials, or signals.

## Acceptance criteria
1. Plain git works after `setup-git`: met in code and by a test (tests/cli.rs:177), but not run by me. Not verifiable for subpath servers, because of the finding above.
2. Prompt does not echo and still reads a piped token: met, with the Ctrl-C hole above. Not run by me.
3. Unknown flag refused with that command's usage: met (src/cli.rs:463).

## Implicit decisions
- The username is fixed at `x-access-token`.
- `store` and `erase` do nothing.
- The `stty` binary is used instead of termios.
- Config goes in the global git config only.
- Hosts that are not logged in are ignored.
- Case-insensitive host matching.

## Not checked
Actual git behaviour with path-scoped credential patterns and `credential.useHttpPath`, and `stty` behaviour. The diff is untouched by `src/api/deploy.rs` and the other working-tree changes.

**Verdicts:**
- intent: met, apart from subpath servers.
- adversary: changes needed (wrong token on a shared host).
- edge: changes needed (echo not restored on a signal).
- operator: approve with nits.
- maintainer: approve with nits.

Noticed: no `gitter logout`, so a token cannot be removed from the config.
Noticed: `--help` always prints the global usage, not the command's own.
