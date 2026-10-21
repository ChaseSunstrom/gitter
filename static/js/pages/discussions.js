// Repository discussions: the list (by category), a new thread, and a thread with replies, answer and votes.
import { html, raw, state, get, send, action, timeEl, avatar, num, plural, $, popover, closePop, confirmDialog, toast, toastErr } from '../lib.js';
import { icon } from '../icons.js';
import { loadRepo, repoUrl, repoPage, mountRepoHeader } from '../repohead.js';
import { renderMarkdown, preloadLangs } from '../md.js';
import { mdEditor, mountMdEditor, clearDraft } from '../mdeditor.js';
import { navigate, refresh } from '../main.js';

const CATS = { general: 'General', qa: 'Q&A', ideas: 'Ideas', announcements: 'Announcements' };

function listUrl(ctx) {
  const { owner, repo } = ctx.params;
  const q = ctx.query;
  return `${repoUrl(owner, repo)}/discussions?category=${encodeURIComponent(q.get('category') || '')}&sort=${q.get('sort') === 'top' ? 'top' : 'new'}&page=${Math.max(1, +q.get('page') || 1)}`;
}

export function prefetch(ctx) {
  const { owner, repo, view, n } = ctx.params;
  get(repoUrl(owner, repo));
  if (view === 'list') get(listUrl(ctx));
  if (view === 'show' && /^\d+$/.test(n)) get(`${repoUrl(owner, repo)}/discussions/${n}`);
}

export default async function (ctx) {
  const { owner, repo, view } = ctx.params;
  if (view === 'new') return newView(ctx, await loadRepo(owner, repo));
  if (view === 'show') return showView(ctx);
  const [meta, data] = await Promise.all([loadRepo(owner, repo), get(listUrl(ctx))]);
  return listView(ctx, meta, data);
}

const catPill = (c) => html`<span class="tag-private">${CATS[c] || c}</span>`;

// ---------------------------------------------------------------- list

function listView(ctx, meta, data) {
  const base = `/${meta.owner}/${meta.name}/discussions`;
  const cat = ctx.query.get('category') || '';
  const sort = ctx.query.get('sort') === 'top' ? 'top' : 'new';
  const link = (over) => {
    const m = { category: cat, sort, page: 1, ...over };
    const p = new URLSearchParams();
    if (m.category) p.set('category', m.category);
    if (m.sort === 'top') p.set('sort', 'top');
    if (m.page > 1) p.set('page', m.page);
    return base + (p.toString() ? '?' + p : '');
  };
  const total = data.categories.reduce((a, c) => a + c.count, 0);
  const rows = data.items.map((d, i) => html`<div class="list-row issue-row" style="--i:${Math.min(i, 20)}">
    <span class="vote-count" title="${plural(d.votes, 'vote')}">${icon('chev-up', 'sm')}<b>${num(d.votes)}</b></span>
    <div class="grow" style="min-width:0">
      <div class="issue-line"><a class="issue-title" href="${base}/${d.number}">${d.title}</a>${catPill(d.category)}
        ${d.answered ? html`<span class="pill answer-tag">${icon('check', 'sm')} Answered</span>` : ''}</div>
      <div class="muted small">#${d.number} started by <b>${d.author}</b> ${timeEl(d.created_at)} · active ${timeEl(d.updated_at)}</div>
    </div>
    <a class="muted small row nowrap" href="${base}/${d.number}" title="${plural(d.comments, 'reply', 'replies')}">${icon('comment', 'sm')}${d.comments}</a>
  </div>`);
  const body = html`<div class="split left">
    <aside class="stack">
      ${data.signed_in ? html`<a class="btn primary" href="${base}/new${cat ? '?category=' + cat : ''}">${icon('plus', 'sm')} New discussion</a>` : ''}
      <div class="box"><div class="list">
        <a class="list-row ${cat ? '' : 'on'}" href="${link({ category: '' })}">${icon('comment', 'sm')}<span class="grow">All discussions</span><span class="counter">${num(total)}</span></a>
        ${data.categories.map((c) => html`<a class="list-row ${cat === c.id ? 'on' : ''}" href="${link({ category: c.id })}"><span class="grow">${c.name}</span><span class="counter">${num(c.count)}</span></a>`)}
      </div></div>
    </aside>
    <div class="stack">
      <div class="toolbar"><h1 class="page-h grow">${cat ? CATS[cat] : 'Discussions'}</h1>
        <div class="seg"><a href="${link({ sort: 'new' })}" class="${sort === 'new' ? 'on' : ''}">Latest activity</a><a href="${link({ sort: 'top' })}" class="${sort === 'top' ? 'on' : ''}">Top</a></div></div>
      <div class="box">${data.items.length ? html`<div class="list stagger">${rows}</div>` : html`<div class="empty">${icon('comment', 'big')}<h3>No discussions yet</h3>
        <p>Ask questions, share ideas and talk about this project with its community.</p>
        ${data.signed_in ? html`<a class="btn primary" href="${base}/new">Start a discussion</a>` : ''}</div>`}</div>
      ${data.page > 1 || data.has_more ? html`<div class="row" style="justify-content:center">
        ${data.page > 1 ? html`<a class="btn" href="${link({ page: data.page - 1 })}">Newer</a>` : ''}
        ${data.has_more ? html`<a class="btn" href="${link({ page: data.page + 1 })}">Older</a>` : ''}</div>` : ''}
    </div>
  </div>`;
  return { title: `Discussions · ${meta.owner}/${meta.name}`, html: repoPage(meta, 'discussions', body), mount: (el) => mountRepoHeader(el, meta) };
}

// ---------------------------------------------------------------- new

function newView(ctx, meta) {
  if (!state.me) return { redirect: '/login?next=' + encodeURIComponent(ctx.url.pathname) };
  const base = `/${meta.owner}/${meta.name}/discussions`;
  const want = ctx.query.get('category') || 'general';
  const cats = Object.entries(CATS).filter(([id]) => id !== 'announcements' || meta.can_write);
  const body = html`<form class="stack narrow" id="new-discussion">
    <h1 class="page-h">${icon('comment')} Start a discussion</h1>
    <div class="field"><label for="d-cat">Category</label><select class="select" id="d-cat" name="category">${cats.map(([id, name]) => html`<option value="${id}" ${id === want ? 'selected' : ''}>${name}</option>`)}</select></div>
    <div class="field"><label for="d-title">Title</label><input class="input lg-input" id="d-title" name="title" required maxlength="256" autofocus autocomplete="off"></div>
    <div class="field"><label for="d-body">Body</label>${mdEditor({ id: 'd-body', placeholder: 'Ask a question, share an idea, start a conversation…', rows: 10 })}</div>
    <div class="row" style="justify-content:flex-end"><a class="btn" href="${base}">Cancel</a><button class="btn primary" type="submit">Start discussion</button></div>
  </form>`;
  return {
    title: `New discussion · ${meta.owner}/${meta.name}`,
    html: repoPage(meta, 'discussions', body),
    mount(el) {
      mountRepoHeader(el, meta);
      mountMdEditor(el, { owner: meta.owner, repo: meta.name, rev: meta.default_branch, dir: '' });
      const form = $('#new-discussion', el);
      form.addEventListener('submit', action(form.querySelector('[type=submit]'), async () => {
        const r = await send('POST', `${repoUrl(meta.owner, meta.name)}/discussions`, { title: form.title.value.trim(), body: $('#d-body', form).value, category: form.category.value });
        clearDraft('d-body');
        navigate(`${base}/${r.number}`, { replace: true });
      }));
    },
  };
}

// ---------------------------------------------------------------- thread

function voteBtn(votes, voted, target) {
  return html`<button class="btn sm vote-btn ${voted ? 'on' : ''}" data-vote="${target}" aria-pressed="${voted}" title="Upvote">${icon('chev-up', 'sm')}<span>${num(votes)}</span></button>`;
}

function card({ id, author, time, body, mdCtx, vote, tag = '', answer = false, menu = true }) {
  return html`<div class="tl-item tl-comment" id="${id}">
    <a href="/${author}" class="tl-avatar hide-sm">${avatar(author, 36)}</a>
    <div class="comment-card ${answer ? 'answer' : ''}">
      <div class="comment-head">
        <a href="/${author}" class="row"><b>${author}</b></a>
        <span class="muted small"><a href="#${id}">${timeEl(time)}</a></span>
        ${tag ? html`<span class="tag-private">${tag}</span>` : ''}
        ${answer ? html`<span class="pill answer-tag">${icon('check', 'sm')} Answer</span>` : ''}
        <span class="grow"></span>${vote}
        ${menu ? html`<button class="btn ghost sm icon" data-menu="${id}" aria-label="More actions">${icon('dots', 'sm')}</button>` : ''}
      </div>
      <div class="comment-body markdown-body" data-body-of="${id}">${body.trim() ? raw(renderMarkdown(body, mdCtx)) : html`<p class="muted"><em>No description provided.</em></p>`}</div>
    </div>
  </div>`;
}

async function showView(ctx) {
  const { owner, repo, n } = ctx.params;
  if (!/^\d+$/.test(n)) return { redirect: `/${owner}/${repo}/discussions` };
  const api = `${repoUrl(owner, repo)}/discussions/${n}`;
  const [meta, data] = await Promise.all([loadRepo(owner, repo), get(api)]);
  const d = data.discussion;
  const mdCtx = { owner, repo, rev: meta.default_branch, dir: '' };
  await preloadLangs([d.body, ...data.comments.map((c) => c.body)].join('\n'));
  const answer = data.comments.find((c) => c.is_answer);
  const items = [
    card({ id: 'discussion-body', author: d.author, time: d.created_at, body: d.body, mdCtx, vote: voteBtn(d.votes, d.voted, 'thread'), tag: 'Author', menu: data.signed_in }),
    ...data.comments.map((c) => card({ id: `reply-${c.id}`, author: c.author, time: c.created_at, body: c.body, mdCtx, vote: voteBtn(c.votes, c.voted, c.id),
      tag: c.author === d.author ? 'Author' : '', answer: c.is_answer, menu: data.signed_in })),
  ];
  const composer = data.signed_in
    ? html`<div class="tl-item tl-composer"><span class="tl-avatar hide-sm">${avatar(state.me.username, 36)}</span>
        <form class="comment-card composer" id="composer"><div class="comment-head"><b>Reply</b></div>
          <div class="composer-body">${mdEditor({ id: 'reply-body', placeholder: 'Write a reply', rows: 5 })}</div>
          <div class="composer-actions"><button class="btn primary" type="submit">Reply</button></div></form></div>`
    : html`<div class="box signin-prompt"><div class="box-body row">${icon('comment')}<span class="grow">Sign in to join the discussion.</span><a class="btn primary" href="/login?next=${encodeURIComponent(location.pathname)}">Sign in</a></div></div>`;
  const body = html`<div class="issue-page">
    <header class="issue-head">
      <div class="issue-title-row"><h1><span class="issue-title-text">${d.title}</span> <span class="issue-num">#${d.number}</span></h1>
        ${data.can_write ? html`<button class="btn sm danger" id="delete-discussion">${icon('trash', 'sm')} Delete</button>` : ''}</div>
      <div class="row wrap-ok issue-meta">${catPill(d.category)}<span class="muted"><b>${d.author}</b> started this discussion ${timeEl(d.created_at)} · ${plural(data.comments.length, 'reply', 'replies')}</span></div>
    </header>
    ${answer ? html`<div class="box box-body row small answer-box">${icon('check', 'sm c-green')}<span class="grow">Answered by <b>${answer.author}</b></span><a class="btn sm" href="#reply-${answer.id}">View answer</a></div>` : ''}
    <div class="timeline">${items}${composer}</div>
  </div>`;
  return {
    title: `${d.title} · Discussion #${d.number} · ${owner}/${repo}`,
    html: repoPage(meta, 'discussions', body),
    mount(el) {
      mountRepoHeader(el, meta);
      mountMdEditor(el, mdCtx);
      const form = $('#composer', el);
      form?.addEventListener('submit', action(form.querySelector('[type=submit]'), async () => {
        const text = $('#reply-body', form).value.trim();
        if (!text) { toast('Write a reply first', 'err'); return; }
        await send('POST', `${api}/comments`, { body: text });
        clearDraft('reply-body');
        await refresh();
      }));
      const del = $('#delete-discussion', el);
      del?.addEventListener('click', action(del, async () => {
        if (!(await confirmDialog({ title: 'Delete this discussion?', text: 'It and all its replies are deleted. This can’t be undone.', ok: 'Delete', danger: true }))) return;
        await send('DELETE', api);
        toast('Discussion deleted');
        navigate(`/${owner}/${repo}/discussions`, { replace: true });
      }));
      el.querySelector('.timeline').addEventListener('click', async (e) => {
        const v = e.target.closest('[data-vote]');
        if (v) {
          if (!state.me) return navigate('/login?next=' + encodeURIComponent(location.pathname));
          const up = !v.classList.contains('on');
          const target = v.dataset.vote;
          try {
            const r = await send('POST', `${api}/vote`, { up, comment_id: target === 'thread' ? undefined : +target });
            v.classList.toggle('on', r.voted);
            v.setAttribute('aria-pressed', r.voted);
            v.querySelector('span').textContent = num(r.votes);
          } catch (err) { toastErr(err); }
          return;
        }
        const m = e.target.closest('[data-menu]');
        if (m) openMenu(el, m, api, data, mdCtx);
      });
    },
  };
}

function openMenu(el, m, api, data, mdCtx) {
  const id = m.dataset.menu;
  const isBody = id === 'discussion-body';
  const c = isBody ? null : data.comments.find((x) => `reply-${x.id}` === id);
  const canEdit = isBody ? data.can_edit : c?.can_edit;
  popover(m, html`
    <button class="menu-item" data-act="link">${icon('link', 'sm')} Copy link</button>
    ${c && data.can_answer ? html`<button class="menu-item" data-act="answer">${icon('check', 'sm')} ${c.is_answer ? 'Unmark as answer' : 'Mark as answer'}</button>` : ''}
    ${canEdit ? html`<div class="menu-sep"></div><button class="menu-item" data-act="edit">${icon('edit', 'sm')} Edit</button>` : ''}
    ${canEdit && c ? html`<button class="menu-item danger-item" data-act="delete">${icon('trash', 'sm')} Delete</button>` : ''}`,
  {
    align: 'right', width: 210,
    onMount(p) {
      p.addEventListener('click', action(null, async (ev) => {
        const act = ev.target.closest('[data-act]')?.dataset.act;
        if (!act) return;
        closePop();
        if (act === 'link') navigator.clipboard?.writeText(`${location.origin}${location.pathname}#${id}`).then(() => toast('Link copied'));
        if (act === 'answer') { await send('POST', `${api}/answer`, { comment_id: c.is_answer ? null : c.id }); refresh(); }
        if (act === 'delete') {
          if (!(await confirmDialog({ title: 'Delete this reply?', text: 'This can’t be undone.', ok: 'Delete', danger: true }))) return;
          await send('DELETE', `${api}/comments/${c.id}`);
          toast('Reply deleted');
          refresh();
        }
        if (act === 'edit') startEdit(el, api, id, isBody ? data.discussion : c, isBody, mdCtx);
      }));
    },
  });
}

function startEdit(el, api, id, item, isBody, mdCtx) {
  const bodyEl = el.querySelector(`[data-body-of="${id}"]`);
  if (!bodyEl || bodyEl.querySelector('form')) return;
  const prev = bodyEl.innerHTML;
  bodyEl.innerHTML = html`<form class="edit-form stack">
    ${isBody ? html`<input class="input" name="title" value="${item.title}" maxlength="256" aria-label="Title" required>` : ''}
    ${mdEditor({ id: `edit-${id}`, value: item.body, rows: 6, draft: false })}
    <div class="row" style="justify-content:flex-end"><button class="btn" type="button" data-cancel>Cancel</button><button class="btn primary" type="submit">Save</button></div></form>`.s;
  mountMdEditor(bodyEl, mdCtx);
  const form = bodyEl.querySelector('form');
  form.querySelector('[data-cancel]').addEventListener('click', () => { bodyEl.innerHTML = prev; });
  form.addEventListener('submit', action(form.querySelector('[type=submit]'), async () => {
    const text = form.querySelector('textarea').value;
    if (isBody) await send('PATCH', api, { title: form.title.value.trim(), body: text });
    else await send('PATCH', `${api}/comments/${item.id}`, { body: text });
    toast('Saved');
    refresh();
  }));
}
