// Site administration: overview, accounts and instance settings.
import { html, state, get, send, bytes, num, timeEl, avatar, toast, action, confirmDialog, debounce, $, $$ } from '../lib.js';
import { icon } from '../icons.js';
import { navigate, refresh, renderTopbar } from '../main.js';

const TABS = [['', 'pulse', 'Overview'], ['users', 'users', 'Accounts'], ['settings', 'settings', 'Settings']];

function uptime(s) {
  const d = Math.floor(s / 86400), h = Math.floor((s % 86400) / 3600), m = Math.floor((s % 3600) / 60);
  return d ? `${d}d ${h}h` : h ? `${h}h ${m}m` : `${m}m`;
}

function stat(label, value, sub = '') {
  return html`<div class="card admin-stat"><span class="muted small">${label}</span><b class="display">${value}</b>${sub ? html`<span class="muted tiny">${sub}</span>` : ''}</div>`;
}

async function overview() {
  const o = await get('/admin/overview', { fresh: true });
  const used = o.repo_bytes + o.db_bytes;
  const total = o.free_bytes != null ? used + o.free_bytes : null;
  const pct = total ? Math.min(100, (used / total) * 100) : 0;
  return html`<div class="stack">
    <div class="admin-stats stagger">
      ${stat('Accounts', num(o.users), o.suspended ? `${o.suspended} suspended` : '')}
      ${stat('Repositories', num(o.repos), `${num(o.private_repos)} private`)}
      ${stat('Open issues', num(o.open_issues))}
      ${stat('Open pull requests', num(o.open_pulls))}
      ${stat('Activity today', num(o.events_today), 'pushes, issues, comments…')}
      ${stat('Uptime', uptime(o.uptime), `Gitter ${o.version} · git ${o.git_version}`)}
    </div>
    <div class="box"><div class="box-head"><b>Storage</b><span class="grow"></span><span class="muted small mono">${o.data_dir}</span></div>
      <div class="box-body stack">
        <div class="disk-bar" role="img" aria-label="Disk use"><span style="width:${pct}%"></span></div>
        <div class="row wrap-ok small"><span><b>${bytes(o.repo_bytes)}</b> repositories</span><span class="muted">·</span><span><b>${bytes(o.db_bytes)}</b> database</span>
          ${o.free_bytes != null ? html`<span class="muted">·</span><span><b>${bytes(o.free_bytes)}</b> free</span>` : ''}</div>
        <p class="muted small">Back everything up with <code>gitter backup /path/gitter-$(date +%F).tar.gz</code> (in Docker: <code>docker compose exec -T gitter gitter backup - &gt; gitter-$(date +%F).tar.gz</code>), and check one with <code>gitter verify</code>.</p>
      </div></div>
    <div class="box"><div class="box-head"><b>Newest accounts</b><span class="grow"></span><a class="btn sm" href="/admin/users">All accounts</a></div>
      <div class="list">${o.recent_users.map((u) => html`<a class="list-row" href="/${u.username}">${avatar(u.username, 22)}<b>${u.username}</b><span class="grow"></span><span class="muted small">joined ${timeEl(u.created_at)}</span></a>`)}</div></div>
  </div>`;
}

async function users(q, page) {
  const d = await get(`/admin/users?q=${encodeURIComponent(q)}&page=${page}`, { fresh: true });
  return html`<div class="stack">
    <div class="search-input" style="max-width:420px">${icon('search')}<input class="input" id="uq" value="${q}" placeholder="Search by name or email" aria-label="Search accounts"></div>
    <div class="box list">${d.users.length ? d.users.map((u) => html`<div class="list-row admin-user" data-user="${u.username}">
      ${avatar(u.username, 30)}
      <div class="grow" style="display:grid;gap:2px">
        <div class="row"><a href="/${u.username}"><b>${u.username}</b></a>${u.display_name ? html`<span class="muted">${u.display_name}</span>` : ''}
          ${u.is_admin ? html`<span class="tag-private">Admin</span>` : ''}${u.suspended ? html`<span class="pill closed">Suspended</span>` : ''}</div>
        <span class="muted small">${u.email} · ${num(u.repos)} ${u.repos === 1 ? 'repository' : 'repositories'} · joined ${timeEl(u.created_at)}${u.last_active ? html` · active ${timeEl(u.last_active)}` : ''}</span>
      </div>
      ${u.username === state.me.username ? html`<span class="muted small">You</span>` : html`<div class="row">
        <button class="btn sm" data-act="admin">${u.is_admin ? 'Remove admin' : 'Make admin'}</button>
        <button class="btn sm ${u.suspended ? '' : 'danger'}" data-act="suspend">${u.suspended ? 'Restore' : 'Suspend'}</button>
        <button class="btn sm icon danger" data-act="delete" aria-label="Delete ${u.username}">${icon('trash', 'sm')}</button></div>`}
    </div>`) : html`<div class="empty">${icon('users', 'big')}<h3>No accounts match</h3></div>`}</div>
    ${page > 1 || d.has_more ? html`<div class="pager">${page > 1 ? html`<a class="btn" href="/admin/users?page=${page - 1}&q=${encodeURIComponent(q)}">Newer</a>` : ''}${d.has_more ? html`<a class="btn" href="/admin/users?page=${page + 1}&q=${encodeURIComponent(q)}">Older</a>` : ''}</div>` : ''}
  </div>`;
}

async function settings() {
  const s = await get('/admin/settings', { fresh: true });
  return html`<form class="box" id="site-settings"><div class="box-body settings-sec" style="max-width:640px">
    <div class="field"><label for="s-name">Instance name</label><input id="s-name" class="input" value="${s.instance_name}" maxlength="40" required>
      <span class="hint">Shown in the browser tab and the top bar.</span></div>
    <div class="field"><label>Sign-up</label><div class="choice">
      <label><input type="radio" name="signup" value="open" ${s.signup === 'open' ? 'checked' : ''}><span><b>Open</b><br><span class="muted small">Anyone can create an account.</span></span></label>
      <label><input type="radio" name="signup" value="closed" ${s.signup === 'closed' ? 'checked' : ''}><span><b>Closed</b><br><span class="muted small">Only existing accounts can sign in. Good for a private team server.</span></span></label></div></div>
    <div class="row wrap-ok" style="align-items:flex-start;gap:16px">
      <div class="field" style="width:220px"><label for="s-repos">Repositories per person</label><input id="s-repos" class="input" type="number" min="1" value="${s.max_repos_per_user}"><span class="hint">Admins have no limit.</span></div>
      <div class="field" style="width:220px"><label for="s-push">Largest push (MB)</label><input id="s-push" class="input" type="number" min="1" value="${s.max_push_mb}"><span class="hint">Applies to every repository right away.</span></div>
    </div>
    <div class="field"><label for="s-ann">Announcement</label><input id="s-ann" class="input" value="${s.announcement}" maxlength="280" placeholder="e.g. Maintenance on Sunday 10:00 UTC">
      <span class="hint">Shown at the top of every page. Leave empty to hide it.</span></div>
    <div><button class="btn primary" id="s-save">Save settings</button></div>
  </div></form>`;
}

export default async function (ctx) {
  if (!state.me) return { redirect: '/login?next=' + encodeURIComponent(ctx.path) };
  if (!state.me.is_admin) throw Object.assign(new Error('Only administrators can open this page'), { status: 403 });
  const tab = ctx.params.tab || '';
  const q = ctx.query.get('q') || '';
  const page = +(ctx.query.get('page') || 1);
  const inner = tab === 'users' ? await users(q, page) : tab === 'settings' ? await settings() : await overview();
  return {
    title: 'Site administration',
    html: html`<div class="wrap">
      <div class="spread" style="margin-bottom:18px"><h1 style="font-size:26px">Site administration</h1></div>
      <nav class="tabs" style="border-bottom:1px solid var(--line);margin-bottom:22px" aria-label="Administration">
        ${TABS.map(([k, ic, label]) => html`<a href="/admin${k ? '/' + k : ''}" class="${tab === k ? 'on' : ''}">${icon(ic, 'sm')}${label}</a>`)}<span class="bar"></span></nav>
      ${inner}</div>`,
    async mount(el) {
      (await import('../repohead.js')).placeTabBar(el);
      $('#uq', el)?.addEventListener('input', debounce((e) => navigate(`/admin/users?q=${encodeURIComponent(e.target.value.trim())}`, { replace: true, scroll: false }), 300));
      if ($('#uq', el) && q) { const i = $('#uq', el); i.focus(); i.setSelectionRange(i.value.length, i.value.length); }
      el.addEventListener('click', async (e) => {
        const b = e.target.closest('[data-act]');
        if (!b) return;
        const name = b.closest('[data-user]').dataset.user;
        const url = `/admin/users/${encodeURIComponent(name)}`;
        await action(b, async () => {
          if (b.dataset.act === 'admin') {
            const makeAdmin = b.textContent.trim() === 'Make admin';
            if (makeAdmin && !(await confirmDialog({ title: `Make ${name} an admin?`, text: 'Admins can read and change every repository and manage all accounts.', ok: 'Make admin' }))) return;
            await send('PATCH', url, { is_admin: makeAdmin });
          } else if (b.dataset.act === 'suspend') {
            const suspend = b.textContent.trim() === 'Suspend';
            if (suspend && !(await confirmDialog({ title: `Suspend ${name}?`, text: 'They are signed out everywhere and can’t sign in, push or use tokens until restored. Their repositories stay.', ok: 'Suspend', danger: true }))) return;
            await send('PATCH', url, { suspended: suspend });
          } else {
            if (!(await confirmDialog({ title: `Delete ${name}?`, text: 'This deletes the account, every repository it owns, and every issue, pull request and comment it wrote anywhere. It can’t be undone.', ok: 'Delete account', danger: true, typed: name }))) return;
            await send('DELETE', url, { confirm: name });
          }
          toast('Saved');
          refresh();
        })();
      });
      const form = $('#site-settings', el);
      form?.addEventListener('submit', action($('#s-save', el), async () => {
        const body = {
          instance_name: $('#s-name', el).value.trim(),
          signup: $$('input[name=signup]', el).find((r) => r.checked).value,
          max_repos_per_user: +$('#s-repos', el).value,
          max_push_mb: +$('#s-push', el).value,
          announcement: $('#s-ann', el).value.trim(),
        };
        const saved = await send('PUT', '/admin/settings', body);
        state.instance = { name: saved.instance_name, announcement: saved.announcement };
        state.signup = saved.signup === 'open';
        renderTopbar();
        toast('Settings saved');
      }));
    },
  };
}
