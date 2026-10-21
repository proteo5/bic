// Builds the webpack fixture app with the BIC plugin. Used by the tests.
import webpack from 'webpack';
import HtmlWebpackPlugin from 'html-webpack-plugin';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { BicWebpackPlugin } from '../../src/webpack.js';

const here = dirname(fileURLToPath(import.meta.url));

export function buildWebpackApp(outDir, bic) {
  const config = {
    mode: 'production',
    context: here,
    entry: './src/index.js',
    output: {
      path: outDir,
      publicPath: '/',
      filename: 'js/[name].[contenthash].js',
      clean: true,
    },
    module: {
      rules: [{
        test: /\.(png|jpe?g)$/,
        type: 'asset/resource',
        generator: { filename: 'assets/[name].[contenthash][ext]' },
      }],
    },
    plugins: [
      new HtmlWebpackPlugin({ template: join(here, 'index.html') }),
      new BicWebpackPlugin({ bic }),
    ],
    infrastructureLogging: { level: 'error' },
    stats: 'errors-only',
  };
  return new Promise((resolve, reject) => {
    webpack(config, (error, stats) => {
      if (error) return reject(error);
      if (stats.hasErrors()) return reject(new Error(stats.toString('errors-only')));
      resolve(stats);
    });
  });
}
