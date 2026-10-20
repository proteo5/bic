# BIC2: experimental image container

This is a prototype format, not a stable interchange standard. All multi-byte
integers are little-endian. `BIC2` is the four-byte magic, and version 2 is
the only supported version. BIC1 files are not readable; re-encode them from
the original image.

## Container

The fixed 32-byte header is `magic[4]`, `version:u8`, `storage:u8`,
`flags:u16`, `width:u32`, `height:u32`, `plain_len:u64`, and `payload_len:u64`.
`flags` is reserved and must be 0; decoders reject any other value.

`storage = 0` stores a Zstandard-compressed native stream of `plain_len`
bytes. `storage = 1` stores a complete PNG and is only emitted by the optional
lossless PNG fallback. The fallback never replaces a lossy encode.

## Native stream

```text
transform:u8  predictor:u8  max_error:u8
[palette]                       only when transform = 2
channel...                      1 channel (palette) or 4 channels
```

### Transforms

* `0` direct: four channels coded in the order G, R, B, A.
* `1` subtract green: same order, but R and B are predicted relative to the
  decoded G (see below).
* `2` palette: `count_minus_one:u8`, then `count` RGBA entries (4 bytes each),
  then a single channel of indices. Indices must be below `count`. Palette
  streams must be lossless (`max_error = 0`).

### Channels

Each channel starts with a kind byte:

* `0` constant, followed by its value byte;
* `1` coded, followed by a plane of `width × height` symbols (see *Blocks and
  modes*).

### Prediction and quantization

Samples are visited in raster order. For sample `i` the decoder forms a
prediction `P` from samples it has already reconstructed:

* `base` is the decoded green sample at `i` for R and B under transform 1,
  and 0 otherwise;
* `D(j) = sample(j) − base(j)`;
* predictor `0` (none): `delta = 0`;
* predictor `1` (up): `delta = D(i − width)`. On the first row it is
  `D(i − 1)`, and for the first sample it is 0;
* `P = clamp(base(i) + delta, 0, 255)`.

Residuals are quantized as in near-lossless JPEG-LS. With `n = max_error`,
`step = 2n + 1` and `range = (255 + 2n) / step + 1`, the encoder computes
`e = x − P`, `q = (e + n) / step` for `e ≥ 0` or `−((n − e) / step)`
otherwise, and stores the symbol `q mod range`. Symbols must be below `range`.

The decoder reconstructs a sample from symbol `s`:

```text
q = s − range  if s ≥ (range + 1) / 2,  else s
r = P + q × step
r += range × step  if r < −n
r −= range × step  if r > 255 + n
sample = clamp(r, 0, 255)
```

Every decoded sample is within `max_error` of the original. With
`max_error = 0` the scheme is lossless modulo-256 residual coding.

### Encoder choices (non-normative)

The encoder tries two candidates in parallel and keeps the smaller file:
palette with no predictor or with up when the image has at most 256 colors
and is lossless, and otherwise direct with no predictor or subtract green
with up. These pairs were chosen from benchmark measurements; any valid
combination decodes.

## Blocks and modes

A plane of symbols is stored as `block_count:u32` followed by blocks. Each
block records `length:u16`, `mode:u8` and `payload_len:u32`, then the payload.
Blocks hold 1 to 4096 symbols. The encoder places boundaries on multiples of
256 symbols and picks, by dynamic programming, the combination of 256-, 1024-
and 4096-symbol blocks and modes with the fewest total bytes.

Modes are `0=RAW`, `1=RLE-4`, `2=RLE-8`, `3=RLE-12`, and `4=RLE-16`. An RLE
payload is `run_count:u16`, then `run_count` value bytes, then the run lengths
minus one, packed least-significant-bit first with 4, 8, 12 or 16 bits each.
Runs longer than the counter's capacity are split.

## Deliberate prototype limits

Only 8-bit RGBA input and output are supported. There is no animation, color
profile, or progressive decoding, and only two predictors are defined.

## Decoder limits

Before allocating, the decoder checks the declared size against a caller-set
`DecodeLimits` (defaults: 100,000,000 pixels and 512 MiB of working memory).
The native path needs about `6 × pixels + plain_len` bytes. Zstandard output
is capped at `plain_len`, so a payload that inflates further is rejected
rather than decompressed. PNG fallback payloads are decoded with the header's
dimensions as hard limits. The decoder is covered by property-based tests and
cargo-fuzz targets (see `fuzz/`).
