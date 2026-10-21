# Brainstorm — lens: bold bets

- **Add a merge queue with speculative batching** — FEATURE — value 5 — effort M — risk med — category: CI/merge
  Why: the pack covers merge gating on required checks, but a busy public repo still has semantic conflicts, because main moves between "green" and "merged". The queue tests PR N on top of PRs 1..N-1 and merges only on green.
  Done when: three PRs that each pass alone but break together, queued at once, result in the bad one ejected and the other two merged, with the queue state visible in the UI.

- **Auto-shard CI to a time budget** — FEATURE — value 4 — effort M — risk med — category: CI speed
  Why: the pack stresses "wayyy faster" and push-to-green under 3s, but only through warm starts. A job declaring `budget: 30s` could be split across runners using recorded per-test timings. Assumption: runners are plentiful enough to parallelise.
  Done when: a test job with recorded timings runs as N parallel shards, and its wall time falls within 20% of the budget on a multi-runner setup.

- **Skip CI jobs by input hash** — PERFORMANCE — value 5 — effort M — risk med — category: CI cache
  Why: the pack has caching and warm snapshots but nothing that skips work. Hashing a job's declared inputs (files, image, env) and reusing the previous result, as Bazel does, makes most pushes near-instant.
  Done when: re-pushing a commit that changes only docs marks the build job "cached (from <sha>)" in under 1s without starting a container.

- **Review across force-pushes with interdiffs** — FEATURE — value 4 — effort M — risk low — category: code review
  Why: the pack lists inline comments and suggestions but not what happens to them after a rebase. Without anchoring comments to the change rather than the SHA, reviewers lose them. Each PR stores its revision history and shows "changes since your last review".
  Done when: after a force-push of a PR with an inline comment, the comment stays attached to the moved line and a "since last review" diff shows only the new changes.

- **Support native stacked PRs** — FEATURE — value 4 — effort L — risk med — category: workflow
  Why: assumption: heavy users want Graphite-style stacks. The pack has plain PRs only. Dependent PRs would be shown as one ordered stack, each reviewed on its own diff, and merged bottom-up with automatic retargeting.
  Done when: a 3-PR stack merged at the bottom retargets the next PR to main and rebases it without manual action.

- **Run a pull-through package proxy for npm, crates and PyPI** — FEATURE — value 4 — effort L — risk med — category: CI speed
  Why: the pack mentions a package registry, but not caching upstream registries. Runners fetching dependencies from the instance itself are faster, work offline, and reduce supply-chain exposure through pinned, scannable artifacts.
  Done when: a CI job's `cargo fetch` and `npm ci` hit the instance proxy, and a second run succeeds with the upstream network blocked.

- **Detect and quarantine flaky tests** — FEATURE — value 4 — effort M — risk low — category: CI reliability
  Why: assumption: test results are parsed (JUnit or TAP). Tracking per-test pass/fail on identical commits spots flakes, auto-retries them, and flags them on the PR instead of failing it.
  Done when: a test that fails then passes on the same SHA is labelled "flaky" on the PR and is excluded from merge gating after a configurable threshold.

- **Add AST-aware diffs with moved-code detection** — FEATURE — value 3 — effort L — risk med — category: code review
  Why: the pack's diff work is about speed and virtualization, not comprehension. Tree-sitter diffs would collapse formatting churn and show moved functions as moves. Assumption: tree-sitter grammars can be vendored without bloating the binary.
  Done when: a commit that only reformats and moves a function shows as "moved, 0 semantic changes" for supported languages.

- **Bisect in parallel on the runner pool** — FEATURE — value 3 — effort M — risk low — category: CI debugging
  Why: since CI already exists, "which commit broke this?" can be a button on a failing run. It runs a k-ary bisect across runners instead of a serial `git bisect`.
  Done when: clicking "find culprit" on a failing job names the first bad commit in a 64-commit range using at most 3 rounds of parallel jobs.

- **Store issues and PRs as git refs** — FEATURE — value 4 — effort L — risk high — category: data model — wild
  Why: assumption: it's affordable to keep the SQLite row as the index and git refs as the source of truth. A clone then carries issues, review threads and CI results, so offline work, backups and federation come almost free (the pack lists federation as wild but not the data model that enables it).
  Done when: `git fetch origin 'refs/gitter/*'` into a fresh instance reproduces the issues and PRs of the source repo, verified by an e2e comparison.

- **Let volunteers donate laptops as runners** — FEATURE — value 3 — effort L — risk high — category: runners — wild
  Why: the pack's pull-based runner protocol already allows runners to sit anywhere. Opt-in desktop runners, limited to the owner's own repos or trusted teams, would give small teams free CI capacity and real macOS/ARM targets. Assumption: the sandbox story holds for untrusted code.
  Done when: running `gitter runner --pair <code>` on a laptop makes it appear in the owner's runner list and run a job labelled `os:macos` end to end.

- **Edit together in a multiplayer web editor** — FEATURE — value 3 — effort L — risk med — category: collaboration — wild
  Why: the pack has a web editor and an "edit then open PR" flow. Live cursors and CRDT sync on a branch would turn it into pair programming in the browser, with commits landing as co-authored.
  Done when: two browser sessions editing the same file see each other's changes within 200ms, and the resulting commit carries both `Co-authored-by` trailers.

- **Run README snippets in the sandbox** — FEATURE — value 2 — effort M — risk high — category: docs — wild
  Why: the sandbox, workspace pre-warming and previews already exist in the plan. Marking a code fence `run` adds a button that executes it in the CI sandbox and streams output inline, so docs get tested and demos stay live. Assumption: public abuse limits cover this.
  Done when: clicking Run on a fenced shell block in a README streams its output in the page, subject to the same quota and sandbox as CI jobs.

- **Tier cold repos to object storage** — PERFORMANCE — value 3 — effort L — risk med — category: scale
  Why: a public site accumulates mostly-dead repos. Packing inactive ones to S3-compatible storage and rehydrating on first access keeps the disk and RAM numbers flat, supporting the published low-RAM guarantee. Assumption: rehydration latency of a few seconds is acceptable for cold repos.
  Done when: a repo untouched for N days is offloaded, its local directory is removed, and a clone afterward succeeds with identical refs.
