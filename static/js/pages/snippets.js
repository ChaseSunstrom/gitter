// Snippets: the public list, your own list, the editor, one snippet, and its revisions.
import { html, raw, state, get, send, action, timeEl, avatar, short, plural, bytes, langOf, enc, $, $$, copy, toast, confirmDialog } from '../lib.js';
import { icon } from '../icons.js';
import { codeBlock, highlightInto } from '../codeview.js';
import { scan, warning } from '../unicode.js';
import { navigate } from '../main.js';

const page = (ctx) => Math.max(1, +ctx.query.get('page') || 1);

function dataUrl(ctx) {
  const { view, id } = ctx.params;
  if (view === 'list') return `/snippets?page=${page(ctx)}`;
  if (view === 'mine') return state.me ? `/snippets?user=${enc(state.me.username)}&page=${page(ctx)}` : null;
  if (view === 'revisions') return `/snippets/${enc(id)}/revisions?page=${page(ctx)}`;
  if (id) return `/snippets/${enc(id)}${ctx.query.get('rev') ? `?rev=${enc(ctx.query.get('rev'))}` : ''}`;
  return null;
}

export function prefetch(ctx) {
  const u = dataUrl(ctx);
  if (u) get(u);
}

export default async function (ctx) {
  const { view, id } = ctx.params;
  if ((view === 'mine' || view === 'edit') && !state.me) return { redirect: '/login?next=' + encodeURIComponent(ctx.url.pathname) };
  const url = dataUrl(ctx);
  const data = url ? await get(url) : null;
  if (view === 'list' || view === 'mine') return listView(ctx, data, view === 'mine');
  if (view === 'edit') return editView(data);
  if (view === 'revisions') return revisionsView(id, data, await get(`/snippets/${enc(id)}`));
  return showView(data);
}

const name = (s) => s.title || s.files?.[0]?.name || s.files?.[0] || 'Untitled snippet';

function pager(base, d) {
  if (d.page <= 1 && !d.has_more) return '';
  return html`<div class="row" style="justify-content:center">
    ${d.page > 1 ? html`<a class="btn" href="${base}?page=${d.page - 1}">Newer</a>` : ''}
    ${d.has_more ? html`<a class="btn" href="${base}?page=${d.page + 1}">Older</a>` : ''}</div>`;
}

function listView(ctx, data, mine) {
  const rows = data.items.map((s, i) => html`<div class="list-row issue-row" style="--i:${Math.min(i, 20)}">
    ${avatar(s.owner, 28)}
    <div class="grow" style="min-width:0">
      <div class="issue-line"><a class="muted" href="/${s.owner}">${s.owner}</a><span class="muted">/</span><a class="issue-title" href="/snippets/${s.id}">${name(s)}</a>
        ${s.secret ? html`<span class="tag-private">${icon('lock', 'sm inline')} Secret</span>` : ''}</div>
      <div class="muted small">${icon('file-code', 'sm inline')} ${s.files.join(', ')} · updated ${timeEl(s.updated_at)}</div>
    </div></div>`);
  const body = html`<div class="wrap narrow"><div class="stack">
    <div class="toolbar"><h1 class="page-h grow">${icon('file-code')} ${mine ? 'Your snippets' : 'Snippets'}</h1>
      <div class="seg"><a href="/snippets" class="${mine ? '' : 'on'}">Everyone's</a>${state.me ? html`<a href="/snippets/mine" class="${mine ? 'on' : ''}">Yours</a>` : ''}</div>
      ${state.me ? html`<a class="btn primary" href="/snippets/new">${icon('plus', 'sm')} New snippet</a>` : ''}</div>
    <div class="box">${data.items.length ? html`<div class="list stagger">${rows}</div>` : html`<div class="empty">${icon('file-code', 'big')}
      <h3>${mine ? "You haven't made any snippets yet" : 'No snippets yet'}</h3>
      <p>Snippets are small sets of files to share: a script, a config, notes. Each edit is kept as a revision.</p>
      ${state.me ? html`<a class="btn primary" href="/snippets/new">Create a snippet</a>` : ''}</div>`}</div>
    ${pager(mine ? '/snippets/mine' : '/snippets', data)}
  </div></div>`;
  return { title: mine ? 'Your snippets' : 'Snippets', html: body };
}

const fileEditor = (f, i) => html`<div class="box snip-file" data-file>
  <div class="box-head row"><input class="input sm mono grow" name="fname" value="${f.name}" placeholder="snippetfile${i + 1}.txt" aria-label="File name" maxlength="100" autocomplete="off">
    <button class="btn ghost sm icon" type="button" data-remove title="Remove this file" aria-label="Remove file">${icon('trash', 'sm')}</button></div>
  <textarea class="textarea mono snip-text" name="content" rows="12" spellcheck="false" aria-label="File content" placeholder="Paste or type here">${f.content}</textarea>
</div>`;

/** New snippet (`s` null) or edit one. */
function editView(s) {
  if (s && !s.can_edit) return { redirect: `/snippets/${s.id}` };
  const files = s ? s.files : [{ name: '', content: '' }];
  const body = html`<div class="wrap narrow"><form class="stack" id="snip-form">
    <h1 class="page-h">${icon('file-code')} ${s ? 'Edit snippet' : 'New snippet'}</h1>
    <input class="input" name="title" value="${s?.title || ''}" maxlength="256" placeholder="What is it? (optional)" aria-label="Title" autocomplete="off">
    <div class="stack" id="snip-files">${files.map(fileEditor)}</div>
    <div class="row wrap-ok">
      <button class="btn" type="button" id="add-file">${icon('plus', 'sm')} Add file</button><span class="grow"></span>
      <label class="check small"><input type="radio" name="vis" value="public" ${s?.secret ? '' : 'checked'}> ${icon('globe', 'sm')} Public: listed for everyone</label>
      <label class="check small"><input type="radio" name="vis" value="secret" ${s?.secret ? 'checked' : ''}> ${icon('lock', 'sm')} Secret: only people with the link</label>
    </div>
    <div class="row" style="justify-content:flex-end"><a class="btn" href="${s ? `/snippets/${s.id}` : '/snippets'}">Cancel</a>
      <button class="btn primary" type="submit">${s ? 'Save changes' : 'Create snippet'}</button></div>
  </form></div>`;
  return {
    title: s ? `Edit ${name(s)}` : 'New snippet',
    html: body,
    mount(el) {
      const form = $('#snip-form', el);
      const list = $('#snip-files', el);
      $('#add-file', el).addEventListener('click', () => {
        const n = $$('[data-file]', list).length;
        list.insertAdjacentHTML('beforeend', fileEditor({ name: '', content: '' }, n).s);
        $$('[data-file]', list).at(-1).querySelector('textarea').focus();
      });
      list.addEventListener('click', (e) => {
        const rm = e.target.closest('[data-remove]');
        if (rm && $$('[data-file]', list).length > 1) rm.closest('[data-file]').remove();
      });
      // Tab indents instead of leaving the box.
      list.addEventListener('keydown', (e) => {
        if (e.key !== 'Tab' || e.shiftKey || !e.target.matches('textarea')) return;
        e.preventDefault();
        e.target.setRangeText('  ', e.target.selectionStart, e.target.selectionEnd, 'end');
      });
      form.addEventListener('submit', action(form.querySelector('[type=submit]'), async () => {
        const req = {
          title: form.title.value,
          secret: form.vis.value === 'secret',
          files: $$('[data-file]', list).map((f) => ({ name: f.querySelector('[name=fname]').value, content: f.querySelector('textarea').value })),
        };
        const r = s ? await send('PUT', `/snippets/${s.id}`, { ...req, expected_sha: s.sha }) : await send('POST', '/snippets', req);
        toast(s ? 'Snippet saved' : 'Snippet created');
        navigate(`/snippets/${s ? s.id : r.id}`, { replace: true });
      }));
    },
  };
}

function showView(s) {
  const at = s.latest ? 'latest' : s.sha;
  const body = html`<div class="wrap narrow"><div class="stack">
    <div class="toolbar">
      ${avatar(s.owner, 28)}<h1 class="page-h grow"><a href="/${s.owner}" class="muted">${s.owner}</a><span class="muted">/</span>${name(s)}</h1>
      ${s.secret ? html`<span class="tag-private" title="Only people with the link can see it">${icon('lock', 'sm inline')} Secret</span>` : ''}
      <a class="btn sm" href="/snippets/${s.id}/revisions">${icon('history', 'sm')} ${plural(s.revisions, 'revision')}</a>
      <button class="btn sm" data-copy="${location.origin}/snippets/${s.id}" data-copy-label="Link copied">${icon('link', 'sm')} Copy link</button>
      ${s.can_edit && s.latest ? html`<a class="btn sm" href="/snippets/${s.id}/edit">${icon('edit', 'sm')} Edit</a><button class="btn sm danger" id="snip-delete">${icon('trash', 'sm')} Delete</button>` : ''}
    </div>
    <div class="muted small">Created ${timeEl(s.created_at)} · updated ${timeEl(s.updated_at)}</div>
    ${s.latest ? '' : html`<div class="box box-body row small">${icon('history', 'sm')}<span class="grow">You're looking at revision ${short(s.sha)}.</span><a class="btn sm" href="/snippets/${s.id}">See the latest</a></div>`}
    ${s.files.map((f, i) => html`<div class="box file-box">
      <div class="box-head file-head row">${icon('file-code', 'sm')}<b class="grow ellipsis">${f.name}</b><span class="muted small">${bytes(f.size)}${f.language ? ` · ${f.language}` : ''}</span>
        <a class="btn sm" href="/raw/snippets/${s.id}/${at}/${enc(f.name)}" target="_blank" rel="noopener">Raw</a>
        <button class="btn sm icon" data-copy-file="${i}" title="Copy" aria-label="Copy ${f.name}">${icon('copy', 'sm')}</button></div>
      ${raw(warning(scan(f.content)))}${codeBlock(f.content, { id: `code-${i}` })}</div>`)}
  </div></div>`;
  return {
    title: `${name(s)} · ${s.owner}`,
    html: body,
    mount(el) {
      s.files.forEach((f, i) => highlightInto($(`#code-${i}`, el), f.content, langOf(f.name)));
      el.addEventListener('click', (e) => {
        const b = e.target.closest('[data-copy-file]');
        if (b) copy(s.files[+b.dataset.copyFile].content, 'Copied');
      });
      const del = $('#snip-delete', el);
      del?.addEventListener('click', action(del, async () => {
        if (!(await confirmDialog({ title: 'Delete this snippet?', text: 'It and all its revisions are deleted. This can’t be undone.', ok: 'Delete', danger: true }))) return;
        await send('DELETE', `/snippets/${s.id}`);
        toast('Snippet deleted');
        navigate('/snippets/mine', { replace: true });
      }));
    },
  };
}

function revisionsView(id, r, s) {
  const body = html`<div class="wrap narrow"><div class="stack">
    <div class="toolbar"><h1 class="page-h grow">${icon('history')} Revisions of <a href="/snippets/${id}">${name(s)}</a></h1></div>
    <div class="box"><div class="list">${r.commits.map((c, i) => html`<div class="list-row">
      ${avatar(c.author_name, 22)}<div class="grow" style="min-width:0"><a class="issue-title" href="/snippets/${id}?rev=${c.sha}">${c.subject}</a>
        <div class="muted small"><b>${c.author_name}</b> ${timeEl(c.commit_time)}${i === 0 && r.page === 1 ? ' · latest' : ''}</div></div>
      <a class="sha" href="/snippets/${id}?rev=${c.sha}">${short(c.sha)}</a></div>`)}</div></div>
    ${pager(`/snippets/${id}/revisions`, r)}
  </div></div>`;
  return { title: `Revisions · ${name(s)}`, html: body };
}
