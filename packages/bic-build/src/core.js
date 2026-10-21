// Bundler-independent part of the plugins: converting images, bundling the
// worker, and the loader. The Vite and webpack plugins call these.

import { build as esbuild } from 'esbuild';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, extname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { encode, inspect } from './bic-cli.js';
import { DEFAULT_POLICY, decide } from './policy.js';

const here = dirname(fileURLToPath(import.meta.url));

export const WORKER_FILE = 'bic-sw.js';
export const LOADER_FILE = 'bic-loader.js';
export const REPORT_FILE = 'bic-report.json';
// JPEG is a lossy format that BIC keeps as it is, so measuring it is wasted work.
const SKIP_EXTENSIONS = ['.jpg', '.jpeg'];

export const LOADER_SOURCE = readFileSync(join(here, 'loader.js'), 'utf8');

/** Policy with defaults filled in. */
export function resolvePolicy(policy = {}) {
  return { ...DEFAULT_POLICY, ...policy };
}

/**
 * Measures and converts a list of images.
 *
 * @param {Array<{ name: string, bytes: Buffer }>} images  Assets of the build; `name` is their output path.
 * @returns {Promise<{ report: object[], conversions: Array<{ name: string, bicName: string, bytes: Buffer }> }>}
 */
export async function convertImages(images, { binary, policy, warn = () => {} }) {
  const work = mkdtempSync(join(tmpdir(), 'bic-convert-'));
  const report = [];
  const conversions = [];
  try {
    let index = 0;
    for (const { name, bytes } of images) {
      index += 1;
      const ext = extname(name).toLowerCase();
      const entry = { file: name, use: false, reason: '', sourceBytes: bytes.length, bicBytes: null };
      report.push(entry);
      if (SKIP_EXTENSIONS.includes(ext)) {
        entry.reason = 'JPEG is kept as it is';
        continue;
      }
      const source = join(work, `${index}-${basename(name)}`);
      const target = `${source}.bic`;
      try {
        writeFileSync(source, bytes);
        const measurement = await inspect(source, binary);
        const decision = decide(measurement, policy);
        entry.reason = decision.reason;
        if (!decision.use) continue;

        await encode(source, target, binary);
        const out = readFileSync(target);
        const bicName = name.replace(/\.[^./]+$/, '.bic');
        if (bicName === name) throw new Error(`cannot name the BIC file for ${name}`);
        entry.use = true;
        entry.bicBytes = out.length;
        conversions.push({ name, bicName, bytes: out });
      } catch (error) {
        // A failed conversion must not break the build: the original stays.
        entry.use = false;
        entry.reason = `not converted: ${error.message}`;
        warn(`bic: ${name}: ${error.message}`);
      }
    }
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
  return { report, conversions };
}

/** Bundles the worker with the decoder, with the routes built in. */
export async function buildWorker(routes) {
  const result = await esbuild({
    entryPoints: [join(here, 'service-worker.js')],
    bundle: true,
    format: 'iife',
    target: 'es2020',
    minify: true,
    write: false,
    define: { __BIC_ROUTES__: JSON.stringify(routes) },
  });
  return Buffer.from(result.outputFiles[0].contents);
}

/** Inserts the loader script before </head>. Returns the HTML unchanged if there is no head. */
export function injectLoader(html, loaderUrl) {
  const tag = `<script src="${loaderUrl}" defer></script>`;
  return html.includes('</head>') ? html.replace('</head>', `${tag}</head>`) : html;
}

export function summary(report, conversions) {
  const saved = conversions.reduce((sum, c) => {
    const entry = report.find((e) => e.file === c.name);
    return sum + (entry ? entry.sourceBytes - c.bytes.length : 0);
  }, 0);
  return `bic: ${conversions.length} of ${report.length} images stored as BIC (${saved} bytes less than the sources)`;
}
