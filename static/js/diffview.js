// Diff viewer: unified or split, syntax colors, word-level changes, file list,
// collapse and "viewed" state, and lazy rendering so huge diffs stay fast.
import { html, raw, esc, encPath, num, $ } from './lib.js';
import { icon } from './icons.js';
import { ensureLang, highlightOne } from './hl.js';
import { langOf } from './lib.js';
import { scan, warning, revealHidden } from './unicode.js';

const STATUS = { added: ['A', 'c-green', 'Added'], deleted: ['D', 'c-red', 'Deleted'], modified: ['M', 'c-amber', 'Modified'], renamed: ['R', 'c-purple', 'Renamed'], copied: ['C', 'c-purple', 'Copied'] };

function getMode() { try { return localStorage.getItem('diff-mode') || 'unified'; } catch { return 'unified'; } }
function setMode(m) { try { localStorage.setItem('diff-mode', m); } catch { /* ignore */ } }
const viewedKey = (k, p) => `viewed:${k}:${p}`;
function isViewed(k, p) { try { return localStorage.getItem(viewedKey(k, p)) === '1'; } catch { return false; } }
function setViewed(k, p, v) { try { v ? localStorage.setItem(viewedKey(k, p), '1') : localStorage.removeItem(viewedKey(k, p)); } catch { /* ignore */ } }

/** Wrap text offsets [a, b) of a highlighted HTML string in <span class="wd">, splitting around tags. */
export function wrapRange(h, a, b) {
  if (a >= b) return h;
  let out = '', pos = 0, open = false;
  const re = /<[^>]+>|&[#\w]+;|[^<&]/g;
  let m;
  while ((m = re.exec(h))) {
    const t = m[0];
    if (t[0] === '<') {
      if (open) { out += '</span>' + t + '<span class="wd">'; } else out += t;
      continue;
    }
    if (pos === a && !open) { out += '<span class="wd">'; open = true; }
    if (pos === b && open) { out += '</span>'; open = false; }
    out += t;
    pos++;
  }
  if (open) out += '</span>';
  return out;
}

/** Common prefix/suffix between two lines -> changed ranges. */
function changedRange(x, y) {
  let p = 0;
  while (p < x.length && p < y.length && x[p] === y[p]) p++;
  let s = 0;
  while (s < x.length - p && s < y.length - p && x[x.length - 1 - s] === y[y.length - 1 - s]) s++;
  return [[p, x.length - s], [p, y.length - s]];
}

/** Turn a hunk's lines into display rows, pairing deletions with additions for word diffs and split view. */
function pairRows(lines) {
  const rows = [];
  let i = 0;
  while (i < lines.length) {
    const [k] = lines[i];
    if (k === '-') {
      const dels = [];
      while (i < lines.length && lines[i][0] === '-') dels.push(lines[i++]);
      const adds = [];
      while (i < lines.length && lines[i][0] === '+') adds.push(lines[i++]);
      const n = Math.max(dels.length, adds.length);
      for (let j = 0; j < n; j++) rows.push({ del: dels[j], add: adds[j] });
    } else if (k === '+') rows.push({ add: lines[i++] });
    else if (k === '\\') { rows.push({ note: lines[i++][3] }); }
    else rows.push({ ctx: lines[i++] });
  }
  return rows;
}

function hl(text, lang) { return lang ? highlightOne(text, lang) : esc(text); }

function lineHtml(text, lang, range) {
  let h = hl(text, lang);
  if (range && range[1] - range[0] < text.length && range[1] > range[0]) h = wrapRange(h, range[0], range[1]);
  return revealHidden(h) || ' ';
}

/** Table rows for a file's hunks, produced lazily so huge files render a chunk at a time. */
function* rowsOf(f, mode) {
  const lang = langOf(f.new_path || f.old_path);
  for (const h of f.hunks) {
    const head = `@@ -${h.old_start} +${h.new_start} @@ ${esc(h.header)}`;
    yield mode === 'split'
      ? `<tr class="hunk"><td class="dn"></td><td class="dc" colspan="3">${head}</td></tr>`
      : `<tr class="hunk"><td class="dn" colspan="2"></td><td class="dc">${head}</td></tr>`;
    for (const r of pairRows(h.lines)) {
      if (r.note) { yield `<tr class="note"><td class="dn" colspan="2"></td><td class="dc" colspan="${mode === 'split' ? 2 : 1}">${esc(r.note)}</td></tr>`; continue; }
      if (r.ctx) {
        const [, o, n, t] = r.ctx;
        const c = lineHtml(t, lang);
        yield mode === 'split'
          ? `<tr class="ctx"><td class="dn" data-n="${o}"></td><td class="dc">${c}</td><td class="dn" data-n="${n}"></td><td class="dc">${c}</td></tr>`
          : `<tr class="ctx"><td class="dn" data-n="${o}"></td><td class="dn" data-n="${n}"></td><td class="dc">${c}</td></tr>`;
        continue;
      }
      let dr = null, ar = null;
      if (r.del && r.add) [dr, ar] = changedRange(r.del[3], r.add[3]);
      const dh = r.del ? lineHtml(r.del[3], lang, dr) : '';
      const ah = r.add ? lineHtml(r.add[3], lang, ar) : '';
      if (mode === 'split') {
        yield '<tr>'
          + (r.del ? `<td class="dn del" data-n="${r.del[1]}"></td><td class="dc del">${dh}</td>` : '<td class="dn empty"></td><td class="dc empty"></td>')
          + (r.add ? `<td class="dn add" data-n="${r.add[2]}"></td><td class="dc add">${ah}</td>` : '<td class="dn empty"></td><td class="dc empty"></td>')
          + '</tr>';
      } else {
        if (r.del) yield `<tr class="del"><td class="dn" data-n="${r.del[1]}"></td><td class="dn"></td><td class="dc">${dh}</td></tr>`;
        if (r.add) yield `<tr class="add"><td class="dn"></td><td class="dn" data-n="${r.add[2]}"></td><td class="dc">${ah}</td></tr>`;
      }
    }
  }
}

const CHUNK = 1000;
/** Up to `n` more rows from a row generator, and whether it's finished. */
function take(gen, n) {
  let out = '';
  for (let k = 0; k < n; k++) {
    const r = gen.next();
    if (r.done) return [out, true];
    out += r.value;
  }
  return [out, false];
}
const tableOpen = (mode) => `<table class="dtable ${mode}"><colgroup>${mode === 'split' ? '<col class="cn"><col><col class="cn"><col>' : '<col class="cn"><col class="cn"><col>'}</colgroup><tbody>`;

function statBar(a, d) {
  const total = a + d;
  const blocks = 5;
  const g = total ? Math.round((a / total) * blocks) : 0;
  const r = total ? Math.min(blocks - g, Math.round((d / total) * blocks)) : 0;
  return `<span class="statbar">${'<i class="g"></i>'.repeat(g)}${'<i class="r"></i>'.repeat(r)}${'<i></i>'.repeat(blocks - g - r)}</span>`;
}

/** A note instead of rows (binary, too large, empty), or null when there are rows to show. */
function fileNote(f, i, canLoad) {
  if (f.binary) return '<div class="dnote">Binary file not shown.</div>';
  if (f.too_large) {
    const load = canLoad && !f.load_failed ? ` <button class="btn sm" data-load="${i}">Load diff</button>` : '';
    return `<div class="dnote">This diff is large (${num(f.additions)} additions, ${num(f.deletions)} deletions).${load}</div>`;
  }
  if (!f.hunks.length) {
    if (f.status === 'renamed') return '<div class="dnote">File renamed without changes.</div>';
    if (f.old_mode && f.new_mode) return `<div class="dnote">Mode changed from ${esc(f.old_mode)} to ${esc(f.new_mode)}.</div>`;
    return '<div class="dnote">Empty file.</div>';
  }
  return null;
}

/**
 * Render a diff. opts: { owner, repo, sha (commit to link "view file" to), key (for viewed state), title }
 */
export function diffView(diff, opts = {}) {
  const files = diff.files || [];
  const mode = getMode();
  const withTree = files.length > 3;
  const list = files.map((f, i) => {
    const path = f.status === 'deleted' ? f.old_path : f.new_path;
    const [letter, cls] = STATUS[f.status] || STATUS.modified;
    return html`<a class="dtree-item" href="#diff-${i}" data-jump="${i}" title="${path}"><span class="st ${cls}">${letter}</span><span class="ellipsis grow">${path}</span><span class="tiny"><span class="c-green">+${f.additions}</span> <span class="c-red">−${f.deletions}</span></span></a>`;
  });
  const sections = files.map((f, i) => {
    const path = f.status === 'deleted' ? f.old_path : f.new_path;
    const [letter, cls, word] = STATUS[f.status] || STATUS.modified;
    const viewed = opts.key && isViewed(opts.key, path);
    const view = opts.sha && f.status !== 'deleted' ? `/${opts.owner}/${opts.repo}/blob/${opts.sha}/${encPath(path)}` : null;
    return html`<section class="dfile ${viewed ? 'collapsed viewed' : ''}" id="diff-${i}" data-i="${i}">
      <header class="dhead">
        <button class="btn ghost sm icon dtoggle" aria-label="Collapse file">${icon('chev-down', 'sm')}</button>
        <span class="st ${cls}" title="${word}">${letter}</span>
        <span class="dpath ellipsis">${f.status === 'renamed' || f.status === 'copied' ? html`<span class="muted">${f.old_path}</span> → ` : ''}<b>${path}</b></span>
        <button class="btn ghost sm icon hide-sm" data-copy="${path}" data-copy-label="Path copied" aria-label="Copy path">${icon('copy', 'sm')}</button>
        <span class="grow"></span>
        <span class="small nowrap"><span class="c-green">+${num(f.additions)}</span> <span class="c-red">−${num(f.deletions)}</span> ${raw(statBar(f.additions, f.deletions))}</span>
        ${opts.key ? html`<label class="viewed-check small hide-sm"><input type="checkbox" data-viewed="${path}" ${viewed ? 'checked' : ''}> Viewed</label>` : ''}
        ${view ? html`<a class="btn ghost sm hide-sm" href="${view}">View file</a>` : ''}
      </header>
      <div class="dbody" data-body="${i}"><div class="dplaceholder"><span class="spin"></span></div></div>
    </section>`;
  });
  const viewedCount = opts.key ? files.filter((f) => isViewed(opts.key, f.status === 'deleted' ? f.old_path : f.new_path)).length : 0;
  return html`<div class="diff ${withTree ? 'with-tree' : ''}" data-mode="${mode}">
    <div class="dsummary">
      <span><b>${num(files.length)}</b> ${files.length === 1 ? 'file' : 'files'} changed</span>
      <span class="c-green">+${num(diff.additions)}</span><span class="c-red">−${num(diff.deletions)}</span>
      ${diff.truncated ? html`<span class="pill" title="Very large change">Showing the first part of a large diff</span>` : ''}
      <span class="grow"></span>
      ${opts.key ? html`<span class="viewed-progress small muted" title="Files marked viewed"><svg viewBox="0 0 20 20" class="ring"><circle cx="10" cy="10" r="8"/><circle cx="10" cy="10" r="8" class="fg" style="stroke-dasharray:${(viewedCount / Math.max(1, files.length)) * 50.3} 60"/></svg><span class="vcount">${viewedCount}</span> / ${files.length} viewed</span>` : ''}
      <button class="btn sm" data-collapse-all>Collapse all</button>
      <div class="seg" role="group" aria-label="Diff layout"><button data-mode="unified" class="${mode === 'unified' ? 'on' : ''}">${icon('rows', 'sm')} Unified</button><button data-mode="split" class="${mode === 'split' ? 'on' : ''}">${icon('columns', 'sm')} Split</button></div>
    </div>
    <div class="dgrid">
      ${withTree ? html`<nav class="dtree" aria-label="Changed files">${list}</nav>` : ''}
      <div class="dfiles">${files.length ? sections : html`<div class="box empty">${icon('check', 'big')}<h3>No changes</h3></div>`}</div>
    </div>
  </div>`;
}

/**
 * Wire up a rendered diff. opts: { key (viewed state), load(query) -> one file's diff for `path=…&old=…` (enables "Load diff") }
 */
export async function mountDiff(root, diff, opts = {}) {
  const el = root.querySelector('.diff');
  if (!el) return;
  const files = diff.files || [];
  await Promise.all([...new Set(files.map((f) => langOf(f.new_path || f.old_path)).filter(Boolean))].map(ensureLang));
  let mode = el.dataset.mode;
  const rendered = new Set();
  const renderBody = (i) => {
    const body = el.querySelector(`[data-body="${i}"]`);
    if (!body) return;
    rendered.add(i);
    body._more?.disconnect();
    const note = fileNote(files[i], i, !!opts.load);
    if (note) { body.innerHTML = note; return; }
    // Rows go in a chunk at a time as the reader scrolls, so a 40,000-line file stays smooth.
    const gen = rowsOf(files[i], mode);
    // Re-rendering (e.g. switching split/unified) keeps as many rows as were already shown.
    const [first, done] = take(gen, Math.max(CHUNK, body._shown || 0));
    body._shown = Math.max(CHUNK, body._shown || 0);
    const warn = warning(scan(files[i].hunks.map((h) => h.lines.map((l) => l[3]).join('\n')).join('\n')));
    body.innerHTML = `${warn}${tableOpen(mode)}${first}</tbody></table>${done ? '' : '<div class="dmore"><span class="spin"></span></div>'}`;
    if (done) return;
    const tbody = body.querySelector('tbody'), more = body.querySelector('.dmore');
    const ob = new IntersectionObserver((es) => {
      if (!es.some((e) => e.isIntersecting)) return;
      const [rows, fin] = take(gen, CHUNK);
      tbody.insertAdjacentHTML('beforeend', rows);
      body._shown += CHUNK;
      if (fin) { ob.disconnect(); more.remove(); return; }
      ob.unobserve(more); ob.observe(more); // fire again if it's still on screen
    }, { rootMargin: '1500px 0px' });
    ob.observe(more);
    body._more = ob;
  };
  // Render the first screenful now, the rest as they approach the viewport.
  let budget = 1500;
  files.forEach((f, i) => {
    const size = f.hunks.reduce((a, h) => a + h.lines.length, 0);
    if (budget > 0 && !el.querySelector(`#diff-${i}`).classList.contains('collapsed')) { renderBody(i); budget -= size; }
  });
  const io = new IntersectionObserver((entries) => {
    for (const e of entries) if (e.isIntersecting) { const i = +e.target.dataset.body; if (!rendered.has(i)) renderBody(i); io.unobserve(e.target); }
  }, { rootMargin: '1200px 0px' });
  el.querySelectorAll('[data-body]').forEach((b) => { if (!rendered.has(+b.dataset.body)) io.observe(b); });

  el.addEventListener('click', async (e) => {
    const t = e.target;
    const lb = t.closest('[data-load]');
    if (lb && opts.load) {
      const i = +lb.dataset.load, f = files[i];
      lb.disabled = true; lb.textContent = 'Loading…';
      try {
        const path = f.status === 'deleted' ? f.old_path : f.new_path;
        const old = f.old_path && f.old_path !== path ? `&old=${encodeURIComponent(f.old_path)}` : '';
        const one = await opts.load(`path=${encodeURIComponent(path)}${old}`);
        if (one && !one.too_large) files[i] = one; else f.load_failed = true;
      } catch { f.load_failed = true; }
      renderBody(i);
      return;
    }
    const tog = t.closest('.dtoggle');
    if (tog) {
      const sec = tog.closest('.dfile');
      sec.classList.toggle('collapsed');
      const i = +sec.dataset.i;
      if (!rendered.has(i)) renderBody(i);
      return;
    }
    const mb = t.closest('.seg [data-mode]');
    if (mb) {
      mode = mb.dataset.mode;
      setMode(mode);
      el.dataset.mode = mode;
      el.querySelectorAll('.seg [data-mode]').forEach((b) => b.classList.toggle('on', b.dataset.mode === mode));
      [...rendered].forEach(renderBody);
      return;
    }
    if (t.closest('[data-collapse-all]')) {
      const secs = el.querySelectorAll('.dfile');
      const allCollapsed = [...secs].every((s) => s.classList.contains('collapsed'));
      secs.forEach((s) => { s.classList.toggle('collapsed', !allCollapsed); if (allCollapsed && !rendered.has(+s.dataset.i)) renderBody(+s.dataset.i); });
      t.closest('[data-collapse-all]').textContent = allCollapsed ? 'Collapse all' : 'Expand all';
      return;
    }
    const jump = t.closest('[data-jump]');
    if (jump) {
      e.preventDefault();
      e.stopPropagation();
      const sec = el.querySelector('#diff-' + jump.dataset.jump);
      sec.classList.remove('collapsed');
      if (!rendered.has(+jump.dataset.jump)) renderBody(+jump.dataset.jump);
      sec.scrollIntoView({ behavior: 'smooth', block: 'start' });
      el.querySelectorAll('.dtree-item.on').forEach((x) => x.classList.remove('on'));
      jump.classList.add('on');
    }
  });
  el.addEventListener('change', (e) => {
    const cb = e.target.closest('[data-viewed]');
    if (!cb || !opts.key) return;
    setViewed(opts.key, cb.dataset.viewed, cb.checked);
    const sec = cb.closest('.dfile');
    sec.classList.toggle('viewed', cb.checked);
    sec.classList.toggle('collapsed', cb.checked);
    if (!cb.checked && !rendered.has(+sec.dataset.i)) renderBody(+sec.dataset.i);
    const n = el.querySelectorAll('[data-viewed]:checked').length;
    const vc = $('.vcount', el);
    if (vc) vc.textContent = n;
    const ring = el.querySelector('.ring .fg');
    if (ring) ring.style.strokeDasharray = `${(n / Math.max(1, files.length)) * 50.3} 60`;
    if (cb.checked) {
      const next = sec.nextElementSibling;
      if (next && sec.getBoundingClientRect().top < 0) sec.scrollIntoView({ block: 'start' });
    }
  });
  document.addEventListener('keydown', function onKey(e) {
    if (!el.isConnected) return document.removeEventListener('keydown', onKey);
    if (e.key === 'u' && !/INPUT|TEXTAREA/.test(document.activeElement?.tagName) && !e.metaKey && !e.ctrlKey) {
      el.querySelector(`.seg [data-mode="${mode === 'split' ? 'unified' : 'split'}"]`)?.click();
    }
  });
}
