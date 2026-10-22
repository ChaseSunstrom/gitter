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
  ['/alice/hello', ['README.md', 'src']],
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
  ['/alice/hello/pull/2', ['Add lib']],
  ['/alice/hello/pull/2/files', ['src/lib.rs']],
  ['/alice/hello/pull/2/commits', ['Add lib']],
  ['/alice/hello/compare/main...pw-branch', []],
  ['/alice/hello/labels', ['bug']],
  ['/alice/hello/issues/new', []],
  ['/alice/hello/insights', ['Alice']],
  ['/alice/hello/search?q=println', ['src/main.rs']],
  ['/alice/hello/settings', ['Danger zone', 'CI secrets', 'Webhooks', hook.url, 'Issues']],
  [`/alice/hello/settings/hooks/${hook.id}`, ['Payload URL', 'Recent deliveries', 'ping', 'issues', 'Delivered', 'Send a ping']],
  ['/alice/hello/settings/hooks/new', ['Add webhook', 'Payload URL', 'Allow private addresses']],
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
  ['/settings/security', ['Two-factor authentication', 'This browser', 'Security log']],
  ['/admin/audit', ['The log is intact', 'Created the account']],
  ['/settings/appearance', []],
  ['/admin', ['Site administration', 'Storage']],
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

// Wiki, discussion and snippet forms work end to end.
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
  } catch (e) { errors.push(String(e)); }
  if (errors.length) { failed++; console.log('✗ wiki/discussion/snippet forms', steps, errors); }
  else console.log(`  ok ${steps.join(', ')} made in the browser`);
  await page.close();
}

await browser.close();
if (failed) { console.log(`${failed} page check(s) failed`); process.exit(1); }
