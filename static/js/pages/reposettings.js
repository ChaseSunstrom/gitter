// Repository settings: name, description, website, default branch, access, deploy tokens, CI secrets, webhooks, visibility, delete.
import { html, get, send, toast, action, confirmDialog, timeEl, avatar, $ } from '../lib.js';
import { icon } from '../icons.js';
import { loadRepo, repoUrl, repoPage, mountRepoHeader, ciIcon } from '../repohead.js';
import { navigate } from '../main.js';
import { eventsLabel, deliveryState } from './webhook.js';

const ROLES = [['read', 'Read'], ['write', 'Write'], ['admin', 'Admin']];

export default async function (ctx) {
  const meta = await loadRepo(ctx.params.owner, ctx.params.repo);
  if (!meta.can_admin) throw Object.assign(new Error('Only admins of this repository can change its settings'), { status: 403 });
  const api = repoUrl(meta.owner, meta.name);
  const full = `${meta.owner}/${meta.name}`;
  const [access, deploy, secrets, hooks] = await Promise.all([
    get(`${api}/collaborators`, { fresh: true }),
    get(`${api}/deploy-tokens`, { fresh: true }).then((d) => d.tokens),
    get(`${api}/ci/secrets`, { fresh: true }).then((d) => d.secrets),
    get(`${api}/hooks`, { fresh: true }).then((d) => d.hooks),
  ]);
  const body = html`<div class="mid stack" style="max-width:820px">
    <form class="box" id="general"><div class="box-head"><b>General</b></div><div class="box-body settings-sec">
      <div class="field"><label for="s-name">Repository name</label><input id="s-name" class="input" value="${meta.name}" required>
        <span class="hint">Renaming changes the clone URL. Update your remotes after.</span></div>
      <div class="field"><label for="s-desc">Description</label><input id="s-desc" class="input" value="${meta.description}" maxlength="350"></div>
      <div class="field"><label for="s-web">Website</label><input id="s-web" class="input" type="url" value="${meta.website}" placeholder="https://"></div>
      ${meta.branches.length ? html`<div class="field"><label for="s-branch">Default branch</label>
        <select id="s-branch" class="select" style="max-width:320px">${meta.branches.map((b) => html`<option ${b === meta.default_branch ? 'selected' : ''}>${b}</option>`)}</select>
        <span class="hint">The branch people see first, and the default target for pull requests.</span></div>` : ''}
      <div><button class="btn primary" id="s-save">Save changes</button></div>
    </div></form>

    <div class="box" id="access"><div class="box-head"><b>Who has access</b></div><div class="box-body stack">
      <ul class="access-list small">${access.access.map((a) => html`<li><b>${a.who}</b>${a.detail ? html` <span class="muted">(${a.detail})</span>` : ''} ${a.text}</li>`)}</ul>
      <form id="collab-form" class="row wrap-ok">
        <input class="input" name="user" placeholder="Username" autocomplete="off" spellcheck="false" aria-label="Username to add" required style="max-width:240px">
        <select class="select" name="role" aria-label="Role" style="max-width:200px">${ROLES.map(([v, t]) => html`<option value="${v}" ${v === 'write' ? 'selected' : ''}>${t}</option>`)}</select>
        <button class="btn primary" type="submit">Add</button>
      </form>
      ${access.collaborators.length ? html`<div class="list">${access.collaborators.map((c) => html`<div class="list-row">${avatar(c.username, 24)}<div class="grow"><a href="/${c.username}"><b>${c.username}</b></a>
          <div class="small muted">${c.text}${c.added_by ? ` · added by ${c.added_by}` : ''}</div></div>
        <select class="select" data-role="${c.username}" aria-label="Role for ${c.username}" style="max-width:160px">${ROLES.map(([v, t]) => html`<option value="${v}" ${v === c.role ? 'selected' : ''}>${t}</option>`)}</select>
        <button class="btn sm danger" data-remove="${c.username}">Remove</button></div>`)}</div>` : html`<p class="muted small">No collaborators yet. Add someone to let them in.</p>`}
    </div></div>

    <div class="box" id="deploy"><div class="box-head"><b>Deploy tokens</b></div><div class="box-body stack">
      <p class="muted small">Give a CI server or deploy machine access to this repository only. Use the token as the password, with any username.</p>
      <form id="dt-form" class="row wrap-ok">
        <input class="input" name="name" placeholder="Where it's used, e.g. build server" maxlength="80" aria-label="Deploy token name" required style="max-width:280px">
        <select class="select" name="expires_days" aria-label="Expires" style="max-width:160px"><option value="90">90 days</option><option value="365">1 year</option><option value="">Never expires</option></select>
        <label class="row small"><input type="checkbox" name="write"> Allow pushing</label>
        <button class="btn primary" type="submit">Create</button>
      </form>
      <div id="dt-new"></div>
      ${deploy.length ? html`<div class="list">${deploy.map((t) => html`<div class="list-row"><div class="grow"><b>${t.name}</b> <span class="pill">${t.scope === 'write' ? 'Read and write' : 'Read only'}</span>${t.expired ? html` <span class="pill closed">Expired</span>` : ''}
          <div class="small muted">Created by ${t.created_by} ${timeEl(t.created_at)} · ${t.last_used ? html`last used ${timeEl(t.last_used)}` : 'never used'}${t.expires_at && !t.expired ? html` · expires ${timeEl(t.expires_at)}` : ''}</div></div>
        <button class="btn sm danger" data-dt-del="${t.id}" data-name="${t.name}">Delete</button></div>`)}</div>` : ''}
    </div></div>

    <div class="box" id="secrets"><div class="box-head"><b>CI secrets</b></div><div class="box-body stack">
      <p class="muted small">CI jobs get these as environment variables, and their values are masked in logs. They're encrypted on the server and can't be read back, only replaced.</p>
      <form id="sec-form" class="row wrap-ok">
        <input class="input mono" name="name" placeholder="NAME" pattern="[A-Za-z_][A-Za-z0-9_]*" title="Letters, digits and _ (saved in capitals)" maxlength="64" required style="max-width:220px" aria-label="Secret name" autocomplete="off" spellcheck="false">
        <input class="input mono" name="value" type="password" placeholder="Value" required autocomplete="new-password" aria-label="Secret value" style="max-width:320px">
        <button class="btn primary" type="submit">Save secret</button>
      </form>
      ${secrets.length ? html`<div class="list">${secrets.map((s) => html`<div class="list-row">${icon('key', 'sm muted')}<div class="grow"><b class="mono">${s.name}</b>
          <div class="small muted">Updated ${timeEl(s.updated_at)}</div></div>
        <button class="btn sm danger" data-sec-del="${s.name}">Delete</button></div>`)}</div>` : ''}
    </div></div>

    <div class="box" id="hooks"><div class="box-head"><b>Webhooks</b><span class="grow"></span><a class="btn sm" href="/${full}/settings/hooks/new">${icon('plus', 'sm')} Add webhook</a></div>
      <div class="box-body"><p class="muted small">Gitter sends a signed POST to each URL when something happens here: pushes, issues, comments, pull requests and finished CI runs.</p></div>
      ${hooks.length ? html`<div class="list">${hooks.map((h) => {
        const [ic, words] = h.last ? deliveryState(h.last) : [ciIcon('pending'), 'Nothing sent yet'];
        return html`<a class="list-row" href="/${full}/settings/hooks/${h.id}">${ic}<div class="grow"><b class="mono">${h.url}</b>
          <div class="small muted">${eventsLabel(h.events)}${h.active ? '' : ' · Turned off'} · ${h.last?.at ? html`Last delivery ${timeEl(h.last.at)}: ` : ''}${words}</div></div>${icon('chev-right', 'sm muted')}</a>`;
      })}</div>` : ''}
    </div>

    <div class="box danger-zone"><div class="box-head"><b class="c-red">Danger zone</b></div><div class="list">
      <div class="list-row"><div class="grow"><b>Change visibility</b><div class="muted small">This repository is <b>${meta.private ? 'private' : 'public'}</b>. ${meta.private ? 'Making it public lets anyone see and clone it.' : 'Making it private hides it from everyone without access.'}</div></div>
        <button class="btn danger" id="s-vis">${icon(meta.private ? 'globe' : 'lock', 'sm')} Make ${meta.private ? 'public' : 'private'}</button></div>
      <div class="list-row"><div class="grow"><b>Delete this repository</b><div class="muted small">Deletes the code, issues and pull requests. This can't be undone.</div></div>
        <button class="btn danger" id="s-del">${icon('trash', 'sm')} Delete repository</button></div>
    </div></div>
  </div>`;
  return {
    title: `Settings · ${full}`,
    html: repoPage(meta, 'settings', body),
    mount(el) {
      mountRepoHeader(el, meta);
      const save = $('#s-save', el);
      $('#general', el).addEventListener('submit', action(save, async () => {
        const patch = {
          name: $('#s-name', el).value.trim(),
          description: $('#s-desc', el).value.trim(),
          website: $('#s-web', el).value.trim(),
        };
        const br = $('#s-branch', el);
        if (br && br.value !== meta.default_branch) patch.default_branch = br.value;
        const r = await send('PATCH', api, patch);
        toast('Settings saved');
        navigate(`/${r.owner}/${r.name}/settings`, { replace: true });
      }));
      const vis = $('#s-vis', el);
      vis.addEventListener('click', action(vis, async () => {
        const goPublic = meta.private;
        const ok = await confirmDialog({
          title: goPublic ? 'Make this repository public?' : 'Make this repository private?',
          text: goPublic ? 'Anyone on the internet will be able to see and clone it.' : 'Only people with access will be able to see it. Stars from others stay, but they lose access.',
          ok: goPublic ? 'Make public' : 'Make private', danger: true, typed: goPublic ? full : '',
        });
        if (!ok) return;
        await send('PATCH', api, { private: !goPublic });
        toast(goPublic ? 'Repository is now public' : 'Repository is now private');
        navigate(location.pathname, { replace: true });
      }));
      const cf = $('#collab-form', el);
      cf.addEventListener('submit', action(cf.querySelector('button'), async () => {
        const user = cf.user.value.trim();
        await send('PUT', `${api}/collaborators/${encodeURIComponent(user)}`, { role: cf.role.value });
        toast(`Added ${user}`);
        navigate(location.pathname, { replace: true, scroll: false });
      }));
      $('#access', el).addEventListener('change', async (e) => {
        const s = e.target.closest('[data-role]');
        if (!s) return;
        try { await send('PUT', `${api}/collaborators/${encodeURIComponent(s.dataset.role)}`, { role: s.value }); } catch (x) { return toast(x.message, 'err'); }
        toast(`${s.dataset.role} now has ${s.value} access`);
        navigate(location.pathname, { replace: true, scroll: false });
      });
      $('#access', el).addEventListener('click', async (e) => {
        const b = e.target.closest('[data-remove]');
        if (!b) return;
        if (!(await confirmDialog({ title: `Remove ${b.dataset.remove}?`, text: 'They lose access to this repository right away.', ok: 'Remove', danger: true }))) return;
        try { await send('DELETE', `${api}/collaborators/${encodeURIComponent(b.dataset.remove)}`); } catch (x) { return toast(x.message, 'err'); }
        toast(`Removed ${b.dataset.remove}`);
        navigate(location.pathname, { replace: true, scroll: false });
      });
      const dt = $('#dt-form', el);
      dt.addEventListener('submit', action(dt.querySelector('button'), async () => {
        const f = Object.fromEntries(new FormData(dt));
        const r = await send('POST', `${api}/deploy-tokens`, { name: f.name.trim(), write: !!f.write, expires_days: f.expires_days ? +f.expires_days : null });
        const url = `${location.protocol}//x-access-token:${r.token}@${location.host}/${full}.git`;
        $('#dt-new', el).innerHTML = html`<div class="token-reveal fade"><div class="row"><b>Deploy token created</b></div>
          <p class="small">Copy it now; it won't be shown again.</p>
          <div class="row"><input class="input mono" readonly value="${r.token}" aria-label="New deploy token"><button class="btn primary" type="button" data-copy="${r.token}" data-copy-label="Token copied">Copy</button></div>
          <pre class="setup-cmd">git clone ${url}</pre></div>`.s;
        dt.reset();
      }));
      $('#deploy', el).addEventListener('click', async (e) => {
        const b = e.target.closest('[data-dt-del]');
        if (!b) return;
        if (!(await confirmDialog({ title: `Delete “${b.dataset.name}”?`, text: 'Anything using it loses access to this repository right away.', ok: 'Delete', danger: true }))) return;
        try { await send('DELETE', `${api}/deploy-tokens/${b.dataset.dtDel}`); } catch (x) { return toast(x.message, 'err'); }
        toast('Deploy token deleted');
        navigate(location.pathname, { replace: true });
      });
      const sf = $('#sec-form', el);
      sf.addEventListener('submit', action(sf.querySelector('button'), async () => {
        const name = sf.name.value.trim().toUpperCase();
        await send('PUT', `${api}/ci/secrets/${encodeURIComponent(name)}`, { value: sf.value.value });
        toast(`Saved ${name}`);
        navigate(location.pathname, { replace: true, scroll: false });
      }));
      $('#secrets', el).addEventListener('click', async (e) => {
        const b = e.target.closest('[data-sec-del]');
        if (!b) return;
        const name = b.dataset.secDel;
        if (!(await confirmDialog({ title: `Delete ${name}?`, text: 'Jobs that use it will no longer get it.', ok: 'Delete', danger: true }))) return;
        try { await send('DELETE', `${api}/ci/secrets/${encodeURIComponent(name)}`); } catch (x) { return toast(x.message, 'err'); }
        toast(`Deleted ${name}`);
        navigate(location.pathname, { replace: true, scroll: false });
      });
      const del = $('#s-del', el);
      del.addEventListener('click', action(del, async () => {
        const ok = await confirmDialog({ title: `Delete ${full}?`, text: 'This permanently deletes the repository, its issues and its pull requests.', ok: 'Delete this repository', danger: true, typed: full });
        if (!ok) return;
        await send('DELETE', api, { confirm: full });
        toast(`Deleted ${full}`);
        navigate('/' + meta.owner);
      }));
    },
  };
}
