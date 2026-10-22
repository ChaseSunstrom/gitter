// Deployments: each environment (production, staging, a pull request's preview) with what's
// deployed there now and before, and for admins its URL, branch rule and secrets.
import { html, get, send, timeEl, short, enc, safeUrl, plural, $, toast, confirmDialog, action, invalidate } from '../lib.js';
import { icon } from '../icons.js';
import { loadRepo, repoUrl, repoPage, mountRepoHeader } from '../repohead.js';
import { navigate } from '../main.js';

const DEP = {
  success: ['check', 'ok', 'Deployed'], failure: ['x', 'bad', 'Failed'], in_progress: ['pulse', 'run', 'Deploying'],
  queued: ['clock', 'wait', 'Queued'], inactive: ['minus', 'off', 'Inactive'],
};

export function depIcon(status) {
  const [ic, k, label] = DEP[status] || DEP.queued;
  return html`<span class="ci-st ${k}" title="${label}" aria-label="${label}">${icon(ic, 'sm')}</span>`;
}

/** The link a deployment offers: its preview, or the URL it's at. */
export function depLink(d, label = 'Open') {
  if (d.preview) return html`<a class="btn sm" href="${d.preview}" target="_blank" rel="noopener noreferrer">${icon('eye', 'sm')} Open preview</a>`;
  return d.url && d.status !== 'inactive' ? html`<a class="btn sm" href="${safeUrl(d.url)}" target="_blank" rel="noopener noreferrer">${icon('globe', 'sm')} ${label}</a>` : '';
}

function row(base, d) {
  const by = d.job_id && d.run_number ? html`<a href="${base}/ci/${d.run_number}">CI run #${d.run_number}</a>` : d.creator ? html`by <a href="/${enc(d.creator)}">${d.creator}</a>` : '';
  return html`<div class="list-row">${depIcon(d.status)}<div class="grow"><span class="mono">${d.ref}</span>
      <a class="mono small" href="${base}/commit/${d.sha}">${short(d.sha)}</a>${d.pr_number ? html` <a class="small" href="${base}/pull/${d.pr_number}">#${d.pr_number}</a>` : ''}
      <div class="small muted">${by}${by ? ' · ' : ''}${timeEl(d.created_at)}${d.description ? html` · ${d.description}` : ''}</div></div>
    ${depLink(d)}</div>`;
}

const rule = (b) => b ? html`<span class="pill" title="Only these branches (or refs) deploy here">${icon('branch', 'sm')} ${b}</span>` : html`<span class="pill" title="Any branch deploys here">Any branch</span>`;

function envBox(base, e, admin) {
  return html`<div class="box" data-env="${e.id}"><div class="box-head row wrap-ok">
      ${icon('globe', 'sm muted')}<b class="grow">${e.name}</b>${rule(e.branches)}
      ${e.url ? html`<a class="small ellipsis" style="max-width:260px" href="${safeUrl(e.url)}" target="_blank" rel="noopener noreferrer">${e.url}</a>` : ''}</div>
    ${e.latest ? html`<div class="list">${row(base, e.latest)}</div>` : html`<div class="box-body muted small">Nothing deployed here yet.</div>`}
    ${e.latest ? html`<details class="env-history"><summary class="box-body small">${icon('history', 'sm')} History</summary>
      <div class="list" data-history><div class="list-row muted small">Loading…</div></div></details>` : ''}
    ${admin ? html`<details class="env-settings"><summary class="box-body small">${icon('settings', 'sm')} Settings and secrets</summary><div class="box-body stack">
      <form class="row wrap-ok env-edit">
        <input class="input" name="url" type="url" placeholder="https://" value="${e.url}" aria-label="URL" style="max-width:300px">
        <input class="input mono" name="branches" placeholder="Any branch (or: main, release/*, refs/tags/v*)" value="${e.branches}" aria-label="Branches that deploy here" style="max-width:300px">
        <button class="btn" type="submit">Save</button></form>
      <div class="env-secrets stack" data-env-secrets="${e.id}"><span class="muted small">Loading secrets…</span></div>
      <div><button class="btn sm danger" data-env-del="${e.id}" data-env-name="${e.name}">${icon('trash', 'sm')} Delete ${e.name}</button></div>
    </div></details>` : ''}</div>`;
}

function secretsHtml(list) {
  return html`<p class="muted small">Only jobs deploying here get these (and only from branches its rule lets in). Masked in logs; never shown again.</p>
    <form class="row wrap-ok env-sec-form">
      <input class="input mono" name="name" placeholder="NAME" pattern="[A-Za-z_][A-Za-z0-9_]*" maxlength="64" required style="max-width:200px" aria-label="Secret name" autocomplete="off" spellcheck="false">
      <input class="input mono" name="value" type="password" placeholder="Value" required autocomplete="new-password" aria-label="Secret value" style="max-width:260px">
      <button class="btn" type="submit">Save secret</button></form>
    ${list.length ? html`<div class="list">${list.map((s) => html`<div class="list-row">${icon('key', 'sm muted')}<b class="mono grow">${s.name}</b>
      <span class="small muted">${timeEl(s.updated_at)}</span><button class="btn sm danger" data-sec-del="${s.name}">Delete</button></div>`)}</div>` : ''}`;
}

export default async function (ctx) {
  const { owner, repo } = ctx.params;
  const api = repoUrl(owner, repo);
  const [meta, envs] = await Promise.all([loadRepo(owner, repo), get(`${api}/environments`, { fresh: true })]);
  const base = `/${owner}/${repo}`;
  const admin = envs.can_admin;
  const previews = envs.environments.filter((e) => e.name.startsWith('preview/'));
  const named = envs.environments.filter((e) => !e.name.startsWith('preview/'));
  const live = previews.filter((e) => e.latest && e.latest.status !== 'inactive');
  const body = html`<div class="stack">
    <div class="toolbar"><h1 class="page-h grow">${icon('globe')} Deployments</h1>
      ${admin ? html`<button class="btn sm primary" id="env-new-btn">${icon('plus', 'sm')} New environment</button>` : ''}</div>
    ${admin ? html`<form class="box" id="env-new" hidden><div class="box-head"><b>New environment</b></div><div class="box-body stack">
      <div class="field"><label for="env-name">Name</label><input id="env-name" class="input" name="name" required maxlength="64" title="Letters, numbers, '.', '-', '_' and '/'" placeholder="production" style="max-width:300px"></div>
      <div class="field"><label for="env-url">URL</label><input id="env-url" class="input" name="url" type="url" placeholder="https://" style="max-width:400px"></div>
      <div class="field"><label for="env-branches">Branches that deploy here</label><input id="env-branches" class="input mono" name="branches" placeholder="Any branch (or: main, release/*, refs/tags/v*)" style="max-width:400px">
        <span class="hint">A job deploying from any other branch fails before it runs, without this environment's secrets.</span></div>
      <div><button class="btn primary" type="submit">Create environment</button></div></div></form>` : ''}
    ${named.map((e) => envBox(base, e, admin))}
    ${previews.length ? html`<div class="box" id="previews"><div class="box-head"><b class="grow">Pull request previews</b><span class="muted small">${plural(live.length, 'live preview')}</span></div>
      <div class="list">${previews.map((e) => e.latest ? row(base, e.latest) : '')}</div></div>` : ''}
    ${!envs.environments.length ? html`<div class="box empty">${icon('globe', 'big')}<h3>No deployments yet</h3>
      <p>CI jobs deploy to an environment with <code>environment: production</code> in <code>.gitter/ci.yml</code>; give a pull request job <code>preview: dist</code> and Gitter serves that folder at a preview link on the pull request. Deploy tools can record deployments through the API too.</p></div>` : ''}
  </div>`;
  return { title: `Deployments · ${owner}/${repo}`, html: repoPage(meta, 'deployments', body), mount: (el) => {
    mountRepoHeader(el, meta);
    const reload = () => { invalidate(`${api}/`); navigate(location.pathname, { replace: true, scroll: false }); };
    const form = $('#env-new', el);
    $('#env-new-btn', el)?.addEventListener('click', () => { form.hidden = !form.hidden; if (!form.hidden) form.name.focus(); });
    form?.addEventListener('submit', action(form.querySelector('button[type=submit]'), async () => {
      await send('PUT', `${api}/environments`, { name: form.name.value.trim(), url: form.url.value.trim(), branches: form.branches.value.trim() });
      toast(`Created ${form.name.value.trim()}`);
      reload();
    }));
    for (const box of el.querySelectorAll('[data-env]')) {
      const id = box.dataset.env;
      const e = envs.environments.find((x) => String(x.id) === id);
      const edit = $('.env-edit', box);
      edit?.addEventListener('submit', action(edit.querySelector('button'), async () => {
        await send('PUT', `${api}/environments`, { name: e.name, url: edit.url.value.trim(), branches: edit.branches.value.trim() });
        toast(`Saved ${e.name}`);
        reload();
      }));
      const hist = $('.env-history', box);
      hist?.addEventListener('toggle', async () => {
        if (!hist.open || hist.dataset.loaded) return;
        hist.dataset.loaded = '1';
        const list = $('[data-history]', hist);
        try {
          const d = await get(`${api}/deployments?environment=${enc(e.name)}`, { fresh: true });
          const older = d.deployments.filter((x) => x.id !== e.latest?.id);
          list.innerHTML = (older.length ? html`${older.map((x) => row(base, x))}` : html`<div class="list-row muted small">Nothing deployed here before.</div>`).s;
        } catch (x) { list.textContent = x.message; }
      });
      const settings = $('.env-settings', box);
      const sec = $('[data-env-secrets]', box);
      const loadSecrets = async () => {
        const d = await get(`${api}/environments/${id}/secrets`, { fresh: true });
        sec.innerHTML = secretsHtml(d.secrets).s;
        const sf = $('.env-sec-form', sec);
        sf.addEventListener('submit', action(sf.querySelector('button'), async () => {
          const name = sf.name.value.trim().toUpperCase();
          await send('PUT', `${api}/environments/${id}/secrets/${enc(name)}`, { value: sf.value.value });
          toast(`Saved ${name}`);
          await loadSecrets();
        }));
      };
      settings?.addEventListener('toggle', () => { if (settings.open && !sec.dataset.loaded) { sec.dataset.loaded = '1'; loadSecrets().catch((x) => { sec.textContent = x.message; }); } });
      sec?.addEventListener('click', async (ev) => {
        const b = ev.target.closest('[data-sec-del]');
        if (!b) return;
        const name = b.dataset.secDel;
        if (!(await confirmDialog({ title: `Delete ${name}?`, text: `Jobs deploying to ${e.name} will no longer get it.`, ok: 'Delete', danger: true }))) return;
        try { await send('DELETE', `${api}/environments/${id}/secrets/${enc(name)}`); } catch (x) { return toast(x.message, 'err'); }
        toast(`Deleted ${name}`);
        loadSecrets().catch((x) => toast(x.message, 'err'));
      });
      const del = $('[data-env-del]', box);
      del?.addEventListener('click', action(del, async () => {
        if (!(await confirmDialog({ title: `Delete ${e.name}?`, text: 'Its deployment history, secrets and previews go too. Jobs that deploy here make it again, open to every branch.', ok: 'Delete environment', danger: true }))) return;
        await send('DELETE', `${api}/environments/${id}`);
        toast(`Deleted ${e.name}`);
        reload();
      }));
    }
  } };
}
