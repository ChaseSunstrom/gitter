# Brainstorm — lens: bold bets

- **Ship a "Gitter Cloud-in-a-Compose" pooled-runner marketplace** — FEATURE — value 4 — effort L — risk high — category: ecosystem
  Why: assumption: public instances want CI capacity without owning hardware; the pack covers volunteer laptops but not accounting. Runners advertise capacity and price/credit, and jobs are matched by label, with signed results.
  Done when: a second machine registers a runner with a credit rate, and a job from another account runs on it and debits and credits both ledgers.

- **Make the repo the unit of deployment: `gitter serve <repo>` on any host** — FEATURE — value 4 — effort M — risk med — category: portability
  Why: the pack stresses single-binary and mirror mode but not a repo that carries its own hosting. A repo with its issues and PR refs is served read-only by one command with the same UI.
  Done when: running `gitter serve` in a cloned repo opens the full UI offline with history, issues and CI results, with no database setup.

- **Add a "zero-state instance" declarative `gitter.toml` that reconverges** — FEATURE — value 4 — effort M — risk low — category: ops
  Why: "settings in the UI" and "GitOps config" are listed, but reconciliation of orgs, repos, runners and policies from one file is not. This enables rebuild-from-scratch in one command.
  Done when: deleting the DB and restarting with the file recreates orgs, teams, repo settings and runner tokens identically.

- **Enforce an executable "simpler than GitLab" budget in CI** — CLEAN — value 4 — effort S — risk low — category: simplicity
  Why: the user's core constraint is simpler and faster than GitLab; existing ideas publish numbers but none fail the build. Gate on image size, idle RSS, cold-boot time, compose line count and env var count.
  Done when: a PR that adds a 6th required env var or raises idle RSS over the budget fails the pipeline.

- **Add per-PR ephemeral full-stack preview via compose profiles** — FEATURE — value 4 — effort L — risk med — category: CI
  Why: previews appear only as a wild URL idea; the stronger form runs the repo's own compose file in the sandbox, routes a subdomain, and tears down on merge, with a TTL and resource cap.
  Done when: opening a PR yields a working URL running the PR's compose stack that disappears after the PR closes.

- **Build a deterministic replay of any CI job from its recorded inputs** — FEATURE — value 4 — effort M — risk med — category: CI
  Why: assumption: flakiness and "works on my machine" are top CI pains. Record the image digest, env, input hash and commit, then `gitter ci replay <job>` reproduces it locally with the same inputs.
  Done when: replaying a failed job locally gives the same exit code and log hash, or reports exactly which input differed.

- **Offer a "fork-the-instance" snapshot sharing** — FEATURE — value 3 — effort M — risk med — category: sharing
  Why: assumption: people want to hand someone a working copy of a project with its issues and CI config. Export a repo bundle containing git data, issues, PRs and pipeline definitions into one signed file that another instance imports.
  Done when: exporting on instance A and importing on B reproduces issues, PRs and pipelines with matching content hashes.

- **Add a built-in agent lane with scoped identity and budget** — FEATURE — value 4 — effort L — risk high — category: agents
  Why: agents appear only as a wild idea. Make agents first-class accounts with scoped tokens, a spend/time budget, forced PR-only writes and CI as the gate, plus an audit trail.
  Done when: an agent account opens a PR from an issue, cannot push to protected branches, and is stopped at its budget with a visible reason.

- **Wild: Make the forge answer questions with a local-only "ask this repo" index** — FEATURE — value 3 — effort L — risk med — category: local-first
  Why: wild; the trigram search exists, but a local embedding index built on push with a small on-box model gives semantic search and "where is X handled" with no outbound calls, off by default.
  Done when: with network blocked, a natural-language query returns the correct file in a fixture repo, and the setting defaults to off.

- **Wild: Turn the merge queue into a visible, playable "train" view** — FEATURE — value 3 — effort M — risk low — category: delight
  Why: wild; the merge queue and speculative batching are listed, but a live animated train of PRs with cars that turn green or get kicked out makes queue state legible and shareable.
  Done when: queued PRs render as cars on a track, and a failing car is visibly ejected while the rest re-batch.

- **Wild: Let an instance "sleep" to zero and wake on first request** — PERFORMANCE — value 4 — effort M — risk med — category: efficiency
  Why: wild; the low-RAM guarantee is listed, but not scale-to-zero. Persist state, drop idle runners and caches, and wake in under a second via a tiny listener, useful for hobby boxes and Raspberry Pis.
  Done when: after idle timeout the process footprint is under a set MB, and the first request returns correctly within one second.

- **Add an in-product "migrate me off" exit guarantee** — FEATURE — value 3 — effort S — risk low — category: trust
  Why: assumption: trust in a self-hosted forge grows when leaving is easy. One command exports all repos, issues, PRs and wiki to open formats with a verification report.
  Done when: the export imports into a stock Gitea or plain git plus JSON, and the report shows zero missing items.

- **Adopt a signed, reproducible release with a one-line verify** — SECURITY — value 3 — effort S — risk low — category: supply-chain
  Why: the product asks admins to run a public, privileged service; the compose file pulls one image. Pin by digest, sign it, publish an SBOM, and have `gitter doctor` verify the running image.
  Done when: `gitter doctor` reports the image digest matches a signed release and fails on a tampered image.
