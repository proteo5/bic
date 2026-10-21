// Vite plugin. During `vite build` it converts the images that pass the
// policy, serves them through a Service Worker under their original URLs, and
// adds the loader that registers that worker to the HTML.
//
//   import { bicVite } from '@proteo5/bic-build/vite';
//   export default { plugins: [bicVite()] };
//
// Limits, stated in the README: images inlined as data URLs and files copied
// from `public/` are not converted; the base path must be absolute.

import { build as esbuild } from 'esbuild';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, extname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { bicBinary, encode, inspect } from './bic-cli.js';
import { IMAGE_EXTENSIONS } from './build.js';
import { DEFAULT_POLICY, decide } from './policy.js';

const here = dirname(fileURLToPath(import.meta.url));
const WORKER_FILE = 'bic-sw.js';
const LOADER_FILE = 'bic-loader.js';
const REPORT_FILE = 'bic-report.json';
// JPEG is a lossy format that BIC keeps as it is, so measuring it is wasted work.
const SKIP_EXTENSIONS = ['.jpg', '.jpeg'];

export function bicVite(options = {}) {
  const policy = { ...DEFAULT_POLICY, ...options.policy };
  const binary = bicBinary(options.bic);
  let base = '/';

  return {
    name: 'bic',
    apply: 'build',
    // Run after Vite's own build hooks, so the HTML pages are already in the bundle.
    enforce: 'post',

    configResolved(config) {
      base = config.base;
    },

    async generateBundle(_outputOptions, bundle) {
      if (!base.startsWith('/')) {
        this.warn(`bic: base "${base}" is not an absolute path; no images were converted`);
        return;
      }

      const routes = {};
      const report = [];
      const work = mkdtempSync(join(tmpdir(), 'bic-vite-'));
      try {
        let index = 0;
        for (const [fileName, item] of Object.entries(bundle)) {
          if (item.type !== 'asset') continue;
          const ext = extname(fileName).toLowerCase();
          if (!IMAGE_EXTENSIONS.includes(ext)) continue;
          index += 1;
          const outcome = await convert(this, item, fileName, ext, work, index, binary, policy);
          report.push(outcome.entry);
          if (outcome.bicName) {
            const bicName = outcome.bicName;
            this.emitFile({ type: 'asset', fileName: bicName, source: outcome.bytes });
            routes[`${base}${fileName}`] = `${base}${bicName}`;
          }
        }
      } finally {
        rmSync(work, { recursive: true, force: true });
      }

      this.emitFile({ type: 'asset', fileName: REPORT_FILE, source: JSON.stringify({ policy, report }, null, 2) });
      const converted = report.filter((e) => e.use);
      if (converted.length === 0) return;

      const worker = await buildWorker(routes);
      this.emitFile({ type: 'asset', fileName: WORKER_FILE, source: worker });
      this.emitFile({ type: 'asset', fileName: LOADER_FILE, source: readFileSync(join(here, 'loader.js'), 'utf8') });
      injectLoader(bundle, `${base}${LOADER_FILE}`);

      const saved = converted.reduce((sum, e) => sum + e.sourceBytes - e.bicBytes, 0);
      console.info(`bic: ${converted.length} of ${report.length} images stored as BIC (${saved} bytes less than the sources)`);
    },
  };
}

async function convert(plugin, item, fileName, ext, work, index, binary, policy) {
  const sourceBytes = Buffer.byteLength(item.source);
  const entry = { file: fileName, use: false, reason: '', sourceBytes, bicBytes: null };
  if (SKIP_EXTENSIONS.includes(ext)) {
    entry.reason = 'JPEG is kept as it is';
    return { entry };
  }

  const source = join(work, `${index}-${basename(fileName)}`);
  const target = `${source}.bic`;
  try {
    writeFileSync(source, item.source);
    const measurement = await inspect(source, binary);
    const decision = decide(measurement, policy);
    entry.reason = decision.reason;
    entry.use = decision.use;
    if (!decision.use) return { entry };

    await encode(source, target, binary);
    const bytes = readFileSync(target);
    entry.bicBytes = bytes.length;
    const bicName = fileName.replace(/\.[^./]+$/, '.bic');
    if (bicName === fileName) throw new Error(`cannot name the BIC file for ${fileName}`);
    return { entry, bicName, bytes };
  } catch (error) {
    // A failed conversion must not break the build: the original stays.
    entry.use = false;
    entry.reason = `not converted: ${error.message}`;
    plugin.warn(`bic: ${fileName}: ${error.message}`);
    return { entry };
  }
}

/** Bundles the worker with the decoder, with the routes built in. */
async function buildWorker(routes) {
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

/** Adds the loader script to every HTML page of the build. */
function injectLoader(bundle, loaderUrl) {
  const tag = `<script src="${loaderUrl}" defer></script>`;
  for (const item of Object.values(bundle)) {
    if (item.type !== 'asset' || !item.fileName.endsWith('.html')) continue;
    const html = String(item.source);
    if (html.includes('</head>')) item.source = html.replace('</head>', `${tag}</head>`);
  }
}
