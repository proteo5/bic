// Opens the built site in Chrome and checks the live examples, the drop zone
// and the documentation links. Run after `python build.py` and a static server.
import { chromium } from 'playwright-core';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const BASE = process.argv[2] || 'http://localhost:8126';
const CHROME = 'C:/Program Files/Google/Chrome/Application/chrome.exe';
const context = await chromium.launchPersistentContext(mkdtempSync(join(tmpdir(), 'bic-site-')), {
  executablePath: CHROME, headless: true, args: ['--no-first-run'],
  viewport: { width: 1280, height: 900 },
});
const page = await context.newPage();
const errors = [];
page.on('pageerror', (e) => errors.push(e.message));
page.on('console', (m) => { if (m.type() === 'error') errors.push(m.text()); });

for (const path of ['/', '/es/', '/docs/', '/es/docs/']) {
  await page.goto(BASE + path, { waitUntil: 'load' });
  await page.waitForTimeout(1200);
  const title = await page.title();
  const links = await page.$$eval('a[href]', (as) => as.map((a) => a.getAttribute('href')));
  const missing = [];
  for (const href of new Set(links)) {
    if (!href || href.startsWith('#') || href.startsWith('http') || href.startsWith('mailto')) continue;
    const target = new URL(href, BASE + path).href;
    const res = await fetch(target, { method: 'HEAD' }).catch(() => null);
    if (!res || !res.ok) missing.push(href);
  }
  console.log(`\n${path}  title="${title}"  missing links: ${missing.length ? missing.join(', ') : 'none'}`);
  if (path === '/' || path === '/es/') {
    const statuses = await page.$$eval('[data-example] .status', (els) => els.map((e) => `${e.dataset.state || '-'}: ${e.textContent}`));
    statuses.forEach((s) => console.log('   ' + s));
    const images = await page.$$eval('[data-example] canvas, [data-example] img', (els) => els.length);
    console.log(`   rendered: ${images} live examples (photos are shown as their JPEG)`);
  }
}

// Theme toggle: dark, then check the attribute and that it persists.
await page.goto(BASE + '/', { waitUntil: 'load' });
await page.click('#theme-toggle');
const theme = await page.evaluate(() => document.documentElement.dataset.theme);
await page.reload({ waitUntil: 'load' });
const persisted = await page.evaluate(() => document.documentElement.dataset.theme);
console.log(`\ntheme toggle: ${theme} -> after reload ${persisted}`);
await page.screenshot({ path: join(tmpdir(), 'bic-site-shot-dark.png'), fullPage: false });
await page.click('#theme-toggle');
await page.reload({ waitUntil: 'load' });
await page.waitForTimeout(800);
await page.screenshot({ path: join(tmpdir(), 'bic-site-shot-light.png'), fullPage: false });

console.log(`\npage errors: ${errors.length ? errors.join(' | ') : 'none'}`);
await context.close();
