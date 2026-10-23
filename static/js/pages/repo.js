// Repository code views: file tree (with README), single file, and blame.
import { html, raw, state, get, invalidate, copy, timeEl, avatar, bytes, plural, short, encPath, enc, esc, num, $, popover, closePop, safeUrl, langOf, send, toast, confirmDialog, action, sigBadge } from '../lib.js';
import { icon } from '../icons.js';
import { loadRepo, repoUrl, splitRev, repoPage, mountRepoHeader, refPicker, mountRefPicker, crumbs } from '../repohead.js';
import { codeBlock, highlightInto, mountLineSelection } from '../codeview.js';
import { scan, warning, revealHidden } from '../unicode.js';
import { renderMarkdown, preloadLangs } from '../md.js';
import { navigate, refresh } from '../main.js';

function target(ctx, meta) {
  const { rev, path } = splitRev(meta, ctx.params.rest || '');
  return { rev, path };
}

export async function prefetch(ctx) {
  const { owner, repo, view } = ctx.params;
  const meta = await get(repoUrl(owner, repo));
  const { rev, path } = target(ctx, meta);
  const ep = view === 'blob' ? 'blob' : view === 'blame' ? 'blame' : 'tree';
  get(`${repoUrl(owner, repo)}/${ep}?rev=${enc(rev)}&path=${enc(path)}`);
}

export default async function (ctx) {
  const { owner, repo, view } = ctx.params;
  // The root page needs no rev, so fetch it alongside the repo metadata.
  const rootTree = !ctx.params.rest && view === 'tree' ? get(`${repoUrl(owner, repo)}/tree`) : null;
  const meta = await loadRepo(owner, repo);
  const { rev, path } = target(ctx, meta);
  state.rev = rev;
  state.revPath = path;
  if (view === 'blob') return blobView(meta, rev, path);
  if (view === 'blame') return blameView(meta, rev, path);
  return treeView(meta, rev, path, rootTree);
}

// ---------------------------------------------------------------- tree

function lastCommitBar(meta, c, count, path) {
  if (!c) return '';
  const base = `/${meta.owner}/${meta.name}`;
  return html`<div class="box-head commit-bar">
    ${avatar(c.author_email || c.author_name, 22)}
    <b class="nowrap">${c.author_name}</b>
    <a class="grow ellipsis muted commit-msg" href="${base}/commit/${c.sha}" title="${c.subject}">${c.subject}</a>
    <span class="ci-slot" data-ci-sha="${c.sha}"></span>
    <a class="sha hide-sm" href="${base}/commit/${c.sha}">${short(c.sha)}</a>
    <span class="muted small nowrap hide-sm">${timeEl(c.commit_time)}</span>
    ${count != null ? html`<a class="btn ghost sm" href="${base}/commits/${encPath(state.rev)}${path ? '/' + encPath(path) : ''}">${icon('history', 'sm')}<b>${num(count)}</b><span class="hide-sm">${count === 1 ? 'commit' : 'commits'}</span></a>` : ''}
  </div>`;
}

function entryIcon(e) {
  if (e.kind === 'tree') return icon('folder', 'c-accent');
  if (e.kind === 'submodule') return icon('submodule', 'c-accent');
  if (e.kind === 'symlink') return icon('symlink', 'muted');
  return icon('file', 'muted');
}

function emptyRepo(meta) {
  const url = meta.clone_url;
  const base = `/${meta.owner}/${meta.name}`;
  if (meta.mirror) {
    const m = meta.mirror;
    return html`<div class="box empty-setup fade"><div class="box-body stack"><h2 style="font-size:18px" class="row">${m.error ? icon('x', 'c-red') : html`<span class="spinner"></span>`} ${m.once ? 'Copying' : 'Waiting for the first sync'}</h2>
      <p class="muted">${m.once ? html`This repository is being copied from <span class="mono">${m.url}</span>; when that's done it's an ordinary repository you can change.` : html`This repository mirrors <span class="mono">${m.url}</span>.`} Its branches and tags show up here once they're fetched; this page updates by itself.${meta.can_admin ? html` See how it's going in <a href="${base}/settings/mirrors">Settings → Mirrors</a>.` : ''}</p>
      ${m.error ? html`<pre class="setup-cmd small c-red">${m.error}</pre><p class="muted small">It tries again on its own; fix the URL or password in Settings → Mirrors, then press Sync now.</p>` : ''}</div></div>`;
  }
  return html`<div class="box empty-setup fade">
    <div class="box-body stack">
      <div><h2 style="font-size:18px">Let's get some code in here</h2><p class="muted">This repository is empty. Push an existing project or create the first file.</p></div>
      <div class="row wrap-ok">
        <input class="input mono" readonly value="${url}" style="max-width:520px" aria-label="Clone URL">
        <button class="btn" data-copy="${url}" data-copy-label="Clone URL copied">${icon('copy', 'sm')} Copy</button>
        ${meta.ssh_url ? html`<button class="btn" data-copy="${meta.ssh_url}" data-copy-label="SSH URL copied">${icon('copy', 'sm')} Copy SSH URL</button>` : ''}
        ${meta.can_write ? html`<a class="btn" href="${base}/new/main">${icon('file', 'sm')} Create a file</a><a class="btn" href="${base}/upload/main">${icon('upload', 'sm')} Upload files</a>` : ''}
      </div>
      <div class="setup-grid">
        <div><div class="section-title">${icon('terminal', 'sm')} Push an existing repository</div>
<pre class="setup-cmd">git remote add origin ${url}
git branch -M main
git push -u origin main</pre><button class="btn sm" data-copy="git remote add origin ${url}\ngit branch -M main\ngit push -u origin main">${icon('copy', 'sm')} Copy commands</button></div>
        <div><div class="section-title">${icon('plus', 'sm')} Start a new repository</div>
<pre class="setup-cmd">echo "# ${meta.name}" > README.md
git init -b main && git add README.md
git commit -m "First commit"
git remote add origin ${url}
git push -u origin main</pre><button class="btn sm" data-copy="echo &quot;# ${meta.name}&quot; > README.md\ngit init -b main && git add README.md\ngit commit -m &quot;First commit&quot;\ngit remote add origin ${url}\ngit push -u origin main">${icon('copy', 'sm')} Copy commands</button></div>
      </div>
      <p class="small muted">${icon('key', 'sm')} Git will ask for a username and password: use your Gitter password or an <a class="c-accent" href="/settings/tokens">access token</a>.</p>
    </div></div>`;
}

/** The latest release (never a draft or pre-release), or a nudge to make one. */
function releasesSide(meta) {
  const base = `/${meta.owner}/${meta.name}/releases`, l = meta.latest_release;
  if (!meta.releases && !meta.can_write) return '';
  return html`<div><div class="section-title"><a href="${base}">Releases</a>${meta.releases ? html`<span class="counter">${num(meta.releases)}</span>` : ''}</div>
    ${l ? html`<a class="row" href="${base}/${l.id}">${icon('tag', 'c-green')}<span class="grow" style="min-width:0"><b class="ellipsis" style="display:block">${l.title}</b>
        <span class="muted small">${l.tag} · ${timeEl(l.published_at)}</span></span><span class="pill open">Latest</span></a>`
      : meta.releases ? '' : html`<p class="muted small">No releases yet — <a class="c-accent" href="${base}/new">create one</a>.</p>`}</div>`;
}

function aboutSide(meta, langs) {
  return html`<aside class="repo-side stack">
    <div><div class="section-title">About</div>
      ${meta.description ? html`<p class="about-desc">${meta.description}</p>` : html`<p class="muted small">No description${meta.can_admin ? html` — <a class="c-accent" href="/${meta.owner}/${meta.name}/settings">add one</a>` : ''}.</p>`}
      ${meta.website ? html`<a class="row c-accent small" href="${safeUrl(meta.website)}" rel="noopener nofollow ugc" target="_blank" style="margin-top:8px">${icon('link', 'sm')}<span class="ellipsis">${meta.website.replace(/^https?:\/\//, '')}</span></a>` : ''}
      <div class="side-stats">
        <a href="/${meta.owner}/${meta.name}/insights">${icon('star', 'sm')} <b>${num(meta.stars)}</b> ${meta.stars === 1 ? 'star' : 'stars'}</a>
        <span>${icon('fork', 'sm')} <b>${num(meta.forks)}</b> ${meta.forks === 1 ? 'fork' : 'forks'}</span>
        <a href="/${meta.owner}/${meta.name}/branches">${icon('branch', 'sm')} <b>${num(meta.branches.length)}</b> ${meta.branches.length === 1 ? 'branch' : 'branches'}</a>
        <a href="/${meta.owner}/${meta.name}/tags">${icon('tag', 'sm')} <b>${num(meta.tags.length)}</b> ${meta.tags.length === 1 ? 'tag' : 'tags'}</a>
      </div>
    </div>
    ${releasesSide(meta)}
    ${langs.length ? html`<div><div class="section-title">Languages</div>
      <div class="lang-bar">${langs.map((l) => html`<span style="width:${Math.max(l.percent, 0.6)}%;background:${l.color}" title="${l.name} ${l.percent.toFixed(1)}%"></span>`)}</div>
      <ul class="lang-list">${langs.slice(0, 8).map((l) => html`<li><span class="dot" style="background:${l.color}"></span><b>${l.name}</b><span class="muted">${l.percent.toFixed(1)}%</span></li>`)}</ul></div>` : ''}
  </aside>`;
}

async function treeView(meta, rev, path, rootTree) {
  const base = `/${meta.owner}/${meta.name}`;
  if (meta.empty) {
    return {
      title: `${meta.owner}/${meta.name}`,
      html: repoPage(meta, 'code', emptyRepo(meta)),
      mount: (el) => {
        mountRepoHeader(el, meta);
        // A first sync is on its way: look again until it lands.
        if (meta.mirror) {
          const again = () => setTimeout(async () => {
            if (!el.isConnected) return;
            const now = await get(repoUrl(meta.owner, meta.name), { fresh: true }).catch(() => null);
            // A one-time copy is done once its mirror is gone (it has files a moment before that).
            if (now && ((!now.empty && !now.mirror?.once) || now.mirror?.error !== meta.mirror.error)) {
              // What was cached while it was empty (its file list) is stale now.
              invalidate(repoUrl(meta.owner, meta.name));
              refresh();
            } else again();
          }, 3000);
          again();
        }
      },
    };
  }
  const [data, langs] = await Promise.all([
    rootTree || get(`${repoUrl(meta.owner, meta.name)}/tree?rev=${enc(rev)}&path=${enc(path)}`),
    path ? Promise.resolve({ languages: [] }) : get(`${repoUrl(meta.owner, meta.name)}/languages`).catch(() => ({ languages: [] })),
  ]);
  if (data.redirect === 'blob') return { redirect: `${base}/blob/${encPath(rev)}/${encPath(path)}` };
  state.sha = data.sha;
  const parent = path.includes('/') ? path.slice(0, path.lastIndexOf('/')) : '';
  const rows = html`
    ${path ? html`<a class="list-row file-row" href="${base}/tree/${encPath(rev)}${parent ? '/' + encPath(parent) : ''}">${icon('folder', 'c-accent')}<span class="name">..</span></a>` : ''}
    ${data.entries.map((e, i) => {
      const href = e.kind === 'tree' ? `${base}/tree/${encPath(rev)}/${encPath(e.path)}` : e.kind === 'submodule' ? '#' : `${base}/blob/${encPath(rev)}/${encPath(e.path)}`;
      return html`<div class="list-row file-row" style="--i:${Math.min(i, 20)}">
        ${entryIcon(e)}<a class="name ellipsis" href="${href}">${e.name}</a>
        <span class="msg ellipsis muted hide-sm" data-entry="${e.name}"><span class="skel" style="display:inline-block;height:10px;width:${40 + ((i * 37) % 45)}%"></span></span>
        <span class="when muted small nowrap" data-when="${e.name}"></span></div>`;
    })}`;
  const readme = data.readme;
  const isMd = readme && /\.(md|markdown)$/i.test(readme.name);
  if (isMd) await preloadLangs(readme.content);
  const readmeDir = readme ? readme.path.split('/').slice(0, -1).join('/') : '';
  const readmeHtml = readme ? (isMd ? raw(renderMarkdown(readme.content, { owner: meta.owner, repo: meta.name, rev, dir: readmeDir })) : html`<pre class="plain-readme">${readme.content}</pre>`) : '';

  const toolbar = html`<div class="toolbar">
    ${refPicker(meta, rev)}
    ${path ? crumbs(meta, rev, path) : html`<a class="muted small row hide-sm" href="${base}/branches">${icon('branch', 'sm')}<b>${meta.branches.length}</b> branches</a><a class="muted small row hide-sm" href="${base}/tags">${icon('tag', 'sm')}<b>${meta.tags.length}</b> tags</a>`}
    <span class="grow"></span>
    <button class="btn sm" id="goto-file">${icon('search', 'sm')}<span class="hide-sm">Go to file</span><kbd class="hide-sm">t</kbd></button>
    ${meta.can_write ? html`<button class="btn sm" id="add-file">${icon('plus', 'sm')}<span class="hide-sm">Add file</span>${icon('chev-down', 'sm')}</button>` : ''}
  </div>`;

  const main = html`<div class="stack">
    ${toolbar}
    <div class="box files-box">
      ${lastCommitBar(meta, data.last_commit, data.commit_count, path)}
      <div class="list stagger">${rows}</div>
    </div>
    ${readme ? html`<div class="box readme" id="readme"><div class="box-head">${icon('book', 'sm')}<a href="${base}/blob/${encPath(rev)}/${encPath(readme.path)}"><b>${readme.name}</b></a>
      ${meta.can_write ? html`<span class="grow"></span><a class="btn ghost sm icon" href="${base}/edit/${encPath(rev)}/${encPath(readme.path)}" aria-label="Edit README">${icon('edit', 'sm')}</a>` : ''}</div>
      <div class="markdown-body box-body">${readmeHtml}</div></div>` : ''}
  </div>`;
  const body = path ? main : html`<div class="split">${main}${aboutSide(meta, langs.languages)}</div>`;

  return {
    title: path ? `${path} at ${rev} · ${meta.owner}/${meta.name}` : `${meta.owner}/${meta.name}${meta.description ? ': ' + meta.description : ''}`,
    html: repoPage(meta, 'code', body),
    mount(el) {
      mountRepoHeader(el, meta);
      mountRefPicker(el, meta, rev, (r) => `${base}/tree/${encPath(r)}${path ? '/' + encPath(path) : ''}`);
      $('#goto-file', el)?.addEventListener('click', async () => (await import('../palette.js')).openPalette('', 'files'));
      $('#add-file', el)?.addEventListener('click', (e) => {
        popover(e.currentTarget, html`<a class="menu-item" href="${base}/new/${encPath(rev)}${path ? '/' + encPath(path) : ''}">${icon('file', 'sm')} Create new file</a>
          <a class="menu-item" href="${base}/upload/${encPath(rev)}${path ? '/' + encPath(path) : ''}">${icon('upload', 'sm')} Upload files</a>`,
        { align: 'right', onMount: (p) => p.addEventListener('click', (ev) => ev.target.closest('a') && closePop()) });
      });
      // Last commit per entry arrives a moment later; fade it in.
      get(`${repoUrl(meta.owner, meta.name)}/tree-commits?rev=${enc(data.sha)}&path=${enc(path)}`).then((tc) => {
        el.querySelectorAll('[data-entry]').forEach((span) => {
          const c = tc.commits[span.dataset.entry];
          span.innerHTML = c ? html`<a href="${base}/commit/${c.sha}" title="${c.subject}">${c.subject}</a>`.s : '';
          span.classList.add('fade');
          const w = el.querySelector(`[data-when="${CSS.escape(span.dataset.entry)}"]`);
          if (w && c) { w.innerHTML = timeEl(c.time).s; w.classList.add('fade'); }
        });
      }).catch(() => el.querySelectorAll('[data-entry]').forEach((s) => { s.textContent = ''; }));
      setupDrop(el, meta, rev, path);
    },
  };
}

/** Drag files anywhere on a code page (the tree or a file) to start an upload commit into that folder. */
function setupDrop(el, meta, rev, path) {
  if (!meta.can_write || !meta.branches.includes(rev)) return;
  const box = el.querySelector('.files-box, .file-box');
  if (!box) return;
  let depth = 0;
  el.addEventListener('dragenter', (e) => { if (e.dataTransfer?.types?.includes('Files')) { depth++; box.classList.add('drop-on'); } });
  el.addEventListener('dragleave', () => { if (--depth <= 0) { depth = 0; box.classList.remove('drop-on'); } });
  el.addEventListener('dragover', (e) => { if (e.dataTransfer?.types?.includes('Files')) e.preventDefault(); });
  el.addEventListener('drop', async (e) => {
    if (!e.dataTransfer?.files?.length) return;
    e.preventDefault();
    box.classList.remove('drop-on');
    const mod = await import('./editor.js');
    mod.stashDrop(e.dataTransfer);
    navigate(`/${meta.owner}/${meta.name}/upload/${encPath(rev)}${path ? '/' + encPath(path) : ''}`);
  });
}

// ---------------------------------------------------------------- blob

function fileToolbar(meta, rev, path, d, mode) {
  const base = `/${meta.owner}/${meta.name}`;
  const raw = `/raw/${meta.owner}/${meta.name}/${encPath(rev)}/${encPath(path)}`;
  const isBranch = meta.branches.includes(rev);
  return html`<div class="box-head file-head">
    <span class="muted small nowrap">${d.content != null ? html`<b>${num(d.content.split('\n').length - (d.content.endsWith('\n') ? 1 : 0))}</b> lines · ` : ''}${bytes(d.lfs ? d.lfs.size : d.size)}${d.lfs ? ' · Git LFS' : ''}${d.language ? html` · ${d.language}` : ''}</span>
    ${d.preview ? html`<div class="seg" style="margin-left:8px"><a href="${base}/blob/${encPath(rev)}/${encPath(path)}" class="${mode === 'blob' && !d.showCode ? 'on' : ''}" data-mode="preview">Preview</a><a href="${base}/blob/${encPath(rev)}/${encPath(path)}?plain=1" class="${d.showCode ? 'on' : ''}" data-mode="code">Code</a></div>` : ''}
    ${mode === 'blame' ? html`<div class="seg" style="margin-left:8px"><a href="${base}/blob/${encPath(rev)}/${encPath(path)}">Code</a><a class="on">Blame</a></div>` : ''}
    <span class="grow"></span>
    <div class="btn-group">
      <a class="btn sm" href="${raw}" target="_blank" rel="noopener">Raw</a>
      ${mode !== 'blame' && !d.binary ? html`<a class="btn sm hide-sm" href="${base}/blame/${encPath(rev)}/${encPath(path)}">Blame</a>` : ''}
      <a class="btn sm hide-sm" href="${base}/history/${encPath(rev)}/${encPath(path)}">History</a>
    </div>
    ${d.content != null ? html`<button class="btn sm icon" id="copy-file" title="Copy file contents" aria-label="Copy file contents">${icon('copy', 'sm')}</button>` : ''}
    <a class="btn sm icon" href="${raw}" download="${path.split('/').pop()}" title="Download" aria-label="Download">${icon('download', 'sm')}</a>
    ${meta.can_write && isBranch && d.editable ? html`<a class="btn sm icon" href="${base}/edit/${encPath(rev)}/${encPath(path)}" title="Edit this file" aria-label="Edit">${icon('edit', 'sm')}</a>` : ''}
    ${meta.can_write && isBranch ? html`<button class="btn sm icon danger" id="delete-file" title="Delete this file" aria-label="Delete">${icon('trash', 'sm')}</button>` : ''}
  </div>`;
}

async function blobView(meta, rev, path) {
  const base = `/${meta.owner}/${meta.name}`;
  const d = await get(`${repoUrl(meta.owner, meta.name)}/blob?rev=${enc(rev)}&path=${enc(path)}`);
  if (d.redirect === 'tree') return { redirect: `${base}/tree/${encPath(rev)}/${encPath(path)}` };
  state.sha = d.sha;
  const raw_ = `/raw/${meta.owner}/${meta.name}/${encPath(d.sha)}/${encPath(path)}`;
  const isMd = /\.(md|markdown|mdx)$/i.test(path);
  const plain = new URLSearchParams(location.search).get('plain') === '1';
  // Rich viewers (tables, notebooks, 3D, PDF, media) come in only for files that have one.
  const V = /\.(csv|tsv|ipynb|stl|pdf|mp3|wav|ogg|oga|opus|flac|m4a|aac|mp4|m4v|webm|ogv|mov)$/i.test(path) ? await import('../viewers.js') : null;
  const kind = V?.kindOf(path);
  d.preview = isMd || /\.svg$/i.test(path) || (!!kind && d.content != null);
  d.showCode = plain && d.preview;
  const lang = langOf(path);
  const viewOpts = { path, content: d.content, raw: raw_, size: d.lfs ? d.lfs.size : d.size, md: { owner: meta.owner, repo: meta.name, rev, dir: path.split('/').slice(0, -1).join('/') } };
  let content, viewer = null;
  if (kind && !d.showCode && d.lfs?.stored !== false && (V.textual(kind) ? d.content != null : true)) {
    try { content = await V.render(kind, viewOpts); viewer = kind; } catch { /* doesn't parse: show it as code */ }
  }
  if (viewer) {
    // `content` is the viewer's
  } else if (d.image && !d.showCode && d.lfs?.stored !== false) {
    content = html`<div class="image-view"><img src="${raw_}" alt="${path}"></div>`;
  } else if (d.too_large) {
    content = html`<div class="empty">${icon('file', 'big')}<h3>This file is too big to show here</h3><a class="btn" href="${raw_}">View raw</a></div>`;
  } else if (d.lfs) {
    content = html`<div class="empty">${icon('file', 'big')}<h3>Stored with Git LFS</h3>${d.lfs.stored
      ? html`<a class="btn" href="${raw_}" download>${icon('download', 'sm')} Download (${bytes(d.lfs.size)})</a>`
      : html`<p class="muted">${bytes(d.lfs.size)} · not uploaded to this server</p>`}</div>`;
  } else if (d.binary) {
    content = html`<div class="empty">${icon('file', 'big')}<h3>Binary file not shown</h3><a class="btn" href="${raw_}" download>${icon('download', 'sm')} Download (${bytes(d.size)})</a></div>`;
  } else if (isMd && !plain) {
    await preloadLangs(d.content);
    content = html`<div class="markdown-body box-body">${raw(renderMarkdown(d.content, { owner: meta.owner, repo: meta.name, rev, dir: path.split('/').slice(0, -1).join('/') }))}</div>`;
  } else {
    content = html`${raw(warning(scan(d.content)))}${codeBlock(d.content)}`;
  }
  const body = html`<div class="stack">
    <div class="toolbar">${refPicker(meta, rev)}${crumbs(meta, rev, path)}</div>
    <div class="box">${lastCommitBar(meta, d.last_commit, null, path)}</div>
    <div class="box file-box">${fileToolbar(meta, rev, path, d, 'blob')}${content}</div>
  </div>`;
  return {
    title: `${path} at ${rev} · ${meta.owner}/${meta.name}`,
    html: repoPage(meta, 'code', body),
    mount(el) {
      mountRepoHeader(el, meta);
      mountRefPicker(el, meta, rev, (r) => `${base}/blob/${encPath(r)}/${encPath(path)}`);
      if (viewer) V.mount(viewer, el, viewOpts);
      setupDrop(el, meta, rev, viewOpts.md.dir);
      const code = el.querySelector('#code');
      mountLineSelection(code);
      if (code) highlightInto(code, d.content, lang);
      $('#copy-file', el)?.addEventListener('click', () => copy(d.content, 'File copied'));
      const del = $('#delete-file', el);
      del?.addEventListener('click', action(del, async () => {
        const ok = await confirmDialog({ title: `Delete ${path.split('/').pop()}?`, text: `This commits the deletion of ${path} to ${rev}.`, ok: 'Delete file', danger: true });
        if (!ok) return;
        await send('POST', `${repoUrl(meta.owner, meta.name)}/contents`, { branch: rev, expected_sha: d.sha, message: `Delete ${path}`, changes: [{ path, delete: true }] });
        toast('File deleted');
        const dir = path.split('/').slice(0, -1).join('/');
        navigate(`${base}/tree/${encPath(rev)}${dir ? '/' + encPath(dir) : ''}`);
      }));
    },
  };
}

// ---------------------------------------------------------------- blame

function heat(t, min, max) {
  const x = max > min ? (t - min) / (max - min) : 1;
  return `color-mix(in srgb, var(--accent) ${Math.round(15 + x * 85)}%, var(--line))`;
}

async function blameView(meta, rev, path) {
  const base = `/${meta.owner}/${meta.name}`;
  const d = await get(`${repoUrl(meta.owner, meta.name)}/blame?rev=${enc(rev)}&path=${enc(path)}`);
  state.sha = d.sha;
  const times = d.chunks.map((c) => c.time);
  const min = Math.min(...times), max = Math.max(...times);
  const text = d.chunks.flatMap((c) => c.lines).join('\n');
  const rows = d.chunks.map((c) => html`<div class="blame-chunk">
    <div class="blame-meta" style="--heat:${heat(c.time, min, max)}">
      <div class="row">${avatar(c.author, 16)}<a class="ellipsis grow" href="${base}/commit/${c.sha}" title="${c.summary}">${c.summary}</a></div>
      <div class="row muted tiny"><a class="sha" href="${base}/commit/${c.sha}">${short(c.sha)}</a>${sigBadge(c.verification)}<span>${c.author}</span><span class="grow"></span>${timeEl(c.time)}
      <a class="btn ghost sm icon" title="Blame before this change" href="${base}/blame/${c.sha}~1/${encPath(path)}" aria-label="Blame prior revision">${icon('history', 'sm')}</a></div>
    </div>
    <div class="code blame-code" style="--digits:${String(c.start + c.lines.length).length}">${raw(c.lines.map((l, i) => `<div class="cl" id="L${c.start + i}"><a class="ln" href="#L${c.start + i}" data-ln="${c.start + i}"></a><div class="lc">${revealHidden(esc(l)) || ' '}</div></div>`).join(''))}</div>
  </div>`);
  const body = html`<div class="stack">
    <div class="toolbar">${refPicker(meta, rev)}${crumbs(meta, rev, path)}</div>
    <div class="box file-box">${fileToolbar(meta, rev, path, { size: text.length, content: text, language: d.language }, 'blame')}
      <div class="blame">${rows}</div></div>
  </div>`;
  return {
    title: `Blame ${path} · ${meta.owner}/${meta.name}`,
    html: repoPage(meta, 'code', body),
    mount(el) {
      mountRepoHeader(el, meta);
      mountRefPicker(el, meta, rev, (r) => `${base}/blame/${encPath(r)}/${encPath(path)}`);
      mountLineSelection(el.querySelector('.blame'));
      $('#copy-file', el)?.addEventListener('click', () => copy(text, 'File copied'));
    },
  };
}
