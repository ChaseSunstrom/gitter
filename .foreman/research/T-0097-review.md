**Verdict: changes needed.** Static read only: I ran no code. The "secret-looking value" pre-audit hit is a false positive. `compat.rs:1108` asserts on the text `${API_TOKEN}`, not a secret.

## lens: adversary
- HIGH — src/ci/compat.rs:854 (`extend`) — Cyclic `extends` with fan-out hangs the server. Only `depth > 10` stops it, with no visited set or memo, and `lines(j.get("extends"))` has no length cap. A ~500-byte `.gitlab-ci.yml` with `.a: {extends: [.b,.c,.d,.e,.f,.g,.h,.i]}` and the other hidden jobs extending each other costs about 7^10 calls per job. It runs synchronously inside `pipeline_at` on every push, PR open and schedule tick, so any writer can pin an async worker. Fix: track the extends chain and error on a repeat, and memoize resolved bases. Confidence high.
- HIGH — src/ci/compat.rs:206-221 (`gh_cond`) — Unbounded recursion on file text overflows the stack. `if: '!!!!…x'`, or the same with nested `((((…))))`, fits in a 256 KB workflow and recurses once per character. Rust aborts the whole process on stack overflow, and `balanced`/`split_top` add O(n²) work on the way. Fix: cap the `if:` length (about 1 KB) or the recursion depth. Confidence high.
- MEDIUM — src/ci/compat.rs:240-241 — `startsWith/endsWith/contains` literals reach `case … in {glob})` with `) ; | newline` unfiltered. `if: startsWith(github.ref, 'x) true ;; esac; id; case y in z')` emits a working `id`. Fix: allow only `[A-Za-z0-9._/@:-]` in the pattern, else return None.
- MEDIUM — src/ci/compat.rs:156, 299, 609 — `matrix.<anything>` is emitted raw. `if: matrix.v"; id; echo " == 'x'` breaks out of the `[ "…" ]`. `env: A: ${{ matrix.x" ; id ; " }}` does the same in the `export A="…"`. Unknown keys stay literal because `interpolate` only replaces exact known keys. The same line-609 path also puts a `quoted()` literal into `"…"` without `dq_escape`. Fix: validate the key as `[A-Za-z0-9_-]+` and `dq_escape` literals.
- MEDIUM — src/ci/compat.rs:75-85 — Notes heredoc breakout. Note text embeds file text verbatim: `uses:`, `if:`, expressions, env names, `rules: if`. `uses: "a\nGITTER_NOTES_EOF\nid\n:#"` ends the heredoc and runs `id` in the first step, which has the job env and secrets. Fix: strip newlines from notes, or use a random delimiter.
- MEDIUM — src/ci/compat.rs:437-451, 578-585 — Jobs fail open onto the built-in runner. `runs-on` labels, including `self-hosted`, and GitLab `tags:` are dropped, so a `[self-hosted, prod]` deploy job runs on the Gitter host. An unusable `image:`/`container:` (variables) is dropped with only a log note, so the job runs as a host process instead of in a container. Fix: map non-ubuntu labels to `runs-on`, and fail the job when the image is unusable.
- MEDIUM — src/ci/compat.rs:272 (`split_top`) — `s[i..]` is evaluated on non-char-boundary bytes. Any non-ASCII character outside single quotes in an `if:` panics. Fix: iterate `char_indices`.
- Anchors: no finding. The budget is checked on every alias, cycles error as "no &x before it", and depth is bounded. A raw `\x01` in a plain scalar can only form an alias to an existing anchor, so it grants nothing.

## lens: edge
- HIGH — src/ci/compat.rs:328-330, 312 — No runnable workflow, or any one bad file, fails the whole pipeline. A repo whose workflows are only tag, release or `workflow_call` returns `Err`, and `trigger.rs:209` starts a failed run on every push and PR with red marks. Likewise one unparseable workflow (see the next item) breaks every other workflow. Fix: return `Ok(None)` when nothing is runnable, and skip or note a bad file.
- MEDIUM — src/ci/config.rs:45 — The pre-existing line check rejects any line starting with `&`, `*` or `!`, even inside `run: |` blocks. `parse_yaml("a: |\n  x\n  && y\n")` errors, and so does `! grep`. Foreign files hit this constantly, and the message ("goes after 'key:'") misleads. Fix: apply the check only outside block text.
- MEDIUM — src/ci/compat.rs:39 (`RUNNER_TEMP`), runner.rs:650 — In image/container jobs, HOME is `/tmp` and each step is a fresh `docker run --rm`. `$GITHUB_ENV` and `$GITHUB_PATH` never persist between steps.
- MEDIUM — runner.rs:622 and `if: always()/failure()` — A failed step skips every later step, so `always()` cleanup, `failure()` notify and GitLab `after_script` never run, even though README promises them. Artifacts are packed only on success too.
- MEDIUM — src/ci/compat.rs:744 plus trigger.rs:104 — GitLab default `needs` is the previous stage only. If `pick` removes that whole stage (MR-only or manual jobs on a push), `retain` leaves `needs=[]`. With `build`, an MR-only `test`, then `deploy`, deploy runs in parallel with build and even after it failed. Fix: replace a dropped need with that job's own needs.
- LOW — trigger.rs:53 — Each push now costs an extra `ls-tree` and `stat` spawn (4 git spawns, was 2) for repos with no CI file.

## lens: intent
- HIGH — src/ci/compat.rs:966-1010 (`gl_when`) — GitLab rules fail open, against the README's promise that an undecidable condition skips. `if: $CI_COMMIT_BRANCH == "main" && $CI_PIPELINE_SOURCE == "push"` is matched on `source("push")` and gets an unfiltered push trigger, so a deploy job runs on every branch. `=~`, `$CI_COMMIT_REF_PROTECTED` and other unknown conditions are "taken as true". `when: never` rules are dropped, and `except` is ignored. Fix: treat unknown conditions as not running, honor `never`, and require the whole condition to be understood.
- LOW — compat.rs:615 — an unterminated `${{` in env duplicates the literal prefix.
- LOW — compat.rs — `parallel: N`, `workflow:` and `pull_request` `types:` are silently ignored with no note.
- LOW — compat.rs:97 — a `script: |` block has its lines trimmed and its blank lines dropped.

## lens: trust
No finding. The actor is still set from `Start`, so a PR file cannot change who a run is on behalf of. Secrets and the built-in-runner gate are unchanged (`jobs.rs` claim). `pull_request_target` maps to `pull_request` with no escalation, because there are no cross-repo PRs. The only gap is the fail-open item under adversary.

## lens: maintainer
- MEDIUM — src/ci/trigger.rs:91 — `pick`, which holds the needs and schedule logic, has no unit test. The job-condition and step-condition blocks duplicate each other at compat.rs:500 and 530. `#[allow(clippy::too_many_arguments)]` is on `step`.

## Acceptance criteria
- GitHub and GitLab translation (`cargo test -q --lib ci::compat`): not verifiable, since I ran nothing. The listed paths are covered by tests, but the hazards above are not.
- Only-workflows run on push, PR filtering, and GitLab stage order (`cargo test -q --test ci compat`): not verifiable. The "stage order" case has the dropped-stage hole.

## Implicit decisions
- Workflow files are read only when there is no `.gitter/ci.yml` or `.gitter/ci.sh`, with no off switch.
- At most 20 workflow files and 100 jobs.
- `schedule` runs every job of every workflow that has a cron.
- Cache and artifact paths are limited to the workspace.
- Unsupported GitHub bits are skipped with a note, while GitLab `rules` are taken as true.

## Not checked
Test execution, `flow()` internals, DB limits on step size, the UI.

**Noticed:** none beyond the above. Step-count amplification (about 1.3 KB of prelude per step) has no cap and could be its own task.
