# @proteo5/bic-build

A build step that stores flat graphics (charts, logos, icons) as BIC when that
saves bandwidth, and leaves everything else alone.

```bash
bic-build src/images dist/bic
```

For each image it runs `bic inspect` and keeps the BIC file only if all of these hold:

- the source is not already a lossy format (JPEG files are never converted);
- the image has at most 4096 distinct colors (more colors means a photograph-like image);
- the BIC file is at least 10 % smaller than the best lossless alternative: PNG, oxipng or WebP lossless.

The output folder gets the converted `.bic` files and `bic-manifest.json`, with
the decision and its reason for every image. Thresholds can be changed with
`--min-saving` (percent) and `--max-colors`, or in code with `optimizeDirectory`.

The `bic` binary is found through `--bic <path>`, the `BIC_BIN` environment
variable, or the `PATH`. Build it with `cargo build --release` in the BIC repository.

Browsers cannot display `.bic` files yet. The runtime in `packages/bic-runtime`
decodes them, and integrating it into pages is the next step of this package.
