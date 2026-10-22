// Releases: the list (notes, files and source code per release), one release,
// and the form that drafts, edits or publishes one and attaches files.
import { html, raw, get, send, busy, timeEl, avatar, bytes, plural, short, enc, encPath, $, toast, toastErr, confirmDialog, invalidate, action, ApiError } from '../lib.js';
import { icon } from '../icons.js';
import { loadRepo, repoUrl, repoPage, mountRepoHeader } from '../repohead.js';
import { renderMarkdown, preloadLangs } from '../md.js';
import { mdEditor, mountMdEditor } from '../mdeditor.js';
import { navigate } from '../main.js';

const api = (owner, repo) => `${repoUrl(owner, repo)}/releases`;

function dataUrl(ctx) {
  const { owner, repo, view, id } = ctx.params;
  if (view === 'list') return `${api(owner, repo)}?page=${Math.max(1, +ctx.query.get('page') || 1)}`;
  return id ? `${api(owner, repo)}/${enc(id)}` : null;
}

export function prefetch(ctx) {
  get(repoUrl(ctx.params.owner, ctx.params.repo));
  const u = dataUrl(ctx);
  if (u) get(u);
}

export default async function (ctx) {
  const { owner, repo, view } = ctx.params;
  const u = dataUrl(ctx);
  const [meta, data] = await Promise.all([loadRepo(owner, repo), u ? get(u) : null]);
  if (view === 'edit') return editView(meta, data);
  const list = view === 'list' ? data.releases : [data];
  await Promise.all(list.map((r) => preloadLangs(r.notes)));
  return view === 'list' ? listView(meta, data) : showView(meta, data);
}

function card(meta, r, { latest = false, link = true } = {}) {
  const base = `/${meta.owner}/${meta.name}`;
  const source = (ext) => `/archive/${meta.owner}/${meta.name}/${encPath(r.tag)}.${ext}`;
  return html`<article class="box" id="release-${r.id}">
    <div class="box-head row wrap-ok">
      <h2 class="grow" style="font-size:18px">${link ? html`<a href="${base}/releases/${r.id}">${r.title}</a>` : r.title}</h2>
      ${r.draft ? html`<span class="pill">Draft</span>` : ''}${r.prerelease ? html`<span class="pill c-amber">Pre-release</span>` : ''}${latest ? html`<span class="pill open">Latest</span>` : ''}
      ${meta.can_write ? html`<a class="btn sm" href="${base}/releases/${r.id}/edit">${icon('edit', 'sm')} Edit</a>` : ''}
    </div>
    <div class="box-body stack">
      <div class="row wrap-ok muted small">${icon('tag', 'sm')}
        ${r.sha ? html`<a class="mono" href="${base}/tree/${encPath(r.tag)}">${r.tag}</a><a class="sha" href="${base}/commit/${r.sha}">${short(r.sha)}</a>`
          : html`<span class="mono">${r.tag}</span><span>${r.draft ? `(made from ${r.target || meta.default_branch} when published)` : '(the tag was deleted)'}</span>`}
        ${r.author ? html`${avatar(r.author, 16)}<b>${r.author}</b>` : ''}
        <span>${r.draft ? html`drafted ${timeEl(r.created_at)}` : html`released ${timeEl(r.published_at)}`}</span></div>
      ${r.notes.trim() ? html`<div class="markdown-body">${raw(renderMarkdown(r.notes, { owner: meta.owner, repo: meta.name, rev: r.tag }))}</div>` : ''}
      <div><div class="section-title">${icon('download', 'sm')} Downloads</div>
        <div class="box list">
          ${r.assets.map((a) => html`<div class="list-row">${icon('file', 'sm muted')}
            <a class="grow ellipsis" href="/api${api(meta.owner, meta.name)}/${r.id}/assets/${a.id}" download>${a.name}</a>
            <span class="muted small nowrap">${bytes(a.size)}</span><span class="muted small nowrap hide-sm">${plural(a.downloads, 'download')}</span></div>`)}
          ${r.sha ? html`<div class="list-row">${icon('file-code', 'sm muted')}<a class="grow" href="${source('zip')}" download>Source code (zip)</a></div>
            <div class="list-row">${icon('file-code', 'sm muted')}<a class="grow" href="${source('tar.gz')}" download>Source code (tar.gz)</a></div>` : ''}
          ${!r.sha && !r.assets.length ? html`<div class="list-row muted small">No files</div>` : ''}
        </div></div>
    </div></article>`;
}

function listView(meta, d) {
  const base = `/${meta.owner}/${meta.name}`;
  const canNew = d.can_write && !meta.empty;
  const body = html`<div class="stack">
    <div class="toolbar"><h1 class="page-h grow">${icon('tag')} Releases</h1>
      <a class="btn sm" href="${base}/tags">${icon('tag', 'sm')} Tags</a>
      ${canNew ? html`<a class="btn sm primary" href="${base}/releases/new">${icon('plus', 'sm')} Draft a new release</a>` : ''}</div>
    ${d.releases.length ? d.releases.map((r) => card(meta, r, { latest: r.id === d.latest_id }))
      : html`<div class="box empty">${icon('tag', 'big')}<h3>No releases yet</h3>
        <p>A release marks a version of the project, with notes on what changed and files to download, like installers or binaries.</p>
        ${canNew ? html`<a class="btn primary" href="${base}/releases/new">${icon('plus', 'sm')} Create the first release</a>` : ''}</div>`}
    ${d.page > 1 || d.has_more ? html`<div class="row" style="justify-content:center">
      ${d.page > 1 ? html`<a class="btn" href="${base}/releases?page=${d.page - 1}">Newer</a>` : ''}
      ${d.has_more ? html`<a class="btn" href="${base}/releases?page=${d.page + 1}">Older</a>` : ''}</div>` : ''}
  </div>`;
  return { title: `Releases · ${meta.owner}/${meta.name}`, html: repoPage(meta, 'releases', body), mount: (el) => mountRepoHeader(el, meta) };
}

function showView(meta, r) {
  const body = html`<div class="stack">
    <div class="toolbar"><a class="btn sm" href="/${meta.owner}/${meta.name}/releases">${icon('arrow-left', 'sm')} All releases</a></div>
    ${card(meta, r, { latest: r.latest, link: false })}</div>`;
  return { title: `${r.title} · Releases · ${meta.owner}/${meta.name}`, html: repoPage(meta, 'releases', body), mount: (el) => mountRepoHeader(el, meta) };
}

/** Send one file as the request body (it streams; nothing is read into memory). */
async function uploadFile(meta, id, file) {
  let res;
  try {
    res = await fetch(`/api${api(meta.owner, meta.name)}/${id}/assets?name=${enc(file.name)}`, {
      method: 'POST', body: file, credentials: 'same-origin',
      headers: { Accept: 'application/json', 'Content-Type': file.type || 'application/octet-stream' },
    });
  } catch { throw new ApiError(0, `Couldn't upload ${file.name}. Check your connection.`); }
  invalidate();
  if (!res.ok) {
    const d = await res.json().catch(() => null);
    throw new ApiError(res.status, `${file.name}: ${d?.error || `the upload failed (${res.status})`}`);
  }
}

const TAG_HINT = 'Pick an existing tag, or type a new one to create it at the branch you choose.';

/** `r` is the release being edited (null: a new one). */
function editView(meta, r) {
  const base = `/${meta.owner}/${meta.name}`;
  if (!meta.can_write || meta.empty) return { redirect: r ? `${base}/releases/${r.id}` : `${base}/releases` };
  const creating = !r;
  const draftish = creating || r.draft;
  const body = html`<form class="stack" id="rel-form" novalidate>
    <h1 class="page-h">${creating ? 'New release' : html`Edit ${r.title}`}</h1>
    ${creating ? html`<div class="row wrap-ok" style="align-items:flex-end">
        <div class="field grow"><label for="rel-tag">Tag</label>
          <input class="input mono" id="rel-tag" list="rel-tags" required maxlength="200" placeholder="v1.0.0" autocomplete="off" spellcheck="false" autofocus>
          <datalist id="rel-tags">${meta.tags.map((t) => html`<option value="${t}"></option>`)}</datalist></div>
        <div class="field" id="rel-target-f"><label for="rel-target">New tag at</label>
          <select class="select" id="rel-target">${meta.branches.map((b) => html`<option ${b === meta.default_branch ? 'selected' : ''}>${b}</option>`)}</select></div>
      </div>
      <p class="muted small" id="rel-tag-hint">${TAG_HINT}</p>`
    : html`<p class="row muted small">${icon('tag', 'sm')}<b class="mono">${r.tag}</b></p>`}
    <div class="field"><label for="rel-title">Title</label>
      <input class="input lg-input" id="rel-title" maxlength="256" value="${r?.title || ''}" placeholder="The tag, if you leave this empty" autocomplete="off"></div>
    <div class="field"><label for="rel-notes">Release notes</label>${mdEditor({ id: 'rel-notes', value: r?.notes || '', rows: 10, draft: false, placeholder: "What's new, what's fixed, how to upgrade" })}</div>
    <div class="field"><label for="rel-files">Files</label>
      ${r?.assets.length ? html`<div class="box list">${r.assets.map((a) => html`<div class="list-row" data-asset="${a.id}">${icon('file', 'sm muted')}
        <span class="grow ellipsis">${a.name}</span><span class="muted small nowrap">${bytes(a.size)}</span>
        <button class="btn sm icon danger" type="button" data-remove="${a.id}" data-name="${a.name}" aria-label="Remove ${a.name}" title="Remove">${icon('trash', 'sm')}</button></div>`)}</div>` : ''}
      <input type="file" id="rel-files" multiple>
      <span class="hint">Installers, binaries or archives. They upload when you save.</span>
      <span class="hint" id="rel-progress" aria-live="polite"></span></div>
    <label class="check"><input type="checkbox" id="rel-pre" ${r?.prerelease ? 'checked' : ''}>
      <span><b>This is a pre-release</b><span class="muted small" style="display:block">Not ready for everyone yet. It's never shown as the latest release.</span></span></label>
    <p class="field-err" id="rel-err" role="alert" hidden></p>
    <div class="row wrap-ok">
      ${creating ? '' : html`<button class="btn danger" type="button" id="rel-delete">${icon('trash', 'sm')} Delete release</button>`}<span class="grow"></span>
      <a class="btn" href="${creating ? `${base}/releases` : `${base}/releases/${r.id}`}">Cancel</a>
      ${draftish ? html`<button class="btn" type="submit" data-draft="1">Save draft</button>` : ''}
      <button class="btn primary" type="submit" data-draft="">${icon('check', 'sm')} ${draftish ? 'Publish release' : 'Update release'}</button></div>
  </form>`;
  return {
    title: `${creating ? 'New release' : `Edit ${r.title}`} · ${meta.owner}/${meta.name}`,
    html: repoPage(meta, 'releases', body),
    mount(el) {
      mountRepoHeader(el, meta);
      mountMdEditor(el, { owner: meta.owner, repo: meta.name });
      const form = $('#rel-form', el), err = $('#rel-err', el), progress = $('#rel-progress', el);
      const tag = $('#rel-tag', el);
      tag?.addEventListener('input', () => {
        const t = tag.value.trim(), known = meta.tags.includes(t);
        $('#rel-target-f', el).hidden = known;
        $('#rel-tag-hint', el).textContent = known ? 'This tag exists, so the release uses it as it is.' : t ? `A new tag ${t} will be made at the branch you choose when you publish.` : TAG_HINT;
      });
      let saving = false;
      form.addEventListener('submit', async (e) => {
        e.preventDefault();
        // Two submit buttons: one save at a time, whichever was pressed.
        if (saving) return;
        saving = true;
        const btn = e.submitter || form.querySelector('[type=submit]');
        busy(btn, true);
        err.hidden = true;
        const draft = btn.dataset.draft === '1';
        const req = { title: $('#rel-title', el).value.trim(), notes: $('#rel-notes', el).value, prerelease: $('#rel-pre', el).checked, draft };
        let id = r?.id, saved = false;
        try {
          if (creating) id = (await send('POST', api(meta.owner, meta.name), { ...req, tag: tag.value.trim(), target: $('#rel-target', el).value })).id;
          else await send('PATCH', `${api(meta.owner, meta.name)}/${id}`, req);
          saved = true;
          for (const f of $('#rel-files', el).files) {
            progress.textContent = `Uploading ${f.name} (${bytes(f.size)})…`;
            await uploadFile(meta, id, f);
          }
          toast(draft ? 'Draft saved' : creating || r.draft ? 'Release published' : 'Release updated');
          navigate(`${base}/releases/${id}`, { replace: true });
        } catch (x) {
          // The release is saved; show what made it, so the rest can be added again.
          if (saved) { toastErr(x); navigate(`${base}/releases/${id}/edit`, { replace: true }); }
          else { err.textContent = x.message; err.hidden = false; }
        } finally { busy(btn, false); saving = false; }
      });
      el.addEventListener('click', async (e) => {
        const b = e.target.closest('[data-remove]');
        if (!b) return;
        const ok = await confirmDialog({ title: `Remove ${b.dataset.name}?`, text: 'It will no longer be downloadable from this release.', ok: 'Remove file', danger: true });
        if (!ok) return;
        await action(b, async () => {
          await send('DELETE', `${api(meta.owner, meta.name)}/${r.id}/assets/${b.dataset.remove}`);
          b.closest('[data-asset]').remove();
          toast('File removed');
        })();
      });
      const del = $('#rel-delete', el);
      del?.addEventListener('click', action(del, async () => {
        const ok = await confirmDialog({ title: `Delete ${r.title}?`, text: `Its files are deleted too. The tag ${r.tag} stays.`, ok: 'Delete release', danger: true });
        if (!ok) return;
        await send('DELETE', `${api(meta.owner, meta.name)}/${r.id}`);
        toast('Release deleted');
        navigate(`${base}/releases`, { replace: true });
      }));
    },
  };
}
