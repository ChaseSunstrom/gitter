// Settings → Mirrors: a pull mirror (this repository follows a remote, read-only here) and
// push mirrors (this repository's branches and tags pushed to a remote after each change).
import { html, get, send, toast, action, confirmDialog, timeEl, invalidate, $ } from '../lib.js';
import { icon } from '../icons.js';
import { loadRepo, repoUrl, repoPage, mountRepoHeader, ciIcon } from '../repohead.js';
import { navigate } from '../main.js';

/** [icon, words] for a mirror's state. */
export function mirrorState(m) {
  if (m.running) return [ciIcon('running'), html`Syncing now`];
  if (m.failures) return [ciIcon('failure'), html`Failing (${m.failures} in a row)${m.next_at ? html`, trying again ${timeEl(m.next_at)}` : ''}${m.last_ok_at ? html`, last synced ${timeEl(m.last_ok_at)}` : ''}`];
  if (!m.last_ok_at) return [ciIcon('queued'), html`Waiting for its first sync`];
  return [ciIcon('success'), html`Synced ${timeEl(m.last_ok_at)}`];
}

function row(m, canPrivate) {
  const [ic, words] = mirrorState(m);
  const pull = m.direction === 'pull';
  return html`<div class="list-row mirror-row" data-mid="${m.id}">${ic}<div class="grow" style="min-width:0">
      <div class="row wrap-ok"><span class="pill">${icon(pull ? 'download' : 'upload', 'sm')} ${m.once ? 'Copies once from' : pull ? 'Pulls from' : 'Pushes to'}</span><b class="mono ellipsis">${m.url}</b></div>
      <div class="small muted">${words}${pull ? ` · every ${interval(m.interval_minutes)}` : ' · after every change'}${m.username ? ` · as ${m.username}` : ''}${m.allow_private ? ' · private addresses allowed' : ''}</div>
      ${m.last_error ? html`<pre class="setup-cmd small c-red mirror-error">${m.last_error}</pre>` : ''}
      <details class="mirror-edit"><summary class="btn sm">Edit</summary>${form(m, canPrivate)}</details></div>
    <button class="btn sm" data-sync="${m.id}">${icon('history', 'sm')} Sync now</button>
    <button class="btn sm danger" data-del="${m.id}" data-dir="${m.direction}">${icon('trash', 'sm')}</button></div>`;
}

const interval = (min) => (min % 1440 === 0 ? `${min / 1440} day${min === 1440 ? '' : 's'}` : min % 60 === 0 ? `${min / 60} hour${min === 60 ? '' : 's'}` : `${min} minutes`);

function form(m, canPrivate, direction = m?.direction) {
  const isNew = !m;
  return html`<form class="stack mirror-form" data-id="${m?.id || ''}" data-direction="${direction}" style="margin-top:12px">
    <div class="field"><label>Remote URL<input class="input mono" name="url" type="url" required maxlength="2048" placeholder="https://github.com/owner/repo.git" value="${m?.url || ''}"></label>
      <span class="hint">http:// or https:// only, without a user name or password in it. Redirects aren't followed.</span></div>
    <div class="row wrap-ok">
      <label class="field grow">User name<input class="input" name="username" maxlength="200" autocomplete="off" value="${m?.username || ''}"></label>
      <label class="field grow">Password or token<input class="input mono" name="password" type="password" maxlength="1024" autocomplete="new-password" placeholder="${m?.has_password ? 'Leave empty to keep the current one' : 'For private remotes'}"></label>
    </div>
    ${direction === 'pull' ? html`<label class="field">Sync every<select class="select" name="interval_minutes" style="max-width:200px">${[10, 30, 60, 240, 480, 1440, 10080].map((v) => html`<option value="${v}" ${v === (m?.interval_minutes || 480) ? 'selected' : ''}>${interval(v)}</option>`)}</select></label>` : ''}
    <label class="check"><input type="checkbox" name="allow_private" ${m?.allow_private ? 'checked' : ''} ${canPrivate ? '' : 'disabled'}><span>Allow private addresses
      <span class="muted small">Lets it reach this server's own network, like localhost, 10.x and 192.168.x. Only site admins can change this.</span></span></label>
    <div><button class="btn primary" type="submit">${isNew ? (direction === 'pull' ? 'Mirror this remote' : 'Add push mirror') : 'Save changes'}</button></div>
  </form>`;
}

export default async function (ctx) {
  const meta = await loadRepo(ctx.params.owner, ctx.params.repo);
  if (!meta.can_admin) throw Object.assign(new Error('Only admins of this repository can change its mirrors'), { status: 403 });
  const api = repoUrl(meta.owner, meta.name);
  const full = `${meta.owner}/${meta.name}`;
  const d = await get(`${api}/mirrors`, { fresh: true });
  const pull = d.mirrors.filter((m) => m.direction === 'pull');
  const push = d.mirrors.filter((m) => m.direction === 'push');
  const body = html`<div class="mid stack" style="max-width:820px">
    <a class="muted small ci-back" href="/${full}/settings">${icon('arrow-left', 'sm')} Settings</a>
    <div class="box" id="pull-mirror"><div class="box-head"><b>Pull mirror</b></div><div class="box-body stack">
      <p class="muted small">This repository follows a remote: its branches and tags are fetched on a schedule and replace the ones here, so nothing can be pushed, committed or merged here while it's a mirror. Syncs don't run CI or send push webhooks.</p>
      ${pull.length ? html`<div class="list">${pull.map((m) => row(m, d.can_allow_private))}</div>`
        : d.can_pull ? html`<details id="add-pull"><summary class="btn sm">${icon('plus', 'sm')} Mirror a remote repository</summary>${form(null, d.can_allow_private, 'pull')}</details>`
        : html`<p class="small">Only an empty repository can become a pull mirror, since syncing replaces all its branches and tags. <a href="/new">Make a new repository</a> for it.</p>`}
    </div></div>
    <div class="box" id="push-mirrors"><div class="box-head"><b>Push mirrors</b></div><div class="box-body stack">
      <p class="muted small">After every change here, all branches and tags are pushed to each remote, replacing what's there (a burst of pushes is one mirror push). Failed pushes are tried again after 1, 2, 4… minutes, up to once a day.</p>
      ${push.length ? html`<div class="list">${push.map((m) => row(m, d.can_allow_private))}</div>` : ''}
      <details id="add-push"><summary class="btn sm">${icon('plus', 'sm')} Add a push mirror</summary>${form(null, d.can_allow_private, 'push')}</details>
    </div></div>
  </div>`;
  const reload = () => { invalidate(api); navigate(location.pathname, { replace: true, scroll: false }); };
  return {
    title: `Mirrors · ${full}`,
    html: repoPage(meta, 'settings', body),
    mount(el) {
      mountRepoHeader(el, meta);
      el.addEventListener('submit', (e) => {
        const f = e.target.closest('.mirror-form');
        if (!f) return;
        e.preventDefault();
        action($('button[type=submit]', f), async () => {
          const v = (n) => f.elements[n];
          const req = { url: v('url').value.trim(), username: v('username').value.trim() };
          if (v('password').value) req.password = v('password').value;
          if (v('interval_minutes')) req.interval_minutes = Number(v('interval_minutes').value);
          if (d.can_allow_private) req.allow_private = v('allow_private').checked;
          if (f.dataset.id) await send('PATCH', `${api}/mirrors/${f.dataset.id}`, req);
          else await send('POST', `${api}/mirrors`, { ...req, direction: f.dataset.direction });
          toast(f.dataset.id ? 'Mirror saved. It syncs again shortly.' : 'Mirror added. The first sync starts shortly.');
          reload();
        })(e);
      });
      el.addEventListener('click', (e) => {
        const s = e.target.closest('[data-sync]');
        if (s) {
          action(s, async () => {
            await send('POST', `${api}/mirrors/${s.dataset.sync}/sync`);
            toast('Syncing shortly');
            reload();
          })(e);
          return;
        }
        const del = e.target.closest('[data-del]');
        if (!del) return;
        action(del, async () => {
          const pulls = del.dataset.dir === 'pull';
          const ok = await confirmDialog({
            title: 'Remove this mirror?',
            text: pulls ? 'The repository stops syncing and becomes an ordinary one you can push to. Its branches and tags stay.' : 'Nothing more is pushed to that remote. What it has stays there.',
            ok: 'Remove', danger: true,
          });
          if (!ok) return;
          await send('DELETE', `${api}/mirrors/${del.dataset.del}`);
          toast('Mirror removed');
          reload();
        })(e);
      });
    },
  };
}
