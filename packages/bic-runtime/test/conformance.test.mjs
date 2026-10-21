// Conformance: the JavaScript decoder must produce exactly what the Rust
// decoder produced for every fixture (see fixtures/make_fixtures.py).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { decodeBic, parseContainer, sniffImageType, BicError } from '../src/index.js';

const here = dirname(fileURLToPath(import.meta.url));
const fixtures = join(here, 'fixtures');
const manifest = JSON.parse(readFileSync(join(fixtures, 'manifest.json'), 'utf8'));
const bytesOf = (name) => readFileSync(join(fixtures, 'bic', name));
const sha256 = (data) => createHash('sha256').update(data).digest('hex');

for (const entry of manifest) {
  test(`decodes ${entry.id} like the Rust decoder`, () => {
    const decoded = decodeBic(bytesOf(entry.file));
    assert.equal(decoded.kind, entry.kind === 'pixels' ? 'pixels' : 'file');
    assert.equal(decoded.width, entry.width);
    assert.equal(decoded.height, entry.height);
    if (entry.kind === 'pixels') {
      assert.equal(sha256(decoded.rgba), entry.rgba_sha256, 'RGBA bytes differ from the Rust decoder');
      assert.equal(decoded.rgba.length, entry.width * entry.height * 4);
    } else {
      assert.equal(decoded.mime, entry.mime);
      if (entry.sha256) assert.equal(sha256(decoded.bytes), entry.sha256, 'stored file differs');
    }
  });
}

test('the fixtures cover every storage type and the main encoder paths', () => {
  const kinds = new Set(manifest.map((e) => e.kind));
  assert.ok(kinds.has('pixels') && kinds.has('file'));
  const mimes = new Set(manifest.filter((e) => e.kind === 'file').map((e) => e.mime));
  assert.ok(mimes.has('image/png') && mimes.has('image/jpeg'));
  assert.ok(manifest.some((e) => e.id.endsWith('--lossy2')));
  assert.ok(manifest.some((e) => e.id.endsWith('--fast')));
  assert.ok(manifest.some((e) => e.id.startsWith('icon-alpha')));
});

test('reads the header fields of a native file', () => {
  const c = parseContainer(bytesOf('chart--lossless.bic'));
  assert.equal(c.storage, 0);
  assert.deepEqual([c.width, c.height], [120, 80]);
  assert.equal(c.icc, null);
});

test('sniffs embedded image types from their signatures', () => {
  assert.equal(sniffImageType(new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])), 'image/png');
  assert.equal(sniffImageType(new Uint8Array([0xff, 0xd8, 0xff, 0xe0])), 'image/jpeg');
  const webp = new TextEncoder().encode('RIFF\0\0\0\0WEBPVP8 ');
  assert.equal(sniffImageType(webp), 'image/webp');
  const heif = new TextEncoder().encode('\0\0\0\x18ftypheic\0\0\0\0');
  assert.equal(sniffImageType(heif), 'image/heif');
  assert.equal(sniffImageType(new TextEncoder().encode('BM\0\0\0\0\0\0')), null);
});

// Malformed input must fail with BicError, never with a TypeError or a hang.
function header({ storage = 0, flags = 0, width = 1, height = 1, plainLen = 0, payloadLen = 0 } = {}) {
  const b = new Uint8Array(32);
  b.set([0x42, 0x49, 0x43, 0x32, 2, storage]);
  const v = new DataView(b.buffer);
  v.setUint16(6, flags, true);
  v.setUint32(8, width, true);
  v.setUint32(12, height, true);
  v.setUint32(16, plainLen, true);
  v.setUint32(24, payloadLen, true);
  return b;
}

test('rejects malformed containers with BicError', () => {
  const good = bytesOf('chart--lossless.bic');
  const cases = {
    'wrong magic': Object.assign(good.slice(), { 0: 0x00 }),
    'unknown version': Object.assign(good.slice(), { 4: 9 }),
    'unknown flag bit': Object.assign(good.slice(), { 6: 0x06 }),
    'truncated file': good.slice(0, 40),
    'payload length mismatch': Object.assign(good.slice(), { 24: 1 }),
    'unknown storage type': header({ storage: 7, width: 1, height: 1 }),
    'pixel count over the limit': header({ width: 100000, height: 100000 }),
    'stream longer than the image allows': header({ width: 1, height: 1, plainLen: 9000, payloadLen: 0 }),
    'empty input': new Uint8Array(0),
  };
  for (const [name, input] of Object.entries(cases)) {
    assert.throws(() => decodeBic(input), (err) => err instanceof BicError, name);
  }
});

test('random corruption of valid files only ever throws BicError', () => {
  let seed = 12345;
  const random = (n) => {
    seed = (seed * 1103515245 + 12345) & 0x7fffffff;
    return seed % n;
  };
  for (const entry of manifest.filter((e) => e.kind === 'pixels').slice(0, 20)) {
    const original = bytesOf(entry.file);
    for (let trial = 0; trial < 60; trial++) {
      const corrupted = original.slice();
      const flips = 1 + random(4);
      for (let k = 0; k < flips; k++) corrupted[32 + random(corrupted.length - 32)] ^= 1 << random(8);
      try {
        decodeBic(corrupted);
      } catch (err) {
        assert.ok(err instanceof BicError, `${entry.id} trial ${trial}: ${err && err.name}: ${err && err.message}`);
      }
    }
  }
});
