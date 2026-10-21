// Integration: the real bic binary on the demo images. Needs a release build
// (cargo build --release) or BIC_BIN pointing at one; skipped otherwise.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, mkdtempSync, copyFileSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { optimizeDirectory } from '../src/index.js';

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, '..', '..', '..');
const sources = join(root, 'site', 'sources');
const exe = process.platform === 'win32' ? 'bic.exe' : 'bic';
const binary = process.env.BIC_BIN || join(root, 'target', 'release', exe);
const available = existsSync(binary) && existsSync(sources);

test('converts only the images that pass the policy, and verifies them', { skip: !available && `no bic binary at ${binary}` }, async () => {
  const input = mkdtempSync(join(tmpdir(), 'bic-build-in-'));
  const output = mkdtempSync(join(tmpdir(), 'bic-build-out-'));
  for (const name of ['bar-chart.png', 'icon.png', 'flowchart.png', 'photo-landscape-1.jpg']) {
    copyFileSync(join(sources, name), join(input, name));
  }
  const { entries, totals } = await optimizeDirectory({ input, output, bic: binary });

  const byName = Object.fromEntries(entries.map((e) => [e.source, e]));
  assert.equal(byName['bar-chart.png'].use, true, byName['bar-chart.png'].reason);
  assert.equal(byName['icon.png'].use, false, 'icon saves about 0.2 %');
  assert.equal(byName['flowchart.png'].use, false, 'BIC is larger than oxipng here');
  assert.equal(byName['photo-landscape-1.jpg'].use, false, 'the JPEG is kept as it is');

  assert.equal(totals.images, 4);
  assert.equal(totals.converted, 1);
  assert.equal(byName['bar-chart.png'].bic, 'bar-chart.bic');
  const written = readFileSync(join(output, 'bar-chart.bic'));
  assert.equal(written.subarray(0, 4).toString('latin1'), 'BIC2');
  assert.equal(written.length, byName['bar-chart.png'].bicBytes);
  const manifest = JSON.parse(readFileSync(join(output, 'bic-manifest.json'), 'utf8'));
  assert.equal(manifest.totals.converted, 1);
});
