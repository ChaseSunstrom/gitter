// Search code in one repository (git grep on a branch).
import { html, raw, esc, state, get, encPath, enc, debounce, num, $ } from '../lib.js';
import { icon } from '../icons.js';
import { loadRepo, repoUrl, repoPage, mountRepoHeader, refPicker, mountRefPicker } from '../repohead.js';
import { navigate } from '../main.js';

function markHits(text, q) {
  if (!q) return esc(text);
  const lower = text.toLowerCase();
  const ql = q.toLowerCase();
  let out = '';
  let i = 0;
  for (let j = lower.indexOf(ql); j >= 0; j = lower.indexOf(ql, i)) {
    out += esc(text.slice(i, j)) + '<mark>' + esc(text.slice(j, j + q.length)) + '</mark>';
    i = j + q.length;
  }
  return out + esc(text.slice(i));
}

export default async function (ctx) {
  const meta = await loadRepo(ctx.params.owner, ctx.params.repo);
  const q = (ctx.query.get('q') || '').trim();
  const rev = ctx.query.get('rev') || meta.default_branch;
  state.rev = rev;
  const base = `/${meta.owner}/${meta.name}`;
  const d = q && !meta.empty ? await get(`${repoUrl(meta.owner, meta.name)}/grep?q=${enc(q)}&rev=${enc(rev)}`) : { results: [] };
  const total = d.results.reduce((a, r) => a + r.matches.length, 0);
  const body = html`<div class="stack">
    <div class="toolbar">${refPicker(meta, rev)}
      <div class="search-input grow" style="max-width:560px">${icon('search')}<input class="input" id="sq" value="${q}" placeholder="Search code in ${meta.name}" aria-label="Search code" autofocus></div>
      ${q ? html`<span class="muted small">${num(total)} ${total === 1 ? 'match' : 'matches'} in ${num(d.results.length)} ${d.results.length === 1 ? 'file' : 'files'}${d.truncated ? ' (first results only)' : ''}</span>` : ''}
    </div>
    ${!q ? html`<div class="box empty">${icon('search', 'big')}<h3>Search this repository</h3><p>Finds text in every file on <b>${rev}</b>. Case doesn't matter.</p></div>`
      : d.results.length ? html`<div class="stack stagger">${d.results.map((r, i) => html`<div class="box search-hit" style="--i:${Math.min(i, 20)}">
          <div class="box-head">${icon('file', 'sm muted')}<a href="${base}/blob/${encPath(rev)}/${encPath(r.path)}"><b>${r.path}</b></a><span class="grow"></span><span class="muted small">${r.matches.length}</span></div>
          <div class="lines">${r.matches.map((m) => html`<div class="line"><a href="${base}/blob/${encPath(rev)}/${encPath(r.path)}#L${m.line}">${m.line}</a><code>${raw(markHits(m.text, q))}</code></div>`)}</div>
        </div>`)}</div>`
      : html`<div class="box empty">${icon('search', 'big')}<h3>No results for “${q}”</h3><p>Try a shorter or different phrase.</p></div>`}
  </div>`;
  return {
    title: q ? `“${q}” · ${meta.owner}/${meta.name}` : `Search · ${meta.owner}/${meta.name}`,
    html: repoPage(meta, 'code', body),
    mount(el) {
      mountRepoHeader(el, meta);
      mountRefPicker(el, meta, rev, (r) => `${base}/search?rev=${enc(r)}${q ? '&q=' + enc(q) : ''}`);
      const input = $('#sq', el);
      input.focus();
      input.setSelectionRange(input.value.length, input.value.length);
      const go = debounce(() => {
        const v = input.value.trim();
        navigate(`${base}/search?${new URLSearchParams({ ...(v ? { q: v } : {}), ...(rev !== meta.default_branch ? { rev } : {}) })}`, { replace: true, scroll: false });
      }, 300);
      input.addEventListener('input', go);
    },
  };
}
