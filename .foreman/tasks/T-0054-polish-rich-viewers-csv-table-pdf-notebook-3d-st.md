---
id: T-0054
type: FEATURE
tier: M
status: captured
priority: normal
scope: []
depends_on: []
source: followup
created: 2026-10-22T00:30:44Z
updated: 2026-10-22T00:48:46Z
---
# Polish: rich viewers (CSV table, PDF, notebook, 3D/STL, audio/video), revert a commit or P
## Raw request
> Polish: rich viewers (CSV table, PDF, notebook, 3D/STL, audio/video), revert a commit or PR from the web, drag-and-drop upload anywhere, PWA manifest, demo seed mode (gitter demo) (split from T-0010)
## Interpretation
Polish that makes the forge pleasant. Rich file viewers: CSV/TSV as a sortable table, PDF inline (sandboxed), Jupyter notebooks rendered (markdown and code cells, outputs as text and images, no script execution), STL/OBJ 3D models with orbit controls, audio and video players, images with zoom and side-by-side or swipe diffs. Revert a commit or a merged PR from the web as a new branch with a revert PR. Drag and drop files anywhere on a repo page to upload. A PWA manifest and icons so it installs as an app. gitter demo seeds a demo instance with sample users, repos, issues, PRs, CI runs and wiki pages for screenshots and trying it out.
## Assumptions (confidence)
## Acceptance criteria
- [ ] Revert of a commit and of a merged PR opens a revert PR; the demo seeder builds a consistent instance — verify with `cargo test -q --test api revert_and_demo`
- [ ] CSV, notebook, PDF, 3D, audio, video and image-diff viewers, drag-and-drop upload and the manifest render without console errors — verify with `bash scripts/e2e.sh`
## Non-goals
## Approach (options → choice → why)
(a) Viewers as lazy-loaded modules chosen by file extension in the blob view, all rendering without eval and inside the existing CSP (notebook outputs sanitized with DOMPurify, PDF in a sandboxed iframe of the raw URL, 3D with a tiny WebGL renderer written here instead of three.js) [chosen: no new dependencies, CSP-safe]; (b) vendoring three.js and pdf.js (heavy). Revert uses git revert in a temporary worktree like merges do. gitter demo uses the API code paths so seeded data is consistent.
## Risks and rollback
## Execution prompt
## Steps
1. [ ] viewers: CSV/TSV table, notebook, PDF, audio and video, image zoom and diff modes  <- CURRENT
2. [ ] 3D viewer for STL and OBJ (small WebGL renderer, orbit controls)
3. [ ] revert a commit or merged PR as a new branch plus PR
4. [ ] drag-and-drop upload anywhere on repo pages; PWA manifest and icons
5. [ ] gitter demo seeder; e2e + ui-check; README
## Resume here
## Docs impact
## Verification evidence
## Log
- 2026-10-22T00:30:44Z created
## Follow-ups captured
