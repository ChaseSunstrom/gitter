// Visits every page type in a headless browser and fails on console errors
// or missing content. Run by scripts/e2e.sh against its seeded server.
import { chromium } from 'playwright-core';
import os from 'node:os';
import fs from 'node:fs';
import { execFileSync } from 'node:child_process';

const BASE = process.env.BASE;
// Use $CHROME, else the newest Chromium that Playwright has downloaded.
const pwDir = `${os.homedir()}/.cache/ms-playwright`;
const found = fs.existsSync(pwDir) ? fs.readdirSync(pwDir).filter((d) => /^chromium-\d+$/.test(d)).sort().reverse()[0] : null;
const exe = process.env.CHROME || (found ? `${pwDir}/${found}/chrome-linux64/chrome` : undefined);
const browser = await chromium.launch({ executablePath: exe });
let failed = 0;

async function visit(ctx, path, expect, { dark = false, allow404 = false } = {}) {
  const page = await ctx.newPage();
  if (dark) await page.emulateMedia({ colorScheme: 'dark' });
  const errors = [];
  page.on('console', (m) => { if (m.type() === 'error' && !(allow404 && /status of 404/.test(m.text()))) errors.push(m.text()); });
  page.on('pageerror', (e) => errors.push(String(e)));
  const t0 = Date.now();
  await page.goto(BASE + path, { waitUntil: 'networkidle' });
  const ms = Date.now() - t0;
  const text = await page.locator('#page').innerText().catch(() => '');
  const missing = expect.filter((s) => !text.includes(s));
  if (errors.length || missing.length) {
    failed++;
    console.log(`✗ ${path}${errors.length ? '\n    console: ' + errors.join('\n    console: ') : ''}${missing.length ? '\n    missing: ' + missing.join(', ') : ''}`);
  } else console.log(`  ok ${path} (${ms} ms)`);
  await page.close();
}

const anon = await browser.newContext();
await visit(anon, '/', ['Gitter']);
await visit(anon, '/explore', ['hello']);
await visit(anon, '/login', ['Sign in']);
await visit(anon, '/signup', ['Create']);
await visit(anon, '/alice/hello', ['README.md', 'src']);
await visit(anon, '/no-such-user-xyz', ['404'], { allow404: true });

const ctx = await browser.newContext();
const r = await ctx.request.post(BASE + '/api/auth/login', { headers: { Origin: BASE }, data: { login: 'alice', password: 'correct horse' } });
if (!r.ok()) { console.log('✗ login failed', r.status()); process.exit(1); }
const sha = (await (await ctx.request.get(BASE + '/api/repos/alice/hello/commits?rev=main')).json()).commits.at(-1).sha;
const snip = (await (await ctx.request.get(BASE + '/api/snippets')).json()).items[0].id;
const hook = (await (await ctx.request.get(BASE + '/api/repos/alice/hello/hooks')).json()).hooks[0];

const pages = [
  ['/', ['Your repositories']],
  ['/alice', ['alice']],
  ['/alice?tab=stars', ['alice']],
  ['/alice/hello', ['README.md', 'src', 'Releases', 'Version one', 'Latest']],
  ['/alice/hello/packages', ['Packages', 'alice/hello', 'docker pull', 'v1', 'Publish an image', 'docker login']],
  ['/alice/hello/releases', ['Draft a new release', 'Next up', 'Draft', 'Version one', 'Latest', 'First stable release', 'hello.txt', '1 download', 'Source code (zip)', 'Source code (tar.gz)']],
  ['/alice/hello/releases/1', ['All releases', 'Version one', 'v1.0', 'hello.txt', 'Source code (zip)']],
  ['/alice/hello/releases/new', ['New release', 'New tag at', 'Release notes', 'This is a pre-release', 'Save draft', 'Publish release']],
  ['/alice/hello/releases/1/edit', ['Edit Version one', 'hello.txt', 'Delete release', 'Update release']],
  ['/alice/hello/tree/main/src', ['main.rs']],
  ['/alice/hello/blob/main/src/main.rs', ['println']],
  ['/alice/hello/blob/main/README.md', ['hello']],
  ['/alice/hello/blame/main/src/main.rs', ['println']],
  ['/alice/hello/commits', ['Commits']],
  [`/alice/hello/commit/${sha}`, ['changed', 'README.md']],
  ['/alice/hello/branches', ['main']],
  ['/alice/hello/tags', ['Tags']],
  ['/alice/hello/issues?state=closed', ['Crash on start']],
  ['/alice/hello/issues/1', ['Crash on start', 'Looking into it']],
  ['/alice/hello/pulls?state=closed', ['Add lib']],
  ['/alice/hello/pull/2', ['Add lib', 'approved these changes', 'Ship it', 'Looks good here', 'Reviewers']],
  ['/alice/hello/pull/2/files', ['src/lib.rs', 'Looks good here', 'Review changes']],
  ['/alice/hello/pull/2/commits', ['Add lib']],
  ['/alice/hello/compare/main...pw-branch', []],
  ['/alice/hello/labels', ['bug']],
  ['/alice/hello/issues/new', []],
  ['/alice/hello/insights', ['Alice', 'Traffic', 'Referring sites', 'example.org', 'Popular pages']],
  ['/alice/hello/commits?graph=1', ['Commits', 'on every branch', 'Add lib']],
  ['/alice/hello/history/main/README.md', ['History for', 'More docs', 'View file']],
  ['/alice/hello/search?q=println', ['src/main.rs']],
  ['/alice/hello/settings', ['Danger zone', 'CI secrets', 'Webhooks', hook.url, 'Issues', 'Who has access', 'Site admins', 'Transfer ownership', 'Recently deleted']],
  [`/alice/hello/settings/hooks/${hook.id}`, ['Payload URL', 'Recent deliveries', 'ping', 'issues', 'Delivered', 'Send a ping']],
  ['/alice/hello/settings/hooks/new', ['Add webhook', 'Payload URL', 'Allow private addresses']],
  ['/alice/secret/settings', ['Who has access', 'bob']],
  ['/acme', ['Acme Inc', 'Organization', 'People', 'tools']],
  ['/orgs/acme/settings', ['What members can do', 'Members', 'bob']],
  ['/orgs/new', ['Create an organization']],
  // Renamed in e2e.sh: the old page address redirects to the new one.
  ['/alice/old-name', ['new-name', 'README.md']],
  ['/alice/hello/ci', ['CI runs', 'Add CI', '#1']],
  ['/alice/hello/ci/1', ['Add CI', 'build', 'test', 'Say hello', 'Set up job', 'Passed']],
  ['/admin/runners', ['Runners', 'built-in', 'Add a runner']],
  ['/alice/hello/edit/main/README.md', ['Commit changes']],
  ['/alice/hello/new/main', ['Commit changes']],
  ['/alice/hello/upload/main', ['Drag files']],
  ['/alice/secret', ['README.md']],
  ['/new', []],
  ['/explore?sort=stars', ['hello']],
  ['/settings', []],
  ['/settings/tokens', ['cli']],
  ['/settings/signing', ['Signing keys', 'Public key']],
  ['/settings/security', ['Two-factor authentication', 'This browser', 'Security log']],
  ['/admin/audit', ['The log is intact', 'Created the account']],
  ['/settings/appearance', []],
  ['/settings/account', ['Change password', 'Export your data', 'Your export is ready', 'Delete your account']],
  ['/settings/repositories', ['Transfers', 'bob/present', 'Accept', 'Recently deleted', 'scratch']],
  ['/admin', ['Site administration', 'Storage', 'Recently deleted repositories', 'alice/scratch']],
  ['/admin/users', ['alice', 'bob']],
  ['/admin/settings', ['Instance name', 'Largest Git LFS file', 'Jobs the built-in runner runs at once']],
  ['/alice/hello/wiki', ['Hello wiki', 'Setup', 'Pages']],
  ['/alice/hello/wiki/Setup', ['Run cargo build']],
  ['/alice/hello/wiki/Home/edit', ['Save page', 'Editing Home']],
  ['/alice/hello/wiki/Home/history', ['History of', 'Create Home']],
  ['/alice/hello/wiki/_new', ['New page']],
  ['/alice/secret/wiki', ['Welcome to the wiki', 'Create the first page']],
  ['/alice/hello/discussions', ['How do I run it?', 'Answered', 'Q&A']],
  ['/alice/hello/discussions?category=ideas', ['No discussions yet']],
  ['/alice/hello/discussions/1', ['How do I run it?', 'Use cargo run', 'Answered by', 'Reply']],
  ['/alice/hello/discussions/new', ['Start a discussion', 'Announcements']],
  ['/snippets', ['Greeting', 'hello.py']],
  ['/snippets/mine', ['Your snippets', 'Greeting']],
  ['/snippets/new', ['New snippet', 'Add file']],
  [`/snippets/${snip}`, ['Greeting', 'print', '1 revision']],
  [`/snippets/${snip}/edit`, ['Edit snippet', 'Save changes']],
  [`/snippets/${snip}/revisions`, ['Create snippet']],
];
// Pushed by e2e.sh when the git-lfs client is available.
if ((await ctx.request.get(BASE + '/api/repos/alice/media')).ok()) pages.push(['/alice/media/blob/main/big.bin', ['Stored with Git LFS', 'Download', 'Git LFS']]);
for (const [p, e] of pages) await visit(ctx, p, e);
await visit(ctx, '/alice/hello/pull/2/files', ['src/lib.rs'], { dark: true });

// Signed commits (e2e.sh pushes one when ssh-keygen is installed): the key is listed in
// settings; the list, the commit page and blame show Verified, and the badge says why.
if ((await (await ctx.request.get(BASE + '/api/me/signing-keys')).json()).keys.length) {
  await visit(ctx, '/settings/signing', ['Signing keys', 'e2e laptop', 'SSH']);
  const log = (await (await ctx.request.get(BASE + '/api/repos/alice/hello/commits?rev=main')).json()).commits;
  const signed = log.find((c) => c.subject === 'Signed commit')?.sha;
  const page = await ctx.newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e)));
  const badges = {};
  for (const [name, path] of [['list', '/alice/hello/commits'], ['commit', `/alice/hello/commit/${signed}`], ['blame', '/alice/hello/blame/main/README.md']]) {
    await page.goto(BASE + path, { waitUntil: 'networkidle' });
    badges[name] = await page.locator('.sig-badge.verified').count();
  }
  await page.locator('button.sig-badge.verified').first().click();
  const why = await page.locator('.sig-pop').innerText().catch(() => '');
  if (errors.length || !badges.list || !badges.commit || !badges.blame || !why.includes("alice's SSH key")) { failed++; console.log('✗ signature badges', { signed, badges, why, errors }); }
  else console.log('  ok signing keys are listed; commits, the commit page and blame show Verified, and the badge says why');
  await page.close();
}

// Insights: the graph draws a dot per commit; the history scrubber steps to an older version's diff.
{
  const page = await ctx.newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e)));
  await page.goto(BASE + '/alice/hello/commits?graph=1', { waitUntil: 'networkidle' });
  const rows = await page.locator('.commit-graph .commit-row').count();
  const dots = await page.locator('.commit-graph .graph-cell i').count();
  await page.goto(BASE + '/alice/hello/history/main/README.md', { waitUntil: 'networkidle' });
  await page.locator('#v-older').click();
  await page.locator('#version-view .diff').waitFor({ timeout: 5000 }).catch(() => {});
  const label = await page.locator('#v-label').innerText().catch(() => '');
  const diff = await page.locator('#version-view .dfile').count();
  if (errors.length || !rows || dots !== rows || !/^\d+ of \d+$/.test(label) || label.startsWith(label.split(' of ')[1] + ' ') || !diff) {
    failed++; console.log('✗ commit graph or history scrubber', { rows, dots, label, diff, errors });
  } else console.log('  ok the commit graph draws every commit; the history scrubber steps back to an older diff');
  await page.close();
}

// Code review: clicking a line number opens the comment composer under that line.
{
  const page = await ctx.newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e)));
  await page.goto(BASE + '/alice/hello/pull/2/files', { waitUntil: 'networkidle' });
  await page.locator('td.dn[data-side="new"][data-n="1"]').first().click();
  const open = await page.locator('.rcomposer textarea').count();
  if (errors.length || !open) { failed++; console.log('✗ review composer', { open, errors }); }
  else console.log('  ok a line number opens the review composer');
  // SHOTS=<dir> saves pictures of the review pages to look at.
  if (process.env.SHOTS) {
    await page.screenshot({ path: `${process.env.SHOTS}/review-files.png`, fullPage: true });
    await page.goto(BASE + '/alice/hello/pull/2', { waitUntil: 'networkidle' });
    await page.screenshot({ path: `${process.env.SHOTS}/review-conversation.png`, fullPage: true });
  }
  await page.close();
}

// CI: a step's fold opens to its log, and commits show their CI result.
{
  const page = await ctx.newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e)));
  await page.goto(BASE + '/alice/hello/ci/1', { waitUntil: 'networkidle' });
  await page.locator('.ci-fold summary', { hasText: 'Say hello' }).click();
  const shown = await page.locator('.ci-fold[open] .ci-log', { hasText: 'hello from ci' }).count();
  await page.goto(BASE + '/alice/hello', { waitUntil: 'networkidle' });
  const badge = await page.locator('.commit-bar .ci-badge .ci-st.ok').count();
  if (errors.length || !shown || !badge) { failed++; console.log('✗ CI fold or badge', { shown, badge, errors }); }
  else console.log('  ok CI log folds open per step; the latest commit shows its passing run');
  await page.close();
}

// CI: a running job's log streams into the page as it's written.
{
  const yml = 'jobs:\n  live:\n    steps:\n      - name: Slow step\n        run: echo first line; sleep 2; echo second line\n';
  await ctx.request.post(BASE + '/api/repos/alice/hello/contents', { headers: { Origin: BASE }, data: { branch: 'main', message: 'Slow CI', changes: [{ path: '.gitter/ci.yml', content: yml }] } });
  const runs = await (await ctx.request.get(BASE + '/api/repos/alice/hello/ci/runs')).json();
  const n = runs.runs[0].number;
  const page = await ctx.newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e)));
  await page.goto(`${BASE}/alice/hello/ci/${n}`, { waitUntil: 'domcontentloaded' });
  const got = await page.locator('.ci-fold[open] .ci-log', { hasText: 'second line' }).waitFor({ timeout: 15000 }).then(() => true, () => false);
  const passed = await page.locator('.ci-head .ci-st.ok').waitFor({ timeout: 10000 }).then(() => true, () => false);
  if (errors.length || !got || !passed) { failed++; console.log('✗ CI live log', { got, passed, errors }); }
  else console.log('  ok a running job streams its log live and the page turns green when it passes');
  await page.close();
}

// Palette opens with Ctrl+K and finds a file.
{
  const page = await ctx.newPage();
  await page.goto(BASE + '/alice/hello', { waitUntil: 'networkidle' });
  await page.keyboard.press('Control+k');
  await page.keyboard.type('main.rs');
  await page.waitForTimeout(400);
  const items = await page.locator('.pal-item').allInnerTexts();
  if (!items.some((t) => t.includes('src/main.rs'))) { failed++; console.log('✗ palette did not find src/main.rs:', items.slice(0, 5)); }
  else console.log('  ok palette finds files');
  await page.keyboard.press('Enter');
  await page.waitForURL(/\/blob\/main\/src\/main\.rs$/);
  console.log('  ok palette navigates');
  await page.close();
}

// A file too big for the full diff loads on demand and renders as you scroll.
{
  const content = Array.from({ length: 5000 }, (_, i) => `row ${i}`).join('\n') + '\n';
  const w = await ctx.request.post(BASE + '/api/repos/alice/hello/contents', { headers: { Origin: BASE },
    data: { branch: 'main', new_branch: 'bigdiff', message: 'Big table', changes: [{ path: 'big.csv', content }] } });
  const { sha: big } = await w.json();
  const page = await ctx.newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e)));
  await page.goto(`${BASE}/alice/hello/commit/${big}`, { waitUntil: 'networkidle' });
  await page.click('[data-load]');
  await page.waitForSelector('.dfile .dtable tr.add');
  const first = await page.locator('.dfile .dtable tr').count();
  for (let k = 0; k < 20 && (await page.locator('.dmore').count()); k++) {
    await page.locator('.dmore').scrollIntoViewIfNeeded();
    await page.waitForTimeout(100);
  }
  const all = await page.locator('.dfile .dtable tr.add').count();
  if (errors.length || first > 1100 || all !== 5000) { failed++; console.log(`✗ big diff: first chunk ${first} rows, ${all} added rows after scrolling`, errors); }
  else console.log(`  ok big diff loads on demand (${first} rows first, ${all} after scrolling)`);
  await page.close();
}

// Webhooks: add one with the form, open its ping in the delivery log, redeliver it.
{
  const page = await ctx.newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e)));
  const steps = [];
  try {
    await page.goto(BASE + '/alice/hello/settings/hooks/new', { waitUntil: 'networkidle' });
    await page.fill('#h-url', hook.url + '/from-browser');
    await page.fill('#h-secret', 'browser-secret');
    await page.check('input[name=which][value=some]');
    await page.check('input[name=ev][value=push]');
    await page.check('#h-private');
    await page.click('#hook-form button[type=submit]');
    await page.waitForURL(/\/settings\/hooks\/\d+$/);
    steps.push('added');
    for (let i = 0; i < 25 && !(await page.locator('.hook-delivery', { hasText: 'Delivered' }).count()); i++) {
      await page.waitForTimeout(200);
      await page.reload({ waitUntil: 'networkidle' });
    }
    await page.locator('.hook-delivery summary', { hasText: 'ping' }).first().click();
    await page.locator('.hook-delivery[open] [data-redeliver]').waitFor({ timeout: 5000 });
    const shown = await page.locator('.hook-delivery[open]').innerText();
    if (!shown.includes('X-Gitter-Signature-256') || !shown.includes('"hook"')) throw new Error('delivery details missing: ' + shown.slice(0, 300));
    steps.push('read its ping');
    await page.click('.hook-delivery[open] [data-redeliver]');
    await page.locator('.hook-delivery .pill', { hasText: 'Redelivery' }).waitFor({ timeout: 5000 });
    steps.push('redelivered it');
  } catch (e) { errors.push(String(e)); }
  if (errors.length || steps.length < 3) { failed++; console.log('✗ webhook form and delivery log', steps, errors); }
  else console.log(`  ok webhook ${steps.join(', ')} in the browser`);
  await page.close();
}

// A sensitive action after the password confirmation expired asks for it, then goes through.
{
  // Age every session's confirmation (the server honours one for 10 minutes).
  execFileSync('python3', ['-c', 'import sqlite3,sys; c=sqlite3.connect(sys.argv[1]); c.execute("UPDATE sessions SET verified_at = 0"); c.commit()', process.env.GITTER_DB]);
  const page = await ctx.newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e)));
  await page.goto(BASE + '/settings/tokens', { waitUntil: 'networkidle' });
  await page.fill('#token-form input[name=name]', 'after-reauth');
  await page.click('#token-form button[type=submit]');
  await page.waitForSelector('#ra-pw');
  await page.fill('#ra-pw', 'wrong password');
  await page.click('.modal [data-ok]');
  await page.waitForSelector('#ra-err:not([hidden])');
  await page.fill('#ra-pw', 'correct horse');
  await page.click('.modal [data-ok]');
  await page.waitForSelector('.token-reveal', { timeout: 5000 }).catch(() => {});
  const made = await page.locator('.token-reveal').count();
  if (errors.length || !made) { failed++; console.log('✗ re-auth prompt did not let the token through', errors); }
  else console.log('  ok sensitive action asks for the password, then goes through');
  await page.close();
}

// Signing in to an account with two-factor auth asks for the code.
{
  const { createHmac } = await import('node:crypto');
  const totp = (secret, step) => {
    const bits = [...secret].map((c) => 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567'.indexOf(c).toString(2).padStart(5, '0')).join('');
    const key = Buffer.from(bits.match(/.{8}/g).map((b) => parseInt(b, 2)));
    const msg = Buffer.alloc(8); msg.writeBigUInt64BE(BigInt(step));
    const h = createHmac('sha1', key).update(msg).digest();
    const o = h[19] & 15;
    return String((h.readUInt32BE(o) & 0x7fffffff) % 1e6).padStart(6, '0');
  };
  const bobCtx = await browser.newContext();
  await bobCtx.request.post(BASE + '/api/auth/login', { headers: { Origin: BASE }, data: { login: 'bob', password: 'another horse' } });
  const { secret } = await (await bobCtx.request.post(BASE + '/api/me/2fa/setup', { headers: { Origin: BASE } })).json();
  const step = Math.floor(Date.now() / 30000);
  const on = await bobCtx.request.post(BASE + '/api/me/2fa/enable', { headers: { Origin: BASE }, data: { code: totp(secret, step) } });
  const page = await (await browser.newContext()).newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e)));
  await page.goto(BASE + '/login', { waitUntil: 'networkidle' });
  await page.fill('#login', 'bob');
  await page.fill('#password', 'another horse');
  await page.click('#auth-form button[type=submit]');
  await page.waitForSelector('#otp');
  await page.fill('#otp', totp(secret, step + 1));
  const signedIn = await page.waitForURL(BASE + '/', { timeout: 5000 }).then(() => true, () => false);
  if (!on.ok() || errors.length || !signedIn) { failed++; console.log('✗ two-factor sign-in', on.status(), errors); }
  else console.log('  ok two-factor sign-in asks for the code and signs in');
  await page.close();
}

// Trojan Source and look-alike letters are marked in file and diff views.
{
  const src = 'int ok = 1;\n/*‮ } ⁦if (admin)⁩ ⁦ */\nchar *pаypal = "a​b";\n';
  const w = await ctx.request.post(BASE + '/api/repos/alice/hello/contents', { headers: { Origin: BASE },
    data: { branch: 'main', new_branch: 'trojan', message: 'Trojan', changes: [{ path: 'trojan.c', content: src }] } });
  const { sha: t } = await w.json();
  const page = await ctx.newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e)));
  const marks = async () => [await page.locator('.uni-warn').count(), await page.locator('.uni.bidi').count(), await page.locator('.uni-mix').count()];
  await page.goto(`${BASE}/alice/hello/blob/trojan/trojan.c`, { waitUntil: 'networkidle' });
  const blob = await marks();
  await page.goto(`${BASE}/alice/hello/commit/${t}`, { waitUntil: 'networkidle' });
  const diff = await marks();
  await page.goto(`${BASE}/alice/hello/blob/main/src/main.rs`, { waitUntil: 'networkidle' });
  const clean = await page.locator('.uni-warn, .uni, .uni-mix').count();
  const ok = (m) => m[0] === 1 && m[1] === 4 && m[2] >= 1;
  if (errors.length || !ok(blob) || !ok(diff) || clean) { failed++; console.log('✗ hidden Unicode marks', { blob, diff, clean, errors }); }
  else console.log('  ok hidden and look-alike characters are marked in file and diff views');
  await page.close();
}

// Deploy tokens: created from repository settings, listed there, work for that repo only.
{
  const page = await ctx.newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e)));
  await page.goto(BASE + '/alice/hello/settings', { waitUntil: 'networkidle' });
  await page.fill('#dt-form input[name=name]', 'build server');
  await page.click('#dt-form button[type=submit]');
  await page.waitForSelector('#dt-new .token-reveal', { timeout: 5000 }).catch(() => {});
  const token = await page.locator('#dt-new input').inputValue().catch(() => '');
  const res = await fetch(`${BASE}/alice/hello.git/info/refs?service=git-upload-pack`, { headers: { Authorization: 'Basic ' + Buffer.from('x-access-token:' + token).toString('base64') } });
  const other = await fetch(`${BASE}/api/repos/alice/secret`, { headers: { Authorization: 'Bearer ' + token } });
  if (errors.length || !token.startsWith('gtr_') || res.status !== 200 || other.status !== 404) { failed++; console.log('✗ deploy token', { token: !!token, clone: res.status, other: other.status, errors }); }
  else console.log('  ok deploy token from repo settings clones that repo and nothing else');
  await page.close();
}

// Report → moderate: someone reports an issue, the admin hides it from the Reports tab; invite links work.
{
  const dctx = await browser.newContext();
  const su = await dctx.request.post(BASE + '/api/auth/signup', { headers: { Origin: BASE }, data: { username: 'dave', email: 'dave@example.com', password: 'correct horse' } });
  const errors = [];
  const page = await dctx.newPage();
  page.on('pageerror', (e) => errors.push(String(e)));
  await page.goto(BASE + '/alice/hello/issues/1', { waitUntil: 'networkidle' });
  await page.click('[data-menu="issue-body"]');
  await page.click('.pop [data-act="report"]');
  await page.click('.modal [data-ok]');
  await page.waitForSelector('.toast', { timeout: 5000 }).catch(() => {});
  const apage = await ctx.newPage();
  apage.on('pageerror', (e) => errors.push(String(e)));
  await apage.goto(BASE + '/admin/reports', { waitUntil: 'networkidle' });
  const rows = await apage.locator('.report-row').count();
  await apage.click('.report-row [data-resolve="hide"]');
  await apage.click('.modal [data-ok]');
  await apage.waitForTimeout(400);
  const gone = (await fetch(`${BASE}/api/repos/alice/hello/issues/1`)).status;
  await apage.goto(BASE + '/admin/settings', { waitUntil: 'networkidle' });
  await apage.click('#inv-form button[type=submit]');
  await apage.waitForSelector('#inv-new input', { timeout: 5000 }).catch(() => {});
  const link = await apage.locator('#inv-new input').inputValue().catch(() => '');
  if (!su.ok() || errors.length || rows !== 1 || gone !== 404 || !link.includes('/signup?invite=')) {
    failed++; console.log('✗ report and moderate', { signup: su.status(), rows, gone, link, errors });
  } else console.log('  ok report → admin hides it → it disappears; invite links are created');
  await page.close(); await apage.close();
}

// Wiki, discussion, snippet and release forms work end to end.
{
  const page = await ctx.newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e)));
  const text = () => page.locator('#page').innerText();
  const steps = [];
  try {
    await page.goto(BASE + '/alice/hello/wiki/_new', { waitUntil: 'networkidle' });
    await page.fill('#wiki-name', 'From the browser');
    await page.fill('#wiki-content', 'Written in **Chromium**, see [Setup](Setup)');
    await page.click('#wiki-form button[type=submit]');
    await page.waitForURL(/\/wiki\/From-the-browser$/);
    await page.waitForSelector('.markdown-body strong');
    if ((await page.getAttribute('.markdown-body a[href$="/wiki/Setup"]', 'href')) == null) throw new Error('wiki link not rewritten');
    steps.push('wiki page');
    await page.goto(BASE + '/alice/hello/discussions/1', { waitUntil: 'networkidle' });
    const vote = page.locator('.vote-btn:not([data-vote="thread"])').first();
    await vote.click();
    await page.waitForFunction(() => document.querySelector('.vote-btn:not([data-vote="thread"]) span')?.textContent === '1');
    await page.fill('#reply-body', 'Thanks from the browser');
    await page.click('#composer button[type=submit]');
    await page.waitForFunction(() => document.querySelector('#page').innerText.includes('Thanks from the browser'));
    steps.push('discussion vote and reply');
    await page.goto(BASE + '/snippets/new', { waitUntil: 'networkidle' });
    await page.fill('[name=fname]', 'notes.txt');
    await page.fill('.snip-file textarea', 'browser snippet');
    await page.check('input[name=vis][value=secret]');
    await page.click('#snip-form button[type=submit]');
    await page.waitForURL(/\/snippets\/[0-9a-f]{32}$/);
    await page.waitForFunction(() => document.querySelector('#page').innerText.includes('browser snippet'));
    if (!(await text()).includes('Secret')) throw new Error('snippet not secret');
    steps.push('secret snippet');
    await page.goto(BASE + '/alice/hello/releases/new', { waitUntil: 'networkidle' });
    await page.fill('#rel-tag', 'v2.0-beta');
    await page.fill('#rel-title', 'Beta from the browser');
    await page.check('#rel-pre');
    await page.setInputFiles('#rel-files', { name: 'notes.txt', mimeType: 'text/plain', buffer: Buffer.from('uploaded from chromium') });
    await page.click('#rel-form button[data-draft=""]');
    await page.waitForURL(/\/releases\/\d+$/);
    await page.waitForFunction(() => document.querySelector('#page').innerText.includes('notes.txt'));
    if (!(await text()).includes('Pre-release')) throw new Error('release not marked as a pre-release');
    const href = await page.getAttribute('a[download][href*="/assets/"]', 'href');
    if ((await (await ctx.request.get(BASE + href)).text()) !== 'uploaded from chromium') throw new Error('release file differs');
    steps.push('pre-release with a file');
  } catch (e) { errors.push(String(e)); }
  if (errors.length) { failed++; console.log('✗ wiki/discussion/snippet/release forms', steps, errors); }
  else console.log(`  ok ${steps.join(', ')} made in the browser`);
  await page.close();
}

// Settings → Repositories: accept the transfer e2e.sh offered, and restore the repository it deleted.
{
  const page = await ctx.newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e)));
  try {
    await page.goto(BASE + '/settings/repositories', { waitUntil: 'networkidle' });
    await page.click('[data-offer] [data-accept]');
    await page.waitForSelector('[data-offer]', { state: 'detached', timeout: 5000 });
    await page.click('[data-trash] [data-restore]');
    await page.waitForSelector('[data-trash]', { state: 'detached', timeout: 5000 });
  } catch (e) { errors.push(String(e)); }
  const present = (await ctx.request.get(BASE + '/api/repos/alice/present')).status();
  const scratch = (await ctx.request.get(BASE + '/api/repos/alice/scratch')).status();
  if (errors.length || present !== 200 || scratch !== 200) { failed++; console.log('✗ settings → repositories', { present, scratch, errors }); }
  else console.log('  ok a transfer accepted and a deleted repository restored from settings');
  await page.close();
}

// Agent tokens: made in Settings → Tokens with repositories and tools; what the agent does
// through /mcp shows an Agent badge, and the security log names the token.
{
  const page = await ctx.newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e)));
  let num = null, badge = 0, logged = false;
  try {
    await page.goto(BASE + '/settings/tokens', { waitUntil: 'networkidle' });
    await page.fill('#token-form input[name=name]', 'ui agent');
    await page.selectOption('#tk-scope', 'agent');
    await page.fill('#tk-repos', 'alice/hello');
    await page.uncheck('#token-form input[name=tool][value=commit_files]');
    await page.click('#token-form button[type=submit]');
    await page.waitForSelector('#new-token .token-reveal', { timeout: 5000 });
    const token = await page.locator('#token-value').inputValue();
    if (!(await page.locator('#new-token pre').innerText()).includes('/mcp')) throw new Error('no MCP config example');
    const call = async (name, args) => (await (await fetch(BASE + '/mcp', { method: 'POST', headers: { Authorization: 'Bearer ' + token, 'Content-Type': 'application/json' },
      body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'tools/call', params: { name, arguments: args } }) })).json()).result;
    const made = await call('create_issue', { repo: 'alice/hello', title: 'Opened by an agent' });
    if (!made || made.isError) throw new Error('create_issue: ' + JSON.stringify(made));
    num = JSON.parse(made.content[0].text).number;
    const denied = await call('commit_files', { repo: 'alice/hello', branch: 'main', new_branch: 'x', message: 'x', files: [{ path: 'x', content: 'x' }] });
    if (denied) throw new Error('a tool left unticked still works');
    await page.goto(`${BASE}/alice/hello/issues/${num}`, { waitUntil: 'networkidle' });
    badge = await page.locator('.comment-head .agent-badge').count();
    await page.goto(BASE + '/settings/security', { waitUntil: 'networkidle' });
    logged = (await page.locator('#page').innerText()).includes('ui agent: create_issue alice/hello');
  } catch (e) { errors.push(String(e)); }
  if (errors.length || !badge || !logged) { failed++; console.log('✗ agent token', { num, badge, logged, errors }); }
  else console.log('  ok agent token made in settings; its issue has an Agent badge and the security log names it');
  await page.close();
}

// Rich viewers (CSV, PDF, notebook, STL, audio, video), image diff modes, drag-and-drop upload, revert.
{
  const { deflateSync } = await import('node:zlib');
  const crcTable = Array.from({ length: 256 }, (_, n) => { let c = n; for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1; return c >>> 0; });
  const crc = (b) => { let c = 0xffffffff; for (const x of b) c = crcTable[(c ^ x) & 255] ^ (c >>> 8); return (c ^ 0xffffffff) >>> 0; };
  const chunk = (type, data) => { const len = Buffer.alloc(4); len.writeUInt32BE(data.length); const td = Buffer.concat([Buffer.from(type), data]); const c = Buffer.alloc(4); c.writeUInt32BE(crc(td)); return Buffer.concat([len, td, c]); };
  const png = ([r, g, b]) => { const w = 8, rows = []; for (let y = 0; y < w; y++) rows.push(Buffer.from([0, ...Array.from({ length: w }, () => [r, g, b]).flat()]));
    const ihdr = Buffer.alloc(13); ihdr.writeUInt32BE(w, 0); ihdr.writeUInt32BE(w, 4); ihdr[8] = 8; ihdr[9] = 2;
    return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk('IHDR', ihdr), chunk('IDAT', deflateSync(Buffer.concat(rows))), chunk('IEND', Buffer.alloc(0))]); };
  const stl = Buffer.alloc(84 + 4 * 50);
  stl.writeUInt32LE(4, 80);
  const tri = [[0, 0, 0, 1, 0, 0, 0, 1, 0], [0, 0, 0, 0, 0, 1, 1, 0, 0], [0, 0, 0, 0, 1, 0, 0, 0, 1], [1, 0, 0, 0, 0, 1, 0, 1, 0]];
  tri.forEach((t, i) => t.forEach((v, k) => stl.writeFloatLE(v, 84 + i * 50 + 12 + k * 4)));
  const wav = Buffer.alloc(44 + 8000, 128);
  wav.write('RIFF', 0); wav.writeUInt32LE(36 + 8000, 4); wav.write('WAVEfmt ', 8); wav.writeUInt32LE(16, 16); wav.writeUInt16LE(1, 20); wav.writeUInt16LE(1, 22);
  wav.writeUInt32LE(8000, 24); wav.writeUInt32LE(8000, 28); wav.writeUInt16LE(1, 32); wav.writeUInt16LE(8, 34); wav.write('data', 36); wav.writeUInt32LE(8000, 40);
  const pdf = '%PDF-1.1\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj 2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj 3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 200 200]>>endobj\ntrailer<</Root 1 0 R>>\n%%EOF\n';
  const nb = JSON.stringify({ nbformat: 4, nbformat_minor: 5, metadata: { language_info: { name: 'python' } }, cells: [
    { cell_type: 'markdown', metadata: {}, source: ['# Notebook title\n', 'Some **bold** text <a href="javascript:window.__pwned=1" onclick="window.__pwned=1">link</a>'] },
    { cell_type: 'code', execution_count: 1, metadata: {}, source: 'print("hi")', outputs: [{ output_type: 'stream', name: 'stdout', text: ['printed hi\n'] }] },
    { cell_type: 'code', execution_count: 2, metadata: {}, source: 'show()', outputs: [
      { output_type: 'display_data', metadata: {}, data: { 'image/png': png([200, 30, 30]).toString('base64'), 'text/plain': '<Figure>' } },
      { output_type: 'display_data', metadata: {}, data: { 'text/html': '<script>window.__pwned=1</script><b onmouseover="window.__pwned=1">table</b>' } }] },
  ] });
  const b64 = (b) => Buffer.from(b).toString('base64');
  const put = async (message, changes, branch = 'main', new_branch) => (await (await ctx.request.post(BASE + '/api/repos/alice/hello/contents', { headers: { Origin: BASE },
    data: { branch, new_branch, message, changes: changes.map(([path, data]) => ({ path, content: b64(data), base64: true })) } })).json());
  await put('Add viewer samples', [['view/table.csv', 'name,n\nb,10\na,2\nc,33\n'], ['view/doc.pdf', pdf], ['view/nb.ipynb', nb], ['view/tetra.stl', stl],
    ['view/song.wav', wav], ['view/clip.webm', Buffer.from([0x1a, 0x45, 0xdf, 0xa3, 0, 0, 0, 0])]], 'main', 'viewers');
  await put('Add a picture', [['pic.png', png([20, 120, 220])]], 'main', 'pictures');
  const { sha: picSha } = await put('Recolour the picture', [['pic.png', png([240, 160, 20])]], 'pictures');

  const page = await ctx.newPage();
  const errors = [], steps = [];
  page.on('console', (m) => { if (m.type() === 'error') errors.push(m.text()); });
  page.on('pageerror', (e) => errors.push(String(e)));
  const go = (p) => page.goto(BASE + p, { waitUntil: 'networkidle' });
  try {
    await go('/alice/hello/blob/viewers/view/table.csv');
    const firstCell = () => page.locator('.csv-table tbody tr:first-child td:first-child').innerText();
    if ((await page.locator('.csv-table tbody tr').count()) !== 3) throw new Error('CSV table rows');
    await page.click('.csv-table th button[data-col="1"]');
    const asc = await firstCell();
    await page.click('.csv-table th button[data-col="1"]');
    if (asc !== 'a' || (await firstCell()) !== 'c') throw new Error(`CSV numeric sort: ${asc}`);
    steps.push('CSV sorts');
    await go('/alice/hello/blob/viewers/view/doc.pdf');
    const frame = await page.getAttribute('iframe.pdf-view', 'src');
    const head = await ctx.request.get(BASE + frame);
    if (head.headers()['content-type'] !== 'application/pdf' || !head.headers()['content-security-policy'].includes("frame-ancestors 'self'")) throw new Error('PDF frame headers');
    steps.push('PDF frame');
    await go('/alice/hello/blob/viewers/view/nb.ipynb');
    const nbText = await page.locator('.notebook').innerText();
    const nbOk = nbText.includes('Notebook title') && nbText.includes('printed hi') && nbText.includes('<script>')
      && (await page.locator('.notebook .nb-md strong').count()) === 1 && (await page.locator('.notebook .nb-out img').count()) === 1
      && (await page.locator('.notebook b, .notebook script').count()) === 0 && !(await page.evaluate(() => window.__pwned));
    if (!nbOk) throw new Error('notebook: ' + nbText.slice(0, 300));
    steps.push('notebook (HTML output as text, nothing ran)');
    await go('/alice/hello/blob/viewers/view/tetra.stl');
    await page.locator('.stl-note', { hasText: /triangles|WebGL/ }).waitFor({ timeout: 5000 });
    const stlNote = await page.locator('.stl-note').innerText();
    if (!stlNote.includes('4 triangles')) throw new Error('STL: ' + stlNote);
    await page.locator('.stl-view canvas').dragTo(page.locator('.stl-view canvas'), { sourcePosition: { x: 50, y: 50 }, targetPosition: { x: 150, y: 90 } });
    await page.locator('.stl-view canvas').click({ clickCount: 2 });
    steps.push('STL in WebGL');
    await go('/alice/hello/blob/viewers/view/song.wav');
    const dur = await page.locator('.media-view audio').evaluate((a) => new Promise((res) => (a.readyState >= 1 ? res(a.duration) : a.addEventListener('loadedmetadata', () => res(a.duration)))));
    if (!(dur > 0.9 && dur < 1.1)) throw new Error('audio duration ' + dur);
    await go('/alice/hello/blob/viewers/view/clip.webm');
    if ((await page.locator('.media-view video[controls]').count()) !== 1) throw new Error('video player');
    steps.push('audio and video players');
    await go(`/alice/hello/commit/${picSha}`);
    await page.locator('.imgdiff-2up img').first().waitFor();
    const loaded = await page.locator('.imgdiff-2up img').evaluateAll((imgs) => imgs.filter((i) => i.complete && i.naturalWidth === 8).length);
    await page.click('button[data-imode="swipe"]');
    await page.locator('.imgdiff-stack input[type=range]').evaluate((r) => { r.value = '30'; r.dispatchEvent(new Event('input', { bubbles: true })); });
    const clip = await page.locator('.imgdiff-layers .top').evaluate((i) => getComputedStyle(i).clipPath);
    await page.click('button[data-imode="onion"]');
    const op = await page.locator('.imgdiff-layers .top').evaluate((i) => getComputedStyle(i).opacity);
    if (loaded !== 2 || !clip.includes('30%') || op !== '0.3' || !(await page.locator('.imgdiff-2up').isHidden())) throw new Error(`image diff: ${loaded} ${clip} ${op}`);
    steps.push('image diff 2-up, swipe, onion skin');
    // Drop a file on a file page: it opens the upload with that file, ready to commit.
    await go('/alice/hello/blob/main/README.md');
    const dt = await page.evaluateHandle(() => { const d = new DataTransfer(); d.items.add(new File(['dropped from the browser\n'], 'dropped.txt', { type: 'text/plain' })); return d; });
    await page.dispatchEvent('.markdown-body', 'dragenter', { dataTransfer: dt });
    await page.dispatchEvent('.markdown-body', 'drop', { dataTransfer: dt });
    await page.waitForURL(/\/upload\/main$/);
    await page.locator('#up-list', { hasText: 'dropped.txt' }).waitFor({ timeout: 5000 });
    await page.click('#cm-go');
    await page.waitForURL(/\/tree\/main$/);
    if (!(await ctx.request.get(BASE + '/raw/alice/hello/main/dropped.txt')).ok()) throw new Error('dropped file not committed');
    steps.push('drag-and-drop upload');
    // Revert that commit from its page, then the merged pull request from its page.
    const last = (await (await ctx.request.get(BASE + '/api/repos/alice/hello/commits?rev=main')).json()).commits[0].sha;
    await go(`/alice/hello/commit/${last}`);
    await page.click('#revert');
    await page.click('.modal [data-ok]');
    await page.waitForURL(/\/pull\/\d+$/);
    await page.locator('#page', { hasText: 'Revert "Add dropped.txt via upload"' }).waitFor({ timeout: 5000 });
    await go('/alice/hello/pull/2');
    await page.click('#revert-pr');
    await page.click('.modal [data-ok]');
    await page.waitForURL(/\/pull\/\d+$/);
    await page.locator('#page', { hasText: 'Revert "Add lib"' }).waitFor({ timeout: 5000 });
    steps.push('revert a commit and a merged pull request');
  } catch (e) { errors.push(String(e)); }
  if (errors.length) { failed++; console.log('✗ viewers, image diff, drop upload, revert', steps, errors); }
  else console.log(`  ok ${steps.join(', ')}`);
  await page.close();
  // The branches above started CI runs: let them finish, so the server stops with nothing running.
  for (let i = 0; i < 120; i++) {
    const { runs } = await (await ctx.request.get(BASE + '/api/repos/alice/hello/ci/runs')).json();
    if (!runs.some((r) => r.status === 'queued' || r.status === 'running')) break;
    await new Promise((r) => setTimeout(r, 500));
  }
}

// Milestones, assignees and templates: a milestone with progress, set from the issue sidebar
// with an assignee; the template chooser fills a new issue; the PR template starts a PR.
{
  const page = await ctx.newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e)));
  page.on('console', (m) => { if (m.type() === 'error') errors.push(m.text()); });
  const api = (method, path, data) => ctx.request.fetch(`${BASE}/api/repos/alice/hello${path}`, { method, headers: { Origin: BASE }, data });
  const seen = {};
  try {
    const made = await api('POST', '/milestones', { title: 'UI milestone', due_on: '2099-12-31', description: 'For the UI check' });
    if (!made.ok()) throw new Error('milestone: ' + made.status());
    const ms = (await made.json()).number;
    const files = [
      { path: '.gitter/ISSUE_TEMPLATE/bug.md', content: '---\nname: UI bug report\nabout: Tell us what broke\ntitle: "[bug] "\nlabels: [bug]\n---\n## What happened\n' },
      { path: '.gitter/pull_request_template.md', content: '## Checklist for this PR\n' },
    ];
    if (!(await api('POST', '/contents', { branch: 'main', message: 'Templates', changes: files })).ok()) throw new Error('templates commit');
    if (!(await api('POST', '/contents', { branch: 'main', new_branch: 'tpl-branch', message: 'Change', changes: [{ path: 'tpl.txt', content: 'x\n' }] })).ok()) throw new Error('branch commit');

    // The sidebar pickers: a milestone, then an assignee.
    await page.goto(`${BASE}/alice/hello/issues/1`, { waitUntil: 'networkidle' });
    await page.click('#edit-milestone');
    await page.click(`.pop [data-ms="${ms}"]`);
    await page.waitForSelector('#milestone-block a[href$="/milestone/' + ms + '"]', { timeout: 5000 });
    await page.click('#edit-assignees');
    await page.check('.pop input[value="alice"]');
    await page.waitForFunction(() => document.querySelector('#assignee-list')?.innerText.includes('alice'), null, { timeout: 5000 });
    seen.sidebar = true;

    await page.goto(`${BASE}/alice/hello/milestones`, { waitUntil: 'networkidle' });
    seen.list = (await page.locator('#page').innerText()).includes('UI milestone') && (await page.locator('.ms-progress progress').count()) > 0;
    await page.goto(`${BASE}/alice/hello/milestone/${ms}`, { waitUntil: 'networkidle' });
    seen.show = (await page.locator('#page').innerText()).includes('Crash on start');
    await page.goto(`${BASE}/alice/hello/issues?milestone=${encodeURIComponent('UI milestone')}&state=closed&assignee=alice`, { waitUntil: 'networkidle' });
    seen.filter = (await page.locator('#page').innerText()).includes('Crash on start');

    // The chooser, then the form filled from the template.
    await page.goto(`${BASE}/alice/hello/issues/new`, { waitUntil: 'networkidle' });
    await page.click('#template-chooser a.btn');
    await page.waitForSelector('#issue-title', { timeout: 5000 });
    seen.template = (await page.inputValue('#issue-title')) === '[bug] ' && (await page.inputValue('#new-issue .mde-input')).includes('What happened');

    await page.goto(`${BASE}/alice/hello/compare/main...tpl-branch`, { waitUntil: 'networkidle' });
    seen.pr = (await page.inputValue('#pr-body')).includes('Checklist for this PR');
  } catch (e) { errors.push(String(e)); }
  if (errors.length || !(seen.sidebar && seen.list && seen.show && seen.filter && seen.template && seen.pr)) { failed++; console.log('✗ milestones, assignees and templates', { seen, errors }); }
  else console.log('  ok milestone and assignee set from the sidebar; milestones list, page and filter; template chooser and PR template prefill');
  await page.close();
}

// Branch rules: added from settings, they show in the merge box; badges are SVG and public links unfurl.
{
  const page = await ctx.newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e)));
  let listed = false, gated = false, removed = false, badge = '', og = '';
  try {
    await page.goto(BASE + '/alice/hello/settings', { waitUntil: 'networkidle' });
    await page.click('#rules summary');
    await page.fill('#rule-form input[name=pattern]', 'main');
    await page.fill('#rule-form input[name=checks]', 'release-check');
    await page.click('#rule-form button[type=submit]');
    await page.waitForSelector('#rules [data-rule-edit]', { timeout: 5000 });
    listed = (await page.locator('#rules').innerText()).includes('checks: release-check');
    // An open pull request into main to look at (e2e's own are merged).
    const num = await page.evaluate(async () => {
      const post = (u, b) => fetch(u, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(b) }).then((r) => r.json());
      await post('/api/repos/alice/hello/contents', { branch: 'main', new_branch: 'ui-rule', message: 'Rule check', changes: [{ path: 'rule.txt', content: 'x\n' }] });
      return (await post('/api/repos/alice/hello/pulls', { title: 'Waits for checks', head: 'ui-rule', base: 'main' })).number;
    });
    await page.goto(`${BASE}/alice/hello/pull/${num}`, { waitUntil: 'networkidle' });
    gated = (await page.locator('.merge-blocked').innerText().catch(() => '')).includes("release-check hasn't run");
    await page.goto(BASE + '/alice/hello/settings', { waitUntil: 'networkidle' });
    await page.click('#rules [data-rule-del]');
    await page.click('.modal [data-ok]');
    await page.waitForSelector('#rules [data-rule-del]', { state: 'detached', timeout: 5000 });
    removed = true;
    const b = await fetch(BASE + '/api/repos/alice/hello/ci/badge.svg');
    badge = b.headers.get('content-type') + ' ' + (await b.text()).slice(0, 4);
    og = await (await fetch(BASE + '/alice/hello')).text();
  } catch (e) { errors.push(String(e)); }
  const unfurls = og.includes('<meta property="og:title" content="alice/hello">');
  if (errors.length || !listed || !gated || !removed || badge !== 'image/svg+xml <svg' || !unfurls) { failed++; console.log('✗ branch rules', { listed, gated, removed, badge, unfurls, errors }); }
  else console.log('  ok branch rule made in settings blocks the merge box; badge is SVG; public pages unfurl');
  await page.close();
}

await browser.close();
if (failed) { console.log(`${failed} page check(s) failed`); process.exit(1); }
