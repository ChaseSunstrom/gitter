// CI test reports on pull requests: a summary line (tests, failures, flaky,
// coverage) and failed tests as annotations on the changed lines they name.
import { html, get, plural } from './lib.js';
import { icon } from './icons.js';

/** Put the failed tests that name a line of this file's new side under that line. */
export function annotate(f, body, mode, list) {
  if (!body || !list?.length) return;
  const path = f.status === 'deleted' ? f.old_path : f.new_path;
  list.forEach((a, i) => {
    if (a.path !== path || !Number.isInteger(a.line) || body.querySelector(`tr[data-ann="${i}"]`)) return;
    const tr = body.querySelector(`td.dn[data-side="new"][data-n="${a.line}"]`)?.closest('tr');
    if (!tr) return;
    const row = document.createElement('tr');
    row.className = 'ci-ann';
    row.dataset.ann = String(i);
    row.innerHTML = html`<td colspan="${mode === 'split' ? 4 : 3}"><div class="ci-ann-box">
      <div class="row">${icon('x', 'sm')}<b class="grow ellipsis">${a.title}</b><span class="muted small">${a.job}</span></div>
      ${a.message ? html`<pre>${a.message}</pre>` : ''}</div></td>`.s;
    tr.after(row);
  });
}

/**
 * Fill `#pr-ci-report` under `el` with the head commit's newest run report and
 * annotate the diff (when `files` is given). Returns the annotation list holder
 * that `annotate` reads, so a diff's decorate hook can use it as rows render.
 */
export function prReport(el, { api, base, prBase, sha, tab, files }) {
  const holder = { list: [] };
  get(`${api}/ci/report?sha=${sha}`, { fresh: true }).then((r) => {
    const slot = el.querySelector('#pr-ci-report');
    if (slot && r.run && (r.tests || r.coverage != null)) {
      slot.innerHTML = html`<div class="box box-body row wrap-ok small">${icon('check', 'sm')}
        <span>CI run <a href="${base}/ci/${r.run}">#${r.run}</a>:</span>
        ${r.tests ? html`<span>${plural(r.tests, 'test')}</span>${r.failed ? html`<span class="c-red">${r.failed} failed</span>` : html`<span class="c-green">all passed</span>`}` : ''}
        ${r.flaky ? html`<span class="badge ci-flaky">${r.flaky} flaky</span>` : ''}
        ${r.coverage != null ? html`<span class="pill ci-cov" title="Lines covered by the tests">${r.coverage}% coverage</span>` : ''}
        ${r.annotations.length && tab !== 'files' ? html`<a href="${prBase}/files">See the failures in the changed files</a>` : ''}
        ${r.expired && r.failed && !r.annotations.length ? html`<span class="muted">Failure details are deleted ${r.retention_days} days after their job.</span>` : ''}</div>`.s;
    }
    holder.list = r.annotations || [];
    const diff = el.querySelector('.diff');
    if (!files || !diff) return;
    diff.querySelectorAll('.dfile').forEach((sec) => annotate(files[+sec.dataset.i], sec.querySelector('[data-body]'), diff.dataset.mode, holder.list));
  }).catch(() => {});
  return holder;
}
