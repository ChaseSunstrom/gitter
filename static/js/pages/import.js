// Import repositories from GitHub or GitLab with their labels, milestones, issues,
// pull requests, comments and releases, and follow the progress.
import { html, state, get, send, $, $$, action, toast, plural } from '../lib.js';
import { icon } from '../icons.js';

const STAGE = { create: 'Waiting', clone: 'Copying the code', labels: 'Labels', milestones: 'Milestones', issues: 'Issues',
  pulls: 'Pull requests', comments: 'Comments', releases: 'Releases', done: 'Done', failed: 'Failed' };
const KIND = { github: 'GitHub', gitlab: 'GitLab' };
const STATE = { running: 'Running', done: 'Done', failed: 'Failed', cancelled: 'Cancelled' };
const HINT = {
  github: 'A fine-grained token with read access to contents, issues, pull requests and metadata (or a classic token with the repo scope).',
  gitlab: 'A personal access token with the read_api and read_repository scopes.',
};
const clock = (t) => new Date(t * 1000).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });

function jobsView(d) {
  if (!d.jobs.length) return '';
  return html`<h2>Your imports</h2>${d.jobs.map((j) => html`<div class="box import-job" data-job="${j.id}">
    <div class="box-head row wrap-ok"><b>${KIND[j.kind] || j.kind}</b><span class="muted small">${j.base_url} → ${j.owner}</span><span class="grow"></span>
      <span class="pill">${STATE[j.state] || j.state}</span>${j.state === 'running' ? html`<button class="btn sm" data-cancel="${j.id}">Cancel</button>` : ''}</div>
    <div class="list">${j.items.map((i) => {
      const waiting = !['done', 'failed'].includes(i.stage) && i.waiting_until * 1000 > Date.now();
      return html`<div class="list-row">${icon('repo', 'sm muted')}<div class="grow">
        ${i.repo ? html`<a href="/${j.owner}/${i.repo}"><b>${j.owner}/${i.repo}</b></a>` : html`<b>${i.name}</b>`} <span class="muted small">from ${i.source}</span>
        ${i.error ? html`<div class="small c-red">${i.error}</div>` : ''}
        ${waiting ? html`<div class="small muted">Waiting for ${KIND[j.kind]}'s rate limit, until ${clock(i.waiting_until)}</div>` : ''}</div>
        <span class="pill">${STAGE[i.stage] || i.stage}</span></div>`;
    })}</div></div>`)}`;
}

export default async function () {
  const me = state.me;
  if (!me) return { redirect: '/login?next=/import' };
  const [jobs, orgs] = await Promise.all([get('/imports', { fresh: true }), get('/me/orgs', { fresh: true }).catch(() => ({ orgs: [] }))]);
  const owners = [me.username, ...orgs.orgs.filter((o) => o.can_create).map((o) => o.name)];
  return {
    title: 'Import repositories',
    html: html`<div class="wrap narrow">
      <h1>Import from GitHub or GitLab</h1>
      <p class="muted">Copies repositories with their labels, milestones, issues, pull requests, comments and releases. Issue and pull request numbers stay
        the same (GitLab merge requests are numbered after the issues), and each one notes who wrote it there.</p>
      <form id="imp-form" class="stack" novalidate>
        <div class="grid-2">
          <div class="field"><label for="imp-kind">From</label><select class="select" id="imp-kind" name="kind">
            ${Object.entries(KIND).map(([k, v]) => html`<option value="${k}">${v}</option>`)}</select></div>
          <div class="field"><label for="imp-base">Address</label><input class="input mono" id="imp-base" name="base_url" placeholder="https://github.com" spellcheck="false">
            <span class="hint">For GitHub Enterprise or a self-managed GitLab.</span></div>
        </div>
        <div class="field"><label for="imp-token">Access token</label><input class="input mono" id="imp-token" name="token" type="password" autocomplete="off" required>
          <span class="hint"><span id="imp-hint">${HINT.github}</span> Kept encrypted and deleted when the import ends.</span></div>
        <div class="grid-2">
          <div class="field"><label for="imp-source">User, organization or group</label><input class="input" id="imp-source" name="source" spellcheck="false" required></div>
          <div class="field"><label for="imp-owner">Import into</label><select class="select" id="imp-owner" name="owner">
            ${owners.map((o) => html`<option value="${o}">${o}</option>`)}</select></div>
        </div>
        ${me.is_admin ? html`<label class="check small"><input type="checkbox" name="allow_private"> Allow private addresses (a server on your own network)</label>` : ''}
        <p class="field-err" id="imp-err" role="alert" hidden></p>
        <div class="row"><button class="btn" type="button" id="imp-list">${icon('search', 'sm')} List repositories</button></div>
        <div id="imp-repos"></div>
      </form>
      <div id="imp-jobs" class="stack" style="margin-top:28px">${jobsView(jobs)}</div>
    </div>`,
    mount(el) {
      const form = $('#imp-form', el), err = $('#imp-err', el), box = $('#imp-repos', el), list = $('#imp-jobs', el);
      const fail = (m) => { err.textContent = m; err.hidden = !m; };
      const body = () => {
        const f = Object.fromEntries(new FormData(form));
        return { kind: f.kind, base_url: f.base_url.trim(), token: f.token.trim(), source: f.source.trim(), owner: f.owner, allow_private: !!form.allow_private?.checked };
      };
      $('#imp-kind', el).addEventListener('change', (e) => {
        $('#imp-base', el).placeholder = e.target.value === 'gitlab' ? 'https://gitlab.com' : 'https://github.com';
        $('#imp-hint', el).textContent = HINT[e.target.value];
      });
      let timer = null;
      const refreshJobs = async () => {
        const d = await get('/imports', { fresh: true }).catch(() => null);
        if (!d || !el.isConnected) return;
        const v = jobsView(d);
        list.innerHTML = v.s ?? v;
        clearTimeout(timer);
        if (d.jobs.some((j) => j.state === 'running')) timer = setTimeout(refreshJobs, 3000);
      };
      if (jobs.jobs.some((j) => j.state === 'running')) timer = setTimeout(refreshJobs, 3000);
      $('#imp-list', el).addEventListener('click', action($('#imp-list', el), async () => {
        fail('');
        const b = body();
        if (!b.token || !b.source) return fail('Enter an access token and who to import from');
        let r;
        try { r = await send('POST', '/imports/preview', b); } catch (x) { return fail(x.message); }
        if (!r.repos.length) { box.innerHTML = html`<p class="muted">${b.source} has no repositories this token can see.</p>`.s; return; }
        box.innerHTML = html`<div class="box"><div class="box-head row"><label class="check"><input type="checkbox" id="imp-all" checked> <b>${plural(r.repos.length, 'repository', 'repositories')}</b></label></div>
          <div class="list">${r.repos.map((x) => html`<label class="list-row check"><input type="checkbox" name="repo" value="${x.source}" checked>
            <span class="grow"><b>${x.source}</b>${x.private ? html` <span class="pill">Private</span>` : ''}${x.description ? html`<span class="muted small" style="display:block">${x.description}</span>` : ''}</span></label>`)}</div></div>
          ${r.truncated ? html`<p class="small muted">Only the first ${r.max} are listed. Import these, then the rest in another run.</p>` : ''}
          <div class="row" style="margin-top:12px"><button class="btn primary" type="submit" id="imp-go">Import</button></div>`.s;
        $('#imp-all', box).addEventListener('change', (e) => $$('input[name=repo]', box).forEach((c) => { c.checked = e.target.checked; }));
      }));
      form.addEventListener('submit', async (e) => {
        e.preventDefault();
        const go = $('#imp-go', el);
        if (!go) return;
        const repos = $$('input[name=repo]:checked', box).map((c) => c.value);
        if (!repos.length) return fail('Pick at least one repository');
        await action(go, async () => {
          fail('');
          try { await send('POST', '/imports', { ...body(), repos }); } catch (x) { return fail(x.message); }
          toast(`Importing ${plural(repos.length, 'repository', 'repositories')}`);
          box.innerHTML = '';
          $('#imp-token', el).value = '';
          refreshJobs();
        })(e);
      });
      list.addEventListener('click', async (e) => {
        const b = e.target.closest('[data-cancel]');
        if (!b) return;
        try { await send('POST', `/imports/${b.dataset.cancel}/cancel`); toast('Import cancelled'); } catch (x) { toast(x.message, 'err'); }
        refreshJobs();
      });
    },
  };
}
