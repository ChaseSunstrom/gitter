// Security tab: secrets push protection refused (and allowing one once), the
// secret scan of the default branch (admins), dependencies, vulnerabilities,
// SBOM downloads, and the site admin's OSV import.
import { html, state, get, send, modal, action, toast, timeEl, plural, short, enc, encPath, $, invalidate } from '../lib.js';
import { icon } from '../icons.js';
import { loadRepo, repoUrl, repoPage, mountRepoHeader } from '../repohead.js';
import { refresh } from '../main.js';

const REASONS = [['false_positive', "It's a false positive"], ['test', "It's a test value"], ['later', "I'll revoke it"]];
const SHOWN = 500;

function findingRow(meta, f, extra = '') {
  const base = `/${meta.owner}/${meta.name}`;
  const at = f.commit ? html`<a class="mono small" href="${base}/commit/${f.commit}">${short(f.commit)}</a>` : '';
  const file = f.commit ? html`<a class="mono" href="${base}/blob/${f.commit}/${encPath(f.path)}#L${f.line}">${f.path}:${f.line}</a>` : html`<span class="mono">${f.path}:${f.line}</span>`;
  return html`<div class="list-row">${icon('key', 'sm muted')}<span class="grow stack" style="gap:2px">${file}
    <span class="small muted">${f.kind} · <span class="mono">${f.masked}</span></span></span>${at}${extra}</div>`;
}

const reasonPicker = (name) => html`<div class="choice" role="radiogroup" aria-label="Why it's safe to allow">${REASONS.map(([k, label], i) => html`
  <label><input type="radio" name="${name}" value="${k}" ${i === 0 ? 'checked' : ''}><span>${label}</span></label>`)}</div>`;

/**
 * Asks to allow findings push protection refused, once each. Resolves true when allowed.
 * Used here and by the editor when a web commit is refused.
 */
export function allowDialog(meta, findings) {
  return modal({
    title: findings.length === 1 ? 'This looks like a secret' : `These look like ${findings.length} secrets`,
    body: html`<div class="stack"><p class="muted">Committing a secret leaks it to everyone who can read this repository, and removing it later doesn't take it out of the history. Take it out, or allow it once if it's safe.</p>
      <div class="box list">${findings.map((f) => findingRow(meta, f))}</div>${reasonPicker('al-why')}</div>`,
    foot: html`<button class="btn" data-close>Cancel</button><button class="btn danger" data-ok>${icon('shield', 'sm')} Allow once</button>`,
    onMount(el, close) {
      const ok = $('[data-ok]', el);
      ok.addEventListener('click', action(ok, async () => {
        const reason = $('input[name="al-why"]:checked', el).value;
        await send('POST', `${repoUrl(meta.owner, meta.name)}/security/bypasses`, { fingerprints: findings.map((f) => f.fingerprint), reason });
        close(true);
      }));
    },
  });
}

function blockedBox(meta, blocked, wanted) {
  const shown = wanted.length ? blocked.filter((b) => wanted.includes(b.fingerprint)) : blocked;
  if (!shown.length) return '';
  const waiting = shown.filter((b) => !b.allowed);
  return html`<div class="box" id="sec-blocked"><div class="box-head">${icon('shield', 'sm')}<b>Push protection refused ${plural(shown.length, 'secret')}</b></div>
    <div class="box-body stack"><p class="muted small">Only you see these. Take the secret out of your commits and push again, or allow it once: your next push or commit with it goes through (within 3 hours).</p>
      <div class="box list">${shown.map((b) => findingRow(meta, b, b.allowed ? html`<span class="pill">${icon('check', 'sm')} Allowed once</span>` : html`<span class="muted small nowrap">${timeEl(b.created_at)}</span>`))}</div>
      ${waiting.length ? html`${reasonPicker('sec-why')}<div class="row"><button class="btn danger" id="sec-allow" data-fps="${waiting.map((b) => b.fingerprint).join(',')}">${icon('shield', 'sm')} Allow ${waiting.length === 1 ? 'it' : `these ${waiting.length}`} once</button></div>` : ''}
    </div></div>`;
}

function scanBox(meta, scan) {
  const findings = scan?.findings || [];
  return html`<div class="box"><div class="box-head">${icon('key', 'sm')}<b>Secret scanning</b><span class="grow"></span>
      <button class="btn sm" id="sec-scan" ${meta.empty ? 'disabled' : ''}>${icon('search', 'sm')} Scan ${meta.default_branch} now</button></div>
    ${!scan ? html`<div class="box-body muted small">Scan every file on ${meta.default_branch} for keys, tokens and passwords. Only repository admins see the results.</div>`
      : html`<div class="box-body muted small">Scanned <a class="mono" href="/${meta.owner}/${meta.name}/commit/${scan.commit}">${short(scan.commit)}</a> ${timeEl(scan.scanned_at)}: ${findings.length ? plural(findings.length, 'possible secret') : 'nothing found'}.${scan.truncated ? ' The branch is large: only part of it was scanned.' : ''}</div>
        ${findings.length ? html`<div class="list" style="border-top:1px solid var(--line)">${findings.map((f) => findingRow(meta, f))}</div>` : ''}`}
  </div>`;
}

function depsBox(meta, d) {
  const deps = d.dependencies;
  const api = `/api${repoUrl(meta.owner, meta.name)}/sbom`;
  return html`<div class="box"><div class="box-head">${icon('package', 'sm')}<b>Dependencies</b><span class="grow"></span>
      ${d.commit ? html`<a class="btn sm" href="${api}?format=spdx" download>${icon('download', 'sm')} SPDX</a><a class="btn sm" href="${api}?format=cyclonedx" download>${icon('download', 'sm')} CycloneDX</a>` : ''}</div>
    ${!deps.length ? html`<div class="empty small"><h3>No dependencies found</h3><p>Lockfiles on ${meta.default_branch} are read: Cargo.lock, package-lock.json, yarn.lock, pnpm-lock.yaml, go.sum, requirements.txt, poetry.lock and Gemfile.lock.</p></div>`
      : html`<div class="box-body muted small">${plural(deps.length, 'package')} from ${d.files.map((f, i) => html`${i ? ', ' : ''}<a class="mono" href="/${meta.owner}/${meta.name}/blob/${encPath(meta.default_branch)}/${encPath(f.path)}">${f.path}</a>`)}.</div>
        <div class="list" style="border-top:1px solid var(--line)">${deps.slice(0, SHOWN).map((x) => html`<div class="list-row"><span class="grow mono">${x.name}</span><span class="mono muted">${x.version}</span><span class="muted small nowrap hide-sm">${x.ecosystem}</span></div>`)}
          ${deps.length > SHOWN ? html`<div class="list-row muted small">…and ${plural(deps.length - SHOWN, 'more')}: download an SBOM for the full list.</div>` : ''}</div>`}
  </div>`;
}

function vulnsBox(d) {
  const v = d.vulnerabilities;
  const sev = (s) => html`<span class="pill ${s === 'CRITICAL' || s === 'HIGH' ? 'closed' : ''}">${s ? s.toLowerCase() : 'unrated'}</span>`;
  const body = !d.osv.advisories
    ? html`<div class="box-body muted small">No vulnerability database yet. A site admin imports one from <a href="https://osv.dev" rel="noopener">OSV</a> (it works offline).</div>`
    : !v.length ? html`<div class="box-body muted small">None of these dependencies has a known vulnerability (${plural(d.osv.advisories, 'advisory', 'advisories')} checked).</div>`
    : html`<div class="list">${v.map((x) => html`<div class="list-row">${icon('flag', 'sm muted')}<span class="grow stack" style="gap:2px"><span><b class="mono">${x.id}</b> ${x.summary}</span>
        <span class="small muted"><span class="mono">${x.name} ${x.version}</span> · ${x.ecosystem} · ${x.path}${x.fixed ? html` · fixed in <span class="mono">${x.fixed}</span>` : ''}</span></span>${sev(x.severity)}</div>`)}</div>`;
  return html`<div class="box"><div class="box-head">${icon('flag', 'sm')}<b>Vulnerabilities</b><span class="grow"></span>${d.osv.advisories ? html`<span class="muted small">${plural(v.length, 'alert')}</span>` : ''}</div>${body}</div>`;
}

const osvBox = () => html`<div class="box"><div class="box-head">${icon('upload', 'sm')}<b>Vulnerability database</b><span class="muted small">site admin</span></div>
  <div class="box-body stack"><p class="muted small">Import OSV advisories: a JSON file, a JSON array, or a zip from osv.dev (one per ecosystem, e.g. crates.io/all.zip). Importing again updates them.</p>
    <div class="row"><input type="file" id="sec-osv" accept=".json,.zip,application/json,application/zip" aria-label="OSV file"><button class="btn sm" id="sec-osv-go">${icon('upload', 'sm')} Import</button></div></div></div>`;

export default async function (ctx) {
  const { owner, repo } = ctx.params;
  const meta = await loadRepo(owner, repo);
  const [sec, d] = await Promise.all([get(`${repoUrl(owner, repo)}/security`, { fresh: true }), get(`${repoUrl(owner, repo)}/dependencies`, { fresh: true })]);
  const wanted = (ctx.query.get('allow') || '').split(',').filter(Boolean);
  const body = html`<div class="stack">
    <div class="toolbar"><h1 class="page-h grow">${icon('shield')} Security</h1></div>
    ${wanted.length && !state.me ? html`<div class="box"><div class="box-body">${icon('shield', 'sm')} <a href="/login?next=${enc(ctx.path + location.search)}">Sign in</a> as the person who pushed to allow a secret.</div></div>` : ''}
    ${wanted.length && state.me && !sec.blocked.some((b) => wanted.includes(b.fingerprint)) ? html`<div class="box"><div class="box-body muted">Nothing push protection refused for you matches that link. It lasts a week, and only the person who pushed can allow it.</div></div>` : ''}
    ${blockedBox(meta, sec.blocked, wanted)}
    ${sec.can_admin ? scanBox(meta, sec.scan) : ''}
    ${vulnsBox(d)}
    ${depsBox(meta, d)}
    ${state.me?.is_admin ? osvBox() : ''}
  </div>`;
  const mount = (el) => {
    mountRepoHeader(el, meta);
    const allow = $('#sec-allow', el);
    allow?.addEventListener('click', action(allow, async () => {
      const reason = $('input[name="sec-why"]:checked', el).value;
      await send('POST', `${repoUrl(owner, repo)}/security/bypasses`, { fingerprints: allow.dataset.fps.split(','), reason });
      toast('Allowed once: push again within 3 hours');
      refresh();
    }));
    const scan = $('#sec-scan', el);
    scan?.addEventListener('click', action(scan, async () => {
      await send('POST', `${repoUrl(owner, repo)}/security/scan`);
      toast('Scan finished');
      refresh();
    }));
    const go = $('#sec-osv-go', el);
    go?.addEventListener('click', action(go, async () => {
      const file = $('#sec-osv', el).files[0];
      if (!file) { toast('Pick an OSV file first', 'err'); return; }
      const res = await fetch('/api/admin/osv', { method: 'POST', credentials: 'same-origin', headers: { 'Content-Type': 'application/octet-stream', Accept: 'application/json' }, body: file });
      const data = await res.json().catch(() => ({}));
      if (!res.ok) throw new Error(data.error || `Import failed (${res.status})`);
      toast(`Imported ${plural(data.imported, 'advisory', 'advisories')}`);
      invalidate();
      refresh();
    }));
    if (wanted.length) $('#sec-blocked', el)?.scrollIntoView({ block: 'start' });
  };
  return { title: `Security · ${owner}/${repo}`, html: repoPage(meta, 'security', body), mount };
}
