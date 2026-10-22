// CI: the runs of a repository, and one run with its jobs and live, folded logs.
import { html, raw, esc, get, send, toast, action, timeEl, short, $, $$ } from '../lib.js';
import { icon } from '../icons.js';
import { loadRepo, repoUrl, repoPage, mountRepoHeader, ciIcon, CI_LABEL as LABEL } from '../repohead.js';
import { navigate } from '../main.js';

const DONE = new Set(['success', 'failure', 'error', 'cancelled', 'skipped']);
const EVENT = { push: 'Push', pull_request: 'Pull request', manual: 'Run by hand', schedule: 'Schedule' };

/** "1m 05s" between two unix times (or until now). */
function dur(a, b) {
  if (!a) return '';
  const s = Math.max(0, (b || Math.floor(Date.now() / 1000)) - a);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  return m < 60 ? `${m}m ${String(s % 60).padStart(2, '0')}s` : `${Math.floor(m / 60)}h ${String(m % 60).padStart(2, '0')}m`;
}

const branchOf = (ref) => String(ref || '').replace(/^refs\/heads\//, '');

// ---------- ANSI colours ----------
const FG = { 30: 'k', 31: 'r', 32: 'g', 33: 'y', 34: 'b', 35: 'm', 36: 'c', 37: 'w', 90: 'k', 91: 'r', 92: 'g', 93: 'y', 94: 'b', 95: 'm', 96: 'c', 97: 'w' };
const SEQ = /\x1b\[([0-9;]*)m|\x1b\[[0-9;?]*[A-Za-z]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)?|\x1b[@-_]/g;

/** Terminal output as safe HTML: colours and bold kept, cursor tricks dropped, \r progress lines collapsed. */
function ansi(text) {
  text = text.split('\n').map((l) => {
    const t = l.endsWith('\r') ? l.slice(0, -1) : l;
    const i = t.lastIndexOf('\r');
    return i >= 0 ? t.slice(i + 1) : t;
  }).join('\n');
  let out = '', fg = null, bold = false, dim = false, last = 0;
  const put = (s) => {
    if (!s) return;
    const c = [fg && `a-${fg}`, bold && 'a-bold', dim && 'a-dim'].filter(Boolean).join(' ');
    out += c ? `<span class="${c}">${esc(s)}</span>` : esc(s);
  };
  for (const m of text.matchAll(SEQ)) {
    put(text.slice(last, m.index));
    last = m.index + m[0].length;
    if (m[1] === undefined) continue;
    for (const p of (m[1] || '0').split(';').map(Number)) {
      if (p === 0) { fg = null; bold = false; dim = false; }
      else if (p === 1) bold = true;
      else if (p === 2) dim = true;
      else if (p === 22) { bold = false; dim = false; }
      else if (p === 39) fg = null;
      else if (FG[p]) fg = FG[p];
    }
  }
  put(text.slice(last));
  return out;
}

// ---------- the list of runs ----------
async function listView(meta, q) {
  const api = repoUrl(meta.owner, meta.name);
  const p = new URLSearchParams({ page: q.get('page') || '1' });
  if (q.get('branch')) p.set('branch', q.get('branch'));
  const d = await get(`${api}/ci/runs?${p}`, { fresh: true });
  const base = `/${meta.owner}/${meta.name}`;
  const page = +(q.get('page') || 1);
  const rows = d.runs.map((r) => html`<a class="list-row ci-run" href="${base}/ci/${r.number}">
    ${ciIcon(r.status, 'lg')}
    <div class="grow"><div class="ci-title ellipsis">${r.title || '(no message)'}</div>
      <div class="muted small row wrap-ok"><b>#${r.number}</b><span>${EVENT[r.event] || r.event}</span>
        <span class="mono ci-ref">${icon('branch', 'sm')}${branchOf(r.ref)}</span><span class="mono">${short(r.sha)}</span>
        ${r.pr_number ? html`<span>PR #${r.pr_number}</span>` : ''}${r.actor ? html`<span>by ${r.actor}</span>` : ''}</div></div>
    <div class="ci-when muted small">${timeEl(r.created_at)}${r.started_at ? html`<div>${icon('clock', 'sm')} ${dur(r.started_at, r.finished_at)}</div>` : ''}</div>
  </a>`);
  const sample = 'jobs:\n  test:\n    steps:\n      - run: make test\n';
  const empty = html`<div class="box empty">${icon('zap', 'big')}<h3>No runs yet</h3>
    <p>Add <code>.gitter/ci.yml</code> (or just a <code>.gitter/ci.sh</code> script) and push: every push and pull request runs it.</p>
    <pre class="setup-cmd" style="text-align:left">${sample}</pre></div>`;
  const body = html`<div class="stack">
    ${d.enabled ? '' : html`<div class="box box-body row small">${icon('shield', 'sm')}<span class="grow">CI is turned off for this repository's owner on this site. A site admin can change that in Site administration → Settings.</span></div>`}
    <div class="toolbar"><h2 style="font-size:17px" class="grow">CI runs</h2>
      ${q.get('branch') ? html`<a class="pill" href="${base}/ci">${icon('branch', 'sm')} ${q.get('branch')} ${icon('x', 'sm')}</a>` : ''}
      ${d.can_run ? html`<form id="run-form" class="row"><select class="select sm" name="branch" aria-label="Branch to run">${(meta.branches || []).map((b) => html`<option ${b === meta.default_branch ? 'selected' : ''}>${b}</option>`)}</select>
        <button class="btn sm primary" type="submit">${icon('play', 'sm')} Run</button></form>` : ''}</div>
    ${d.runs.length ? html`<div class="box list">${rows}</div>` : empty}
    ${page > 1 || page * 30 < d.total ? html`<div class="pager">
      ${page > 1 ? html`<a class="btn" href="${base}/ci?page=${page - 1}">${icon('chev-left', 'sm')} Newer</a>` : ''}
      ${page * 30 < d.total ? html`<a class="btn" href="${base}/ci?page=${page + 1}">Older ${icon('chev-right', 'sm')}</a>` : ''}</div>` : ''}
  </div>`;
  const live = d.runs.some((r) => !DONE.has(r.status));
  let timer;
  return {
    title: `CI · ${meta.owner}/${meta.name}`,
    html: repoPage(meta, 'ci', body),
    mount(el) {
      mountRepoHeader(el, meta);
      const f = $('#run-form', el);
      f?.addEventListener('submit', action(f.querySelector('button'), async () => {
        const r = await send('POST', `${api}/ci/runs`, { branch: f.branch.value });
        navigate(`${base}/ci/${r.number}`);
      }));
      // Keep the list fresh while something is running.
      if (live) timer = setTimeout(() => location.pathname === `${base}/ci` && navigate(location.href, { replace: true, scroll: false }), 4000);
    },
    unmount() { clearTimeout(timer); },
  };
}

// ---------- one run ----------
const SKIPPED = { paths: 'no matching file changed', needs: "a job it needs didn't pass" };

function jobRow(base, run, j, on) {
  const why = j.status === 'skipped' && SKIPPED[j.reason];
  return html`<a class="list-row ci-job ${on ? 'on' : ''}" href="${base}/ci/${run.number}?job=${j.id}" data-job="${j.id}" ${why ? html`title="Skipped: ${why}"` : ''}>
    ${ciIcon(j.status)}<span class="grow ellipsis">${j.name}</span><span class="muted small">${why ? 'skipped' : dur(j.started_at, j.finished_at)}</span></a>`;
}

/** The job's log cut into folds: the set-up output, then one per step (by byte offset). */
function folds(job, bytes) {
  const dec = new TextDecoder();
  const steps = job.steps || [];
  const starts = steps.map((s) => s.offset ?? null);
  const out = [];
  const firstStart = starts.find((s) => s != null) ?? bytes.length;
  out.push({ name: 'Set up job', status: steps.length && steps[0].status === 'pending' && !DONE.has(job.status) ? 'running' : 'success', text: dec.decode(bytes.subarray(0, firstStart)), setup: true });
  steps.forEach((s, i) => {
    const from = starts[i];
    const next = starts.slice(i + 1).find((x) => x != null);
    out.push({ ...s, index: i, text: from == null ? '' : dec.decode(bytes.subarray(from, next ?? bytes.length)) });
  });
  return out;
}

const MAX_FOLD = 2 << 20;

function foldHtml(f, job, open) {
  const t = f.text.length > MAX_FOLD ? f.text.slice(-MAX_FOLD) : f.text;
  return html`<details class="ci-fold" data-fold="${f.setup ? 'setup' : f.index}" ${open ? 'open' : ''}>
    <summary>${icon('chev-right', 'sm chev')}${ciIcon(f.status)}<span class="grow">${f.name}</span>
      <span class="muted small">${f.started_at ? dur(f.started_at, f.finished_at || (DONE.has(job.status) ? job.finished_at : null)) : ''}</span></summary>
    ${f.text.length > MAX_FOLD ? html`<div class="muted small ci-cut">Showing the last 2 MB of this step.</div>` : ''}
    <pre class="ci-log">${raw(ansi(t))}</pre></details>`;
}

async function runView(meta, number, q) {
  const api = repoUrl(meta.owner, meta.name);
  const base = `/${meta.owner}/${meta.name}`;
  let run = await get(`${api}/ci/runs/${number}`, { fresh: true });
  const pick = () => run.jobs.find((j) => String(j.id) === q.get('job'))
    || run.jobs.find((j) => j.status === 'failure') || run.jobs.find((j) => j.status === 'running') || run.jobs[0];
  let job = pick();
  const head = () => html`<div class="ci-head">
    <div class="row wrap-ok">${ciIcon(run.status, 'lg')}<h1 class="page-h grow">${run.title || `Run #${run.number}`}</h1>
      ${run.can_write && !DONE.has(run.status) ? html`<button class="btn sm danger" id="ci-cancel">${icon('stop', 'sm')} Cancel</button>` : ''}
      ${run.can_write && DONE.has(run.status) && run.jobs.length ? html`${run.status !== 'success' ? html`<button class="btn sm" id="ci-rerun">${icon('history', 'sm')} Re-run failed</button>` : ''}
        <button class="btn sm" id="ci-rerun-all">${icon('history', 'sm')} Re-run all</button>` : ''}</div>
    <div class="muted small row wrap-ok"><b>${LABEL[run.status] || run.status}</b><span>#${run.number}</span><span>${EVENT[run.event] || run.event}</span>
      <a class="mono ci-ref" href="${base}/ci?branch=${encodeURIComponent(branchOf(run.ref))}">${icon('branch', 'sm')}${branchOf(run.ref)}</a>
      <a class="mono" href="${base}/commit/${run.sha}">${short(run.sha)}</a>
      ${run.pr_number ? html`<a href="${base}/pull/${run.pr_number}">PR #${run.pr_number}</a>` : ''}
      ${run.actor ? html`<span>by ${run.actor}</span>` : ''}<span>${timeEl(run.created_at)}</span>
      ${run.started_at ? html`<span>${icon('clock', 'sm')} ${dur(run.started_at, run.finished_at)}</span>` : ''}</div>
    ${run.error ? html`<div class="box box-body ci-error">${icon('x', 'sm')} <b>The pipeline couldn't start:</b> <span class="mono">${run.error}</span></div>` : ''}
  </div>`;
  const jobHead = () => job ? html`<div class="box-head row wrap-ok">${ciIcon(job.status)}<b class="grow">${job.name}</b>
    ${job.image ? html`<span class="pill mono">${job.image}</span>` : ''}
    ${job.runner ? html`<span class="muted small">on ${job.runner}</span>` : ''}
    ${job.attempt > 1 ? html`<span class="muted small">attempt ${job.attempt}</span>` : ''}
    ${job.artifact_size != null ? html`<a class="btn sm" href="/api${api}/ci/jobs/${job.id}/artifacts" download>${icon('download', 'sm')} Artifacts</a>` : ''}
    <a class="btn sm ghost" href="/api${api}/ci/jobs/${job.id}/log" target="_blank" rel="noopener">${icon('file', 'sm')} Raw log</a></div>` : '';
  const body = html`<div class="stack">
    <a class="muted small ci-back" href="${base}/ci">${icon('arrow-left', 'sm')} All runs</a>
    <div id="ci-head">${head()}</div>
    ${run.jobs.length ? html`<div class="ci-split">
      <aside class="box list" id="ci-jobs">${run.jobs.map((j) => jobRow(base, run, j, j === job))}</aside>
      <section class="box ci-job-box" id="ci-job">${jobHead()}<div id="ci-folds"><div class="box-body muted small">Loading the log…</div></div></section>
    </div>` : ''}
  </div>`;

  // The log as received: whole chunks, joined only when drawn (cheap even near the 16 MB cap).
  let chunks = [], size = 0, bytes = new Uint8Array(0);
  let es = null, poll = null, gone = false, drawTimer = null, seq = 0;
  const enc = new TextEncoder();
  const joined = () => {
    if (bytes.length === size) return bytes;
    const all = new Uint8Array(size);
    let at = 0;
    for (const c of chunks) { all.set(c, at); at += c.length; }
    chunks = [all];
    return (bytes = all);
  };
  const renderFolds = (el) => {
    const box = $('#ci-folds', el);
    if (!box || !job) return;
    const open = new Set($$('details[open]', box).map((d) => d.dataset.fold));
    const fs = folds(job, joined());
    const first = !box.querySelector('details');
    box.innerHTML = fs.map((f) => {
      const key = f.setup ? 'setup' : String(f.index);
      const auto = f.status === 'running' || f.status === 'failure';
      return foldHtml(f, job, first ? auto : open.has(key) || auto).s;
    }).join('') || '<div class="box-body muted small">No output.</div>';
  };
  // Live output redraws at most five times a second.
  const draw = (el) => { if (!drawTimer) drawTimer = setTimeout(() => { drawTimer = null; renderFolds(el); }, 200); };
  const renderJobHead = (el) => {
    const old = $('#ci-job .box-head', el);
    if (!old) return;
    const t = document.createElement('div');
    t.innerHTML = jobHead().s;
    if (t.firstElementChild) old.replaceWith(t.firstElementChild);
  };
  const stream = (el) => {
    es?.close();
    es = null;
    if (!job || gone || DONE.has(job.status)) return;
    const mine = seq;
    // Event ids are byte offsets: a reconnect (by the browser or below) carries on from what we have.
    es = new EventSource(`/api${api}/ci/jobs/${job.id}/log/stream?offset=${size}`);
    es.addEventListener('data', (e) => {
      if (mine !== seq) return;
      const chunk = enc.encode(JSON.parse(e.data));
      chunks.push(chunk);
      size += chunk.length;
      draw(el);
    });
    es.addEventListener('status', (e) => { if (DONE.has(e.data)) { es?.close(); es = null; } });
    es.onerror = () => {
      es?.close();
      es = null;
      if (mine === seq && !gone) setTimeout(() => mine === seq && !es && stream(el), 2000);
    };
  };
  const loadJob = async (el) => {
    const mine = ++seq;
    chunks = []; size = 0; bytes = new Uint8Array(0);
    if (!job) return;
    const r = await fetch(`/api${api}/ci/jobs/${job.id}/log`, { credentials: 'same-origin' });
    if (mine !== seq) return;
    if (!r.ok) { $('#ci-folds', el).innerHTML = html`<div class="box-body muted small">Couldn't load the log (${r.status}).</div>`.s; return; }
    const first = new Uint8Array(await r.arrayBuffer());
    if (mine !== seq) return;
    chunks = [first]; size = first.length;
    renderFolds(el);
    stream(el);
  };
  const refresh = async (el) => {
    if (gone) return;
    try { run = await get(`${api}/ci/runs/${number}`, { fresh: true }); } catch { poll = setTimeout(() => refresh(el), 4000); return; }
    const was = job?.status;
    job = run.jobs.find((j) => j.id === job?.id) || pick();
    $('#ci-head', el).innerHTML = head().s;
    const list = $('#ci-jobs', el);
    if (list) list.innerHTML = run.jobs.map((j) => jobRow(base, run, j, j === job).s).join('');
    renderJobHead(el);
    // A job that was waiting has started: follow its output.
    if (was !== job?.status && !es) stream(el);
    renderFolds(el);
    if (!DONE.has(run.status)) poll = setTimeout(() => refresh(el), 2000);
  };
  return {
    title: `Run #${run.number} · CI · ${meta.owner}/${meta.name}`,
    html: repoPage(meta, 'ci', body),
    mount(el) {
      mountRepoHeader(el, meta);
      loadJob(el);
      if (!DONE.has(run.status)) poll = setTimeout(() => refresh(el), 2000);
      el.addEventListener('click', async (e) => {
        const a = e.target.closest('[data-job]');
        if (a) {
          e.preventDefault();
          job = run.jobs.find((j) => String(j.id) === a.dataset.job);
          history.replaceState(history.state, '', a.getAttribute('href'));
          $$('#ci-jobs .ci-job', el).forEach((x) => x.classList.toggle('on', x === a));
          renderJobHead(el);
          $('#ci-folds', el).innerHTML = '';
          es?.close(); es = null;
          await loadJob(el);
          return;
        }
        const act = e.target.closest('#ci-cancel, #ci-rerun, #ci-rerun-all');
        if (!act) return;
        await action(act, async () => {
          if (act.id === 'ci-cancel') { await send('POST', `${api}/ci/runs/${number}/cancel`); toast('Run cancelled'); }
          else { await send('POST', `${api}/ci/runs/${number}/rerun`, { all: act.id === 'ci-rerun-all' }); toast('Running again'); }
          navigate(location.pathname, { replace: true, scroll: false });
        })(e);
      });
    },
    unmount() { gone = true; seq++; es?.close(); clearTimeout(poll); clearTimeout(drawTimer); },
  };
}

export default async function (ctx) {
  const meta = await loadRepo(ctx.params.owner, ctx.params.repo);
  return ctx.params.n ? runView(meta, ctx.params.n, ctx.query) : listView(meta, ctx.query);
}

