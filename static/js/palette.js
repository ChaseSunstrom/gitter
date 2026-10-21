// Command palette (Ctrl/Cmd+K), file finder (t) and keyboard shortcuts.
import { html, raw, esc, state, get, debounce, modal, avatar, toast, encPath } from './lib.js';
import { icon } from './icons.js';

let nav = (u) => { location.href = u; };
let themeToggle = () => {};
let skinList = [];
let applySkin = () => {};
let open = null;

/** Fuzzy subsequence match. Returns {score, idx} or null. Rewards consecutive runs, word starts and file-name hits. */
export function fuzzy(q, text) {
  if (!q) return { score: 0, idx: [] };
  const t = text.toLowerCase();
  const ql = q.toLowerCase();
  const nameStart = t.lastIndexOf('/') + 1;
  let ti = 0, score = 0, run = 0;
  const idx = [];
  for (const ch of ql) {
    if (ch === ' ') continue;
    const found = t.indexOf(ch, ti);
    if (found < 0) return null;
    const boundary = found === 0 || '/-_. '.includes(t[found - 1]);
    run = found === ti ? run + 1 : 0;
    score += 1 + run * 3 + (boundary ? 4 : 0) + (found >= nameStart ? 2 : 0) - Math.min(3, (found - ti) * 0.05);
    idx.push(found);
    ti = found + 1;
  }
  score -= text.length * 0.01;
  if (t.endsWith(ql)) score += 6;
  if (t.slice(nameStart).startsWith(ql)) score += 8;
  return { score, idx };
}

function mark(text, idx) {
  if (!idx?.length) return esc(text);
  const set = new Set(idx);
  let out = '';
  for (let i = 0; i < text.length; i++) out += set.has(i) ? `<mark>${esc(text[i])}</mark>` : esc(text[i]);
  return out;
}

const repoBase = () => state.repo && `/${state.repo.owner}/${state.repo.name}`;
let filesFor = null;
let filesPromise = null;
function repoFiles() {
  const r = state.repo;
  if (!r || r.empty) return Promise.resolve([]);
  const key = `${r.owner}/${r.name}@${state.rev || r.default_branch}`;
  if (filesFor !== key) {
    filesFor = key;
    filesPromise = get(`/repos/${r.owner}/${r.name}/files?rev=${encodeURIComponent(state.rev || r.default_branch)}`).then((d) => d.files).catch(() => []);
  }
  return filesPromise;
}

function commands() {
  const me = state.me;
  const r = state.repo;
  const base = repoBase();
  const list = [];
  const add = (group, ico, label, go, hint = '') => list.push({ group, ico, label, go, hint });
  if (r) {
    const g = `${r.owner}/${r.name}`;
    add(g, 'code', 'Code', base, 'g c');
    add(g, 'file', 'Go to file…', () => openPalette('', 'files'), 't');
    add(g, 'issue', 'Issues', base + '/issues', 'g i');
    add(g, 'pr', 'Pull requests', base + '/pulls', 'g p');
    add(g, 'history', 'Commits', base + '/commits');
    add(g, 'branch', 'Branches', base + '/branches');
    add(g, 'tag', 'Tags', base + '/tags');
    add(g, 'chart', 'Insights', base + '/insights');
    add(g, 'search', 'Search code in this repository', () => { const q = open?.input.value.trim(); nav(base + '/search' + (q ? '?q=' + encodeURIComponent(q) : '')); });
    if (me) add(g, 'plus', 'New issue', base + '/issues/new');
    if (me) add(g, 'pr', 'New pull request', base + '/compare');
    if (r.can_write) add(g, 'file', 'New file', `${base}/new/${encPath(state.rev || r.default_branch)}`);
    if (r.can_write) add(g, 'upload', 'Upload files', `${base}/upload/${encPath(state.rev || r.default_branch)}`);
    if (r.can_write) add(g, 'settings', 'Repository settings', base + '/settings');
    add(g, 'copy', 'Copy clone URL', () => navigator.clipboard?.writeText(r.clone_url).then(() => toast('Clone URL copied')));
    for (const b of (r.branches || []).slice(0, 50)) add('Switch branch', 'branch', b, `${base}/tree/${encPath(b)}`);
  }
  add('Go to', 'home', 'Home', '/', 'g h');
  add('Go to', 'compass', 'Explore', '/explore', 'g e');
  if (me) {
    add('Go to', 'user', 'Your profile', '/' + me.username);
    add('Go to', 'star', 'Your stars', `/${me.username}?tab=stars`);
    add('Go to', 'settings', 'Settings', '/settings');
    if (me.is_admin) add('Go to', 'shield', 'Site administration', '/admin');
    add('Go to', 'key', 'Access tokens', '/settings/tokens');
    add('Create', 'repo', 'New repository', '/new', 'g n');
  } else {
    add('Account', 'user', 'Sign in', '/login');
    if (state.signup) add('Account', 'plus', 'Create an account', '/signup');
  }
  add('Preferences', 'moon', 'Toggle dark mode', () => themeToggle(), '⇧ T');
  for (const sk of skinList) add('Preferences', 'sparkle', `Theme: ${sk.name}`, () => applySkin(sk.id));
  add('Help', 'keyboard', 'Keyboard shortcuts', () => showShortcuts(), '?');
  return list;
}

function render() {
  const o = open;
  const items = o.items;
  o.list.innerHTML = items.length
    ? (() => {
      let out = '';
      let g = null;
      items.forEach((it, i) => {
        if (it.group !== g) { g = it.group; out += `<div class="menu-head">${esc(g)}</div>`; }
        out += `<button class="menu-item pal-item${i === o.sel ? ' sel' : ''}" data-i="${i}" role="option" aria-selected="${i === o.sel}">`
          + (it.avatar ? avatar(it.avatar, 18).s : icon(it.ico || 'file').s)
          + `<span class="grow ellipsis">${it.html || esc(it.label)}</span>`
          + (it.sub ? `<span class="faint small ellipsis" style="max-width:40%">${esc(it.sub)}</span>` : '')
          + (it.hint ? `<kbd>${esc(it.hint)}</kbd>` : '') + '</button>';
      });
      return out;
    })()
    : `<div class="empty small" style="padding:28px">${o.loading ? '<span class="spin"></span>' : 'No matches'}</div>`;
  o.list.querySelector('.sel')?.scrollIntoView({ block: 'nearest' });
}

async function update() {
  const o = open;
  if (!o) return;
  const q = o.input.value.trim();
  const mode = o.mode;
  const my = ++o.seq;
  let items = [];
  if (mode === 'files') {
    const files = await repoFiles();
    if (my !== o.seq) return;
    const scored = [];
    for (const f of files) {
      const m = fuzzy(q, f);
      if (m) scored.push({ f, m });
      if (!q && scored.length >= 60) break;
    }
    scored.sort((a, b) => b.m.score - a.m.score);
    const base = repoBase();
    items = scored.slice(0, 60).map(({ f, m }) => ({ group: 'Files', ico: 'file', label: f, html: mark(f, m.idx), go: `${base}/blob/${encPath(state.rev || state.repo.default_branch)}/${encPath(f)}` }));
  } else {
    const cmdQ = q.startsWith('>') ? q.slice(1).trim() : q;
    const cmds = commands().map((c) => ({ c, m: fuzzy(cmdQ, c.label) })).filter((x) => x.m);
    if (cmdQ) cmds.sort((a, b) => b.m.score - a.m.score);
    items = cmds.slice(0, cmdQ ? 8 : 40).map(({ c, m }) => ({ ...c, html: mark(c.label, m.idx) }));
    if (q && !q.startsWith('>')) {
      const issueNum = /^#(\d+)$/.exec(q);
      if (issueNum && state.repo) items.unshift({ group: 'Jump', ico: 'issue', label: `Go to #${issueNum[1]}`, go: `${repoBase()}/issues/${issueNum[1]}` });
      if (state.repo) {
        const files = await repoFiles();
        if (my !== o.seq) return;
        const fm = files.map((f) => ({ f, m: fuzzy(q, f) })).filter((x) => x.m).sort((a, b) => b.m.score - a.m.score).slice(0, 6);
        items.push(...fm.map(({ f, m }) => ({ group: 'Files', ico: 'file', label: f, html: mark(f, m.idx), go: `${repoBase()}/blob/${encPath(state.rev || state.repo.default_branch)}/${encPath(f)}` })));
      }
      o.loading = true;
      remote(q, my);
      items.push(...(o.remote || []));
    }
  }
  if (my !== o.seq) return;
  o.items = items;
  o.sel = Math.min(o.sel, Math.max(0, items.length - 1));
  render();
}

const remote = debounce(async (q, my) => {
  if (!open) return;
  try {
    const d = await get('/search?q=' + encodeURIComponent(q));
    if (!open || my !== open.seq) return;
    const res = [
      ...d.repos.map((r) => ({ group: 'Repositories', ico: r.private ? 'lock' : 'repo', label: `${r.owner}/${r.name}`, sub: r.description, go: `/${r.owner}/${r.name}` })),
      ...d.users.map((u) => ({ group: 'People', avatar: u.username, label: u.username, sub: u.display_name, go: '/' + u.username })),
    ];
    open.remote = res;
    open.loading = false;
    open.items = open.items.filter((i) => i.group !== 'Repositories' && i.group !== 'People').concat(res);
    render();
  } catch { if (open) open.loading = false; }
}, 110);

function run(it) {
  if (!it) return;
  close();
  if (typeof it.go === 'function') it.go();
  else if (it.go) nav(it.go);
}

function close() {
  if (!open) return;
  open.el.remove();
  open.prev?.focus?.();
  open = null;
}

export function openPalette(initial = '', mode = 'all') {
  close();
  const el = document.createElement('div');
  el.className = 'scrim pal-scrim';
  el.innerHTML = html`<div class="palette" role="dialog" aria-modal="true" aria-label="Command palette">
    <div class="pal-input">${icon(mode === 'files' ? 'file' : 'search')}<input placeholder="${mode === 'files' ? 'Find a file…' : 'Search or jump to… (type > for commands, # for issues)'}" spellcheck="false" autocomplete="off" aria-label="Search">
    <kbd>esc</kbd></div><div class="pal-list" role="listbox"></div>
    <div class="pal-foot"><span><kbd>↑</kbd><kbd>↓</kbd> move</span><span><kbd>↵</kbd> open</span>${state.repo ? html`<span><kbd>t</kbd> files</span>` : ''}<span class="grow"></span>${raw('<span class="faint">Gitter</span>')}</div></div>`.s;
  document.body.append(el);
  open = { el, input: el.querySelector('input'), list: el.querySelector('.pal-list'), items: [], sel: 0, seq: 0, mode, prev: document.activeElement };
  open.input.value = initial;
  el.addEventListener('mousedown', (e) => { if (e.target === el) close(); });
  open.list.addEventListener('click', (e) => { const b = e.target.closest('[data-i]'); if (b) run(open.items[+b.dataset.i]); });
  open.list.addEventListener('mousemove', (e) => {
    const b = e.target.closest('[data-i]');
    if (b && +b.dataset.i !== open.sel) { open.sel = +b.dataset.i; open.list.querySelectorAll('.sel').forEach((x) => x.classList.remove('sel')); b.classList.add('sel'); }
  });
  open.input.addEventListener('input', () => { open.sel = 0; update(); });
  open.input.addEventListener('keydown', (e) => {
    const o = open;
    if (e.key === 'ArrowDown' || (e.key === 'n' && e.ctrlKey)) { e.preventDefault(); o.sel = (o.sel + 1) % Math.max(1, o.items.length); render(); }
    else if (e.key === 'ArrowUp' || (e.key === 'p' && e.ctrlKey)) { e.preventDefault(); o.sel = (o.sel - 1 + o.items.length) % Math.max(1, o.items.length); render(); }
    else if (e.key === 'Enter') { e.preventDefault(); run(o.items[o.sel]); }
    else if (e.key === 'Escape') { e.preventDefault(); close(); }
  });
  open.input.focus();
  update();
}

const SHORTCUTS = [
  ['Anywhere', [['Ctrl/⌘ K', 'Command palette'], ['/', 'Command palette'], ['?', 'This help'], ['⇧ T', 'Toggle dark mode'], ['g h', 'Home'], ['g e', 'Explore'], ['g n', 'New repository']]],
  ['In a repository', [['t', 'Find a file'], ['g c', 'Code'], ['g i', 'Issues'], ['g p', 'Pull requests'], ['g b', 'Branches'], ['g l', 'Commits'], ['y', 'Permalink to this commit'], ['b', 'Blame (on a file)']]],
  ['Files and diffs', [['l', 'Jump to line'], ['Shift-click', 'Select a range of lines'], ['u', 'Toggle unified / split diff']]],
];

export function showShortcuts() {
  modal({
    title: 'Keyboard shortcuts',
    wide: true,
    body: html`<div class="shortcut-grid">${SHORTCUTS.map(([g, list]) => html`<div><div class="section-title">${g}</div>
      ${list.map(([k, d]) => html`<div class="spread small" style="padding:5px 0;border-bottom:1px solid var(--line)"><span>${d}</span><span>${k.split(' ').map((x) => html`<kbd>${x}</kbd> `)}</span></div>`)}</div>`)}</div>`,
  });
}

function typing(e) {
  const t = e.target;
  return t.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName);
}

export function initPalette({ navigate, cycleTheme, skins = [], setSkin = () => {} }) {
  nav = navigate;
  themeToggle = cycleTheme;
  skinList = skins;
  applySkin = setSkin;
  let pendingG = 0;
  document.addEventListener('keydown', (e) => {
    if ((e.key === 'k' || e.key === 'K') && (e.metaKey || e.ctrlKey)) { e.preventDefault(); open ? close() : openPalette(); return; }
    if (typing(e) || e.metaKey || e.ctrlKey || e.altKey || open || document.querySelector('.scrim')) return;
    const base = repoBase();
    if (Date.now() - pendingG < 900) {
      pendingG = 0;
      const go = { h: '/', e: '/explore', n: '/new', c: base, i: base && base + '/issues', p: base && base + '/pulls', b: base && base + '/branches', l: base && base + '/commits' }[e.key];
      if (go) { e.preventDefault(); navigate(go); }
      return;
    }
    if (e.key === 'g') { pendingG = Date.now(); return; }
    if (e.key === '/') { e.preventDefault(); openPalette(); }
    else if (e.key === '?') { e.preventDefault(); showShortcuts(); }
    else if (e.key === 'T' && e.shiftKey) { e.preventDefault(); cycleTheme(); }
    else if (e.key === 't' && state.repo && !state.repo.empty) { e.preventDefault(); openPalette('', 'files'); }
    else if (e.key === 'y' && state.repo && state.sha) {
      const m = /^(\/[^/]+\/[^/]+\/(?:blob|tree|blame))\/(.+)$/.exec(location.pathname);
      if (m && state.revPath !== undefined) {
        e.preventDefault();
        navigate(`${m[1]}/${state.sha}${state.revPath ? '/' + encPath(state.revPath) : ''}${location.hash}`, { replace: true });
        toast('URL now points at this exact commit');
      }
    } else if (e.key === 'b' && state.repo && state.sha && /\/blob\//.test(location.pathname)) {
      e.preventDefault(); navigate(location.pathname.replace('/blob/', '/blame/'));
    }
  });
}
