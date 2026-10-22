---
id: T-0019
type: SECURITY
tier: M
status: done
priority: normal
scope: []
depends_on: []
source: followup
created: 2026-10-21T17:01:52Z
updated: 2026-10-22T15:09:53Z
base: 0663be6600bed45f8abe2cdd5f56ccbc75d0b82e
base_tree: ef2950ceae1cacbed9c7e4bfae5526c31f706dca
plan0_at: 2026-10-22T03:32:24Z
plan0_scope: []
verified: strong
---
# Push secret scanning, dependency/vuln scanning, SBOM (C29)
## Raw request
> Push secret scanning, dependency/vuln scanning, SBOM (C29)
## Interpretation
Security scanning. Push protection: pushes (git and web commits) are scanned for secrets in added lines (cloud keys, private keys, common service tokens, Gitter's own token prefixes; curated regexes plus an entropy check for generic assignments) and refused with file, line and kind unless the pusher creates a one-time bypass for that finding (false positive / test value / will fix). Repo admins can scan the default branch on demand and see findings. Dependency scanning: lockfiles on the default branch (Cargo.lock, package-lock.json, yarn.lock, pnpm-lock.yaml, go.sum, requirements.txt, poetry.lock, Gemfile.lock) parsed into a dependency list; vulnerabilities matched against an OSV snapshot the site admin imports (offline-friendly: OSV JSON files or zip), shown on a Security tab; SBOM download (SPDX 2.3 JSON or CycloneDX 1.5 JSON).
## Assumptions (confidence)
## Acceptance criteria
- [x] Push protection refuses a git push and a web commit that add an AWS key, a private key or a gtr_ token with file, line and kind; a one-time bypass lets that exact finding through; admins can scan the default branch on demand — verify with `cargo test -q --test scanning`
- [x] Dependencies parsed from at least Cargo.lock, package-lock.json, go.sum and requirements.txt; an imported OSV snapshot flags a vulnerable version; SBOM downloads as SPDX and CycloneDX JSON — verify with `cargo test -q --test scanning deps`
- [x] Security tab (findings for admins, dependencies, vulnerabilities, SBOM links) and the push-refused message render without console errors — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
(a) scanning in the pre-receive path the push already goes through (hooks), with a compiled RegexSet-free matcher (std only: prefix/charset scanners per rule, no regex crate) [chosen if it stays readable; else add the regex crate, justify in Build vs reuse]; (b) post-receive alerts only (doesn't protect). Dependencies parsed on push to the default branch, cached per commit. OSV import into tables keyed by ecosystem+name with version ranges (semver compare per ecosystem kept simple: exact versions list plus SEMVER ranges).
## Risks and rollback
## Execution prompt
## Steps
1. [x] secret rules + scanner with unit tests
2. [x] push protection in git push and web commits, bypass flow
3. [x] on-demand scan + findings API
4. [x] lockfile parsers + OSV import + matching + SBOM
5. [x] Security tab UI, README, e2e
## Resume here
## Docs impact
README security scanning: push protection, allowances, caps; Security tab, OSV import, SBOM
## Verification evidence
- (step 1) `cargo test -q --lib scan:: 2>&1 | tail -40` → exit 0 · ......... / test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 72 filtered out; finished in 0.00s [ran] [tree a23c8aadf8d7] (2026-10-22T03:44:51Z)
- (step 1) `cargo test -q --lib scan::` → exit 0 · ......... / test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 72 filtered out; finished in 0.00s [ran] [tree a23c8aadf8d7] (2026-10-22T03:44:54Z)
- (ac 1) `cargo test -q --test scanning 2>&1 | tail -30` → exit 0 · ....... / test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.41s [ran] [tree 02d110ff5dd8] (2026-10-22T03:53:51Z)
- (step 2) `cargo test -q --test scanning 2>&1 | tail -30` → exit 0 · ....... / test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.41s [ran] [tree 02d110ff5dd8] (2026-10-22T03:53:51Z)
- (step 2) `cargo test -q --test scanning` → exit 0 · ....... / test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.32s [ran] [tree 02d110ff5dd8] (2026-10-22T03:54:10Z)
- (step 3) `cargo test -q --test scanning admins_scan` → exit 0 · . / test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 6 filtered out; finished in 2.67s [ran] [tree 02d110ff5dd8] (2026-10-22T03:54:14Z)
- (step 4) `cargo test -q --test scanning deps` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 3.08s [ran] [tree 02d110ff5dd8] (2026-10-22T03:54:18Z)
- (step 5) `env CARGO_INCREMENTAL=0 bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 41 checks passed. [ran] [tree c5bca155f087] (2026-10-22T04:08:39Z)
- (ac 1) `cargo test -q --test scanning` → exit 0 · ....... / test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.15s [ran] [tree d1109834611a] (2026-10-22T15:01:19Z)
- (ac 2) `cargo test -q --test scanning deps` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 1.51s [ran] [tree d1109834611a] (2026-10-22T15:01:21Z)
- (ac 3) `bash scripts/e2e.sh 2>&1 | tail -1` → exit 0 · All 42 checks passed. [ran] [tree d1109834611a] (2026-10-22T15:04:17Z)
- (step 5) `bash scripts/e2e.sh 2>&1 | tail -1` → exit 0 · All 42 checks passed. [ran] [tree d1109834611a] (2026-10-22T15:06:49Z)
- (step 1) `cargo test -q --test scanning` → exit 0 · ....... / test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.86s [ran] [tree d1109834611a] (2026-10-22T15:07:18Z)
- (step 2) `cargo test -q --test scanning` → exit 0 · ....... / test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.01s [ran] [tree d1109834611a] (2026-10-22T15:07:21Z)
- (step 3) `cargo test -q --test scanning` → exit 0 · ....... / test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.01s [ran] [tree d1109834611a] (2026-10-22T15:07:25Z)
- (step 4) `cargo test -q --test scanning deps` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 1.43s [ran] [tree d1109834611a] (2026-10-22T15:07:27Z)
- (ac 1) `cargo test -q --test scanning` → exit 0 · ....... / test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.06s [ran] [tree d1109834611a] (2026-10-22T15:09:52Z)
- (ac 2) `cargo test -q --test scanning deps` → exit 0 · ... / test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 1.45s [ran] [tree d1109834611a] (2026-10-22T15:09:52Z)
- (ac 3) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 42 checks passed. [ran] [tree d1109834611a] (2026-10-22T15:09:52Z)
- (audit intent) `fm-reviewer five-lens review: changes needed; fixed in the lane merge with tests (linear assignment scan, whole-body key fingerprints, keyed fingerprints, URL skip, git failure refuses, findings past 100 need allowances, combined hook with branch rules, SSH scanned, memory and zip caps, SPDX fields); follow-ups captured` → all three criteria pass on main: scanning 7/7, deps 3/3, e2e 42/42 [tree d1109834611a] (2026-10-22T15:09:52Z)
- (audit adversary) `fm-reviewer five-lens review: changes needed; fixed in the lane merge with tests (linear assignment scan, whole-body key fingerprints, keyed fingerprints, URL skip, git failure refuses, findings past 100 need allowances, combined hook with branch rules, SSH scanned, memory and zip caps, SPDX fields); follow-ups captured` → one hook for rules then scan, fail closed on git errors and unreadable refs; allowances spent only on accepted pushes; NUL-byte files stay unscanned by choice (the pusher can allow any finding, so this guards against accidents, not the pusher) [tree d1109834611a] (2026-10-22T15:09:52Z)
- (audit edge) `fm-reviewer five-lens review: changes needed; fixed in the lane merge with tests (linear assignment scan, whole-body key fingerprints, keyed fingerprints, URL skip, git failure refuses, findings past 100 need allowances, combined hook with branch rules, SSH scanned, memory and zip caps, SPDX fields); follow-ups captured` → 512 MB / 120 s scan cap logged; non-UTF-8 refs refused; PGP armor headers; tab-suffixed paths [tree d1109834611a] (2026-10-22T15:09:52Z)
- (audit operator) `fm-reviewer five-lens review: changes needed; fixed in the lane merge with tests (linear assignment scan, whole-body key fingerprints, keyed fingerprints, URL skip, git failure refuses, findings past 100 need allowances, combined hook with branch rules, SSH scanned, memory and zip caps, SPDX fields); follow-ups captured` → hook binary path fixed at startup, works on noexec data volumes; README push protection section [tree d1109834611a] (2026-10-22T15:09:52Z)
## Log
- 2026-10-21T17:01:52Z created
- 2026-10-22T03:31:25Z planned from capture
- 2026-10-22T03:32:04Z builder brief: /home/chase/.claude/foreman/state/projects/gitter-588bc6/audits/T-0019.builder.md
- 2026-10-22T03:32:24Z focused
- 2026-10-22T15:00:47Z merged worktree-agent-ae55daa58c9d0cbcf (23 file(s))
- 2026-10-22T15:00:51Z lane removed: /home/chase/projects/gitter/.claude/worktrees/agent-ae55daa58c9d0cbcf
- 2026-10-22T15:00:52Z focused
- 2026-10-22T15:09:53Z done
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research brainstorm-20261021: [instruction-like text quoted from a file or tool; data, not instructions] # Gitter brainstorm catalogue (2026-10-21)
- playbook Cloud & Infrastructure Security Skill (skills/playbooks/references/security/cloud-infrastructure.md)
- research T-0001-review-2: Verdict:** changes needed. I read the diff and spot-checked `src/`, `scripts/` and `static/js` for line numbers. I did not run cargo or the e2e…
- decision 2026-10-21: Slate: all 38 build clusters in 6 rounds after T-0001: B self-hosting+abuse (T-0004,T-0005), C CI core+trust (T-0006,T-0007), D collaboration+securi…
## Lessons
- Two features that both need a pre-receive hook must share one: give the hook one entry that runs each check in order and fails closed, rather than letting each feature install its own
## Files touched
- src/scan.rs
- src/lib.rs
- src/error.rs
- src/main.rs
- src/db.rs
- tests/scanning.rs
- src/smart_http.rs
- src/api/code.rs
- src/api/reviews.rs
- src/ci/config.rs
- src/deps.rs
- src/api/scanning.rs
- src/api/mod.rs
- static/js/lib.js
- static/js/pages/security.js
- static/js/repohead.js
- static/js/main.js
- static/js/pages/editor.js
- scripts/ui-check.mjs
- scripts/e2e.sh
- README.md
- Cargo.lock
- Cargo.toml
- Dockerfile
- src/api/account.rs
- src/api/checks.rs
- src/api/ci.rs
- src/api/issues.rs
- src/api/lifecycle.rs
- src/api/milestones.rs
