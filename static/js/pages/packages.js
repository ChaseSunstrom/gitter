// Packages: the repository's container images (tags, sizes, the pull command)
// and generic package files, with how to publish them.
import { html, get, timeEl, bytes, encPath } from '../lib.js';
import { icon } from '../icons.js';
import { loadRepo, repoUrl, repoPage, mountRepoHeader } from '../repohead.js';

const copyBtn = (text, label) => html`<button class="btn ghost sm icon" data-copy="${text}" data-copy-label="${label}" aria-label="${label}">${icon('copy', 'sm')}</button>`;

function image(img) {
  return html`<article class="box">
    <div class="box-head row wrap-ok"><h2 class="grow mono" style="font-size:15px">${icon('package', 'sm')} ${img.name}</h2></div>
    <div class="box-body stack">
      <div class="row"><pre class="setup-cmd grow" style="margin:0">docker pull ${img.pull}</pre>${copyBtn(`docker pull ${img.pull}`, 'Pull command copied')}</div>
      <div class="box list">
        ${img.tags.map((t) => html`<div class="list-row">${icon('tag', 'sm muted')}
          <span class="mono grow">${t.tag}</span>
          <span class="mono muted small hide-sm" title="${t.digest}">${t.digest.slice(7, 19)}</span>
          <span class="muted small nowrap">${bytes(t.size)}</span>
          <span class="muted small nowrap hide-sm">${timeEl(t.updated_at)}</span></div>`)}
      </div>
    </div></article>`;
}

function pkg(meta, p) {
  const base = `/api${repoUrl(meta.owner, meta.name)}/packages/generic/${encPath(p.name)}/${encPath(p.version)}`;
  return html`<div class="list-row">${icon('package', 'sm muted')}
    <b>${p.name}</b><span class="mono muted">${p.version}</span>
    <span class="grow row wrap-ok">${p.files.map((f) => html`<a class="mono" href="${base}/${encPath(f.file)}" download>${f.file}</a><span class="muted small">${bytes(f.size)}</span>`)}</span>
  </div>`;
}

export default async function (ctx) {
  const { owner, repo } = ctx.params;
  const [meta, d] = await Promise.all([loadRepo(owner, repo), get(`${repoUrl(owner, repo)}/packages`)]);
  const host = d.registry;
  const how = html`<div class="box"><div class="box-head"><h2 style="font-size:15px">Publish an image</h2></div><div class="box-body stack">
    <p class="muted">Sign in with your username and password (or an access token), tag an image under this repository, and push it.
      ${meta.private ? 'Only people who can read this repository can pull it.' : 'Anyone can pull it; only people who can push to the repository can publish.'}</p>
    <pre class="setup-cmd" style="text-align:left">docker login ${host}
docker tag my-image ${d.image}:latest
docker push ${d.image}:latest</pre>
    <p class="muted">Generic packages: upload any file under a name and version.</p>
    <pre class="setup-cmd" style="text-align:left">curl -H "Authorization: Bearer $TOKEN" --upload-file ./tool.tar.gz \\
  ${location.origin}/api${repoUrl(meta.owner, meta.name)}/packages/generic/tool/1.0.0/tool.tar.gz</pre>
  </div></div>`;
  const empty = !d.images.length && !d.packages.length;
  const body = html`<div class="stack">
    <div class="toolbar"><h1 class="page-h grow">${icon('package')} Packages</h1></div>
    ${empty ? html`<div class="box empty">${icon('package', 'big')}<h3>No packages yet</h3>
      <p>Container images and package files published here live with the repository and follow who can see it.</p></div>` : ''}
    ${d.images.map(image)}
    ${d.packages.length ? html`<div><div class="section-title">${icon('download', 'sm')} Generic packages</div>
      <div class="box list">${d.packages.map((p) => pkg(meta, p))}</div></div>` : ''}
    ${d.can_write ? how : ''}
  </div>`;
  return { title: `Packages · ${meta.owner}/${meta.name}`, html: repoPage(meta, 'packages', body), mount: (el) => mountRepoHeader(el, meta) };
}
