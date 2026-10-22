// A single issue or pull request: conversation, commits and changed files.
import { html, raw, state, get, send, action, timeEl, avatar, short, plural, $, popover, closePop, confirmDialog, toast, toastErr, encPath, reportDialog, agentBadge, safeUrl } from '../lib.js';
import { icon } from '../icons.js';
import { loadRepo, repoUrl, repoPage, mountRepoHeader, placeTabBar, ciIcon } from '../repohead.js';
import { renderMarkdown, preloadLangs } from '../md.js';
import { mdEditor, mountMdEditor, clearDraft } from '../mdeditor.js';
import { diffView, mountDiff } from '../diffview.js';
import { diffReview, reviewBar, mountReview, reviewItems, reviewersBlock } from '../review.js';
import { refresh } from '../main.js';
import { labelChip } from './issues.js';

const STATE_TEXT = { open: 'Open', closed: 'Closed', merged: 'Merged' };

function statePill(issue) {
  const ic = issue.is_pr
    ? (issue.state === 'merged' ? 'merged' : issue.state === 'closed' ? 'pr-closed' : 'pr')
    : (issue.state === 'open' ? 'issue' : 'issue-closed');
  const cls = issue.state === 'open' ? 'open' : issue.is_pr && issue.state === 'closed' ? 'closed' : 'merged';
  return html`<span class="pill big ${cls}">${icon(ic, 'sm')} ${STATE_TEXT[issue.state] || issue.state}</span>`;
}

export async function prefetch(ctx) {
  const { owner, repo, n, kind } = ctx.params;
  get(repoUrl(owner, repo));
  get(`${repoUrl(owner, repo)}/issues/${n}`);
  if (kind === 'pr' && ctx.params.tab && ctx.params.tab !== 'conversation') get(`${repoUrl(owner, repo)}/pulls/${n}/diff`);
}

export default async function (ctx) {
  const { owner, repo, n } = ctx.params;
  const kind = ctx.params.kind;
  const tab = kind === 'pr' ? (['commits', 'files'].includes(ctx.params.tab) ? ctx.params.tab : 'conversation') : 'conversation';
  if (!/^\d+$/.test(n)) return { redirect: `/${owner}/${repo}/${kind === 'pr' ? 'pulls' : 'issues'}` };
  const api = repoUrl(owner, repo);
  const [meta, data] = await Promise.all([loadRepo(owner, repo), get(`${api}/issues/${n}`)]);
  const issue = data.issue;
  if (issue.is_pr && kind !== 'pr') return { redirect: `/${owner}/${repo}/pull/${n}` };
  if (!issue.is_pr && kind === 'pr') return { redirect: `/${owner}/${repo}/issues/${n}` };
  const [pd, rv] = issue.is_pr
    ? await Promise.all([get(`${api}/pulls/${n}/diff`), tab === 'commits' ? null : get(`${api}/pulls/${n}/reviews`, { fresh: true }).catch((e) => { if (e.status !== 400) toastErr(e); return null; })])
    : [null, null];
  const rctx = { api, n, isAuthor: state.me?.username === issue.author };
  const mdCtx = { owner, repo, rev: meta.default_branch, dir: '' };
  await preloadLangs([issue.body, ...data.comments.map((c) => c.body), ...(rv?.threads || []).flatMap((t) => t.comments.map((c) => c.body))].join('\n'));
  rctx.mdCtx = { owner, repo, rev: meta.default_branch, dir: '' };

  const base = `/${owner}/${repo}`;
  const prBase = `${base}/pull/${n}`;
  let content;
  if (tab === 'commits') content = commitsTab(base, pd);
  else if (tab === 'files') content = html`<div class="pr-files">${reviewBar(rv)}${diffView(pd.diff, { owner, repo, sha: pd.head_sha, key: `${owner}/${repo}#${n}:${pd.head_sha}` })}</div>`;
  else content = conversation(meta, data, pd, mdCtx, rv);

  const body = html`<div class="issue-page">
    ${header(meta, data, pd)}
    ${issue.is_pr ? html`<nav class="tabs pr-tabs" aria-label="Pull request">
      <a href="${prBase}" class="${tab === 'conversation' ? 'on' : ''}">${icon('comment', 'sm')} Conversation <span class="counter">${data.comments.filter((c) => c.kind === 'comment').length}</span></a>
      <a href="${prBase}/commits" class="${tab === 'commits' ? 'on' : ''}">${icon('commit', 'sm')} Commits <span class="counter">${pd.commits.length}</span></a>
      <a href="${prBase}/files" class="${tab === 'files' ? 'on' : ''}">${icon('file', 'sm')} Files changed <span class="counter">${pd.diff.files.length}</span></a>
      <span class="bar"></span></nav>` : ''}
    ${content}
  </div>`;

  return {
    title: `${issue.title} · ${issue.is_pr ? 'Pull request' : 'Issue'} #${n} · ${owner}/${repo}`,
    html: repoPage(meta, issue.is_pr ? 'pulls' : 'issues', body),
    mount(el) {
      mountRepoHeader(el, meta);
      placeTabBar(el.querySelector('.issue-page'));
      mountHeader(el, api, issue);
      if (tab === 'files') {
        mountDiff(el, pd.diff, { key: `${owner}/${repo}#${n}:${pd.head_sha}`, load: (q) => get(`/repos/${owner}/${repo}/pulls/${n}/diff?${q}`).then((x) => x.diff.files[0]), ...(rv ? diffReview(rv, rctx) : {}) });
      }
      if (tab !== 'commits') mountReview(el.querySelector('.issue-page'), rv, rctx);
      if (tab === 'conversation') mountConversation(el, meta, api, data, mdCtx);
    },
  };
}

// ---------------------------------------------------------------- header

function header(meta, data, pd) {
  const { issue, pull } = data;
  const base = `/${meta.owner}/${meta.name}`;
  let line;
  if (issue.is_pr) {
    const branch = (b, exists) => html`<a class="branch-name" href="${exists ? `${base}/tree/${encPath(b)}` : '#'}">${b}</a>`;
    const count = plural(pd.commits.length, 'commit');
    line = issue.state === 'merged'
      ? html`<b>${pull.merged_by || issue.author}</b> merged ${count} into ${branch(pull.base, pull.base_exists)} from ${branch(pull.head, pull.head_exists)} ${pull.merged_at ? timeEl(pull.merged_at) : ''}`
      : html`<b>${issue.author}</b> wants to merge ${count} into ${branch(pull.base, pull.base_exists)} from ${branch(pull.head, pull.head_exists)}`;
  } else {
    line = html`<b>${issue.author}</b> opened this issue ${timeEl(issue.created_at)} · ${plural(issue.comments, 'comment')}`;
  }
  return html`<header class="issue-head">
    <div class="issue-title-row" id="title-view">
      <h1><span class="issue-title-text">${issue.title}</span> <span class="issue-num">#${issue.number}</span></h1>
      <div class="row">
        ${data.can_edit ? html`<button class="btn sm" id="edit-title">${icon('edit', 'sm')} Edit</button>` : ''}
        ${data.signed_in ? html`<a class="btn sm primary" href="${issue.is_pr ? `${base}/compare` : `${base}/issues/new`}">New ${issue.is_pr ? 'pull request' : 'issue'}</a>` : ''}
      </div>
    </div>
    <form class="issue-title-row" id="title-edit" hidden>
      <input class="input" name="title" value="${issue.title}" maxlength="256" aria-label="Title" required>
      <button class="btn primary" type="submit">Save</button><button class="btn" type="button" id="cancel-title">Cancel</button>
    </form>
    <div class="row wrap-ok issue-meta">${statePill(issue)}<span class="muted">${line}</span></div>
  </header>`;
}

function mountHeader(el, api, issue) {
  const view = $('#title-view', el);
  const form = $('#title-edit', el);
  if (!form) return;
  $('#edit-title', el)?.addEventListener('click', () => { view.hidden = true; form.hidden = false; form.title.focus(); form.title.select(); });
  $('#cancel-title', el).addEventListener('click', () => { form.hidden = true; view.hidden = false; });
  const btn = form.querySelector('[type=submit]');
  form.addEventListener('submit', action(btn, async () => {
    const title = form.title.value.trim();
    if (title && title !== issue.title) await send('PATCH', `${api}/issues/${issue.number}`, { title });
    refresh();
  }));
}

// ---------------------------------------------------------------- conversation

function card({ id, author, time, edited, bodyHtml, actions, tag = '', agent = null }) {
  return html`<div class="tl-item tl-comment" id="${id}">
    <a href="/${author}" class="tl-avatar hide-sm">${avatar(author, 36)}</a>
    <div class="comment-card">
      <div class="comment-head">
        <a href="/${author}" class="row"><span class="show-sm">${avatar(author, 20)}</span><b>${author}</b></a>${agentBadge(agent, author)}
        <span class="muted small">commented <a href="#${id}">${timeEl(time)}</a>${edited ? ' · edited' : ''}</span>
        ${tag ? html`<span class="tag-private">${tag}</span>` : ''}
        <span class="grow"></span>
        <button class="btn ghost sm icon" data-menu="${id}" aria-label="More actions">${icon('dots', 'sm')}</button>
      </div>
      <div class="comment-body markdown-body" data-body-of="${id}">${bodyHtml}</div>
      ${actions || ''}
    </div>
  </div>`;
}

function eventRow(c, meta, base) {
  const words = {
    closed: [base.isPr ? 'pr-closed' : 'issue-closed', 'c-red', 'closed this'],
    reopened: [base.isPr ? 'pr' : 'issue', 'c-green', 'reopened this'],
    merged: ['merged', 'c-purple', null],
  }[c.kind];
  if (!words) return '';
  const [ic, cls, text] = words;
  return html`<div class="tl-item tl-event">
    <span class="tl-event-icon ${cls}">${icon(ic, 'sm')}</span>
    <span class="small">${avatar(c.author, 18)} <b>${c.author}</b> ${text ?? html`merged commit <a class="sha" href="/${meta.owner}/${meta.name}/commit/${c.body}">${short(c.body)}</a> into <b>${base.base}</b>`} ${timeEl(c.created_at)}</span>
  </div>`;
}

function mergeBox(meta, data, pd) {
  const { issue, pull } = data;
  const base = `/${meta.owner}/${meta.name}`;
  if (issue.state === 'merged') {
    return html`<div class="merge-box merged-box">
      <span class="merge-icon">${icon('merged')}</span>
      <div class="grow"><b>Pull request merged</b><div class="muted small">${pull.merged_by ? html`${pull.merged_by} merged ` : 'Merged '}${pull.merged_sha ? html`commit <a class="sha" href="${base}/commit/${pull.merged_sha}">${short(pull.merged_sha)}</a> ` : ''}into ${pull.base}${pull.merged_at ? html` ${timeEl(pull.merged_at)}` : ''}.</div></div>
      ${pull.head_exists && meta.can_write && pull.head !== meta.default_branch ? html`<button class="btn sm" id="delete-head">${icon('trash', 'sm')} Delete branch</button>` : ''}
    </div>`;
  }
  if (issue.state === 'closed') {
    return html`<div class="merge-box"><span class="merge-icon closed">${icon('pr-closed')}</span><div class="grow"><b>Closed with unmerged commits</b><div class="muted small">Reopen it to merge.</div></div></div>`;
  }
  let status;
  if (pull.reason === 'missing_branch') {
    status = html`<span class="merge-icon warn">${icon('x')}</span><div class="grow"><b>${pull.head_exists ? 'The target branch is gone' : 'The source branch is gone'}</b><div class="muted small">Restore ${pull.head_exists ? pull.base : pull.head} to merge this pull request.</div></div>`;
  } else if (pull.reason === 'conflicts') {
    status = html`<span class="merge-icon warn">${icon('x')}</span><div class="grow"><b>This branch has conflicts that must be resolved</b>
      <div class="muted small">Merge ${pull.base} into ${pull.head} locally, fix these files, and push:</div>
      <ul class="conflict-list">${(pull.conflicts || []).map((f) => html`<li class="mono">${f}</li>`)}</ul></div>`;
  } else if (pd.commits.length === 0) {
    status = html`<span class="merge-icon">${icon('check')}</span><div class="grow"><b>Nothing to merge</b><div class="muted small">${pull.head} has no commits that ${pull.base} doesn't have.</div></div>`;
  } else {
    status = html`<span class="merge-icon ok">${icon('check')}</span><div class="grow"><b>No conflicts with ${pull.base}</b><div class="muted small">This branch can be merged automatically.</div></div>`;
  }
  const blocked = pull.blocked || [];
  const canMerge = meta.can_write && pull.mergeable && pd.commits.length > 0 && (!blocked.length || pull.can_bypass);
  const checks = pull.checks || [];
  const ST = { success: 'success', failure: 'failure', pending: 'running' };
  const checksList = checks.length ? html`<div class="merge-checks">${checks.map((c) => html`<div class="row small merge-check">${ciIcon(ST[c.state] || 'pending')}
    <b class="mono">${c.name}</b><span class="muted grow ellipsis">${c.description}</span>${c.url ? html`<a href="${safeUrl(c.url)}" rel="noopener">Details</a>` : ''}</div>`)}</div>` : '';
  const rules = blocked.length ? html`<div class="merge-blocked small"><b>${icon('shield', 'sm')} ${pull.base} is protected. Before merging:</b>
    <ul>${blocked.map((b) => html`<li>${b}</li>`)}</ul></div>` : '';
  const auto = pull.auto_merge ? html`<div class="row small merge-auto">${icon('zap', 'sm')}<span class="grow">This pull request merges itself (${pull.auto_merge.method === 'squash' ? 'squashed' : pull.auto_merge.method === 'ff' ? 'fast-forward' : 'merge commit'}) once the requirements are met${pull.auto_merge.by ? `, as ${pull.auto_merge.by} asked` : ''}.</span>
    ${meta.can_write ? html`<button class="btn sm" id="auto-off">Cancel</button>` : ''}</div>`
    : meta.can_write && pull.mergeable && blocked.length && pd.commits.length ? html`<div class="row small merge-auto"><span class="grow muted">Merge it by itself when the checks pass and the reviews are in.</span>
      <select class="select" id="auto-method" aria-label="Auto-merge method"><option value="merge">Merge commit</option><option value="squash">Squash</option></select>
      <button class="btn sm" id="auto-on">${icon('zap', 'sm')} Merge when ready</button></div>` : '';
  return html`<div class="merge-box ${canMerge && !blocked.length ? 'ready' : ''}">
    <div class="row merge-status">${status}${pd.head_sha ? html`<span class="merge-ci" data-ci-sha="${pd.head_sha}" data-ci-label="1"></span>` : ''}</div>
    ${checksList}${rules}${auto}
    ${canMerge ? html`<form class="merge-form" id="merge-form">
      <select class="select" name="method" aria-label="Merge method">
        <option value="merge">Create a merge commit</option>
        <option value="squash">Squash and merge</option>
        ${pull.can_ff ? html`<option value="ff">Fast-forward</option>` : ''}
      </select>
      <label class="check small"><input type="checkbox" name="delete_branch" ${pull.head !== meta.default_branch ? '' : 'disabled'}> Delete ${pull.head} after merging</label>
      ${blocked.length && pull.can_bypass ? html`<label class="check small"><input type="checkbox" name="bypass_rules"> Merge without waiting (you're an admin)</label>` : ''}
      <span class="grow"></span>
      <button class="btn green" type="submit">${icon('merged', 'sm')} Merge pull request</button>
    </form>` : !meta.can_write && pull.mergeable ? html`<div class="muted small merge-note">Only people with write access can merge.</div>` : ''}
  </div>`;
}

function sidebar(meta, data, rv) {
  const { issue, participants, all_labels } = data;
  return html`<aside class="issue-side">
    ${issue.is_pr ? reviewersBlock(rv) : ''}
    <div class="side-block" id="labels-block">
      <div class="section-title">${icon('label', 'sm')} Labels
        ${data.can_write ? html`<span class="grow"></span><button class="btn ghost sm icon" id="edit-labels" aria-label="Edit labels">${icon('settings', 'sm')}</button>` : ''}</div>
      <div class="row wrap-ok" id="label-list">${issue.labels.length ? issue.labels.map((l) => labelChip(l, `/${meta.owner}/${meta.name}/issues?label=${encodeURIComponent(l.name)}`)) : html`<span class="muted small">None yet</span>`}</div>
    </div>
    <div class="side-block">
      <div class="section-title">${icon('users', 'sm')} ${plural(participants.length, 'participant')}</div>
      <div class="row wrap-ok">${participants.map((p) => html`<a href="/${p}" title="${p}">${avatar(p, 26)}</a>`)}</div>
    </div>
    <div class="side-block">
      <button class="btn sm" data-copy="${location.origin}${location.pathname}" data-copy-label="Link copied">${icon('link', 'sm')} Copy link</button>
    </div>
    ${all_labels.length === 0 && data.can_write ? html`<a class="small c-accent" href="/${meta.owner}/${meta.name}/labels">Create labels</a>` : ''}
  </aside>`;
}

function conversation(meta, data, pd, mdCtx, rv) {
  const { issue } = data;
  const base = { isPr: issue.is_pr, base: data.pull?.base };
  const timeline = [
    ...data.comments.map((c) => ({ time: c.created_at, node: c.kind === 'comment'
      ? card({ id: `comment-${c.id}`, author: c.author, time: c.created_at, edited: c.updated_at > c.created_at + 5, tag: c.author === issue.author ? 'Author' : '', agent: c.agent,
        bodyHtml: c.hidden && !c.body ? html`<p class="muted"><em>A moderator hid this comment.</em></p>`
          : html`${c.hidden ? html`<p class="mod-note">${icon('eye', 'sm')} A moderator hid this comment; only you and admins can read it.</p>` : ''}${raw(renderMarkdown(c.body, mdCtx))}` })
      : eventRow(c, meta, base) })),
    ...reviewItems(rv, mdCtx),
  ].sort((a, b) => a.time - b.time);
  const items = [
    card({ id: 'issue-body', author: issue.author, time: issue.created_at, tag: 'Author', agent: issue.agent,
      bodyHtml: issue.body.trim() ? raw(renderMarkdown(issue.body, mdCtx)) : html`<p class="muted"><em>No description provided.</em></p>` }),
    ...timeline.map((x) => x.node),
  ];
  const mayToggle = data.can_edit && issue.state !== 'merged';
  const noun = issue.is_pr ? 'pull request' : 'issue';
  const composer = data.signed_in
    ? html`<div class="tl-item tl-composer">
        <span class="tl-avatar hide-sm">${avatar(state.me.username, 36)}</span>
        <form class="comment-card composer" id="composer">
          <div class="comment-head"><b>Add a comment</b></div>
          <div class="composer-body">${mdEditor({ id: 'comment-body', placeholder: 'Leave a comment', rows: 5 })}</div>
          <div class="composer-actions">
            ${mayToggle ? html`<button class="btn" type="button" id="toggle-state" data-to="${issue.state === 'open' ? 'closed' : 'open'}">
              ${issue.state === 'open' ? html`${icon(issue.is_pr ? 'pr-closed' : 'issue-closed', 'sm c-purple')} <span>Close ${noun}</span>` : html`${icon(issue.is_pr ? 'pr' : 'issue', 'sm c-green')} <span>Reopen ${noun}</span>`}</button>` : ''}
            <button class="btn primary" type="submit">Comment</button>
          </div>
        </form></div>`
    : html`<div class="box signin-prompt"><div class="box-body row">${icon('comment')}<span class="grow">Sign in to join the conversation.</span><a class="btn primary" href="/login?next=${encodeURIComponent(location.pathname)}">Sign in</a></div></div>`;
  return html`<div class="split issue-split">
    <div class="timeline">
      ${items}
      ${issue.is_pr ? html`<div class="tl-item tl-merge">${mergeBox(meta, data, pd)}</div>` : ''}
      ${composer}
    </div>
    ${sidebar(meta, data, rv)}
  </div>`;
}

function mountConversation(el, meta, api, data, mdCtx) {
  const { issue } = data;
  mountMdEditor(el, mdCtx);

  // Composer: comment, and optionally close/reopen in the same step.
  const form = $('#composer', el);
  if (form) {
    const ta = $('#comment-body', form);
    const toggle = $('#toggle-state', form);
    const submitBtn = form.querySelector('[type=submit]');
    const noun = issue.is_pr ? 'pull request' : 'issue';
    const relabel = () => {
      if (!toggle) return;
      const span = toggle.querySelector('span');
      const closing = toggle.dataset.to === 'closed';
      span.textContent = ta.value.trim() ? `${closing ? 'Close' : 'Reopen'} with comment` : `${closing ? 'Close' : 'Reopen'} ${noun}`;
    };
    ta.addEventListener('input', relabel);
    relabel();
    const post = async (newState) => {
      const body = ta.value.trim();
      if (!body && !newState) { toast('Write a comment first', 'err'); ta.focus(); return; }
      await send('POST', `${api}/issues/${issue.number}/comments`, { body, state: newState || undefined });
      clearDraft('comment-body');
      await refresh();
    };
    form.addEventListener('submit', action(submitBtn, () => post(null)));
    toggle?.addEventListener('click', action(toggle, () => post(toggle.dataset.to)));
  }

  // Per-comment menu: edit, delete, quote, copy link.
  el.querySelector('.timeline')?.addEventListener('click', (e) => {
    const m = e.target.closest('[data-menu]');
    if (!m) return;
    const id = m.dataset.menu;
    const isBody = id === 'issue-body';
    const cid = isBody ? null : +id.replace('comment-', '');
    const c = isBody ? { body: issue.body, can_edit: data.can_edit } : data.comments.find((x) => x.id === cid);
    popover(m, html`
      <button class="menu-item" data-act="link">${icon('link', 'sm')} Copy link</button>
      ${data.signed_in ? html`<button class="menu-item" data-act="quote">${icon('comment', 'sm')} Quote reply</button>` : ''}
      ${c?.can_edit ? html`<div class="menu-sep"></div><button class="menu-item" data-act="edit">${icon('edit', 'sm')} Edit</button>` : ''}
      ${c?.can_edit && !isBody ? html`<button class="menu-item danger-item" data-act="delete">${icon('trash', 'sm')} Delete</button>` : ''}
      ${data.signed_in && (isBody ? issue.author : c?.author) !== state.me?.username ? html`<div class="menu-sep"></div><button class="menu-item" data-act="report">${icon('flag', 'sm')} Report</button>` : ''}`,
    {
      align: 'right', width: 200,
      onMount(p) {
        p.addEventListener('click', async (ev) => {
          const act = ev.target.closest('[data-act]')?.dataset.act;
          if (!act) return;
          closePop();
          if (act === 'link') navigator.clipboard?.writeText(`${location.origin}${location.pathname}#${id}`).then(() => toast('Link copied'));
          if (act === 'quote') {
            const ta = $('#comment-body', el);
            if (!ta) return;
            const quoted = c.body.split('\n').map((l) => '> ' + l).join('\n');
            ta.value = (ta.value ? ta.value.trimEnd() + '\n\n' : '') + quoted + '\n\n';
            ta.dispatchEvent(new Event('input'));
            ta.focus();
            ta.scrollIntoView({ block: 'center', behavior: 'smooth' });
          }
          if (act === 'edit') startEdit(el, api, issue, id, cid, c, mdCtx);
          if (act === 'report') reportDialog(isBody ? { kind: 'issue', owner: meta.owner, repo: meta.name, number: issue.number } : { kind: 'comment', owner: meta.owner, repo: meta.name, comment: cid }, isBody ? (issue.is_pr ? 'pull request' : 'issue') : 'comment');
          if (act === 'delete') {
            const ok = await confirmDialog({ title: 'Delete this comment?', text: 'This can’t be undone.', ok: 'Delete', danger: true });
            if (!ok) return;
            try { await send('DELETE', `${api}/comments/${cid}`); toast('Comment deleted'); refresh(); } catch (err) { toastErr(err); }
          }
        });
      },
    });
  });

  // Labels picker (writers).
  $('#edit-labels', el)?.addEventListener('click', (e) => {
    const chosen = new Set(issue.labels.map((l) => l.id));
    const pop = popover(e.currentTarget, html`<div class="menu-head">Apply labels</div>
      ${data.all_labels.length ? data.all_labels.map((l) => html`<label class="menu-item label-opt"><input type="checkbox" value="${l.id}" ${chosen.has(l.id) ? 'checked' : ''}><span class="dot" style="background:${l.color}"></span><span class="grow">${l.name}</span></label>`)
        : html`<div class="small muted" style="padding:8px 10px">No labels yet.</div>`}
      <div class="menu-sep"></div><a class="menu-item" href="/${meta.owner}/${meta.name}/labels">${icon('settings', 'sm')} Manage labels</a>`,
    { align: 'right', width: 250 });
    pop.addEventListener('change', async (ev) => {
      const cb = ev.target.closest('input[type=checkbox]');
      if (!cb) return;
      cb.checked ? chosen.add(+cb.value) : chosen.delete(+cb.value);
      try {
        await send('PATCH', `${api}/issues/${issue.number}`, { labels: [...chosen] });
        issue.labels = data.all_labels.filter((l) => chosen.has(l.id));
        $('#label-list', el).innerHTML = (issue.labels.length
          ? html`${issue.labels.map((l) => labelChip(l, `/${meta.owner}/${meta.name}/issues?label=${encodeURIComponent(l.name)}`))}`
          : html`<span class="muted small">None yet</span>`).s;
      } catch (err) { toastErr(err); cb.checked = !cb.checked; }
    });
  });

  // Merge.
  const mf = $('#merge-form', el);
  if (mf) {
    const btn = mf.querySelector('[type=submit]');
    mf.addEventListener('submit', action(btn, async () => {
      const method = mf.method.value;
      const label = { merge: 'Merge', squash: 'Squash and merge', ff: 'Fast-forward' }[method];
      if (data.pull.blocked?.length && !mf.bypass_rules?.checked) return toast('Tick “Merge without waiting” to merge before the requirements are met', 'err');
      const ok = await confirmDialog({ title: `${label} #${issue.number}?`, text: `This updates ${data.pull.base} with the changes from ${data.pull.head}.`, ok: label });
      if (!ok) return;
      await send('POST', `${api}/pulls/${issue.number}/merge`, { method, delete_branch: mf.delete_branch.checked, bypass_rules: !!mf.bypass_rules?.checked });
      toast('Pull request merged');
      await refresh();
      document.querySelector('.merged-box')?.classList.add('celebrate');
    }));
  }
  const on = $('#auto-on', el);
  on?.addEventListener('click', action(on, async () => {
    await send('PUT', `${api}/pulls/${issue.number}/auto-merge`, { method: $('#auto-method', el).value });
    toast('It will merge itself when ready');
    refresh();
  }));
  const off = $('#auto-off', el);
  off?.addEventListener('click', action(off, async () => {
    await send('DELETE', `${api}/pulls/${issue.number}/auto-merge`);
    toast('Auto-merge cancelled');
    refresh();
  }));
  const del = $('#delete-head', el);
  del?.addEventListener('click', action(del, async () => {
    await send('DELETE', `${api}/branches?name=${encodeURIComponent(data.pull.head)}`);
    toast(`Deleted ${data.pull.head}`);
    refresh();
  }));
}

function startEdit(el, api, issue, id, cid, c, mdCtx) {
  const bodyEl = el.querySelector(`[data-body-of="${id}"]`);
  if (!bodyEl || bodyEl.querySelector('form')) return;
  const prev = bodyEl.innerHTML;
  const edId = `edit-${id}`;
  bodyEl.innerHTML = html`<form class="edit-form">${mdEditor({ id: edId, value: c.body, rows: 6, draft: false })}
    <div class="row" style="justify-content:flex-end;margin-top:8px"><button class="btn" type="button" data-cancel>Cancel</button><button class="btn primary" type="submit">Save</button></div></form>`.s;
  mountMdEditor(bodyEl, mdCtx);
  const form = bodyEl.querySelector('form');
  const ta = form.querySelector('textarea');
  ta.focus();
  ta.setSelectionRange(ta.value.length, ta.value.length);
  form.querySelector('[data-cancel]').addEventListener('click', () => { bodyEl.innerHTML = prev; });
  const btn = form.querySelector('[type=submit]');
  form.addEventListener('submit', action(btn, async () => {
    const v = ta.value;
    if (cid) await send('PATCH', `${api}/comments/${cid}`, { body: v });
    else await send('PATCH', `${api}/issues/${issue.number}`, { body: v });
    toast('Saved');
    refresh();
  }));
}

// ---------------------------------------------------------------- commits tab

function commitsTab(base, pd) {
  if (!pd.commits.length) return html`<div class="box empty">${icon('commit', 'big')}<h3>No commits</h3><p>The source branch has nothing new.</p></div>`;
  return html`<div class="box"><div class="list stagger">${pd.commits.map((c, i) => html`<div class="list-row" style="--i:${Math.min(i, 20)}">
    ${avatar(c.author_email || c.author_name, 22)}
    <div class="grow" style="min-width:0"><a class="ellipsis issue-title" style="display:block" href="${base}/commit/${c.sha}">${c.subject}</a>
      <div class="muted small"><b>${c.author_name}</b> committed ${timeEl(c.commit_time)}</div></div>
    <a class="sha" href="${base}/commit/${c.sha}">${short(c.sha)}</a>
    <a class="btn ghost sm icon hide-sm" href="${base}/tree/${c.sha}" title="Browse files at this commit" aria-label="Browse files">${icon('code', 'sm')}</a>
  </div>`)}</div></div>`;
}

