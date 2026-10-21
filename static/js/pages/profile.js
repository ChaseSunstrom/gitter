// User profile: overview with contribution heatmap, repositories, stars.
import { html, raw, state, get, avatar, safeUrl, day, num, enc, $, $$ } from '../lib.js';
import { icon } from '../icons.js';
import { repoCard } from './explore.js';
import { eventList } from './home.js';

const userUrl = (u) => `/users/${enc(u)}`;

export function prefetch(ctx) { get(userUrl(ctx.params.user)); }

const iso = (d) => `${d.getUTCFullYear()}-${String(d.getUTCMonth() + 1).padStart(2, '0')}-${String(d.getUTCDate()).padStart(2, '0')}`;
const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

/** 53-week × 7-day grid ending today (UTC, weeks start Sunday). */
function heatmap(rows) {
  const counts = new Map(rows.map(([d, c]) => [d, c]));
  const today = new Date();
  const end = new Date(Date.UTC(today.getUTCFullYear(), today.getUTCMonth(), today.getUTCDate()));
  const start = new Date(end);
  start.setUTCDate(start.getUTCDate() - 52 * 7 - end.getUTCDay());
  const max = Math.max(1, ...counts.values());
  const level = (c) => (c <= 0 ? 0 : c >= max * 0.75 ? 4 : c >= max * 0.5 ? 3 : c >= max * 0.25 ? 2 : 1);
  const cell = 11, gap = 3, left = 28, top = 18;
  let cells = '', months = '', total = 0, lastMonth = -1;
  const d = new Date(start);
  for (let w = 0; w < 53; w++) {
    for (let dow = 0; dow < 7; dow++) {
      if (d > end) break;
      const key = iso(d);
      const c = counts.get(key) || 0;
      total += c;
      const label = `${c ? num(c) : 'No'} contribution${c === 1 ? '' : 's'} on ${d.toLocaleDateString(undefined, { month: 'long', day: 'numeric', year: 'numeric', timeZone: 'UTC' })}`;
      cells += `<rect class="hm l${level(c)}" x="${left + w * (cell + gap)}" y="${top + dow * (cell + gap)}" width="${cell}" height="${cell}" rx="2.5"><title>${label}</title></rect>`;
      if (dow === 0 && d.getUTCMonth() !== lastMonth && d.getUTCDate() <= 7) {
        lastMonth = d.getUTCMonth();
        if (w < 52) months += `<text x="${left + w * (cell + gap)}" y="11">${MONTHS[lastMonth]}</text>`;
      }
      d.setUTCDate(d.getUTCDate() + 1);
    }
  }
  const days = [[1, 'Mon'], [3, 'Wed'], [5, 'Fri']].map(([i, t]) => `<text x="0" y="${top + i * (cell + gap) + 9}">${t}</text>`).join('');
  const width = left + 53 * (cell + gap);
  const height = top + 7 * (cell + gap);
  return html`<div class="heatmap-card box">
    <div class="box-head"><span><b>${num(total)}</b> contribution${total === 1 ? '' : 's'} in the last year</span></div>
    <div class="heatmap-scroll"><svg class="heatmap" viewBox="0 0 ${width} ${height}" role="img" aria-label="Contribution graph">${raw(months + days + cells)}</svg></div>
    <div class="hm-legend small muted"><span>Less</span>${[0, 1, 2, 3, 4].map((l) => raw(`<svg width="11" height="11" aria-hidden="true"><rect class="hm l${l}" width="11" height="11" rx="2.5"/></svg>`))}<span>More</span></div>
  </div>`;
}

function sidebar(u, isMe) {
  return html`<aside class="profile-side">
    <div class="profile-avatar">${avatar(u.username, 260, 'sq')}</div>
    <h1 class="profile-name">${u.display_name || u.username}</h1>
    <p class="profile-handle muted">@${u.username}${u.is_admin ? html` <span class="tag-private">Admin</span>` : ''}</p>
    ${u.bio ? html`<p class="profile-bio">${u.bio}</p>` : ''}
    ${isMe ? html`<a class="btn" href="/settings" style="width:100%">${icon('edit', 'sm')} Edit profile</a>` : ''}
    <ul class="profile-facts small">
      ${u.location ? html`<li>${icon('globe', 'sm')}<span>${u.location}</span></li>` : ''}
      ${u.website ? html`<li>${icon('link', 'sm')}<a class="c-accent ellipsis" href="${safeUrl(u.website)}" target="_blank" rel="noopener nofollow ugc">${u.website.replace(/^https?:\/\//, '')}</a></li>` : ''}
      <li>${icon('clock', 'sm')}<span>Joined ${day(u.created_at)}</span></li>
    </ul>
  </aside>`;
}

export default async function (ctx) {
  const name = ctx.params.user;
  const tab = ['repos', 'stars'].includes(ctx.query.get('tab')) ? ctx.query.get('tab') : 'overview';
  const [d, stars] = await Promise.all([get(userUrl(name)), tab === 'stars' ? get(`${userUrl(name)}/stars`) : null]);
  const u = d.user;
  const isMe = state.me?.username?.toLowerCase() === u.username.toLowerCase();
  const base = `/${u.username}`;
  const tabs = html`<nav class="tabs profile-tabs" aria-label="Profile">
    <a href="${base}" class="${tab === 'overview' ? 'on' : ''}">${icon('book', 'sm')} Overview</a>
    <a href="${base}?tab=repos" class="${tab === 'repos' ? 'on' : ''}">${icon('repo', 'sm')} Repositories <span class="counter">${d.repos.length}</span></a>
    <a href="${base}?tab=stars" class="${tab === 'stars' ? 'on' : ''}">${icon('star', 'sm')} Stars <span class="counter">${d.stars}</span></a>
    <span class="bar"></span></nav>`;

  let body;
  if (tab === 'overview') {
    const top = d.repos.slice(0, 6);
    body = html`<div class="stack">
      <div><div class="section-title">${top.length ? 'Recently updated' : 'Repositories'}</div>
        ${top.length ? html`<div class="repo-grid two stagger">${top.map((r, i) => html`<div style="--i:${i}">${repoCard(r, { owner: false })}</div>`)}</div>`
          : html`<div class="box empty small">${icon('repo', 'big')}<h3>${isMe ? "You don't have any repositories yet" : `${u.username} doesn't have any public repositories yet`}</h3>${isMe ? html`<a class="btn primary sm" href="/new">Create one</a>` : ''}</div>`}</div>
      ${heatmap(d.heatmap)}
      <div><div class="section-title">Recent activity</div><div class="box">${eventList(d.events, { compact: true })}</div></div>
    </div>`;
  } else if (tab === 'repos') {
    body = html`<div class="stack">
      <div class="search-input">${icon('search')}<input class="input" id="repo-filter" placeholder="Find a repository…" aria-label="Filter repositories" autocomplete="off"></div>
      ${d.repos.length ? html`<div class="box list profile-repos">${d.repos.map((r) => html`<div class="profile-repo" data-name="${r.name} ${r.description}">${repoCard(r, { owner: false })}</div>`)}</div>
        <p class="muted small" id="repo-none" hidden>No repositories match.</p>`
        : html`<div class="box empty small"><h3>No repositories yet</h3></div>`}
    </div>`;
  } else {
    body = stars.repos.length
      ? html`<div class="repo-grid two stagger">${stars.repos.map((r, i) => html`<div style="--i:${Math.min(i, 16)}">${repoCard(r)}</div>`)}</div>`
      : html`<div class="box empty">${icon('star', 'big')}<h3>${isMe ? "You haven't starred anything yet" : `${u.username} hasn't starred anything yet`}</h3><p>Stars are a way to bookmark repositories.</p><a class="btn sm" href="/explore">Explore</a></div>`;
  }

  return {
    title: u.display_name ? `${u.username} (${u.display_name})` : u.username,
    html: html`<div class="wrap profile">${sidebar(u, isMe)}<section class="profile-main">${tabs}<div class="profile-body">${body}</div></section></div>`,
    mount(el) {
      const tabsEl = $('.profile-tabs', el);
      const on = tabsEl.querySelector('.on');
      const bar = tabsEl.querySelector('.bar');
      if (on && bar) { bar.style.left = on.offsetLeft + 8 + 'px'; bar.style.width = on.offsetWidth - 16 + 'px'; }
      const f = $('#repo-filter', el);
      f?.addEventListener('input', () => {
        const q = f.value.trim().toLowerCase();
        let shown = 0;
        $$('.profile-repo', el).forEach((r) => { const hit = r.dataset.name.toLowerCase().includes(q); r.hidden = !hit; shown += hit; });
        $('#repo-none', el).hidden = shown > 0;
      });
      const hs = $('.heatmap-scroll', el);
      if (hs) hs.scrollLeft = hs.scrollWidth;
    },
  };
}
