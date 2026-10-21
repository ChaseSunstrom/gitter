// Markdown textarea with Write / Preview tabs, autosize, Ctrl/Cmd+Enter to
// submit, and a draft saved per page so nothing typed is lost on reload.
import { html } from './lib.js';
import { renderMarkdown, preloadLangs } from './md.js';

const draftKey = (id) => `draft:${location.pathname}:${id}`;
function loadDraft(id) { try { return localStorage.getItem(draftKey(id)); } catch { return null; } }
function saveDraft(id, v) { try { v ? localStorage.setItem(draftKey(id), v) : localStorage.removeItem(draftKey(id)); } catch { /* private mode */ } }
export function clearDraft(id) { saveDraft(id, ''); }

export function mdEditor({ id, value = '', placeholder = 'Leave a comment', rows = 6, draft = true }) {
  const v = (draft && loadDraft(id)) || value;
  return html`<div class="mde" data-mde="${id}" data-draft="${draft ? '1' : ''}">
    <div class="mde-tabs seg" role="tablist"><button type="button" class="on" data-mde-tab="write">Write</button><button type="button" data-mde-tab="preview">Preview</button></div>
    <textarea class="textarea mde-input" id="${id}" name="${id}" rows="${rows}" placeholder="${placeholder}">${v}</textarea>
    <div class="mde-preview markdown-body" hidden></div>
    <div class="mde-foot tiny faint">Markdown supported · <kbd>Ctrl</kbd>+<kbd>Enter</kbd> to submit</div>
  </div>`;
}

function autosize(ta) {
  ta.style.height = 'auto';
  ta.style.height = Math.min(Math.max(ta.scrollHeight + 2, 96), 640) + 'px';
}

/** Wire every editor inside `root`. ctx gives repo context for links in the preview. */
export function mountMdEditor(root, ctx = {}) {
  root.querySelectorAll('[data-mde]').forEach((box) => {
    const id = box.dataset.mde;
    const ta = box.querySelector('textarea');
    const pv = box.querySelector('.mde-preview');
    const useDraft = box.dataset.draft === '1';
    autosize(ta);
    ta.addEventListener('input', () => { autosize(ta); if (useDraft) saveDraft(id, ta.value); });
    ta.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        const form = ta.closest('form');
        form?.querySelector('[type=submit]:not([disabled])')?.click();
      }
    });
    box.querySelector('.mde-tabs').addEventListener('click', async (e) => {
      const b = e.target.closest('[data-mde-tab]');
      if (!b) return;
      const preview = b.dataset.mdeTab === 'preview';
      box.querySelectorAll('[data-mde-tab]').forEach((x) => x.classList.toggle('on', x === b));
      if (preview) {
        await preloadLangs(ta.value);
        pv.innerHTML = ta.value.trim() ? renderMarkdown(ta.value, ctx) : html`<p class="muted">Nothing to preview</p>`.s;
        pv.style.minHeight = ta.offsetHeight + 'px';
      }
      ta.hidden = preview;
      pv.hidden = !preview;
      if (!preview) ta.focus();
    });
  });
}
