// The installer against the real v0.2.0 release (needs network access).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  targetFor, assetName, parseChecksums, sha256, exeSuffix, installBinary, releaseUrl,
} from '../src/install-binary.js';

test('maps each supported platform to its release asset', () => {
  assert.deepEqual(targetFor('win32', 'x64'), { target: 'x86_64-pc-windows-msvc', archive: 'zip' });
  assert.deepEqual(targetFor('linux', 'x64'), { target: 'x86_64-unknown-linux-gnu', archive: 'tar.gz' });
  assert.deepEqual(targetFor('darwin', 'arm64'), { target: 'aarch64-apple-darwin', archive: 'tar.gz' });
  assert.deepEqual(targetFor('darwin', 'x64'), { target: 'x86_64-apple-darwin', archive: 'tar.gz' });
  assert.equal(targetFor('freebsd', 'x64'), null);
  assert.equal(assetName('0.2.0', 'x86_64-pc-windows-msvc', 'zip'), 'bic-v0.2.0-x86_64-pc-windows-msvc.zip');
  assert.equal(releaseUrl('0.2.0', 'SHA256SUMS.txt'), 'https://github.com/proteo5/bic/releases/download/v0.2.0/SHA256SUMS.txt');
  assert.equal(exeSuffix('win32'), '.exe');
  assert.equal(exeSuffix('linux'), '');
});

test('reads sha256sum output, with and without the binary-mode marker', () => {
  const hash = 'a'.repeat(64);
  const sums = parseChecksums(`${hash}  bic-v0.2.0-x86_64-unknown-linux-gnu.tar.gz\n${'b'.repeat(64)} *bic.zip\nnot a line\n`);
  assert.equal(sums.get('bic-v0.2.0-x86_64-unknown-linux-gnu.tar.gz'), hash);
  assert.equal(sums.get('bic.zip'), 'b'.repeat(64));
  assert.equal(sha256(Buffer.from('')), 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855');
});

test('downloads, verifies and installs the release binary, which then runs', { timeout: 300000 }, async () => {
  const vendor = mkdtempSync(join(tmpdir(), 'bic-vendor-'));
  const path = await installBinary({ version: '0.2.0', vendorDir: vendor });
  assert.ok(existsSync(path), 'bic executable exists');
  // The v0.2.0 release predates `bic inspect`, so only the shared commands are checked here.
  const help = execFileSync(path, ['--help'], { encoding: 'utf8' });
  assert.match(help, /encode/);
  assert.match(help, /verify/);
});
