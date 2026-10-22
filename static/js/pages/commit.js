// One commit: message, authorship, parents and the full diff.
import { html, state, get, send, avatar, timeEl, short, fullDate, sigBadge, action, toast, $, safeUrl, confirmDialog } from '../lib.js';
import { icon } from '../icons.js';
import { loadRepo, repoUrl, repoPage, mountRepoHeader, ciIcon } from '../repohead.js';
import { diffView, mountDiff } from '../diffview.js';
import { navigate } from '../main.js';

const url = (ctx) => `${repoUrl(ctx.params.owner, ctx.params.repo)}/commit/${encodeURIComponent(ctx.params.sha)}`;

export function prefetch(ctx) { get(url(ctx)); }

export default async function (ctx) {
  const { owner, repo } = ctx.params;
  const [meta, d] = await Promise.all([loadRepo(owner, repo), get(url(ctx))]);
  // Statuses posted from outside (CI here shows in the badge above).
  const outside = await get(`${repoUrl(owner, repo)}/checks/${d.commit.sha}`).then((x) => x.checks.filter((k) => k.kind === 'status')).catch(() => []);
  const c = d.commit;
  state.sha = c.sha;
  const base = `/${owner}/${repo}`;
  const sameCommitter = c.committer_name === c.author_name;
  const body = html`<div class="stack">
    <div class="box">
      <div class="commit-head">
        <div class="spread" style="align-items:flex-start">
          <h1>${c.subject}</h1>
          <div class="row"><span class="ci-slot" data-ci-sha="${c.sha}" data-ci-label="1"></span>
          ${meta.can_write && c.parents.length ? html`<button class="btn sm" id="revert" title="Open a pull request that undoes this commit">${icon('history', 'sm')} Revert</button>` : ''}
          <a class="btn sm" href="${base}/tree/${c.sha}">${icon('code', 'sm')} Browse files</a></div>
        </div>
        ${c.body ? html`<pre class="body">${c.body}</pre>` : ''}
      </div>
      <div class="commit-meta">
        ${avatar(c.author_email || c.author_name, 22)}
        <span><b>${c.author_name}</b> authored ${timeEl(c.author_time)}</span>
        ${sameCommitter ? '' : html`<span class="muted">· ${c.committer_name} committed ${timeEl(c.commit_time)}</span>`}
        ${sigBadge(c.verification)}
        <span class="grow"></span>
        <span class="muted small">${c.parents.length === 0 ? 'Root commit' : c.parents.length === 1 ? '1 parent' : `${c.parents.length} parents`}</span>
        ${c.parents.map((p) => html`<a class="sha" href="${base}/commit/${p}">${short(p)}</a>`)}
        <span class="muted small">commit</span>
        <span class="sha" title="${fullDate(c.commit_time)}">${c.sha}</span>
        <button class="btn ghost sm icon" data-copy="${c.sha}" data-copy-label="Commit SHA copied" aria-label="Copy SHA">${icon('copy', 'sm')}</button>
      </div>
    </div>
    ${outside.length ? html`<div class="box"><div class="box-head"><b>Statuses</b></div><div class="list">${outside.map((k) => html`<div class="list-row small">
      ${ciIcon({ success: 'success', failure: 'failure' }[k.state] || 'running')}<b class="mono">${k.name}</b><span class="muted grow ellipsis">${k.description}</span>
      ${k.url ? html`<a href="${safeUrl(k.url)}" rel="noopener">Details</a>` : ''}</div>`)}</div></div>` : ''}
    ${c.parents.length > 1 ? html`<p class="small muted">${icon('merged', 'sm')} Merge commit — showing changes against the first parent.</p>` : ''}
    ${diffView(d.diff, { owner, repo, sha: c.sha, base: c.parents[0], key: `${owner}/${repo}@${c.sha}` })}
  </div>`;
  return {
    title: `${c.subject} · ${owner}/${repo}@${short(c.sha)}`,
    html: repoPage(meta, 'commits', body),
    mount(el) {
      mountRepoHeader(el, meta);
      const rb = $('#revert', el);
      rb?.addEventListener('click', action(rb, async () => {
        if (!(await confirmDialog({ title: `Revert ${short(c.sha)}?`, text: 'This opens a pull request that undoes this commit, on a new branch.', ok: 'Revert' }))) return;
        const r = await send('POST', `${repoUrl(owner, repo)}/revert`, { sha: c.sha });
        toast(`Opened pull request #${r.number} on ${r.branch}`);
        navigate(`${base}/pull/${r.number}`);
      }));
      mountDiff(el, d.diff, { key: `${owner}/${repo}@${c.sha}`, load: (q) => get(`/repos/${owner}/${repo}/commit/${c.sha}?${q}`).then((x) => x.diff.files[0]) });
    },
  };
}
