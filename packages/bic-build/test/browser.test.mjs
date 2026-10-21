// End to end in Chrome: build the fixture app with each bundler, serve it, and
// check what the browser actually downloads over three visits.
//
//   visit 1: no worker yet      -> the PNG comes from the network
//   visit 2: worker active      -> the .bic is fetched and decoded once
//   visit 3: decoded image cached -> no network for the chart at all
//
// Needs Chrome. Set BIC_CHROME to its path; without it the test is skipped.
// Run manually: BIC_CHROME="C:\...\chrome.exe" node --test test/browser.test.mjs
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { build } from 'vite';
import { chromium } from 'playwright-core';
import { createServer } from 'node:http';
import { existsSync, mkdtempSync, readFileSync, statSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, extname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { bicVite } from '../src/vite.js';
import { buildWebpackApp } from './webpack-app/build.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, '..', '..', '..');
const exe = process.platform === 'win32' ? 'bic.exe' : 'bic';
const binary = process.env.BIC_BIN || join(root, 'target', 'release', exe);
const chrome = process.env.BIC_CHROME;
const expected = JSON.parse(readFileSync(join(root, 'spike', 'wasm-decoder', 'test', 'expected.json'), 'utf8'));
const chartHash = expected.find((e) => e.id === 'bar-chart').sha256;
const types = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.png': 'image/png', '.jpg': 'image/jpeg', '.bic': 'application/octet-stream' };

function serve(dir) {
  return new Promise((resolve) => {
    const server = createServer((req, res) => {
      const path = join(dir, decodeURIComponent(new URL(req.url, 'http://x').pathname));
      const file = path.endsWith('/') || (existsSync(path) && statSync(path).isDirectory()) ? join(path, 'index.html') : path;
      if (!existsSync(file)) {
        res.writeHead(404).end();
        return;
      }
      res.writeHead(200, { 'Content-Type': types[extname(file)] || 'application/octet-stream' });
      res.end(readFileSync(file));
    });
    server.listen(0, '127.0.0.1', () => resolve(server));
  });
}

async function buildVite(outDir) {
  await build({
    root: join(here, 'vite-app'),
    logLevel: 'silent',
    plugins: [bicVite({ bic: binary })],
    build: { outDir, emptyOutDir: true },
  });
  return '/';
}

async function buildWebpack(outDir) {
  await buildWebpackApp(outDir, binary);
  return '/index.html';
}

for (const [name, buildApp] of [['Vite', buildVite], ['webpack', buildWebpack]]) {
  test(`${name}: the browser shows the chart from BIC on later visits, with exact pixels`, { skip: !chrome && 'set BIC_CHROME to a Chrome executable', timeout: 900000 }, () => scenario(buildApp));
}

async function scenario(buildApp) {
  const outDir = mkdtempSync(join(tmpdir(), 'bic-browser-dist-'));
  const pagePath = await buildApp(outDir);
  const server = await serve(outDir);
  const origin = `http://127.0.0.1:${server.address().port}`;
  const context = await chromium.launchPersistentContext(mkdtempSync(join(tmpdir(), 'bic-profile-')), {
    executablePath: chrome,
    headless: true,
    args: ['--no-first-run', '--no-default-browser-check'],
  });

  async function visit(label) {
    const page = await context.newPage();
    const cdp = await context.newCDPSession(page);
    await cdp.send('Network.enable');
    const urls = new Map();
    cdp.on('Network.requestWillBeSent', (e) => urls.set(e.requestId, e.request.url));
    await page.goto(origin + pagePath, { waitUntil: 'load' });
    await page.waitForTimeout(1500);
    const result = await page.evaluate(async () => {
      const chart = document.querySelector('#chart');
      const canvas = document.createElement('canvas');
      canvas.width = chart.naturalWidth;
      canvas.height = chart.naturalHeight;
      const ctx = canvas.getContext('2d');
      ctx.drawImage(chart, 0, 0);
      const digest = new Uint8Array(await crypto.subtle.digest('SHA-256', ctx.getImageData(0, 0, canvas.width, canvas.height).data));
      const resources = performance.getEntriesByType('resource').map((e) => ({ name: e.name.replace(location.origin, ''), transfer: e.transferSize }));
      return {
        controlled: !!navigator.serviceWorker.controller,
        width: chart.naturalWidth,
        hash: [...digest].map((b) => b.toString(16).padStart(2, '0')).join(''),
        resources,
      };
    });
    await page.close();
    const downloaded = result.resources.filter((r) => /\.(png|jpg|bic)$/.test(r.name));
    console.log(`\n== ${label}: controlled=${result.controlled}`);
    for (const r of downloaded) console.log(`   ${r.name.padEnd(30)} network ${r.transfer} B`);
    return { ...result, downloaded };
  }

  try {
    // First visit: the page is not controlled yet, so the PNG is downloaded.
    const first = await visit('visit 1');
    const firstChart = first.downloaded.find((r) => /chart[-.].*\.png$/.test(r.name));
    assert.ok(firstChart && firstChart.transfer > 0, 'visit 1 downloads the chart PNG');

    // Second visit: the worker serves the chart, fetching the .bic from the network.
    const second = await visit('visit 2');
    assert.equal(second.controlled, true, 'the worker controls the page on visit 2');
    assert.equal(second.hash, chartHash, 'chart pixels match the source');
    assert.ok(!second.downloaded.some((r) => /chart[-.].*\.png$/.test(r.name) && r.transfer > 0), 'no PNG for the chart on visit 2');

    // Third visit: the decoded image comes from the cache, so nothing is downloaded for it.
    const third = await visit('visit 3');
    assert.equal(third.hash, chartHash, 'chart pixels still match');
    assert.ok(!third.downloaded.some((r) => /chart[-.].*\.(png|bic)$/.test(r.name) && r.transfer > 0), 'no network for the chart on visit 3');
    assert.ok(third.downloaded.some((r) => /icon[-.].*\.png$/.test(r.name)), 'the icon, kept as PNG, is still served');
  } finally {
    await context.close();
    server.close();
  }
}
