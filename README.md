# BIC — experimental planar image codec

[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
![Status: experimental](https://img.shields.io/badge/status-experimental-orange.svg)
![Rust 2021](https://img.shields.io/badge/rust-2021-informational.svg)

**BIC** is a Rust prototype of a planar RGBA image codec designed for graphics
with large solid regions: screenshots, UI captures, diagrams, icons and flat
illustrations. It ships as a library and a command-line tool with a small
native viewer.

> **Status:** research prototype. The `BIC1` format is not a stable interchange
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
- **Planar coding**: each RGBA channel is coded independently. Channels that
  are absent or constant cost one or two bytes.
- **Adaptive block coding**: each plane is split into blocks of 256, 1024 or
  4096 samples. Each block is stored RAW or with run-length encoding using 4-,
  8-, 12- or 16-bit counters. The split with the fewest total bytes is chosen
  by dynamic programming.
- **Zstandard wrapping** of the whole plane stream, with a configurable level.
- **Optional lossy mode**: uniform quantization and block mean snapping, plus
  an optional residual that makes a lossy encode exact again.
- **PNG fallback**: optionally stores a PNG inside the container whenever it is
  smaller, so a lossless encode is never larger than the equivalent PNG.
- **Wide input support**: PNG, JPEG, WebP, BMP, GIF, TIFF, HEIC and HEIF.
- **Bounded decoding**: memory and pixel limits are checked before anything is
  allocated.
- **Desktop viewer** and a Windows file association for `.bic` files.

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

The binary is written to `target/release/bic` (`bic.exe` on Windows).

## Command-line usage

| Command | Description |
| --- | --- |
| `bic encode <input> <output.bic>` | Encode an image to BIC |
| `bic decode <input.bic> <output>` | Decode to any format inferred from the output extension |
| `bic view <file>` | Open a `.bic` or any supported image in the viewer (Esc closes it) |
| `bic verify <original> <file.bic>` | Check that the BIC decodes to exactly the original pixels |
| `bic compare <paths>... [--csv out.csv]` | Benchmark lossless BIC against PNG, oxipng, WebP and QOI on files or folders |
| `bic install-association` | Associate `.bic` with the viewer for the current Windows user |

### Encode options

| Option | Default | Effect |
| --- | --- | --- |
| `--quant-step <N>` | `1` | Uniform quantization step. Values above 1 are lossy. |
| `--smooth-tolerance <N>` | `0` | Snap samples within `N` of the block mean to the mean. Lossy. |
| `--residual` | off | Store the difference so a lossy transform decodes exactly. |
| `--png-fallback` | off | Store a PNG instead when it is smaller (lossless encodes only). |
| `--zstd-level <N>` | `9` | Zstandard compression level. |

### Examples

```bash
# Lossless encode and exact check
bic encode screenshot.png screenshot.bic
bic verify screenshot.png screenshot.bic

# Lossy encode
bic encode diagram.png diagram.bic --quant-step 8 --smooth-tolerance 5

# Never larger than PNG
bic encode photo.jpg photo.bic --png-fallback

# Decode back to PNG
bic decode screenshot.bic restored.png
```

### Windows file association

After building a release executable, run once:

```bash
bic install-association
```

This writes per-user keys under `HKCU\Software\Classes`, so double-clicking a
`.bic` file opens it in the viewer. No administrator rights are needed, and
nothing changes until you run the command.

## Library usage

Add the crate from Git:

```toml
[dependencies]
bic = { git = "https://github.com/proteo5/bic" }
```

```rust
use bic::{decode_rgba, decode_rgba_with_limits, encode_rgba, DecodeLimits, EncodeOptions};

fn roundtrip(width: u32, height: u32, rgba: &[u8]) -> anyhow::Result<()> {
    let file = encode_rgba(width, height, rgba, EncodeOptions::default())?;

    // Default limits: 100 M pixels, 1 GiB working memory.
    let (w, h, pixels) = decode_rgba(&file)?;
    assert_eq!((w, h, pixels.as_slice()), (width, height, rgba));

    // Stricter limits for untrusted input.
    let limits = DecodeLimits { max_pixels: 16_000_000, max_alloc: 256 << 20 };
    decode_rgba_with_limits(&file, limits)?;
    Ok(())
}
```

Input and output are 8-bit RGBA in row-major order (`width × height × 4`
bytes).

## How it works

```text
RGBA image
  └─ split into 4 planes (R, G, B, A)
       ├─ absent / constant plane  →  1–2 byte descriptor
       └─ variable plane
            ├─ subtract channel minimum   (reversible)
            ├─ optional lossy transform   (+ optional residual planes)
            └─ adaptive blocks: RAW | RLE-4 | RLE-8 | RLE-12 | RLE-16
  └─ Zstandard
  └─ BIC1 container (32-byte header)
```

The full wire format is specified in [FORMAT.md](FORMAT.md).

## Safety and decoder limits

Decoding untrusted files is guarded by `DecodeLimits`:

- Declared dimensions are checked against `max_pixels` before allocation.
- The decoder's peak memory (about `7 × pixels` plus the decompressed stream)
  must fit within `max_alloc`.
- Zstandard output is capped at the length declared in the header, so payloads
  that inflate beyond it are rejected.
- Embedded PNG payloads are decoded with the header dimensions as hard limits.
- Known malformed inputs, such as an inverted channel range, return errors
  instead of panicking.

The decoder has not been fuzzed yet. Treat it as hardened prototype code, not
as a security boundary.

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

The current baseline is in [BENCHMARKS.md](BENCHMARKS.md). In short, BIC beats
PNG and QOI on flat graphics, is close to oxipng by total size, and is not
competitive on photographs yet. CI reruns the synthetic benchmark on every push.

## Limitations

- 8-bit RGBA only. 16-bit images are reduced to 8 bits on load.
- No spatial predictors or color decorrelation yet.
- ICC color profiles and EXIF orientation are not preserved.
- No animation, palettes, or progressive decoding.
- The format version may change without a migration path.

## Roadmap

- [ ] Spatial prediction (left, up, Paeth, MED) before entropy coding
- [ ] Reversible color decorrelation (e.g. YCoCg-R)
- [ ] Evaluate the RLE stage against plain Zstandard on predicted planes
- [ ] Zigzag-coded `u8` residuals; store quantization indices
- [x] Image corpus and regression benchmarks against PNG, oxipng, WebP lossless and QOI
- [ ] Decoder fuzzing (`cargo-fuzz`) and property-based roundtrip tests
- [ ] Viewer without a console window, with frame-rate limiting
- [ ] EXIF orientation and ICC profile handling

## Development

```bash
cargo test                    # unit tests
cargo clippy --all-targets    # lints
cargo fmt                     # formatting
```

Issues and pull requests are welcome. Please include tests for codec changes
and, for compression changes, before/after sizes on representative images.

## License

Licensed under the [Apache License, Version 2.0](LICENSE).

Unless you explicitly state otherwise, any contribution you intentionally
submit for inclusion in this work shall be licensed under the Apache License,
Version 2.0, without any additional terms or conditions.
