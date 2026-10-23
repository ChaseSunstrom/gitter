---
id: T-0073
type: FIX
tier: S
status: done
priority: normal
scope: []
depends_on: []
source: discovered
created: 2026-10-22T02:35:00Z
updated: 2026-10-23T03:51:11Z
group: container registry follow-ups
base: efa99a9eddc6e9cb3ccc87fa6d0c7b9c66befc43
base_tree: 260af4bd3720422b82dd091e94b2545574735717
plan0_at: 2026-10-23T03:42:07Z
plan0_scope: []
verified: strong
---
# Registry: repository names OCI can't use get a clear refusal and no pull command
## Raw request
> Repo names that are valid in Gitter but not valid OCI components (`a..b`, `a-.b`) cannot use the registry, yet the Packages tab shows a pull command for them. `foo.git` also resolves to repo `foo`, b…
> (noticed by an agent; research T-0014-review)
## Interpretation
Gitter allows repository names that the OCI distribution spec doesn't (consecutive separators like a..b, a separator before a dot like a-.b). The registry then can't serve them, yet the Packages tab still shows a docker pull command. Also foo.git in a registry path resolves to the repository foo. The registry should refuse such names clearly, the page shouldn't offer a command that can't work, and .git shouldn't be stripped in registry paths.
## Assumptions (confidence)
## Acceptance criteria
- [x] a repository whose name isn't a valid OCI path component (a..b, a-.b) gets a refusal saying why on push and pull, and the Packages tab shows no pull command for it; foo.git doesn't resolve to foo in registry paths — verify with `cargo test -q --test api registry`
## Non-goals
## Approach (options → choice → why)
Options: (1) check names against the OCI component grammar ([a-z0-9]+((\.|_|__|-+)[a-z0-9]+)*) in the registry and say why; (2) map invalid names to a valid alias; (3) forbid such repository names outright. Choice: 1, it changes nothing for repositories that don't use the registry; aliasing adds hidden names to learn.
## Risks and rollback
## Execution prompt
## Steps
1. [x] test: names a..b and a-.b refused with a reason by the registry, Packages API says unavailable; fix: src/registry.rs name check and the packages endpoint
## Resume here
## Docs impact
## Verification evidence
- (step 1) `cargo test -q --test api registry` → ✗ exit 101 · test result: FAILED. 7 passed; 1 failed; 0 ignored; 0 measured; 81 filtered out; finished in 21.22s / error: test failed, to rerun pass `--test api` [ran] [tree dbcb637b2246] (2026-10-23T03:44:26Z)
- (step 1) `cargo test -q --test api registry` → exit 0 · ........ / test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 81 filtered out; finished in 22.49s [ran] [tree 2a43d482d959] (2026-10-23T03:45:32Z)
- (step 1) `cargo test -q --test api registry` → exit 0 · ........ / test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 81 filtered out; finished in 15.25s [ran] [tree 4dd55ba0bbe7] (2026-10-23T03:47:44Z)
- (ac 1) `cargo test -q --test api registry` → exit 0 · ........ / test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 81 filtered out; finished in 11.65s [ran] [tree 4dd55ba0bbe7] (2026-10-23T03:47:57Z)
- `cargo test -q --test api registry` → ✗ exit 101 · test result: FAILED. 7 passed; 1 failed; 0 ignored; 0 measured; 81 filtered out; finished in 11.95s / error: test failed, to rerun pass `--test api` (start tree + this task's tests) [ran] [tree 4dd55ba0bbe7] (2026-10-23T03:49:09Z)
- `cargo test -q --test api registry` → exit 0 · ........ / test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 81 filtered out; finished in 10.84s [ran] [tree 4dd55ba0bbe7] (2026-10-23T03:49:09Z)
- (ac 1) `cargo test -q --test api registry` → exit 0 · ........ / test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 81 filtered out; finished in 10.64s [ran] [tree 4dd55ba0bbe7] (2026-10-23T03:51:11Z)
- (audit self) `self review of the diff: names are checked after lowercasing with the same component() the image parser uses; mount sources get the same check so foo.git can't reach foo; the Packages API says why and the tab shows that instead of commands; a..b can't be a repository name, so a-.b and a_.b stand in` → no findings [tree 4dd55ba0bbe7] (2026-10-23T03:51:11Z)
## Log
- 2026-10-22T02:35:00Z created
- 2026-10-23T03:41:47Z planned from capture
- 2026-10-23T03:41:50Z approved: the user asked to implement every queued feature (standing request)
- 2026-10-23T03:41:50Z set approved=True
- 2026-10-23T03:41:55Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0073.builder.md
- 2026-10-23T03:42:07Z focused
- 2026-10-23T03:46:45Z merged worktree-agent-a6f3a8e0afb90835f (3 file(s))
- 2026-10-23T03:46:55Z lane removed: /home/chase/projects/gitter/.claude/worktrees/agent-a6f3a8e0afb90835f
- 2026-10-23T03:47:03Z focused
- 2026-10-23T03:51:11Z done
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research T-0014-review: Verdict: approve with nits.** I found no CRITICAL or HIGH defects, and the access-control focus areas all hold. Two MEDIUM operational gaps shoul…
- T-0014 [FEATURE M, done, 5 steps] Package/container registry + pull-through proxies (C20) — lesson: A registry that keeps untagged manifests needs a visible way to delet…
- research brainstorm-20261021: [instruction-like text quoted from a file or tool; data, not instructions] # Gitter brainstorm catalogue (2026-10-21)
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
- similar past tasks were M: consider --tier M
## Plan gaps
- failed first: step 1 test: names a..b and a-.b refused with a reason by the registry, Packages API says unavailable; fix…
## Files touched
- tests/api.rs
- src/registry.rs
- static/js/pages/packages.js
