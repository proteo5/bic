// Fetches the bic binary for this platform from the BIC GitHub release that
// matches this package's version, checks it against the release's
// SHA256SUMS.txt, and keeps it in vendor/. Runs on postinstall.
//
// Environment:
//   BIC_SKIP_DOWNLOAD=1  do not download (for offline installs)
//   BIC_BIN=/path/bic    use this binary instead (see bicBinary in bic-cli.js)

import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import {
  chmodSync, copyFileSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

export const RELEASE_REPO = 'proteo5/bic';

const PACKAGE_DIR = join(dirname(fileURLToPath(import.meta.url)), '..');
export const VENDOR_DIR = join(PACKAGE_DIR, 'vendor');

/** The release target triple and archive type for a Node platform, or null if none. */
export function targetFor(platform = process.platform, arch = process.arch) {
  if (platform === 'win32' && arch === 'x64') return { target: 'x86_64-pc-windows-msvc', archive: 'zip' };
  if (platform === 'linux' && arch === 'x64') return { target: 'x86_64-unknown-linux-gnu', archive: 'tar.gz' };
  if (platform === 'darwin' && arch === 'arm64') return { target: 'aarch64-apple-darwin', archive: 'tar.gz' };
  if (platform === 'darwin' && arch === 'x64') return { target: 'x86_64-apple-darwin', archive: 'tar.gz' };
  return null;
}

/** Executable suffix for a platform: '.exe' on Windows. */
export function exeSuffix(platform = process.platform) {
  return platform === 'win32' ? '.exe' : '';
}

export function assetName(version, target, archive) {
  return `bic-v${version}-${target}.${archive}`;
}

export function releaseUrl(version, file) {
  return `https://github.com/${RELEASE_REPO}/releases/download/v${version}/${file}`;
}

/** Parses `sha256  filename` lines, as written by sha256sum. */
export function parseChecksums(text) {
  const sums = new Map();
  for (const line of text.split('\n')) {
    const match = /^([0-9a-f]{64})\s+\*?(\S+)\s*$/.exec(line.trim());
    if (match) sums.set(match[2], match[1]);
  }
  return sums;
}

export function sha256(data) {
  return createHash('sha256').update(data).digest('hex');
}

async function download(url) {
  const response = await fetch(url, { redirect: 'follow' });
  if (!response.ok) throw new Error(`${url} returned ${response.status}`);
  return Buffer.from(await response.arrayBuffer());
}

/** Extracts an archive with the system tar (bsdtar on Windows also reads zip). */
function extract(archivePath, into) {
  const tar = process.platform === 'win32' && process.env.SystemRoot
    ? join(process.env.SystemRoot, 'System32', 'tar.exe')
    : 'tar';
  execFileSync(tar, ['-xf', archivePath, '-C', into], { stdio: 'pipe' });
}

/**
 * Downloads and installs the binary for `version`. Returns the path of the
 * installed `bic` executable.
 */
export async function installBinary({ version, vendorDir = VENDOR_DIR, platform, arch } = {}) {
  const info = targetFor(platform, arch);
  if (!info) throw new Error(`no prebuilt bic binary for ${platform ?? process.platform}-${arch ?? process.arch}; set BIC_BIN`);

  const file = assetName(version, info.target, info.archive);
  const sums = parseChecksums((await download(releaseUrl(version, 'SHA256SUMS.txt'))).toString('utf8'));
  const expected = sums.get(file);
  if (!expected) throw new Error(`${file} is not listed in the release checksums`);

  const archive = await download(releaseUrl(version, file));
  if (sha256(archive) !== expected) throw new Error(`checksum mismatch for ${file}`);

  const work = mkdtempSync(join(tmpdir(), 'bic-install-'));
  try {
    const archivePath = join(work, file);
    writeFileSync(archivePath, archive);
    extract(archivePath, work);
    const folder = readdirSync(work).find((name) => name.startsWith('bic-v') && !name.endsWith(`.${info.archive}`));
    if (!folder) throw new Error(`${file} does not contain the expected folder`);

    mkdirSync(vendorDir, { recursive: true });
    const exe = exeSuffix(platform);
    for (const name of [`bic${exe}`, `bic-view${exe}`]) {
      const from = join(work, folder, name);
      if (!existsSync(from)) continue;
      const to = join(vendorDir, name);
      copyFileSync(from, to);
      chmodSync(to, 0o755);
    }
    const installed = join(vendorDir, `bic${exe}`);
    if (!existsSync(installed)) throw new Error('the archive did not contain bic');
    return installed;
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
}

/** The vendored binary, if the install step put one there. */
export function vendoredBinary(vendorDir = VENDOR_DIR) {
  const path = join(vendorDir, `bic${exeSuffix()}`);
  return existsSync(path) ? path : null;
}

// postinstall entry point. Failing here would break offline installs, so a
// failure is reported and the package keeps working with BIC_BIN or a PATH binary.
const isMain = process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1];
if (isMain) {
  if (process.env.BIC_SKIP_DOWNLOAD) {
    console.log('bic-build: BIC_SKIP_DOWNLOAD set, not downloading the bic binary');
  } else if (vendoredBinary()) {
    // Already installed.
  } else {
    const pkg = JSON.parse(readFileSync(join(PACKAGE_DIR, 'package.json'), 'utf8'));
    try {
      const path = await installBinary({ version: pkg.version });
      console.log(`bic-build: installed ${path}`);
    } catch (error) {
      console.warn(`bic-build: could not install the bic binary (${error.message}).`);
      console.warn('bic-build: set BIC_BIN to a bic executable, or put bic on the PATH.');
    }
  }
}
