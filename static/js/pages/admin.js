// Site administration: overview, accounts, security log and instance settings.
import { html, state, get, send, bytes, num, timeEl, avatar, toast, action, confirmDialog, debounce, auditLabel, plural, modal, $, $$ } from '../lib.js';
import { icon } from '../icons.js';
import { navigate, refresh, renderTopbar } from '../main.js';

const TABS = [['', 'pulse', 'Overview'], ['users', 'users', 'Accounts'], ['reports', 'flag', 'Reports'], ['audit', 'shield', 'Security log'], ['runners', 'zap', 'CI runners'], ['sign-in', 'key', 'Sign-in'], ['settings', 'settings', 'Settings']];

function uptime(s) {
  const d = Math.floor(s / 86400), h = Math.floor((s % 86400) / 3600), m = Math.floor((s % 3600) / 60);
  return d ? `${d}d ${h}h` : h ? `${h}h ${m}m` : `${m}m`;
}

function stat(label, value, sub = '') {
  return html`<div class="card admin-stat"><span class="muted small">${label}</span><b class="display">${value}</b>${sub ? html`<span class="muted tiny">${sub}</span>` : ''}</div>`;
}

async function overview() {
  const [o, trash] = await Promise.all([get('/admin/overview', { fresh: true }), get('/admin/trash', { fresh: true })]);
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
    ${o.failing_mirrors.length ? html`<div class="box" id="failing-mirrors"><div class="box-head"><b>Failing mirrors</b><span class="grow"></span><span class="muted small">Retried with longer waits, up to once a day</span></div>
      <div class="list">${o.failing_mirrors.map((m) => html`<a class="list-row" href="/${m.owner}/${m.name}/settings/mirrors">${icon(m.direction === 'pull' ? 'download' : 'upload', 'muted')}<span class="grow" style="min-width:0"><b>${m.owner}/${m.name}</b>
        <span class="muted small"> · ${m.direction === 'pull' ? 'from' : 'to'} <span class="mono">${m.url}</span> · failed ${m.failures} times${m.last_tried_at ? html`, last ${timeEl(m.last_tried_at)}` : ''}</span>
        <div class="small c-red ellipsis">${m.last_error}</div></span></a>`)}</div></div>` : ''}
    ${trash.repos.length ? html`<div class="box" id="trash"><div class="box-head"><b>Recently deleted repositories</b><span class="grow"></span></div>
      <div class="list">${trash.repos.map((r) => html`<div class="list-row">${icon('trash', 'muted')}<span class="grow"><b>${r.owner}/${r.name}</b>
        <span class="muted small"> · deleted ${timeEl(r.deleted_at)} · gone for good ${timeEl(r.purge_at)}</span></span>
        <button class="btn sm danger" data-purge="${r.id}" data-name="${r.owner}/${r.name}">Delete now</button></div>`)}</div></div>` : ''}
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

async function runners() {
  const d = await get('/admin/runners', { fresh: true });
  const seen = (r) => r.builtin ? 'inside the server' : r.last_seen ? html`last seen ${timeEl(r.last_seen)}` : 'never connected';
  return html`<div class="stack">
    <div class="box" id="runners"><div class="box-head"><b>Runners</b><span class="grow"></span><span class="muted small">${d.queued} job(s) waiting</span></div>
      <div class="list">${d.runners.map((r) => html`<div class="list-row">${icon(r.builtin ? 'zap' : 'monitor', 'sm muted')}
        <div class="grow"><b>${r.name}</b> ${r.running ? html`<span class="pill">${r.running} running</span>` : ''}
          <div class="small muted row wrap-ok"><span>${seen(r)}</span><span class="runner-labels">${r.labels.map((l) => html`<span class="pill mono">${l}</span>`)}</span></div></div>
        ${r.builtin ? html`<a class="btn sm ghost" href="/admin/settings">Settings</a>` : html`<button class="btn sm danger" data-runner-del="${r.id}" data-name="${r.name}">Remove</button>`}</div>`)}</div></div>
    <div class="box"><div class="box-head"><b>Add a runner</b></div><div class="box-body stack">
      <p class="muted small">Run CI jobs on another machine: make a token here, then start <code>gitter runner</code> there with it. Jobs that name <code>runs-on</code> labels go to runners that have them all.</p>
      <form class="row wrap-ok" id="runner-form"><input class="input" name="name" placeholder="Machine name, e.g. build-box" maxlength="60" required style="max-width:280px" aria-label="Runner name">
        <button class="btn primary" type="submit">Create token</button></form>
      <div id="runner-new"></div>
    </div></div>
  </div>`;
}

const KIND = { github: 'GitHub', gitlab: 'GitLab', oidc: 'OpenID Connect' };
const ADDRESS = { github: 'https://github.com', gitlab: 'https://gitlab.com', oidc: 'https://id.example.com' };

/** The fields of a provider; `p` when editing (the secret stays unless a new one is typed). */
function providerFields(p, id = 'pv') {
  const kind = p?.kind || 'github';
  return html`<div class="grid-2">
      ${p ? '' : html`<div class="field"><label for="${id}-kind">Kind</label><select class="select" id="${id}-kind" name="kind">
        ${Object.entries(KIND).map(([k, v]) => html`<option value="${k}">${v}</option>`)}</select></div>`}
      <div class="field"><label for="${id}-name">Button name</label><input class="input" id="${id}-name" name="name" maxlength="60" value="${p?.name || ''}" placeholder="${KIND[kind]}"></div>
    </div>
    <div class="field"><label for="${id}-base">Address</label><input class="input mono" id="${id}-base" name="base_url" value="${p?.base_url || ''}" placeholder="${ADDRESS[kind]}">
      <span class="hint">GitHub Enterprise or self-managed GitLab: its address. OpenID Connect: the issuer URL (Gitter reads its discovery document).</span></div>
    <div class="grid-2">
      <div class="field"><label for="${id}-id">Client ID</label><input class="input mono" id="${id}-id" name="client_id" value="${p?.client_id || ''}" autocomplete="off" required></div>
      <div class="field"><label for="${id}-secret">Client secret</label><input class="input mono" id="${id}-secret" name="client_secret" type="password" autocomplete="new-password" placeholder="${p ? 'Unchanged' : ''}" ${p ? '' : 'required'}></div>
    </div>
    <label class="check"><input type="checkbox" name="allow_signup" ${!p || p.allow_signup ? 'checked' : ''}><span>New accounts: anyone with an account there can sign up here (while sign-up is open)</span></label>
    <label class="check"><input type="checkbox" name="allow_private" ${p?.allow_private ? 'checked' : ''}><span>On a private network: allow private addresses and plain http://</span></label>`;
}

function providerBody(form) {
  const f = Object.fromEntries(new FormData(form));
  const body = { name: f.name?.trim() || undefined, base_url: f.base_url?.trim() || undefined, client_id: f.client_id?.trim(), client_secret: f.client_secret,
    allow_signup: !!form.allow_signup.checked, allow_private: !!form.allow_private.checked };
  // The short name in sign-in URLs, from the button name: "Company SSO" → company-sso.
  if (f.kind) { body.kind = f.kind; body.slug = (body.name || KIND[f.kind]).toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 30) || f.kind; }
  return body;
}

async function signin() {
  const d = await get('/admin/oauth-providers', { fresh: true });
  return html`<div class="stack">
    <div class="box" id="providers"><div class="box-head"><b>Sign-in providers</b><span class="grow"></span><span class="muted small">Sign in with an account elsewhere</span></div>
      <div class="list">${d.providers.length ? d.providers.map((p) => html`<div class="list-row" data-provider="${p.id}" data-name="${p.name}">${icon('key', 'sm muted')}
        <div class="grow"><b>${p.name}</b> <span class="pill">${KIND[p.kind] || p.kind}</span>${p.enabled ? '' : html` <span class="pill">Off</span>`}
          <div class="small muted">${p.base_url} · ${plural(p.identities, 'linked account')} · ${p.allow_signup ? 'new accounts allowed' : 'existing accounts only'}</div></div>
        <button class="btn sm" data-prov="edit" aria-label="Edit ${p.name}">Edit</button>
        <button class="btn sm" data-prov="toggle" data-on="${p.enabled ? '1' : ''}" aria-label="${p.enabled ? 'Turn off' : 'Turn on'} ${p.name}">${p.enabled ? 'Turn off' : 'Turn on'}</button>
        <button class="btn sm icon danger" data-prov="delete" aria-label="Remove ${p.name}">${icon('trash', 'sm')}</button></div>`)
        : html`<div class="list-row muted small">None yet: people sign in with a password.</div>`}</div></div>
    <div class="box"><div class="box-head"><b>Add a provider</b></div><div class="box-body stack">
      <p class="muted small">Register an OAuth application with the provider (GitHub: Settings → Developer settings → OAuth Apps; GitLab: Preferences → Applications with the <code>read_user</code> scope; or your OpenID Connect provider), with this callback URL:</p>
      <div class="row"><input class="input mono" readonly value="${d.callback_url}" aria-label="Callback URL"><button class="btn" type="button" data-copy="${d.callback_url}" data-copy-label="Callback URL copied">Copy</button></div>
      <form class="stack" id="provider-form" novalidate>${providerFields(null)}
        <p class="field-err" id="pv-err" role="alert" hidden></p>
        <div class="row"><button class="btn primary" type="submit">Add provider</button></div></form>
      <p class="muted small">People are matched by the provider's account id, never by email. Someone who already has an account here links the provider under Settings → Security.</p>
    </div></div>
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
  const fed = s.federation ? await get('/admin/federation', { fresh: true }).catch(() => null) : null;
  const mb = (b) => (b >= 1048576 ? `${Math.round(b / 1048576)} MB` : `${Math.ceil(b / 1024)} KB`);
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
      <div class="field" style="width:220px"><label for="s-lfs">Largest Git LFS file (MB)</label><input id="s-lfs" class="input" type="number" min="1" value="${s.max_lfs_mb}"><span class="hint">Per file, checked while uploading.</span></div>
    </div>
    <div class="field"><label>CI</label><div class="choice">
      <label><input type="radio" name="ci" value="off" ${s.ci === 'off' ? 'checked' : ''}><span><b>Off</b><br><span class="muted small">No pipelines run.</span></span></label>
      <label><input type="radio" name="ci" value="admins" ${s.ci === 'admins' ? 'checked' : ''}><span><b>Admins' repositories</b><br><span class="muted small">Jobs run the repository's code. On the built-in runner that's the server itself, with access to its data folder.</span></span></label>
      <label><input type="radio" name="ci" value="everyone" ${s.ci === 'everyone' ? 'checked' : ''}><span><b>Everyone</b><br><span class="muted small">Any account's pushes run jobs, on separate runners only: the built-in runner still takes just admins' repositories. Use disposable runner machines.</span></span></label></div></div>
    <div class="field" style="width:220px"><label for="s-builtin">Jobs the built-in runner runs at once</label><input id="s-builtin" class="input" type="number" min="0" max="64" value="${s.builtin_runner_jobs}"><span class="hint">0 turns it off (use separate runners).</span></div>
    <div class="row wrap-ok" style="gap:16px;align-items:flex-start">
      <div class="field" style="width:160px"><label for="s-cpus">CPUs per job</label><input id="s-cpus" class="input" type="number" min="0" max="256" value="${s.ci_cpus}"><span class="hint">0: no limit.</span></div>
      <div class="field" style="width:180px"><label for="s-mem">Memory per job (MB)</label><input id="s-mem" class="input" type="number" min="0" max="1048576" value="${s.ci_memory_mb}"><span class="hint">0: no limit.</span></div>
      <div class="field" style="width:220px"><label for="s-trash">Deleted repositories kept (days)</label><input id="s-trash" class="input" type="number" min="1" max="365" value="${s.trash_days}"><span class="hint">How long they can be restored. Applies to repositories already deleted: shortening it deletes older ones for good at the next hourly sweep.</span></div>
      <div class="field" style="width:220px"><label for="s-offer">Transfer offers last (days)</label><input id="s-offer" class="input" type="number" min="1" max="365" value="${s.transfer_offer_days}"></div>
      <label class="check" style="margin-top:26px"><input type="checkbox" id="s-net" ${s.ci_network ? 'checked' : ''}> Jobs may use the network</label>
    </div>
    <p class="hint">Limits apply to jobs that run in a container (they name an <code>image</code>). Containers never get the host's docker socket or new privileges.</p>
    <div class="field"><span class="label">Federation</span>
      <label class="check"><input type="checkbox" id="s-fed" ${s.federation ? 'checked' : ''}> Work with other Gitter servers</label>
      <span class="hint">People here can fork public repositories from other Gitter servers that do too, and open pull requests back to them; people there can do the same with this server's public repositories. Requests between servers are signed with this server's key, published at <code>/.well-known/gitter</code>.</span>
      <label class="check small"><input type="checkbox" id="s-fed-private" ${s.federation_private ? 'checked' : ''}> Those servers may be at private addresses (your own network)</label>
      <div class="row wrap-ok" style="gap:12px;align-items:flex-start">
        <div class="field grow"><label for="s-fed-allow">Only these servers <span class="muted" style="font-weight:400">(empty: any)</span></label>
          <textarea id="s-fed-allow" class="textarea mono" rows="3" placeholder="git.example.com">${s.federation_allow.join('\n')}</textarea></div>
        <div class="field grow"><label for="s-fed-block">Never these servers</label>
          <textarea id="s-fed-block" class="textarea mono" rows="3" placeholder="spam.example">${s.federation_block.join('\n')}</textarea></div>
      </div>
      <span class="hint">One per line, as host or host:port. A server left out can't open pull requests here, and people here can't fork from it or propose to it.</span>
      ${fed?.servers.length ? html`<div class="box" id="fed-servers"><div class="box-head"><b>Servers with something here</b><span class="grow"></span><span class="muted small">each may bring in ${mb(fed.daily_budget)} a day</span></div>
        ${fed.servers.map((r) => html`<div class="list-row"><span class="mono grow">${r.host}${r.allowed ? '' : html` <span class="pill">left out</span>`}</span>
          <span class="muted small">${r.open_pulls} open pull request${r.open_pulls === 1 ? '' : 's'} · ${r.forks} fork${r.forks === 1 ? '' : 's'} from it · ${mb(r.fetched_today)} today</span></div>`)}</div>` : ''}</div>
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
  const inner = tab === 'users' ? await users(q, page) : tab === 'reports' ? await reports(ctx.query.get('status') || 'open') : tab === 'audit' ? await audit(q, page)
    : tab === 'runners' ? await runners() : tab === 'sign-in' ? await signin() : tab === 'settings' ? await settings() : await overview();
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
        const pb = e.target.closest('[data-purge]');
        if (pb) {
          if (!(await confirmDialog({ title: `Delete ${pb.dataset.name} now?`, text: 'Its owner can no longer restore it. Its code, issues and pull requests are gone for good.', ok: 'Delete now', danger: true }))) return;
          await action(pb, async () => { await send('DELETE', `/admin/trash/${pb.dataset.purge}`); toast(`Deleted ${pb.dataset.name}`); refresh(); })();
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
      const rf = $('#runner-form', el);
      rf?.addEventListener('submit', action(rf.querySelector('button'), async () => {
        const r = await send('POST', '/admin/runners', { name: rf.name.value.trim() });
        const cmd = `GITTER_RUNNER_TOKEN=${r.token} gitter runner --url ${location.origin}`;
        $('#runner-new', el).innerHTML = html`<div class="token-reveal fade"><b>Runner token created</b>
          <p class="small">Copy it now; it won't be shown again. On the runner machine (it needs git, and docker for jobs with an <code>image</code>):</p>
          <div class="row"><input class="input mono" readonly value="${cmd}" aria-label="Runner command"><button class="btn primary" type="button" data-copy="${cmd}" data-copy-label="Command copied">Copy</button></div></div>`.s;
        rf.reset();
      }));
      $('#runners', el)?.addEventListener('click', async (e) => {
        const b = e.target.closest('[data-runner-del]');
        if (!b) return;
        if (!(await confirmDialog({ title: `Remove ${b.dataset.name}?`, text: 'Its token stops working, and jobs it was running go back to the queue.', ok: 'Remove', danger: true }))) return;
        try { await send('DELETE', `/admin/runners/${b.dataset.runnerDel}`); } catch (x) { return toast(x.message, 'err'); }
        toast('Runner removed');
        navigate(location.pathname, { replace: true, scroll: false });
      });
      const pf = $('#provider-form', el);
      pf?.addEventListener('change', (e) => {
        if (e.target.name !== 'kind') return;
        $('#pv-name', el).placeholder = KIND[e.target.value];
        $('#pv-base', el).placeholder = ADDRESS[e.target.value];
      });
      pf?.addEventListener('submit', action(pf.querySelector('button[type=submit]'), async () => {
        const err = $('#pv-err', el);
        err.hidden = true;
        try { await send('POST', '/admin/oauth-providers', providerBody(pf)); }
        catch (x) { err.textContent = x.message; err.hidden = false; return; }
        toast('Provider added');
        navigate(location.pathname, { replace: true, scroll: false });
      }));
      $('#providers', el)?.addEventListener('click', async (e) => {
        const b = e.target.closest('[data-prov]');
        if (!b) return;
        const row = b.closest('[data-provider]'), id = row.dataset.provider, name = row.dataset.name;
        try {
          if (b.dataset.prov === 'toggle') await send('PATCH', `/admin/oauth-providers/${id}`, { enabled: !b.dataset.on });
          else if (b.dataset.prov === 'delete') {
            if (!(await confirmDialog({ title: `Remove ${name}?`, text: 'Accounts linked to it are unlinked. Anyone who signs in only through it would be locked out, so that is refused: turn it off instead.', ok: 'Remove', danger: true }))) return;
            await send('DELETE', `/admin/oauth-providers/${id}`);
          } else if (b.dataset.prov === 'edit') {
            const p = (await get('/admin/oauth-providers', { fresh: true })).providers.find((x) => String(x.id) === id);
            const saved = await modal({
              title: `Edit ${name}`, wide: true,
              body: html`<form class="stack" id="pv-edit" novalidate>${providerFields(p, 'pe')}<p class="field-err" id="pv-edit-err" role="alert" hidden></p></form>`,
              foot: html`<button class="btn" data-close>Cancel</button><button class="btn primary" data-ok>Save</button>`,
              onMount(m, close) {
                const ok = m.querySelector('[data-ok]'), f = m.querySelector('#pv-edit'), er = m.querySelector('#pv-edit-err');
                ok.addEventListener('click', action(ok, async () => {
                  er.hidden = true;
                  try { await send('PATCH', `/admin/oauth-providers/${id}`, providerBody(f)); close(true); }
                  catch (x) { er.textContent = x.message; er.hidden = false; }
                }));
              },
            });
            if (!saved) return;
          }
        } catch (x) { return toast(x.message, 'err'); }
        navigate(location.pathname, { replace: true, scroll: false });
      });
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
          max_lfs_mb: +$('#s-lfs', el).value,
          ci: $$('input[name=ci]', el).find((r) => r.checked)?.value || 'admins',
          builtin_runner_jobs: +$('#s-builtin', el).value,
          ci_cpus: +$('#s-cpus', el).value,
          ci_memory_mb: +$('#s-mem', el).value,
          ci_network: $('#s-net', el).checked,
          trash_days: +$('#s-trash', el).value,
          transfer_offer_days: +$('#s-offer', el).value,
          announcement: $('#s-ann', el).value.trim(),
          federation: $('#s-fed', el).checked,
          federation_private: $('#s-fed-private', el).checked,
          federation_allow: $('#s-fed-allow', el).value.split('\n').map((x) => x.trim()).filter(Boolean),
          federation_block: $('#s-fed-block', el).value.split('\n').map((x) => x.trim()).filter(Boolean),
        };
        const saved = await send('PUT', '/admin/settings', body);
        state.instance = { ...state.instance, name: saved.instance_name, announcement: saved.announcement, trash_days: saved.trash_days, transfer_offer_days: saved.transfer_offer_days, federation: saved.federation };
        state.signup = saved.signup === 'open';
        state.signupMode = saved.signup;
        renderTopbar();
        toast('Settings saved');
      }));
    },
  };
}
