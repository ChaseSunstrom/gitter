// Home: a landing page for visitors, a dashboard for signed-in people.
// Also exports the activity row used by profile pages.
import { html, state, get, timeEl, avatar, short, plural, $, $$, agentBadge } from '../lib.js';
import { icon, logo } from '../icons.js';
import { repoCard, langColor } from './explore.js';

// ---------- activity ----------

function eventText(e) {
  const repo = e.repo ? html`<a class="ev-repo" href="/${e.repo}">${e.repo}</a>` : html`<span class="muted">a deleted repository</span>`;
  const user = html`<a class="ev-user" href="/${e.user}">${e.user}</a>${e.agent ? html` ${agentBadge(e.agent, e.user)}` : ''}`;
  const n = e.target;
  switch (e.kind) {
    case 'create': return [icon('repo'), html`${user} created ${repo}`];
    case 'push': return [icon('commit'), html`${user} pushed ${plural(e.count, 'commit')} to <a class="ev-branch" href="/${e.repo}/tree/${e.title}">${e.title}</a> in ${repo}`];
    case 'issue': return [icon('issue', 'c-green'), html`${user} opened <a href="/${e.repo}/issues/${n}">#${n} ${e.title}</a> in ${repo}`];
    case 'pull': return [icon('pr', 'c-green'), html`${user} opened pull request <a href="/${e.repo}/pull/${n}">#${n} ${e.title}</a> in ${repo}`];
    case 'merge': return [icon('merged', 'c-purple'), html`${user} merged <a href="/${e.repo}/pull/${n}">#${n} ${e.title}</a> in ${repo}`];
    case 'comment': return [icon('comment'), html`${user} commented on <a href="/${e.repo}/issues/${n}">#${n} ${e.title}</a> in ${repo}`];
    case 'star': return [icon('star', 'c-amber'), html`${user} starred ${repo}`];
    case 'fork': return [icon('fork'), html`${user} forked <a href="/${e.title}">${e.title}</a> to ${repo}`];
    default: return [icon('pulse'), html`${user} was active in ${repo}`];
  }
}

/** Merge consecutive pushes by the same person to the same branch. */
function groupPushes(events) {
  const out = [];
  for (const e of events) {
    const prev = out[out.length - 1];
    if (prev && e.kind === 'push' && prev.kind === 'push' && prev.user === e.user && prev.agent === e.agent && prev.repo === e.repo && prev.title === e.title) {
      prev.count += e.count;
      continue;
    }
    out.push({ ...e });
  }
  return out;
}

export function eventList(events, { compact = false } = {}) {
  const list = groupPushes(events);
  if (!list.length) return html`<div class="empty small">${icon('pulse', 'big')}<h3>No activity yet</h3></div>`;
  return html`<ol class="feed stagger">${list.map((e, i) => {
    const [ic, text] = eventText(e);
    return html`<li class="feed-item" style="--i:${Math.min(i, 18)}">
      <span class="feed-icon">${ic}</span>
      ${compact ? '' : html`<a href="/${e.user}" class="feed-avatar" aria-hidden="true" tabindex="-1">${avatar(e.user, 28)}</a>`}
      <div class="feed-body"><div class="feed-text">${text}</div>
        ${e.kind === 'push' && e.target ? html`<a class="sha" href="/${e.repo}/commit/${e.target}">${short(e.target)}</a>` : ''}
      </div>
      <span class="feed-time muted small nowrap">${timeEl(e.created_at)}</span>
    </li>`;
  })}</ol>`;
}

// ---------- landing (signed out) ----------

const FEATURES = [
  ['zap', 'Instant pages', 'Prefetched on hover and rendered without a framework, so clicks land before you notice.'],
  ['lock', 'Private repositories', 'Keep code to yourself. Private repos stay invisible to everyone else, even in search.'],
  ['pr', 'Pull requests', 'Compare branches, read clean diffs, and merge on the server without surprise conflicts.'],
  ['command', 'Command palette', 'Press Ctrl K to jump to any repo, file, branch or action from anywhere.'],
  ['terminal', 'Self-host in one file', 'One small binary and one volume. Runs comfortably on a tiny server.'],
  ['sparkle', 'CI is next', 'Pipelines that live in your repo, with runners in the same compose file.'],
];

const heroMark = () => logo(96, { animated: true, cls: 'hero-mark' });

function landing(feed) {
  const events = feed.events || [];
  return html`<div class="landing">
    <section class="hero"><div class="hero-in">
      ${heroMark()}
      <h1>Git hosting that feels instant.</h1>
      <p class="hero-sub">Gitter is a calm, fast home for your code: repositories, pull requests and reviews, with nothing in the way.</p>
      <div class="row hero-cta">
        ${state.signup ? html`<a class="btn primary lg" href="/signup">Create your account</a>` : html`<a class="btn primary lg" href="/login">Sign in</a>`}
        <a class="btn lg" href="/explore">${icon('compass', 'sm')} Explore repositories</a>
      </div>
      <p class="hero-hint muted small">Press <kbd>Ctrl</kbd> <kbd>K</kbd> anywhere to jump around.</p>
    </div></section>
    <div class="wrap">
      <section class="feature-grid stagger">${FEATURES.map(([ic, t, d], i) => html`<div class="feature" style="--i:${i}">
        <span class="feature-icon">${icon(ic)}</span><h3>${t}</h3><p class="muted small">${d}</p></div>`)}</section>
      <section class="landing-cols">
        <div><div class="section-title">${icon('star', 'sm')} Trending repositories</div>
          ${feed.trending?.length ? html`<div class="stack">${feed.trending.map((r) => repoCard(r))}</div>` : html`<div class="box empty small"><h3>Nothing here yet</h3><p>Public repositories show up here.</p></div>`}</div>
        <div><div class="section-title">${icon('pulse', 'sm')} Recent activity</div>
          <div class="box">${eventList(events)}</div></div>
      </section>
    </div>
  </div>`;
}

// ---------- dashboard (signed in) ----------

function repoMini(r) {
  return html`<li data-name="${r.owner}/${r.name}"><a class="mini-repo" href="/${r.owner}/${r.name}">
    ${avatar(r.owner, 18, 'sq')}<span class="ellipsis grow"><span class="muted">${r.owner}/</span><b>${r.name}</b></span>
    ${r.private ? icon('lock', 'sm muted') : ''}${r.language ? html`<span class="dot" title="${r.language}" style="background:${langColor(r.language)}"></span>` : ''}</a></li>`;
}

function trendingMini(r) {
  return html`<li><a href="/${r.owner}/${r.name}" class="trend">
    <span class="ellipsis"><span class="muted">${r.owner}/</span><b>${r.name}</b></span>
    ${r.description ? html`<span class="muted small ellipsis">${r.description}</span>` : ''}
    <span class="row tiny muted">${r.language ? html`<span class="dot" style="background:${langColor(r.language)}"></span>${r.language}` : ''}${icon('star', 'sm')}${r.stars}</span></a></li>`;
}

function dashboard(feed) {
  const me = state.me;
  const repos = feed.repos || [];
  const sparse = (feed.events || []).length < 10 && (feed.global || []).length;
  return html`<div class="wrap dash">
    <aside class="dash-left">
      <div class="spread"><div class="section-title" style="margin:0">Your repositories</div><a class="btn sm primary" href="/new">${icon('plus', 'sm')} New</a></div>
      ${repos.length > 5 ? html`<input class="input sm" id="repo-filter" placeholder="Find a repository…" aria-label="Filter your repositories" autocomplete="off">` : ''}
      ${repos.length ? html`<ul class="mini-repos">${repos.map(repoMini)}</ul><p class="small muted" id="repo-none" hidden>No repositories match.</p>`
        : html`<div class="box empty small"><h3>No repositories yet</h3><p>Create one to get started.</p><a class="btn primary sm" href="/new">Create repository</a></div>`}
    </aside>
    <section class="dash-main">
      <div class="dash-greet">${avatar(me.username, 40)}<div><h1>Welcome back, ${me.display_name || me.username}</h1><p class="muted small">Here's what's been happening.</p></div></div>
      <div class="section-title">${icon('pulse', 'sm')} Your activity and the repositories you follow</div>
      <div class="box">${eventList(feed.events || [])}</div>
      ${sparse ? html`<div class="section-title" style="margin-top:24px">${icon('globe', 'sm')} Happening across Gitter</div><div class="box">${eventList(feed.global)}</div>` : ''}
    </section>
    <aside class="dash-right">
      <div class="card tip-card"><div class="row">${icon('command', 'c-accent')}<b>Tip</b></div>
        <p class="small muted">Press <kbd>Ctrl</kbd> <kbd>K</kbd> to search repositories, files and commands. In a repository, press <kbd>t</kbd> to find a file.</p></div>
      <div><div class="section-title">${icon('star', 'sm')} Trending</div>
        ${feed.trending?.length ? html`<ul class="trend-list">${feed.trending.map(trendingMini)}</ul>` : html`<p class="small muted">Nothing trending yet.</p>`}
        <a class="small c-accent" href="/explore">Explore more →</a></div>
    </aside>
  </div>`;
}

export function prefetch() { get('/feed'); }

export default async function () {
  const feed = await get('/feed');
  if (!state.me) return { title: 'Git hosting that feels instant', html: landing(feed) };
  return {
    title: 'Dashboard',
    html: dashboard(feed),
    mount(el) {
      const f = $('#repo-filter', el);
      f?.addEventListener('input', () => {
        const q = f.value.trim().toLowerCase();
        let shown = 0;
        $$('.mini-repos li', el).forEach((li) => { const hit = li.dataset.name.toLowerCase().includes(q); li.hidden = !hit; shown += hit; });
        $('#repo-none', el).hidden = shown > 0;
      });
    },
  };
}
