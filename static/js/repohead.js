// Shared repository chrome: header, tabs, branch picker, clone menu, breadcrumbs.
import { html, state, get, send, popover, closePop, toastErr, num, encPath, esc, modal, action, $, toast, reportDialog } from './lib.js';
import { icon } from './icons.js';
import { navigate, refresh } from './main.js';

export function repoUrl(o, n) { return `/repos/${encodeURIComponent(o)}/${encodeURIComponent(n)}`; }

export async function loadRepo(owner, name) {
  const meta = await get(repoUrl(owner, name));
  state.repo = meta;
  return meta;
}

/** Split "feature/x/src/a.rs" into rev and path using the longest matching branch or tag. */
export function splitRev(meta, rest) {
  rest = (rest || '').replace(/^\/+/, '');
  if (!rest) return { rev: meta.default_branch, path: '' };
  let best = '';
  for (const r of [...(meta.branches || []), ...(meta.tags || [])]) {
    if ((rest === r || rest.startsWith(r + '/')) && r.length > best.length) best = r;
  }
  const rev = best || rest.split('/')[0];
  return { rev, path: rest.slice(rev.length).replace(/^\/+/, '') };
}

const TABS = [
  ['code', 'code', 'Code', ''],
  ['issues', 'issue', 'Issues', '/issues'],
  ['pulls', 'pr', 'Pull requests', '/pulls'],
  ['ci', 'zap', 'CI', '/ci'],
  ['discussions', 'comment', 'Discussions', '/discussions'],
  ['wiki', 'book', 'Wiki', '/wiki'],
  ['commits', 'history', 'Commits', '/commits'],
  ['branches', 'branch', 'Branches', '/branches'],
  ['insights', 'chart', 'Insights', '/insights'],
  ['settings', 'settings', 'Settings', '/settings'],
];

export function repoHeader(meta, tab) {
  const base = `/${meta.owner}/${meta.name}`;
  const counts = { issues: meta.open_issues, pulls: meta.open_pulls };
  return html`<div class="repo-head" data-repo="${meta.owner}/${meta.name}"><div class="repo-head-in">
    <div class="repo-title-row">
      <div class="repo-title">
        ${icon(meta.private ? 'lock' : 'repo', 'lg')}
        <a href="/${meta.owner}" class="owner">${meta.owner}</a><span class="slash">/</span><a href="${base}" class="name">${meta.name}</a>
        ${meta.private ? html`<span class="tag-private">Private</span>` : html`<span class="tag-private">Public</span>`}
      </div>
      <div class="repo-actions">
        <div class="btn-group">
          <button class="btn sm star-btn ${meta.starred ? 'on' : ''}" id="star-btn" aria-pressed="${meta.starred}">${icon('star', 'sm')}<span>${meta.starred ? 'Starred' : 'Star'}</span></button>
          <a class="btn sm" href="${base}/insights" title="Stargazers"><span class="count" id="star-count">${num(meta.stars)}</span></a>
        </div>
        <div class="btn-group">
          <button class="btn sm" id="fork-btn">${icon('fork', 'sm')}<span>Fork</span></button>
          <span class="btn sm" style="pointer-events:none"><span class="count">${num(meta.forks)}</span></span>
        </div>
        <button class="btn sm primary" id="clone-btn">${icon('code', 'sm')}<span>Clone</span>${icon('chev-down', 'sm')}</button>
        ${state.me && !meta.is_owner ? html`<button class="btn sm icon ghost" id="report-btn" title="Report this repository" aria-label="Report this repository">${icon('flag', 'sm')}</button>` : ''}
      </div>
    </div>
    ${meta.hidden ? html`<div class="mod-banner">${icon('eye', 'sm')} A moderator hid this repository. Only you and the site's admins can see it.</div>` : ''}
    ${meta.fork_of ? html`<div class="forked muted small">${icon('fork', 'sm')} forked from <a href="/${meta.fork_of.owner}/${meta.fork_of.name}">${meta.fork_of.owner}/${meta.fork_of.name}</a></div>` : ''}
    <nav class="tabs repo-tabs" aria-label="Repository">
      ${TABS.filter(([k]) => k !== 'settings' || meta.can_write).map(([k, ic, label, path]) => html`<a href="${base}${path}" class="${tab === k ? 'on' : ''}" data-tab="${k}">
        ${icon(ic, 'sm')}<span>${label}</span>${counts[k] ? html`<span class="counter">${num(counts[k])}</span>` : ''}</a>`)}
      <span class="bar"></span>
    </nav>
  </div></div>`;
}

/** Position a .tabs .bar under the active tab (animated by CSS). */
export function placeTabBar(root) {
  root.querySelectorAll('.tabs').forEach((tabs) => {
    const on = tabs.querySelector('.on');
    const bar = tabs.querySelector('.bar');
    if (!bar) return;
    if (!on) { bar.style.width = '0'; return; }
    const prev = window.__lastTab;
    const target = { left: on.offsetLeft + 8, width: on.offsetWidth - 16 };
    if (prev && prev.key === tabs.className) {
      bar.style.transition = 'none';
      bar.style.left = prev.left + 'px'; bar.style.width = prev.width + 'px';
      bar.offsetWidth;
      bar.style.transition = '';
    }
    bar.style.left = target.left + 'px'; bar.style.width = target.width + 'px';
    window.__lastTab = { key: tabs.className, ...target };
  });
}

// ---------- CI status ----------
export const CI_LABEL = {
  success: 'Passed', failure: 'Failed', error: 'Lost', cancelled: 'Cancelled', skipped: 'Skipped',
  running: 'Running', queued: 'Queued', waiting: 'Waiting', pending: 'Not started',
};
const CI_ICON = {
  success: ['check', 'ok'], failure: ['x', 'bad'], error: ['x', 'bad'], cancelled: ['minus', 'off'], skipped: ['minus', 'off'],
  running: ['pulse', 'run'], queued: ['clock', 'wait'], waiting: ['clock', 'wait'], pending: ['clock', 'wait'],
};

export function ciIcon(status, cls = '') {
  const [ic, k] = CI_ICON[status] || CI_ICON.pending;
  const label = CI_LABEL[status] || status;
  return html`<span class="ci-st ${k} ${cls}" title="${label}" aria-label="${label}">${icon(ic, 'sm')}</span>`;
}

/** Fill every `[data-ci-sha]` placeholder under `root` with that commit's latest CI result. */
export function paintCiStatus(root, meta) {
  const els = [...root.querySelectorAll('[data-ci-sha]')];
  const shas = [...new Set(els.map((e) => e.dataset.ciSha))].slice(0, 100);
  if (!shas.length) return;
  get(`${repoUrl(meta.owner, meta.name)}/ci/status?shas=${shas.join(',')}`, { fresh: true }).then((m) => {
    for (const e of els) {
      const s = m[e.dataset.ciSha];
      if (!s) continue;
      const label = e.dataset.ciLabel ? html` <span>${CI_LABEL[s.status] || s.status}</span>` : '';
      e.innerHTML = html`<a class="ci-badge" href="/${meta.owner}/${meta.name}/ci/${s.number}" title="CI run #${s.number}: ${CI_LABEL[s.status] || s.status}">${ciIcon(s.status)}${label}</a>`.s;
    }
  }).catch(() => {});
}

export function mountRepoHeader(root, meta) {
  placeTabBar(root);
  paintCiStatus(root, meta);
  $('#report-btn', root)?.addEventListener('click', () => reportDialog({ kind: 'repo', owner: meta.owner, repo: meta.name }, 'repository'));
  const base = repoUrl(meta.owner, meta.name);
  const starBtn = $('#star-btn', root);
  starBtn?.addEventListener('click', async () => {
    if (!state.me) return navigate('/login?next=' + encodeURIComponent(location.pathname));
    const on = !meta.starred;
    meta.starred = on;
    meta.stars += on ? 1 : -1;
    starBtn.classList.toggle('on', on);
    starBtn.classList.remove('pop'); starBtn.offsetWidth; starBtn.classList.add('pop');
    starBtn.querySelector('span').textContent = on ? 'Starred' : 'Star';
    $('#star-count', root).textContent = num(meta.stars);
    try { await send(on ? 'POST' : 'DELETE', base + '/star'); } catch (e) { toastErr(e); refresh(); }
  });
  $('#fork-btn', root)?.addEventListener('click', () => {
    if (!state.me) return navigate('/login?next=' + encodeURIComponent(location.pathname));
    modal({
      title: `Fork ${meta.owner}/${meta.name}`,
      body: html`<p class="muted">A fork is your own copy of this repository, with all its branches and history. You can change it freely and open pull requests back.</p>
        <div class="field"><label for="fork-name">Repository name</label><div class="row"><span class="muted">${state.me.username} /</span><input id="fork-name" class="input" value="${meta.name}" autofocus></div></div>`,
      foot: html`<button class="btn" data-close>Cancel</button><button class="btn primary" id="do-fork">${icon('fork', 'sm')} Create fork</button>`,
      onMount(el, close) {
        const b = el.querySelector('#do-fork');
        b.addEventListener('click', action(b, async () => {
          const r = await send('POST', base + '/fork', { name: el.querySelector('#fork-name').value.trim() });
          close(true);
          toast('Fork created');
          navigate(`/${r.owner}/${r.name}`);
        }));
      },
    });
  });
  $('#clone-btn', root)?.addEventListener('click', (e) => {
    const rev = state.rev || meta.default_branch;
    popover(e.currentTarget, html`<div class="clone-pop">
      <div class="menu-head">Clone with HTTPS</div>
      <div class="row" style="padding:2px 6px 8px"><input class="input sm mono" readonly value="${meta.clone_url}" aria-label="Clone URL" id="clone-url">
        <button class="btn sm icon" data-copy="${meta.clone_url}" data-copy-label="Clone URL copied" aria-label="Copy">${icon('copy', 'sm')}</button></div>
      <div class="row" style="padding:0 6px 8px"><input class="input sm mono" readonly value="git clone ${meta.clone_url}" aria-label="Clone command">
        <button class="btn sm icon" data-copy="git clone ${meta.clone_url}" data-copy-label="Command copied" aria-label="Copy">${icon('terminal', 'sm')}</button></div>
      <p class="tiny muted" style="padding:0 8px 8px">Push with your password or an <a href="/settings/tokens" class="c-accent">access token</a>.</p>
      ${meta.empty ? '' : html`<div class="menu-sep"></div>
      <a class="menu-item" href="/archive/${meta.owner}/${meta.name}/${encPath(rev)}.zip" download>${icon('download', 'sm')} Download ZIP</a>
      <a class="menu-item" href="/archive/${meta.owner}/${meta.name}/${encPath(rev)}.tar.gz" download>${icon('download', 'sm')} Download tar.gz</a>`}</div>`,
    { align: 'right', width: 380, onMount: (p) => p.querySelector('#clone-url').select() });
  });
}

/** Full repo page: header + body. */
export function repoPage(meta, tab, body) {
  return html`${repoHeader(meta, tab)}<div class="wrap" id="repo-body">${body}</div>`;
}

/** Branch / tag picker button. `hrefFor(rev)` builds each option's link. */
export function refPicker(meta, rev, id = 'ref-picker') {
  const isTag = (meta.tags || []).includes(rev);
  const shortRev = /^[0-9a-f]{40}$/.test(rev) ? rev.slice(0, 7) : rev;
  return html`<button class="btn sm ref-btn" id="${id}" title="Switch branches or tags">${icon(isTag ? 'tag' : 'branch', 'sm')}<span class="ellipsis" style="max-width:180px">${shortRev}</span>${icon('chev-down', 'sm')}</button>`;
}

export function mountRefPicker(root, meta, rev, hrefFor, id = 'ref-picker') {
  const btn = $('#' + id, root);
  btn?.addEventListener('click', () => {
    let kind = (meta.tags || []).includes(rev) ? 'tags' : 'branches';
    const pop = popover(btn, html`<div class="ref-pop">
      <div class="menu-head">Switch branches/tags</div>
      <div style="padding:2px 6px 6px"><input class="input sm" placeholder="Filter branches and tags" id="ref-filter" autocomplete="off"></div>
      <div class="seg" style="margin:0 6px 6px"><button data-k="branches">Branches</button><button data-k="tags">Tags</button></div>
      <div class="ref-list" style="max-height:300px;overflow:auto"></div></div>`, { width: 300 });
    const list = pop.querySelector('.ref-list');
    const filter = pop.querySelector('#ref-filter');
    const paint = () => {
      const q = filter.value.trim().toLowerCase();
      pop.querySelectorAll('.seg button').forEach((b) => b.classList.toggle('on', b.dataset.k === kind));
      const items = (meta[kind] || []).filter((r) => r.toLowerCase().includes(q)).slice(0, 200);
      list.innerHTML = items.length
        ? items.map((r) => `<a class="menu-item" href="${esc(hrefFor(r))}">${r === rev ? icon('check', 'sm').s : '<span style="width:14px"></span>'}<span class="ellipsis grow">${esc(r)}</span>${r === meta.default_branch ? '<span class="tag-private">default</span>' : ''}</a>`).join('')
        : `<div class="small muted" style="padding:10px">Nothing to show</div>`;
    };
    pop.querySelector('.seg').addEventListener('click', (e) => { const b = e.target.closest('button'); if (b) { kind = b.dataset.k; paint(); } });
    filter.addEventListener('input', paint);
    filter.addEventListener('keydown', (e) => { if (e.key === 'Enter') { const a = list.querySelector('a'); if (a) { closePop(); navigate(a.getAttribute('href')); } } });
    list.addEventListener('click', (e) => { if (e.target.closest('a')) closePop(); });
    paint();
    filter.focus();
  });
}

/** owner/repo/rev breadcrumb: repo-name / dir / dir / file */
export function crumbs(meta, rev, path, { last = true } = {}) {
  const base = `/${meta.owner}/${meta.name}`;
  const parts = path ? path.split('/') : [];
  return html`<nav class="crumbs" aria-label="Path"><a href="${base}/tree/${encPath(rev)}" class="crumb-root">${meta.name}</a>${parts.map((p, i) => {
    const sub = parts.slice(0, i + 1).join('/');
    return html`<span class="slash">/</span>${i === parts.length - 1 && last ? html`<b>${p}</b>` : html`<a href="${base}/tree/${encPath(rev)}/${encPath(sub)}">${p}</a>`}`;
  })}${path ? html`<button class="btn ghost sm icon" data-copy="${path}" data-copy-label="Path copied" title="Copy path" aria-label="Copy path">${icon('copy', 'sm')}</button>` : ''}</nav>`;
}
