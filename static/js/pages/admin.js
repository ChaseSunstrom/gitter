// Site administration: overview, accounts, security log and instance settings.
import { html, state, get, send, bytes, num, timeEl, avatar, toast, action, confirmDialog, debounce, auditLabel, $, $$ } from '../lib.js';
import { icon } from '../icons.js';
import { navigate, refresh, renderTopbar } from '../main.js';

const TABS = [['', 'pulse', 'Overview'], ['users', 'users', 'Accounts'], ['reports', 'flag', 'Reports'], ['audit', 'shield', 'Security log'], ['settings', 'settings', 'Settings']];

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
          ${u.is_admin ? html`<span class="tag-private">Admin</span>` : ''}${u.suspended ? html`<span class="pill closed">Suspended</span>` : ''}${u.two_factor ? html`<span class="pill" title="Two-factor authentication is on">2FA</span>` : ''}</div>
        <span class="muted small">${u.email} · ${num(u.repos)} ${u.repos === 1 ? 'repository' : 'repositories'} · joined ${timeEl(u.created_at)}${u.last_active ? html` · active ${timeEl(u.last_active)}` : ''}</span>
      </div>
      ${u.username === state.me.username ? html`<span class="muted small">You</span>` : html`<div class="row">
        ${u.two_factor ? html`<button class="btn sm" data-act="reset2fa" title="For someone who lost their phone and recovery codes">Reset 2FA</button>` : ''}
        <button class="btn sm" data-act="admin">${u.is_admin ? 'Remove admin' : 'Make admin'}</button>
        <button class="btn sm ${u.suspended ? '' : 'danger'}" data-act="suspend">${u.suspended ? 'Restore' : 'Suspend'}</button>
        <button class="btn sm icon danger" data-act="delete" aria-label="Delete ${u.username}">${icon('trash', 'sm')}</button></div>`}
    </div>`) : html`<div class="empty">${icon('users', 'big')}<h3>No accounts match</h3></div>`}</div>
    ${page > 1 || d.has_more ? html`<div class="pager">${page > 1 ? html`<a class="btn" href="/admin/users?page=${page - 1}&q=${encodeURIComponent(q)}">Newer</a>` : ''}${d.has_more ? html`<a class="btn" href="/admin/users?page=${page + 1}&q=${encodeURIComponent(q)}">Older</a>` : ''}</div>` : ''}
  </div>`;
}

async function audit(q, page) {
  const d = await get(`/admin/audit?q=${encodeURIComponent(q)}&page=${page}`, { fresh: true });
  const ok = d.integrity.ok;
  return html`<div class="stack">
    <div class="box"><div class="box-body row">${icon(ok ? 'check' : 'x', ok ? 'c-green' : 'c-red')}<span class="grow">${ok
      ? html`<b>The log is intact.</b> <span class="muted small">Each of its ${num(d.integrity.events)} entries is sealed to the one before, so an entry edited or removed later shows up here. The latest is #${d.integrity.head_id}, seal <code>${d.integrity.head}</code>: note it somewhere else to also catch the newest entries being removed.</span>`
      : html`<b>The log was altered.</b> <span class="muted small">Entry #${d.integrity.altered_at} (or one just before it) was edited or removed outside Gitter.</span>`}</span></div></div>
    <div class="search-input" style="max-width:420px">${icon('search')}<input class="input" id="aq" value="${q}" placeholder="Filter by person, action, address…" aria-label="Filter the security log"></div>
    <div class="box list">${d.events.length ? d.events.map((e) => html`<div class="list-row small">
      <span class="grow"><b>${auditLabel(e.action)}</b>${e.detail ? html` <span class="muted">· ${e.detail}</span>` : ''}
        <span class="muted">${e.actor ? html` · by <a href="/${e.actor}">${e.actor}</a>` : ''}${e.subject && e.subject !== e.actor ? html` · account <a href="/${e.subject}">${e.subject}</a>` : ''}</span></span>
      <span class="muted nowrap">${e.ip ? e.ip + ' · ' : ''}${timeEl(e.created_at)}</span></div>`) : html`<div class="empty">${icon('shield', 'big')}<h3>Nothing logged${q ? ' matches' : ' yet'}</h3></div>`}</div>
    ${page > 1 || d.has_more ? html`<div class="pager">${page > 1 ? html`<a class="btn" href="/admin/audit?page=${page - 1}&q=${encodeURIComponent(q)}">Newer</a>` : ''}${d.has_more ? html`<a class="btn" href="/admin/audit?page=${page + 1}&q=${encodeURIComponent(q)}">Older</a>` : ''}</div>` : ''}
  </div>`;
}

const REASON = { spam: 'Spam', abuse: 'Abuse', illegal: 'Illegal', malware: 'Malware', impersonation: 'Impersonation', other: 'Other' };

async function reports(status) {
  const d = await get(`/admin/reports?status=${status}`, { fresh: true });
  const open = status !== 'resolved';
  return html`<div class="stack">
    <div class="row"><div class="seg" role="group" aria-label="Which reports">
      <a class="btn sm ${open ? 'on' : ''}" href="/admin/reports">Open <span class="counter">${d.open}</span></a>
      <a class="btn sm ${open ? '' : 'on'}" href="/admin/reports?status=resolved">Resolved</a></div></div>
    <div class="box list">${d.reports.length ? d.reports.map((r) => html`<div class="list-row report-row" data-report="${r.id}" data-kind="${r.kind}">
      <div class="grow" style="display:grid;gap:4px">
        <div class="row wrap-ok"><span class="pill">${r.kind}</span><span class="pill closed">${REASON[r.reason] || r.reason}</span>
          ${r.target.url ? html`<a href="${r.target.url}"><b>${r.target.label}</b></a>` : html`<b>${r.target.label}</b>`}
          ${r.target.hidden ? html`<span class="pill">${r.kind === 'user' ? 'Suspended' : 'Hidden'}</span>` : ''}</div>
        ${r.target.excerpt ? html`<div class="small muted ellipsis">“${r.target.excerpt}”</div>` : ''}
        ${r.details ? html`<div class="small">${r.details}</div>` : ''}
        <div class="tiny muted">Reported by ${r.reporter || 'a deleted account'} ${timeEl(r.created_at)}${r.target.author ? html` · by ${r.target.author}` : ''}${r.resolution ? html` · ${r.resolution} by ${r.resolved_by || '?'} ${timeEl(r.resolved_at)}` : ''}</div>
      </div>
      <div class="row">${open ? html`
        <button class="btn sm" data-resolve="dismiss">Dismiss</button>
        ${r.kind !== 'user' ? html`<button class="btn sm danger" data-resolve="hide">Hide</button>` : ''}
        <button class="btn sm danger" data-resolve="suspend">Suspend ${r.target.author || 'account'}</button>`
        : r.target.hidden && r.kind !== 'user' ? html`<button class="btn sm" data-resolve="unhide">Unhide</button>` : ''}</div>
    </div>`) : html`<div class="empty">${icon('flag', 'big')}<h3>${open ? 'No open reports' : 'Nothing resolved yet'}</h3><p>When people report repositories, issues, comments or accounts, they show up here.</p></div>`}</div>
  </div>`;
}

async function invitesBox() {
  const d = await get('/admin/invites', { fresh: true });
  return html`<div class="box" id="invites"><div class="box-head"><b>Invite links</b><span class="grow"></span><span class="muted small">For invite-only sign-up</span></div>
    <div class="box-body stack">
      <form class="row wrap-ok" id="inv-form">
        <input class="input" name="note" placeholder="Who it's for (only you see this)" maxlength="120" style="max-width:260px" aria-label="Note">
        <select class="select" name="max_uses" style="max-width:140px" aria-label="Uses"><option value="1">1 person</option><option value="10">10 people</option><option value="100">100 people</option></select>
        <select class="select" name="expires_days" style="max-width:160px" aria-label="Expires"><option value="7">Expires in 7 days</option><option value="30">30 days</option><option value="">Never expires</option></select>
        <button class="btn primary" type="submit">Create link</button>
      </form>
      <div id="inv-new"></div>
      ${d.invites.length ? html`<div class="list">${d.invites.map((i) => html`<div class="list-row small"><span class="grow"><b>${i.note || 'Invite'}</b> · ${i.uses} of ${i.max_uses} used${i.expires_at ? html` · expires ${timeEl(i.expires_at)}` : ''}${i.live ? '' : html` <span class="pill closed">Done</span>`}</span>
        <button class="btn sm" data-inv-del="${i.id}">Delete</button></div>`)}</div>` : ''}
    </div></div>`;
}

async function settings() {
  const s = await get('/admin/settings', { fresh: true });
  return html`<form class="box" id="site-settings"><div class="box-body settings-sec" style="max-width:640px">
    <div class="field"><label for="s-name">Instance name</label><input id="s-name" class="input" value="${s.instance_name}" maxlength="40" required>
      <span class="hint">Shown in the browser tab and the top bar.</span></div>
    <div class="field"><label>Sign-up</label><div class="choice">
      <label><input type="radio" name="signup" value="open" ${s.signup === 'open' ? 'checked' : ''}><span><b>Open</b><br><span class="muted small">Anyone can create an account.</span></span></label>
      <label><input type="radio" name="signup" value="invite" ${s.signup === 'invite' ? 'checked' : ''}><span><b>Invite only</b><br><span class="muted small">People need an invite link from you (make them below).</span></span></label>
      <label><input type="radio" name="signup" value="closed" ${s.signup === 'closed' ? 'checked' : ''}><span><b>Closed</b><br><span class="muted small">Only existing accounts can sign in. Good for a private team server.</span></span></label></div></div>
    <div class="row wrap-ok" style="align-items:flex-start;gap:16px">
      <div class="field" style="width:220px"><label for="s-repos">Repositories per person</label><input id="s-repos" class="input" type="number" min="1" value="${s.max_repos_per_user}"><span class="hint">Admins have no limit.</span></div>
      <div class="field" style="width:220px"><label for="s-push">Largest push (MB)</label><input id="s-push" class="input" type="number" min="1" value="${s.max_push_mb}"><span class="hint">Applies to every repository right away.</span></div>
      <div class="field" style="width:220px"><label for="s-storage">Storage per person (MB)</label><input id="s-storage" class="input" type="number" min="0" value="${s.max_storage_mb}"><span class="hint">0 means no limit. Admins have none.</span></div>
    </div>
    <div class="row wrap-ok" style="align-items:flex-start;gap:16px">
      <div class="field" style="width:220px"><label for="s-api">Requests per minute per address</label><input id="s-api" class="input" type="number" min="0" value="${s.api_rate_per_min}"><span class="hint">Slows down scrapers. 0 means no limit.</span></div>
      <div class="field" style="width:220px"><label for="s-write">Changes per minute per person</label><input id="s-write" class="input" type="number" min="0" value="${s.write_rate_per_min}"><span class="hint">Posts, edits and pushes. Admins have no limit.</span></div>
    </div>
    <div class="field"><label for="s-ann">Announcement</label><input id="s-ann" class="input" value="${s.announcement}" maxlength="280" placeholder="e.g. Maintenance on Sunday 10:00 UTC">
      <span class="hint">Shown at the top of every page. Leave empty to hide it.</span></div>
    <div><button class="btn primary" id="s-save">Save settings</button></div>
  </div></form>
  ${await invitesBox()}`;
}

export default async function (ctx) {
  if (!state.me) return { redirect: '/login?next=' + encodeURIComponent(ctx.path) };
  if (!state.me.is_admin) throw Object.assign(new Error('Only administrators can open this page'), { status: 403 });
  const tab = ctx.params.tab || '';
  const q = ctx.query.get('q') || '';
  const page = +(ctx.query.get('page') || 1);
  const inner = tab === 'users' ? await users(q, page) : tab === 'reports' ? await reports(ctx.query.get('status') || 'open') : tab === 'audit' ? await audit(q, page) : tab === 'settings' ? await settings() : await overview();
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
      $('#aq', el)?.addEventListener('input', debounce((e) => navigate(`/admin/audit?q=${encodeURIComponent(e.target.value.trim())}`, { replace: true, scroll: false }), 300));
      if ($('#aq', el) && q) { const i = $('#aq', el); i.focus(); i.setSelectionRange(i.value.length, i.value.length); }
      el.addEventListener('click', async (e) => {
        const rb = e.target.closest('[data-resolve]');
        if (rb) {
          const act = rb.dataset.resolve, row = rb.closest('[data-report]');
          const words = { dismiss: ['Dismiss this report?', 'Nothing changes; the report is closed.', 'Dismiss'], hide: ['Hide this?', 'Only its owner and admins will see it. Other reports about it are closed too.', 'Hide'],
            suspend: ['Suspend this account?', 'They are signed out everywhere and can’t sign in, push or use tokens until restored.', 'Suspend'], unhide: ['Show this again?', 'Everyone who could see it before can see it again.', 'Unhide'] }[act];
          if (!(await confirmDialog({ title: words[0], text: words[1], ok: words[2], danger: act === 'hide' || act === 'suspend' }))) return;
          await action(rb, async () => { await send('POST', `/admin/reports/${row.dataset.report}/resolve`, { action: act }); toast('Done'); refresh(); })();
          return;
        }
        const del = e.target.closest('[data-inv-del]');
        if (del) {
          await action(del, async () => { await send('DELETE', `/admin/invites/${del.dataset.invDel}`); refresh(); })();
          return;
        }
        const b = e.target.closest('[data-act]');
        if (!b) return;
        const name = b.closest('[data-user]').dataset.user;
        const url = `/admin/users/${encodeURIComponent(name)}`;
        await action(b, async () => {
          if (b.dataset.act === 'reset2fa') {
            if (!(await confirmDialog({ title: `Reset ${name}'s two-factor authentication?`, text: 'Only do this after confirming who they are some other way. They can then sign in with just their password and set it up again.', ok: 'Reset 2FA', danger: true }))) return;
            await send('PATCH', url, { reset_2fa: true });
          } else if (b.dataset.act === 'admin') {
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
      const inv = $('#inv-form', el);
      inv?.addEventListener('submit', action(inv.querySelector('button'), async () => {
        const f = Object.fromEntries(new FormData(inv));
        const r = await send('POST', '/admin/invites', { note: f.note, max_uses: +f.max_uses, expires_days: f.expires_days ? +f.expires_days : null });
        $('#inv-new', el).innerHTML = html`<div class="token-reveal fade"><b>Invite link created</b><p class="small">Send this to the people you're inviting. It won't be shown again.</p>
          <div class="row"><input class="input mono" readonly value="${r.url}" aria-label="Invite link"><button class="btn primary" type="button" data-copy="${r.url}" data-copy-label="Link copied">Copy</button></div></div>`.s;
        inv.reset();
      }));
      const form = $('#site-settings', el);
      form?.addEventListener('submit', action($('#s-save', el), async () => {
        const body = {
          instance_name: $('#s-name', el).value.trim(),
          signup: $$('input[name=signup]', el).find((r) => r.checked).value,
          max_repos_per_user: +$('#s-repos', el).value,
          max_push_mb: +$('#s-push', el).value,
          max_storage_mb: +$('#s-storage', el).value,
          api_rate_per_min: +$('#s-api', el).value,
          write_rate_per_min: +$('#s-write', el).value,
          announcement: $('#s-ann', el).value.trim(),
        };
        const saved = await send('PUT', '/admin/settings', body);
        state.instance = { name: saved.instance_name, announcement: saved.announcement };
        state.signup = saved.signup === 'open';
        state.signupMode = saved.signup;
        renderTopbar();
        toast('Settings saved');
      }));
    },
  };
}
