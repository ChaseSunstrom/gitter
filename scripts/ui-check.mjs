// Visits every page type in a headless browser and fails on console errors
// or missing content. Run by scripts/e2e.sh against its seeded server.
import { chromium } from 'playwright-core';
import os from 'node:os';
import fs from 'node:fs';

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
  ['/alice/hello/settings', ['Danger zone']],
  ['/alice/hello/edit/main/README.md', ['Commit changes']],
  ['/alice/hello/new/main', ['Commit changes']],
  ['/alice/hello/upload/main', ['Drag files']],
  ['/alice/secret', ['README.md']],
  ['/new', []],
  ['/explore?sort=stars', ['hello']],
  ['/settings', []],
  ['/settings/tokens', ['cli']],
  ['/settings/appearance', []],
  ['/admin', ['Site administration', 'Storage']],
  ['/admin/users', ['alice', 'bob']],
  ['/admin/settings', ['Instance name']],
];
for (const [p, e] of pages) await visit(ctx, p, e);
await visit(ctx, '/alice/hello/pull/2/files', ['src/lib.rs'], { dark: true });

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

await browser.close();
if (failed) { console.log(`${failed} page check(s) failed`); process.exit(1); }
