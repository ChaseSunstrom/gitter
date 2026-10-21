// Screenshot a page and report console errors.
// usage: node scripts/shot.mjs <url> <out.png> [--dark] [--mobile] [--full] [--cookie name=value] [--click selector] [--wait ms]
import { chromium } from 'playwright-core';
import os from 'node:os';
import fs from 'node:fs';

const args = process.argv.slice(2);
const [url, out] = args;
const flag = (f) => args.includes(f);
const opt = (f) => { const i = args.indexOf(f); return i >= 0 ? args[i + 1] : null; };
// Use $CHROME, else the newest Chromium that Playwright has downloaded.
const pwDir = `${os.homedir()}/.cache/ms-playwright`;
const found = fs.existsSync(pwDir) ? fs.readdirSync(pwDir).filter((d) => /^chromium-\d+$/.test(d)).sort().reverse()[0] : null;
const exe = process.env.CHROME || (found ? `${pwDir}/${found}/chrome-linux64/chrome` : undefined);
const browser = await chromium.launch({ executablePath: exe });
const ctx = await browser.newContext({
  viewport: flag('--mobile') ? { width: 390, height: 844 } : { width: 1360, height: 900 },
  deviceScaleFactor: 1,
  colorScheme: flag('--dark') ? 'dark' : 'light',
});
const cookie = opt('--cookie');
if (cookie) { const [name, value] = cookie.split('='); await ctx.addCookies([{ name, value, url: new URL(url).origin }]); }
const skin = opt('--skin');
if (skin) await ctx.addInitScript((s) => { try { localStorage.setItem('skin', s); } catch {} }, skin);
const page = await ctx.newPage();
const errors = [];
page.on('console', (m) => { if (m.type() === 'error') errors.push(m.text()); });
page.on('pageerror', (e) => errors.push(String(e)));
await page.goto(url, { waitUntil: 'networkidle' });
const click = opt('--click');
if (click) { await page.click(click); }
await page.waitForTimeout(+(opt('--wait') || 400));
if (out) await page.screenshot({ path: out, fullPage: flag('--full') });
console.log(errors.length ? 'CONSOLE ERRORS:\n' + errors.join('\n') : 'no console errors');
await browser.close();
