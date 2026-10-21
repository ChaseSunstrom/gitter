// Repository settings: name, description, website, default branch, visibility, delete.
import { html, send, toast, action, confirmDialog, $ } from '../lib.js';
import { icon } from '../icons.js';
import { loadRepo, repoUrl, repoPage, mountRepoHeader } from '../repohead.js';
import { navigate } from '../main.js';

export default async function (ctx) {
  const meta = await loadRepo(ctx.params.owner, ctx.params.repo);
  if (!meta.can_write) throw Object.assign(new Error('Only people who can write to this repository can change its settings'), { status: 403 });
  const api = repoUrl(meta.owner, meta.name);
  const full = `${meta.owner}/${meta.name}`;
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

    <div class="box danger-zone"><div class="box-head"><b class="c-red">Danger zone</b></div><div class="list">
      <div class="list-row"><div class="grow"><b>Change visibility</b><div class="muted small">This repository is <b>${meta.private ? 'private' : 'public'}</b>. ${meta.private ? 'Making it public lets anyone see and clone it.' : 'Making it private hides it from everyone but you.'}</div></div>
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
          text: goPublic ? 'Anyone on the internet will be able to see and clone it.' : 'Only you will be able to see it. Stars from other people stay, but they lose access.',
          ok: goPublic ? 'Make public' : 'Make private', danger: true, typed: goPublic ? full : '',
        });
        if (!ok) return;
        await send('PATCH', api, { private: !goPublic });
        toast(goPublic ? 'Repository is now public' : 'Repository is now private');
        navigate(location.pathname, { replace: true });
      }));
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
