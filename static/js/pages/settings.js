// Account settings: profile, account (password, export, delete), repositories (transfers, trash), access tokens, appearance.
import { html, raw, state, get, send, $, action, toast, timeEl, confirmDialog, avatar, copy, auditLabel, deviceName, bytes, plural, busy, startOAuth } from '../lib.js';
import { icon } from '../icons.js';
import { refresh, renderTopbar, setTheme, getTheme, SKINS, getSkin, setSkin } from '../main.js';

const TABS = [['profile', 'user', 'Profile'], ['account', 'shield', 'Account'], ['repositories', 'repo', 'Repositories'], ['security', 'lock', 'Security'], ['tokens', 'key', 'Access tokens'], ['ssh', 'terminal', 'SSH keys'], ['signing', 'edit', 'Signing keys'], ['appearance', 'sun', 'Appearance']];

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

function accountTab(exp, hasPassword) {
  return html`<form id="pw-form" class="stack settings-form" novalidate>
    <h2>${hasPassword ? 'Change password' : 'Set a password'}</h2>
    ${hasPassword
      ? html`<div class="field"><label for="current">Current password</label><input class="input" id="current" name="current" type="password" autocomplete="current-password" required></div>`
      : html`<p class="muted">You sign in through a linked account. A password lets you sign in without it, and with git over HTTPS.</p>`}
    <div class="field"><label for="new">New password</label><input class="input" id="new" name="new" type="password" autocomplete="new-password" minlength="8" required><span class="hint">At least 8 characters.</span></div>
    <div class="field"><label for="confirm">Confirm new password</label><input class="input" id="confirm" name="confirm" type="password" autocomplete="new-password" required></div>
    <p class="field-err" id="pw-err" role="alert" hidden></p>
    <div class="row"><button class="btn primary" type="submit">${hasPassword ? 'Update password' : 'Set password'}</button></div>
  </form>
  <div class="stack settings-form" style="margin-top:28px">
    <h2>Export your data</h2>
    <p class="muted">Download a .tar.gz with your profile, a git bundle of each repository you own (and its wiki), your snippets, the issues, pull requests, comments and discussions you wrote, and your security log. It's ready to download for a day.</p>
    ${exportBox(exp)}
  </div>
  <div class="stack settings-form" style="margin-top:28px">
    <h2>Delete account</h2>
    <div class="box danger-zone"><div class="list"><div class="list-row"><div class="grow"><b>Delete your account</b>
      <div class="muted small">Deletes your profile, every repository you own (also the ones in Recently deleted), your snippets, and the issues, pull requests and comments you wrote. This can't be undone.</div></div>
      <button class="btn danger" id="del-me">${icon('trash', 'sm')} Delete account</button></div></div></div>
  </div>`;
}

function exportBox(e) {
  if (!e) return html`<div class="row"><button class="btn" data-export>${icon('download', 'sm')} Export your data</button></div>`;
  if (e.status === 'running') {
    return html`<div class="box"><div class="box-body row">${icon('clock', 'muted')}<span class="grow"><b>Preparing your export…</b> <span class="muted small">Started ${timeEl(e.created_at)}. This page updates when it's ready.</span></span></div></div>`;
  }
  if (e.status === 'ready') {
    return html`<div class="box"><div class="box-body row wrap-ok">${icon('check', 'c-green')}<span class="grow"><b>Your export is ready</b> <span class="muted small">${bytes(e.size)} · can be downloaded until ${timeEl(e.expires_at)}</span></span>
      <a class="btn primary" href="/api/me/export/download" download>${icon('download', 'sm')} Download</a><button class="btn" data-export>Make a new one</button></div></div>`;
  }
  return html`<div class="box"><div class="box-body row">${icon('x', 'c-red')}<span class="grow"><b>The export didn't finish</b> <span class="muted small">${e.error}</span></span><button class="btn" data-export>Try again</button></div></div>`;
}

function reposTab(tr, trash) {
  const incoming = tr.incoming.map((t) => html`<div class="list-row" data-offer="${t.id}">${icon('repo', 'muted')}
    <div class="grow"><b>${t.repo}</b>${t.private ? html` <span class="tag-private">Private</span>` : ''}
      <div class="small muted">${t.from} offered it to you · the offer ends ${timeEl(t.expires_at)}</div></div>
    <button class="btn sm" data-decline>Decline</button><button class="btn sm primary" data-accept>Accept</button></div>`);
  const outgoing = tr.outgoing.map((t) => html`<div class="list-row">${icon('repo', 'muted')}
    <div class="grow"><b>${t.repo}</b><div class="small muted">Offered to ${t.to} · the offer ends ${timeEl(t.expires_at)}</div></div>
    <a class="btn sm" href="/${t.repo}/settings">Manage</a></div>`);
  const deleted = trash.repos.map((r) => html`<div class="list-row" data-trash="${r.id}">${icon('trash', 'muted')}
    <div class="grow"><b>${r.name}</b>${r.private ? html` <span class="tag-private">Private</span>` : ''}
      <div class="small muted">Deleted ${timeEl(r.deleted_at)} · gone for good ${timeEl(r.purge_at)}${r.name_taken ? ` · you have another repository named ${r.name}: rename it to restore this one` : ''}</div></div>
    <button class="btn sm" data-restore ${r.name_taken ? 'disabled' : ''}>Restore</button></div>`);
  return html`<div class="stack settings-form">
    <h2>Transfers</h2>
    <p class="muted">Repositories someone offered to you. Accepting makes you the owner: issues, pull requests, stars and history come with it, and its old address leads to its new one. Offers end after ${plural(state.instance?.transfer_offer_days ?? 7, 'day')}.</p>
    ${incoming.length ? html`<div class="box list">${incoming}</div>` : html`<p class="muted small">No offers waiting for you.</p>`}
    ${outgoing.length ? html`<h3 class="section-title">Offers you made</h3><div class="box list">${outgoing}</div>` : ''}
    <h2 style="margin-top:18px">Recently deleted</h2>
    <p class="muted">Deleted repositories stay here for ${plural(state.instance?.trash_days ?? 7, 'day')}, then they're gone for good. Until then they still count toward your storage.</p>
    ${deleted.length ? html`<div class="box list">${deleted}</div>` : html`<p class="muted small">Nothing deleted lately.</p>`}
  </div>`;
}

const SCOPES = { read: 'Read only', write: 'Read and write', all: 'Full access' };

function tokensTab({ tokens, agent_tools: tools = [] }) {
  const toolName = Object.fromEntries(tools.map((t) => [t.name, t.title]));
  return html`<div class="stack settings-form">
    <h2>Personal access tokens</h2>
    <p class="muted">Tokens work like passwords for git over HTTPS and for the API. Give each one a name so you know where it's used, and delete any you no longer need.
      An <b>agent token</b> lets an AI agent work through Gitter's MCP server at <code>/mcp</code>, only in the repositories and with the tools you pick. It can't change settings, see secrets or tokens, commit outside the branches agents start, or merge; its CI runs get no secrets and never run on this server, and what it does is marked as an agent's.</p>
    <form id="token-form" class="box"><div class="box-body stack">
      <div class="field"><label for="tk-name">Name</label><input class="input" id="tk-name" name="name" placeholder="What's this token for? e.g. laptop" maxlength="80" required></div>
      <div class="grid-2">
        <div class="field"><label for="tk-scope">Access</label><select class="select" id="tk-scope" name="scope">
          <option value="write" selected>Read and write: clone, push, issues, pull requests</option>
          <option value="read">Read only: clone and view</option>
          <option value="all">Full access: everything your account can do</option>
          <option value="agent">Agent (MCP): chosen repositories and tools</option></select></div>
        <div class="field"><label for="tk-exp">Expires</label><select class="select" id="tk-exp" name="expires_days">
          <option value="30">In 30 days</option><option value="90" selected>In 90 days</option><option value="365">In a year</option><option value="">Never</option></select></div>
      </div>
      <div class="field" id="tk-one"><label for="tk-repo">Only for one repository <span class="muted">(optional)</span></label><input class="input" id="tk-repo" name="repo" placeholder="owner/name, or leave empty for all your repositories" autocapitalize="off" spellcheck="false"></div>
      <div class="stack" id="tk-agent" hidden>
        <div class="field"><label for="tk-repos">Repositories</label><input class="input" id="tk-repos" name="repos" placeholder="owner/name owner/other (up to 20)" autocapitalize="off" spellcheck="false"></div>
        <div class="field" role="group" aria-labelledby="tk-tools-l"><label id="tk-tools-l">Tools</label>
          <div class="row wrap-ok">${tools.map((t) => html`<label class="check small"><input type="checkbox" name="tool" value="${t.name}" checked> ${t.title}</label>`)}</div></div>
      </div>
      <div class="row"><button class="btn primary" type="submit">${icon('plus', 'sm')} Generate token</button></div>
    </div></form>
    <div id="new-token"></div>
    ${tokens.length ? html`<div class="box list">${tokens.map((t) => html`<div class="list-row token-row">
        ${icon('key', 'muted')}<div class="grow"><b>${t.name}</b> ${t.kind === 'agent' ? html`<span class="pill agent-badge">Agent</span>${t.repos.map((r) => html` <span class="pill">${r}</span>`)}` : html`<span class="pill">${SCOPES[t.scope] || t.scope}</span>`}${t.repo ? html` <span class="pill">${t.repo}</span>` : ''}${t.expired ? html` <span class="pill closed">Expired</span>` : ''}
          ${t.kind === 'agent' ? html`<div class="small muted">Tools: ${t.tools.map((x) => toolName[x] || x).join(', ')}</div>` : ''}
          <div class="small muted">Created ${timeEl(t.created_at)} · ${t.last_used ? html`Last used ${timeEl(t.last_used)}` : 'Never used'}${t.expires_at && !t.expired ? html` · expires ${timeEl(t.expires_at)}` : ''}</div></div>
        <button class="btn sm danger" data-del="${t.id}" data-name="${t.name}">${icon('trash', 'sm')} Delete</button></div>`)}</div>`
      : html`<div class="box empty small">${icon('key', 'big')}<h3>No tokens yet</h3><p>Generate one to push from scripts or other machines.</p></div>`}
  </div>`;
}

function signingTab(keys, ch) {
  const me = state.me;
  const text = ch.challenge;
  return html`<div class="stack settings-form">
    <h2>Signing keys</h2>
    <p class="muted">Commits you sign with one of these keys show as <b>Verified</b> when their committer email is your account's email
      (or <code>${me.id}+${me.username}@users.noreply.gitter</code>). Paste an SSH public key (the <code>.pub</code> file) or a GPG public key (<code>gpg --armor --export you@example.com</code>).</p>
    <form id="sk-form" class="box"><div class="box-body stack">
      <div class="field"><label for="sk-title">Title <span class="muted">(optional)</span></label><input class="input" id="sk-title" name="title" maxlength="80" placeholder="e.g. laptop"></div>
      <div class="field"><label for="sk-key">Public key</label><textarea class="textarea mono" id="sk-key" name="key" rows="4" required spellcheck="false" placeholder="ssh-ed25519 AAAA… or -----BEGIN PGP PUBLIC KEY BLOCK-----"></textarea>
        <span class="hint">To sign with an SSH key: <code>git config gpg.format ssh</code>, <code>git config user.signingkey ~/.ssh/id_ed25519.pub</code>, then <code>git commit -S</code>.</span></div>
      <div class="field"><label for="sk-sig">Signature</label>
        <p class="small muted">Prove the key is yours: sign this text with it (it works once, for an hour) and paste the signature.</p>
        <code class="mono small" id="sk-challenge">${text}</code>
        <span class="hint">SSH: <code>printf '%s' '${text}' | ssh-keygen -Y sign -n ${ch.ssh_namespace} -f ~/.ssh/id_ed25519</code><br>
          GPG: <code>printf '%s' '${text}' | gpg --armor --detach-sign -u you@example.com</code></span>
        <textarea class="textarea mono" id="sk-sig" name="signature" rows="4" required spellcheck="false" placeholder="-----BEGIN SSH SIGNATURE----- or -----BEGIN PGP SIGNATURE-----"></textarea></div>
      <div class="row"><button class="btn primary" type="submit">${icon('plus', 'sm')} Add key</button></div>
    </div></form>
    ${keys.length ? html`<div class="box list">${keys.map((k) => html`<div class="list-row signing-key">
        ${icon('key', 'muted')}<div class="grow"><b>${k.title}</b> <span class="pill">${k.kind === 'ssh' ? 'SSH' : 'GPG'}</span>
          ${k.proved_at ? html`<span class="pill" title="You signed a challenge with it when adding it">Proved</span>`
            : html`<span class="pill muted" title="Added before keys needed a signature proving you hold them. Remove it and add it again to prove it.">Not proved</span>`}
          <div class="small muted"><code class="mono">${k.fingerprint}</code> · added ${timeEl(k.created_at)}</div></div>
        <button class="btn sm danger" data-sk-del="${k.id}" data-name="${k.title}">${icon('trash', 'sm')} Remove</button></div>`)}</div>`
      : html`<div class="box empty small">${icon('key', 'big')}<h3>No signing keys yet</h3><p>Add one to have your signed commits show as Verified.</p></div>`}
  </div>`;
}

function sshTab(r) {
  return html`<div class="stack settings-form">
    <h2>SSH keys</h2>
    <p class="muted">Clone, fetch and push over SSH with these keys, without a password or token. Paste a public key (the <code>.pub</code> file, e.g. <code>~/.ssh/id_ed25519.pub</code>); make one with <code>ssh-keygen -t ed25519</code>.</p>
    ${r.enabled ? '' : html`<div class="box"><div class="box-body small muted">${icon('lock', 'sm')} Git over SSH is off on this server: the site admin turns it on with <code>GITTER_SSH_ADDR</code>. Until then, use HTTPS.</div></div>`}
    <form id="ssh-form" class="box"><div class="box-body stack">
      <div class="field"><label for="ssh-title">Title <span class="muted">(optional)</span></label><input class="input" id="ssh-title" name="title" maxlength="80" placeholder="e.g. laptop"></div>
      <div class="field"><label for="ssh-key">Public key</label><textarea class="textarea mono" id="ssh-key" name="key" rows="3" required spellcheck="false" placeholder="ssh-ed25519 AAAA…"></textarea>
        <span class="hint">Ed25519, ECDSA or RSA (2048 bits or more). A key can belong to one account only.</span></div>
      <div class="row"><button class="btn primary" type="submit">${icon('plus', 'sm')} Add key</button></div>
    </div></form>
    ${r.keys.length ? html`<div class="box list">${r.keys.map((k) => html`<div class="list-row ssh-key">
        ${icon('key', 'muted')}<div class="grow"><b>${k.title}</b>
          <div class="small muted"><code class="mono">${k.fingerprint}</code> · added ${timeEl(k.created_at)} · ${k.last_used ? html`last used ${timeEl(k.last_used)}` : 'never used'}</div></div>
        <button class="btn sm danger" data-ssh-del="${k.id}" data-name="${k.title}">${icon('trash', 'sm')} Remove</button></div>`)}</div>`
      : html`<div class="box empty small">${icon('key', 'big')}<h3>No SSH keys yet</h3><p>Add one to use git over SSH.</p></div>`}
  </div>`;
}

/** Password and linked sign-in accounts (GitHub, GitLab, OpenID Connect), when an admin added providers. */
function methodsView(ids, providers) {
  const linked = new Map(ids.identities.map((i) => [i.provider.slug, i]));
  const row = (p, i) => html`<div class="list-row">${icon('key', 'muted')}<div class="grow"><b>${p.name}</b>
      <div class="small muted">${i ? html`Linked${i.login ? html` as ${i.login}` : ''} ${timeEl(i.created_at)}${i.provider.enabled ? '' : ' · turned off by an admin'}` : 'Not linked'}</div></div>
      ${i ? html`<button class="btn sm" data-unlink="${i.id}" data-name="${p.name}" aria-label="Unlink ${p.name}">Unlink</button>` : html`<button class="btn sm" data-link="${p.slug}" aria-label="Link ${p.name}">Link</button>`}</div>`;
  const rows = providers.map((p) => { const i = linked.get(p.slug); linked.delete(p.slug); return row(p, i); });
  // Accounts at providers an admin turned off still show, so they can be unlinked.
  for (const i of linked.values()) rows.push(row(i.provider, i));
  if (!rows.length) return '';
  return html`<h2>Sign-in methods</h2>
    <div class="box list" id="sign-in-methods"><div class="list-row">${icon('lock', 'muted')}<div class="grow"><b>Password</b>
      <div class="small muted">${ids.has_password ? 'Set' : 'Not set: you sign in through a linked account'}</div></div>
      <a class="btn sm" href="/settings/account">${ids.has_password ? 'Change' : 'Set a password'}</a></div>${rows}</div>`;
}

function securityTab(sec, log, ids, providers) {
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
    ${methodsView(ids, providers)}
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
  let inner, exp = null, timer;
  if (tab === 'profile') { const m = await get('/me', { fresh: true }); inner = profileTab(m.profile, m.storage); }
  else if (tab === 'tokens') inner = tokensTab(await get('/me/tokens', { fresh: true }));
  else if (tab === 'signing') {
    const [r, ch] = await Promise.all([get('/me/signing-keys', { fresh: true }), send('POST', '/me/signing-keys/challenge')]);
    inner = signingTab(r.keys, ch);
  }
  else if (tab === 'ssh') inner = sshTab(await get('/me/ssh-keys', { fresh: true }));
  else if (tab === 'account') {
    const [e, ids] = await Promise.all([get('/me/export', { fresh: true }), get('/me/identities', { fresh: true })]);
    exp = e.export;
    inner = accountTab(exp, ids.has_password);
  }
  else if (tab === 'repositories') {
    const [tr, trash] = await Promise.all([get('/me/transfers', { fresh: true }), get('/me/trash', { fresh: true })]);
    inner = reposTab(tr, trash);
  }
  else if (tab === 'security') {
    const [sec, log, ids, prov] = await Promise.all([get('/me/security', { fresh: true }), get('/me/audit', { fresh: true }), get('/me/identities', { fresh: true }), get('/auth/providers')]);
    inner = securityTab(sec, log, ids, prov.providers);
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
          try { await send('POST', '/me/password', { current: f.current || '', new: f.new }); }
          catch (e) { return fail(e.message); }
          pw.reset();
          toast(f.current === undefined ? 'Password set' : 'Password updated');
          if (f.current === undefined) refresh();
        }));
      }
      const tf = $('#token-form', el);
      if (tf) {
        const btn = tf.querySelector('[type=submit]');
        const scopeSel = $('#tk-scope', tf);
        const showKind = () => { const ag = scopeSel.value === 'agent'; $('#tk-agent', tf).hidden = !ag; $('#tk-one', tf).hidden = ag; };
        scopeSel.addEventListener('change', showKind);
        tf.addEventListener('submit', action(btn, async () => {
          const fd = new FormData(tf);
          const f = Object.fromEntries(fd);
          const name = f.name.trim();
          if (!name) return;
          const agent = f.scope === 'agent';
          const expires_days = f.expires_days ? +f.expires_days : null;
          const r = agent
            ? await send('POST', '/me/tokens', { name, kind: 'agent', repos: f.repos.split(/[\s,]+/).filter(Boolean), tools: fd.getAll('tool'), expires_days })
            : await send('POST', '/me/tokens', { name, scope: f.scope, repo: f.repo.trim(), expires_days });
          await refresh();
          const box = $('#new-token');
          const example = agent
            ? JSON.stringify({ mcpServers: { gitter: { type: 'http', url: `${location.origin}/mcp`, headers: { Authorization: `Bearer ${r.token}` } } } }, null, 2)
            : `git clone ${location.protocol}//${state.me.username}:${r.token}@${location.host}/${state.me.username}/<repo>.git`;
          box.innerHTML = html`<div class="token-reveal fade">
            <div class="row">${icon('check', 'c-green')}<b>Token “${name}” created</b></div>
            <p class="small">Copy it now. For your security it won't be shown again.</p>
            <div class="row"><input class="input mono" readonly value="${r.token}" aria-label="New token" id="token-value"><button class="btn primary" data-copy="${r.token}" data-copy-label="Token copied">${icon('copy', 'sm')} Copy</button></div>
            <p class="small muted">${agent ? 'Add Gitter to your agent\'s MCP servers, for example:' : 'Use it as your password when git asks, or in a URL:'}</p>
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
      const sk = $('#sk-form', el);
      if (sk) {
        sk.addEventListener('submit', action(sk.querySelector('[type=submit]'), async () => {
          const f = Object.fromEntries(new FormData(sk));
          if (!f.key.trim() || !f.signature.trim()) return;
          await send('POST', '/me/signing-keys', { title: f.title, key: f.key, signature: f.signature });
          toast('Signing key added');
          refresh();
        }));
        $('.settings-body', el).addEventListener('click', async (e) => {
          const b = e.target.closest('[data-sk-del]');
          if (!b) return;
          const ok = await confirmDialog({ title: `Remove “${b.dataset.name}”?`, text: 'Commits signed with this key will show as Unverified.', ok: 'Remove key', danger: true });
          if (!ok) return;
          try { await send('DELETE', `/me/signing-keys/${b.dataset.skDel}`); } catch (err) { return toast(err.message, 'err'); }
          toast('Signing key removed');
          refresh();
        });
      }
      const sf = $('#ssh-form', el);
      if (sf) {
        sf.addEventListener('submit', action(sf.querySelector('[type=submit]'), async () => {
          const f = Object.fromEntries(new FormData(sf));
          if (!f.key.trim()) return;
          await send('POST', '/me/ssh-keys', { title: f.title, key: f.key });
          toast('SSH key added');
          refresh();
        }));
        $('.settings-body', el).addEventListener('click', async (e) => {
          const b = e.target.closest('[data-ssh-del]');
          if (!b) return;
          const ok = await confirmDialog({ title: `Remove “${b.dataset.name}”?`, text: 'Git over SSH with this key will stop working immediately.', ok: 'Remove key', danger: true });
          if (!ok) return;
          try { await send('DELETE', `/me/ssh-keys/${b.dataset.sshDel}`); } catch (err) { return toast(err.message, 'err'); }
          toast('SSH key removed');
          refresh();
        });
      }
      if (tab === 'security') mountSecurity(el);
      if (tab === 'repositories') mountRepos(el);
      if (tab === 'account') {
        // Check back until a running export is ready.
        if (exp?.status === 'running') timer = setTimeout(() => refresh(), 2000);
        el.querySelectorAll('[data-export]').forEach((b) => b.addEventListener('click', action(b, async () => {
          await send('POST', '/me/export');
          toast('Preparing your export');
          refresh();
        })));
        const del = $('#del-me', el);
        del.addEventListener('click', action(del, async () => {
          const name = state.me.username;
          const ok = await confirmDialog({ title: 'Delete your account?', text: `This deletes ${name}, every repository you own and everything you wrote. It can't be undone.`, ok: 'Delete my account', danger: true, typed: name });
          if (!ok) return;
          await send('DELETE', '/me', { confirm: name });
          location.href = '/';
        }));
      }
      el.querySelectorAll('input[name=theme]').forEach((r) => r.addEventListener('change', () => { setTheme(r.value); toast(`Theme: ${r.closest('label').textContent.trim()}`); }));
      el.querySelectorAll('input[name=skin]').forEach((r) => r.addEventListener('change', () => { setSkin(r.value); toast(`Theme: ${SKINS.find((x) => x.id === r.value).name}`); }));
    },
    unmount() { clearTimeout(timer); },
  };
}

function mountRepos(el) {
  $('.settings-body', el).addEventListener('click', async (e) => {
    const b = e.target.closest('[data-accept], [data-decline], [data-restore]');
    if (!b) return;
    const row = b.closest('.list-row');
    await action(b, async () => {
      if (b.matches('[data-accept]')) {
        const r = await send('POST', `/me/transfers/${row.dataset.offer}/accept`);
        toast(`You now own ${r.owner}/${r.name}`);
      } else if (b.matches('[data-decline]')) {
        await send('POST', `/me/transfers/${row.dataset.offer}/decline`);
        toast('Offer declined');
      } else {
        const r = await send('POST', `/me/trash/${row.dataset.trash}/restore`);
        toast(`Restored ${r.owner}/${r.name}`);
      }
      refresh();
    })();
  });
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
      } else if (t.closest('[data-link]')) {
        const b = t.closest('[data-link]');
        busy(b, true);
        // Stays busy while the browser leaves for the provider.
        try { await startOAuth(b.dataset.link, 'link'); } catch (x) { busy(b, false); throw x; }
      } else if (t.closest('[data-unlink]')) {
        const b = t.closest('[data-unlink]');
        if (!(await confirmDialog({ title: `Unlink ${b.dataset.name}?`, text: `You won't be able to sign in with ${b.dataset.name} until you link it again.`, ok: 'Unlink', danger: true }))) return;
        await send('DELETE', `/me/identities/${b.dataset.unlink}`);
        toast(`Unlinked ${b.dataset.name}`);
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
