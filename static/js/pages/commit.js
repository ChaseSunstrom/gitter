// One commit: message, authorship, parents and the full diff.
import { html, state, get, avatar, timeEl, short, fullDate, sigBadge } from '../lib.js';
import { icon } from '../icons.js';
import { loadRepo, repoUrl, repoPage, mountRepoHeader } from '../repohead.js';
import { diffView, mountDiff } from '../diffview.js';

const url = (ctx) => `${repoUrl(ctx.params.owner, ctx.params.repo)}/commit/${encodeURIComponent(ctx.params.sha)}`;

export function prefetch(ctx) { get(url(ctx)); }

export default async function (ctx) {
  const { owner, repo } = ctx.params;
  const [meta, d] = await Promise.all([loadRepo(owner, repo), get(url(ctx))]);
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
    ${c.parents.length > 1 ? html`<p class="small muted">${icon('merged', 'sm')} Merge commit — showing changes against the first parent.</p>` : ''}
    ${diffView(d.diff, { owner, repo, sha: c.sha, key: `${owner}/${repo}@${c.sha}` })}
  </div>`;
  return {
    title: `${c.subject} · ${owner}/${repo}@${short(c.sha)}`,
    html: repoPage(meta, 'commits', body),
    mount(el) {
      mountRepoHeader(el, meta);
      mountDiff(el, d.diff, { key: `${owner}/${repo}@${c.sha}`, load: (q) => get(`/repos/${owner}/${repo}/commit/${c.sha}?${q}`).then((x) => x.diff.files[0]) });
    },
  };
}
