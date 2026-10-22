// Create a repository.
import { html, state, get, send, $, action, avatar, toast } from '../lib.js';
import { icon } from '../icons.js';
import { navigate } from '../main.js';

function nameError(n) {
  if (!n) return '';
  if (n.length > 100) return 'Names are up to 100 characters';
  if (!/^[A-Za-z0-9]/.test(n)) return 'Start with a letter or number';
  if (!/^[A-Za-z0-9._-]+$/.test(n)) return "Use only letters, numbers, '-', '_' or '.'";
  if (n.includes('..')) return "Two dots in a row aren't allowed";
  if (/\.git$/i.test(n)) return "Names can't end in .git";
  return '';
}

/** Suggest a valid name from free text: "My Cool App!" -> "my-cool-app". */
const suggest = (s) => s.trim().replace(/[^A-Za-z0-9._-]+/g, '-').replace(/^[^A-Za-z0-9]+/, '').replace(/-+$/, '').replace(/\.{2,}/g, '.').slice(0, 100);

export default async function (ctx) {
  const me = state.me;
  if (!me) return { redirect: '/login?next=/new' };
  const orgs = (await get('/me/orgs', { fresh: true }).catch(() => ({ orgs: [] }))).orgs.filter((o) => o.can_create);
  const want = ctx.query.get('owner');
  return {
    title: 'New repository',
    html: html`<div class="wrap narrow new-repo">
      <h1>Create a new repository</h1>
      <p class="muted">A repository holds your project's files and their full history.</p>
      <form id="new-repo" class="stack" novalidate>
        <div class="owner-name">
          ${orgs.length ? html`<div class="field"><label for="repo-owner">Owner</label><select class="select" id="repo-owner">
            <option value="${me.username}">${me.username}</option>${orgs.map((o) => html`<option value="${o.name}" ${o.name === want ? 'selected' : ''}>${o.name}</option>`)}</select></div>`
          : html`<div class="field"><label>Owner</label><div class="owner-chip">${avatar(me.username, 20)}<b>${me.username}</b></div></div>`}
          <span class="owner-slash">/</span>
          <div class="field grow"><label for="repo-name">Repository name</label>
            <input class="input" id="repo-name" name="name" autocomplete="off" spellcheck="false" autofocus required maxlength="100"></div>
        </div>
        <p class="hint small" id="name-msg">Short and memorable works best. Need ideas? <i>quick-notes</i>, <i>api-server</i>, <i>website</i>.</p>
        <div class="field"><label for="repo-desc">Description <span class="muted" style="font-weight:400">(optional)</span></label>
          <input class="input" id="repo-desc" name="description" maxlength="350" autocomplete="off"></div>
        <fieldset class="vis-cards"><legend class="label">Visibility</legend>
          <label class="vis-card"><input type="radio" name="vis" value="public" checked>
            <span class="vis-icon">${icon('globe')}</span><span><b>Public</b><span class="muted small">Anyone can see and clone it. Only people you give access can push.</span></span></label>
          <label class="vis-card"><input type="radio" name="vis" value="private">
            <span class="vis-icon">${icon('lock')}</span><span><b>Private</b><span class="muted small">Only people you give access can see it. It won't appear in search for anyone else.</span></span></label>
        </fieldset>
        <fieldset class="vis-cards start-cards"><legend class="label">Start with</legend>
          <label class="vis-card"><input type="radio" name="start" value="readme" checked>
            <span class="vis-icon">${icon('book')}</span><span><b>A README</b><span class="muted small">A first commit, so you can clone it right away.</span></span></label>
          <label class="vis-card"><input type="radio" name="start" value="empty">
            <span class="vis-icon">${icon('repo')}</span><span><b>Nothing</b><span class="muted small">An empty repository to push an existing project into.</span></span></label>
          <label class="vis-card"><input type="radio" name="start" value="copy">
            <span class="vis-icon">${icon('download')}</span><span><b>A copy of another repository</b><span class="muted small">From GitHub, GitLab or any git server, by its URL.</span></span></label>
        </fieldset>
        <div class="stack copy-fields" id="copy-fields" hidden>
          <div class="field"><label for="copy-url">Repository URL</label>
            <input class="input mono" id="copy-url" name="copy_url" type="url" placeholder="https://github.com/owner/project" spellcheck="false" autocomplete="off"></div>
          <div class="row wrap-ok" style="gap:12px">
            <div class="field grow"><label for="copy-user">User name <span class="muted" style="font-weight:400">(private repositories)</span></label><input class="input" id="copy-user" name="copy_user" autocomplete="off" spellcheck="false"></div>
            <div class="field grow"><label for="copy-pass">Password or token</label><input class="input" id="copy-pass" name="copy_pass" type="password" autocomplete="new-password"></div>
          </div>
          <label class="check box"><input type="checkbox" name="copy_sync" checked>
            <span><b>Keep it in sync</b><span class="muted small" style="display:block">A mirror: it follows the remote on a schedule and is read-only here. Untick for a one-time copy you can change.</span></span></label>
          ${me.is_admin ? html`<label class="check small"><input type="checkbox" name="copy_private"> Allow private addresses (a server on your own network)</label>` : ''}
        </div>
        <p class="field-err" id="form-err" role="alert" hidden></p>
        <div class="row"><button class="btn primary lg" type="submit" id="create-btn" disabled>${icon('repo', 'sm')} Create repository</button></div>
      </form>
    </div>`,
    mount(el) {
      const form = $('#new-repo', el);
      const name = $('#repo-name', el);
      const msg = $('#name-msg', el);
      const btn = $('#create-btn', el);
      const err = $('#form-err', el);
      const owner = () => $('#repo-owner', el)?.value || me.username;
      $('#repo-owner', el)?.addEventListener('change', () => check());
      const check = () => {
        const v = name.value.trim();
        const e = nameError(v);
        const s = suggest(v);
        btn.disabled = !v || !!e;
        name.setAttribute('aria-invalid', e ? 'true' : 'false');
        if (e) {
          msg.className = 'err small';
          msg.innerHTML = '';
          msg.append(e + '. ');
          if (s && !nameError(s) && s !== v) {
            const b = document.createElement('button');
            b.type = 'button';
            b.className = 'linkish c-accent';
            b.textContent = `Use “${s}”`;
            b.addEventListener('click', () => { name.value = s; check(); name.focus(); });
            msg.append(b);
          }
        } else {
          msg.className = 'hint small';
          msg.textContent = v ? `Your repository will live at ${location.host}/${owner()}/${v}` : 'Short and memorable works best.';
        }
      };
      name.addEventListener('input', check);
      const fields = $('#copy-fields', el);
      const copyUrl = $('#copy-url', el);
      form.addEventListener('change', (e) => {
        if (e.target.name === 'start') fields.hidden = e.target.value !== 'copy';
      });
      // A pasted URL suggests the name: https://host/owner/project.git -> project.
      copyUrl.addEventListener('input', () => {
        const last = copyUrl.value.trim().replace(/\/+$/, '').split('/').pop().replace(/\.git$/i, '');
        if (!name.value.trim() && last) { name.value = suggest(last); check(); }
      });
      form.addEventListener('submit', action(btn, async () => {
        err.hidden = true;
        const f = new FormData(form);
        const copying = f.get('start') === 'copy';
        const url = copyUrl.value.trim();
        if (copying && !/^https?:\/\/[^/]+\/./i.test(url)) {
          err.textContent = 'Give the repository URL, starting with https:// or http://';
          err.hidden = false;
          return copyUrl.focus();
        }
        let r;
        try {
          r = await send('POST', '/repos', { owner: owner(), name: name.value.trim(), description: String(f.get('description') || '').trim(), private: f.get('vis') === 'private', readme: f.get('start') === 'readme' });
        } catch (e) { err.textContent = e.message; err.hidden = false; return; }
        if (copying) {
          try {
            await send('POST', `/repos/${r.owner}/${r.name}/mirrors`, {
              direction: 'pull', url, username: String(f.get('copy_user') || '').trim(), password: String(f.get('copy_pass') || ''),
              allow_private: f.get('copy_private') === 'on', once: f.get('copy_sync') !== 'on',
            });
          } catch (e) {
            // The repository exists; the copy can be set up again where mirrors live.
            toast(`Created ${r.owner}/${r.name}, but the copy couldn't start: ${e.message}`, 'err');
            return navigate(`/${r.owner}/${r.name}/settings/mirrors`);
          }
        }
        navigate(`/${r.owner}/${r.name}`);
      }));
    },
  };
}
