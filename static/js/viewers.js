// Rich file viewers for the file page, loaded on demand: CSV/TSV tables you can
// sort, PDFs, Jupyter notebooks, STL models in 3D, audio and video. Pushed content
// never runs here: markdown goes through the sanitizer, HTML and script outputs
// show as text, media and PDFs come from /raw with its own locked-down headers.
import { html, raw, esc, num } from './lib.js';
import { highlightBlock, ensureLang } from './hl.js';
import { renderMarkdown, preloadLangs } from './md.js';

const KINDS = {
  csv: 'csv', tsv: 'csv', ipynb: 'notebook', stl: 'stl', pdf: 'pdf',
  mp3: 'audio', wav: 'audio', ogg: 'audio', oga: 'audio', opus: 'audio', flac: 'audio', m4a: 'audio', aac: 'audio',
  mp4: 'video', m4v: 'video', webm: 'video', ogv: 'video', mov: 'video',
};
/** Which viewer shows `path`, if any. */
export const kindOf = (path) => KINDS[(path.split('.').pop() || '').toLowerCase()] || null;
/** Viewers that read the file as text from the blob API (the rest load /raw). */
export const textual = (kind) => kind === 'csv' || kind === 'notebook';

/** The viewer's HTML. opts: { path, content, raw (URL), size, md (markdown link context) }. Throws if the file doesn't parse. */
export async function render(kind, opts) {
  if (kind === 'csv') return csvView(opts);
  if (kind === 'notebook') return notebookView(opts);
  if (kind === 'pdf') return html`<iframe class="pdf-view" src="${opts.raw}" title="${opts.path}"></iframe>`;
  if (kind === 'audio') return html`<div class="media-view"><audio controls preload="metadata" src="${opts.raw}"></audio></div>`;
  if (kind === 'video') return html`<div class="media-view"><video controls preload="metadata" src="${opts.raw}"></video></div>`;
  if (kind === 'stl') return html`<div class="stl-view"><canvas aria-label="3D view of ${opts.path}"></canvas><div class="stl-note muted small">Loading model…</div></div>`;
  return '';
}

/** Wire up what needs it once the HTML is on the page. */
export function mount(kind, el, opts) {
  if (kind === 'csv') mountCsv(el.querySelector('.csv-view'), opts);
  if (kind === 'stl') mountStl(el.querySelector('.stl-view'), opts);
}

// ---------------------------------------------------------------- CSV

const MAX_ROWS = 1000;
/** A file with a huge first line mustn't build a billion cells. */
const MAX_COLS = 200;

/** RFC 4180-style fields (quotes, doubled quotes, CRLF); stops after `max` rows. */
export function parseCsv(text, delim, max = Infinity) {
  const rows = [];
  let row = [], field = '', q = false, i = 0;
  const n = text.length;
  while (i < n && rows.length < max) {
    const c = text[i];
    if (q) {
      if (c === '"') { if (text[i + 1] === '"') { field += '"'; i += 2; continue; } q = false; i++; continue; }
      field += c; i++; continue;
    }
    if (c === '"' && field === '') { q = true; i++; continue; }
    if (c === delim) { row.push(field); field = ''; i++; continue; }
    if (c === '\n' || c === '\r') {
      row.push(field); rows.push(row); row = []; field = '';
      i += c === '\r' && text[i + 1] === '\n' ? 2 : 1;
      continue;
    }
    field += c; i++;
  }
  if (rows.length < max && (field !== '' || row.length)) { row.push(field); rows.push(row); }
  return { rows, more: i < n };
}

const isNum = (s) => s.trim() !== '' && !isNaN(Number(s));

function csvRows(rows, width) {
  return rows.map((r) => `<tr>${Array.from({ length: width }, (_, j) => `<td>${esc(r[j] ?? '')}</td>`).join('')}</tr>`).join('');
}

function csvView({ path, content }) {
  const delim = /\.tsv$/i.test(path) ? '\t' : ',';
  const { rows, more } = parseCsv(content, delim, MAX_ROWS + 1);
  const [head = [], ...body] = rows.filter((r) => !(r.length === 1 && r[0] === ''));
  const wide = Math.max(head.length, ...body.map((r) => r.length));
  const width = Math.min(wide, MAX_COLS);
  if (!width) throw new Error('empty');
  return html`<div class="csv-view">
    <div class="csv-scroll"><table class="csv-table"><thead><tr>${Array.from({ length: width }, (_, j) => html`<th><button data-col="${j}" title="Sort by this column">${head[j] ?? ''}</button></th>`)}</tr></thead>
    <tbody>${raw(csvRows(body, width))}</tbody></table></div>
    ${more || wide > width ? html`<p class="csv-more muted small">Showing the first ${more ? `${num(MAX_ROWS)} rows` : ''}${more && wide > width ? ' and ' : ''}${wide > width ? `${num(MAX_COLS)} columns` : ''}. Download the file or view the code for the rest.</p>` : ''}
  </div>`;
}

function mountCsv(box, { path, content }) {
  if (!box) return;
  const delim = /\.tsv$/i.test(path) ? '\t' : ',';
  const { rows } = parseCsv(content, delim, MAX_ROWS + 1);
  const [, ...body] = rows.filter((r) => !(r.length === 1 && r[0] === ''));
  const data = body.slice(0, MAX_ROWS);
  const width = box.querySelectorAll('th').length;
  let col = -1, dir = 1;
  box.querySelector('thead').addEventListener('click', (e) => {
    const b = e.target.closest('[data-col]');
    if (!b) return;
    const c = +b.dataset.col;
    dir = c === col ? -dir : 1;
    col = c;
    const numeric = data.every((r) => (r[c] ?? '') === '' || isNum(r[c]));
    const cmp = numeric ? (a, b2) => (Number(a[c] || 0) - Number(b2[c] || 0)) : (a, b2) => String(a[c] ?? '').localeCompare(String(b2[c] ?? ''), undefined, { numeric: true });
    data.sort((a, b2) => dir * cmp(a, b2));
    box.querySelector('tbody').innerHTML = csvRows(data, width);
    box.querySelectorAll('th').forEach((th, j) => th.setAttribute('aria-sort', j === c ? (dir > 0 ? 'ascending' : 'descending') : 'none'));
  });
}

// ---------------------------------------------------------------- notebooks

const text = (v) => (Array.isArray(v) ? v.join('') : String(v ?? ''));
// eslint-disable-next-line no-control-regex
const noAnsi = (s) => s.replace(/\x1b\[[0-9;]*[A-Za-z]/g, '');
const B64 = /^[A-Za-z0-9+/=\s]+$/;

function outputHtml(o, md) {
  if (o.output_type === 'stream') return html`<pre class="nb-out${o.name === 'stderr' ? ' nb-err' : ''}">${noAnsi(text(o.text))}</pre>`;
  if (o.output_type === 'error') return html`<pre class="nb-out nb-err">${noAnsi(text(o.traceback) || `${o.ename}: ${o.evalue}`)}</pre>`;
  const d = o.data || {};
  for (const t of ['image/png', 'image/jpeg', 'image/gif']) {
    const b = text(d[t]).replace(/\s/g, '');
    if (b && B64.test(b)) return html`<div class="nb-out"><img src="data:${t};base64,${b}" alt="Output image"></div>`;
  }
  // An SVG as an image can't run scripts.
  if (d['image/svg+xml']) return html`<div class="nb-out"><img src="data:image/svg+xml;base64,${btoa(unescape(encodeURIComponent(text(d['image/svg+xml']))))}" alt="Output image"></div>`;
  if (d['text/markdown']) return html`<div class="nb-out markdown-body">${raw(renderMarkdown(text(d['text/markdown']), md))}</div>`;
  if (d['text/plain'] != null) return html`<pre class="nb-out">${noAnsi(text(d['text/plain']))}</pre>`;
  // HTML (and anything scripted) shows as its source, never as live markup.
  if (d['text/html'] != null) return html`<div class="nb-out"><div class="muted tiny">HTML output, shown as source</div><pre>${text(d['text/html'])}</pre></div>`;
  const types = Object.keys(d);
  return types.length ? html`<div class="nb-out muted small">Output not shown (${types.join(', ')})</div>` : '';
}

async function notebookView({ content, md }) {
  const nb = JSON.parse(content);
  if (!Array.isArray(nb.cells)) throw new Error('not a notebook');
  const lang = String(nb.metadata?.language_info?.name || nb.metadata?.kernelspec?.language || 'python').toLowerCase();
  await ensureLang(lang);
  await Promise.all(nb.cells.filter((c) => c.cell_type === 'markdown').map((c) => preloadLangs(text(c.source))));
  return html`<div class="notebook">${nb.cells.map((c) => {
    const src = text(c.source);
    if (c.cell_type === 'markdown') return html`<div class="nb-cell nb-md markdown-body">${raw(renderMarkdown(src, md))}</div>`;
    if (c.cell_type !== 'code') return html`<div class="nb-cell"><pre class="nb-out">${src}</pre></div>`;
    const n = c.execution_count == null ? ' ' : c.execution_count;
    return html`<div class="nb-cell nb-code">
      <div class="nb-in"><span class="nb-prompt">[${n}]</span><pre class="nb-src"><code class="hljs">${raw(highlightBlock(src, lang))}</code></pre></div>
      ${(c.outputs || []).map((o) => outputHtml(o, md))}
    </div>`;
  })}</div>`;
}

// ---------------------------------------------------------------- STL

const MAX_STL_BYTES = 64 << 20;
const MAX_TRIANGLES = 1_000_000;

/** Triangle corners as a flat Float32Array (9 numbers per triangle), from binary or ASCII STL. */
export function parseStl(buf) {
  const dv = new DataView(buf);
  const count = buf.byteLength >= 84 ? dv.getUint32(80, true) : -1;
  if (count >= 0 && 84 + count * 50 === buf.byteLength) {
    if (count > MAX_TRIANGLES) throw new Error(`${num(count)} triangles is more than this viewer shows`);
    const out = new Float32Array(count * 9);
    for (let t = 0; t < count; t++) for (let k = 0; k < 9; k++) out[t * 9 + k] = dv.getFloat32(84 + t * 50 + 12 + k * 4, true);
    return out;
  }
  const src = new TextDecoder().decode(buf);
  const re = /vertex\s+(\S+)\s+(\S+)\s+(\S+)/g;
  const v = [];
  let m;
  while ((m = re.exec(src))) {
    if (v.length >= MAX_TRIANGLES * 9) throw new Error('This model has more triangles than this viewer shows');
    v.push(+m[1], +m[2], +m[3]);
  }
  if (!v.length || v.length % 9 || v.some((x) => !Number.isFinite(x))) throw new Error("This doesn't look like an STL model");
  return new Float32Array(v);
}

/** Flat normals, and the model centred and scaled to fit a unit sphere. */
function prepare(pos) {
  const lo = [Infinity, Infinity, Infinity], hi = [-Infinity, -Infinity, -Infinity];
  for (let i = 0; i < pos.length; i++) { const a = i % 3; if (pos[i] < lo[a]) lo[a] = pos[i]; if (pos[i] > hi[a]) hi[a] = pos[i]; }
  const mid = lo.map((l, a) => (l + hi[a]) / 2);
  const r = Math.hypot(hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]) / 2 || 1;
  const p = new Float32Array(pos.length), nrm = new Float32Array(pos.length);
  for (let i = 0; i < pos.length; i++) p[i] = (pos[i] - mid[i % 3]) / r;
  for (let t = 0; t < p.length; t += 9) {
    const ux = p[t + 3] - p[t], uy = p[t + 4] - p[t + 1], uz = p[t + 5] - p[t + 2];
    const vx = p[t + 6] - p[t], vy = p[t + 7] - p[t + 1], vz = p[t + 8] - p[t + 2];
    let nx = uy * vz - uz * vy, ny = uz * vx - ux * vz, nz = ux * vy - uy * vx;
    const l = Math.hypot(nx, ny, nz) || 1;
    nx /= l; ny /= l; nz /= l;
    for (let k = 0; k < 9; k += 3) { nrm[t + k] = nx; nrm[t + k + 1] = ny; nrm[t + k + 2] = nz; }
  }
  return { p, nrm };
}

const VS = `attribute vec3 p; attribute vec3 n; uniform mat4 mvp; uniform mat4 rot; varying vec3 vn;
void main() { vn = (rot * vec4(n, 0.0)).xyz; gl_Position = mvp * vec4(p, 1.0); }`;
const FS = `precision mediump float; varying vec3 vn; uniform vec3 col;
void main() { vec3 l = normalize(vec3(0.4, 0.6, 1.0)); float d = abs(dot(normalize(vn), l));
  gl_FragColor = vec4(col * (0.25 + 0.75 * d), 1.0); }`;

function mul(a, b) {
  const o = new Float32Array(16);
  for (let c = 0; c < 4; c++) for (let r = 0; r < 4; r++) { let s = 0; for (let k = 0; k < 4; k++) s += a[k * 4 + r] * b[c * 4 + k]; o[c * 4 + r] = s; }
  return o;
}
function rotation(yaw, pitch) {
  const cy = Math.cos(yaw), sy = Math.sin(yaw), cp = Math.cos(pitch), sp = Math.sin(pitch);
  // Model z is up (as CAD tools save it): turn it to screen up, then orbit.
  const up = new Float32Array([1, 0, 0, 0, 0, 0, -1, 0, 0, 1, 0, 0, 0, 0, 0, 1]);
  const y = new Float32Array([cy, 0, -sy, 0, 0, 1, 0, 0, sy, 0, cy, 0, 0, 0, 0, 1]);
  const x = new Float32Array([1, 0, 0, 0, 0, cp, sp, 0, 0, -sp, cp, 0, 0, 0, 0, 1]);
  return mul(x, mul(y, up));
}
function perspective(aspect, dist) {
  const f = 1 / Math.tan(Math.PI / 8), near = 0.05, far = dist + 4;
  const proj = new Float32Array([f / aspect, 0, 0, 0, 0, f, 0, 0, 0, 0, (far + near) / (near - far), -1, 0, 0, (2 * far * near) / (near - far), 0]);
  const view = new Float32Array([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, -dist, 1]);
  return mul(proj, view);
}

/** The page's accent colour as 0..1 RGB (whatever CSS colour syntax the theme uses). */
function accent(el) {
  const c = document.createElement('canvas').getContext('2d');
  c.fillStyle = getComputedStyle(el).getPropertyValue('--accent').trim() || '#7c5cff';
  c.fillRect(0, 0, 1, 1);
  const [r, g, b] = c.getImageData(0, 0, 1, 1).data;
  return [r / 255, g / 255, b / 255];
}

async function mountStl(box, { raw: url, size }) {
  if (!box) return;
  const note = box.querySelector('.stl-note'), canvas = box.querySelector('canvas');
  const say = (msg) => { note.textContent = msg; };
  if (size > MAX_STL_BYTES) return say('This model is too big to show here. Download it to view it.');
  let pos;
  try {
    const res = await fetch(url, { credentials: 'same-origin' });
    if (!res.ok) throw new Error("Couldn't load the model");
    pos = parseStl(await res.arrayBuffer());
  } catch (e) { return say(e.message); }
  const gl = canvas.getContext('webgl', { antialias: true });
  if (!gl) return say("This browser can't show 3D models (WebGL is off). Download the file to view it.");
  const prog = gl.createProgram();
  for (const [type, src] of [[gl.VERTEX_SHADER, VS], [gl.FRAGMENT_SHADER, FS]]) {
    const s = gl.createShader(type);
    gl.shaderSource(s, src); gl.compileShader(s); gl.attachShader(prog, s);
  }
  gl.linkProgram(prog);
  gl.useProgram(prog);
  const { p, nrm } = prepare(pos);
  for (const [name, data] of [['p', p], ['n', nrm]]) {
    gl.bindBuffer(gl.ARRAY_BUFFER, gl.createBuffer());
    gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW);
    const loc = gl.getAttribLocation(prog, name);
    gl.enableVertexAttribArray(loc);
    gl.vertexAttribPointer(loc, 3, gl.FLOAT, false, 0, 0);
  }
  gl.enable(gl.DEPTH_TEST);
  gl.uniform3fv(gl.getUniformLocation(prog, 'col'), accent(box));
  const view = { yaw: 0.6, pitch: 0.45, dist: 3 };
  let queued = false;
  const draw = () => {
    queued = false;
    const w = Math.max(1, Math.round(canvas.clientWidth * devicePixelRatio)), h = Math.max(1, Math.round(canvas.clientHeight * devicePixelRatio));
    if (canvas.width !== w || canvas.height !== h) { canvas.width = w; canvas.height = h; }
    gl.viewport(0, 0, w, h);
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
    const rot = rotation(view.yaw, view.pitch);
    gl.uniformMatrix4fv(gl.getUniformLocation(prog, 'rot'), false, rot);
    gl.uniformMatrix4fv(gl.getUniformLocation(prog, 'mvp'), false, mul(perspective(w / h, view.dist), rot));
    gl.drawArrays(gl.TRIANGLES, 0, p.length / 3);
  };
  const redraw = () => { if (!queued) { queued = true; requestAnimationFrame(draw); } };
  let drag = null;
  canvas.addEventListener('pointerdown', (e) => { drag = { x: e.clientX, y: e.clientY }; canvas.setPointerCapture(e.pointerId); });
  canvas.addEventListener('pointermove', (e) => {
    if (!drag) return;
    view.yaw += (e.clientX - drag.x) * 0.01;
    view.pitch = Math.max(-1.5, Math.min(1.5, view.pitch + (e.clientY - drag.y) * 0.01));
    drag = { x: e.clientX, y: e.clientY };
    redraw();
  });
  canvas.addEventListener('pointerup', () => { drag = null; });
  canvas.addEventListener('wheel', (e) => { e.preventDefault(); view.dist = Math.max(1.2, Math.min(12, view.dist * (e.deltaY > 0 ? 1.1 : 0.9))); redraw(); }, { passive: false });
  canvas.addEventListener('dblclick', () => { Object.assign(view, { yaw: 0.6, pitch: 0.45, dist: 3 }); redraw(); });
  const ro = new ResizeObserver(() => (canvas.isConnected ? redraw() : ro.disconnect()));
  ro.observe(canvas);
  say(`${num(p.length / 9)} triangles · drag to turn, scroll to zoom, double-click to reset`);
  box.dataset.ready = '1';
  draw();
}
