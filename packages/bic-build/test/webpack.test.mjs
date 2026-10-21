// The webpack plugin on the fixture app: converted chart, kept icon and photo,
// the worker, and the loader in the HTML.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, mkdtempSync, readdirSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { buildWebpackApp } from './webpack-app/build.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, '..', '..', '..');
const exe = process.platform === 'win32' ? 'bic.exe' : 'bic';
const binary = process.env.BIC_BIN || join(root, 'target', 'release', exe);

test('webpack: converts the chart, keeps the rest, emits the worker and the loader', { skip: !existsSync(binary) && `no bic binary at ${binary}`, timeout: 600000 }, async () => {
  const outDir = mkdtempSync(join(tmpdir(), 'bic-webpack-dist-'));
  await buildWebpackApp(outDir, binary);

  const assets = readdirSync(join(outDir, 'assets'));
  const bic = assets.filter((f) => f.endsWith('.bic'));
  assert.equal(bic.length, 1, `one converted image, got ${assets.join(', ')}`);
  assert.ok(bic[0].startsWith('chart.'), bic[0]);
  assert.equal(readFileSync(join(outDir, 'assets', bic[0])).subarray(0, 4).toString('latin1'), 'BIC2');

  assert.ok(existsSync(join(outDir, 'bic-sw.js')), 'worker emitted');
  assert.ok(existsSync(join(outDir, 'bic-loader.js')), 'loader emitted');
  const html = readFileSync(join(outDir, 'index.html'), 'utf8');
  assert.match(html, /<script src="\/bic-loader\.js" defer><\/script><\/head>/);

  const worker = readFileSync(join(outDir, 'bic-sw.js'), 'utf8');
  const chartPng = assets.find((f) => f.startsWith('chart.') && f.endsWith('.png'));
  assert.ok(worker.includes(`/assets/${chartPng}`), 'worker routes the PNG URL');
  assert.ok(worker.includes(`/assets/${bic[0]}`), 'worker fetches the BIC file');
  assert.ok(!worker.includes('__BIC_ROUTES__'), 'placeholder replaced');

  const report = JSON.parse(readFileSync(join(outDir, 'bic-report.json'), 'utf8'));
  const byFile = Object.fromEntries(report.report.map((e) => [basename(e.file).replace(/\.[0-9a-f]+(\.[a-z]+)$/, '$1'), e]));
  assert.equal(byFile['chart.png'].use, true, byFile['chart.png'].reason);
  assert.equal(byFile['icon.png'].use, false);
  assert.equal(byFile['photo.jpg'].use, false);
});

test('webpack: needs an absolute publicPath, and says so when it is missing', { timeout: 600000 }, async () => {
  const outDir = mkdtempSync(join(tmpdir(), 'bic-webpack-auto-'));
  const { BicWebpackPlugin } = await import('../src/webpack.js');
  const webpack = (await import('webpack')).default;
  const HtmlWebpackPlugin = (await import('html-webpack-plugin')).default;
  const stats = await new Promise((resolve, reject) => {
    webpack({
      mode: 'production',
      context: join(here, 'webpack-app'),
      entry: './src/index.js',
      output: { path: outDir, filename: 'app.js' },
      module: { rules: [{ test: /\.(png|jpe?g)$/, type: 'asset/resource' }] },
      plugins: [new HtmlWebpackPlugin({ template: join(here, 'webpack-app', 'index.html') }), new BicWebpackPlugin({ bic: binary })],
      infrastructureLogging: { level: 'error' },
    }, (error, result) => (error ? reject(error) : resolve(result)));
  });
  assert.ok(stats.compilation.warnings.some((w) => /publicPath must be an absolute path/.test(w.message)));
  assert.ok(!existsSync(join(outDir, 'bic-sw.js')));
});
