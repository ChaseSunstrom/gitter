// App shell: router, navigation, prefetch, theme, top bar.
import { html, raw, state, get, send, toastErr, popover, closePop, avatar, $, copy, ApiError } from './lib.js';
import { icon, logo } from './icons.js';
import { initPalette, openPalette } from './palette.js';

// ---------- routes ----------
const ROUTES = [
  ['/', 'home'], ['/login', 'auth', { mode: 'login' }], ['/signup', 'auth', { mode: 'signup' }], ['/new', 'newrepo'],
  ['/explore', 'explore'], ['/settings', 'settings'], ['/settings/:tab', 'settings'],
  ['/admin', 'admin'], ['/admin/:tab', 'admin'],
  ['/snippets', 'snippets', { view: 'list' }], ['/snippets/mine', 'snippets', { view: 'mine' }], ['/snippets/new', 'snippets', { view: 'edit' }],
  ['/snippets/:id', 'snippets', { view: 'show' }], ['/snippets/:id/edit', 'snippets', { view: 'edit' }],
  ['/snippets/:id/revisions', 'snippets', { view: 'revisions' }],
  ['/orgs/new', 'orgs', { view: 'new' }], ['/orgs/:org/settings', 'orgs', { view: 'settings' }],
  ['/:user', 'profile'],
  ['/:owner/:repo', 'repo', { view: 'tree' }],
  ['/:owner/:repo/tree/*', 'repo', { view: 'tree' }],
  ['/:owner/:repo/blob/*', 'repo', { view: 'blob' }],
  ['/:owner/:repo/blame/*', 'repo', { view: 'blame' }],
  ['/:owner/:repo/edit/*', 'editor', { mode: 'edit' }],
  ['/:owner/:repo/new/*', 'editor', { mode: 'new' }],
  ['/:owner/:repo/upload/*', 'editor', { mode: 'upload' }],
  ['/:owner/:repo/commits', 'commits'], ['/:owner/:repo/commits/*', 'commits'], ['/:owner/:repo/history/*', 'history'],
  ['/:owner/:repo/commit/:sha', 'commit'],
  ['/:owner/:repo/compare', 'compare'], ['/:owner/:repo/compare/*', 'compare'],
  ['/:owner/:repo/branches', 'branches', { view: 'branches' }], ['/:owner/:repo/tags', 'branches', { view: 'tags' }],
  ['/:owner/:repo/issues', 'issues', { kind: 'issue' }], ['/:owner/:repo/issues/new', 'newissue', { kind: 'issue' }],
  ['/:owner/:repo/issues/:n', 'issue', { kind: 'issue' }],
  ['/:owner/:repo/pulls', 'issues', { kind: 'pr' }], ['/:owner/:repo/pull/:n', 'issue', { kind: 'pr', tab: 'conversation' }],
  ['/:owner/:repo/pull/:n/:tab', 'issue', { kind: 'pr' }],
  ['/:owner/:repo/ci', 'ci'], ['/:owner/:repo/ci/:n', 'ci'],
  ['/:owner/:repo/labels', 'labels'], ['/:owner/:repo/settings', 'reposettings'],
  ['/:owner/:repo/settings/hooks/new', 'webhook', { create: true }], ['/:owner/:repo/settings/hooks/:id', 'webhook'],
  ['/:owner/:repo/insights', 'insights'], ['/:owner/:repo/search', 'search'],
  ['/:owner/:repo/wiki', 'wiki', { view: 'page' }], ['/:owner/:repo/wiki/_new', 'wiki', { view: 'edit', create: true }],
  ['/:owner/:repo/wiki/:page', 'wiki', { view: 'page' }], ['/:owner/:repo/wiki/:page/edit', 'wiki', { view: 'edit' }],
  ['/:owner/:repo/wiki/:page/history', 'wiki', { view: 'history' }],
  ['/:owner/:repo/discussions', 'discussions', { view: 'list' }], ['/:owner/:repo/discussions/new', 'discussions', { view: 'new' }],
  ['/:owner/:repo/discussions/:n', 'discussions', { view: 'show' }],
  ['/:owner/:repo/releases', 'releases', { view: 'list' }], ['/:owner/:repo/releases/new', 'releases', { view: 'edit' }],
  ['/:owner/:repo/releases/:id', 'releases', { view: 'show' }], ['/:owner/:repo/releases/:id/edit', 'releases', { view: 'edit' }],
  ['/:owner/:repo/packages', 'packages'],
];

function compile([pattern, page, extra = {}]) {
  const keys = [];
  const src = pattern.split('/').map((seg) => {
    if (seg === '*') { keys.push('rest'); return '(.*)'; }
    if (seg.startsWith(':')) { keys.push(seg.slice(1)); return '([^/]+)'; }
    return seg.replace(/[.+?^${}()|[\]\\]/g, '\\$&');
  }).join('/');
  return { re: new RegExp(`^${src || '/'}/?$`), keys, page, extra };
}
const compiled = ROUTES.map(compile);

function safeDecode(s) { try { return decodeURIComponent(s); } catch { return s; } }

export function match(pathname) {
  for (const r of compiled) {
    const m = r.re.exec(pathname);
    if (!m) continue;
    const params = { ...r.extra };
    r.keys.forEach((k, i) => { params[k] = safeDecode(m[i + 1] || ''); });
    return { page: r.page, params };
  }
  return null;
}

const modules = new Map();
const loadPage = (name) => {
  if (!modules.has(name)) modules.set(name, import(`./pages/${name}.js`));
  return modules.get(name);
};

// ---------- progress bar ----------
const bar = document.createElement('div');
bar.className = 'progress';
document.body.append(bar);
let barTimer;
function progress(on) {
  clearTimeout(barTimer);
  if (on) {
    bar.style.transition = 'none'; bar.style.width = '0'; bar.offsetWidth; // restart
    bar.style.transition = ''; barTimer = setTimeout(() => { bar.classList.add('on'); bar.style.width = '72%'; }, 90);
  } else {
    bar.style.width = '100%';
    barTimer = setTimeout(() => { bar.classList.remove('on'); setTimeout(() => { bar.style.width = '0'; }, 300); }, 160);
  }
}

// ---------- navigation ----------
const pageEl = () => $('#page');
let seq = 0;
let current = null;

function ctxFor(url) {
  const u = new URL(url, location.href);
  const m = match(u.pathname);
  return { url: u, path: u.pathname, query: u.searchParams, params: m?.params || {}, page: m?.page || null };
}

export async function navigate(url, { replace = false, scroll = true, pop = false } = {}) {
  const ctx = ctxFor(url);
  const my = ++seq;
  closePop();
  progress(true);
  let view;
  try {
    if (!ctx.page) throw new ApiError(404, 'Page not found');
    const mod = await loadPage(ctx.page);
    Object.assign(state, { repo: null, rev: null, sha: null, revPath: undefined });
    view = await mod.default(ctx);
  } catch (e) {
    view = errorView(e);
  }
  if (my !== seq) return;
  if (view?.redirect) { progress(false); return navigate(view.redirect, { replace: true }); }
  if (!pop) {
    const target = ctx.url.pathname + ctx.url.search + ctx.url.hash;
    if (replace || target === location.pathname + location.search + location.hash) history.replaceState({ y: 0 }, '', target);
    else { history.replaceState({ ...(history.state || {}), y: scrollY }, ''); history.pushState({ y: 0 }, '', target); }
  }
  const swap = () => {
    // Each page gets a fresh root, so listeners a page attaches die with it.
    const el = document.createElement('div');
    current?.unmount?.();
    el.innerHTML = view.html?.s ?? '';
    // Staying inside the same repository: keep the header still and only fade the body in.
    const prevRepo = pageEl().querySelector('.repo-head')?.dataset.repo;
    const nextRepo = el.querySelector('.repo-head')?.dataset.repo;
    el.className = 'page-root' + (replace ? '' : prevRepo && prevRepo === nextRepo ? ' enter-body' : ' enter');
    pageEl().replaceChildren(el);
    const site = state.instance?.name || 'Gitter';
    document.title = view.title ? `${view.title} · ${site}` : site;
    current = view;
    view.mount?.(el, ctx);
    paintNav(ctx);
    if (pop) scrollTo(0, history.state?.y || 0);
    else if (ctx.url.hash) jumpToHash(ctx.url.hash, false);
    else if (scroll) scrollTo(0, 0);
  };
  swap();
  progress(false);
}

/** Re-render the current page in place (after a change). */
export const refresh = () => navigate(location.href, { replace: true, scroll: false });

function errorView(e) {
  const status = e?.status || 500;
  const nf = status === 404;
  if (status === 401) return { redirect: '/login?next=' + encodeURIComponent(location.pathname + location.search) };
  return {
    title: nf ? 'Not found' : 'Error',
    html: html`<div class="wrap narrow"><div class="empty fade" style="padding:80px 20px">
      <div class="err-logo">${logo(56)}</div>
      <h1 style="font-size:44px;letter-spacing:-.04em">${nf ? '404' : status}</h1>
      <h3>${nf ? "This page doesn't exist, or you don't have access to it." : e?.message || 'Something went wrong'}</h3>
      <div class="row" style="margin-top:8px"><button class="btn" data-back>Go back</button><a class="btn primary" href="/">Home</a></div>
    </div></div>`,
  };
}

function jumpToHash(hash, smooth = true) {
  const id = safeDecode(hash.slice(1));
  const el = document.getElementById(id) || document.getElementById('user-content-' + id);
  if (el) el.scrollIntoView({ behavior: smooth ? 'smooth' : 'auto', block: 'start' });
}

function internal(a) {
  if (!a || a.target || a.hasAttribute('download') || a.dataset.native !== undefined) return false;
  const href = a.getAttribute('href');
  if (!href || href.startsWith('javascript:') || href.startsWith('mailto:')) return false;
  const u = new URL(a.href, location.href);
  if (u.origin !== location.origin) return false;
  return !/^\/(raw|archive|api|assets)\//.test(u.pathname) && !/\.git\//.test(u.pathname);
}

document.addEventListener('click', (e) => {
  if (e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
  if (e.target.closest('[data-back]')) { e.preventDefault(); history.back(); return; }
  const cp = e.target.closest('[data-copy]');
  if (cp) { e.preventDefault(); copy(cp.dataset.copy, cp.dataset.copyLabel || 'Copied'); return; }
  const a = e.target.closest('a[href]');
  if (!a) return;
  const href = a.getAttribute('href');
  if (href.startsWith('#')) { e.preventDefault(); history.replaceState(history.state, '', href); jumpToHash(href); return; }
  if (!internal(a)) return;
  e.preventDefault();
  const u = new URL(a.href);
  if (u.pathname === location.pathname && u.search === location.search && u.hash) { history.replaceState(history.state, '', u.hash); jumpToHash(u.hash); return; }
  navigate(a.href);
});

addEventListener('popstate', () => navigate(location.href, { pop: true }));
if ('scrollRestoration' in history) history.scrollRestoration = 'manual';

// Prefetch on hover: load the page module and warm its data.
let hoverTimer;
document.addEventListener('pointerover', (e) => {
  const a = e.target.closest?.('a[href]');
  if (!a || !internal(a)) return;
  clearTimeout(hoverTimer);
  hoverTimer = setTimeout(async () => {
    const ctx = ctxFor(a.href);
    if (!ctx.page) return;
    try { const mod = await loadPage(ctx.page); mod.prefetch?.(ctx); } catch { /* ignore */ }
  }, 65);
});
document.addEventListener('pointerout', () => clearTimeout(hoverTimer));
// Pressing a link starts loading right away, ~100 ms before the click lands.
document.addEventListener('pointerdown', (e) => {
  const a = e.target.closest?.('a[href]');
  if (!a || e.button !== 0 || !internal(a)) return;
  const ctx = ctxFor(a.href);
  if (ctx.page) loadPage(ctx.page).then((mod) => mod.prefetch?.(ctx)).catch(() => {});
});

// ---------- theme ----------
export function getTheme() { try { return localStorage.getItem('theme') || 'system'; } catch { return 'system'; } }
export function setTheme(t) {
  const apply = () => {
    if (t === 'system') document.documentElement.removeAttribute('data-theme');
    else document.documentElement.setAttribute('data-theme', t);
  };
  try { localStorage.setItem('theme', t); } catch { /* private mode */ }
  if (document.startViewTransition && !matchMedia('(prefers-reduced-motion: reduce)').matches) document.startViewTransition(apply);
  else apply();
}
/** Color themes ("skins"). Each also has its own fonts and shapes; see css/themes.css. */
export const SKINS = [
  { id: 'aurora', name: 'Aurora', blurb: 'Violet to magenta to amber, on deep blue-black glass.', bg: '#07080d', surface: '#131623', ink: '#eceefe', grad: 'linear-gradient(110deg,#7c5cff,#e255d6 55%,#ff9a5a)' },
  { id: 'forge', name: 'Forge', blurb: 'Warm charcoal and molten ember.', bg: '#110d0b', surface: '#211915', ink: '#f7ede5', grad: 'linear-gradient(110deg,#ffc04a,#ff6a2b 55%,#e0245e)' },
  { id: 'ultraviolet', name: 'Ultraviolet', blurb: 'Purple-black with electric cyan and hot pink.', bg: '#0a0614', surface: '#180f2d', ink: '#f1ebff', grad: 'linear-gradient(90deg,#3ef0e0,#8f7bff 50%,#ff4fd8)' },
  { id: 'daylight', name: 'Daylight', blurb: 'Bright paper, ink outlines and electric cobalt.', bg: '#ffffff', surface: '#f6f7f9', ink: '#0b0b0c', grad: '#2f4bff' },
];
export function getSkin() { try { return localStorage.getItem('skin') || 'aurora'; } catch { return 'aurora'; } }
export function setSkin(id) {
  const apply = () => { if (id === 'aurora') document.documentElement.removeAttribute('data-skin'); else document.documentElement.setAttribute('data-skin', id); };
  try { localStorage.setItem('skin', id); } catch { /* private mode */ }
  if (document.startViewTransition && !matchMedia('(prefers-reduced-motion: reduce)').matches) document.startViewTransition(apply);
  else apply();
}

export function cycleTheme() {
  const dark = document.documentElement.dataset.theme === 'dark' || (!document.documentElement.dataset.theme && matchMedia('(prefers-color-scheme: dark)').matches);
  setTheme(dark ? 'light' : 'dark');
}

// ---------- top bar ----------
function topbar() {
  const me = state.me;
  return html`<div class="in">
    <a class="brand" href="/" aria-label="Gitter home">${logo(28)}<span class="hide-sm">${(state.instance?.name || 'Gitter').toLowerCase() === 'gitter' ? 'gitter' : state.instance.name}</span></a>
    <button class="jump" id="jump" aria-label="Search or jump to">${icon('search', 'sm')}<span>Search or jump to…</span><kbd>${navigator.platform.includes('Mac') ? '⌘' : 'Ctrl'} K</kbd></button>
    <nav class="topnav">
      <a class="nav hide-sm" href="/explore" data-nav="explore">Explore</a>
      <a class="nav hide-sm" href="/snippets" data-nav="snippets">Snippets</a>
      ${me ? html`
        <button class="btn ghost icon" id="new-menu" aria-label="Create new">${icon('plus')}</button>
        <button class="btn ghost" id="user-menu" aria-label="Account menu" style="padding:0 4px">${avatar(me.username, 26)}${icon('chev-down', 'sm')}</button>`
      : html`<a class="btn ghost" href="/login">Sign in</a>${state.signup ? html`<a class="btn primary" href="/signup">Sign up</a>` : ''}`}
    </nav></div>`;
}

function paintNav(ctx) {
  document.querySelectorAll('[data-nav]').forEach((a) => a.classList.toggle('on', a.dataset.nav === ctx.page));
}

function renderAnnouncement() {
  let el = $('.announce');
  const text = state.instance?.announcement;
  if (!text) { el?.remove(); return; }
  if (!el) { el = document.createElement('div'); el.className = 'announce'; el.setAttribute('role', 'status'); $('.topbar').after(el); }
  el.innerHTML = html`<span class="announce-in">${icon('bell', 'sm')}<span>${text}</span></span>`.s;
}

export function renderTopbar() {
  const tb = $('.topbar');
  tb.innerHTML = topbar().s;
  renderAnnouncement();
  $('#jump').addEventListener('click', () => openPalette());
  $('#new-menu')?.addEventListener('click', (e) => {
    const r = state.repo;
    popover(e.currentTarget, html`
      <a class="menu-item" href="/new">${icon('repo')} New repository</a>
      <a class="menu-item" href="/snippets/new">${icon('file-code')} New snippet</a>
      <a class="menu-item" href="/orgs/new">${icon('users')} New organization</a>
      ${r ? html`<div class="menu-sep"></div><div class="menu-head">${r.owner}/${r.name}</div>
        <a class="menu-item" href="/${r.owner}/${r.name}/issues/new">${icon('issue')} New issue</a>
        <a class="menu-item" href="/${r.owner}/${r.name}/discussions/new">${icon('comment')} New discussion</a>
        <a class="menu-item" href="/${r.owner}/${r.name}/compare">${icon('pr')} New pull request</a>
        ${r.can_write ? html`<a class="menu-item" href="/${r.owner}/${r.name}/new/${r.default_branch}">${icon('file')} New file</a>` : ''}` : ''}`,
    { align: 'right', onMount: (p) => p.addEventListener('click', (ev) => ev.target.closest('a') && closePop()) });
  });
  $('#user-menu')?.addEventListener('click', (e) => {
    const me = state.me;
    popover(e.currentTarget, html`
      <div class="row" style="padding:8px 10px 10px">${avatar(me.username, 32)}<div class="grow"><b>${me.display_name || me.username}</b><div class="muted small">@${me.username}</div></div></div>
      <div class="menu-sep"></div>
      <a class="menu-item" href="/${me.username}">${icon('user')} Your profile</a>
      <a class="menu-item" href="/${me.username}?tab=repos">${icon('repo')} Your repositories</a>
      <a class="menu-item" href="/${me.username}?tab=stars">${icon('star')} Your stars</a>
      <a class="menu-item" href="/snippets/mine">${icon('file-code')} Your snippets</a>
      <div class="menu-sep"></div>
      <button class="menu-item" data-theme-toggle>${icon('moon')} Toggle theme <span class="grow"></span><kbd>⇧T</kbd></button>
      <a class="menu-item" href="/settings">${icon('settings')} Settings</a>
      ${me.is_admin ? html`<a class="menu-item" href="/admin">${icon('shield')} Site administration</a>` : ''}
      <button class="menu-item" data-shortcuts>${icon('keyboard')} Keyboard shortcuts <span class="grow"></span><kbd>?</kbd></button>
      <div class="menu-sep"></div>
      <button class="menu-item" data-logout>${icon('logout')} Sign out</button>`,
    {
      align: 'right', width: 250,
      onMount(p) {
        p.addEventListener('click', async (ev) => {
          if (ev.target.closest('a')) closePop();
          if (ev.target.closest('[data-theme-toggle]')) { cycleTheme(); closePop(); }
          if (ev.target.closest('[data-shortcuts]')) { closePop(); (await import('./palette.js')).showShortcuts(); }
          if (ev.target.closest('[data-logout]')) {
            closePop();
            try { await send('POST', '/auth/logout'); state.me = null; renderTopbar(); navigate('/'); } catch (err) { toastErr(err); }
          }
        });
      },
    });
  });
}

function footer() {
  return html`<div class="in">${logo(18)}<span>Gitter</span><span class="grow"></span>
    <a href="/explore">Explore</a><button class="linkish" data-shortcuts-link>Shortcuts</button><button class="linkish" data-theme-link>Theme</button></div>`;
}

// ---------- boot ----------
async function boot() {
  try {
    const me = await get('/me', { fresh: true });
    state.me = me.user;
    state.signup = me.signup;
    state.signupMode = me.signup_mode;
    state.instance = me.instance;
  } catch { state.me = null; }
  renderTopbar();
  const f = $('footer.site');
  f.innerHTML = footer().s;
  f.addEventListener('click', async (e) => {
    if (e.target.closest('[data-theme-link]')) { e.preventDefault(); cycleTheme(); }
    if (e.target.closest('[data-shortcuts-link]')) { e.preventDefault(); (await import('./palette.js')).showShortcuts(); }
  });
  initPalette({ navigate, cycleTheme, skins: SKINS, setSkin });
  await navigate(location.href, { replace: true, scroll: false });
  // Warm the heavier modules while the browser is idle, so first visits to them are instant too.
  const warm = () => ['./hl.js', './md.js', './diffview.js', './pages/repo.js', './pages/issues.js', './pages/issue.js', './pages/commits.js']
    .forEach((m) => import(m).catch(() => {}));
  (window.requestIdleCallback || ((f) => setTimeout(f, 300)))(warm);
}

boot();
export { raw };
