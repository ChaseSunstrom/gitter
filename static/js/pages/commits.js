// Commit history for a branch (optionally one path), with message/author search.
import { html, state, get, avatar, timeEl, short, encPath, day, debounce, $, sigBadge } from '../lib.js';
import { icon } from '../icons.js';
import { loadRepo, repoUrl, splitRev, repoPage, mountRepoHeader, refPicker, mountRefPicker, crumbs } from '../repohead.js';
import { navigate } from '../main.js';

function apiUrl(meta, rev, path, q) {
  const p = new URLSearchParams({ rev, path, page: q.get('page') || '1' });
  if (q.get('q')) p.set('q', q.get('q'));
  if (q.get('author')) p.set('author', q.get('author'));
  return `${repoUrl(meta.owner, meta.name)}/commits?${p}`;
}

export async function prefetch(ctx) {
  const meta = await get(repoUrl(ctx.params.owner, ctx.params.repo));
  const { rev, path } = splitRev(meta, ctx.params.rest || '');
  get(apiUrl(meta, rev, path, ctx.query));
}

export function commitRow(meta, c, i = 0) {
  const base = `/${meta.owner}/${meta.name}`;
  return html`<div class="commit-row" style="--i:${Math.min(i, 20)}">
    ${avatar(c.author_email || c.author_name, 32)}
    <div class="grow">
      <div class="row" style="align-items:flex-start"><a class="subj" href="${base}/commit/${c.sha}">${c.subject}</a>
        ${c.body ? html`<button class="more" data-more aria-label="Show full message">…</button>` : ''}</div>
      <div class="muted small row wrap-ok"><b style="color:var(--ink-2)">${c.author_name}</b>
        ${c.committer_name !== c.author_name ? html`<span>authored, ${c.committer_name} committed</span>` : html`<span>committed</span>`} ${timeEl(c.commit_time)}</div>
      ${c.body ? html`<pre class="body" hidden>${c.body}</pre>` : ''}
    </div>
    <div class="acts">
      ${sigBadge(c.verification)}
      <span class="ci-slot" data-ci-sha="${c.sha}"></span>
      <div class="btn-group"><button class="btn sm icon" data-copy="${c.sha}" data-copy-label="Commit SHA copied" title="Copy full SHA" aria-label="Copy SHA">${icon('copy', 'sm')}</button>
      <a class="btn sm mono" href="${base}/commit/${c.sha}">${short(c.sha)}</a></div>
      <a class="btn sm icon hide-sm" href="${base}/tree/${c.sha}" title="Browse the repository at this point" aria-label="Browse files">${icon('code', 'sm')}</a>
    </div></div>`;
}

export function groupByDay(meta, commits) {
  const groups = [];
  for (const c of commits) {
    const d = day(c.commit_time);
    if (groups.at(-1)?.d !== d) groups.push({ d, list: [] });
    groups.at(-1).list.push(c);
  }
  let i = 0;
  return html`${groups.map((g) => html`<div class="commit-group"><div class="commit-day">Commits on ${g.d}</div>
    <div class="box list stagger">${g.list.map((c) => commitRow(meta, c, i++))}</div></div>`)}`;
}

export function mountCommitRows(el) {
  el.addEventListener('click', (e) => {
    const b = e.target.closest('[data-more]');
    if (!b) return;
    const body = b.closest('.commit-row').querySelector('.body');
    body.hidden = !body.hidden;
  });
}

export default async function (ctx) {
  const { owner, repo } = ctx.params;
  const meta = await loadRepo(owner, repo);
  if (meta.empty) return { redirect: `/${owner}/${repo}` };
  const { rev, path } = splitRev(meta, ctx.params.rest || '');
  state.rev = rev;
  const q = ctx.query;
  const d = await get(apiUrl(meta, rev, path, q));
  state.sha = d.sha;
  const base = `/${owner}/${repo}`;
  const page = d.page;
  const pageHref = (n) => { const p = new URLSearchParams(q); p.set('page', n); return `${base}/commits/${encPath(rev)}${path ? '/' + encPath(path) : ''}?${p}`; };
  const body = html`<div class="stack">
    <div class="toolbar">
      ${refPicker(meta, rev)}
      ${path ? html`<span class="muted small">History for</span>${crumbs(meta, rev, path, { last: false })}` : html`<h2 style="font-size:17px">Commits</h2>`}
      <span class="grow"></span>
      <div class="filters">
        <div class="search-input">${icon('search')}<input class="input sm" id="cq" placeholder="Search messages" value="${q.get('q') || ''}" aria-label="Search commit messages"></div>
        <input class="input sm" id="ca" placeholder="Author" style="width:140px" value="${q.get('author') || ''}" aria-label="Filter by author">
      </div>
    </div>
    ${d.commits.length ? groupByDay(meta, d.commits) : html`<div class="box empty">${icon('history', 'big')}<h3>No commits found</h3><p>Try a different search.</p></div>`}
    ${page > 1 || d.has_more ? html`<div class="pager">
      ${page > 1 ? html`<a class="btn" href="${pageHref(page - 1)}">${icon('chev-left', 'sm')} Newer</a>` : ''}
      ${d.has_more ? html`<a class="btn" href="${pageHref(page + 1)}">Older ${icon('chev-right', 'sm')}</a>` : ''}</div>` : ''}
  </div>`;
  return {
    title: `Commits · ${owner}/${repo}`,
    html: repoPage(meta, 'commits', body),
    mount(el) {
      mountRepoHeader(el, meta);
      mountRefPicker(el, meta, rev, (r) => `${base}/commits/${encPath(r)}${path ? '/' + encPath(path) : ''}`);
      mountCommitRows(el);
      const apply = debounce(() => {
        const p = new URLSearchParams();
        const qv = $('#cq', el).value.trim();
        const av = $('#ca', el).value.trim();
        if (qv) p.set('q', qv);
        if (av) p.set('author', av);
        navigate(`${base}/commits/${encPath(rev)}${path ? '/' + encPath(path) : ''}${p.toString() ? '?' + p : ''}`, { replace: true, scroll: false });
      }, 350);
      ['#cq', '#ca'].forEach((s) => $(s, el).addEventListener('input', apply));
      if (q.get('q') || q.get('author')) { const f = $(q.get('author') && !q.get('q') ? '#ca' : '#cq', el); f.focus(); f.setSelectionRange(f.value.length, f.value.length); }
    },
  };
}
