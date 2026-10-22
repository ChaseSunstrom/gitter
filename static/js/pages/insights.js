// Repository insights: contributors, weekly commit activity, languages.
import { html, raw, get, avatar, num, plural } from '../lib.js';
import { icon } from '../icons.js';
import { loadRepo, repoUrl, repoPage, mountRepoHeader } from '../repohead.js';

export function prefetch(ctx) {
  const { owner, repo } = ctx.params;
  get(repoUrl(owner, repo));
  get(`${repoUrl(owner, repo)}/contributors`);
}

/** 52 weekly bars with a y-axis and month ticks. */
function activityChart(weeks) {
  const W = 1180, H = 210, left = 34, bottom = 24, top = 10;
  const max = Math.max(1, ...weeks);
  const nice = max <= 4 ? max : Math.ceil(max / 4) * 4;
  const plotW = W - left - 6, plotH = H - top - bottom;
  const bw = plotW / weeks.length;
  const y = (v) => top + plotH - (v / nice) * plotH;
  let grid = '';
  for (let i = 0; i <= 4; i++) {
    const v = Math.round((nice / 4) * i);
    if (i && v === Math.round((nice / 4) * (i - 1))) continue;
    grid += `<line x1="${left}" x2="${W - 6}" y1="${y(v)}" y2="${y(v)}" class="grid"/><text x="${left - 6}" y="${y(v) + 3.5}" text-anchor="end">${v}</text>`;
  }
  const now = new Date();
  let bars = '', ticks = '', lastMonth = -1;
  weeks.forEach((c, i) => {
    const weekStart = new Date(now.getTime() - (weeks.length - 1 - i) * 7 * 86400000);
    const label = `${plural(c, 'commit')} the week of ${weekStart.toLocaleDateString(undefined, { month: 'short', day: 'numeric' })}`;
    const h = Math.max(c ? 2 : 0, (c / nice) * plotH);
    bars += `<rect x="${left + i * bw + 1}" y="${top + plotH - h}" width="${Math.max(1, bw - 2)}" height="${h}" rx="1.5" class="bar${c ? '' : ' zero'}"><title>${label}</title></rect>`;
    const m = weekStart.getMonth();
    if (m !== lastMonth && i > 0 && i < weeks.length - 2) {
      ticks += `<text x="${left + i * bw}" y="${H - 6}">${weekStart.toLocaleDateString(undefined, { month: 'short' })}</text>`;
    }
    lastMonth = m;
  });
  return raw(`<svg class="activity-chart" viewBox="0 0 ${W} ${H}" role="img" aria-label="Commits per week over the last year">${grid}${bars}${ticks}</svg>`);
}

/** 14 daily bars: all of `key`, with the unique part drawn over it. */
function dailyChart(days, key, label) {
  const W = 560, H = 150, left = 30, bottom = 22, top = 8;
  const max = Math.max(1, ...days.map((d) => d[key]));
  const plotW = W - left - 4, plotH = H - top - bottom, bw = plotW / days.length;
  const h = (v) => (v / max) * plotH;
  let out = `<line x1="${left}" x2="${W - 4}" y1="${top}" y2="${top}" class="grid"/><text x="${left - 6}" y="${top + 3.5}" text-anchor="end">${max}</text>`;
  days.forEach((d, i) => {
    const x = left + i * bw + 2, w = Math.max(1, bw - 4), all = d[key], uniq = d['unique_' + key];
    const date = new Date(d.time * 1000).toLocaleDateString(undefined, { month: 'short', day: 'numeric', timeZone: 'UTC' });
    out += `<g><title>${date}: ${plural(all, label)}, ${uniq} unique</title>
      <rect x="${x}" y="${top + plotH - h(all)}" width="${w}" height="${h(all)}" rx="1.5" class="bar soft"/>
      <rect x="${x}" y="${top + plotH - h(uniq)}" width="${w}" height="${h(uniq)}" rx="1.5" class="bar${all ? '' : ' zero'}"/></g>`;
    if (i % 3 === 0) out += `<text x="${x}" y="${H - 6}">${date}</text>`;
  });
  return raw(`<svg class="activity-chart traffic-chart" viewBox="0 0 ${W} ${H}" role="img" aria-label="${label}s per day over the last two weeks">${out}</svg>`);
}

function topList(rows, empty) {
  return rows.length ? html`<ol class="top-list">${rows.map((r) => html`<li><span class="ellipsis grow">${r.name}</span><b class="small">${num(r.count)}</b></li>`)}</ol>`
    : html`<p class="muted small">${empty}</p>`;
}

function trafficBox(t) {
  const sum = (k) => t.days.reduce((a, d) => a + d[k], 0);
  return html`<div class="box traffic"><div class="box-head">${icon('eye', 'sm')}<b>Traffic</b><span class="grow"></span><span class="muted small">Last 14 days (UTC) · only admins see this</span></div>
    <div class="box-body traffic-grid">
      <div><div class="spread"><b>${plural(sum('views'), 'view')}</b><span class="muted small">${num(sum('unique_views'))} unique a day, summed</span></div>${dailyChart(t.days, 'views', 'view')}</div>
      <div><div class="spread"><b>${plural(sum('clones'), 'clone')}</b><span class="muted small">${num(sum('unique_clones'))} unique a day, summed</span></div>${dailyChart(t.days, 'clones', 'clone')}</div>
      <div><b class="small">Referring sites</b>${topList(t.referrers, 'No visits from other sites yet.')}</div>
      <div><b class="small">Popular pages</b>${topList(t.paths, 'No page views yet.')}</div>
    </div></div>`;
}

export default async function (ctx) {
  const { owner, repo } = ctx.params;
  const meta = await loadRepo(owner, repo);
  const [c, l, t] = await Promise.all([
    get(`${repoUrl(owner, repo)}/contributors`),
    get(`${repoUrl(owner, repo)}/languages`).catch(() => ({ languages: [] })),
    meta.can_admin ? get(`${repoUrl(owner, repo)}/traffic`, { fresh: true }).catch(() => null) : null,
  ]);
  const people = c.contributors;
  const top = Math.max(1, ...people.map((p) => p.commits));
  const totalCommits = people.reduce((a, p) => a + p.commits, 0);
  const yearCommits = c.weeks.reduce((a, b) => a + b, 0);
  const langs = l.languages;

  const body = html`<div class="insights stack">
    <div class="stat-row">
      <div class="stat"><b>${num(totalCommits)}</b><span class="muted small">commits on ${meta.default_branch}</span></div>
      <div class="stat"><b>${num(people.length)}</b><span class="muted small">${people.length === 1 ? 'contributor' : 'contributors'}</span></div>
      <div class="stat"><b>${num(meta.stars)}</b><span class="muted small">${meta.stars === 1 ? 'star' : 'stars'}</span></div>
      <div class="stat"><b>${num(meta.forks)}</b><span class="muted small">${meta.forks === 1 ? 'fork' : 'forks'}</span></div>
    </div>
    <div class="box"><div class="box-head">${icon('pulse', 'sm')}<b>Commit activity</b><span class="grow"></span><span class="muted small">${plural(yearCommits, 'commit')} in the last year</span></div>
      <div class="box-body chart-wrap">${meta.empty ? html`<p class="muted">No commits yet.</p>` : activityChart(c.weeks)}</div></div>
    ${t ? trafficBox(t) : ''}
    <div class="insights-cols">
      <div class="box"><div class="box-head">${icon('users', 'sm')}<b>Contributors</b></div>
        ${people.length ? html`<ol class="contrib-list">${people.map((p, i) => html`<li>
          <span class="rank muted tiny">${i + 1}</span>${avatar(p.email_hash, 28)}
          <div class="grow"><div class="spread"><b class="ellipsis">${p.name}</b><span class="small muted nowrap">${plural(p.commits, 'commit')}</span></div>
            <div class="contrib-bar"><span style="width:${(p.commits / top) * 100}%"></span></div></div></li>`)}</ol>`
          : html`<div class="empty small"><h3>No contributors yet</h3><p>Push some commits to see who's working on what.</p></div>`}</div>
      <div class="box"><div class="box-head">${icon('code', 'sm')}<b>Languages</b></div>
        <div class="box-body">${langs.length ? html`<div class="lang-bar big">${langs.map((x) => html`<span style="width:${Math.max(x.percent, 0.6)}%;background:${x.color}" title="${x.name} ${x.percent.toFixed(1)}%"></span>`)}</div>
          <ul class="lang-rows">${langs.map((x) => html`<li><span class="dot" style="background:${x.color}"></span><b>${x.name}</b><span class="grow"></span><span class="muted small">${x.percent.toFixed(1)}%</span></li>`)}</ul>`
          : html`<p class="muted small">No recognized source files yet.</p>`}</div></div>
    </div>
  </div>`;
  return {
    title: `Insights · ${owner}/${repo}`,
    html: repoPage(meta, 'insights', body),
    mount(el) {
      mountRepoHeader(el, meta);
      const wrap = el.querySelector('.chart-wrap');
      if (wrap) wrap.scrollLeft = wrap.scrollWidth;
    },
  };
}
