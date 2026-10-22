// Commit history for a branch (optionally one path), with message/author search.
import { html, raw, state, get, avatar, timeEl, short, encPath, day, debounce, $, sigBadge } from '../lib.js';
import { icon } from '../icons.js';
import { loadRepo, repoUrl, splitRev, repoPage, mountRepoHeader, refPicker, mountRefPicker, crumbs } from '../repohead.js';
import { navigate } from '../main.js';

const isGraph = (q, path) => q.get('graph') === '1' && !path;

function apiUrl(meta, rev, path, q) {
  if (isGraph(q, path)) return `${repoUrl(meta.owner, meta.name)}/graph?page=${encodeURIComponent(q.get('page') || '1')}`;
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

/** The commit's graph cell: lanes as lines through the row, the commit as a dot. */
const LANE_W = 14, MAX_LANES = 12;
function graphCell(c, width) {
  const w = Math.min(Math.max(width, 1), MAX_LANES) * LANE_W;
  const x = (j) => j * LANE_W + LANE_W / 2;
  const line = (j, x1, y1, x2, y2) => `<line class="g${j % 6}" x1="${x1}" y1="${y1}" x2="${x2}" y2="${y2}" vector-effect="non-scaling-stroke"/>`;
  const lines = [
    ...c.pass.map((j) => line(j, x(j), 0, x(j), 100)),
    ...c.up.map((j) => line(j, x(j), 0, x(c.lane), 50)),
    ...c.down.map((j) => line(j, x(c.lane), 50, x(j), 100)),
  ].join('');
  const merge = c.parents.length > 1 ? ' merge' : '';
  return raw(`<span class="graph-cell" style="width:${w}px" aria-hidden="true"><svg viewBox="0 0 ${w} 100" preserveAspectRatio="none">${lines}</svg>${c.lane < MAX_LANES ? `<i class="g${c.lane % 6}${merge}" style="left:${x(c.lane) - 4.5}px"></i>` : ''}</span>`);
}

export function commitRow(meta, c, i = 0, lead = '') {
  const base = `/${meta.owner}/${meta.name}`;
  return html`<div class="commit-row" style="--i:${Math.min(i, 20)}">
    ${lead}${avatar(c.author_email || c.author_name, 32)}
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
  const graph = isGraph(q, path);
  const toggle = path ? '' : html`<div class="seg" role="group" aria-label="Commit view">
    <a href="${base}/commits/${encPath(rev)}" class="${graph ? '' : 'on'}">${icon('history', 'sm')} List</a>
    <a href="${base}/commits?graph=1" class="${graph ? 'on' : ''}" title="Every branch, with merges drawn">${icon('branch', 'sm')} Graph</a></div>`;
  const list = !d.commits.length
    ? html`<div class="box empty">${icon('history', 'big')}<h3>No commits found</h3><p>${graph ? 'No branch has commits yet.' : 'Try a different search.'}</p></div>`
    : graph ? html`<div class="box list commit-graph">${d.commits.map((c, i) => commitRow(meta, c, i, graphCell(c, d.width)))}</div>`
      : groupByDay(meta, d.commits);
  const body = html`<div class="stack">
    <div class="toolbar">
      ${graph ? '' : refPicker(meta, rev)}
      ${path ? html`<span class="muted small">History for</span>${crumbs(meta, rev, path, { last: false })}` : html`<h2 style="font-size:17px">Commits</h2>`}
      ${graph ? html`<span class="muted small">on every branch</span>` : ''}
      <span class="grow"></span>
      ${toggle}
      ${graph ? '' : html`<div class="filters">
        <div class="search-input">${icon('search')}<input class="input sm" id="cq" placeholder="Search messages" value="${q.get('q') || ''}" aria-label="Search commit messages"></div>
        <input class="input sm" id="ca" placeholder="Author" style="width:140px" value="${q.get('author') || ''}" aria-label="Filter by author">
      </div>`}
    </div>
    ${list}
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
      if (graph) return;
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
