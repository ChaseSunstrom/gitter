// Markdown -> sanitized HTML. GFM via marked, code highlighting, relative
// link/image rewriting for READMEs, #123 and @user references, GitHub-style alerts.
import { Marked } from '../vendor/marked.esm.js';
import DOMPurify from '../vendor/purify.es.mjs';
import { highlightBlock, ensureLang } from './hl.js';
import { esc, encPath } from './lib.js';

let ctx = null; // { owner, repo, rev, dir } while rendering

const refs = {
  name: 'ref',
  level: 'inline',
  start(src) {
    const re = /(^|[\s(])[#@][A-Za-z0-9]/g;
    const m = re.exec(src);
    return m ? m.index + m[1].length : undefined;
  },
  tokenizer(src) {
    const m = /^#(\d{1,7})\b/.exec(src) || /^@([A-Za-z0-9][A-Za-z0-9._-]{0,38})(?![A-Za-z0-9._-]*@)/.exec(src);
    if (!m) return undefined;
    return { type: 'ref', raw: m[0], kind: m[0][0], value: m[1] };
  },
  renderer(t) {
    if (t.kind === '@') return `<a class="mention" href="/${esc(t.value)}">@${esc(t.value)}</a>`;
    if (ctx?.owner) return `<a class="issue-ref" href="/${esc(ctx.owner)}/${esc(ctx.repo)}/issues/${t.value}">#${t.value}</a>`;
    return esc(t.raw);
  },
};

const md = new Marked({ gfm: true, breaks: false });
md.use({
  extensions: [refs],
  renderer: {
    code({ text, lang }) {
      const l = (lang || '').split(/\s/)[0].toLowerCase();
      return `<pre><code class="hljs${l ? ' language-' + esc(l) : ''}">${highlightBlock(text, l)}</code></pre>`;
    },
  },
});

DOMPurify.addHook('afterSanitizeAttributes', (node) => {
  if (node.tagName === 'A') {
    const href = node.getAttribute('href') || '';
    if (/^https?:\/\//i.test(href) && !href.startsWith(location.origin)) {
      node.setAttribute('target', '_blank');
      node.setAttribute('rel', 'noopener noreferrer nofollow ugc');
    }
  }
});

// Only highlight.js class names survive; user-supplied classes could restyle the page.
DOMPurify.addHook('uponSanitizeAttribute', (node, data) => {
  if (data.attrName === 'class') {
    const keep = data.attrValue.split(/\s+/).filter((c) => /^(hljs(-[\w-]+)?|language-[\w+-]+)$/.test(c));
    data.attrValue = keep.join(' ');
    if (!keep.length) data.keepAttr = false;
  }
});

const PURIFY = { FORBID_TAGS: ['style', 'form', 'button', 'textarea', 'select', 'iframe', 'object', 'embed'], FORBID_ATTR: ['style'], ALLOW_DATA_ATTR: false };

function slug(s) {
  return s.toLowerCase().trim().replace(/[^\p{L}\p{N}\s-]/gu, '').replace(/\s+/g, '-');
}

// Takes the context as an argument: the module-level `ctx` is only set while marked parses.
function resolveRel(href, c) {
  if (!c?.owner || !href || /^([a-z][a-z0-9+.-]*:|\/\/|#|\/)/i.test(href)) return null;
  const [pathPart, hash = ''] = href.split('#');
  const parts = (c.dir ? c.dir.split('/') : []);
  for (const seg of pathPart.split('/')) {
    if (seg === '..') parts.pop();
    else if (seg && seg !== '.') parts.push(seg);
  }
  return { path: parts.join('/'), hash };
}

/** Render markdown. `c` gives repo context so relative links and #refs resolve. */
export function renderMarkdown(src, c = null) {
  ctx = c;
  let out;
  try { out = md.parse(String(src || '')); } finally { ctx = null; }
  const clean = DOMPurify.sanitize(out, PURIFY);
  const t = document.createElement('template');
  t.innerHTML = clean;
  const root = t.content;
  root.querySelectorAll('pre > code').forEach((code) => code.classList.add('hljs'));
  const seen = new Map();
  root.querySelectorAll('h1,h2,h3,h4,h5,h6').forEach((h) => {
    let id = slug(h.textContent) || 'section';
    const n = seen.get(id) || 0;
    seen.set(id, n + 1);
    if (n) id += '-' + n;
    h.id = 'user-content-' + id;
    const a = document.createElement('a');
    a.className = 'anchor';
    a.href = '#' + id;
    a.setAttribute('aria-label', 'Link to this section');
    a.textContent = '#';
    h.prepend(a);
  });
  if (c?.owner) {
    // In a wiki, relative links name other pages ("Setup" or "Setup.md").
    const target = (p) => c.wiki ? `/${c.owner}/${c.repo}/wiki/${encPath(p.replace(/\.md$/i, ''))}` : `/${c.owner}/${c.repo}/blob/${encPath(c.rev)}/${encPath(p)}`;
    root.querySelectorAll('a[href]').forEach((a) => {
      const r = resolveRel(a.getAttribute('href'), c);
      if (r) a.setAttribute('href', `${target(r.path)}${r.hash ? '#' + r.hash : ''}`);
    });
    root.querySelectorAll('img[src]').forEach((img) => {
      const r = c.wiki ? null : resolveRel(img.getAttribute('src'), c);
      if (r) img.setAttribute('src', `/raw/${c.owner}/${c.repo}/${encPath(c.rev)}/${encPath(r.path)}`);
      img.setAttribute('loading', 'lazy');
    });
  }
  root.querySelectorAll('blockquote').forEach((bq) => {
    const p = bq.firstElementChild;
    const m = p && /^\s*\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\]\s*/i.exec(p.textContent);
    if (!m) return;
    const kind = m[1].toLowerCase();
    p.innerHTML = p.innerHTML.replace(/^\s*\[![A-Za-z]+\]\s*(<br>)?/, '');
    bq.classList.add('alert', 'alert-' + kind);
    const title = document.createElement('p');
    title.className = 'alert-title';
    title.textContent = kind[0].toUpperCase() + kind.slice(1);
    bq.prepend(title);
  });
  root.querySelectorAll('table').forEach((tb) => {
    const w = document.createElement('div');
    w.className = 'table-wrap';
    tb.replaceWith(w);
    w.append(tb);
  });
  const div = document.createElement('div');
  div.append(root);
  return div.innerHTML;
}

/** Load highlight languages used by fenced blocks before rendering. */
export async function preloadLangs(src) {
  const langs = new Set([...String(src || '').matchAll(/^```\s*([\w+-]+)/gm)].map((m) => m[1].toLowerCase()));
  await Promise.all([...langs].map((l) => ensureLang(l)));
}
