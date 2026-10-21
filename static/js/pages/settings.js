// Account settings: profile, password, access tokens, appearance.
import { html, raw, state, get, send, $, action, toast, timeEl, confirmDialog, avatar, copy, auditLabel, deviceName, bytes } from '../lib.js';
import { icon } from '../icons.js';
import { refresh, renderTopbar, setTheme, getTheme, SKINS, getSkin, setSkin } from '../main.js';

const TABS = [['profile', 'user', 'Profile'], ['account', 'shield', 'Password'], ['security', 'lock', 'Security'], ['tokens', 'key', 'Access tokens'], ['appearance', 'sun', 'Appearance']];

function nav(tab) {
  return html`<nav class="settings-nav" aria-label="Settings">${TABS.map(([k, ic, label]) =>
    html`<a href="/settings${k === 'profile' ? '' : '/' + k}" class="${tab === k ? 'on' : ''}">${icon(ic, 'sm')}${label}</a>`)}</nav>`;
}

function profileTab(p, storage) {
  const pct = storage?.quota ? Math.min(100, (storage.used / storage.quota) * 100) : 0;
  return html`<form id="profile-form" class="stack settings-form" novalidate>
    <h2>Public profile</h2>
    ${storage ? html`<div class="small muted">Storage: <b>${bytes(storage.used)}</b>${storage.quota ? html` of ${bytes(storage.quota)} used <span class="disk-bar small-bar" role="img" aria-label="Storage used"><span style="width:${pct}%"></span></span>` : ' used'}</div>` : ''}
    <div class="field"><label for="display_name">Name</label><input class="input" id="display_name" name="display_name" value="${p.display_name}" maxlength="80" autocomplete="name">
      <span class="hint">Shown on your profile and on commits you make on the web.</span></div>
    <div class="field"><label for="email">Email</label><input class="input" id="email" name="email" type="email" value="${p.email}" maxlength="254" autocomplete="email">
      <span class="hint">Private. Used as the author email for web commits.</span></div>
    <div class="field"><label for="bio">Bio</label><textarea class="textarea" id="bio" name="bio" maxlength="300" rows="3" style="min-height:80px">${p.bio}</textarea>
      <span class="hint"><span id="bio-count">${p.bio.length}</span> / 300</span></div>
    <div class="grid-2">
      <div class="field"><label for="location">Location</label><input class="input" id="location" name="location" value="${p.location}" maxlength="80"></div>
      <div class="field"><label for="website">Website</label><input class="input" id="website" name="website" value="${p.website}" maxlength="200" placeholder="https://"></div>
    </div>
    <div class="row"><button class="btn primary" type="submit">Save profile</button></div>
  </form>`;
}

function accountTab() {
  return html`<form id="pw-form" class="stack settings-form" novalidate>
    <h2>Change password</h2>
    <div class="field"><label for="current">Current password</label><input class="input" id="current" name="current" type="password" autocomplete="current-password" required></div>
    <div class="field"><label for="new">New password</label><input class="input" id="new" name="new" type="password" autocomplete="new-password" minlength="8" required><span class="hint">At least 8 characters.</span></div>
    <div class="field"><label for="confirm">Confirm new password</label><input class="input" id="confirm" name="confirm" type="password" autocomplete="new-password" required></div>
    <p class="field-err" id="pw-err" role="alert" hidden></p>
    <div class="row"><button class="btn primary" type="submit">Update password</button></div>
  </form>`;
}

const SCOPES = { read: 'Read only', write: 'Read and write', all: 'Full access' };

function tokensTab(tokens) {
  return html`<div class="stack settings-form">
    <h2>Personal access tokens</h2>
    <p class="muted">Tokens work like passwords for git over HTTPS and for the API. Give each one a name so you know where it's used, and delete any you no longer need.</p>
    <form id="token-form" class="box"><div class="box-body stack">
      <div class="field"><label for="tk-name">Name</label><input class="input" id="tk-name" name="name" placeholder="What's this token for? e.g. laptop" maxlength="80" required></div>
      <div class="grid-2">
        <div class="field"><label for="tk-scope">Access</label><select class="select" id="tk-scope" name="scope">
          <option value="write" selected>Read and write: clone, push, issues, pull requests</option>
          <option value="read">Read only: clone and view</option>
          <option value="all">Full access: everything your account can do</option></select></div>
        <div class="field"><label for="tk-exp">Expires</label><select class="select" id="tk-exp" name="expires_days">
          <option value="30">In 30 days</option><option value="90" selected>In 90 days</option><option value="365">In a year</option><option value="">Never</option></select></div>
      </div>
      <div class="field"><label for="tk-repo">Only for one repository <span class="muted">(optional)</span></label><input class="input" id="tk-repo" name="repo" placeholder="owner/name, or leave empty for all your repositories" autocapitalize="off" spellcheck="false"></div>
      <div class="row"><button class="btn primary" type="submit">${icon('plus', 'sm')} Generate token</button></div>
    </div></form>
    <div id="new-token"></div>
    ${tokens.length ? html`<div class="box list">${tokens.map((t) => html`<div class="list-row token-row">
        ${icon('key', 'muted')}<div class="grow"><b>${t.name}</b> <span class="pill">${SCOPES[t.scope] || t.scope}</span>${t.repo ? html` <span class="pill">${t.repo}</span>` : ''}${t.expired ? html` <span class="pill closed">Expired</span>` : ''}
          <div class="small muted">Created ${timeEl(t.created_at)} · ${t.last_used ? html`Last used ${timeEl(t.last_used)}` : 'Never used'}${t.expires_at && !t.expired ? html` · expires ${timeEl(t.expires_at)}` : ''}</div></div>
        <button class="btn sm danger" data-del="${t.id}" data-name="${t.name}">${icon('trash', 'sm')} Delete</button></div>`)}</div>`
      : html`<div class="box empty small">${icon('key', 'big')}<h3>No tokens yet</h3><p>Generate one to push from scripts or other machines.</p></div>`}
  </div>`;
}

function securityTab(sec, log) {
  const tfa = sec.two_factor
    ? html`<div class="box"><div class="box-body row">${icon('check', 'c-green')}<div class="grow"><b>Two-factor authentication is on</b>
        <div class="small muted">Signing in needs your password and a code from your app. ${sec.recovery_left} of 10 recovery codes left.</div></div>
        <button class="btn sm" data-tfa="codes">New recovery codes</button><button class="btn sm danger" data-tfa="off">Turn off</button></div></div>`
    : html`<div class="box"><div class="box-body row">${icon('shield', 'muted')}<div class="grow"><b>Two-factor authentication is off</b>
        <div class="small muted">Protect your account even if your password leaks: signing in will also need a code from an app like 1Password, Bitwarden, Google Authenticator or Aegis.</div></div>
        <button class="btn primary sm" data-tfa="setup">Set up</button></div></div>`;
  const sessions = sec.sessions.map((x) => html`<div class="list-row">
      ${icon('globe', 'muted')}<div class="grow"><b>${deviceName(x.agent)}</b>${x.current ? html` <span class="pill">This browser</span>` : ''}
      <div class="small muted">${x.ip || 'unknown address'} · signed in ${timeEl(x.created_at)} · active ${timeEl(x.last_seen)}</div></div>
      ${x.current ? '' : html`<button class="btn sm" data-revoke="${x.id}">Sign out</button>`}</div>`);
  const events = log.events.map((e) => html`<div class="list-row small">
      <span class="grow"><b>${auditLabel(e.action)}</b>${e.detail ? html` <span class="muted">· ${e.detail}</span>` : ''}${e.actor && e.actor !== state.me.username ? html` <span class="muted">by ${e.actor}</span>` : ''}</span>
      <span class="muted nowrap">${e.ip ? e.ip + ' · ' : ''}${timeEl(e.created_at)}</span></div>`);
  return html`<div class="stack settings-form">
    <h2>Two-factor authentication</h2>
    ${tfa}<div id="tfa-flow"></div>
    <h2>Where you're signed in</h2>
    <div class="box list">${sessions}</div>
    ${sec.sessions.length > 1 ? html`<div class="row"><button class="btn sm" data-revoke-others>Sign out everywhere else</button></div>` : ''}
    <h2>Security log</h2>
    <p class="muted small">Sign-ins and changes to your account's security, newest first.</p>
    ${events.length ? html`<div class="box list">${events}</div>` : html`<p class="muted">Nothing yet.</p>`}
  </div>`;
}

function codesView(codes) {
  return html`<div class="token-reveal fade stack">
    <div class="row">${icon('check', 'c-green')}<b>Save your recovery codes</b></div>
    <p class="small">If you lose your phone, each of these signs you in once. Keep them somewhere safe, like a password manager. They won't be shown again.</p>
    <pre class="setup-cmd recovery-codes">${codes.join('\n')}</pre>
    <div class="row"><button class="btn sm" data-codes-copy>${icon('copy', 'sm')} Copy</button><a class="btn sm" data-codes-dl download="gitter-recovery-codes.txt">Download</a>
      <span class="grow"></span><button class="btn primary sm" data-done>I've saved them</button></div></div>`;
}

function themeCard(value, label, current) {
  return html`<label class="theme-card ${value}">
    <input type="radio" name="theme" value="${value}" ${current === value ? 'checked' : ''}>
    <span class="theme-preview" aria-hidden="true"><span class="tp-bar"></span><span class="tp-line w60"></span><span class="tp-line w80"></span><span class="tp-line w40 accent"></span></span>
    <span class="row"><b>${label}</b></span></label>`;
}

function appearanceTab() {
  const cur = getTheme();
  return html`<div class="stack settings-form">
    <h2>Appearance</h2>
    <p class="muted">Choose how Gitter looks to you. This is saved in this browser. Press <kbd>Shift</kbd> <kbd>T</kbd> anywhere to switch quickly.</p>
    <h3 class="section-title" style="margin-top:6px">Color theme</h3>
    <div class="skin-cards" role="radiogroup" aria-label="Color theme">${SKINS.map((sk) => html`<label class="skin-card">
      <input type="radio" name="skin" value="${sk.id}" ${getSkin() === sk.id ? 'checked' : ''}>
      <span class="skin-preview" style="--p-bg:${sk.bg};--p-surface:${sk.surface};--p-ink:${sk.ink};--p-grad:${sk.grad}" aria-hidden="true">
        <span class="sp-bar"></span><span class="sp-line"></span><span class="sp-line short"></span><span class="sp-btn"></span></span>
      <span class="skin-text"><b>${sk.name}</b><span class="muted small">${sk.blurb}</span></span></label>`)}</div>
    <h3 class="section-title" style="margin-top:6px">Light or dark</h3>
    <div class="theme-cards" role="radiogroup" aria-label="Light or dark">${themeCard('light', 'Light', cur)}${themeCard('dark', 'Dark', cur)}${themeCard('system', 'Match system', cur)}</div>
  </div>`;
}

export default async function (ctx) {
  if (!state.me) return { redirect: '/login?next=' + encodeURIComponent(ctx.path) };
  const tab = TABS.some(([k]) => k === ctx.params.tab) ? ctx.params.tab : 'profile';
  let inner;
  if (tab === 'profile') { const m = await get('/me', { fresh: true }); inner = profileTab(m.profile, m.storage); }
  else if (tab === 'tokens') inner = tokensTab((await get('/me/tokens', { fresh: true })).tokens);
  else if (tab === 'account') inner = accountTab();
  else if (tab === 'security') {
    const [sec, log] = await Promise.all([get('/me/security', { fresh: true }), get('/me/audit', { fresh: true })]);
    inner = securityTab(sec, log);
  }
  else inner = appearanceTab();
  const me = state.me;
  return {
    title: 'Settings',
    html: html`<div class="wrap mid settings">
      <div class="settings-head">${avatar(me.username, 40)}<div><h1>${me.display_name || me.username}</h1><p class="muted small">Your personal account · <a class="c-accent" href="/${me.username}">View profile</a></p></div></div>
      <div class="settings-grid">${nav(tab)}<section class="settings-body fade">${inner}</section></div>
    </div>`,
    mount(el) {
      const pf = $('#profile-form', el);
      if (pf) {
        const bio = $('#bio', el);
        bio.addEventListener('input', () => { $('#bio-count', el).textContent = bio.value.length; });
        const btn = pf.querySelector('[type=submit]');
        pf.addEventListener('submit', action(btn, async () => {
          const f = Object.fromEntries(new FormData(pf));
          await send('PATCH', '/me', f);
          state.me = { ...state.me, display_name: f.display_name.trim(), email: f.email.trim() };
          renderTopbar();
          toast('Profile saved');
        }));
      }
      const pw = $('#pw-form', el);
      if (pw) {
        const err = $('#pw-err', el);
        const btn = pw.querySelector('[type=submit]');
        pw.addEventListener('submit', action(btn, async () => {
          err.hidden = true;
          const f = Object.fromEntries(new FormData(pw));
          const fail = (m) => { err.textContent = m; err.hidden = false; };
          if (f.new.length < 8) return fail('Use at least 8 characters for your new password');
          if (f.new !== f.confirm) return fail("The new passwords don't match");
          try { await send('POST', '/me/password', { current: f.current, new: f.new }); }
          catch (e) { return fail(e.message); }
          pw.reset();
          toast('Password updated');
        }));
      }
      const tf = $('#token-form', el);
      if (tf) {
        const btn = tf.querySelector('[type=submit]');
        tf.addEventListener('submit', action(btn, async () => {
          const f = Object.fromEntries(new FormData(tf));
          const name = f.name.trim();
          if (!name) return;
          const r = await send('POST', '/me/tokens', { name, scope: f.scope, repo: f.repo.trim(), expires_days: f.expires_days ? +f.expires_days : null });
          await refresh();
          const box = $('#new-token');
          const example = `git clone ${location.protocol}//${state.me.username}:${r.token}@${location.host}/${state.me.username}/<repo>.git`;
          box.innerHTML = html`<div class="token-reveal fade">
            <div class="row">${icon('check', 'c-green')}<b>Token “${name}” created</b></div>
            <p class="small">Copy it now. For your security it won't be shown again.</p>
            <div class="row"><input class="input mono" readonly value="${r.token}" aria-label="New token" id="token-value"><button class="btn primary" data-copy="${r.token}" data-copy-label="Token copied">${icon('copy', 'sm')} Copy</button></div>
            <p class="small muted">Use it as your password when git asks, or in a URL:</p>
            <pre class="setup-cmd">${example}</pre></div>`.s;
          $('#token-value', box).select();
        }));
        $('.settings-body', el).addEventListener('click', async (e) => {
          const b = e.target.closest('[data-del]');
          if (!b) return;
          const ok = await confirmDialog({ title: `Delete “${b.dataset.name}”?`, text: 'Anything using this token will stop working immediately.', ok: 'Delete token', danger: true });
          if (!ok) return;
          try { await send('DELETE', `/me/tokens/${b.dataset.del}`); } catch (err) { return toast(err.message, 'err'); }
          toast('Token deleted');
          refresh();
        });
      }
      if (tab === 'security') mountSecurity(el);
      el.querySelectorAll('input[name=theme]').forEach((r) => r.addEventListener('change', () => { setTheme(r.value); toast(`Theme: ${r.closest('label').textContent.trim()}`); }));
      el.querySelectorAll('input[name=skin]').forEach((r) => r.addEventListener('change', () => { setSkin(r.value); toast(`Theme: ${SKINS.find((x) => x.id === r.value).name}`); }));
    },
  };
}

function mountSecurity(el) {
  const flow = $('#tfa-flow', el);
  const showCodes = (codes) => {
    flow.innerHTML = codesView(codes).s;
    const blob = new Blob([codes.join('\n') + '\n'], { type: 'text/plain' });
    $('[data-codes-dl]', flow).href = URL.createObjectURL(blob);
    $('[data-codes-copy]', flow).addEventListener('click', () => copy(codes.join('\n'), 'Recovery codes copied'));
    $('[data-done]', flow).addEventListener('click', () => refresh());
  };
  $('.settings-body', el).addEventListener('click', async (e) => {
    const t = e.target;
    const tfa = t.closest('[data-tfa]')?.dataset.tfa;
    try {
      if (tfa === 'setup') {
        const s = await send('POST', '/me/2fa/setup');
        flow.innerHTML = html`<div class="box fade"><div class="box-body stack">
          <b>1. Scan this with your authenticator app</b>
          <div class="tfa-qr">${raw(s.qr)}</div>
          <p class="small muted">Can't scan? Enter this key instead: <code class="mono">${s.secret.replace(/(.{4})/g, '$1 ').trim()}</code></p>
          <form id="tfa-form" class="stack" novalidate><label for="tfa-code"><b>2. Enter the 6-digit code it shows</b></label>
            <div class="row"><input class="input otp" id="tfa-code" inputmode="numeric" autocomplete="one-time-code" maxlength="8" required><button class="btn primary" type="submit">Turn on</button></div>
            <p class="field-err" id="tfa-err" role="alert" hidden></p></form></div></div>`.s;
        const form = $('#tfa-form', flow), err = $('#tfa-err', flow);
        $('#tfa-code', flow).focus();
        form.addEventListener('submit', action(form.querySelector('button'), async () => {
          err.hidden = true;
          try { showCodes((await send('POST', '/me/2fa/enable', { code: $('#tfa-code', flow).value })).recovery_codes); toast('Two-factor authentication is on'); }
          catch (x) { err.textContent = x.message; err.hidden = false; }
        }));
      } else if (tfa === 'codes') {
        if (!(await confirmDialog({ title: 'Make new recovery codes?', text: 'Your old recovery codes will stop working.', ok: 'Make new codes' }))) return;
        showCodes((await send('POST', '/me/2fa/recovery')).recovery_codes);
      } else if (tfa === 'off') {
        if (!(await confirmDialog({ title: 'Turn off two-factor authentication?', text: 'Signing in will only need your password again, and your recovery codes will stop working.', ok: 'Turn off', danger: true }))) return;
        await send('POST', '/me/2fa/disable');
        toast('Two-factor authentication is off');
        refresh();
      } else if (t.closest('[data-revoke]')) {
        await send('DELETE', `/me/sessions/${t.closest('[data-revoke]').dataset.revoke}`);
        toast('Signed out that session');
        refresh();
      } else if (t.closest('[data-revoke-others]')) {
        const r = await send('POST', '/me/sessions/revoke-others');
        toast(`Signed out ${r.revoked} other ${r.revoked === 1 ? 'session' : 'sessions'}`);
        refresh();
      }
    } catch (x) { toast(x.message, 'err'); }
  });
}
