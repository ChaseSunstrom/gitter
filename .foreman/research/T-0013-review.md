**Verdict:** approve with nits. I found no token leak and no change to server-side runner behaviour. The findings below are two MEDIUM and several LOW. I read the code and did not run it.

## lens: adversary
- MEDIUM — src/cli.rs:1005-1006 — The `ci run` scratch dir is created with `create_dir_all`, which uses the default mode (0755 under umask 022) and accepts an existing path or symlink. In a shared `/tmp`, other local users can read the copy of a private work tree while a job runs. A pre-planted symlink at the guessed `gitter-ci-<pid>-<rand>` name would also be followed. — Create it with `DirBuilder::new().mode(0o700).create()` (non-recursive, so it fails if the path exists). — high
- LOW — src/cli.rs:482-503, 924 — With `colors=true`, SGR codes and `\r` pass through from server logs. A log can emit `\x1b[8m` (conceal) and leave the terminal hiding output, or use `\r` to overwrite a "FAILED" line. `clean(false)` also keeps bidi overrides (U+202E, U+2066-9), so titles can be spoofed. — Emit `\x1b[0m` after each log and drop bidi/zero-width chars. — medium
- LOW — src/cli.rs:964-970 — `create_dir_all` and `fs::copy` follow a destination symlink created earlier in the same loop. If the file list ever held both `x` (symlink) and `x/f`, the copy would write outside `to`. Git indexes cannot normally contain that pair, so this is unreachable through a clone. — Reject the file when `dest.parent()` is a symlink. — low
- LOW — src/cli.rs:301 — `rename` replaces a symlinked `config.json` (dotfiles setups) with a regular file. `load` never warns about a pre-existing world-readable config. — Resolve the symlink before saving, and warn when `mode & 077` is set. — medium
- LOW — src/cli.rs:587, 593, README — `--token` is visible in argv and shell history. The prompt echoes the token. `http://` logins send the token in plaintext. — Add `--with-token` on stdin, turn echo off, and warn on non-loopback `http`. — high

Verified safe:
- **Token binding:** the token only goes to the config host key. `match_url` rejects `git.example.com.evil.net` and `git.example.com@evil`.
- **Config file:** it is created with `create_new` at 0600 in a 0700 dir, then renamed, so there is no umask race.
- **Escape stripping:** `clean` strips ESC and C1 characters.
- **Job environment:** `env_clear` plus a fixed list keeps `GITTER_TOKEN` and `SSH_AUTH_SOCK` out of jobs.
- **Image names:** the parser validates them.

## lens: edge
- MEDIUM — src/cli.rs:561, 276 — `load()` runs before dispatch, so `gitter ci run` fails with "HOME isn't set" or on a corrupt `config.json`, though it needs no server. — Dispatch `("ci","run")` before `load()`. — high
- LOW — src/cli.rs:950-972 — Ctrl-C during `copy_tree` is swallowed, because tokio's handler replaces the default SIGINT behaviour and the copy never checks `stopped()`. A big tree in tmpfs cannot be interrupted. — Check `stopped()` per file. — medium
- LOW — src/cli.rs:963-971 — Submodules and nested repos are silently skipped. — Print a note listing skipped paths. — high
- LOW — src/cli.rs:919-924 — Each poll decodes with `from_utf8_lossy` and `clean`. The server cuts at 4 MB (src/api/ci.rs:210) and flushes lines over 64 KB, so U+FFFD appears and SGR codes split. — Carry incomplete trailing bytes into the next poll. — medium

## lens: operator
- LOW — src/cli.rs:768-783, 938-944 — `issue close`, `issue comment` and `ci rerun` ignore `--json`, against the spec. — Call `print_json(&v)` when `--json` is set. — high
- LOW — README / src/cli.rs:361 — The token is never stored for plain git, so `git pull` or `git push` in a CLI-cloned private repo prompts or fails. — Add a credential-helper subcommand. — medium
- LOW — src/ci/runner.rs:575-587 — `--here` with an `image` job mounts the real repo read-write, `.git` included. It then runs `chown -R` over the whole tree to the directory owner's uid. — Refuse or warn, and document that non-image steps run directly on the host. — medium

## lens: intent / maintainer
- LOW — src/cli.rs:789-809 — `pr create -R other/repo` inside an unrelated clone pushes the cwd's branch to `other/repo`. — Require `--head` when `-R` doesn't match the cwd remote. — medium
- Nit — Usage text is duplicated in `src/main.rs` and `USAGE`. `login` prints "Logged in" before `save`. `tests/cli.rs` has no test for lookalike hosts, overwriting an existing 0644 config, or symlinks.

## Acceptance criteria
- Login, repo, issue, PR commands and `pr checkout`: **met**. `tests/cli.rs` covers 0600 on creation and checkout of a PR head. Recorded evidence shows `cargo test -q --test cli` passing.
- `ci run` with needs, matrix, a failing step and exit code: **met**, per the `ci_run` test and its recorded evidence.

## Implicit decisions
- JSON config keyed by the login URL.
- A tmp+rename config write.
- The token reaches git as a Basic extraHeader in the environment.
- ureq `SameHost` redirect auth.
- Logs are polled once a second, not streamed over SSE.
- `/tmp` is the scratch location.
- Each job gets a fresh copy.
- Exit code 130 on interrupt.

## Not checked
- I did not run the tests or the build.
- Curl and git cross-host redirect header handling.
- Windows (the code is unix-only).
- `Backend::source` internals beyond the call site.

**Noticed:** none
