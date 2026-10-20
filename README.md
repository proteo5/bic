# BIC — experimental planar image codec

[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
![Status: experimental](https://img.shields.io/badge/status-experimental-orange.svg)
![Rust 2021](https://img.shields.io/badge/rust-2021-informational.svg)

**BIC** is a Rust prototype of a lossless and near-lossless RGBA image codec.
It combines palettes, simple prediction and run-length coding with Zstandard,
and on the project's benchmarks it produces smaller files than oxipng for both
flat graphics and photographs (see [BENCHMARKS.md](BENCHMARKS.md)). It ships
as a library and a command-line tool with a small native viewer.

> **Status:** research prototype. The `BIC2` format is not a stable interchange
> standard and may change between versions. Do not use it for archival storage.

## Contents

- [Features](#features)
- [Installation](#installation)
- [Command-line usage](#command-line-usage)
- [Library usage](#library-usage)
- [How it works](#how-it-works)
- [Safety and decoder limits](#safety-and-decoder-limits)
- [Benchmarking](#benchmarking)
- [Limitations](#limitations)
- [Roadmap](#roadmap)
- [Development](#development)
- [License](#license)

## Features

- **Lossless by default**: decoding returns exactly the original pixels.
- **Near-lossless mode**: `--max-error N` guarantees every decoded sample is
  within `N` of the original (JPEG-LS-style quantization).
- **Per-image choice of encoding**: a palette for images with up to 256
  colors, an optional subtract-green color transform, and prediction that is
  either fixed (none or up) or chosen per row (none, left, up, average, Paeth,
  MED). The encoder tries every valid combination and keeps the smallest.
- **Size first, fast decoding second**: encoding is deliberately exhaustive
  and slow. Lossless decoding uses vectorizable row loops.
- **Planar coding**: channels are coded separately; constant channels cost two
  bytes.
- **Adaptive block coding**: each plane is split into blocks of 256, 1024 or
  4096 samples. Each block is stored RAW or with run-length encoding using 4-,
  8-, 12- or 16-bit counters. The split with the fewest total bytes is chosen
  by dynamic programming.
- **Zstandard wrapping** of the whole plane stream, with a configurable level.
- **PNG fallback**: optionally stores a PNG inside the container whenever it is
  smaller, so a lossless encode is never larger than the equivalent PNG.
- **Wide input support**: PNG, JPEG, WebP, BMP, GIF, TIFF, HEIC and HEIF.
  EXIF orientation is applied, so phone photos come out upright.
- **ICC color profiles** are kept: read from the source image, stored in the
  `.bic` file and written back when decoding to PNG.
- **Bounded decoding**: memory and pixel limits are checked before anything is
  allocated.
- **Desktop viewer and converter**: `bic-view` opens `.bic` files and every
  supported input format. **File > Open** (Ctrl+O) opens another image, and
  **File > Save as BIC** (Ctrl+S) converts the current one to a lossless `.bic`
  in the background, keeping its ICC profile. It runs without a console
  window, opens on double-click through a Windows file association, shows
  transparency over a checkerboard, keeps proportions when resized, and
  reports errors in a dialog.

## Installation

You need a recent stable [Rust toolchain](https://rustup.rs/).

```bash
cargo install --git https://github.com/proteo5/bic
```

Or build from a clone:

```bash
git clone https://github.com/proteo5/bic
cd bic
cargo build --release
```

This builds two programs in `target/release`: `bic`, the command-line tool,
and `bic-view`, the viewer (`.exe` on Windows).

## Command-line usage

| Command | Description |
| --- | --- |
| `bic encode <input> <output.bic>` | Encode an image to BIC |
| `bic decode <input.bic> <output>` | Decode to a format chosen by the output extension (PNG keeps the ICC profile) |
| `bic view <file>` | Open a `.bic` or any supported image in the viewer (Esc closes it) |
| `bic-view [file]` | The same viewer without a console window; without a file it asks for one |
| `bic verify <original> <file.bic> [--max-error N]` | Check a BIC file against its original: exact, or within `N` |
| `bic compare <paths>... [--max-error N] [--fast-decode] [--csv out.csv]` | Benchmark BIC against PNG, oxipng, WebP and QOI on files or folders; reports PSNR when `N > 0` |
| `bic install-association` | Open `.bic` files with `bic-view` for the current Windows user |

### Encode options

| Option | Default | Effect |
| --- | --- | --- |
| `--max-error <N>` | `0` | Largest allowed difference per sample. `0` is lossless. The encoder also tries lossless coding and keeps it when it is smaller. |
| `--png-fallback` | off | Store a PNG instead when it is smaller (lossless encodes only). |
| `--fast-decode` | off | Skip per-row prediction. On photographs: about 1 % larger, about 35 % faster to decode. No effect on flat graphics. |
| `--zstd-level <N>` | `19` | Zstandard compression level (1–22). Higher levels shrink files; decoding speed barely changes. |

### Examples

```bash
# Lossless encode and exact check
bic encode screenshot.png screenshot.bic
bic verify screenshot.png screenshot.bic

# Near-lossless: no sample differs by more than 2
bic encode photo.jpg photo.bic --max-error 2
bic verify photo.jpg photo.bic --max-error 2

# Faster to open, slightly larger
bic encode photo.jpg photo.bic --fast-decode

# Never larger than PNG
bic encode screenshot.png screenshot.bic --png-fallback

# Decode back to PNG
bic decode screenshot.bic restored.png
```

### Windows file association

After building, keep `bic` and `bic-view` in the same folder and run once:

```bash
bic install-association
```

This writes per-user keys under `HKCU\Software\Classes`, so double-clicking a
`.bic` file opens it in `bic-view`. No administrator rights are needed, and
nothing changes until you run the command. If `bic-view` is missing, the
association falls back to `bic view`, which also opens a console window.

## Library usage

Add the crate from Git:

```toml
[dependencies]
bic = { git = "https://github.com/proteo5/bic", default-features = false }
```

`default-features = false` leaves out the `cli` feature (command-line tool,
viewer, file dialogs and extra input formats). The codec then depends only on
`anyhow`, `image` (PNG) and `zstd`.

```rust
use bic::{decode_rgba, decode_rgba_with_limits, encode_rgba, DecodeLimits, EncodeOptions};

fn roundtrip(width: u32, height: u32, rgba: &[u8]) -> anyhow::Result<()> {
    let file = encode_rgba(width, height, rgba, EncodeOptions::default())?;

    // Default limits: 100 M pixels, 512 MiB working memory.
    let (w, h, pixels) = decode_rgba(&file)?;
    assert_eq!((w, h, pixels.as_slice()), (width, height, rgba));

    // Stricter limits for untrusted input.
    let limits = DecodeLimits { max_pixels: 16_000_000, max_alloc: 256 << 20 };
    decode_rgba_with_limits(&file, limits)?;
    Ok(())
}
```

Input and output are 8-bit RGBA in row-major order (`width × height × 4`
bytes). Set `EncodeOptions::max_error` for near-lossless encoding. To store
or read an ICC profile, use `encode_rgba_with_metadata` and
`decode_with_metadata`.

## How it works

```text
RGBA image
  ├─ ≤ 256 colors (lossless)  →  palette + one index plane
  └─ otherwise                →  G, R, B, A planes (R and B relative to G)
       └─ per plane: constant → 2 bytes, or
            ├─ prediction (none, up, or a rule per row)
            ├─ residual quantization (exact when max_error = 0)
            └─ adaptive blocks: RAW | RLE-4 | RLE-8 | RLE-12 | RLE-16
  └─ Zstandard
  └─ BIC2 container (32-byte header)
```

The encoder builds every candidate encoding in parallel and keeps the smallest.

The full wire format is specified in [FORMAT.md](FORMAT.md).

## Safety and decoder limits

Decoding untrusted files is guarded by `DecodeLimits`:

- Declared dimensions are checked against `max_pixels` before allocation.
- The decoder's peak memory (about `6 × pixels` plus the decompressed stream)
  must fit within `max_alloc`.
- Zstandard output is capped at the length declared in the header, so payloads
  that inflate beyond it are rejected.
- Embedded PNG payloads are decoded with the header dimensions as hard limits.
- Malformed streams, such as out-of-range palette indices or quantizer
  symbols, return errors instead of panicking.

The decoder is fuzzed in CI (see [Fuzzing](#fuzzing)), but it is still prototype
code. Do not treat it as a security boundary.

## Benchmarking

```bash
# Deterministic synthetic corpus of flat graphics plus two controls
cargo run --release --example gen_corpus -- target/corpus

# Benchmark any mix of files and folders
bic compare target/corpus ~/Pictures/screenshots --csv results.csv
```

`compare` encodes each image losslessly and verifies the exact roundtrip. It
then prints a per-image table and a summary against PNG (image-rs), oxipng,
WebP lossless and QOI, using both total-size and geometric-mean ratios. If any
image fails, the command exits with an error.

Results are recorded in [BENCHMARKS.md](BENCHMARKS.md). With BIC2 at the
default settings, files are on average (geometric mean) about 24 % smaller than
oxipng on the synthetic flat-graphics corpus and about 16 % smaller on a set of
photographs. CI reruns the synthetic benchmark on every push.

## Limitations

- 8-bit RGBA only. 16-bit images are reduced to 8 bits on load.
- One color transform (subtract green).
- ICC profiles are stored but not applied: the viewer shows raw RGB values.
- The viewer window keeps its first size; images of another shape opened
  later are letterboxed.
- Decoding to formats other than PNG drops the ICC profile, and JPEG output
  drops transparency.
- Other EXIF metadata (camera, GPS, dates) is not kept.
- No animation or progressive decoding.
- Encoding is slow by design (about a minute for a 20-megapixel photo).
- Rows that use the left, average, Paeth or MED rules decode sequentially,
  about half the speed of `none` and `up` rows. `--fast-decode` avoids them.
- Near-lossless decoding uses a per-sample loop and is slower than lossless.
- The format version may change without a migration path.

## Roadmap

- [x] Spatial prediction and color decorrelation, chosen by measurement (BIC2)
- [x] Palette mode for images with up to 256 colors
- [x] Near-lossless mode with a guaranteed per-sample error bound
- [x] Keep or drop the RLE stage: measured, kept (plain Zstandard was larger)
- [x] Faster lossless decoding (vectorizable row loops)
- [x] Per-row predictor choice (left, average, Paeth, MED)
- [x] Image corpus and regression benchmarks against PNG, oxipng, WebP lossless and QOI
- [x] Decoder fuzzing (`cargo-fuzz`) and property-based roundtrip tests
- [x] Viewer without a console window, with frame-rate limiting
- [x] EXIF orientation and ICC profile handling
- [x] Open any supported image in the viewer and save it as BIC
- [ ] Color-managed display in the viewer

## Development

```bash
cargo test                          # unit and property-based tests
PROPTEST_CASES=5000 cargo test      # longer property-based run
cargo clippy --all-targets -- -D warnings
cargo fmt
```

### Fuzzing

The `fuzz/` crate has three [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz)
targets:

| Target | Input |
| --- | --- |
| `decode` | Arbitrary bytes as a whole BIC file |
| `decode_plain` | Arbitrary bytes as the plane stream, inside a valid container |
| `roundtrip` | Arbitrary images and encoder options; checks exactness or the lossy error bound |

```bash
rustup toolchain install nightly
cargo install cargo-fuzz
cd fuzz
cargo +nightly fuzz run decode_plain -- -max_total_time=300
```

On Windows (MSVC), the AddressSanitizer runtime must be on `PATH`, for example
`VC\Tools\MSVC\<version>\bin\Hostx64\x64` from your Visual Studio
installation. CI fuzzes each target for two minutes on every push.

Issues and pull requests are welcome. Please include tests for codec changes
and, for compression changes, before/after sizes on representative images.

## License

Licensed under the [Apache License, Version 2.0](LICENSE).

Unless you explicitly state otherwise, any contribution you intentionally
submit for inclusion in this work shall be licensed under the Apache License,
Version 2.0, without any additional terms or conditions.
