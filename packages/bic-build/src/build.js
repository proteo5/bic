// Walks an input folder, measures every image, and writes a .bic next to each
// one that passes the policy. Images that do not pass keep their original
// file; the manifest records what happened to each.

import { mkdir, readdir, stat, writeFile } from 'node:fs/promises';
import { join, relative, extname, basename, dirname, parse } from 'node:path';
import { bicBinary, encode, inspect } from './bic-cli.js';
import { DEFAULT_POLICY, decide } from './policy.js';

// Formats `bic inspect` can read.
export const IMAGE_EXTENSIONS = [
  '.png', '.jpg', '.jpeg', '.webp', '.bmp', '.gif', '.tif', '.tiff', '.qoi', '.heic', '.heif',
];

async function* walk(dir) {
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) yield* walk(path);
    else if (entry.isFile() && IMAGE_EXTENSIONS.includes(extname(entry.name).toLowerCase())) yield path;
  }
}

/**
 * @param {object} options
 * @param {string} options.input   Folder with the source images.
 * @param {string} options.output  Folder for the .bic files and the manifest.
 * @param {string} [options.bic]   Path to the bic binary (default: BIC_BIN or `bic`).
 * @param {object} [options.policy] Overrides for DEFAULT_POLICY.
 * @returns {Promise<{ entries: object[], totals: object }>}
 */
export async function optimizeDirectory({ input, output, bic, policy = {} }) {
  const binary = bicBinary(bic);
  const effective = { ...DEFAULT_POLICY, ...policy };
  const entries = [];

  for await (const file of walk(input)) {
    const rel = relative(input, file);
    const measurement = await inspect(file, binary);
    const decision = decide(measurement, effective);
    const sourceBytes = (await stat(file)).size;
    const entry = {
      source: rel.split('\\').join('/'),
      use: decision.use,
      reason: decision.reason,
      sourceBytes,
      bestLossless: decision.best,
      bicBytes: measurement.sizes.bic,
      bic: null,
    };
    if (decision.use) {
      const target = join(output, dirname(rel), `${parse(basename(file)).name}.bic`);
      await mkdir(dirname(target), { recursive: true });
      await encode(file, target, binary);
      entry.bic = relative(output, target).split('\\').join('/');
    }
    entries.push(entry);
  }

  const converted = entries.filter((e) => e.use);
  const totals = {
    images: entries.length,
    converted: converted.length,
    sourceBytesConverted: converted.reduce((sum, e) => sum + e.sourceBytes, 0),
    bicBytesConverted: converted.reduce((sum, e) => sum + e.bicBytes, 0),
  };
  totals.savedBytes = totals.sourceBytesConverted - totals.bicBytesConverted;

  await mkdir(output, { recursive: true });
  await writeFile(
    join(output, 'bic-manifest.json'),
    JSON.stringify({ policy: effective, totals, entries }, null, 2) + '\n'
  );
  return { entries, totals };
}
