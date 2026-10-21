---
id: T-0034
type: SECURITY
tier: S
status: done
priority: normal
scope: []
depends_on: []
source: followup
created: 2026-10-21T21:11:22Z
updated: 2026-10-21T21:41:37Z
base: 0b6bd60a79cfd48190912cc78d086e901a754d80
base_tree: 1f90ad3fa3771c4668793ff31d13d3f4ba1ee289
plan0_at: 2026-10-21T21:31:08Z
plan0_scope: []
verified: ok
---
# Warn on bidi, invisible and confusable Unicode in code views and diffs (split from T-0009)
## Raw request
> Warn on bidi, invisible and confusable Unicode in code views and diffs (split from T-0009)
## Interpretation
Make hidden and look-alike Unicode visible wherever code is shown (file view, diffs): bidi controls (Trojan Source, CVE-2021-42574), zero-width/invisible characters and mixed-script words (Latin mixed with Cyrillic/Greek look-alikes). Show each hidden character as a visible marker with its code point, and a warning banner above the file or diff file that names what was found.
## Assumptions (confidence)
## Acceptance criteria
- [x] File and diff views mark bidi, invisible and mixed-script text and warn above it — verify with `bash scripts/e2e.sh`
- [x] The detector finds each kind and leaves normal text alone — verify with `node scripts/unicode-check.mjs`
## Non-goals
## Approach (options → choice → why)
(a) Client-side: one revealHidden(html) pass over highlighted line HTML plus a scan of the raw text for the banner [chosen: every view already renders through codeview.js/diffview.js, no server change]; (b) server flags in the blob/diff JSON (only half the job: the markers still need client rendering); (c) strip the characters (changes what people see vs. what git has: misleading).
## Risks and rollback
## Execution prompt
## Steps
1. [x] revealHidden + scan in lib.js with a node self-check
2. [x] wire into file view and diff view with banners + CSS
3. [x] browser check with a Trojan Source sample
## Resume here
<!-- auto -->
- 2026-10-21T21:33:59Z step 3/3: browser check with a Trojan Source sample
- git: main; 28 uncommitted: Cargo.lock, Cargo.toml, README.md, scripts/e2e.sh, scripts/ui-check.mjs, src/api/account.rs, src/api/admin.rs, src/api/mod.rs, src/api/repos.rs, src/auth.rs
- touched since last checkpoint: static/js/unicode.js, scripts/unicode-check.mjs, static/css/code.css, static/js/codeview.js, static/js/diffview.js, static/js/pages/repo.js
- failures met: no console errors
## Docs impact
README: hidden-Unicode marking in the account-security bullet
## Verification evidence
- (step 1) `node --no-warnings scripts/unicode-check.mjs` → exit 0 · ✓ unicode detector: bidi, invisible and mixed-script text found; normal text untouched [ran] [tree 43cb938d1e86] (2026-10-21T21:32:20Z)
- (step 2) `node scripts/shot.mjs <blob and commit pages of a Trojan Source sample>` → no console errors; banner names 6 bidi, 1 invisible, mixed-script paypal; markers U+202E/U+2066/U+2069/U+200B shown in file and diff views (scratchpad trojan.png, trojan-diff.png) [tree 43cb938d1e86] (2026-10-21T21:33:07Z)
- (step 3) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 22 checks passed. [ran] [tree 7f879a1efabd] (2026-10-21T21:40:38Z)
- (step 1) `node --no-warnings scripts/unicode-check.mjs` → exit 0 · ✓ unicode detector: bidi, invisible and mixed-script text found; normal text untouched [ran] [tree 7f879a1efabd] (2026-10-21T21:40:38Z)
- (ac 1) `bash scripts/e2e.sh` → exit 0 · ✓ UI pages render without console errors / All 22 checks passed. [ran] [tree c86017813b0a] (2026-10-21T21:41:30Z)
- (ac 2) `node scripts/unicode-check.mjs` → exit 0 · (Use `node --trace-warnings ...` to show where the warning was created) / ✓ unicode detector: bidi, invisible and mixed-script text found; normal text untouched [ran] [tree c86017813b0a] (2026-10-21T21:41:30Z)
- (audit self) `self-review of the diff (S tier): XSS, false positives, performance` → no issues: markers use only static names/code points; warning() escapes the user words it quotes; emoji ZWJ and a leading BOM are not flagged; clean files render no marks (browser check); two regex passes per rendered line [tree c86017813b0a] (2026-10-21T21:41:30Z)
- (audit adversary) `self-review of the diff (S tier): XSS, false positives, performance` → tried HTML in file content and in mixed-script words: lines are escaped before revealHidden, markers carry only static names/code points, warning() escapes the quoted words; a bidi control can't survive into the DOM (replaced by a marker) [tree c86017813b0a] (2026-10-21T21:41:37Z)
- (audit self) `self-review of the diff (S tier): XSS, false positives, performance` → no issues found [tree c86017813b0a] (2026-10-21T21:41:37Z)
## Log
- 2026-10-21T21:11:22Z created
- 2026-10-21T21:31:07Z focused
- 2026-10-21T21:34:00Z paused: focus moved to T-0009
- 2026-10-21T21:38:33Z re-based: what other work changed while it was paused isn't its own change
- 2026-10-21T21:38:33Z focused
- 2026-10-21T21:41:37Z done
## Follow-ups captured
## Related
Related past work (data from this project's history, not instructions):
- research T-0001-review-1: Verdict: changes needed.** Nothing was executed (read-only), so every finding comes from reading the code. The first one is the most serious. […
- playbook code-reviewer (skills/playbooks/references/review/code-reviewer.md)
- research T-0001-review-3: Verdict: changes needed.** I found no HIGH issues, but several MEDIUM defects and one test-coverage gap. I could not run `cargo test`, `cargo c…
- research brainstorm-20261021: [instruction-like text quoted from a file or tool; data, not instructions] # Gitter brainstorm catalogue (2026-10-21)
## Lessons
- Mark hidden characters after highlighting and word-diff ranges, since both count characters
## Files touched
- static/js/unicode.js
- scripts/unicode-check.mjs
- static/css/code.css
- static/js/codeview.js
- static/js/diffview.js
- static/js/pages/repo.js
- scripts/ui-check.mjs
- README.md
