// Organizations: create one, and its settings (profile, base permission, members, delete).
import { html, state, get, send, action, avatar, enc, toast, confirmDialog, $ } from '../lib.js';
import { icon } from '../icons.js';
import { navigate } from '../main.js';

const BASE = [['none', 'No access', 'Members only see repositories they were added to.'],
  ['read', 'Read', 'Members can see and clone every repository.'],
  ['write', 'Write', 'Members can also push, and manage issues and pull requests.']];

export default async function (ctx) {
  if (!state.me) return { redirect: '/login?next=' + encodeURIComponent(ctx.url.pathname) };
  return ctx.params.view === 'new' ? newOrg() : settings(ctx.params.org);
}

function newOrg() {
  return {
    title: 'New organization',
    html: html`<div class="wrap narrow">
      <h1>Create an organization</h1>
      <p class="muted">An organization owns repositories that a group shares. Its owners decide who gets in and what members can do.</p>
      <form id="new-org" class="stack">
        <div class="field"><label for="org-name">Organization name</label>
          <input class="input" id="org-name" name="name" autocomplete="off" spellcheck="false" autofocus required maxlength="39">
          <span class="hint">It's the first part of its repositories' addresses, like ${location.host}/<b>name</b>/repo.</span></div>
        <div class="field"><label for="org-display">Display name <span class="muted" style="font-weight:400">(optional)</span></label>
          <input class="input" id="org-display" name="display_name" maxlength="80"></div>
        <p class="field-err" id="org-err" role="alert" hidden></p>
        <div class="row"><button class="btn primary" type="submit">${icon('users', 'sm')} Create organization</button></div>
      </form></div>`,
    mount(el) {
      const form = $('#new-org', el);
      const err = $('#org-err', el);
      form.addEventListener('submit', action(form.querySelector('button'), async () => {
        err.hidden = true;
        try {
          const r = await send('POST', '/orgs', { name: form.name.value.trim(), display_name: form.display_name.value.trim() });
          navigate(`/${r.name}`);
        } catch (e) { err.textContent = e.message; err.hidden = false; }
      }));
    },
  };
}

async function settings(name) {
  const d = await get(`/orgs/${enc(name)}`, { fresh: true });
  if (!d.is_owner) throw Object.assign(new Error("Only this organization's owners can change its settings"), { status: 403 });
  const o = d.org;
  const api = `/orgs/${enc(o.name)}`;
  return {
    title: `Settings · ${o.name}`,
    html: html`<div class="wrap mid stack" style="max-width:820px">
      <div class="row">${avatar(o.name, 40, 'sq')}<div><h1 style="margin:0">${o.display_name || o.name}</h1><a class="muted small" href="/${o.name}">@${o.name}</a></div></div>
      <form class="box" id="org-profile"><div class="box-head"><b>Profile</b></div><div class="box-body settings-sec">
        <div class="field"><label for="o-display">Display name</label><input class="input" id="o-display" name="display_name" value="${o.display_name}" maxlength="80"></div>
        <div class="field"><label for="o-bio">Description</label><textarea class="textarea" id="o-bio" name="bio" maxlength="300" rows="3" style="min-height:80px">${o.bio}</textarea></div>
        <div class="field"><label for="o-web">Website</label><input class="input" id="o-web" name="website" value="${o.website}" maxlength="200" placeholder="https://"></div>
        <div><button class="btn primary">Save profile</button></div>
      </div></form>
      <form class="box" id="org-base"><div class="box-head"><b>What members can do</b></div><div class="box-body stack">
        <p class="muted small">Applies to every repository this organization owns. Owners can do everything; people added to one repository keep that access too.</p>
        ${BASE.map(([v, t, hint]) => html`<label class="check"><input type="radio" name="base_role" value="${v}" ${o.base_role === v ? 'checked' : ''}><span><b>${t}</b><span class="muted small" style="display:block">${hint}</span></span></label>`)}
        <div><button class="btn primary">Save</button></div>
      </div></form>
      <div class="box" id="org-members"><div class="box-head"><b>Members</b></div><div class="box-body stack">
        <form id="member-form" class="row wrap-ok">
          <input class="input" name="user" placeholder="Username" autocomplete="off" spellcheck="false" aria-label="Username to add" required style="max-width:240px">
          <select class="select" name="role" aria-label="Role" style="max-width:160px"><option value="member" selected>Member</option><option value="owner">Owner</option></select>
          <button class="btn primary" type="submit">Add</button>
        </form>
        <div class="list">${d.members.map((m) => html`<div class="list-row">${avatar(m.username, 24)}<div class="grow"><a href="/${m.username}"><b>${m.username}</b></a>${m.display_name ? html` <span class="muted small">${m.display_name}</span>` : ''}</div>
          <select class="select" data-role="${m.username}" aria-label="Role for ${m.username}" style="max-width:140px"><option value="member" ${m.role === 'member' ? 'selected' : ''}>Member</option><option value="owner" ${m.role === 'owner' ? 'selected' : ''}>Owner</option></select>
          <button class="btn sm danger" data-remove="${m.username}">${m.username === state.me.username ? 'Leave' : 'Remove'}</button></div>`)}</div>
      </div></div>
      <div class="box danger-zone"><div class="box-head"><b class="c-red">Danger zone</b></div><div class="list">
        <div class="list-row"><div class="grow"><b>Delete this organization</b><div class="muted small">Only possible once it owns no repositories: delete or move them first.</div></div>
          <button class="btn danger" id="org-del">${icon('trash', 'sm')} Delete organization</button></div>
      </div></div>
    </div>`,
    mount(el) {
      const reload = () => navigate(location.pathname, { replace: true, scroll: false });
      const prof = $('#org-profile', el);
      prof.addEventListener('submit', action(prof.querySelector('button'), async () => {
        await send('PATCH', api, { display_name: prof.display_name.value.trim(), bio: prof.bio.value.trim(), website: prof.website.value.trim() });
        toast('Profile saved');
      }));
      const base = $('#org-base', el);
      base.addEventListener('submit', action(base.querySelector('button'), async () => {
        await send('PATCH', api, { base_role: new FormData(base).get('base_role') });
        toast('Saved');
      }));
      const mf = $('#member-form', el);
      mf.addEventListener('submit', action(mf.querySelector('button'), async () => {
        const user = mf.user.value.trim();
        await send('PUT', `${api}/members/${enc(user)}`, { role: mf.role.value });
        toast(`Added ${user}`);
        reload();
      }));
      const members = $('#org-members', el);
      members.addEventListener('change', async (e) => {
        const s = e.target.closest('[data-role]');
        if (!s) return;
        try { await send('PUT', `${api}/members/${enc(s.dataset.role)}`, { role: s.value }); } catch (x) { toast(x.message, 'err'); }
        reload();
      });
      members.addEventListener('click', async (e) => {
        const b = e.target.closest('[data-remove]');
        if (!b) return;
        const me = b.dataset.remove === state.me.username;
        if (!(await confirmDialog({ title: me ? `Leave ${o.name}?` : `Remove ${b.dataset.remove}?`, text: 'They lose access to its repositories right away, except ones they were added to directly.', ok: me ? 'Leave' : 'Remove', danger: true }))) return;
        try { await send('DELETE', `${api}/members/${enc(b.dataset.remove)}`); } catch (x) { return toast(x.message, 'err'); }
        if (me) return navigate(`/${o.name}`);
        toast(`Removed ${b.dataset.remove}`);
        reload();
      });
      const del = $('#org-del', el);
      del.addEventListener('click', action(del, async () => {
        if (!(await confirmDialog({ title: `Delete ${o.name}?`, text: "This can't be undone.", ok: 'Delete organization', danger: true, typed: o.name }))) return;
        await send('DELETE', api, { confirm: o.name });
        toast(`Deleted ${o.name}`);
        navigate('/');
      }));
    },
  };
}
