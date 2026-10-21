// Line-numbered code with linkable, selectable line ranges (#L10 or #L10-L20).
import { html, raw, esc } from './lib.js';
import { highlightLines } from './hl.js';

export function codeBlock(text, { id = 'code' } = {}) {
  const lines = String(text).replace(/\n$/, '').split('\n');
  return html`<div class="code" id="${id}" style="--digits:${String(lines.length).length}">${raw(linesHtml(lines.map(esc)))}</div>`;
}

function linesHtml(lines) {
  let out = '';
  for (let i = 0; i < lines.length; i++) {
    const n = i + 1;
    out += `<div class="cl" id="L${n}"><a class="ln" href="#L${n}" data-ln="${n}" aria-label="Line ${n}"></a><div class="lc">${lines[i] || ' '}</div></div>`;
  }
  return out;
}

/** Re-render the block with syntax colors once highlight.js has run (keeps first paint instant). */
export async function highlightInto(el, text, lang) {
  if (!el || !lang) return;
  const src = String(text).replace(/\n$/, '');
  if (src.length > 600_000) return;
  const lines = await highlightLines(src, lang);
  if (!el.isConnected) return;
  const sel = [...el.querySelectorAll('.cl.hl')].map((x) => x.id);
  el.innerHTML = linesHtml(lines);
  el.classList.add('hl-done');
  sel.forEach((id) => el.querySelector('#' + id)?.classList.add('hl'));
}

export function parseRange(hash) {
  const m = /^#L(\d+)(?:-L?(\d+))?$/.exec(hash || '');
  if (!m) return null;
  const a = +m[1];
  const b = +(m[2] || m[1]);
  return [Math.min(a, b), Math.max(a, b)];
}

function paint(el, range) {
  el.querySelectorAll('.cl.hl').forEach((x) => x.classList.remove('hl'));
  if (!range) return;
  for (let i = range[0]; i <= range[1]; i++) el.querySelector('#L' + i)?.classList.add('hl');
}

/** Click a line number to select it; shift-click to extend. Keeps the URL hash in sync. */
export function mountLineSelection(el) {
  if (!el) return;
  let anchor = null;
  const initial = parseRange(location.hash);
  if (initial) {
    paint(el, initial);
    anchor = initial[0];
    requestAnimationFrame(() => el.querySelector('#L' + initial[0])?.scrollIntoView({ block: 'center' }));
  }
  el.addEventListener('click', (e) => {
    const ln = e.target.closest('.ln');
    if (!ln) return;
    e.preventDefault();
    e.stopPropagation();
    const n = +ln.dataset.ln;
    let range = [n, n];
    if (e.shiftKey && anchor) range = [Math.min(anchor, n), Math.max(anchor, n)];
    else anchor = n;
    paint(el, range);
    const hash = range[0] === range[1] ? `#L${range[0]}` : `#L${range[0]}-L${range[1]}`;
    history.replaceState(history.state, '', location.pathname + location.search + hash);
  });
}
