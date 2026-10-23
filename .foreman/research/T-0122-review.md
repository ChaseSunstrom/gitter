**Verdict:** approve with nits. I found no HIGH or CRITICAL defect. I had no shell, so this is a code reading and I did not run the tests.

## lens: adversary
- MEDIUM-low (LOW) — src/cli.rs:709 — The old no-server matcher `v.starts_with("!'") && v.ends_with("' auth git-credential")` accepts any quoted executable, not just gitter. Reproducer: `git config --global credential.https://git.example.com.helper "!'/home/J Smith/bin/gh' auth git-credential"`, then `gitter logout https://git.example.com`. That is gh's quoted form for a path with a space, as I recall gh quoting it. The helper is removed and the output says "git no longer uses a gitter login". The normal case, `!/usr/bin/gh …` unquoted, is safe and tested. Fix: for the old form also require the executable's file name to be `gitter`, for example `v.ends_with("/gitter' auth git-credential")`. Confidence: medium.
- Option injection: no finding. The key always starts with `credential.`. Values go after the key, where `git config` stops option parsing, and only the user's own helpers could start with `-`. I could not run git to confirm that behaviour.

## lens: edge
- No finding on several servers, servers with quotes, or servers with spaces. `quote()` is applied the same way in setup-git and in the `ours` matcher, and git escapes subsection text.
- Default server: when the removed server is the default, it moves to the first remaining key alphabetically, not the most recent login. `Ctx::host` already falls back to the first key, so this is harmless.
- If the helper cleanup fails, the token is already gone but a rerun still removes the helper (the `!had` path). That is sound.

## lens: correctness
- LOW — src/cli.rs:541 — The `login` hint uses `contains("auth git-credential")`. This line is unchanged by the diff. It matches gh's helper, so anyone who ran `gh auth setup-git` is told "run `gitter auth setup-git` again" after logging in, though they never used gitter's helper. Reproducer: have `credential.https://github.com.helper=!gh auth git-credential`, then `gitter login <any server>`. Fix: put the matcher from line 709 in one function and use it in both places.

## lens: silent-failures
- LOW — src/cli.rs:550-556, 708 — `global_config` turns any git failure, such as an unparsable `~/.gitconfig`, into an empty list. Logout then prints "Logged out of X" without saying it never checked the helpers. The leftover helper answers nothing once the token is gone, so the harm is small. Fix: warn on stderr when `git config --get-all` exits with a code other than 1.
- LOW — src/cli.rs:554 — `from_utf8_lossy` means a user's non-UTF-8 helper value comes back with U+FFFD when `set_all` rewrites the key. This is very unlikely to happen.

## lens: test-coverage
- LOW — tests/cli.rs:398-444 — These cases are not tested, and the first two would pass unnoticed if the matcher broke:
  - the old no-server form (`ends_with("' auth git-credential")`);
  - a user helper before gitter's, such as `["mine","","ours"]`;
  - a server URL containing a quote;
  - the `!had` case where only the helper remains;
  - the `set_all` rollback.
  - The gh entry on line 421 is unquoted, so removing `starts_with("!'")` would go unnoticed. Add a quoted-gh case.
- LOW — tests/cli.rs:464 — `help bogus` passes only because the server's `HELP` in main.rs also lists `gitter logout`, not because the client's `USAGE` was printed.

## Acceptance criteria
- Logout removes the token and gitter's helper: met (the test at tests/cli.rs:398, plus code at cli.rs:689-718).
- Logout needs no argument with one login: met (tests/cli.rs:437).
- `<command> --help` and `help <command>` print only that command's usage and exit 0: met (cli.rs:464-472, tests/cli.rs:447).
- Both `cargo test` commands passing: not verified by me. The brief records them as passing.

## Implicit decisions
- With two or more logins, `logout` requires an explicit SERVER even when a default exists.
- The default moves to the alphabetically first remaining host.
- The token is removed before the git cleanup.
- Only `--global` git config is touched.
- Removing a helper rewrites the whole key.
- `-h` is a global alias for `--help`.

## Not checked
I did not run the tests or git. I did not check git's handling of a `-`-leading value. I did not check how gh quotes its helper path, or the README text beyond confirming it mentions logout.

**Per-lens verdicts**
- adversary: approve with nits (one LOW).
- edge: approve.
- correctness: approve with nits (one pre-existing LOW).
- silent-failures: approve with nits.
- test-coverage: approve with nits.

**Noticed:** `gitter runner --help` fails with "unknown or incomplete option" (src/main.rs:119-123) instead of printing usage.
