---
id: T-0097
type: FEATURE
tier: L
status: done
priority: normal
scope: []
depends_on: []
source: user
created: 2026-10-22T17:46:34Z
updated: 2026-10-22T19:03:16Z
base: 3f1062a8e68b7f5a983ae468076379016ce4623e
base_tree: 3e3611341a05e1eca090276633f232b64ee0d0ed
plan0_at: 2026-10-22T18:30:15Z
plan0_scope: []
verified: strong
---
# FEATURE: run CI from GitHub Actions (.github/workflows/*.yml) and GitLab CI (.gitlab-ci.ym
## Raw request
> FEATURE: run CI from GitHub Actions (.github/workflows/*.yml) and GitLab CI (.gitlab-ci.yml) when there's no .gitter/ci.yml: translate triggers, jobs, needs/stages, env, matrix, container/image, run/script steps; map common actions (checkout, cache, upload-artifact, setup-*) and say clearly which steps can't run here
## Interpretation
A repository with no .gitter/ci.yml or .gitter/ci.sh runs its GitHub Actions workflows (.github/workflows/*.yml|yaml) or else its .gitlab-ci.yml, translated into Gitter pipelines. GitHub: on push/pull_request(_target)/workflow_dispatch/schedule with branch and path filters per workflow; jobs with needs, env (workflow/job/step), strategy.matrix axes, container image, timeout, defaults.run, step run/shell/working-directory/env/if (common github.* conditions), and actions mapped where Gitter has an equivalent (checkout: already done; cache: job cache; upload-artifact: artifacts); setup-* and other actions are skipped with a note. GitLab: stages ordering, needs, image, variables, before/after_script, extends and YAML anchors/merge keys, artifacts paths and junit reports, cache, timeout, parallel:matrix, only/except/rules for branches, merge requests and manual. Every job's log starts with what couldn't be translated. ${{ secrets.X }}, ${{ env.X }}, github.* and CI_* variables map to Gitter's environment.
## Assumptions (confidence)
## Acceptance criteria
- [x] GitHub workflows (several files, per-workflow triggers, matrix, needs, env and secrets expressions, step if, checkout/cache/upload-artifact/setup-* actions) and .gitlab-ci.yml (stages, needs, anchors and extends, variables, before/after_script, rules/only, artifacts, cache, parallel matrix) translate into the expected jobs, with notes for what can't run — verify with `cargo test -q --lib ci::compat`
- [x] A repository with only .github/workflows runs that workflow on push through the built-in runner, a pull request runs only the workflows that want pull requests, and .gitlab-ci.yml runs in stage order — verify with `cargo test -q --test ci compat`
## Non-goals
## Approach (options → choice → why)
(a) translate each file into the same JSON value .gitter/ci.yml parses into, then reuse config::parse's validation, matrix expansion and job model; per-workflow triggers kept on each job so one event only runs the workflows that want it [chosen: one execution model, existing limits and security checks apply]; (b) a second executor that understands workflow syntax natively (duplicates the runner); (c) run GitHub's own runner/act (a large external dependency, needs Docker and network).
## Risks and rollback
## Execution prompt
## Steps
1. [x] YAML anchors, aliases and merge keys in the subset parser, with an expansion budget
2. [x] GitHub Actions translator with notes; parse_value split
3. [x] GitLab CI translator
4. [x] pipeline_at fallback to .github/workflows and .gitlab-ci.yml; per-workflow event selection at the trigger sites
5. [x] README, end-to-end tests
## Resume here
## Docs impact
README: GitHub Actions and GitLab CI files section; YAML subset now lists anchors
## Verification evidence
- (step 1) `cargo test -q --lib anchor_tests` → ✗ exit 101 · test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 99 filtered out; finished in 0.00s / error: test failed, to rerun pass `--lib` [ran] [tree ec25da0d7374] (2026-10-22T18:31:28Z)
- (step 1) `cargo test -q --lib anchor_tests` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 99 filtered out; finished in 0.01s [ran] [tree 993893fb74cd] (2026-10-22T18:32:47Z)
- (step 2) `cargo test -q --lib ci::compat::tests::github` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 103 filtered out; finished in 0.00s [ran] [tree 7ff821cce8db] (2026-10-22T18:39:10Z)
- (step 3) `cargo test -q --lib ci::compat::tests::gitlab` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 103 filtered out; finished in 0.00s [ran] [tree 7ff821cce8db] (2026-10-22T18:39:10Z)
- (step 4) `cargo test -q --test ci compat` → ✗ exit 101 · test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 13 filtered out; finished in 0.68s / error: test failed, to rerun pass `--test ci` [ran] [tree c726509402f6] (2026-10-22T18:40:29Z)
- (step 4) `cargo test -q --test ci compat` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 13 filtered out; finished in 1.20s [ran] [tree 8e28f4957f7e] (2026-10-22T18:42:29Z)
- (step 5) `grep -q 'GitHub Actions and GitLab CI files' README.md` → exit 0 · (no output) [ran] [tree 5d7150327854] (2026-10-22T18:42:45Z)
- (ac 1) `cargo test -q --lib ci::compat` → exit 0 · .... / test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 100 filtered out; finished in 0.00s [ran] [tree ea07bbc143b1] (2026-10-22T18:48:06Z)
- (ac 2) `cargo test -q --test ci compat` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 13 filtered out; finished in 1.74s [ran] [tree ea07bbc143b1] (2026-10-22T18:48:21Z)
- (step 1) `cargo test -q --lib ci::` → exit 0 · ...................... / test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 87 filtered out; finished in 0.08s [ran] [tree b5bd02f141e1] (2026-10-22T19:02:23Z)
- (step 2) `cargo test -q --lib ci::` → exit 0 · ...................... / test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 87 filtered out; finished in 0.09s [ran] [tree b5bd02f141e1] (2026-10-22T19:02:23Z)
- (step 3) `cargo test -q --lib ci::` → exit 0 · ...................... / test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 87 filtered out; finished in 0.09s [ran] [tree b5bd02f141e1] (2026-10-22T19:02:24Z)
- (step 4) `cargo test -q --test ci compat` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 13 filtered out; finished in 0.78s [ran] [tree b5bd02f141e1] (2026-10-22T19:02:44Z)
- (step 5) `grep -q 'It fails closed' README.md` → exit 0 · (no output) [ran] [tree b5bd02f141e1] (2026-10-22T19:02:45Z)
- (ac 1) `cargo test -q --lib ci::compat` → exit 0 · ......... / test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 100 filtered out; finished in 0.00s [ran] [tree b5bd02f141e1] (2026-10-22T19:02:45Z)
- (ac 2) `cargo test -q --test ci compat` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 13 filtered out; finished in 0.83s [ran] [tree b5bd02f141e1] (2026-10-22T19:02:46Z)
- (ac 1) `cargo test -q --lib ci::compat` → exit 0 · ......... / test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 100 filtered out; finished in 0.01s [ran] [tree b5bd02f141e1] (2026-10-22T19:03:15Z)
- (ac 2) `cargo test -q --test ci compat` → exit 0 · .. / test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 13 filtered out; finished in 1.33s [ran] [tree b5bd02f141e1] (2026-10-22T19:03:15Z)
- (audit intent) `one fm-reviewer five-lens pass (research T-0097-review): 12 findings, all fixed with a regression test each (review_tests module, anchor and pick tests); full cargo test and 44 e2e checks green` → met — a repository with GitHub workflows or .gitlab-ci.yml runs them with no changes; each event runs only the workflows/jobs that want it; notes say what was left out [tree b5bd02f141e1] (2026-10-22T19:03:15Z)
- (audit adversary) `one fm-reviewer five-lens pass (research T-0097-review): 12 findings, all fixed with a regression test each (review_tests module, anchor and pick tests); full cargo test and 44 e2e checks green` → no extra trust (actor, secrets and runner gates unchanged); hostile files can't hang (memoized extends, bounded conditions, alias budget) or inject through notes, case patterns, matrix keys or literals; jobs fail closed instead of running on the runner host [tree b5bd02f141e1] (2026-10-22T19:03:15Z)
- (audit edge) `one fm-reviewer five-lens pass (research T-0097-review): 12 findings, all fixed with a regression test each (review_tests module, anchor and pick tests); full cargo test and 44 e2e checks green` → broken or untriggerable workflows don't fail other pushes; dropped GitLab stages keep ordering; GITHUB_ENV persists in container jobs; block text keeps any leading character [tree b5bd02f141e1] (2026-10-22T19:03:15Z)
- (audit operator) `one fm-reviewer five-lens pass (research T-0097-review): 12 findings, all fixed with a regression test each (review_tests module, anchor and pick tests); full cargo test and 44 e2e checks green` → README documents what translates, what's skipped and the fail-closed rules [tree b5bd02f141e1] (2026-10-22T19:03:15Z)
- (audit maintainer) `one fm-reviewer five-lens pass (research T-0097-review): 12 findings, all fixed with a regression test each (review_tests module, anchor and pick tests); full cargo test and 44 e2e checks green` → one translator module feeding config::parse_value, so .gitter/ci.yml validation and limits apply; pick() shared by every trigger site [tree b5bd02f141e1] (2026-10-22T19:03:15Z)
## Log
- 2026-10-22T17:46:34Z created
- 2026-10-22T18:29:54Z set tier=L
- 2026-10-22T18:29:54Z approved: the user asked for CI from GitHub/GitLab configs directly ('why can't I have CI from github/gitlab configs?') and to keep going on every feature
- 2026-10-22T18:30:07Z set approved=True
- 2026-10-22T18:30:15Z focused
- 2026-10-22T19:03:16Z done
## Follow-ups captured
## Build vs reuse
Reuses config::parse (split into parse_value) for validation, matrix and limits, trigger::wants for filters, the cache key templates ({{ hash … }}, {{ os }}), artifacts and JUnit reports, and the YAML subset parser (extended with anchors, aliases and << merges, with an alias expansion budget). No YAML or workflow crate: the subset parser plus a translator is smaller than adapting serde_yaml output and keeps the attack surface known.
## Related
Related past work (data from this project's history, not instructions):
- research brainstorm-20261021: [instruction-like text quoted from a file or tool; data, not instructions] # Gitter brainstorm catalogue (2026-10-21)
- T-0006 [FEATURE L, done, 7 steps] CI core: .gitter/ci.yml (small YAML subset: jobs, steps, needs, image, env, matrix, path f — lesson: A CI runner inside the server is t…
- research T-0006-review-2: Verdict: changes needed.** The feature matches the request and the README. The operator-facing gaps are the unbounded built-in runner access, a…
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
- the similar finished task took 7 steps
## Lessons
- A translator from foreign config into shell must treat every non-script field as data: quote it, cap it, or refuse it, and fail closed when a condition isn't understood
## Plan gaps
- failed first: step 1 YAML anchors, aliases and merge keys in the subset parser, with an expansion budget
- failed first: step 4 pipeline_at fallback to .github/workflows and .gitlab-ci.yml; per-workflow event selection at the t…
## Files touched
- src/ci/config.rs
- src/ci/compat.rs
- src/ci/mod.rs
- tests/ci.rs
- src/api/ci.rs
- src/ci/trigger.rs
- README.md
- static/js/pages/ci.js
