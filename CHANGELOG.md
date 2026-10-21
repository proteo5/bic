# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/). While the version is 0.x, the file
format and the API may change between minor versions.

## [Unreleased]

### Changed

- The README is rewritten in English, with a Spanish version in `README.es.md`.
  It describes the three parts of the project (the command-line tool, the
  JavaScript runtime and the build step), the measured results and the limits.
- The documentation site (https://proteo5.github.io/bic/) has English and Spanish
  pages for the demo, the runtime API, the build plugins, the conversion policy,
  the command line, the format and troubleshooting. Its numbers are measured
  during each build, and its live examples decode with the published
  `@proteo5/bic-runtime`.
- The package READMEs link to the documentation site.

## [0.2.1] - 2026-10-21

### Added

- `bic inspect <image> [--json]`: sizes of PNG, oxipng, WebP lossless and BIC
  for one image, for build tools.
- `@proteo5/bic-build`, a build step and plugins that store flat graphics
  (charts, logos, icons) as BIC and serve them in the browser through a Service
  Worker, under their original URLs:
  - `bic-build <input> <output>` command;
  - a Vite plugin (`@proteo5/bic-build/vite`);
  - a webpack 5 plugin (`@proteo5/bic-build/webpack`).
  The conversion rule: at least 10 % smaller than the best lossless alternative,
  never JPEG, never more than 4096 colors. Thresholds can be changed.
- `@proteo5/bic-runtime`, the BIC decoder in JavaScript (5.7 KiB gzip with zstd).
- The package downloads the bic release binary for its platform on install and
  checks it against the release checksums.

## [0.2.0] - 2026-10-20

The BIC2 format replaces BIC1, and BIC1 files can no longer be read. Re-encode
them from the original images.

BIC targets images with large solid areas: screenshots, UI, diagrams,
charts, icons and pixel art. It offers little or no advantage on photographs.
JPEG sources are stored unchanged.

### Added

- **BIC2 coding**:
  - a palette for images with up to 256 colors;
  - a subtract-green color transform;
  - row prediction, either fixed (none or up) or chosen per row (none, left,
    up, average, Paeth, MED).
- **Exhaustive encoder**: it tries every transform and predictor combination
  in parallel and keeps the smallest. The default Zstandard level is now 19.
- **Near-lossless mode** (`--max-error N`) in the style of JPEG-LS: every
  decoded sample is within `N` of the original. Lossless candidates are tried
  too, so flat graphics stay exact.
- **Original-file storage**: a PNG, JPEG, WebP or HEIF source that is smaller
  than BIC's own encoding is stored unchanged, so a `.bic` file is never larger
  than its original plus 32 bytes. `--transcode` disables this.
- **ICC profiles** are stored in the container and written back to PNG
  output.
- **EXIF orientation** is applied when reading sources.
- **`--fast-decode`**: skips per-row prediction for faster opening, at the cost
  of slightly larger photographs.
- **`bic-view`**, a viewer without a console window:
  - File > Open (Ctrl+O) and File > Save as BIC (Ctrl+S), with saving in the
    background;
  - smooth resampling when the window is resized;
  - errors shown in a dialog;
  - used by the Windows file association.
- **`bic compare`**: benchmarks against PNG, oxipng, WebP lossless and QOI on
  files or folders, with CSV output and PSNR for near-lossless runs.
- **`bic verify --max-error`** for checking near-lossless files.
- **Library API**: `encode_file`, `encode_source`, `read_source`,
  `encode_rgba_with_metadata`, `decode_with_metadata`, `storage`, and
  `DecodeLimits` for untrusted input.
- **`cli` feature** (on by default) for the programs. With
  `default-features = false` the library depends only on `anyhow`, `image`,
  `heic-rs` and `zstd`.
- **Testing**: property-based tests and cargo-fuzz targets. CI runs on Linux
  and Windows, with fuzzing and a synthetic-corpus benchmark.

### Changed

- On the synthetic benchmark corpus (geometric mean against oxipng), flat graphics
  went from 1.316 to 0.755. Lossless photographs went from 1.849 to 0.837, but
  that comparison excludes strong photographic codecs.
- Lossless decoding processes whole rows with vectorizable loops.
- The decoder's default memory limit is 512 MiB, set with `--max-alloc-mib`.

### Removed

- `--quant-step`, `--smooth-tolerance` and `--residual`, replaced by
  `--max-error`.
- `is_png_fallback`, replaced by `storage`.

### Fixed

- Block partitioning only ever used 256-sample blocks.
- A malformed file with an inverted channel range could crash the decoder.
- Payloads that decompressed past their declared size were not rejected.
- Encoding an empty image with `--png-fallback` failed.
- Decoding to JPEG failed on RGBA images.

## [0.1.0] - 2026-10-20

### Added

- First prototype: the BIC1 planar format with run-length blocks and
  Zstandard, a CLI and a simple viewer.

[0.2.0]: https://github.com/proteo5/bic/compare/67ff50d...v0.2.0
[0.1.0]: https://github.com/proteo5/bic/commit/67ff50d
