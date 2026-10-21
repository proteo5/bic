# BIC

[![CI](https://github.com/proteo5/bic/actions/workflows/ci.yml/badge.svg)](https://github.com/proteo5/bic/actions/workflows/ci.yml)
[![npm: @proteo5/bic-runtime](https://img.shields.io/npm/v/@proteo5/bic-runtime?label=bic-runtime)](https://www.npmjs.com/package/@proteo5/bic-runtime)
[![npm: @proteo5/bic-build](https://img.shields.io/npm/v/@proteo5/bic-build?label=bic-build)](https://www.npmjs.com/package/@proteo5/bic-build)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

**BIC is a lossless image format for flat graphics.** Charts, logos, icons,
diagrams and pixel art are stored in fewer bytes than optimized PNG in many
cases. Photographs are kept as their original JPEG, so they neither gain nor lose.

- **Documentation, live examples and results:** <https://proteo5.github.io/bic/>
- **Español:** [README.es.md](README.es.md)

The project has three parts:

| Part | What it is | Where |
| --- | --- | --- |
| `bic` | Command-line tool and viewer for Windows, Linux and macOS | [Releases](https://github.com/proteo5/bic/releases) |
| `@proteo5/bic-runtime` | Decoder for browsers and Node.js (5.7 KiB gzip, one dependency) | [npm](https://www.npmjs.com/package/@proteo5/bic-runtime) |
| `@proteo5/bic-build` | Build step and plugins for Vite and webpack, plus the `bic-build` command | [npm](https://www.npmjs.com/package/@proteo5/bic-build) |

> **Status:** experimental. The format may change between versions. Every
> file is checked to decode to the exact source pixels before it is published.

## When to use BIC

| Content | BIC against optimized PNG |
| --- | --- |
| Bar charts and graphs with solid areas | **Smaller**: 46 % in the bar chart example |
| Icons and logos with few colors | **Equal or smaller** (icon: 0.2 % smaller) |
| Software screenshots with anti-aliased text | Larger (+19 %): thousands of colors |
| Flowcharts and pixel art | Larger (+12 % to +15 %) |
| Photographs | No gain: the JPEG is kept as it is (+32 bytes) |

Lossy formats such as WebP and AVIF are often much smaller than any lossless
format, but they change pixel values. Use them when exact pixels are not needed.
The method and the full tables are in [BENCHMARKS.md](BENCHMARKS.md) and on the
[documentation site](https://proteo5.github.io/bic/).

## Quick start

### Decode a .bic file in the browser or Node.js

```bash
npm install @proteo5/bic-runtime
```

```js
import { decodeBic, BicError } from '@proteo5/bic-runtime';

const bytes = new Uint8Array(await (await fetch('chart.bic')).arrayBuffer());
try {
  const image = decodeBic(bytes);
  if (image.kind === 'pixels') {
    // image.width, image.height, image.rgba: 8-bit RGBA, row by row
  } else {
    // image.mime and image.bytes: a PNG or an original JPEG/WebP, show as is
  }
} catch (error) {
  if (error instanceof BicError) console.error(error.message);
  else throw error;
}
```

### Use the build step in a Vite or webpack project

```bash
npm install --save-dev @proteo5/bic-build
```

```js
// vite.config.js
import { bicVite } from '@proteo5/bic-build/vite';

export default { plugins: [bicVite()] };
```

```js
// webpack.config.mjs (production builds; publicPath must be absolute)
import { BicWebpackPlugin } from '@proteo5/bic-build/webpack';

export default {
  mode: 'production',
  output: { publicPath: '/' },
  plugins: [new BicWebpackPlugin()],
};
```

The build converts an image only when the BIC file is at least 10 % smaller than
the best lossless format, and never a JPEG or an image with more than 4096
colors. Your HTML and CSS do not change: a Service Worker answers the original
image URLs with the converted image. Without the worker, the browser downloads
the PNG as usual. The reference is in the [documentation](https://proteo5.github.io/bic/docs/).

### Use the command line

Download the release for your system from the [releases page](https://github.com/proteo5/bic/releases),
or build it from source (see [Development](#development)).

```bash
bic encode logo.png logo.bic                   # lossless
bic verify logo.png logo.bic                   # the decoded pixels must match exactly
bic decode logo.bic logo-copy.png              # back to PNG
bic inspect logo.png --json                    # sizes of PNG, oxipng, WebP and BIC
bic encode photo.png photo.bic --max-error 2   # near-lossless: no sample differs by more than 2
npx bic-build src/images dist/bic              # convert a folder, with bic-manifest.json
```

Run `bic --help` for every option.

## Results

Measured on the example images of the [documentation site](https://proteo5.github.io/bic/).
All of them are public-domain or Creative Commons images from Wikimedia Commons.
Sizes in KiB. The baseline is oxipng, the smallest lossless format for every example.

| Example | Pixels | oxipng | BIC | BIC against oxipng |
| --- | ---: | ---: | ---: | ---: |
| Bar chart | 1600×800 | 87.7 | **47.3** | **46 % smaller** |
| Icon (transparent) | 512×512 | 7.3 | 7.3 | 0.2 % smaller |
| Software interface | 1600×859 | 114.5 | 136.3 | 19 % larger |
| Pixel art | 800×480 | 77.2 | 89.0 | 15 % larger |
| Flowchart | 706×905 | 10.0 | 11.1 | 12 % larger |
| Photograph (JPEG kept) | 1600×1064 | — | 492.5 | the JPEG plus 32 bytes |

On the synthetic benchmark corpus, the geometric mean of BIC against oxipng is
0.755 (lower is smaller). On a private set of photographs, lossless coding gives
0.837. Both sets and the method are in [BENCHMARKS.md](BENCHMARKS.md).

## How it works

- Images with at most 256 colors can be stored as a palette. Otherwise the planes can have green subtracted from red and blue. Each row can use its own predictor (none, left, up, average, Paeth or MED).
- The planes are coded as run-length blocks and compressed with Zstandard. The encoder tries a small set of these candidates and keeps the smallest result. The choice of candidates is explained in [BENCHMARKS.md](BENCHMARKS.md).
- Near-lossless mode (`--max-error N`) bounds the error of every sample.
- A file can embed the original PNG, JPEG, WebP or HEIF. A `.bic` is then never larger than its source plus 32 bytes.
- The decoders check sizes and lengths before they allocate memory. Malformed input returns an error.

The byte-level specification is in [FORMAT.md](FORMAT.md).

## Packages and versions

| Package | Registry | Latest version |
| --- | --- | --- |
| `bic` (codec library and CLI) | [crates.io](https://crates.io/crates/bic) | 0.2.0 on crates.io; 0.2.1 in this repository |
| `bic` binaries and `bic-view` viewer | [GitHub releases](https://github.com/proteo5/bic/releases) | 0.2.1 |
| `@proteo5/bic-runtime` | [npm](https://www.npmjs.com/package/@proteo5/bic-runtime) | 0.2.1 |
| `@proteo5/bic-build` | [npm](https://www.npmjs.com/package/@proteo5/bic-build) | 0.2.1 |

The build step downloads the `bic` binary for your platform when it is installed
(Windows x64, Linux x64, macOS Apple Silicon and Intel) and checks it against the
release checksums. Set `BIC_SKIP_DOWNLOAD=1` to skip the download, or `BIC_BIN`
to use your own binary.

## Command-line reference

| Command | What it does |
| --- | --- |
| `bic encode <input> <output.bic>` | Encode an image. Options: `--max-error N`, `--png-fallback`, `--fast-decode`, `--transcode`, `--zstd-level N` |
| `bic decode <input.bic> <output>` | Decode to an image file; the format follows the extension. PNG output keeps the ICC profile |
| `bic verify <original> <file.bic> [--max-error N]` | Check that the file decodes to the original pixels (or within `N`) |
| `bic inspect <image> [--json]` | Sizes of PNG, oxipng, WebP lossless and BIC for one image |
| `bic compare <paths>... [--csv out.csv]` | Benchmark BIC against the other lossless formats, on files or folders |
| `bic view <file>` | Open a `.bic` or any supported image in the viewer |
| `bic-view [file]` | The same viewer without a console window |
| `bic install-association` | Open `.bic` files with the viewer for the current Windows user |
| `bic-build <input> <output>` | Convert a folder with the build rule. Options: `--min-saving 10`, `--max-colors 4096`, `--bic <path>` |

## Development

Requirements: a recent stable Rust toolchain for the codec. The JavaScript packages need Node.js 20 or later; CI uses Node.js 22.

```bash
cargo test                                       # codec, CLI and property-based tests
cargo clippy --all-targets -- -D warnings
cargo build --release --bin bic                  # the command-line tool

cd packages/bic-runtime && npm ci && npm test    # decoder, checked against 53 fixtures written by the Rust codec
cd packages/bic-build && npm ci && npm test      # build step: conversion, verification, binary installer
```

The site is built with `python site/build.py` after `npm ci` in `site/`. It needs
Pillow and a release build of `bic`. The end-to-end browser test runs in Chrome
when `BIC_CHROME` points to it (see `packages/bic-build/test/browser.test.mjs`).

### Repository layout

| Path | Contents |
| --- | --- |
| `src/` | The codec library, the `bic` command-line tool and the `bic-view` viewer |
| `packages/bic-runtime/` | The JavaScript decoder (`@proteo5/bic-runtime`) and its conformance fixtures |
| `packages/bic-build/` | The build step and the Vite and webpack plugins (`@proteo5/bic-build`) |
| `site/` | The documentation and demo site (Python and esbuild) |
| `fuzz/` | cargo-fuzz targets for the decoder |
| `spike/` | The browser experiments behind the design: Service Worker and WebAssembly |
| `examples/` | `gen_corpus` (synthetic benchmark images) and `dump_rgba` (reference pixels for tests) |

### Fuzzing

The `fuzz/` crate has three [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz)
targets: `decode` (whole files), `decode_plain` (the plane stream inside a valid
container) and `roundtrip` (arbitrary images and options). CI fuzzes each one for
two minutes on every push.

```bash
rustup toolchain install nightly
cargo install cargo-fuzz
cd fuzz
cargo +nightly fuzz run decode_plain -- -max_total_time=300
```

On Windows (MSVC) the AddressSanitizer runtime must be on `PATH`, for example
`VC\Tools\MSVC\<version>\bin\Hostx64\x64` from your Visual Studio installation.

## Limits

- The codec works on 8-bit RGBA. Higher bit depths are reduced to 8 bits when they are read.
- ICC profiles are written to PNG output only. Other output formats drop them and print a note.
- Service Workers need HTTPS or `localhost`. Chrome is tested end to end; Firefox and Safari are not tested yet.
- Encoding is slow: the encoder tries several candidates, and a 20-megapixel photo takes about a minute. Decoding flat graphics takes a few milliseconds.

## Contributing

Issues and pull requests are welcome. For codec changes, include tests and, for
compression changes, before and after sizes on representative images. The
verification checklist that CI runs is in the workflow files under `.github/workflows/`.

## License

Licensed under the [Apache License, Version 2.0](LICENSE). Unless you state
otherwise, any contribution you submit is licensed under the same terms.
