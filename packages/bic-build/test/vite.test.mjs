// The Vite plugin on a small app: one flat chart (converted), one icon and one
// photograph (kept). Checks the output folder, the worker, and the loader.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { build } from 'vite';
import { existsSync, readFileSync, readdirSync, mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { bicVite } from '../src/vite.js';

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, '..', '..', '..');
const exe = process.platform === 'win32' ? 'bic.exe' : 'bic';
const binary = process.env.BIC_BIN || join(root, 'target', 'release', exe);
const available = existsSync(binary);

test('builds converted images, a worker and a loader; keeps the rest', { skip: !available && `no bic binary at ${binary}`, timeout: 600000 }, async () => {
  const outDir = mkdtempSync(join(tmpdir(), 'bic-vite-dist-'));
  await build({
    root: join(here, 'vite-app'),
    logLevel: 'silent',
    plugins: [bicVite({ bic: binary })],
    build: { outDir, emptyOutDir: true },
  });

  const files = readdirSync(join(outDir, 'assets'));
  const bic = files.filter((f) => f.endsWith('.bic'));
  assert.equal(bic.length, 1, `one converted image, got ${files.join(', ')}`);
  assert.ok(bic[0].startsWith('chart-'), bic[0]);
  assert.equal(readFileSync(join(outDir, 'assets', bic[0])).subarray(0, 4).toString('latin1'), 'BIC2');

  assert.ok(existsSync(join(outDir, 'bic-sw.js')), 'worker emitted');
  assert.ok(existsSync(join(outDir, 'bic-loader.js')), 'loader emitted');
  const html = readFileSync(join(outDir, 'index.html'), 'utf8');
  assert.match(html, /<script src="\/bic-loader\.js" defer><\/script><\/head>/);

  const worker = readFileSync(join(outDir, 'bic-sw.js'), 'utf8');
  const chartPng = files.find((f) => f.startsWith('chart-') && f.endsWith('.png'));
  assert.ok(worker.includes(`/assets/${chartPng}`), 'worker routes the PNG URL');
  assert.ok(worker.includes(`/assets/${bic[0]}`), 'worker fetches the BIC file');
  assert.ok(!worker.includes('__BIC_ROUTES__'), 'placeholder replaced');

  const report = JSON.parse(readFileSync(join(outDir, 'bic-report.json'), 'utf8'));
  const unhashed = (file) => basename(file).replace(/-[A-Za-z0-9_-]+(\.[a-z]+)$/, '$1');
  const byFile = Object.fromEntries(report.report.map((e) => [unhashed(e.file), e]));
  assert.equal(byFile['chart.png'].use, true, byFile['chart.png'].reason);
  assert.equal(byFile['icon.png'].use, false);
  assert.equal(byFile['photo.jpg'].use, false);
});
