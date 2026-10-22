// Edit a file, create a new one, or upload files — then commit to the branch
// or to a new branch that becomes a pull request.
import { html, raw, esc, state, get, send, encPath, enc, bytes, langOf, toast, action, $, $$ } from '../lib.js';
import { icon } from '../icons.js';
import { loadRepo, repoUrl, splitRev, repoPage, mountRepoHeader } from '../repohead.js';
import { highlightBlock, ensureLang } from '../hl.js';
import { renderMarkdown, preloadLangs } from '../md.js';
import { navigate } from '../main.js';
import { allowDialog } from './security.js';

// ---------- dropped files (handed over from the tree page) ----------
let stashed = null;
async function readEntry(entry, prefix, out) {
  if (entry.isFile) {
    const file = await new Promise((res, rej) => entry.file(res, rej));
    out.push({ path: prefix + entry.name, file });
  } else if (entry.isDirectory) {
    const reader = entry.createReader();
    let batch;
    do {
      batch = await new Promise((res, rej) => reader.readEntries(res, rej));
      for (const e of batch) await readEntry(e, prefix + entry.name + '/', out);
    } while (batch.length);
  }
}
/** Collect dropped files and folders. Must be called inside the drop event. */
export function stashDrop(dt) {
  const entries = [...(dt.items || [])].map((i) => i.webkitGetAsEntry?.()).filter(Boolean);
  const files = [...dt.files];
  stashed = (async () => {
    if (!entries.length) return files.map((f) => ({ path: f.name, file: f }));
    const out = [];
    for (const e of entries) await readEntry(e, '', out);
    return out;
  })();
}

const toB64 = (file) => new Promise((res, rej) => {
  const r = new FileReader();
  r.onload = () => res(String(r.result).split(',')[1] || '');
  r.onerror = rej;
  r.readAsDataURL(file);
});

// ---------- code editor ----------
function editorHtml(text) {
  return html`<div class="code-editor"><div class="ed-scroll"><div class="ed-inner">
    <div class="ed-gutter" aria-hidden="true"></div>
    <div class="ed-stack"><pre aria-hidden="true"><code class="hljs"></code></pre>
    <textarea id="ed-text" spellcheck="false" autocapitalize="off" autocomplete="off" aria-label="File contents">${text}</textarea></div>
  </div></div></div>`;
}

function mountEditor(root, getPath, onSave) {
  const ta = $('#ed-text', root);
  const code = $('.code-editor pre code', root);
  const gutter = $('.ed-gutter', root);
  let lang = langOf(getPath());
  let lines = 0;
  let queued = false;
  const paint = () => {
    queued = false;
    const v = ta.value;
    code.innerHTML = (v.length < 300_000 && lang ? highlightBlock(v, lang) : esc(v)) + '\n';
    const n = v.split('\n').length;
    if (n !== lines) { lines = n; gutter.textContent = Array.from({ length: n }, (_, i) => i + 1).join('\n'); }
  };
  const schedule = () => { if (!queued) { queued = true; requestAnimationFrame(paint); } };
  ensureLang(lang).then(schedule);
  ta.addEventListener('input', schedule);
  root.addEventListener('path-change', () => { lang = langOf(getPath()); ensureLang(lang).then(schedule); });
  const indentUnit = /^\t/m.test(ta.value) ? '\t' : '  ';
  ta.addEventListener('keydown', (e) => {
    if ((e.metaKey || e.ctrlKey) && e.key === 's') { e.preventDefault(); onSave(); return; }
    if (e.key === 'Escape') { ta.blur(); return; }
    const { selectionStart: s, selectionEnd: t, value: v } = ta;
    if (e.key === 'Tab') {
      e.preventDefault();
      const lineStart = v.lastIndexOf('\n', s - 1) + 1;
      if (e.shiftKey) {
        const m = v.slice(lineStart).match(/^( {1,2}|\t)/);
        if (m) { ta.setRangeText('', lineStart, lineStart + m[0].length, 'preserve'); }
      } else if (s === t) {
        ta.setRangeText(indentUnit, s, t, 'end');
      } else {
        const block = v.slice(lineStart, t).replace(/^/gm, indentUnit);
        ta.setRangeText(block, lineStart, t, 'select');
      }
      schedule();
    } else if (e.key === 'Enter' && !e.shiftKey && s === t) {
      const lineStart = v.lastIndexOf('\n', s - 1) + 1;
      const indent = v.slice(lineStart, s).match(/^[ \t]*/)[0];
      const extra = /[{[(:]\s*$/.test(v.slice(lineStart, s)) ? indentUnit : '';
      e.preventDefault();
      ta.setRangeText('\n' + indent + extra, s, t, 'end');
      schedule();
    }
  });
  paint();
}

// ---------- commit box ----------
function commitBox(meta, branch, defaultMsg) {
  const suggestion = `${state.me?.username || 'patch'}-patch-${Math.floor(Math.random() * 900 + 100)}`;
  return html`<div class="box"><div class="box-head"><b>Commit changes</b></div><div class="box-body commit-box">
    <div class="field"><label for="cm-msg">Commit message</label><input id="cm-msg" class="input" placeholder="${defaultMsg}"></div>
    <div class="field"><label for="cm-desc">Extended description <span class="muted" style="font-weight:400">(optional)</span></label><textarea id="cm-desc" class="textarea" rows="3" style="min-height:70px"></textarea></div>
    ${meta.empty ? '' : html`<div class="choice" role="radiogroup">
      <label><input type="radio" name="cm-where" value="direct" checked><span>${icon('commit', 'sm')} Commit directly to <span class="branch-name">${branch}</span></span></label>
      <label><input type="radio" name="cm-where" value="branch"><span style="display:grid;gap:6px">${raw(`${icon('pr', 'sm').s} Create a new branch and start a pull request`)}
        <input class="input sm mono" id="cm-branch" value="${suggestion}" aria-label="New branch name"></span></label>
    </div>`}
    <div class="row"><button class="btn primary" id="cm-go">${icon('check', 'sm')} Commit changes</button><button class="btn" data-back>Cancel</button>
      <span class="muted small hide-sm">${icon('keyboard', 'sm')} Ctrl/⌘ S to commit</span></div>
  </div></div>`;
}

function readCommit(root, defaultMsg) {
  const msg = $('#cm-msg', root).value.trim() || defaultMsg;
  const desc = $('#cm-desc', root).value.trim();
  const where = $('input[name="cm-where"]:checked', root)?.value || 'direct';
  const newBranch = where === 'branch' ? $('#cm-branch', root).value.trim() : null;
  return { message: desc ? `${msg}\n\n${desc}` : msg, newBranch };
}

async function doCommit(meta, branch, expected, changes, root, defaultMsg, after) {
  const { message, newBranch } = readCommit(root, defaultMsg);
  const commit = () => send('POST', `${repoUrl(meta.owner, meta.name)}/contents`, {
    branch, expected_sha: expected || undefined, new_branch: newBranch || undefined, message, changes,
  });
  let r;
  try { r = await commit(); } catch (e) {
    // Push protection: the commit adds something that looks like a secret.
    if (e.code !== 'secret_detected' || !e.details?.findings?.length) throw e;
    if (!await allowDialog(meta, e.details.findings)) return;
    r = await commit();
  }
  const base = `/${meta.owner}/${meta.name}`;
  if (newBranch) {
    toast('Changes committed to ' + newBranch);
    navigate(`${base}/compare/${encPath(branch)}...${encPath(newBranch)}`);
  } else {
    toast('Changes committed');
    navigate(after(r.branch));
  }
}

// ---------- page ----------
export default async function (ctx) {
  const { owner, repo, mode } = ctx.params;
  if (!state.me) return { redirect: '/login?next=' + encodeURIComponent(ctx.path) };
  const meta = await loadRepo(owner, repo);
  if (!meta.can_write) throw Object.assign(new Error("You can't edit this repository"), { status: 403 });
  const { rev: branch, path } = meta.empty ? { rev: (ctx.params.rest || 'main').split('/')[0], path: (ctx.params.rest || '').split('/').slice(1).join('/') } : splitRev(meta, ctx.params.rest || '');
  if (!meta.empty && !meta.branches.includes(branch)) throw Object.assign(new Error('You can only edit files on a branch'), { status: 400 });
  const base = `/${owner}/${repo}`;
  if (mode === 'upload') return uploadView(meta, branch, path);

  let text = '';
  let expected = null;
  let crlf = false;
  if (mode === 'edit') {
    const d = await get(`${repoUrl(owner, repo)}/blob?rev=${enc(branch)}&path=${enc(path)}`, { fresh: true });
    if (!d.editable) throw Object.assign(new Error("This file can't be edited in the browser (binary, not UTF-8, a symlink, or mixed line endings). Use git instead."), { status: 400 });
    text = d.content;
    expected = d.sha;
    crlf = d.eol === 'crlf';
  } else if (!meta.empty) {
    expected = (await get(`${repoUrl(owner, repo)}/tree?rev=${enc(branch)}`, { fresh: true })).sha;
  }
  const dir = mode === 'new' ? path : path.split('/').slice(0, -1).join('/');
  const startPath = mode === 'new' ? (dir ? dir + '/' : '') : path;
  const isMd = (p) => /\.(md|markdown)$/i.test(p);
  const body = html`<div class="stack">
    <div class="editor-head">
      <a class="crumbs" href="${base}/tree/${encPath(branch)}"><b>${meta.name}</b></a><span class="muted">/</span>
      <input class="input path-input" id="ed-path" value="${startPath}" placeholder="Name your file…" aria-label="File path" ${mode === 'new' ? 'autofocus' : ''}>
      <span class="muted small">in</span><span class="branch-name">${branch}</span>
    </div>
    <div class="box">
      <div class="box-head"><div class="seg" id="ed-tabs"><button class="on" data-t="edit">${icon('edit', 'sm')} Edit</button><button data-t="preview" ${isMd(startPath) ? '' : 'hidden'}>${icon('eye', 'sm')} Preview</button></div>
        <span class="grow"></span><span class="muted small" id="ed-lang"></span></div>
      <div id="ed-edit">${editorHtml(text)}</div>
      <div id="ed-preview" class="markdown-body box-body" hidden></div>
    </div>
    ${commitBox(meta, branch, mode === 'edit' ? `Update ${path.split('/').pop()}` : 'Create file')}
  </div>`;
  return {
    title: `${mode === 'edit' ? 'Editing' : 'New file in'} ${meta.owner}/${meta.name}`,
    html: repoPage(meta, 'code', body),
    mount(el) {
      mountRepoHeader(el, meta);
      const pathIn = $('#ed-path', el);
      const langLabel = $('#ed-lang', el);
      const setLabel = () => { langLabel.textContent = langOf(pathIn.value) || 'Plain text'; $('[data-t="preview"]', el).hidden = !isMd(pathIn.value); };
      setLabel();
      const go = $('#cm-go', el);
      const save = action(go, async () => {
        const p = pathIn.value.trim().replace(/^\/+/, '');
        if (!p || p.endsWith('/')) { pathIn.focus(); throw new Error('Give the file a name'); }
        const value = $('#ed-text', el).value;
        const changes = [{ path: p, content: crlf ? value.replace(/\r?\n/g, '\r\n') : value }];
        if (mode === 'edit' && p !== path) changes.unshift({ path, delete: true });
        const defaultMsg = mode === 'edit' ? (p !== path ? `Rename ${path} to ${p}` : `Update ${p.split('/').pop()}`) : `Create ${p.split('/').pop()}`;
        await doCommit(meta, branch, expected, changes, el, defaultMsg, (b) => `${base}/blob/${encPath(b)}/${encPath(p)}`);
      });
      mountEditor(el, () => pathIn.value, save);
      pathIn.addEventListener('input', () => { setLabel(); el.dispatchEvent(new Event('path-change')); });
      go.addEventListener('click', save);
      $('#ed-tabs', el).addEventListener('click', async (e) => {
        const b = e.target.closest('[data-t]');
        if (!b) return;
        $$('#ed-tabs [data-t]', el).forEach((x) => x.classList.toggle('on', x === b));
        const preview = b.dataset.t === 'preview';
        $('#ed-edit', el).hidden = preview;
        const pv = $('#ed-preview', el);
        pv.hidden = !preview;
        if (preview) {
          const src = $('#ed-text', el).value;
          await preloadLangs(src);
          pv.innerHTML = renderMarkdown(src, { owner, repo, rev: branch, dir });
        }
      });
      if (mode === 'edit') $('#ed-text', el).focus({ preventScroll: true });
    },
  };
}

function uploadView(meta, branch, dir) {
  const base = `/${meta.owner}/${meta.name}`;
  let files = [];
  const body = html`<div class="stack">
    <div class="editor-head"><a class="crumbs" href="${base}/tree/${encPath(branch)}${dir ? '/' + encPath(dir) : ''}"><b>${meta.name}</b>${dir ? html`<span class="muted"> / ${dir}</span>` : ''}</a>
      <span class="muted small">Upload to</span><span class="branch-name">${branch}</span></div>
    <label class="dropzone" id="dz" for="up-input">
      <div style="display:grid;gap:8px;justify-items:center">${icon('upload', 'lg')}<b>Drag files or folders here</b><span class="small">or click to choose files · up to 25 MB per commit</span></div>
      <input type="file" id="up-input" multiple hidden>
    </label>
    <div class="box" id="up-box" hidden><div class="box-head"><b id="up-count"></b><span class="grow"></span><button class="btn sm" id="up-clear">Clear</button></div><div class="list upload-list" id="up-list"></div></div>
    ${commitBox(meta, branch, 'Add files via upload')}
  </div>`;
  return {
    title: `Upload files · ${meta.owner}/${meta.name}`,
    html: repoPage(meta, 'code', body),
    async mount(el) {
      mountRepoHeader(el, meta);
      const paint = () => {
        $('#up-box', el).hidden = !files.length;
        const total = files.reduce((a, f) => a + f.file.size, 0);
        $('#up-count', el).textContent = `${files.length} ${files.length === 1 ? 'file' : 'files'} · ${bytes(total)}`;
        $('#up-list', el).innerHTML = files.map((f, i) => html`<div class="list-row">${icon('file', 'sm muted')}<span class="grow ellipsis">${dir ? dir + '/' : ''}${f.path}</span>
          <span class="muted small">${bytes(f.file.size)}</span><button class="btn ghost sm icon" data-rm="${i}" aria-label="Remove">${icon('x', 'sm')}</button></div>`.s).join('');
      };
      const add = (list) => {
        const seen = new Set(files.map((f) => f.path));
        for (const f of list) if (!seen.has(f.path) && !f.path.split('/').some((s) => s === '.git')) files.push(f);
        paint();
      };
      if (stashed) { add(await stashed); stashed = null; }
      const dz = $('#dz', el);
      dz.addEventListener('dragover', (e) => { e.preventDefault(); dz.classList.add('on'); });
      dz.addEventListener('dragleave', () => dz.classList.remove('on'));
      dz.addEventListener('drop', async (e) => { e.preventDefault(); dz.classList.remove('on'); stashDrop(e.dataTransfer); add(await stashed); stashed = null; });
      $('#up-input', el).addEventListener('change', (e) => add([...e.target.files].map((f) => ({ path: f.name, file: f }))));
      $('#up-list', el).addEventListener('click', (e) => { const b = e.target.closest('[data-rm]'); if (b) { files.splice(+b.dataset.rm, 1); paint(); } });
      $('#up-clear', el).addEventListener('click', () => { files = []; paint(); });
      const go = $('#cm-go', el);
      go.addEventListener('click', action(go, async () => {
        if (!files.length) throw new Error('Add some files first');
        const total = files.reduce((a, f) => a + f.file.size, 0);
        if (total > 25 * 1024 * 1024) throw new Error('That is more than 25 MB. Push large files with git instead.');
        const expected = meta.empty ? null : (await get(`${repoUrl(meta.owner, meta.name)}/tree?rev=${enc(branch)}`, { fresh: true })).sha;
        const changes = await Promise.all(files.map(async (f) => ({ path: (dir ? dir + '/' : '') + f.path, content: await toB64(f.file), base64: true })));
        await doCommit(meta, branch, expected, changes, el, `Add ${files.length === 1 ? files[0].path : files.length + ' files'} via upload`,
          (b) => `${base}/tree/${encPath(b)}${dir ? '/' + encPath(dir) : ''}`);
      }));
    },
  };
}
