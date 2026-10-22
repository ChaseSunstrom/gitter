// Shared helpers: safe HTML templating, API client, formatting, avatars, UI primitives.

// ---------- safe HTML ----------
export class Safe { constructor(s) { this.s = s; } toString() { return this.s; } }
export const raw = (s) => new Safe(String(s ?? ''));
const ESC = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' };
export const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ESC[c]);
function val(v) {
  if (v == null || v === false || v === true) return '';
  if (v instanceof Safe) return v.s;
  if (Array.isArray(v)) return v.map(val).join('');
  return esc(v);
}
/** Tagged template: interpolations are escaped unless wrapped in raw()/html``. */
export function html(strings, ...vals) {
  let out = strings[0];
  for (let i = 0; i < vals.length; i++) out += val(vals[i]) + strings[i + 1];
  return new Safe(out);
}
/** Only http(s), mailto and same-site links survive; anything else becomes '#'. */
export function safeUrl(u) {
  const s = String(u || '').trim();
  return /^(https?:\/\/|mailto:|\/(?!\/)|#)/i.test(s) ? s : '#';
}
export const enc = (s) => encodeURIComponent(s);
/** Encode each path segment but keep the slashes. */
export const encPath = (p) => String(p || '').split('/').map(encodeURIComponent).join('/');
export const $ = (sel, root = document) => root.querySelector(sel);
export const $$ = (sel, root = document) => [...root.querySelectorAll(sel)];

// ---------- state ----------
export const state = { me: null, signup: true, repo: null };

// ---------- API ----------
const cache = new Map(); // url -> { t, p }
const TTL = 20_000;
export class ApiError extends Error { constructor(status, msg, code) { super(msg); this.status = status; this.code = code; } }

async function request(method, url, body) {
  const opts = { method, headers: { Accept: 'application/json' }, credentials: 'same-origin' };
  if (body !== undefined) { opts.headers['Content-Type'] = 'application/json'; opts.body = JSON.stringify(body); }
  let res;
  try { res = await fetch('/api' + url, opts); } catch { throw new ApiError(0, "Can't reach the server. Check your connection."); }
  let data = null;
  try { data = await res.json(); } catch { /* empty body */ }
  if (!res.ok) throw new ApiError(res.status, data?.error || `Request failed (${res.status})`, data?.code);
  return data;
}

/** GET with a short-lived cache and in-flight de-duplication (used by hover prefetch). */
export function get(url, { fresh = false } = {}) {
  const hit = cache.get(url);
  if (!fresh && hit && Date.now() - hit.t < TTL) return hit.p;
  const p = request('GET', url);
  cache.set(url, { t: Date.now(), p });
  p.catch(() => cache.delete(url));
  return p;
}
export function peek(url) { const h = cache.get(url); return h && Date.now() - h.t < TTL ? h.p : null; }
export function invalidate(prefix = '') { for (const k of [...cache.keys()]) if (k.startsWith(prefix)) cache.delete(k); }
export async function send(method, url, body) {
  const b = body ?? (method === 'GET' ? undefined : {});
  let r;
  try { r = await request(method, url, b); } catch (e) {
    // Sensitive actions ask for the password again (sudo mode), then go through as if nothing happened.
    if (e.code !== 'reauth' || !(await reauth())) throw e;
    r = await request(method, url, b);
  }
  invalidate();
  return r;
}

let reauthing = null;
/** Ask for the password (and a 2FA code when the account has one). Resolves true when confirmed. */
function reauth() {
  reauthing ??= modal({
    title: 'Confirm it’s you',
    body: html`<p class="muted">This is a sensitive action. Enter your password to continue; you won’t be asked again for a few minutes.</p>
      <form id="ra-form" class="stack" novalidate>
        <div class="field"><label for="ra-pw">Password</label><input class="input" id="ra-pw" type="password" autocomplete="current-password" autofocus></div>
        <div class="field" id="ra-code-f" hidden><label for="ra-code">Authentication code</label><input class="input" id="ra-code" inputmode="numeric" autocomplete="one-time-code" placeholder="123456, or a recovery code"></div>
        <p class="field-err" id="ra-err" role="alert" hidden></p>
      </form>`,
    foot: html`<button class="btn" data-close>Cancel</button><button class="btn primary" data-ok>Confirm</button>`,
    onMount(el, close) {
      const ok = el.querySelector('[data-ok]'), err = el.querySelector('#ra-err');
      const submit = async (e) => {
        e?.preventDefault();
        busy(ok, true);
        err.hidden = true;
        try {
          await request('POST', '/auth/reauth', { password: el.querySelector('#ra-pw').value, code: el.querySelector('#ra-code').value });
          close(true);
        } catch (x) {
          err.textContent = x.message; err.hidden = false;
          if (x.code === 'need_code') { el.querySelector('#ra-code-f').hidden = false; el.querySelector('#ra-code').focus(); }
        } finally { busy(ok, false); }
      };
      ok.addEventListener('click', submit);
      el.querySelector('#ra-form').addEventListener('keydown', (e) => { if (e.key === 'Enter') submit(e); });
    },
  }).finally(() => { reauthing = null; });
  return reauthing;
}

// ---------- formatting ----------
const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' });
export function ago(ts) {
  if (!ts) return '';
  const s = Math.round(ts - Date.now() / 1000);
  const a = Math.abs(s);
  if (a < 45) return 'just now';
  const units = [[60, 'second'], [3600, 'minute'], [86400, 'hour'], [86400 * 7, 'day'], [86400 * 30, 'week'], [86400 * 365, 'month'], [Infinity, 'year']];
  const div = { second: 1, minute: 60, hour: 3600, day: 86400, week: 86400 * 7, month: 86400 * 30, year: 86400 * 365 };
  for (const [lim, u] of units) if (a < lim) return rtf.format(Math.round(s / div[u]), u);
  return '';
}
export const fullDate = (ts) => ts ? new Date(ts * 1000).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' }) : '';
export const day = (ts) => new Date(ts * 1000).toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' });
export const timeEl = (ts) => html`<time datetime="${new Date(ts * 1000).toISOString()}" title="${fullDate(ts)}">${ago(ts)}</time>`;
export function bytes(n) {
  if (n == null) return '';
  const u = ['B', 'KB', 'MB', 'GB'];
  let i = 0;
  while (n >= 1024 && i < u.length - 1) { n /= 1024; i++; }
  return `${i ? n.toFixed(n < 10 ? 1 : 0) : n} ${u[i]}`;
}
export const plural = (n, one, many = one + 's') => `${Number(n).toLocaleString()} ${n === 1 ? one : many}`;
export const short = (sha) => String(sha || '').slice(0, 7);
export const num = (n) => Number(n || 0).toLocaleString(undefined, { notation: n >= 10000 ? 'compact' : 'standard' });

// ---------- avatars (identicons, no external requests) ----------
function hash32(s) { let h = 2166136261; for (const c of String(s)) { h ^= c.codePointAt(0); h = Math.imul(h, 16777619); } return h >>> 0; }
const avatarCache = new Map();
export function avatarUrl(seed) {
  const key = String(seed || '?').toLowerCase();
  if (avatarCache.has(key)) return avatarCache.get(key);
  const h = hash32(key);
  const hue = h % 360;
  const fg = `hsl(${hue} 58% 52%)`;
  const bg = `hsl(${hue} 45% 94%)`;
  let cells = '';
  for (let y = 0; y < 5; y++) for (let x = 0; x < 3; x++) {
    if ((hash32(key + ':' + (y * 3 + x)) & 3) > 1) {
      cells += `<rect x="${x + 1}" y="${y + 1}" width="1" height="1"/>`;
      if (x < 2) cells += `<rect x="${5 - x}" y="${y + 1}" width="1" height="1"/>`;
    }
  }
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 7 7" shape-rendering="crispEdges"><rect width="7" height="7" fill="${bg}"/><g fill="${fg}">${cells}</g></svg>`;
  const url = 'data:image/svg+xml,' + encodeURIComponent(svg);
  avatarCache.set(key, url);
  return url;
}
export const avatar = (seed, size = 20, cls = '') => html`<img class="avatar ${cls}" src="${avatarUrl(seed)}" width="${size}" height="${size}" alt="" loading="lazy">`;

// ---------- toasts ----------
export function toast(msg, kind = '') {
  let host = $('.toasts');
  if (!host) { host = document.createElement('div'); host.className = 'toasts'; host.setAttribute('role', 'status'); document.body.append(host); }
  const t = document.createElement('div');
  t.className = 'toast ' + kind;
  t.textContent = msg;
  host.append(t);
  setTimeout(() => { t.classList.add('out'); setTimeout(() => t.remove(), 260); }, kind === 'err' ? 5200 : 2600);
}
export const toastErr = (e) => toast(e?.message || String(e), 'err');

// ---------- copy ----------
export async function copy(text, label = 'Copied') {
  try { await navigator.clipboard.writeText(text); toast(label); }
  catch {
    const ta = document.createElement('textarea'); ta.value = text; document.body.append(ta); ta.select();
    try { document.execCommand('copy'); toast(label); } catch { toast("Couldn't copy", 'err'); }
    ta.remove();
  }
}

// ---------- modal ----------
/** Opens a modal. `body` is Safe HTML; `onMount(el, close)` wires it up. Resolves with what close() was given. */
export function modal({ title, body, foot = '', wide = false, onMount }) {
  return new Promise((resolve) => {
    const prev = document.activeElement;
    const scrim = document.createElement('div');
    scrim.className = 'scrim';
    scrim.innerHTML = html`<div class="modal ${wide ? 'wide' : ''}" role="dialog" aria-modal="true" aria-label="${title}">
      <div class="modal-head"><h2>${title}</h2><button class="btn ghost sm icon" data-close aria-label="Close">${raw('<svg class="ico" viewBox="0 0 24 24"><path d="M6 6l12 12M18 6 6 18"/></svg>')}</button></div>
      <div class="modal-body">${body}</div>${foot ? html`<div class="modal-foot">${foot}</div>` : ''}</div>`.s;
    const close = (v) => { scrim.remove(); document.removeEventListener('keydown', onKey, true); prev?.focus?.(); resolve(v); };
    const onKey = (e) => { if (e.key === 'Escape') { e.stopPropagation(); close(null); } };
    scrim.addEventListener('mousedown', (e) => { if (e.target === scrim) close(null); });
    scrim.addEventListener('click', (e) => { if (e.target.closest('[data-close]')) close(null); });
    document.addEventListener('keydown', onKey, true);
    document.body.append(scrim);
    const m = scrim.firstElementChild;
    onMount?.(m, close);
    (m.querySelector('[autofocus]') || m.querySelector('input,textarea,select,button.primary') || m).focus?.();
  });
}

export function confirmDialog({ title, text, ok = 'Confirm', danger = false, typed = '' }) {
  return modal({
    title,
    body: html`<p class="muted">${text}</p>${typed ? html`<div class="field"><label for="cf-typed">Type <b>${typed}</b> to confirm</label><input id="cf-typed" class="input" autocomplete="off" autofocus></div>` : ''}`,
    foot: html`<button class="btn" data-close>Cancel</button><button class="btn ${danger ? 'danger' : 'primary'}" data-ok ${typed ? 'disabled' : ''}>${ok}</button>`,
    onMount(el, close) {
      const okBtn = el.querySelector('[data-ok]');
      const inp = el.querySelector('#cf-typed');
      inp?.addEventListener('input', () => { okBtn.disabled = inp.value.trim().toLowerCase() !== typed.toLowerCase(); });
      okBtn.addEventListener('click', () => close(true));
      el.addEventListener('keydown', (e) => { if (e.key === 'Enter' && !okBtn.disabled) close(true); });
    },
  });
}

// ---------- popover menus ----------
let openPop = null;
export function closePop() { openPop?.remove(); openPop = null; }
/** Show `content` (Safe) anchored under `anchor`. Returns the popover element. */
export function popover(anchor, content, { align = 'left', onMount, width } = {}) {
  closePop();
  const pop = document.createElement('div');
  pop.className = 'pop';
  if (width) pop.style.width = width + 'px';
  pop.innerHTML = content.s ?? content;
  document.body.append(pop);
  const r = anchor.getBoundingClientRect();
  const pw = pop.offsetWidth;
  let left = align === 'right' ? r.right - pw : r.left;
  left = Math.max(8, Math.min(left, innerWidth - pw - 8));
  let top = r.bottom + 6;
  if (top + pop.offsetHeight > innerHeight - 8 && r.top > pop.offsetHeight + 12) top = r.top - pop.offsetHeight - 6;
  pop.style.left = left + 'px';
  pop.style.top = top + 'px';
  openPop = pop;
  onMount?.(pop);
  setTimeout(() => {
    const off = (e) => {
      if (!pop.isConnected) return document.removeEventListener('mousedown', off, true);
      if (!pop.contains(e.target) && !anchor.contains(e.target)) { closePop(); document.removeEventListener('mousedown', off, true); }
    };
    document.addEventListener('mousedown', off, true);
  });
  return pop;
}
document.addEventListener('keydown', (e) => { if (e.key === 'Escape' && openPop) closePop(); });
addEventListener('scroll', () => { if (openPop && !openPop.matches(':hover')) closePop(); }, { passive: true });

// ---------- misc ----------
export const debounce = (fn, ms) => { let t; return (...a) => { clearTimeout(t); t = setTimeout(() => fn(...a), ms); }; };
export function busy(btn, on = true) { if (!btn) return; btn.classList.toggle('busy', on); btn.disabled = on; }
/** Wrap an async click/submit handler: shows busy state and toasts errors. */
export function action(btn, fn) {
  return async (e) => {
    e?.preventDefault?.();
    if (btn?.disabled) return;
    busy(btn, true);
    try { await fn(e); } catch (err) { toastErr(err); } finally { busy(btn, false); }
  };
}
export function formData(form) { return Object.fromEntries(new FormData(form).entries()); }
export function langOf(path) {
  const f = String(path).split('/').pop().toLowerCase();
  const ext = f.includes('.') ? f.split('.').pop() : f;
  const m = { rs: 'rust', ts: 'typescript', tsx: 'typescript', js: 'javascript', jsx: 'javascript', mjs: 'javascript', cjs: 'javascript', py: 'python', go: 'go',
    rb: 'ruby', java: 'java', kt: 'kotlin', kts: 'kotlin', swift: 'swift', c: 'c', h: 'c', cc: 'cpp', cpp: 'cpp', hpp: 'cpp', cxx: 'cpp', cs: 'csharp', php: 'php',
    sh: 'bash', bash: 'bash', zsh: 'bash', fish: 'bash', ps1: 'powershell', html: 'xml', htm: 'xml', xml: 'xml', svg: 'xml', vue: 'xml', svelte: 'xml', css: 'css', scss: 'scss', less: 'less',
    json: 'json', jsonc: 'json', yml: 'yaml', yaml: 'yaml', toml: 'ini', ini: 'ini', cfg: 'ini', md: 'markdown', markdown: 'markdown', sql: 'sql', lua: 'lua', pl: 'perl', pm: 'perl',
    r: 'r', dart: 'dart', ex: 'elixir', exs: 'elixir', erl: 'erlang', hs: 'haskell', scala: 'scala', clj: 'clojure', cljs: 'clojure', ml: 'ocaml', fs: 'fsharp', elm: 'elm',
    nix: 'nix', dockerfile: 'dockerfile', makefile: 'makefile', mk: 'makefile', gradle: 'groovy', groovy: 'groovy', jl: 'julia', tex: 'latex', proto: 'protobuf', vim: 'vim',
    diff: 'diff', patch: 'diff', graphql: 'graphql', gql: 'graphql', wasm: 'wasm', zig: 'zig', cmake: 'cmake', nginx: 'nginx', m: 'objectivec', mm: 'objectivec', txt: 'plaintext' };
  if (f === 'dockerfile' || f.startsWith('dockerfile.')) return 'dockerfile';
  if (f === 'makefile' || f === 'gnumakefile') return 'makefile';
  if (f === 'cmakelists.txt') return 'cmake';
  return m[ext] || null;
}

// ---------- security log ----------
const AUDIT = {
  signup: 'Created the account', login: 'Signed in', password_changed: 'Changed the password',
  '2fa_enabled': 'Turned on two-factor authentication', '2fa_disabled': 'Turned off two-factor authentication',
  '2fa_reset': 'Two-factor authentication was reset by an admin', '2fa_failed': 'A wrong two-factor code was entered',
  recovery_codes_replaced: 'Replaced the recovery codes', session_revoked: 'Signed out a session',
  token_created: 'Created an access token', token_deleted: 'Deleted an access token',
  repo_deleted: 'Deleted a repository', repo_visibility: 'Changed who can see a repository',
  admin_granted: 'Made someone an admin', admin_revoked: 'Removed someone’s admin rights',
  user_suspended: 'Suspended an account', user_unsuspended: 'Restored an account', user_deleted: 'Deleted an account',
  settings_changed: 'Changed site settings', invite_created: 'Created an invite link',
  moderation_dismiss: 'Dismissed a report', moderation_hide: 'Hid reported content', moderation_unhide: 'Unhid content', moderation_suspend: 'Suspended a reported account',
  wiki_page_deleted: 'Deleted a wiki page', discussion_deleted: 'Deleted a discussion', discussion_reply_deleted: 'Removed a reply in a discussion', snippet_deleted: 'Deleted a snippet',
  ci_secret_set: 'Set a CI secret', ci_secret_deleted: 'Deleted a CI secret', ci_runner_created: 'Added a CI runner', ci_runner_deleted: 'Removed a CI runner',
  webhook_created: 'Added a webhook', webhook_changed: 'Changed a webhook', webhook_deleted: 'Deleted a webhook',
};
export const auditLabel = (a) => AUDIT[a] || String(a).replace(/_/g, ' ');
/** "Firefox on Linux" from a user agent string. */
export function deviceName(ua) {
  const b = [['Firefox', 'Firefox'], ['Edg', 'Edge'], ['Chrome', 'Chrome'], ['Safari', 'Safari'], ['git', 'git'], ['curl', 'curl']].find(([k]) => ua.includes(k));
  const o = [['Windows', 'Windows'], ['Mac OS', 'macOS'], ['Android', 'Android'], ['iPhone', 'iPhone'], ['Linux', 'Linux']].find(([k]) => ua.includes(k));
  return `${b ? b[1] : 'Unknown browser'}${o ? ' on ' + o[1] : ''}`;
}

// ---------- reporting ----------
const REASONS = [['spam', 'Spam or advertising'], ['abuse', 'Harassment or abuse'], ['illegal', 'Illegal content'], ['malware', 'Malware or phishing'], ['impersonation', 'Impersonation'], ['other', 'Something else']];
/** Ask why, then report `target` ({ kind, owner, repo, number, comment, user }) to the site's moderators. */
export function reportDialog(target, what) {
  return modal({
    title: `Report ${what}`,
    body: html`<p class="muted small">Reports go to this site's moderators. They won't see who you are unless they're admins.</p>
      <div class="choice report-reasons">${REASONS.map(([v, label], i) => html`<label><input type="radio" name="reason" value="${v}" ${i === 0 ? 'checked' : ''}><span>${label}</span></label>`)}</div>
      <div class="field"><label for="rp-details">Anything moderators should know? <span class="muted">(optional)</span></label><textarea class="textarea" id="rp-details" maxlength="2000" rows="3"></textarea></div>`,
    foot: html`<button class="btn" data-close>Cancel</button><button class="btn danger" data-ok>Send report</button>`,
    onMount(el, close) {
      const ok = el.querySelector('[data-ok]');
      ok.addEventListener('click', async () => {
        busy(ok, true);
        try {
          await send('POST', '/reports', { ...target, reason: el.querySelector('input[name=reason]:checked').value, details: el.querySelector('#rp-details').value });
          toast('Thanks. A moderator will take a look.');
          close(true);
        } catch (e) { toastErr(e); } finally { busy(ok, false); }
      });
    },
  });
}
