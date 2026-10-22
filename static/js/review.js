// Code review on pull requests: threads placed in the diff, the line-comment
// composer, the review form, and the review cards of the conversation.
import { html, raw, state, send, timeEl, avatar, popover, closePop, toast, confirmDialog } from './lib.js';
import { icon } from './icons.js';
import { renderMarkdown } from './md.js';
import { refresh } from './main.js';

const VERB = { approved: ['check', 'c-green', 'approved these changes'], changes_requested: ['x', 'c-red', 'requested changes'], commented: ['comment', 'muted', 'reviewed'] };

const where = (t) => `${t.path} ${t.start_line ? `lines ${t.start_line}–${t.line}` : `line ${t.line}`}${t.side === 'old' ? ' (old)' : ''}`;

function commentHtml(c, mdCtx) {
  return html`<div class="rc" data-c="${c.id}">
    <div class="rc-head">${avatar(c.author, 20)}<a href="/${c.author}"><b>${c.author}</b></a><span class="muted small">${timeEl(c.created_at)}${c.updated_at > c.created_at + 5 ? ' · edited' : ''}</span>
      ${c.pending ? html`<span class="pill">Pending</span>` : ''}<span class="grow"></span>
      ${c.can_edit ? html`<button class="btn ghost sm icon" data-rc-edit="${c.id}" aria-label="Edit comment">${icon('edit', 'sm')}</button>` : ''}
      ${c.can_delete ? html`<button class="btn ghost sm icon" data-rc-del="${c.id}" aria-label="Delete comment">${icon('trash', 'sm')}</button>` : ''}</div>
    <div class="markdown-body rc-body" data-rc-body="${c.id}">${raw(renderMarkdown(c.body, mdCtx))}</div>
  </div>`;
}

/** One thread: its comments, a reply box, and resolve / apply buttons. */
export function threadHtml(t, mdCtx) {
  const resolved = !!t.resolved_by;
  const n = t.comments.length;
  return html`<details class="rthread ${resolved ? 'resolved' : ''}" data-thread="${t.id}" ${resolved ? '' : 'open'}>
    <summary class="small">${icon('comment', 'sm')} <span class="mono ellipsis">${where(t)}</span>
      ${t.outdated ? html`<span class="pill">Outdated</span>` : ''}${t.pending ? html`<span class="pill">Pending</span>` : ''}
      <span class="grow"></span><span class="muted">${resolved ? `Resolved by ${t.resolved_by}` : `${n} comment${n === 1 ? '' : 's'}`}</span></summary>
    ${t.comments.map((c) => commentHtml(c, mdCtx))}
    <div class="rthread-foot">
      ${state.me ? html`<form class="rreply" data-reply="${t.id}"><textarea class="textarea" name="body" rows="1" placeholder="Reply…" aria-label="Reply" required></textarea>
        <button class="btn sm primary" type="submit">Reply</button></form>` : ''}
      <div class="row">${t.can_apply ? html`<button class="btn sm" data-apply="${t.id}" data-updated="${t.comments[0].updated_at}">${icon('check', 'sm')} Apply suggestion</button>` : ''}
        ${t.can_resolve ? html`<button class="btn sm" data-resolve="${t.id}" data-to="${resolved ? '0' : '1'}">${resolved ? 'Unresolve' : 'Resolve conversation'}</button>` : ''}</div>
    </div>
  </details>`;
}

/** The text of `line` on `side` of a diff file, from its hunks (null: not shown in the diff). */
function lineText(f, side, line) {
  for (const h of f.hunks) for (const [k, o, n, t] of h.lines) if ((side === 'old' ? k !== '+' && o === line : k !== '-' && n === line)) return t;
  return null;
}

/** mountDiff options that put threads in the diff and open a composer on a line number click. */
export function diffReview(rv, ctx) {
  const placed = (t, f) => !t.outdated && t.path === (t.side === 'old' ? f.old_path : f.new_path);
  let last = null;
  return {
    decorate(f, body, mode) {
      for (const t of rv.threads.filter((x) => placed(x, f))) {
        if (body.querySelector(`[data-thread-row="${t.id}"]`)) continue;
        const td = body.querySelector(`td.dn[data-side="${t.side}"][data-n="${t.line}"]`);
        if (!td) continue;
        let after = td.closest('tr');
        while (after.nextElementSibling?.matches('.rthread-row')) after = after.nextElementSibling;
        after.insertAdjacentHTML('afterend', html`<tr class="rthread-row" data-thread-row="${t.id}"><td colspan="${mode === 'split' ? 4 : 3}">${threadHtml(t, ctx.mdCtx)}</td></tr>`.s);
      }
    },
    lineClick({ file, side, line, tr, shift, mode }) {
      if (!state.me) return toast('Sign in to comment on lines');
      const range = shift && last && last.file === file && last.side === side ? [Math.min(last.line, line), Math.max(last.line, line)] : [line, line];
      last = { file, side, line };
      document.querySelectorAll('.rcomposer-row').forEach((r) => r.remove());
      document.querySelectorAll('tr.rsel').forEach((r) => r.classList.remove('rsel'));
      const body = tr.closest('tbody');
      let end = tr;
      for (let n = range[0]; n <= range[1]; n++) {
        const row = body.querySelector(`td.dn[data-side="${side}"][data-n="${n}"]`)?.closest('tr');
        if (row) { row.classList.add('rsel'); end = row; }
      }
      const path = side === 'old' ? file.old_path : file.new_path;
      const label = range[0] === range[1] ? `line ${range[1]}` : `lines ${range[0]}–${range[1]}`;
      const original = [];
      for (let n = range[0]; n <= range[1]; n++) original.push(lineText(file, side, n));
      end.insertAdjacentHTML('afterend', html`<tr class="rcomposer-row"><td colspan="${mode === 'split' ? 4 : 3}">
        <form class="rcomposer box">
          <div class="small muted">Comment on <span class="mono">${path}</span> ${label} <span class="hide-sm">· Shift-click another line number to comment on a range</span></div>
          <textarea class="textarea" name="body" rows="4" placeholder="Leave a comment" aria-label="Comment" required></textarea>
          <div class="row wrap-ok">${side === 'new' ? html`<button class="btn sm ghost" type="button" data-suggest ${original.includes(null) ? 'disabled' : ''} title="${original.includes(null) ? "Some of these lines aren't shown in the diff" : 'Propose replacement lines'}">${icon('edit', 'sm')} Suggest a change</button>` : ''}
            <span class="grow"></span><button class="btn sm" type="button" data-cancel>Cancel</button>
            <button class="btn sm" type="submit" data-single>Add single comment</button>
            <button class="btn sm primary" type="submit">${rv.pending ? 'Add review comment' : 'Start a review'}</button></div>
        </form></td></tr>`.s);
      const form = end.nextElementSibling.querySelector('form');
      const ta = form.body;
      ta.focus();
      form.querySelector('[data-cancel]').onclick = () => { form.closest('tr').remove(); document.querySelectorAll('tr.rsel').forEach((r) => r.classList.remove('rsel')); };
      const sg = form.querySelector('[data-suggest]');
      if (sg) sg.onclick = () => { ta.value += `${ta.value ? '\n' : ''}\`\`\`suggestion\n${original.join('\n')}\n\`\`\``; ta.focus(); };
      form.addEventListener('submit', async (e) => {
        e.preventDefault();
        const pending = !e.submitter?.hasAttribute('data-single');
        const req = { path, side, line: range[1], body: ta.value };
        if (range[0] < range[1]) req.start_line = range[0];
        if (pending) req.pending = true;
        try { await send('POST', `${ctx.api}/pulls/${ctx.n}/comments`, req); } catch (x) { return toast(x.message, 'err'); }
        refresh();
      });
    },
  };
}

/** The "Review changes" button and the outdated-threads note above a PR's diff. */
export function reviewBar(rv) {
  if (!rv) return '';
  const outdated = rv.threads.filter((t) => t.outdated).length;
  return html`<div class="review-bar row">
    ${outdated ? html`<span class="muted small">${icon('comment', 'sm')} ${outdated} outdated conversation${outdated === 1 ? '' : 's'} on the Conversation tab</span>` : ''}
    <span class="grow"></span>
    ${state.me ? html`<button class="btn sm primary" id="review-btn">Review changes${rv.pending ? html` <span class="counter">${rv.pending.comments}</span>` : ''}</button>` : ''}
  </div>`;
}

/** Wire up threads (reply, resolve, apply, edit, delete) and the review form under `root`. */
export function mountReview(root, rv, ctx) {
  if (!rv) return;
  const url = `${ctx.api}/pulls/${ctx.n}`;
  const act = (fn) => async (...a) => { try { await fn(...a); } catch (x) { toast(x.message, 'err'); } };
  root.addEventListener('submit', act(async (e) => {
    const f = e.target.closest('[data-reply]');
    if (!f) return;
    e.preventDefault();
    await send('POST', `${url}/comments`, { in_reply_to: +f.dataset.reply, body: f.body.value });
    refresh();
  }));
  root.addEventListener('click', act(async (e) => {
    const t = e.target;
    const res = t.closest('[data-resolve]');
    if (res) { await send('POST', `${url}/comments/${res.dataset.resolve}/resolve`, { resolved: res.dataset.to === '1' }); return refresh(); }
    const ap = t.closest('[data-apply]');
    if (ap) {
      ap.disabled = true;
      try { await send('POST', `${url}/comments/${ap.dataset.apply}/apply`, { updated_at: +ap.dataset.updated }); } finally { ap.disabled = false; }
      toast('Suggestion committed to the branch');
      return refresh();
    }
    const del = t.closest('[data-rc-del]');
    if (del) {
      if (!(await confirmDialog({ title: 'Delete this comment?', text: "A thread's first comment takes its replies with it.", ok: 'Delete', danger: true }))) return;
      await send('DELETE', `${url}/comments/${del.dataset.rcDel}`);
      return refresh();
    }
    const ed = t.closest('[data-rc-edit]');
    if (ed) {
      const id = ed.dataset.rcEdit;
      const c = rv.threads.flatMap((x) => x.comments).find((x) => String(x.id) === id);
      const box = root.querySelector(`[data-rc-body="${id}"]`);
      box.innerHTML = html`<form class="rc-edit"><textarea class="textarea" name="body" rows="4" aria-label="Edit comment" required>${c.body}</textarea>
        <div class="row"><span class="grow"></span><button class="btn sm" type="button" data-x>Cancel</button><button class="btn sm primary" type="submit">Save</button></div></form>`.s;
      const f = box.querySelector('form');
      f.querySelector('[data-x]').onclick = refresh;
      f.onsubmit = act(async (ev) => { ev.preventDefault(); await send('PATCH', `${url}/comments/${id}`, { body: f.body.value }); refresh(); });
      return;
    }
    const rb = t.closest('#review-btn');
    if (rb) {
      const mine = ctx.isAuthor;
      popover(rb, html`<form class="review-form stack" id="review-form">
        <b>Finish your review</b>
        <textarea class="textarea" name="body" rows="4" placeholder="Leave a comment" aria-label="Review summary"></textarea>
        <label class="check"><input type="radio" name="event" value="comment" checked><span><b>Comment</b><span class="muted small" style="display:block">General feedback without a decision.</span></span></label>
        <label class="check"><input type="radio" name="event" value="approve" ${mine ? 'disabled' : ''}><span><b>Approve</b><span class="muted small" style="display:block">${mine ? "You can't approve your own pull request." : 'Ready to merge, as far as you can tell.'}</span></span></label>
        <label class="check"><input type="radio" name="event" value="request_changes" ${mine ? 'disabled' : ''}><span><b>Request changes</b><span class="muted small" style="display:block">Feedback that must be addressed first.</span></span></label>
        <div class="row">${rv.pending ? html`<button class="btn sm ghost" type="button" data-discard>Discard ${rv.pending.comments} pending</button>` : ''}<span class="grow"></span><button class="btn sm primary" type="submit">Submit review</button></div>
      </form>`, {
        width: 380, align: 'right', onMount(p) {
          const f = p.querySelector('form');
          f.onsubmit = act(async (ev) => { ev.preventDefault(); await send('POST', `${url}/reviews`, { event: new FormData(f).get('event'), body: f.body.value }); closePop(); toast('Review submitted'); refresh(); });
          const d = f.querySelector('[data-discard]');
          if (d) d.onclick = act(async () => { await send('DELETE', `${url}/reviews/pending`); closePop(); refresh(); });
        },
      });
    }
  }));
}

/** Conversation cards: submitted reviews (with their threads) and single line comments, each with the time it belongs at. */
export function reviewItems(rv, mdCtx) {
  if (!rv) return [];
  const byReview = new Map();
  const single = [];
  for (const t of rv.threads) {
    if (t.pending) continue;
    if (t.review_id) (byReview.get(t.review_id) || byReview.set(t.review_id, []).get(t.review_id)).push(t);
    else single.push(t);
  }
  const code = (t) => html`<div class="rsnippet"><pre class="mono">${t.original}</pre></div>`;
  const reviews = rv.reviews.map((r) => {
    const [ic, cls, verb] = VERB[r.state] || VERB.commented;
    const threads = byReview.get(r.id) || [];
    return { time: r.submitted_at, node: html`<div class="tl-item tl-review" id="review-${r.id}">
      <a href="/${r.author}" class="tl-avatar hide-sm">${avatar(r.author, 36)}</a>
      <div class="comment-card"><div class="comment-head"><span class="${cls}">${icon(ic, 'sm')}</span><a href="/${r.author}"><b>${r.author}</b></a><span class="muted small">${verb} ${timeEl(r.submitted_at)}</span></div>
        ${r.body ? html`<div class="comment-body markdown-body">${raw(renderMarkdown(r.body, mdCtx))}</div>` : ''}
        ${threads.map((t) => html`<div class="rcard-thread">${code(t)}${threadHtml(t, mdCtx)}</div>`)}
      </div></div>` };
  });
  const singles = single.map((t) => ({ time: t.comments[0].created_at, node: html`<div class="tl-item tl-review">
    <a href="/${t.comments[0].author}" class="tl-avatar hide-sm">${avatar(t.comments[0].author, 36)}</a>
    <div class="comment-card"><div class="comment-head"><span class="muted">${icon('comment', 'sm')}</span><b>${t.comments[0].author}</b><span class="muted small">commented on a line ${timeEl(t.comments[0].created_at)}</span></div>
      <div class="rcard-thread">${code(t)}${threadHtml(t, mdCtx)}</div></div></div>` }));
  return [...reviews, ...singles];
}

/** The sidebar's reviewers: who approved and who asked for changes. */
export function reviewersBlock(rv) {
  if (!rv) return '';
  const { approved, changes_requested: changes } = rv.summary;
  const row = (who, ic, cls, title) => html`<div class="row small" title="${title}"><a href="/${who}" class="row">${avatar(who, 20)}<span>${who}</span></a><span class="grow"></span><span class="${cls}">${icon(ic, 'sm')}</span></div>`;
  return html`<div class="side-block"><div class="section-title">${icon('users', 'sm')} Reviewers</div>
    ${approved.length || changes.length
      ? html`${changes.map((w) => row(w, 'x', 'c-red', 'Requested changes'))}${approved.map((w) => row(w, 'check', 'c-green', 'Approved'))}`
      : html`<span class="muted small">No reviews yet</span>`}</div>`;
}
