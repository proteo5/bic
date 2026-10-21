import { test } from 'node:test';
import assert from 'node:assert/strict';
import { decide, DEFAULT_POLICY } from '../src/policy.js';

const measured = (overrides = {}) => ({
  width: 100,
  height: 100,
  distinct_colors: 50,
  sizes: { png: 1000, oxipng: 800, webp_lossless: 900, bic: 600 },
  bic_storage: 'native',
  ...overrides,
});

test('converts when BIC saves at least the threshold against the best lossless format', () => {
  const d = decide(measured());
  assert.equal(d.use, true);
  assert.equal(d.best.name, 'oxipng');
  assert.ok(Math.abs(d.saving - 0.25) < 1e-9);
});

test('keeps the original when the saving is below the threshold', () => {
  const d = decide(measured({ sizes: { png: 1000, oxipng: 800, webp_lossless: 900, bic: 760 } }));
  assert.equal(d.use, false);
  assert.match(d.reason, /below the 10% threshold/);
});

test('keeps the original when BIC is larger', () => {
  const d = decide(measured({ sizes: { png: 1000, oxipng: 800, webp_lossless: 900, bic: 900 } }));
  assert.equal(d.use, false);
  assert.ok(d.saving < 0);
  assert.match(d.reason, /12.5% larger than oxipng/);
});

test('picks the smallest of PNG, oxipng and WebP lossless as the baseline', () => {
  const d = decide(measured({ sizes: { png: 1000, oxipng: 800, webp_lossless: 500, bic: 400 } }));
  assert.equal(d.best.name, 'WebP lossless');
  assert.equal(d.best.bytes, 500);
  assert.ok(Math.abs(d.saving - 0.2) < 1e-9);
});

test('never converts photograph-like images, whatever the size', () => {
  const d = decide(measured({ distinct_colors: 5000, sizes: { png: 1000, oxipng: 800, webp_lossless: 900, bic: 100 } }));
  assert.equal(d.use, false);
  assert.match(d.reason, /above the 4096 limit/);
});

test('never converts a source that BIC keeps as its original lossy file', () => {
  const d = decide(measured({ bic_storage: 'original' }));
  assert.equal(d.use, false);
});

test('thresholds can be changed per project', () => {
  const m = measured({ sizes: { png: 1000, oxipng: 800, webp_lossless: 900, bic: 680 } });
  assert.equal(decide(m).use, true, 'saves 15%, above the default 10%');
  assert.equal(decide(m, { minSaving: 0.3 }).use, false);
  assert.equal(decide(measured({ distinct_colors: 9000 }), { maxDistinctColors: 10000 }).use, true);
  assert.equal(DEFAULT_POLICY.minSaving, 0.1);
});
