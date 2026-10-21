// Vite plugin. During `vite build` it converts the images that pass the
// policy, serves them through a Service Worker under their original URLs, and
// adds the loader that registers that worker to the HTML.
//
//   import { bicVite } from '@proteo5/bic-build/vite';
//   export default { plugins: [bicVite()] };
//
// Limits, stated in the README: images inlined as data URLs and files copied
// from `public/` are not converted; the base path must be absolute.

import { IMAGE_EXTENSIONS } from './build.js';
import { bicBinary } from './bic-cli.js';
import { LOADER_SOURCE, LOADER_FILE, REPORT_FILE, WORKER_FILE, buildWorker, convertImages, injectLoader, resolvePolicy, summary } from './core.js';
import { extname } from 'node:path';

export function bicVite(options = {}) {
  const policy = resolvePolicy(options.policy);
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

      const images = [];
      for (const [fileName, item] of Object.entries(bundle)) {
        if (item.type !== 'asset') continue;
        if (!IMAGE_EXTENSIONS.includes(extname(fileName).toLowerCase())) continue;
        images.push({ name: fileName, bytes: Buffer.from(item.source) });
      }

      const { report, conversions } = await convertImages(images, {
        binary,
        policy,
        warn: (message) => this.warn(message),
      });
      this.emitFile({ type: 'asset', fileName: REPORT_FILE, source: JSON.stringify({ policy, report }, null, 2) });
      if (conversions.length === 0) return;

      const routes = {};
      for (const c of conversions) {
        this.emitFile({ type: 'asset', fileName: c.bicName, source: c.bytes });
        routes[`${base}${c.name}`] = `${base}${c.bicName}`;
      }
      this.emitFile({ type: 'asset', fileName: WORKER_FILE, source: await buildWorker(routes) });
      this.emitFile({ type: 'asset', fileName: LOADER_FILE, source: LOADER_SOURCE });
      for (const item of Object.values(bundle)) {
        if (item.type === 'asset' && item.fileName.endsWith('.html')) {
          item.source = injectLoader(String(item.source), `${base}${LOADER_FILE}`);
        }
      }
      console.info(summary(report, conversions));
    },
  };
}
