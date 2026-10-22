// Repository settings: name, description, website, default branch, access, deploy tokens, CI secrets, webhooks, visibility, transfer, delete.
import { html, get, send, toast, action, confirmDialog, modal, timeEl, avatar, plural, state, $ } from '../lib.js';
import { icon } from '../icons.js';
import { loadRepo, repoUrl, repoPage, mountRepoHeader, ciIcon } from '../repohead.js';
import { navigate } from '../main.js';
import { eventsLabel, deliveryState } from './webhook.js';

const ROLES = [['read', 'Read'], ['write', 'Write'], ['admin', 'Admin']];

/** Ask who should get the repository, with its name typed to confirm. Resolves to their username, or null. */
function askRecipient(full) {
  return modal({
    title: `Transfer ${full}`,
    body: html`<div class="stack"><p class="muted">They'll find the offer in their settings. Nothing moves until they accept, and the offer ends after ${plural(state.instance?.transfer_offer_days ?? 7, 'day')}.</p>
      <div class="field"><label for="tr-to">New owner's username</label><input id="tr-to" class="input" autocomplete="off" autocapitalize="off" spellcheck="false" autofocus></div>
      <div class="field"><label for="tr-typed">Type <b>${full}</b> to confirm</label><input id="tr-typed" class="input" autocomplete="off"></div></div>`,
    foot: html`<button class="btn" data-close>Cancel</button><button class="btn danger" data-ok disabled>Offer it</button>`,
    onMount(el, close) {
      const ok = el.querySelector('[data-ok]'), to = el.querySelector('#tr-to'), typed = el.querySelector('#tr-typed');
      const check = () => { ok.disabled = !to.value.trim() || typed.value.trim().toLowerCase() !== full.toLowerCase(); };
      to.addEventListener('input', check);
      typed.addEventListener('input', check);
      ok.addEventListener('click', () => close(to.value.trim()));
    },
  });
}

const ruleText = (r) => [
  r.require_pr || r.merge_queue ? 'pull requests only' : 'direct pushes allowed',
  r.approvals ? `${r.approvals} approval${r.approvals > 1 ? 's' : ''}` : '',
  r.checks.length ? `checks: ${r.checks.join(', ')}` : '',
  r.allow_force_push ? 'force-pushes allowed' : 'no force-pushes',
  r.allow_deletion ? 'can be deleted' : "can't be deleted",
  r.merge_queue ? 'merge queue' : '',
  r.enforce_admins ? 'admins too' : '',
].filter(Boolean).join(' · ');

const ruleFields = (r = {}) => html`<div class="field"><label>Branch pattern</label><input class="input mono" name="pattern" value="${r.pattern || ''}" placeholder="main, or release/*" required spellcheck="false"></div>
  <label class="check small"><input type="checkbox" name="require_pr" ${(r.require_pr ?? true) ? 'checked' : ''}> Changes go through pull requests (no direct pushes or web edits)</label>
  <div class="row wrap-ok" style="gap:12px;align-items:flex-start">
    <div class="field" style="width:180px"><label>Approvals needed</label><input class="input" type="number" min="0" max="10" name="approvals" value="${r.approvals || 0}"><span class="hint">From people with write access.</span></div>
    <div class="field grow"><label>Checks that must pass</label><input class="input mono" name="checks" value="${(r.checks || []).join(', ')}" placeholder="test, lint" spellcheck="false"><span class="hint">CI job names or commit status contexts, comma-separated.</span></div></div>
  <label class="check small"><input type="checkbox" name="merge_queue" ${r.merge_queue ? 'checked' : ''}> Merge through a queue (one at a time, each tested on top of the branch first)</label>
  <label class="check small"><input type="checkbox" name="allow_force_push" ${r.allow_force_push ? 'checked' : ''}> Allow force-pushes</label>
  <label class="check small"><input type="checkbox" name="allow_deletion" ${r.allow_deletion ? 'checked' : ''}> Allow deleting the branch</label>
  <label class="check small"><input type="checkbox" name="enforce_admins" ${r.enforce_admins ? 'checked' : ''}> Apply to admins too (otherwise they can bypass)</label>`;

const readRule = (f) => ({
  pattern: f.pattern.value.trim(), require_pr: f.require_pr.checked, approvals: +f.approvals.value || 0,
  checks: f.checks.value.split(',').map((s) => s.trim()).filter(Boolean),
  allow_force_push: f.allow_force_push.checked, allow_deletion: f.allow_deletion.checked, enforce_admins: f.enforce_admins.checked,
  merge_queue: f.merge_queue.checked,
});

export default async function (ctx) {
  const meta = await loadRepo(ctx.params.owner, ctx.params.repo);
  if (!meta.can_admin) throw Object.assign(new Error('Only admins of this repository can change its settings'), { status: 403 });
  const api = repoUrl(meta.owner, meta.name);
  const full = `${meta.owner}/${meta.name}`;
  const [access, deploy, secrets, hooks, offer, rules] = await Promise.all([
    get(`${api}/collaborators`, { fresh: true }),
    get(`${api}/deploy-tokens`, { fresh: true }).then((d) => d.tokens),
    get(`${api}/ci/secrets`, { fresh: true }).then((d) => d.secrets),
    get(`${api}/hooks`, { fresh: true }).then((d) => d.hooks),
    get(`${api}/transfer`, { fresh: true }).then((d) => d.offer),
    get(`${api}/branch-rules`, { fresh: true }).then((d) => d.rules),
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

    <div class="box" id="rules"><div class="box-head"><b>Branch rules</b></div><div class="box-body stack">
      <p class="muted small">Protect the branches matching a pattern (<code>*</code> within a part of the name, <code>**</code> across parts). Rules apply to git pushes, web edits and merges alike.</p>
      ${rules.length ? html`<div class="list">${rules.map((r) => html`<div class="list-row"><div class="grow"><b class="mono">${r.pattern}</b><div class="small muted">${ruleText(r)}</div></div>
        <button class="btn sm" data-rule-edit="${r.id}">Edit</button><button class="btn sm danger" data-rule-del="${r.id}" data-name="${r.pattern}">Delete</button></div>`)}</div>` : ''}
      <details><summary class="btn sm">Add a rule</summary><form id="rule-form" class="stack" style="margin-top:12px">${ruleFields()}<div><button class="btn primary" type="submit">Add rule</button></div></form></details>
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
      <p class="muted small">CI jobs get these as environment variables, and their values are masked in logs. They're encrypted on the server and can't be read back, only replaced. Secrets for one environment only (like production), and which branches may deploy there, are set on the <a href="/${full}/deployments">Deployments</a> page.</p>
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

    <div class="box" id="mirrors"><div class="box-head"><b>Mirrors</b><span class="grow"></span><a class="btn sm" href="/${full}/settings/mirrors">${icon('settings', 'sm')} Manage mirrors</a></div>
      <div class="box-body"><p class="muted small">${meta.mirror ? html`This repository mirrors <b class="mono">${meta.mirror.url}</b> and is read-only here.` : 'Follow a remote repository (pull mirror), or push this one to other servers after every change (push mirrors).'}</p></div>
    </div>

    <div class="box danger-zone"><div class="box-head"><b class="c-red">Danger zone</b></div><div class="list">
      <div class="list-row"><div class="grow"><b>Change visibility</b><div class="muted small">This repository is <b>${meta.private ? 'private' : 'public'}</b>. ${meta.private ? 'Making it public lets anyone see and clone it.' : 'Making it private hides it from everyone without access.'}</div></div>
        <button class="btn danger" id="s-vis">${icon(meta.private ? 'globe' : 'lock', 'sm')} Make ${meta.private ? 'public' : 'private'}</button></div>
      ${meta.can_transfer ? html`<div class="list-row" id="transfer"><div class="grow"><b>Transfer ownership</b><div class="muted small">${offer
          ? html`Offered to <b>${offer.to}</b>, who can accept or decline it until ${timeEl(offer.expires_at)}.`
          : 'Give this repository to another account. It moves once they accept, with its issues, stars and history, and its old address leads to the new one.'}</div></div>
        ${offer ? html`<button class="btn" id="s-untransfer">Cancel the offer</button>` : html`<button class="btn danger" id="s-transfer">${icon('users', 'sm')} Transfer</button>`}</div>` : ''}
      <div class="list-row"><div class="grow"><b>Delete this repository</b><div class="muted small">Moves it, with its issues and pull requests, to Recently deleted in your settings for ${plural(state.instance?.trash_days ?? 7, 'day')}; after that it's gone for good.</div></div>
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
      const rf = $('#rule-form', el);
      rf.addEventListener('submit', action(rf.querySelector('button'), async (e) => {
        e.preventDefault();
        await send('POST', `${api}/branch-rules`, readRule(rf));
        toast('Rule added');
        navigate(location.pathname, { replace: true, scroll: false });
      }));
      $('#rules', el).addEventListener('click', async (e) => {
        const del = e.target.closest('[data-rule-del]');
        const edit = e.target.closest('[data-rule-edit]');
        if (del) {
          if (!(await confirmDialog({ title: `Remove the rule for ${del.dataset.name}?`, text: 'Those branches lose its protection right away.', ok: 'Remove', danger: true }))) return;
          try { await send('DELETE', `${api}/branch-rules/${del.dataset.ruleDel}`); } catch (x) { return toast(x.message, 'err'); }
          toast('Rule removed');
          navigate(location.pathname, { replace: true, scroll: false });
        } else if (edit) {
          const r = rules.find((x) => String(x.id) === edit.dataset.ruleEdit);
          const saved = await modal({
            title: `Rule for ${r.pattern}`, wide: true,
            body: html`<form id="rule-edit" class="stack">${ruleFields(r)}</form>`,
            foot: html`<button class="btn" data-close>Cancel</button><button class="btn primary" data-ok>Save</button>`,
            onMount(m, close) {
              const f = m.querySelector('#rule-edit');
              const go = action(m.querySelector('[data-ok]'), async () => { await send('PUT', `${api}/branch-rules/${r.id}`, readRule(f)); close(true); });
              m.querySelector('[data-ok]').addEventListener('click', go);
              f.addEventListener('submit', (ev) => { ev.preventDefault(); go(ev); });
            },
          });
          if (saved) { toast('Rule saved'); navigate(location.pathname, { replace: true, scroll: false }); }
        }
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
      const tr = $('#s-transfer', el);
      tr?.addEventListener('click', action(tr, async () => {
        const to = await askRecipient(full);
        if (!to) return;
        await send('POST', `${api}/transfer`, { to, confirm: full });
        toast(`Offered ${full} to ${to}`);
        navigate(location.pathname, { replace: true, scroll: false });
      }));
      const untr = $('#s-untransfer', el);
      untr?.addEventListener('click', action(untr, async () => {
        await send('DELETE', `${api}/transfer`);
        toast('Offer cancelled');
        navigate(location.pathname, { replace: true, scroll: false });
      }));
      const del = $('#s-del', el);
      del.addEventListener('click', action(del, async () => {
        const ok = await confirmDialog({ title: `Delete ${full}?`, text: `It moves to Recently deleted (Settings → Repositories), where you can restore it for ${plural(state.instance?.trash_days ?? 7, 'day')}. After that it and its issues and pull requests are gone for good.`, ok: 'Delete this repository', danger: true, typed: full });
        if (!ok) return;
        await send('DELETE', api, { confirm: full });
        toast(`Deleted ${full}`);
        navigate('/' + meta.owner);
      }));
    },
  };
}
