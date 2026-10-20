# BIC1: experimental image container

This is a prototype format, not a stable interchange standard.  All multi-byte
integers are little-endian.  `BIC1` is the four-byte magic and version 1 is the
only supported version.

## Container

The fixed header is `magic[4]`, `version:u8`, `storage:u8`, `flags:u16`,
`width:u32`, `height:u32`, `plain_len:u64`, and `payload_len:u64`.
`flags` is reserved and must be 0; decoders reject any other value.

`storage = 0` stores a Zstandard-compressed native plane stream. `plain_len`
is checked while decoding. `storage = 1` stores a complete PNG payload and is
only emitted by the optional, lossless PNG fallback. The fallback makes the
container self-describing; it never silently changes an explicitly lossy
request.

## Native plane stream

The plain stream begins with four channel descriptors in RGBA order:

* `0` absent: decoder supplies 0 for RGB and 255 for alpha;
* `1` constant followed by its byte value;
* `2` variable followed by `min:u8`, `max:u8`, `quant_step:u8`,
  `has_residual:u8`, then its coded samples.

Variable samples are stored after *offset normalization*: `sample - min`.
This is reversible because the minimum is retained. It deliberately does not
stretch the range back to 0..255, because such a scale/round operation is
lossy. `max` is integrity metadata and validates the reconstructed plane.

The encoder can apply optional mean snapping within a block (`--smooth-
tolerance`) and uniform scalar quantization (`--quant-step`). With no residue,
these are lossy. With `--residual`, the signed `original - transformed` value
is retained as little-endian `i16` values, so the decoded image is exact.

## Blocks and modes

Each byte plane is partitioned into blocks selected from 256, 1024, and 4096
samples (the tail is shorter). Each block records `length:u16`, `mode:u8`,
`payload_len:u32`, then payload. The encoder places block boundaries on
multiples of 256 samples and picks, by dynamic programming, the combination of
block sizes and modes with the fewest total bytes for the plane. Modes are `0=RAW`, `1=RLE-4`, `2=RLE-8`, `3=RLE-12`, and
`4=RLE-16`. RLE payloads are `(value:u8, run_minus_one:bits)` pairs; counts
are packed least-significant-bit first, allowing real 4/8/12/16-bit counters.
An RLE payload starts with `run_count:u16`, then `run_count` values, followed
by the packed counters. Runs longer than the counter's capacity are split.

Residual `i16` data is represented as two byte planes (low then high), using
the same block codec. This keeps the format’s compression vocabulary small.

## Deliberate prototype limits

Only 8-bit RGBA input/output is supported. There are no chroma transforms,
predictors, palettes, animation, color profiles, or progressive decoding.

## Decoder limits

Before allocating, the decoder checks the declared size against a caller-set
`DecodeLimits` (defaults: 100,000,000 pixels and 1 GiB of working memory). The
native path needs about `7 × pixels + plain_len` bytes. Zstandard output is
capped at `plain_len`, so a payload that inflates further is rejected rather
than decompressed. PNG fallback payloads are decoded with the header's
dimensions as hard limits. The decoder has not been fuzzed.
Benchmarks must compare both size and encode/decode time against a defined PNG
configuration; no compression-superiority claim follows from this prototype.
