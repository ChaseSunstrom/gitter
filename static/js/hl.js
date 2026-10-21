// Syntax highlighting with highlight.js, split into per-line HTML so code
// views can number and link individual lines.
import hljs from '../vendor/highlight.min.js';
import { esc } from './lib.js';

const EXTRA = ['clojure', 'cmake', 'dart', 'dockerfile', 'elixir', 'elm', 'erlang', 'fsharp', 'groovy', 'haskell', 'julia', 'latex', 'nginx', 'nix', 'ocaml', 'powershell', 'protobuf', 'scala', 'vim'];
const loading = new Map();

async function ensure(lang) {
  if (!lang || hljs.getLanguage(lang)) return !!lang && !!hljs.getLanguage(lang);
  if (!EXTRA.includes(lang)) return false;
  if (!loading.has(lang)) {
    loading.set(lang, import(`../vendor/hljs-lang/${lang}.min.js`).then((m) => hljs.registerLanguage(lang, m.default)).catch(() => {}));
  }
  await loading.get(lang);
  return !!hljs.getLanguage(lang);
}

/** Split highlight.js HTML into lines, closing and reopening spans across newlines. */
export function splitLines(htmlStr) {
  const lines = [];
  const stack = [];
  let cur = '';
  const re = /<span[^>]*>|<\/span>|\n|[^<\n]+|</g;
  let m;
  while ((m = re.exec(htmlStr))) {
    const t = m[0];
    if (t === '\n') {
      lines.push(cur + '</span>'.repeat(stack.length));
      cur = stack.join('');
    } else if (t.startsWith('<span')) { stack.push(t); cur += t; }
    else if (t === '</span>') { stack.pop(); cur += t; }
    else cur += t;
  }
  lines.push(cur);
  return lines;
}

/** Highlighted lines for `code`, or plain escaped lines if the language is unknown or the file is huge. */
export async function highlightLines(code, lang) {
  const plain = () => code.split('\n').map(esc);
  if (code.length > 600_000 || !(await ensure(lang))) return plain();
  try { return splitLines(hljs.highlight(code, { language: lang, ignoreIllegals: true }).value); }
  catch { return plain(); }
}

/** Highlight a single line (used by diffs where context is partial). */
export function highlightOne(line, lang) {
  if (!lang || !hljs.getLanguage(lang)) return esc(line);
  try { return hljs.highlight(line, { language: lang, ignoreIllegals: true }).value; } catch { return esc(line); }
}
export { ensure as ensureLang };

export function highlightBlock(code, lang) {
  if (lang && hljs.getLanguage(lang)) {
    try { return hljs.highlight(code, { language: lang, ignoreIllegals: true }).value; } catch { /* fall through */ }
  }
  return esc(code);
}
