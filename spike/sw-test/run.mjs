// Spike test: does a Service Worker make <img src="x.bic"> show the right
// pixels, and how many bytes cross the network on each visit?
//
// Each scenario uses its own throwaway Chrome profile, so the first visit
// really has no Service Worker. Nothing from your browser profile is read.
import { chromium } from 'playwright-core';
import { mkdtempSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const CHROME = 'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe';
const BASE = 'http://localhost:8124/';
const hashes = Object.fromEntries(
  JSON.parse(readFileSync(new URL('../wasm-decoder/test/expected.json', import.meta.url), 'utf8'))
    .map((e) => [e.id, e.sha256])
);

async function newContext() {
  return chromium.launchPersistentContext(mkdtempSync(join(tmpdir(), 'bic-sw-')), {
    executablePath: CHROME,
    headless: true,
    args: ['--no-first-run', '--no-default-browser-check'],
  });
}

async function visit(context, label, path) {
  const page = await context.newPage();
  await page.goto(BASE + path, { waitUntil: 'load' });
  // Let the Service Worker finish installing and claiming the page.
  await page.waitForTimeout(1500);

  const state = await page.evaluate(async () => {
    const images = [...document.images].map((img) => ({
      file: img.currentSrc.split('/').pop(),
      width: img.naturalWidth,
      height: img.naturalHeight,
    }));
    // Pixel check for the bar chart: draw it and hash the RGBA bytes.
    const img = document.getElementById('chart');
    const canvas = document.createElement('canvas');
    canvas.width = img.naturalWidth;
    canvas.height = img.naturalHeight;
    const ctx = canvas.getContext('2d');
    ctx.drawImage(img, 0, 0);
    const data = ctx.getImageData(0, 0, canvas.width, canvas.height).data;
    const digest = new Uint8Array(await crypto.subtle.digest('SHA-256', data));
    // transferSize: bytes that crossed the network for this page; 0 when the
    // Service Worker answered the request.
    const resources = performance.getEntriesByType('resource')
      .filter((e) => /\.(bic|png|jpg|wasm)$/.test(e.name))
      .map((e) => ({ url: e.name, bytes: e.transferSize || 0 }));
    return {
      controlled: !!navigator.serviceWorker.controller,
      images,
      resources,
      hash: [...digest].map((b) => b.toString(16).padStart(2, '0')).join(''),
      workerLog: document.getElementById('log') ? document.getElementById('log').textContent : '',
    };
  });

  // Bytes the Service Worker fetched from the network, reported in its messages.
  const workerBytes = state.workerLog
    .split('\n')
    .map((line) => /network=(\d+) B/.exec(line))
    .filter(Boolean)
    .map((m) => Number(m[1]))
    .reduce((a, b) => a + b, 0);
  const pageBytes = state.resources.reduce((sum, r) => sum + r.bytes, 0);
  const total = pageBytes + workerBytes;
  const pixelsOk = state.hash === hashes['bar-chart'];

  console.log(`\n== ${label}: controlled by SW = ${state.controlled}`);
  for (const r of state.resources) {
    console.log(`   page   ${r.url.split('/').pop().padEnd(26)} ${String(r.bytes).padStart(8)} B`);
  }
  console.log(`   worker fetched from network    ${String(workerBytes).padStart(8)} B`);
  console.log(`   total for images               ${String(total).padStart(8)} B`);
  for (const i of state.images) {
    console.log(`   <img> ${i.file.padEnd(26)} ${i.width}x${i.height}`);
  }
  console.log(`   bar chart pixels match source: ${pixelsOk}`);
  return { total, pixelsOk, controlled: state.controlled };
}

async function scenario(name, path) {
  const context = await newContext();
  try {
    const first = await visit(context, `${name}: first visit`, path);
    const second = await visit(context, `${name}: second visit`, path);
    return { first, second };
  } finally {
    await context.close();
  }
}

const results = {
  index: await scenario('index.html', 'index.html'),
  deferred: await scenario('index-deferred.html', 'index-deferred.html'),
};

console.log('\nsummary (bytes over the network for the three images; WASM not included):');
for (const [name, r] of Object.entries(results)) {
  console.log(
    `  ${name.padEnd(9)} first visit ${String(r.first.total).padStart(8)} B` +
    `   second visit ${String(r.second.total).padStart(8)} B` +
    `   second-visit pixels match: ${r.second.pixelsOk}`
  );
}
