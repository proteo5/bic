# @proteo5/bic-build

Stores flat graphics (charts, logos, icons) as BIC when that saves bandwidth,
and leaves everything else alone. The browser shows them through a Service
Worker, under their original URLs.

Documentation and a live demo: https://proteo5.github.io/bic/docs/
(Español: https://proteo5.github.io/bic/es/docs/)

## Vite

```js
// vite.config.js
import { bicVite } from '@proteo5/bic-build/vite';

export default {
  plugins: [bicVite()],
};
```

`vite build` then:

1. measures every image and keeps the BIC version of those that pass the policy below;
2. emits `bic-sw.js`, a worker that answers requests for the converted images, and
   `bic-loader.js`, which registers it. The loader is added to each HTML page;
3. writes `bic-report.json` with the decision and its reason for every image.

Nothing in your HTML, CSS or JavaScript changes. `<img src="chart.png">` and
`background: url(chart.png)` keep their URLs. The worker answers them with the
decoded BIC image. Without the worker, which is the first visit, the browser
downloads the PNG as usual, so nobody downloads a file twice. After the first
visit the worker stores the decoded image in the browser's cache, so the
network is used once per image per browser.

The plugin runs in `vite build` only. In `vite dev` images load as before.
Vite 7 is the tested version.

## webpack

```js
// webpack.config.mjs (or webpack.config.js in a project with "type": "module")
import { BicWebpackPlugin } from '@proteo5/bic-build/webpack';

module.exports = {
  mode: 'production',
  output: { publicPath: '/' },   // must be absolute; 'auto' converts nothing
  plugins: [new BicWebpackPlugin()],
};
```

It works the same way as the Vite plugin: a worker under the original URLs and
a loader added to each HTML page (it needs `html-webpack-plugin` or another
plugin that writes the HTML). The plugin runs in production builds; pass
`enableInDevelopment: true` to run it in development too.

## Which images are converted

An image is converted only if all of these hold:

- it is not a JPEG (JPEG is lossy, and BIC keeps it as it is);
- it has at most 4096 distinct colors (more colors means a photograph-like image);
- the BIC file is at least 10 % smaller than the best lossless alternative (PNG, oxipng or WebP lossless).

Measured on the project's demo images, only the bar chart passes. Interfaces,
pixel art and flowcharts are larger in BIC than in oxipng, so they stay PNG.

Thresholds: `bicVite({ policy: { minSaving: 0.2, maxDistinctColors: 2000 } })`.

## Limits

- The base path must be absolute (`/`, `/app/`). With a relative base the plugin
  converts nothing and says so.
- Images that Vite inlines as data URLs, and files copied from `public/`, are not converted.
- Browsers need a secure context for Service Workers: HTTPS, or `localhost`.
- Only images that the page requests from its own origin are served by the worker.
- Images that a page loads from another origin are not converted.

## Command line

```bash
bic-build src/images dist/bic
```

Writes the converted `.bic` files and `bic-manifest.json` to the output folder.
Options: `--min-saving 10` (percent), `--max-colors 4096`, `--bic <path>`.

## The bic binary

The package needs the `bic` program. On install it downloads the binary for
your platform from the BIC release with the same version number, checks it
against the release's `SHA256SUMS.txt`, and keeps it in `vendor/`. Platforms:
Windows x64, Linux x64, macOS arm64 and x64.

- `BIC_SKIP_DOWNLOAD=1` skips the download (offline installs).
- `BIC_BIN=/path/to/bic` uses your own binary.
- Without either, `bic` on the PATH is used.

The binary must be current: `bic inspect` is required, and releases before it
was added do not have it.

## Browser support

Service Workers and `Cache` are supported by current Chrome, Edge, Firefox and
Safari. The end-to-end test runs in Chrome only; see `test/browser.test.mjs`
(set `BIC_CHROME`). Firefox and Safari are not tested yet.
