---
id: T-0019
type: FEATURE
tier: M
status: captured
priority: normal
scope: []
depends_on: []
source: followup
created: 2026-10-21T17:01:52Z
updated: 2026-10-22T00:48:44Z
---
# Push secret scanning, dependency/vuln scanning, SBOM (C29)
## Raw request
> Push secret scanning, dependency/vuln scanning, SBOM (C29)
## Interpretation
Security scanning. Push protection: the pre-receive hook scans added lines for secrets (cloud keys, private keys, tokens of common services and Gitter's own token prefixes) with a curated regex set plus entropy checks, and refuses the push with the file, line and kind, unless the pusher marks it as a false positive or test value with a one-time bypass link. Existing content can be scanned on demand. Dependency scanning: lockfiles (Cargo.lock, package-lock.json, yarn.lock, pnpm-lock.yaml, go.sum, requirements.txt, poetry.lock, Gemfile.lock) are parsed into a dependency list per default branch; known vulnerabilities come from an OSV database snapshot the admin imports (offline-friendly) or fetches; alerts show on a Security tab. SBOM: download a CycloneDX JSON (and SPDX) for the default branch.
## Assumptions (confidence)
## Acceptance criteria
- [ ] Pushes adding a secret are refused with the location; bypass works once; scanning is bounded on huge pushes; Gitter's own tokens are caught — verify with `cargo test -q --test api secret_scanning`
- [ ] Lockfiles parse into dependencies, imported OSV advisories match by version, the Security tab lists alerts, CycloneDX SBOM downloads — verify with `cargo test -q --test api dependencies`
- [ ] Security tab and the push-protection message render; e2e push of a fake key is refused — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
(a) Scan only the diff of each pushed ref in the existing pre-receive hook (src/hooks.rs), streaming git diff output, capped in bytes and time so a huge push can't stall receive-pack [chosen]; (b) a post-receive scan that only alerts (secrets already pushed). OSV data stored in a table keyed by ecosystem and package with version ranges; matching uses each ecosystem's version order (semver for cargo and npm, PEP 440 subset for PyPI).
## Risks and rollback
## Execution prompt
## Steps
1. [ ] secret patterns and entropy checks with tests (true and false positives)  <- CURRENT
2. [ ] push protection in the pre-receive hook: diff scan per ref with byte and time caps, refusal message, one-time bypass
3. [ ] lockfile parsers to a dependency list per default branch
4. [ ] OSV import and matching; Security tab with alerts; CycloneDX and SPDX SBOM
5. [ ] e2e, README
## Resume here
## Docs impact
## Verification evidence
## Log
- 2026-10-21T17:01:52Z created
## Follow-ups captured
