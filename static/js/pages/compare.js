// Compare two branches and open a pull request.
import { html, state, get, send, action, timeEl, avatar, short, plural, enc, encPath, $ } from '../lib.js';
import { icon } from '../icons.js';
import { loadRepo, repoUrl, repoPage, mountRepoHeader, refPicker, mountRefPicker } from '../repohead.js';
import { diffView, mountDiff } from '../diffview.js';
import { mdEditor, mountMdEditor, clearDraft } from '../mdeditor.js';
import { navigate } from '../main.js';

function pick(meta, rest) {
  const [b, h] = (rest || '').split('...');
  const base = b || meta.default_branch;
  const head = h ?? (meta.branches.find((x) => x !== base) || '');
  return { base, head };
}

/** "fix/readme-typos" -> "Fix readme typos" */
const humanize = (b) => { const s = b.split('/').pop().replace(/[-_]+/g, ' ').trim(); return s ? s[0].toUpperCase() + s.slice(1) : b; };

export async function prefetch(ctx) {
  const meta = await get(repoUrl(ctx.params.owner, ctx.params.repo));
  const { base, head } = pick(meta, ctx.params.rest);
  if (head && head !== base) get(`${repoUrl(ctx.params.owner, ctx.params.repo)}/compare?base=${enc(base)}&head=${enc(head)}`);
}

export default async function (ctx) {
  const { owner, repo } = ctx.params;
  const meta = await loadRepo(owner, repo);
  const { base, head } = pick(meta, ctx.params.rest);
  const root = `/${owner}/${repo}`;
  const url = (b, h) => `${root}/compare/${encPath(b)}...${encPath(h)}`;
  const same = !head || head === base;
  const d = same ? null : await get(`${repoUrl(owner, repo)}/compare?base=${enc(base)}&head=${enc(head)}`);
  const can = !!state.me && d && d.ahead > 0 && meta.branches.includes(head) && meta.branches.includes(base);
  // The repository's pull request template (.gitter/pull_request_template.md) starts the description.
  const prTemplate = can ? (await get(`${repoUrl(owner, repo)}/issue-templates`).catch(() => ({}))).pull || '' : '';
  const title = d?.commits.length === 1 ? d.commits[0].subject : humanize(head || '');

  const pickers = html`<div class="box compare-bar"><div class="box-body row wrap-ok">
    ${icon('pr', 'muted')}
    <span class="muted small">base</span>${refPicker(meta, base, 'base-picker')}
    ${icon('arrow-left', 'muted')}
    <span class="muted small">compare</span>${head ? refPicker(meta, head, 'head-picker') : html`<button class="btn sm" id="head-picker">${icon('branch', 'sm')} Choose a branch ${icon('chev-down', 'sm')}</button>`}
    <span class="grow"></span>
    ${d ? (d.ahead > 0
      ? html`<span class="small c-green row">${icon('check', 'sm')} ${plural(d.ahead, 'commit')} ahead${d.behind ? html`, ${plural(d.behind, 'commit')} behind` : ''}</span>`
      : html`<span class="small muted">${head} has nothing new</span>`) : ''}
  </div></div>`;

  let main;
  if (meta.empty) {
    main = html`<div class="box empty">${icon('branch', 'big')}<h3>This repository is empty</h3><p>Push some branches first.</p></div>`;
  } else if (same) {
    main = html`<div class="box empty">${icon('pr', 'big')}<h3>Compare changes</h3>
      <p>Pick two branches to see what changed and start a pull request.${meta.branches.length < 2 ? ' This repository has only one branch so far.' : ''}</p></div>`;
  } else if (d.ahead === 0) {
    main = html`<div class="box empty">${icon('check', 'big')}<h3>There isn't anything to compare</h3><p><b>${base}</b> already has everything in <b>${head}</b>.
      ${d.behind ? html`<a class="c-accent" href="${url(head, base)}">Swap the branches</a> to see the other direction.` : ''}</p></div>`;
  } else {
    const form = can ? html`<form class="box pr-form" id="pr-form"><div class="box-body stack">
      <div class="row">${avatar(state.me.username, 32)}<input class="input lg-input" name="title" value="${title}" maxlength="256" required aria-label="Pull request title"></div>
      ${mdEditor({ id: 'pr-body', value: prTemplate, placeholder: 'Describe what this changes and why', rows: 6 })}
      <div class="row" style="justify-content:flex-end"><button class="btn primary" type="submit">${icon('pr', 'sm')} Create pull request</button></div>
    </div></form>` : !state.me ? html`<div class="box"><div class="box-body row">${icon('pr')}<span class="grow">Sign in to open a pull request.</span><a class="btn primary" href="/login?next=${encodeURIComponent(location.pathname)}">Sign in</a></div></div>` : '';
    main = html`<div class="stack">
      ${form}
      <div class="box"><div class="box-head">${icon('commit', 'sm')}<b>${plural(d.commits.length, 'commit')}</b></div>
        <div class="list stagger">${d.commits.map((c, i) => html`<div class="list-row" style="--i:${Math.min(i, 20)}">
          ${avatar(c.author_email || c.author_name, 20)}
          <a class="grow ellipsis" href="${root}/commit/${c.sha}">${c.subject}</a>
          <span class="muted small hide-sm">${c.author_name} · ${timeEl(c.commit_time)}</span>
          <a class="sha" href="${root}/commit/${c.sha}">${short(c.sha)}</a></div>`)}</div></div>
      ${diffView(d.diff, { owner, repo, sha: d.head_sha })}
    </div>`;
  }

  const body = html`<div class="stack">
    <div><h2 class="page-h">Compare changes</h2><p class="muted">Choose two branches to see what changed, then open a pull request to discuss and merge.</p></div>
    ${pickers}${main}</div>`;

  return {
    title: `Compare ${base}...${head} · ${owner}/${repo}`,
    html: repoPage(meta, 'pulls', body),
    mount(el) {
      mountRepoHeader(el, meta);
      mountRefPicker(el, meta, base, (r) => url(r, head || r), 'base-picker');
      mountRefPicker(el, meta, head || base, (r) => url(base, r), 'head-picker');
      if (d && d.ahead > 0) mountDiff(el, d.diff, { load: (q) => get(`${repoUrl(owner, repo)}/compare?base=${enc(base)}&head=${enc(head)}&${q}`).then((x) => x.diff.files[0]) });
      mountMdEditor(el, { owner, repo, rev: meta.default_branch, dir: '' });
      const f = $('#pr-form', el);
      if (f) {
        const btn = f.querySelector('[type=submit]');
        f.addEventListener('submit', action(btn, async () => {
          const r = await send('POST', `${repoUrl(owner, repo)}/pulls`, { title: f.title.value.trim(), body: $('#pr-body', f).value, head, base });
          clearDraft('pr-body');
          navigate(`${root}/pull/${r.number}`);
        }));
      }
    },
  };
}
