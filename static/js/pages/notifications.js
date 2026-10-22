// The notifications inbox: threads you take part in, are mentioned or assigned in, or watch.
import { html, get, send, action, timeEl, toast, toastErr, $ } from '../lib.js';
import { icon } from '../icons.js';
import { refresh, refreshBadge } from '../main.js';

const FILTERS = [['all', 'compass', 'Inbox'], ['unread', 'bell', 'Unread'], ['participating', 'comment', 'Participating'], ['mentions', 'user', 'Mentions'], ['done', 'check', 'Done']];
const REASON = { mention: 'Mentioned', assign: 'Assigned', author: 'Author', comment: 'Commented', subscribed: 'Subscribed', watching: 'Watching', ci: 'CI failed' };

function kindIcon(n) {
  if (n.kind === 'pr') return icon(n.state === 'merged' ? 'merged' : n.state === 'closed' ? 'pr-closed' : 'pr', `sm ${n.state === 'open' ? 'c-green' : 'c-purple'}`);
  if (n.kind === 'issue') return icon(n.state === 'open' ? 'issue' : 'issue-closed', `sm ${n.state === 'open' ? 'c-green' : 'c-purple'}`);
  if (n.kind === 'discussion') return icon('comment', 'sm');
  return icon('zap', 'sm c-red');
}

function href(filter, repo, page) {
  const q = new URLSearchParams();
  if (filter !== 'all') q.set('filter', filter);
  if (repo) q.set('repo', repo);
  if (page > 1) q.set('page', page);
  const s = q.toString();
  return '/notifications' + (s ? '?' + s : '');
}

export default async function (ctx) {
  const filter = FILTERS.some(([k]) => k === ctx.query.get('filter')) ? ctx.query.get('filter') : 'all';
  const repo = ctx.query.get('repo') || '';
  const page = Math.max(1, +ctx.query.get('page') || 1);
  const q = new URLSearchParams({ filter, page });
  if (repo) q.set('repo', repo);
  const data = await get(`/notifications?${q}`, { fresh: true });
  const nav = html`<nav class="settings-nav" aria-label="Notifications">
    ${FILTERS.map(([k, ic, label]) => html`<a href="${href(k, repo, 1)}" class="${filter === k ? 'on' : ''}">${icon(ic, 'sm')}${label}${k === 'unread' && data.unread ? html`<span class="grow"></span><span class="counter">${data.unread}</span>` : ''}</a>`)}
    ${data.repos.length ? html`<div class="menu-head" style="margin-top:12px">Repositories</div>
      ${data.repos.map((r) => html`<a href="${href(filter, r.repo === repo ? '' : r.repo, 1)}" class="${r.repo === repo ? 'on' : ''}">${icon('repo', 'sm')}<span class="grow notif-repo">${r.repo}</span>${r.unread ? html`<span class="counter">${r.unread}</span>` : ''}</a>`)}` : ''}
  </nav>`;
  const rows = data.items.map((n, i) => html`<div class="list-row notif-row ${n.unread ? 'unread' : ''}" style="--i:${Math.min(i, 20)}" data-id="${n.id}">
    <span class="notif-dot" aria-label="${n.unread ? 'Unread' : 'Read'}"></span>
    ${kindIcon(n)}
    <div class="grow notif-main">
      <div class="small muted">${n.repo} ${n.kind === 'ci' ? `run #${n.number}` : `#${n.number}`}</div>
      <a class="notif-title" href="${n.url}" data-open="${n.id}">${n.title || (n.kind === 'ci' ? 'CI run' : 'Untitled')}</a>
    </div>
    <span class="pill small">${REASON[n.reason] || n.reason}</span>
    <span class="small muted hide-sm">${n.actor ? html`${n.actor} · ` : ''}${timeEl(n.updated_at)}</span>
    <div class="row notif-acts">
      ${n.unread ? html`<button class="btn ghost sm icon" data-act="read" title="Mark as read" aria-label="Mark as read">${icon('check', 'sm')}</button>` : ''}
      ${filter !== 'done' ? html`<button class="btn ghost sm icon" data-act="done" title="Done" aria-label="Mark as done">${icon('x', 'sm')}</button>` : ''}
      ${n.kind !== 'ci' ? html`<button class="btn ghost sm icon" data-act="unsubscribe" title="Unsubscribe" aria-label="Unsubscribe from this thread">${icon('bell', 'sm')}</button>` : ''}
    </div>
  </div>`);
  const empty = html`<div class="empty">${icon('bell', 'big')}<h3>${filter === 'done' ? 'Nothing marked done' : 'All caught up'}</h3>
    <p>Notifications show up here when someone mentions you, assigns you, or replies in a thread you take part in or watch.</p></div>`;
  const pager = page > 1 || data.has_more ? html`<div class="row" style="justify-content:center;padding:12px">
    ${page > 1 ? html`<a class="btn sm" href="${href(filter, repo, page - 1)}">Newer</a>` : ''}
    ${data.has_more ? html`<a class="btn sm" href="${href(filter, repo, page + 1)}">Older</a>` : ''}</div>` : '';
  const body = html`<div class="box notif-box">
    <div class="box-head spread"><b>${FILTERS.find(([k]) => k === filter)[2]}${repo ? html` · ${repo}` : ''}</b>
      ${data.unread ? html`<button class="btn sm" id="read-all">${icon('check', 'sm')} Mark all as read</button>` : ''}</div>
    ${data.items.length ? html`<div class="list stagger" id="notif-list">${rows}</div>` : empty}
    ${pager}
  </div>`;
  return {
    title: 'Notifications',
    html: html`<div class="wrap mid settings"><h1 class="page-h" style="margin-bottom:16px">${icon('bell')} Notifications</h1>
      <div class="settings-grid">${nav}<section class="settings-body fade">${body}</section></div></div>`,
    mount(el) {
      refreshBadge(data.unread);
      const all = $('#read-all', el);
      all?.addEventListener('click', action(all, async () => {
        await send('POST', '/notifications/read', repo ? { repo } : {});
        toast('All marked as read');
        refresh();
      }));
      $('#notif-list', el)?.addEventListener('click', async (e) => {
        const open = e.target.closest('[data-open]');
        if (open) {
          // Opening a notification reads it (the link navigates on its own).
          send('POST', `/notifications/${open.dataset.open}/read`).then(() => refreshBadge(undefined, true)).catch(() => {});
          return;
        }
        const b = e.target.closest('[data-act]');
        if (!b) return;
        const id = b.closest('[data-id]').dataset.id;
        try {
          await send('POST', `/notifications/${id}/${b.dataset.act}`);
          if (b.dataset.act === 'unsubscribe') toast('Unsubscribed from this thread');
          refresh();
        } catch (err) { toastErr(err); }
      });
    },
  };
}
