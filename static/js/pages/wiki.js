// Repository wiki: a page (Home by default) with the page list, the editor, and a page's history.
import { html, raw, get, send, action, timeEl, avatar, short, enc, encPath, $, toast, confirmDialog } from '../lib.js';
import { icon } from '../icons.js';
import { loadRepo, repoUrl, repoPage, mountRepoHeader } from '../repohead.js';
import { renderMarkdown, preloadLangs } from '../md.js';
import { mdEditor, mountMdEditor } from '../mdeditor.js';
import { navigate } from '../main.js';

const api = (owner, repo, page) => `${repoUrl(owner, repo)}/wiki/${enc(page)}`;
const pageName = (ctx) => ctx.params.page || 'Home';

function dataUrl(ctx) {
  const { owner, repo, view } = ctx.params;
  if (ctx.params.create) return `${repoUrl(owner, repo)}/wiki`;
  const page = api(owner, repo, pageName(ctx));
  if (view === 'history') return `${page}/history?page=${Math.max(1, +ctx.query.get('page') || 1)}`;
  const rev = view === 'page' && ctx.query.get('rev');
  return page + (rev ? `?rev=${enc(rev)}` : '');
}

export function prefetch(ctx) {
  get(repoUrl(ctx.params.owner, ctx.params.repo));
  get(dataUrl(ctx));
}

export default async function (ctx) {
  const { owner, repo, view } = ctx.params;
  const [meta, data] = await Promise.all([loadRepo(owner, repo), get(dataUrl(ctx))]);
  const page = pageName(ctx);
  if (view === 'edit') return editView(meta, data.page || null, ctx.params.create ? '' : page, data);
  if (view === 'history') return historyView(meta, data);
  if (data.page) await preloadLangs(data.page.content);
  return pageView(meta, page, data);
}

function sidebar(meta, data, current) {
  const base = `/${meta.owner}/${meta.name}/wiki`;
  return html`<aside class="stack">
    <div class="box"><div class="box-head row"><b>Pages</b><span class="counter">${data.pages.length}</span></div>
      ${data.pages.length ? html`<div class="list wiki-pages">${data.pages.map((p) => html`<a class="list-row ${p.name === current ? 'on' : ''}" href="${base}/${encPath(p.name)}">${icon('file', 'sm muted')}<span class="ellipsis">${p.title}</span></a>`)}</div>`
        : html`<div class="box-body muted small">No pages yet.</div>`}
    </div>
    ${data.can_write ? html`<a class="btn" href="${base}/_new">${icon('plus', 'sm')} New page</a>` : ''}
  </aside>`;
}

function pageView(meta, name, data) {
  const base = `/${meta.owner}/${meta.name}/wiki`;
  const p = data.page;
  let main;
  if (!p) {
    const first = !data.pages.length;
    main = html`<div class="box empty">${icon('book', 'big')}
      <h3>${first ? 'Welcome to the wiki' : `There's no ${name.replace(/-/g, ' ')} page yet`}</h3>
      <p>${first ? 'Wikis hold guides, how-tos and anything else worth writing down about this project.' : 'Pages are written in Markdown and keep their full history.'}</p>
      ${data.can_write ? html`<a class="btn primary" href="${base}/${encPath(name)}/edit">${icon('plus', 'sm')} ${first ? 'Create the first page' : 'Create this page'}</a>` : ''}
    </div>`;
  } else {
    const c = p.last_commit;
    main = html`<div class="stack">
      ${p.latest ? '' : html`<div class="box box-body row small">${icon('history', 'sm')}<span class="grow">You're looking at an older version of this page${c ? html` (${short(c.sha)})` : ''}.</span><a class="btn sm" href="${base}/${encPath(p.name)}">See the latest</a></div>`}
      <div class="toolbar"><h1 class="page-h grow">${p.title}</h1>
        <a class="btn sm" href="${base}/${encPath(p.name)}/history">${icon('history', 'sm')} History</a>
        ${data.can_write && p.latest ? html`<a class="btn sm primary" href="${base}/${encPath(p.name)}/edit">${icon('edit', 'sm')} Edit</a>` : ''}</div>
      ${c ? html`<div class="muted small">${avatar(c.author_name, 16)} <b>${c.author_name}</b> edited this page ${timeEl(c.commit_time)}</div>` : ''}
      <div class="box"><div class="markdown-body box-body">${raw(renderMarkdown(p.content, { owner: meta.owner, repo: meta.name, wiki: true }))}</div></div>
    </div>`;
  }
  return {
    title: `${p ? p.title : name} · Wiki · ${meta.owner}/${meta.name}`,
    html: repoPage(meta, 'wiki', html`<div class="split">${main}${sidebar(meta, data, p?.name)}</div>`),
    mount: (el) => mountRepoHeader(el, meta),
  };
}

/** `p` is the page being edited (null: a new one, named `name` if given). */
function editView(meta, p, name, data) {
  const base = `/${meta.owner}/${meta.name}/wiki`;
  if (!data.can_write) return { redirect: name ? `${base}/${encPath(name)}` : base };
  const creating = !p;
  const body = html`<form class="stack" id="wiki-form">
    <h1 class="page-h">${creating ? 'New page' : html`Editing ${p.title}`}</h1>
    <div class="field"><label for="wiki-name">Title</label>
      <input class="input lg-input" id="wiki-name" name="name" required maxlength="100" value="${creating ? name.replace(/-/g, ' ') : p.title}" placeholder="Getting started" autocomplete="off" ${creating ? 'autofocus' : ''}></div>
    <div class="field"><label for="wiki-content">Content</label>${mdEditor({ id: 'wiki-content', value: p?.content || '', rows: 18, draft: false, placeholder: 'Write in Markdown. Link other pages with [their title](Page-Name).' })}</div>
    <div class="field"><label for="wiki-msg">Edit message <span class="muted">(optional)</span></label>
      <input class="input" id="wiki-msg" name="message" maxlength="500" placeholder="${creating ? 'Create' : 'Update'} the page" autocomplete="off"></div>
    <div class="row"><span class="grow"></span>
      ${creating ? '' : html`<button class="btn danger" type="button" id="wiki-delete">${icon('trash', 'sm')} Delete page</button>`}
      <a class="btn" href="${creating ? base : `${base}/${encPath(p.name)}`}">Cancel</a>
      <button class="btn primary" type="submit">${icon('check', 'sm')} Save page</button></div>
  </form>`;
  return {
    title: `${creating ? 'New page' : `Edit ${p.title}`} · Wiki · ${meta.owner}/${meta.name}`,
    html: repoPage(meta, 'wiki', body),
    mount(el) {
      mountRepoHeader(el, meta);
      mountMdEditor(el, { owner: meta.owner, repo: meta.name, wiki: true });
      const form = $('#wiki-form', el);
      const btn = form.querySelector('[type=submit]');
      form.addEventListener('submit', action(btn, async () => {
        const req = { name: form.name.value, content: $('#wiki-content', form).value, message: form.message.value };
        const r = creating
          ? await send('POST', `${repoUrl(meta.owner, meta.name)}/wiki`, req)
          : await send('PUT', api(meta.owner, meta.name, p.name), { ...req, expected_oid: p.oid });
        toast('Page saved');
        navigate(`${base}/${encPath(r.name)}`, { replace: true });
      }));
      const del = $('#wiki-delete', el);
      del?.addEventListener('click', action(del, async () => {
        const ok = await confirmDialog({ title: `Delete ${p.title}?`, text: 'The page leaves the wiki; its history stays in the wiki repository.', ok: 'Delete page', danger: true });
        if (!ok) return;
        await send('DELETE', api(meta.owner, meta.name, p.name));
        toast('Page deleted');
        navigate(base, { replace: true });
      }));
    },
  };
}

function historyView(meta, h) {
  const base = `/${meta.owner}/${meta.name}/wiki/${encPath(h.name)}`;
  const body = html`<div class="stack">
    <div class="toolbar"><h1 class="page-h grow">${icon('history')} History of <a href="${base}">${h.title}</a></h1></div>
    <div class="box"><div class="list">${h.commits.map((c, i) => html`<div class="list-row">
      ${avatar(c.author_name, 22)}
      <div class="grow" style="min-width:0"><a class="issue-title ellipsis" style="display:block" href="${base}?rev=${c.sha}">${c.subject}</a>
        <div class="muted small"><b>${c.author_name}</b> ${i === h.commits.length - 1 && !h.has_more ? 'created' : 'edited'} ${timeEl(c.commit_time)}</div></div>
      <a class="sha" href="${base}?rev=${c.sha}">${short(c.sha)}</a></div>`)}</div></div>
    ${h.page > 1 || h.has_more ? html`<div class="row" style="justify-content:center">
      ${h.page > 1 ? html`<a class="btn" href="${base}/history?page=${h.page - 1}">Newer</a>` : ''}
      ${h.has_more ? html`<a class="btn" href="${base}/history?page=${h.page + 1}">Older</a>` : ''}</div>` : ''}
  </div>`;
  return {
    title: `History of ${h.title} · Wiki · ${meta.owner}/${meta.name}`,
    html: repoPage(meta, 'wiki', body),
    mount: (el) => mountRepoHeader(el, meta),
  };
}
