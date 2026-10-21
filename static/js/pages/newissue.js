// New issue form.
import { html, state, get, send, action, $, $$ } from '../lib.js';
import { icon } from '../icons.js';
import { loadRepo, repoUrl, repoPage, mountRepoHeader } from '../repohead.js';
import { mdEditor, mountMdEditor, clearDraft } from '../mdeditor.js';
import { navigate } from '../main.js';
import { labelChip } from './issues.js';

export default async function (ctx) {
  const { owner, repo } = ctx.params;
  if (!state.me) return { redirect: '/login?next=' + encodeURIComponent(ctx.url.pathname) };
  const meta = await loadRepo(owner, repo);
  const labels = meta.can_write ? (await get(`${repoUrl(owner, repo)}/labels`).catch(() => ({ labels: [] }))).labels : [];
  const body = html`<form class="new-issue split" id="new-issue">
    <div class="stack">
      <div class="field"><label for="issue-title">Title</label><input class="input lg-input" id="issue-title" name="title" required maxlength="256" placeholder="Short summary of the problem or idea" autofocus autocomplete="off"></div>
      <div class="field"><label for="issue-body">Description</label>${mdEditor({ id: 'issue-body', placeholder: 'Steps to reproduce, what you expected, what happened…', rows: 10 })}</div>
      <div class="row" style="justify-content:flex-end"><a class="btn" href="/${owner}/${repo}/issues">Cancel</a><button class="btn primary" type="submit">${icon('issue', 'sm')} Create issue</button></div>
    </div>
    <aside class="issue-side">
      ${labels.length ? html`<div class="side-block"><div class="section-title">${icon('label', 'sm')} Labels</div>
        <div class="label-picks">${labels.map((l) => html`<label class="label-pick"><input type="checkbox" value="${l.id}">${labelChip(l)}</label>`)}</div></div>` : ''}
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
          body: $('#issue-body', form).value,
          labels: $$('.label-pick input:checked', form).map((c) => +c.value),
        });
        clearDraft('issue-body');
        navigate(`/${owner}/${repo}/issues/${r.number}`, { replace: true });
      }));
    },
  };
}
