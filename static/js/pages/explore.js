// Explore: search and browse public repositories. Also exports the repo card
// and language colors used by the home and profile pages.
import { html, get, timeEl, num, enc, debounce, $ } from '../lib.js';
import { icon } from '../icons.js';
import { navigate } from '../main.js';

const LANG_COLORS = {
  Rust: '#dea584', TypeScript: '#3178c6', JavaScript: '#f1e05a', Python: '#3572a5', Go: '#00add8', Java: '#b07219',
  Kotlin: '#a97bff', Swift: '#f05138', C: '#555555', 'C++': '#f34b7d', 'C#': '#178600', Ruby: '#701516', PHP: '#4f5d95',
  Shell: '#89e051', HTML: '#e34c26', CSS: '#663399', SCSS: '#c6538c', Vue: '#41b883', Svelte: '#ff3e00', Dart: '#00b4ab',
  Elixir: '#6e4a7e', Haskell: '#5e5086', Scala: '#c22d40', Lua: '#000080', Zig: '#ec915c', Nix: '#7e7eff', Dockerfile: '#384d54',
  Makefile: '#427819', SQL: '#e38c00', OCaml: '#ef7a08', Clojure: '#db5855', Erlang: '#b83998', Julia: '#a270ba', R: '#198ce7',
};
export const langColor = (name) => LANG_COLORS[name] || 'var(--faint)';

export function repoCard(r, { owner = true } = {}) {
  return html`<article class="card hov repo-card">
    <div class="row">
      ${icon(r.private ? 'lock' : r.fork_of ? 'fork' : 'repo', 'muted')}
      <a class="repo-card-name ellipsis" href="/${r.owner}/${r.name}">${owner ? html`<span class="muted">${r.owner} / </span>` : ''}<b>${r.name}</b></a>
      ${r.private ? html`<span class="tag-private">Private</span>` : ''}
    </div>
    ${r.fork_of ? html`<div class="tiny muted">Forked from <a href="/${r.fork_of}">${r.fork_of}</a></div>` : ''}
    <p class="repo-card-desc muted small">${r.description || html`<span class="faint">No description</span>`}</p>
    <div class="repo-card-meta small muted">
      ${r.language ? html`<span class="row"><span class="dot" style="background:${langColor(r.language)}"></span>${r.language}</span>` : ''}
      <a class="row" href="/${r.owner}/${r.name}/insights">${icon('star', 'sm')}${num(r.stars)}</a>
      <span class="row">${icon('fork', 'sm')}${num(r.forks)}</span>
      <span class="grow"></span>
      <span class="nowrap">Updated ${timeEl(r.updated_at)}</span>
    </div>
  </article>`;
}

const SORTS = [['updated', 'Recently updated'], ['stars', 'Most stars'], ['new', 'Newest']];

function apiUrl(q, sort, page) {
  return `/explore?q=${enc(q)}&sort=${enc(sort)}&page=${page}`;
}

function parse(ctx) {
  const q = ctx.query.get('q') || '';
  const sort = SORTS.some(([k]) => k === ctx.query.get('sort')) ? ctx.query.get('sort') : 'updated';
  const page = Math.max(1, parseInt(ctx.query.get('page') || '1', 10) || 1);
  return { q, sort, page };
}

export function prefetch(ctx) {
  const { q, sort, page } = parse(ctx);
  get(apiUrl(q, sort, page));
}

function pageHref(q, sort, page) {
  const p = new URLSearchParams();
  if (q) p.set('q', q);
  if (sort !== 'updated') p.set('sort', sort);
  if (page > 1) p.set('page', page);
  const s = p.toString();
  return '/explore' + (s ? '?' + s : '');
}

export default async function (ctx) {
  const { q, sort, page } = parse(ctx);
  const d = await get(apiUrl(q, sort, page));
  return {
    title: q ? `Explore: ${q}` : 'Explore',
    html: html`<div class="wrap mid explore">
      <div class="explore-head">
        <div><h1>Explore</h1><p class="muted">Public repositories on this instance.</p></div>
      </div>
      <div class="explore-bar">
        <div class="search-input grow">${icon('search')}<input class="input" id="ex-q" type="search" placeholder="Search repositories and people" value="${q}" aria-label="Search repositories" autocomplete="off"></div>
        <div class="seg" role="group" aria-label="Sort">${SORTS.map(([k, label]) => html`<a href="${pageHref(q, k, 1)}" class="${k === sort ? 'on' : ''}">${label}</a>`)}</div>
      </div>
      ${d.repos.length
        ? html`<div class="repo-grid stagger">${d.repos.map((r, i) => html`<div style="--i:${Math.min(i, 16)}">${repoCard(r)}</div>`)}</div>`
        : html`<div class="box empty">${icon('compass', 'big')}<h3>${q ? 'Nothing matches that search' : 'No public repositories yet'}</h3>
            <p>${q ? 'Try a shorter or different word.' : 'Be the first to share something.'}</p>${q ? '' : html`<a class="btn primary" href="/new">${icon('plus', 'sm')} New repository</a>`}</div>`}
      ${page > 1 || d.has_more ? html`<div class="pager">
        ${page > 1 ? html`<a class="btn" href="${pageHref(q, sort, page - 1)}">${icon('chev-left', 'sm')} Newer</a>` : html`<span></span>`}
        <span class="muted small">Page ${page}</span>
        ${d.has_more ? html`<a class="btn" href="${pageHref(q, sort, page + 1)}">Older ${icon('chev-right', 'sm')}</a>` : html`<span></span>`}
      </div>` : ''}
    </div>`,
    mount(el) {
      const input = $('#ex-q', el);
      const go = debounce(() => navigate(pageHref(input.value.trim(), sort, 1), { replace: true, scroll: false }), 280);
      input.addEventListener('input', go);
      input.addEventListener('keydown', (e) => { if (e.key === 'Enter') { e.preventDefault(); navigate(pageHref(input.value.trim(), sort, 1), { replace: true, scroll: false }); } });
      if (q) { input.focus(); input.setSelectionRange(q.length, q.length); }
    },
  };
}
