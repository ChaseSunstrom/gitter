// Sign in and sign up.
import { html, state, send, $, action } from '../lib.js';
import { logo } from '../icons.js';
import { navigate, renderTopbar } from '../main.js';

// Keep in sync with RESERVED in src/validate.rs (the server has the final say).
const RESERVED = ['about', 'admin', 'api', 'archive', 'assets', 'ci', 'dashboard', 'explore', 'favicon.ico', 'favicon.svg', 'help', 'login', 'logout', 'new', 'notifications', 'org', 'orgs', 'raw', 'robots.txt', 'runners', 'search', 'settings', 'signup', 'static', 'user', 'users'];

export function usernameError(u) {
  if (!u) return 'Pick a username';
  if (u.length > 39) return 'Usernames are up to 39 characters';
  if (!/^[A-Za-z0-9]/.test(u)) return 'Start with a letter or number';
  if (!/^[A-Za-z0-9._-]+$/.test(u)) return "Use only letters, numbers, '-', '_' or '.'";
  if (u.includes('..')) return "Two dots in a row aren't allowed";
  if (/\.git$/i.test(u)) return "Usernames can't end in .git";
  if (RESERVED.includes(u.toLowerCase())) return 'That name is reserved';
  return '';
}

function strength(pw) {
  if (!pw) return 0;
  let s = pw.length >= 8 ? 1 : 0;
  if (pw.length >= 12) s++;
  if (/[a-z]/.test(pw) && /[A-Z]/.test(pw)) s++;
  if (/\d/.test(pw) || /[^A-Za-z0-9]/.test(pw)) s++;
  if (pw.length >= 16) s++;
  return Math.min(4, s);
}
const STRENGTH = ['Too short', 'Weak', 'Okay', 'Good', 'Strong'];

const next = (ctx) => {
  const n = ctx.query.get('next') || '/';
  return n.startsWith('/') && !n.startsWith('//') ? n : '/';
};

function card(inner) {
  return html`<div class="auth-wrap"><div class="auth-card fade">
    <a href="/" class="auth-logo" aria-label="Gitter home">${logo(44)}</a>${inner}</div></div>`;
}

function loginView(ctx) {
  const q = ctx.url.search;
  return card(html`<h1>Sign in to Gitter</h1>
    <form id="auth-form" class="stack" novalidate>
      <div class="field"><label for="login">Username or email</label><input class="input" id="login" name="login" autocomplete="username" autofocus required></div>
      <div class="field"><label for="password">Password</label><input class="input" id="password" name="password" type="password" autocomplete="current-password" required></div>
      <p class="field-err" id="form-err" role="alert" hidden></p>
      <button class="btn primary lg" type="submit">Sign in</button>
    </form>
    ${state.signup ? html`<p class="auth-alt muted small">New here? <a class="c-accent" href="/signup${q}">Create an account</a></p>` : ''}`);
}

function signupView(ctx) {
  const setup = ctx.query.get('setup') || '';
  if (state.instance?.needs_setup && !setup) {
    return card(html`<h1>Finish setting up Gitter</h1><p class="muted">This server has no accounts yet. To create the admin account, open the setup link printed in the server log
      (with Docker: <code>docker compose logs gitter</code>).</p>`);
  }
  const invite = ctx.query.get('invite') || '';
  if (state.signupMode === 'invite' && !invite && !state.instance?.needs_setup) {
    return card(html`<h1>Sign-up is by invitation</h1><p class="muted">Ask the person who runs this Gitter for an invite link.</p>
      <a class="btn lg" href="/login${ctx.url.search}">Sign in instead</a>`);
  }
  if (!state.signup && !invite && !state.instance?.needs_setup) {
    return card(html`<h1>Sign-ups are closed</h1><p class="muted">This Gitter instance isn't accepting new accounts right now. Ask the person who runs it for an invite.</p>
      <a class="btn lg" href="/login${ctx.url.search}">Sign in instead</a>`);
  }
  return card(html`<h1>${setup ? 'Create the admin account' : 'Create your account'}</h1>
    ${setup ? html`<p class="muted">This first account runs the server: it can manage accounts and settings.</p>` : ''}
    <form id="auth-form" class="stack" novalidate>
      <div class="field"><label for="username">Username</label><input class="input" id="username" name="username" autocomplete="username" autocapitalize="off" spellcheck="false" autofocus required>
        <span class="hint" id="username-msg">Letters, numbers, '-', '_' or '.'. This is your public URL.</span></div>
      <div class="field"><label for="email">Email</label><input class="input" id="email" name="email" type="email" autocomplete="email" required>
        <span class="hint">For signing in and account recovery. Never shown publicly.</span></div>
      <div class="field"><label for="password">Password</label><input class="input" id="password" name="password" type="password" autocomplete="new-password" minlength="8" required>
        <div class="pw-meter" aria-hidden="true"><i></i><i></i><i></i><i></i></div><span class="hint" id="pw-msg">At least 8 characters.</span></div>
      <p class="field-err" id="form-err" role="alert" hidden></p>
      <button class="btn primary lg" type="submit">Create account</button>
    </form>
    <p class="auth-alt muted small">Already have an account? <a class="c-accent" href="/login${ctx.url.search}">Sign in</a></p>`);
}

/** Second sign-in step for accounts with two-factor authentication. */
function codeStep(el, challenge, ctx) {
  const card = $('.auth-card', el);
  card.innerHTML = html`<a href="/" class="auth-logo" aria-label="Gitter home">${logo(44)}</a>
    <h1>Two-factor authentication</h1>
    <p class="muted">Open your authenticator app and enter the 6-digit code for Gitter.</p>
    <form id="code-form" class="stack" novalidate>
      <div class="field"><label for="otp">Authentication code</label><input class="input otp" id="otp" name="code" inputmode="numeric" autocomplete="one-time-code" maxlength="20" autofocus required></div>
      <p class="field-err" id="code-err" role="alert" hidden></p>
      <button class="btn primary lg" type="submit">Verify</button>
    </form>
    <p class="auth-alt muted small">Lost your phone? Enter one of your recovery codes instead.</p>`.s;
  const form = $('#code-form', card), err = $('#code-err', card), input = $('#otp', card);
  input.focus();
  // Submit as soon as six digits are in (typed or pasted).
  input.addEventListener('input', () => { if (/^\d{6}$/.test(input.value.replace(/\s/g, ''))) form.requestSubmit(); });
  const btn = form.querySelector('button[type=submit]');
  form.addEventListener('submit', action(btn, async () => {
    err.hidden = true;
    let res;
    try { res = await send('POST', '/auth/login/2fa', { challenge, code: input.value }); }
    catch (e) {
      if (e.code === 'restart') return navigate(location.pathname + location.search, { replace: true });
      err.textContent = e.message; err.hidden = false; input.select(); return;
    }
    state.me = res.user;
    renderTopbar();
    navigate(next(ctx), { replace: true });
  }));
}

export default async function (ctx) {
  if (state.me) return { redirect: next(ctx) };
  const signup = ctx.params.mode === 'signup';
  return {
    title: signup ? 'Sign up' : 'Sign in',
    html: signup ? signupView(ctx) : loginView(ctx),
    mount(el) {
      const form = $('#auth-form', el);
      if (!form) return;
      const err = $('#form-err', el);
      const showErr = (m) => { err.textContent = m; err.hidden = !m; };
      const user = $('#username', el);
      const pw = $('#password', el);
      if (signup) {
        const umsg = $('#username-msg', el);
        user.addEventListener('input', () => {
          const e = user.value ? usernameError(user.value.trim()) : '';
          umsg.textContent = e || (user.value ? `Your profile will be ${location.host}/${user.value.trim()}` : "Letters, numbers, '-', '_' or '.'. This is your public URL.");
          umsg.className = e ? 'err' : 'hint';
          user.setAttribute('aria-invalid', e ? 'true' : 'false');
        });
        const meter = $('.pw-meter', el);
        const pmsg = $('#pw-msg', el);
        pw.addEventListener('input', () => {
          const s = pw.value.length < 8 ? 0 : strength(pw.value);
          meter.dataset.s = pw.value ? s : '';
          pmsg.textContent = pw.value ? STRENGTH[s] : 'At least 8 characters.';
        });
      }
      const btn = form.querySelector('button[type=submit]');
      form.addEventListener('submit', action(btn, async () => {
        showErr('');
        const f = Object.fromEntries(new FormData(form));
        let res;
        if (signup) {
          const ue = usernameError(f.username.trim());
          if (ue) { user.focus(); return showErr(ue); }
          if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(f.email.trim())) return showErr('Enter a valid email address');
          if (f.password.length < 8) { pw.focus(); return showErr('Use at least 8 characters for your password'); }
          try { res = await send('POST', '/auth/signup', { username: f.username.trim(), email: f.email.trim(), password: f.password, setup: ctx.query.get('setup') || '', invite: ctx.query.get('invite') || '' }); }
          catch (e) { return showErr(e.message); }
        } else {
          if (!f.login.trim() || !f.password) return showErr('Enter your username and password');
          try { res = await send('POST', '/auth/login', { login: f.login.trim(), password: f.password }); }
          catch (e) { pw.select(); return showErr(e.message); }
          if (res.two_factor) return codeStep(el, res.challenge, ctx);
        }
        state.me = res.user;
        renderTopbar();
        navigate(next(ctx), { replace: true });
      }));
    },
  };
}

