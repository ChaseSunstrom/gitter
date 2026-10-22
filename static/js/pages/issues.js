// Issue and pull request lists.
import { html, get, enc, timeEl, num, $, popover, agentBadge, avatar, state } from '../lib.js';
import { icon } from '../icons.js';
import { loadRepo, repoUrl, repoPage, mountRepoHeader } from '../repohead.js';
import { navigate } from '../main.js';

/** State icon for an issue or PR row: green open, purple merged, red/grey closed. */
export function stateIcon(item, cls = '') {
  if (item.is_pr) {
    if (item.state === 'merged') return icon('merged', `c-purple ${cls}`);
    if (item.state === 'closed') return icon('pr-closed', `c-red ${cls}`);
    return icon('pr', `c-green ${cls}`);
  }
  return item.state === 'open' ? icon('issue', `c-green ${cls}`) : icon('issue-closed', `c-purple ${cls}`);
}

export const labelChip = (l, href = null) => href
  ? html`<a class="label-chip" style="--c:${l.color}" href="${href}">${l.name}</a>`
  : html`<span class="label-chip" style="--c:${l.color}">${l.name}</span>`;

function query(ctx) {
  const q = ctx.query;
  return {
    state: q.get('state') === 'closed' ? 'closed' : 'open',
    q: q.get('q') || '',
    label: q.get('label') || '',
    author: q.get('author') || '',
    milestone: q.get('milestone') || '',
    assignee: q.get('assignee') || '',
    page: Math.max(1, +q.get('page') || 1),
  };
}

const FILTERS = ['q', 'label', 'author', 'milestone', 'assignee'];

function listUrl(ctx) {
  const { owner, repo, kind } = ctx.params;
  const f = query(ctx);
  return `${repoUrl(owner, repo)}/issues?kind=${kind === 'pr' ? 'pr' : 'issue'}&state=${f.state}${FILTERS.map((k) => `&${k}=${enc(f[k])}`).join('')}&page=${f.page}`;
}

export async function prefetch(ctx) {
  get(repoUrl(ctx.params.owner, ctx.params.repo));
  get(listUrl(ctx));
}

export default async function (ctx) {
  const { owner, repo, kind } = ctx.params;
  const pr = kind === 'pr';
  const [meta, data, labels, queue] = await Promise.all([
    loadRepo(owner, repo),
    get(listUrl(ctx)),
    get(`${repoUrl(owner, repo)}/labels`).catch(() => ({ labels: [] })),
    pr ? get(`${repoUrl(owner, repo)}/merge-queue`).catch(() => ({ entries: [] })) : { entries: [] },
  ]);
  const f = query(ctx);
  const base = `/${owner}/${repo}`;
  const path = `${base}/${pr ? 'pulls' : 'issues'}`;
  const link = (over) => {
    const p = new URLSearchParams();
    const m = { ...f, page: 1, ...over };
    if (m.state === 'closed') p.set('state', 'closed');
    for (const k of FILTERS) if (m[k]) p.set(k, m[k]);
    if (m.page > 1) p.set('page', m.page);
    const s = p.toString();
    return path + (s ? '?' + s : '');
  };
  const filtered = FILTERS.some((k) => f[k]);
  const none = Object.fromEntries(FILTERS.map((k) => [k, '']));
  const noun = pr ? 'pull request' : 'issue';

  const rows = data.items.map((it, i) => {
    const href = `${base}/${it.is_pr ? 'pull' : 'issues'}/${it.number}`;
    const verb = it.state === 'merged' ? 'merged' : it.state === 'closed' ? 'closed' : 'opened';
    const when = it.state === 'open' ? it.created_at : it.closed_at || it.updated_at;
    return html`<div class="list-row issue-row" style="--i:${Math.min(i, 20)}">
      ${stateIcon(it)}
      <div class="grow" style="min-width:0">
        <div class="issue-line"><a class="issue-title" href="${href}">${it.title}</a>
          ${it.labels.map((l) => labelChip(l, link({ label: l.name })))}
          ${it.review_state === 'approved' ? html`<span class="pill open small" title="Approved by a reviewer">${icon('check', 'sm')} Approved</span>` : it.review_state === 'changes_requested' ? html`<span class="pill closed small" title="A reviewer asked for changes">Changes requested</span>` : ''}</div>
        <div class="muted small">#${it.number} ${verb} ${timeEl(when)} by <a href="${link({ author: it.author })}">${it.author}</a> ${agentBadge(it.agent, it.author)}
          ${it.is_pr && it.head ? html`<span class="hide-sm"> · ${icon('branch', 'sm inline')} ${it.head} → ${it.base}</span>` : ''}
          ${it.milestone ? html`<a class="hide-sm" href="${link({ milestone: it.milestone.title })}"> · ${icon('flag', 'sm inline')} ${it.milestone.title}</a>` : ''}</div>
      </div>
      ${it.assignees.length ? html`<span class="assignee-stack">${it.assignees.slice(0, 3).map((a) => html`<a href="${link({ assignee: a })}" title="Assigned to ${a}">${avatar(a, 20)}</a>`)}</span>` : ''}
      ${it.comments ? html`<a class="muted small row nowrap" href="${href}" title="${it.comments} comments">${icon('comment', 'sm')}${it.comments}</a>` : ''}
    </div>`;
  });

  const empty = html`<div class="empty">
    ${icon(pr ? 'pr' : 'issue', 'big')}
    <h3>${filtered ? `No ${noun}s match` : f.state === 'open' ? `No open ${noun}s` : `No closed ${noun}s`}</h3>
    <p>${filtered ? html`<a class="c-accent" href="${link(none)}">Clear filters</a>`
      : pr ? 'Pull requests let you propose and review changes before they land.' : 'Issues track bugs, ideas and tasks for this repository.'}</p>
    ${!filtered && f.state === 'open' ? html`<a class="btn primary" href="${pr ? `${base}/compare` : `${base}/issues/new`}">${icon('plus', 'sm')} New ${noun}</a>` : ''}
  </div>`;

  const body = html`<div class="stack">
    <div class="toolbar issues-toolbar">
      <form class="search-input grow" id="issue-search" role="search">${icon('search')}<input class="input" name="q" value="${f.q}" placeholder="Search ${noun}s" aria-label="Search ${noun}s"></form>
      ${pr ? '' : html`<a class="btn" href="${base}/labels">${icon('label', 'sm')} Labels</a><a class="btn" href="${base}/milestones">${icon('flag', 'sm')} Milestones</a>`}
      <a class="btn primary" href="${pr ? `${base}/compare` : `${base}/issues/new`}">${icon('plus', 'sm')} New ${noun}</a>
    </div>
    ${filtered ? html`<div class="row wrap-ok small filter-chips">
      ${f.q ? html`<a class="pill" href="${link({ q: '' })}">“${f.q}” ${icon('x', 'sm')}</a>` : ''}
      ${f.label ? html`<a class="pill" href="${link({ label: '' })}">label: ${f.label} ${icon('x', 'sm')}</a>` : ''}
      ${f.author ? html`<a class="pill" href="${link({ author: '' })}">author: ${f.author} ${icon('x', 'sm')}</a>` : ''}
      ${f.milestone ? html`<a class="pill" href="${link({ milestone: '' })}">milestone: ${f.milestone} ${icon('x', 'sm')}</a>` : ''}
      ${f.assignee ? html`<a class="pill" href="${link({ assignee: '' })}">assignee: ${f.assignee} ${icon('x', 'sm')}</a>` : ''}
    </div>` : ''}
    ${queue.entries.length ? html`<div class="box" id="merge-queue">
      <div class="box-head"><b>${icon('clock', 'sm')} Merge queue</b><span class="muted small">Merged one at a time, each tested on top of its target branch first.</span></div>
      <div class="list">${queue.entries.map((q) => html`<div class="list-row small">
        <span class="counter">${q.position}</span>
        <a class="grow ellipsis" href="${base}/pull/${q.number}">#${q.number} ${q.title}</a>
        <span class="muted hide-sm">into ${q.base} · added by ${q.added_by}</span>
        ${q.state === 'testing' ? html`<span class="pill small">${icon('play', 'sm')} Testing</span>` : html`<span class="pill small muted">Waiting</span>`}
      </div>`)}</div>
    </div>` : ''}
    <div class="box">
      <div class="box-head">
        <a class="state-tab ${f.state === 'open' ? 'on' : ''}" href="${link({ state: 'open' })}">${icon(pr ? 'pr' : 'issue', 'sm')} ${num(data.open)} Open</a>
        <a class="state-tab ${f.state === 'closed' ? 'on' : ''}" href="${link({ state: 'closed' })}">${icon('check', 'sm')} ${num(data.closed)} ${pr ? 'Closed or merged' : 'Closed'}</a>
        <span class="grow"></span>
        ${labels.labels.length ? html`<button class="btn ghost sm" id="label-filter">${icon('label', 'sm')} Label ${icon('chev-down', 'sm')}</button>` : ''}
        <button class="btn ghost sm" id="milestone-filter">${icon('flag', 'sm')} Milestone ${icon('chev-down', 'sm')}</button>
        <button class="btn ghost sm" id="assignee-filter">${icon('user', 'sm')} Assignee ${icon('chev-down', 'sm')}</button>
      </div>
      ${data.items.length ? html`<div class="list stagger">${rows}</div>` : empty}
    </div>
    ${data.page > 1 || data.has_more ? html`<div class="row" style="justify-content:center">
      ${data.page > 1 ? html`<a class="btn" href="${link({ page: data.page - 1 })}">${icon('chev-left', 'sm')} Newer</a>` : ''}
      ${data.has_more ? html`<a class="btn" href="${link({ page: data.page + 1 })}">Older ${icon('chev-right', 'sm')}</a>` : ''}
    </div>` : ''}
  </div>`;

  return {
    title: `${pr ? 'Pull requests' : 'Issues'} · ${owner}/${repo}`,
    html: repoPage(meta, pr ? 'pulls' : 'issues', body),
    mount(el) {
      mountRepoHeader(el, meta);
      $('#issue-search', el).addEventListener('submit', (e) => {
        e.preventDefault();
        navigate(link({ q: e.target.q.value.trim() }));
      });
      $('#label-filter', el)?.addEventListener('click', (e) => {
        popover(e.currentTarget, html`<div class="menu-head">Filter by label</div>
          ${labels.labels.map((l) => html`<a class="menu-item" href="${link({ label: l.name })}"><span class="dot" style="background:${l.color}"></span><span class="grow">${l.name}</span><span class="faint small">${l.open}</span></a>`)}
          ${f.label ? html`<div class="menu-sep"></div><a class="menu-item" href="${link({ label: '' })}">${icon('x', 'sm')} Clear label filter</a>` : ''}`,
        { align: 'right', width: 240 });
      });
      $('#milestone-filter', el).addEventListener('click', async (e) => {
        const anchor = e.currentTarget;
        const ms = (await get(`${repoUrl(owner, repo)}/milestones`).catch(() => ({ milestones: [] }))).milestones;
        popover(anchor, html`<div class="menu-head">Filter by milestone</div>
          <a class="menu-item" href="${link({ milestone: 'none' })}">${icon('x', 'sm')} No milestone</a>
          ${ms.map((m) => html`<a class="menu-item" href="${link({ milestone: m.title })}">${icon('flag', 'sm')}<span class="grow">${m.title}</span><span class="faint small">${m.open}</span></a>`)}
          <div class="menu-sep"></div><a class="menu-item" href="${base}/milestones">${icon('settings', 'sm')} All milestones</a>`,
        { align: 'right', width: 240 });
      });
      $('#assignee-filter', el).addEventListener('click', (e) => {
        popover(e.currentTarget, html`<div class="menu-head">Filter by assignee</div>
          ${state.me ? html`<a class="menu-item" href="${link({ assignee: state.me.username })}">${avatar(state.me.username, 18)} Assigned to me</a>` : ''}
          <a class="menu-item" href="${link({ assignee: 'none' })}">${icon('x', 'sm')} Assigned to nobody</a>
          ${f.assignee ? html`<div class="menu-sep"></div><a class="menu-item" href="${link({ assignee: '' })}">${icon('x', 'sm')} Clear assignee filter</a>` : ''}`,
        { align: 'right', width: 220 });
      });
    },
  };
}
