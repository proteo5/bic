// webpack 5 plugin. In a production build it converts the images that pass
// the policy, serves them through a Service Worker under their original URLs,
// and adds the loader that registers that worker to the HTML pages.
//
//   // webpack.config.js
//   const { BicWebpackPlugin } = require('@proteo5/bic-build/webpack');
//   module.exports = {
//     mode: 'production',
//     output: { publicPath: '/' },
//     plugins: [new BicWebpackPlugin()],
//   };
//
// Requires an absolute `output.publicPath`, such as '/'. With the default
// 'auto' the plugin converts nothing and says so. Images emitted as data URLs
// (asset/inline) and files copied by CopyWebpackPlugin are not converted.

import { IMAGE_EXTENSIONS } from './build.js';
import { bicBinary } from './bic-cli.js';
import { LOADER_SOURCE, LOADER_FILE, REPORT_FILE, WORKER_FILE, buildWorker, convertImages, injectLoader, resolvePolicy, summary } from './core.js';
import { extname } from 'node:path';

const PLUGIN = 'BicWebpackPlugin';

export class BicWebpackPlugin {
  /**
   * @param {object} [options]
   * @param {object} [options.policy]  Overrides for the conversion policy.
   * @param {string} [options.bic]     Path to the bic binary.
   * @param {boolean} [options.enableInDevelopment=false]  Convert in development builds too.
   */
  constructor(options = {}) {
    this.options = options;
  }

  apply(compiler) {
    const { webpack } = compiler;
    const { RawSource } = webpack.sources;
    const { Compilation, WebpackError } = webpack;
    const policy = resolvePolicy(this.options.policy);
    const binary = bicBinary(this.options.bic);

    compiler.hooks.thisCompilation.tap(PLUGIN, (compilation) => {
      compilation.hooks.processAssets.tapPromise(
        { name: PLUGIN, stage: Compilation.PROCESS_ASSETS_STAGE_REPORT },
        async () => {
          if (compiler.options.mode !== 'production' && !this.options.enableInDevelopment) return;

          const publicPath = compilation.outputOptions.publicPath;
          if (typeof publicPath !== 'string' || !publicPath.startsWith('/')) {
            compilation.warnings.push(new WebpackError(
              `${PLUGIN}: output.publicPath must be an absolute path such as "/" (it is ${JSON.stringify(publicPath)}); no images were converted`
            ));
            return;
          }

          const images = [];
          for (const { name, source } of compilation.getAssets()) {
            if (!IMAGE_EXTENSIONS.includes(extname(name).toLowerCase())) continue;
            images.push({ name, bytes: Buffer.from(source.source()) });
          }

          const { report, conversions } = await convertImages(images, {
            binary,
            policy,
            warn: (message) => compilation.warnings.push(new WebpackError(`${PLUGIN}: ${message}`)),
          });
          compilation.emitAsset(REPORT_FILE, new RawSource(JSON.stringify({ policy, report }, null, 2)));
          if (conversions.length === 0) return;

          const routes = {};
          for (const c of conversions) {
            compilation.emitAsset(c.bicName, new RawSource(c.bytes));
            routes[`${publicPath}${c.name}`] = `${publicPath}${c.bicName}`;
          }
          compilation.emitAsset(WORKER_FILE, new RawSource(await buildWorker(routes)));
          compilation.emitAsset(LOADER_FILE, new RawSource(LOADER_SOURCE));

          for (const { name, source } of compilation.getAssets()) {
            if (!name.endsWith('.html')) continue;
            const html = String(source.source());
            compilation.updateAsset(name, new RawSource(injectLoader(html, `${publicPath}${LOADER_FILE}`)));
          }
          console.info(summary(report, conversions));
        }
      );
    });
  }
}
