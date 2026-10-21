// Repository labels: browse, create and delete.
import { html, get, send, action, confirmDialog, toast, plural, $ } from '../lib.js';
import { icon } from '../icons.js';
import { loadRepo, repoUrl, repoPage, mountRepoHeader } from '../repohead.js';
import { refresh } from '../main.js';
import { labelChip } from './issues.js';

const SWATCHES = ['#e5484d', '#e5932b', '#f5d90a', '#46a758', '#0e9384', '#3e9ef5', '#6e56cf', '#d6409f', '#8d8d8d'];

export default async function (ctx) {
  const { owner, repo } = ctx.params;
  const [meta, data] = await Promise.all([loadRepo(owner, repo), get(`${repoUrl(owner, repo)}/labels`)]);
  const base = `/${owner}/${repo}`;
  const start = SWATCHES[data.labels.length % SWATCHES.length];
  const form = meta.can_write ? html`<form class="box label-form" id="label-form">
    <div class="box-body row wrap-ok">
      <div class="field grow"><label for="label-name">Name</label><input class="input" id="label-name" name="name" maxlength="50" placeholder="e.g. needs design" required autocomplete="off"></div>
      <div class="field"><label for="label-color">Color</label>
        <div class="row"><input type="color" id="label-color" name="color" value="${start}" class="color-input">
        <div class="swatches">${SWATCHES.map((c) => html`<button type="button" class="swatch" style="background:${c}" data-c="${c}" aria-label="Use ${c}"></button>`)}</div></div></div>
      <div class="field"><span class="label">Preview</span><span id="label-preview">${labelChip({ name: 'label name', color: start })}</span></div>
      <button class="btn primary" type="submit" style="align-self:flex-end">${icon('plus', 'sm')} Create label</button>
    </div></form>` : '';
  const rows = data.labels.map((l, i) => html`<div class="list-row" style="--i:${Math.min(i, 20)}">
    <span class="label-col">${labelChip(l, `${base}/issues?label=${encodeURIComponent(l.name)}`)}</span>
    <a class="muted small grow" href="${base}/issues?label=${encodeURIComponent(l.name)}">${plural(l.open, 'open issue')}</a>
    ${meta.can_write ? html`<button class="btn ghost sm icon danger" data-del="${l.id}" data-name="${l.name}" aria-label="Delete ${l.name}">${icon('trash', 'sm')}</button>` : ''}
  </div>`);
  const body = html`<div class="stack mid" style="margin:0 auto">
    <div class="spread"><h2 class="page-h">${icon('label')} Labels</h2><a class="btn" href="${base}/issues">${icon('issue', 'sm')} Issues</a></div>
    ${form}
    <div class="box"><div class="box-head"><b>${plural(data.labels.length, 'label')}</b></div>
      ${data.labels.length ? html`<div class="list stagger">${rows}</div>` : html`<div class="empty">${icon('label', 'big')}<h3>No labels yet</h3><p>Labels group issues and pull requests by type, area or priority.</p></div>`}</div>
  </div>`;
  return {
    title: `Labels · ${owner}/${repo}`,
    html: repoPage(meta, 'issues', body),
    mount(el) {
      mountRepoHeader(el, meta);
      const f = $('#label-form', el);
      if (f) {
        const paint = () => { $('#label-preview', el).innerHTML = labelChip({ name: f.name.value.trim() || 'label name', color: f.color.value }).s; };
        f.addEventListener('input', paint);
        f.addEventListener('click', (e) => { const s = e.target.closest('[data-c]'); if (s) { f.color.value = s.dataset.c; paint(); } });
        const btn = f.querySelector('[type=submit]');
        f.addEventListener('submit', action(btn, async () => {
          await send('POST', `${repoUrl(owner, repo)}/labels`, { name: f.name.value.trim(), color: f.color.value });
          toast('Label created');
          refresh();
        }));
      }
      el.addEventListener('click', async (e) => {
        const b = e.target.closest('[data-del]');
        if (!b) return;
        const ok = await confirmDialog({ title: `Delete “${b.dataset.name}”?`, text: 'It will be removed from every issue and pull request that uses it.', ok: 'Delete label', danger: true });
        if (!ok) return;
        await action(b, async () => { await send('DELETE', `${repoUrl(owner, repo)}/labels/${b.dataset.del}`); toast('Label deleted'); refresh(); })();
      });
    },
  };
}
