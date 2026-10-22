// Milestones: the list with progress, one milestone's issues and pull requests, and the form.
import { html, get, send, action, confirmDialog, toast, plural, timeEl, enc, $ } from '../lib.js';
import { icon } from '../icons.js';
import { loadRepo, repoUrl, repoPage, mountRepoHeader } from '../repohead.js';
import { navigate, refresh } from '../main.js';
import { stateIcon } from './issues.js';

const pct = (m) => (m.open + m.closed ? Math.round((m.closed / (m.open + m.closed)) * 100) : 0);
const overdue = (m) => m.state === 'open' && m.due_on && m.due_on < new Date().toISOString().slice(0, 10);
const dueText = (m) => m.due_on
  ? html`<span class="${overdue(m) ? 'c-red' : ''}">${icon('clock', 'sm inline')} ${overdue(m) ? 'Past due by' : 'Due by'} ${new Date(m.due_on + 'T00:00:00').toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' })}</span>`
  : html`<span>No due date</span>`;

export function progress(m) {
  return html`<div class="ms-progress"><progress max="100" value="${pct(m)}" aria-label="${pct(m)}% complete"></progress>
    <span class="small muted"><b>${pct(m)}%</b> complete · ${m.open} open · ${m.closed} closed</span></div>`;
}

export default async function (ctx) {
  const { owner, repo, view } = ctx.params;
  if (view === 'edit') return edit(ctx);
  if (view === 'show') return show(ctx);
  const closed = ctx.query.get('state') === 'closed';
  const api = repoUrl(owner, repo);
  const [meta, data] = await Promise.all([loadRepo(owner, repo), get(`${api}/milestones?state=${closed ? 'closed' : 'open'}`)]);
  const base = `/${owner}/${repo}`;
  const rows = data.milestones.map((m, i) => html`<div class="list-row ms-row" style="--i:${Math.min(i, 20)}">
    <div class="grow" style="min-width:0">
      <a class="issue-title" href="${base}/milestone/${m.number}">${m.title}</a>
      <div class="muted small row wrap-ok">${m.state === 'closed' ? html`<span>Closed ${timeEl(m.closed_at)}</span>` : dueText(m)}</div>
      ${m.description ? html`<p class="small ms-desc">${m.description}</p>` : ''}
    </div>
    <div class="ms-side">${progress(m)}
      ${meta.can_write ? html`<div class="row small">
        <a class="c-accent" href="${base}/milestone/${m.number}/edit">Edit</a>
        <button class="btn ghost sm" data-toggle="${m.number}" data-to="${m.state === 'open' ? 'closed' : 'open'}">${m.state === 'open' ? 'Close' : 'Reopen'}</button>
        <button class="btn ghost sm danger" data-del="${m.number}" data-title="${m.title}">Delete</button></div>` : ''}
    </div>
  </div>`);
  const body = html`<div class="stack">
    <div class="spread"><h2 class="page-h">${icon('flag')} Milestones</h2>
      <div class="row"><a class="btn" href="${base}/issues">${icon('issue', 'sm')} Issues</a>
      ${meta.can_write ? html`<a class="btn primary" href="${base}/milestones/new">${icon('plus', 'sm')} New milestone</a>` : ''}</div></div>
    <div class="box">
      <div class="box-head">
        <a class="state-tab ${closed ? '' : 'on'}" href="${base}/milestones">${icon('flag', 'sm')} ${data.counts.open} Open</a>
        <a class="state-tab ${closed ? 'on' : ''}" href="${base}/milestones?state=closed">${icon('check', 'sm')} ${data.counts.closed} Closed</a>
      </div>
      ${rows.length ? html`<div class="list stagger">${rows}</div>` : html`<div class="empty">${icon('flag', 'big')}<h3>${closed ? 'No closed milestones' : 'No open milestones'}</h3>
        <p>Milestones group issues and pull requests toward a release or a goal, and show how far along it is.</p></div>`}
    </div></div>`;
  return {
    title: `Milestones · ${owner}/${repo}`,
    html: repoPage(meta, 'issues', body),
    mount(el) {
      mountRepoHeader(el, meta);
      el.addEventListener('click', async (e) => {
        const t = e.target.closest('[data-toggle]');
        if (t) return action(t, async () => { await send('PATCH', `${api}/milestones/${t.dataset.toggle}`, { state: t.dataset.to }); refresh(); })();
        const d = e.target.closest('[data-del]');
        if (!d) return;
        const ok = await confirmDialog({ title: `Delete “${d.dataset.title}”?`, text: 'Its issues and pull requests stay, without a milestone.', ok: 'Delete milestone', danger: true });
        if (ok) await action(d, async () => { await send('DELETE', `${api}/milestones/${d.dataset.del}`); toast('Milestone deleted'); refresh(); })();
      });
    },
  };
}

async function show(ctx) {
  const { owner, repo, n } = ctx.params;
  const api = repoUrl(owner, repo);
  const meta = await loadRepo(owner, repo);
  const m = await get(`${api}/milestones/${enc(n)}`);
  const list = (kind) => get(`${api}/issues?kind=${kind}&state=all&milestone=${enc(m.title)}`);
  const [issues, pulls] = await Promise.all([list('issue'), list('pr')]);
  const base = `/${owner}/${repo}`;
  const items = [...issues.items, ...pulls.items].sort((a, b) => (b.state === 'open') - (a.state === 'open') || b.number - a.number);
  const more = issues.has_more || pulls.has_more;
  const body = html`<div class="stack">
    <div class="spread"><div><h2 class="page-h">${m.title}</h2>
      <div class="muted small row wrap-ok">${m.state === 'closed' ? html`<span class="pill merged small">Closed</span>` : dueText(m)}</div></div>
      <div class="row"><a class="btn" href="${base}/milestones">${icon('flag', 'sm')} Milestones</a>
      ${meta.can_write ? html`<a class="btn" href="${base}/milestone/${m.number}/edit">${icon('edit', 'sm')} Edit</a>` : ''}</div></div>
    ${progress(m)}
    ${m.description ? html`<p>${m.description}</p>` : ''}
    <div class="box">
      ${items.length ? html`<div class="list stagger">${items.map((it, i) => html`<div class="list-row issue-row" style="--i:${Math.min(i, 20)}">
        ${stateIcon(it)}<div class="grow" style="min-width:0"><a class="issue-title" href="${base}/${it.is_pr ? 'pull' : 'issues'}/${it.number}">${it.title}</a>
        <div class="muted small">#${it.number} by ${it.author}${it.assignees.length ? html` · assigned to ${it.assignees.join(', ')}` : ''}</div></div></div>`)}</div>`
        : html`<div class="empty">${icon('issue', 'big')}<h3>Nothing here yet</h3><p>Set this milestone from an issue's or pull request's sidebar.</p></div>`}
      ${more ? html`<div class="box-body small">Showing the latest 30 of each: see its <a class="c-accent" href="${base}/issues?milestone=${enc(m.title)}">issues</a> and <a class="c-accent" href="${base}/pulls?milestone=${enc(m.title)}">pull requests</a></div>` : ''}
    </div></div>`;
  return { title: `${m.title} · Milestones · ${owner}/${repo}`, html: repoPage(meta, 'issues', body), mount(el) { mountRepoHeader(el, meta); } };
}

async function edit(ctx) {
  const { owner, repo, n } = ctx.params;
  const api = repoUrl(owner, repo);
  const meta = await loadRepo(owner, repo);
  const m = n ? await get(`${api}/milestones/${enc(n)}`) : { title: '', description: '', due_on: '' };
  const base = `/${owner}/${repo}`;
  const body = html`<form class="box mid" style="margin:0 auto" id="ms-form"><div class="box-body stack">
    <h2 class="page-h">${n ? 'Edit milestone' : 'New milestone'}</h2>
    <div class="field"><label for="ms-title">Title</label><input class="input" id="ms-title" name="title" value="${m.title}" maxlength="100" required autocomplete="off" placeholder="e.g. v1.0"></div>
    <div class="field"><label for="ms-due">Due date (optional)</label><input class="input" type="date" id="ms-due" name="due_on" value="${m.due_on || ''}"></div>
    <div class="field"><label for="ms-desc">Description (optional)</label><textarea class="textarea" id="ms-desc" name="description" rows="4" maxlength="10000">${m.description}</textarea></div>
    <div class="row" style="justify-content:flex-end"><a class="btn" href="${n ? `${base}/milestone/${m.number}` : `${base}/milestones`}">Cancel</a>
      <button class="btn primary" type="submit">${n ? 'Save changes' : 'Create milestone'}</button></div>
  </div></form>`;
  return {
    title: `${n ? 'Edit milestone' : 'New milestone'} · ${owner}/${repo}`,
    html: repoPage(meta, 'issues', body),
    mount(el) {
      mountRepoHeader(el, meta);
      const f = $('#ms-form', el);
      f.addEventListener('submit', action(f.querySelector('[type=submit]'), async () => {
        const data = { title: f.title.value.trim(), due_on: f.due_on.value, description: f.description.value };
        const r = n ? await send('PATCH', `${api}/milestones/${enc(n)}`, data) : await send('POST', `${api}/milestones`, data);
        navigate(`${base}/milestone/${n || r.number}`, { replace: true });
      }));
    },
  };
}
