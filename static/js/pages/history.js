// One file's history: every version through renames, a scrubber to step through
// them, and what each version changed.
import { html, state, get, avatar, timeEl, short, encPath, enc, sigBadge, toastErr, $ } from '../lib.js';
import { icon } from '../icons.js';
import { loadRepo, repoUrl, splitRev, repoPage, mountRepoHeader, crumbs } from '../repohead.js';
import { diffView, mountDiff } from '../diffview.js';

const WORD = { added: 'Added', modified: 'Changed', deleted: 'Deleted', renamed: 'Renamed', copied: 'Copied' };

function versionHead(meta, v, i, n) {
  const base = `/${meta.owner}/${meta.name}`;
  const c = v.commit;
  return html`<div class="box-head version-head">
    ${avatar(c.author_email || c.author_name, 22)}
    <div class="grow"><a class="subj" href="${base}/commit/${c.sha}"><b>${c.subject}</b></a>
      <div class="muted small">${WORD[v.status] || 'Changed'}${v.old_path ? html` from <span class="mono">${v.old_path}</span>` : ''} · ${c.author_name} ${timeEl(c.commit_time)} · version ${n - i} of ${n}</div></div>
    ${sigBadge(c.verification)}
    ${v.status !== 'deleted' ? html`<a class="btn sm" href="${base}/blob/${c.sha}/${encPath(v.path)}">${icon('file', 'sm')} View file</a>` : ''}
    <a class="btn sm mono" href="${base}/commit/${c.sha}">${short(c.sha)}</a></div>`;
}

export default async function (ctx) {
  const { owner, repo } = ctx.params;
  const meta = await loadRepo(owner, repo);
  if (meta.empty) return { redirect: `/${owner}/${repo}` };
  const { rev, path } = splitRev(meta, ctx.params.rest || '');
  if (!path) return { redirect: `/${owner}/${repo}/commits/${encPath(rev)}` };
  state.rev = rev;
  const page = Math.max(1, Math.floor(Number(ctx.query.get('page'))) || 1);
  const api = repoUrl(owner, repo);
  const d = await get(`${api}/history?rev=${enc(rev)}&path=${enc(path)}&page=${page}`);
  const vs = d.versions;
  const n = vs.length;
  const base = `/${owner}/${repo}`;
  const pageHref = (p) => `${base}/history/${encPath(rev)}/${encPath(path)}?page=${p}`;
  const body = html`<div class="stack">
    <div class="toolbar"><span class="muted small">History for</span>${crumbs(meta, rev, path, { last: false })}<span class="grow"></span>
      <a class="btn sm" href="${base}/commits/${encPath(rev)}/${encPath(path)}">${icon('history', 'sm')} Commit list</a></div>
    ${n ? html`<div class="box scrubber">
      <div class="box-body row">
        <button class="btn sm icon" id="v-older" aria-label="Older version">${icon('chev-left', 'sm')}</button>
        <input type="range" id="v-range" min="0" max="${n - 1}" value="${n - 1}" step="1" aria-label="Version (left is oldest)" class="grow">
        <button class="btn sm icon" id="v-newer" aria-label="Newer version">${icon('chev-right', 'sm')}</button>
        <span class="muted small nowrap" id="v-label"></span>
      </div>
      <ol class="version-list">${vs.map((v, i) => html`<li><button class="linkish" data-v="${i}">
        <span class="st-dot ${v.status}"></span><span class="ellipsis grow">${v.commit.subject}</span>
        <span class="muted tiny nowrap">${v.path !== path ? html`<span class="mono">${v.path}</span> · ` : ''}${timeEl(v.commit.commit_time)}</span></button></li>`)}</ol>
    </div>
    <div id="version-view"></div>`
    : html`<div class="box empty">${icon('history', 'big')}<h3>No history for this file</h3><p>No commit on ${rev} touched ${path}.</p></div>`}
    ${page > 1 || d.has_more ? html`<div class="pager">
      ${page > 1 ? html`<a class="btn" href="${pageHref(page - 1)}">${icon('chev-left', 'sm')} Newer</a>` : ''}
      ${d.has_more ? html`<a class="btn" href="${pageHref(page + 1)}">Older ${icon('chev-right', 'sm')}</a>` : ''}</div>` : ''}
  </div>`;
  return {
    title: `History of ${path} · ${owner}/${repo}`,
    html: repoPage(meta, 'code', body),
    mount(el) {
      mountRepoHeader(el, meta);
      if (!n) return;
      const range = $('#v-range', el), view = $('#version-view', el), label = $('#v-label', el);
      let seq = 0;
      // The slider runs oldest (left) to newest (right); versions come newest first.
      const show = async (i) => {
        i = Math.max(0, Math.min(n - 1, i));
        range.value = String(n - 1 - i);
        label.textContent = `${n - i} of ${n}`;
        el.querySelectorAll('[data-v]').forEach((b) => b.classList.toggle('on', Number(b.dataset.v) === i));
        const v = vs[i], my = ++seq;
        const q = new URLSearchParams({ path: v.path });
        if (v.old_path) q.set('old', v.old_path);
        const box = document.createElement('div');
        box.className = 'stack';
        const head = `<div class="box">${versionHead(meta, v, i, n)}</div>`;
        box.innerHTML = `${head}<span class="spin"></span>`;
        view.replaceChildren(box);
        try {
          const c = await get(`${api}/commit/${v.commit.sha}?${q}`);
          if (my !== seq) return;
          const key = `${owner}/${repo}@${v.commit.sha}`;
          box.innerHTML = `${head}${diffView(c.diff, { owner, repo, sha: v.commit.sha, key })}`;
          mountDiff(box, c.diff, { key, load: (qs) => get(`${api}/commit/${v.commit.sha}?${qs}`).then((x) => x.diff.files[0]) });
        } catch (e) { if (my === seq) toastErr(e); }
      };
      const at = () => n - 1 - Number(range.value);
      range.addEventListener('input', () => show(at()));
      $('#v-older', el).addEventListener('click', () => show(at() + 1));
      $('#v-newer', el).addEventListener('click', () => show(at() - 1));
      el.addEventListener('click', (e) => { const b = e.target.closest('[data-v]'); if (b) show(Number(b.dataset.v)); });
      show(0);
    },
  };
}
