// New issue form, with a chooser first when the repository has issue templates.
import { html, state, get, send, action, enc, $, $$ } from '../lib.js';
import { icon } from '../icons.js';
import { loadRepo, repoUrl, repoPage, mountRepoHeader } from '../repohead.js';
import { mdEditor, mountMdEditor, clearDraft } from '../mdeditor.js';
import { navigate } from '../main.js';
import { labelChip } from './issues.js';

export default async function (ctx) {
  const { owner, repo } = ctx.params;
  if (!state.me) return { redirect: '/login?next=' + encodeURIComponent(ctx.url.pathname + ctx.url.search) };
  const [meta, tpl] = await Promise.all([
    loadRepo(owner, repo),
    get(`${repoUrl(owner, repo)}/issue-templates`).catch(() => ({ issue: [] })),
  ]);
  const base = `/${owner}/${repo}`;
  const want = ctx.query.get('template');
  const chosen = tpl.issue.find((t) => t.file === want) || null;
  if (tpl.issue.length && want === null) {
    const body = html`<div class="stack mid" style="margin:0 auto">
      <h2 class="page-h">${icon('issue')} New issue</h2>
      <div class="box"><div class="list" id="template-chooser">${tpl.issue.map((t) => html`<div class="list-row">
        <div class="grow" style="min-width:0"><b>${t.name}</b>${t.about ? html`<div class="muted small">${t.about}</div>` : ''}</div>
        <a class="btn primary sm" href="${base}/issues/new?template=${enc(t.file)}">Get started</a></div>`)}</div></div>
      <a class="small c-accent" href="${base}/issues/new?template=">Open a blank issue</a>
    </div>`;
    return { title: `New issue · ${owner}/${repo}`, html: repoPage(meta, 'issues', body), mount(el) { mountRepoHeader(el, meta); } };
  }
  const labels = meta.can_write ? (await get(`${repoUrl(owner, repo)}/labels`).catch(() => ({ labels: [] }))).labels : [];
  const preset = new Set(chosen?.labels || []);
  // A template's text, not a saved draft of another one: drafts are kept per template.
  const draftId = chosen ? `issue-body-${chosen.file}` : 'issue-body';
  const body = html`<form class="new-issue split" id="new-issue">
    <div class="stack">
      ${chosen ? html`<div class="muted small">${icon('book', 'sm inline')} From the <b>${chosen.name}</b> template · <a class="c-accent" href="${base}/issues/new">Choose another</a></div>` : ''}
      <div class="field"><label for="issue-title">Title</label><input class="input lg-input" id="issue-title" name="title" required maxlength="256" value="${chosen?.title || ''}" placeholder="Short summary of the problem or idea" autofocus autocomplete="off"></div>
      <div class="field"><label for="${draftId}">Description</label>${mdEditor({ id: draftId, value: chosen?.body || '', placeholder: 'Steps to reproduce, what you expected, what happened…', rows: 10 })}</div>
      <div class="row" style="justify-content:flex-end"><a class="btn" href="/${owner}/${repo}/issues">Cancel</a><button class="btn primary" type="submit">${icon('issue', 'sm')} Create issue</button></div>
    </div>
    <aside class="issue-side">
      ${labels.length ? html`<div class="side-block"><div class="section-title">${icon('label', 'sm')} Labels</div>
        <div class="label-picks">${labels.map((l) => html`<label class="label-pick"><input type="checkbox" value="${l.id}" ${preset.has(l.name) ? 'checked' : ''}>${labelChip(l)}</label>`)}</div></div>` : ''}
      <div class="side-block muted small">${icon('book', 'sm inline')} Markdown works here: <code>**bold**</code>, <code>\`code\`</code>, lists, task lists, tables, and <code>#123</code> or <code>@name</code> references.</div>
    </aside>
  </form>`;
  return {
    title: `New issue · ${owner}/${repo}`,
    html: repoPage(meta, 'issues', body),
    mount(el) {
      mountRepoHeader(el, meta);
      mountMdEditor(el, { owner, repo, rev: meta.default_branch, dir: '' });
      const form = $('#new-issue', el);
      const btn = form.querySelector('[type=submit]');
      form.addEventListener('submit', action(btn, async () => {
        const r = await send('POST', `${repoUrl(owner, repo)}/issues`, {
          title: form.title.value.trim(),
          body: form.querySelector('.mde-input').value,
          labels: $$('.label-pick input:checked', form).map((c) => +c.value),
          // Writers pick labels themselves (the template's start ticked); for others the template's apply.
          template: meta.can_write ? undefined : chosen?.file,
        });
        clearDraft(draftId);
        navigate(`/${owner}/${repo}/issues/${r.number}`, { replace: true });
      }));
    },
  };
}
