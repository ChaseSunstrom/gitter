# Brainstorm — lens: capability map

Products mapped: GitHub, GitLab, Forgejo/Gitea, SourceHut, Gerrit, Bitbucket, Woodpecker, Drone, Buildkite, Dagger, Radicle. The pack only shows the core, a CI sketch and a compose-file goal. These are the gaps, best first.

- **Ship one-file compose with runner sidecar** — FEATURE — value 5 — effort M — risk med — category: deploy
  Why: the user's core constraint is one simple docker-compose.yml with runners.
  Done when: `docker compose up` on a clean host yields a working site and a runner that completes a sample pipeline.

- **Add Actions-style workflow YAML with matrix and cache** — FEATURE — value 5 — effort L — risk med — category: CI
  Why: T-0002 says "pipelines in repo"; the format is undefined. Assumption: GitHub-Actions-compatible syntax eases migration.
  Done when: a `.gitter/workflows/*.yml` matrix job runs in parallel and restores a cache on the second run.

- **Register runners by token with labels and autoscale** — FEATURE — value 4 — effort M — risk med — category: runners
  Why: "runners" are asked for; assumption: remote runners must join with a one-time token.
  Done when: a second machine registers via token and picks up only jobs matching its labels.

- **Stream CI logs live with secrets masking** — FEATURE — value 4 — effort M — risk low — category: CI
  Why: T-0002 asks for log streaming; secrets are a public-site risk.
  Done when: a log shows lines within 1s and a stored secret value appears as `***`.

- **Gate merges on required checks and branch protection** — FEATURE — value 4 — effort M — risk low — category: PRs
  Why: the status checks on PRs in T-0002 need enforcement to be useful.
  Done when: a PR with a failing required check has its merge button disabled and the API rejects the merge.

- **Add code review: inline comments, suggestions, approvals** — FEATURE — value 4 — effort M — risk low — category: review
  Why: PRs with merge are planned, review is not. Assumption: needed for parity with GitHub and GitLab.
  Done when: a reviewer comments on a diff line, posts a suggested change, and the author applies it in one click.

- **Provide packages and container registry** — FEATURE — value 4 — effort L — risk med — category: registry
  Why: GitLab and Forgejo both ship this and CI needs somewhere to push images. Assumption.
  Done when: CI pushes an image and `docker pull` fetches it using a personal access token.

- **Add webhooks, OAuth apps and a REST API with OpenAPI** — FEATURE — value 4 — effort M — risk low — category: integrations
  Why: only tokens are planned. Assumption: a self-hosted tool is part of a larger toolchain.
  Done when: a push fires a signed webhook and the API spec validates against a generated client.

- **Add orgs, teams, permissions and SSO (OIDC)** — FEATURE — value 4 — effort L — risk med — category: access
  Why: a multi-user public site needs more than a flat user list. Assumption.
  Done when: an org team has read access to a private repo and an OIDC login maps to an account.

- **Add abuse controls: rate limits, quotas, spam, captcha** — SECURITY — value 4 — effort M — risk low — category: safety
  Why: the pack says a public multi-user site, so abuse and safety matter.
  Done when: a burst of signups or pushes beyond the limit gets 429, and a per-user storage quota blocks a push.

- **Sandbox CI jobs with resource limits and no network by default** — SECURITY — value 5 — effort M — risk high — category: CI safety
  Why: docker runners executing public users' code is the biggest risk. Assumption: untrusted forks need isolation.
  Done when: a fork PR job cannot read secrets or reach the host, and is killed at its memory and time limits.

- **Back up and restore with one command, plus Litestream-style replication** — FEATURE — value 4 — effort M — risk low — category: ops
  Why: the single-binary SQLite bet makes the data one volume to protect.
  Done when: `gitter backup` then restore on a fresh volume gives identical repos and database.

- **Add pages and releases with asset uploads** — FEATURE — value 3 — effort M — risk low — category: publishing
  Why: tags are planned, but release artifacts from CI and static hosting are standard. Assumption.
  Done when: a tagged CI job attaches a binary to a release and a pages branch serves a site.

- **Git LFS support** — FEATURE — value 3 — effort M — risk med — category: storage
  Why: the planned push size limits will block binary-heavy repos. Assumption.
  Done when: `git lfs push` and clone work against a repo with a 100MB file.

- **Wild: Run CI as local-first "preview environments" per PR** — FEATURE — value 4 — effort L — risk high — category: CI
  Why: wild. Every PR gets a live URL from its built container, like Vercel.
  Done when: opening a PR yields a reachable subdomain serving that branch's build, torn down on merge.

- **Wild: Add a built-in AI-free "explain this diff" via local LLM hook** — FEATURE — value 3 — effort M — risk med — category: review
  Why: wild. A configurable local model endpoint summarises PRs and failing CI logs, off by default.
  Done when: with an endpoint set, a failing job shows a summary; with none set, nothing is sent anywhere.

- **Wild: Federate repos and PRs between Gitter instances** — FEATURE — value 3 — effort L — risk high — category: federation
  Why: wild. Forgejo is working on ForgeFed, and a fast self-hosted node could fork and PR across instances.
  Done when: a user on instance A forks and opens a PR against a repo on instance B.

- **Wild: Offer a zero-config `gitter ci run` that executes the same workflow locally** — FEATURE — value 4 — effort M — risk low — category: CI
  Why: wild. Like `act` or Dagger, it removes the push-and-wait loop; the surface is a CLI.
  Done when: running it locally gives the same pass or fail as the server for a sample workflow.

- **Add built-in code search across repos with trigram index** — PERFORMANCE — value 4 — effort M — risk low — category: search
  Why: the pack plans explore/search only; "way faster" than GitLab needs instant code search.
  Done when: a query across 100 repos returns in under 100ms with highlighted matches.

- **Add a project board, milestones and issue templates** — FEATURE — value 3 — effort M — risk low — category: planning
  Why: issues are planned, but planning tools are part of "all features". Assumption.
  Done when: an issue moves across board columns and a milestone shows percent complete.
