// Branches (with ahead/behind against the default branch) and tags.
import { html, get, send, avatar, timeEl, short, encPath, num, modal, action, confirmDialog, toast, $, $$ } from '../lib.js';
import { icon } from '../icons.js';
import { loadRepo, repoUrl, repoPage, mountRepoHeader } from '../repohead.js';
import { navigate, refresh } from '../main.js';

export function prefetch(ctx) {
  const { owner, repo, view } = ctx.params;
  get(`${repoUrl(owner, repo)}/${view === 'tags' ? 'tags' : 'branches'}`);
}

function abBars(b, max) {
  const w = (n) => (max ? Math.max(n ? 4 : 0, (n / max) * 100) : 0);
  return html`<div class="ab" title="${b.behind} behind, ${b.ahead} ahead of the default branch">
    <div class="n"><span>${num(b.behind)}</span></div><div class="n"><span>${num(b.ahead)}</span></div>
    <div class="bar behind"><i style="width:${w(b.behind)}%"></i></div><div class="bar ahead"><i style="width:${w(b.ahead)}%"></i></div></div>`;
}

function branchRow(meta, b, max) {
  const base = `/${meta.owner}/${meta.name}`;
  return html`<div class="branch-row list-row" data-branch="${b.name}">
    <div class="grow" style="display:grid;gap:4px">
      <div class="row"><a class="branch-name" href="${base}/tree/${encPath(b.name)}">${b.name}</a>
        <button class="btn ghost sm icon" data-copy="${b.name}" data-copy-label="Branch name copied" aria-label="Copy branch name">${icon('copy', 'sm')}</button></div>
      <div class="muted small row ellipsis">${avatar(b.author, 14)} <span class="ellipsis">${b.author} · <a href="${base}/commit/${b.sha}">${b.subject}</a></span></div>
    </div>
    <span class="muted small bmeta">Updated ${timeEl(b.date)}</span>
    ${b.default ? html`<span></span>` : abBars(b, max)}
    <div class="row" style="justify-content:flex-end">
      ${b.default ? html`<span class="tag-private">Default</span>` : ''}
      ${b.pull ? html`<a class="btn sm" href="${base}/pull/${b.pull}">${icon('pr', 'sm c-green')} #${b.pull}</a>` : ''}
      ${!b.default && !b.pull && b.ahead > 0 ? html`<a class="btn sm hide-sm" href="${base}/compare/${encPath(meta.default_branch)}...${encPath(b.name)}">${icon('pr', 'sm')} New pull request</a>` : ''}
      ${meta.can_write && !b.default ? html`<button class="btn sm icon danger" data-delete="${b.name}" aria-label="Delete branch ${b.name}" title="Delete branch">${icon('trash', 'sm')}</button>` : ''}
    </div></div>`;
}

async function branchesView(meta) {
  const d = await get(`${repoUrl(meta.owner, meta.name)}/branches`);
  const def = d.branches.filter((b) => b.default);
  const rest = d.branches.filter((b) => !b.default);
  const max = Math.max(1, ...rest.map((b) => Math.max(b.ahead, b.behind)));
  const body = html`<div class="stack">
    <div class="toolbar"><h2 style="font-size:17px">Branches</h2><span class="counter">${d.branches.length}</span><span class="grow"></span>
      <div class="search-input"><svg class="ico" viewBox="0 0 24 24"><circle cx="11" cy="11" r="7"/><path d="m20 20-3.5-3.5"/></svg><input class="input sm" id="bfilter" placeholder="Filter branches" aria-label="Filter branches"></div>
      ${meta.can_write && !meta.empty ? html`<button class="btn sm primary" id="new-branch">${icon('branch', 'sm')} New branch</button>` : ''}
      <a class="btn sm" href="/${meta.owner}/${meta.name}/tags">${icon('tag', 'sm')} Tags</a></div>
    ${def.length ? html`<div><div class="section-title">Default branch</div><div class="box list">${def.map((b) => branchRow(meta, b, max))}</div></div>` : ''}
    <div><div class="section-title">Other branches</div>
      ${rest.length ? html`<div class="box list stagger" id="blist">${rest.map((b) => branchRow(meta, b, max))}</div>` : html`<div class="box empty">${icon('branch', 'big')}<h3>No other branches</h3><p>Branches let you work on changes without touching ${meta.default_branch}.</p></div>`}
    </div></div>`;
  return {
    title: `Branches · ${meta.owner}/${meta.name}`,
    html: repoPage(meta, 'branches', body),
    mount(el) {
      mountRepoHeader(el, meta);
      $('#bfilter', el)?.addEventListener('input', (e) => {
        const q = e.target.value.toLowerCase();
        $$('[data-branch]', el).forEach((r) => { r.hidden = !r.dataset.branch.toLowerCase().includes(q); });
      });
      el.addEventListener('click', async (e) => {
        const del = e.target.closest('[data-delete]');
        if (!del) return;
        const name = del.dataset.delete;
        if (!(await confirmDialog({ title: `Delete ${name}?`, text: 'Commits only on this branch will no longer be reachable from a branch.', ok: 'Delete branch', danger: true }))) return;
        await action(del, async () => {
          await send('DELETE', `${repoUrl(meta.owner, meta.name)}/branches?name=${encodeURIComponent(name)}`);
          const row = del.closest('.branch-row');
          row.style.transition = 'opacity .25s, transform .25s';
          row.style.opacity = '0'; row.style.transform = 'translateX(12px)';
          toast(`Deleted ${name}`);
          setTimeout(refresh, 250);
        })();
      });
      $('#new-branch', el)?.addEventListener('click', () => modal({
        title: 'Create a branch',
        body: html`<div class="field"><label for="nb-name">Branch name</label><input id="nb-name" class="input mono" placeholder="feature/my-change" autofocus></div>
          <div class="field"><label for="nb-from">Start from</label><select id="nb-from" class="select">${meta.branches.map((b) => html`<option ${b === meta.default_branch ? 'selected' : ''}>${b}</option>`)}${meta.tags.map((t) => html`<option>${t}</option>`)}</select></div>`,
        foot: html`<button class="btn" data-close>Cancel</button><button class="btn primary" id="nb-go">Create branch</button>`,
        onMount(m, close) {
          const go = m.querySelector('#nb-go');
          const submit = action(go, async () => {
            const name = m.querySelector('#nb-name').value.trim();
            await send('POST', `${repoUrl(meta.owner, meta.name)}/branches`, { name, from: m.querySelector('#nb-from').value });
            close(true);
            toast(`Created ${name}`);
            navigate(`/${meta.owner}/${meta.name}/tree/${encPath(name)}`);
          });
          go.addEventListener('click', submit);
          m.querySelector('#nb-name').addEventListener('keydown', (e) => { if (e.key === 'Enter') submit(e); });
        },
      }));
    },
  };
}

async function tagsView(meta) {
  const d = await get(`${repoUrl(meta.owner, meta.name)}/tags`);
  const base = `/${meta.owner}/${meta.name}`;
  const body = html`<div class="stack">
    <div class="toolbar"><h2 style="font-size:17px">Tags</h2><span class="counter">${d.tags.length}</span><span class="grow"></span>
      <a class="btn sm" href="${base}/branches">${icon('branch', 'sm')} Branches</a></div>
    ${d.tags.length ? html`<div class="box list stagger">${d.tags.map((t, i) => html`<div class="list-row" style="--i:${Math.min(i, 20)}">
      ${icon('tag', 'c-accent')}
      <div class="grow" style="display:grid;gap:2px"><a href="${base}/tree/${encPath(t.name)}"><b>${t.name}</b></a>
        <span class="muted small ellipsis">${t.subject}</span></div>
      <span class="muted small hide-sm">${timeEl(t.date)}</span>
      <a class="sha" href="${base}/commit/${t.sha}">${short(t.sha)}</a>
      <a class="btn sm" href="/archive/${meta.owner}/${meta.name}/${encPath(t.name)}.zip" download>${icon('download', 'sm')} zip</a>
      <a class="btn sm hide-sm" href="/archive/${meta.owner}/${meta.name}/${encPath(t.name)}.tar.gz" download>tar.gz</a></div>`)}</div>`
    : html`<div class="box empty">${icon('tag', 'big')}<h3>No tags yet</h3><p>Tag a release with <code>git tag v1.0.0 && git push --tags</code>.</p></div>`}
  </div>`;
  return { title: `Tags · ${meta.owner}/${meta.name}`, html: repoPage(meta, 'branches', body), mount: (el) => mountRepoHeader(el, meta) };
}

export default async function (ctx) {
  const meta = await loadRepo(ctx.params.owner, ctx.params.repo);
  return ctx.params.view === 'tags' ? tagsView(meta) : branchesView(meta);
}
