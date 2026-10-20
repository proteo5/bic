# Benchmarks

Baseline measurements for lossless BIC, recorded so that codec changes can be
judged against them. Every run also verifies that each BIC file decodes to the
exact original pixels.

## Reproducing

```bash
cargo run --release --example gen_corpus -- target/corpus
cargo run --release -- compare target/corpus --csv target/bench.csv
```

`compare` accepts any mix of image files and folders (searched recursively), so
you can point it at your own corpus too. CI runs the synthetic benchmark on
every push and attaches the CSV as a build artifact.

**Baselines:** PNG from the `image` crate's default encoder; oxipng preset 2;
WebP lossless from the `image` crate's pure-Rust encoder (weaker than libwebp
`cwebp -lossless`); QOI. All sizes are in KiB. In the summary, **BIC/total**
is BIC's total size over the baseline's total, and **BIC/geomean** is the
geometric mean of per-image ratios, so a single large image cannot dominate
it. Below 1.000 means BIC is smaller.

## Baseline — v0.1.0, BIC1 format, Zstd level 9

### Synthetic corpus (`gen_corpus`)

| Image | Size | PNG | oxipng | WebP | QOI | BIC | BIC/oxipng |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| chart | 800×600 | 11.4 | 1.3 | 0.8 | 14.8 | 2.4 | 1.882 |
| control_gradient | 512×512 | 222.8 | 20.6 | 131.3 | 209.9 | 17.5 | 0.850 |
| control_noise | 256×256 | 256.3 | 192.3 | 192.1 | 255.9 | 192.4 | 1.000 |
| diagram | 1024×768 | 57.2 | 6.0 | 15.2 | 32.9 | 6.7 | 1.107 |
| icon_alpha | 256×256 | 6.1 | 1.7 | 3.3 | 4.0 | 1.9 | 1.125 |
| pixel_art | 320×240 | 8.0 | 1.3 | 3.1 | 16.6 | 3.1 | 2.448 |
| ui_mock | 1280×800 | 194.9 | 17.5 | 53.9 | 85.1 | 24.5 | 1.398 |

| Baseline | Total KiB | BIC/total | BIC/geomean | BIC smaller |
| --- | ---: | ---: | ---: | ---: |
| PNG (image-rs) | 756.7 | 0.328 | 0.216 | 7/7 |
| oxipng preset 2 | 240.7 | 1.032 | 1.316 | 1/7 |
| WebP lossless (image-rs) | 399.7 | 0.622 | 0.647 | 4/7 |
| QOI | 619.2 | 0.401 | 0.245 | 7/7 |

The corpus generator was corrected after the first recording. Its random
source had short-period low bits that made the "noise" control partly
predictable. The numbers above use the corrected generator.

### Private photo set

Fourteen images, mostly photographs decoded from JPEG, plus two scanned
diagrams and a logo with transparency. The images are not published.

| Baseline | BIC/total | BIC/geomean | BIC smaller |
| --- | ---: | ---: | ---: |
| PNG (image-rs) | 1.253 | 1.108 | 3/14 |
| oxipng preset 2 | 1.801 | 1.849 | 0/14 |
| WebP lossless (image-rs) | 1.762 | 1.912 | 0/14 |
| QOI | 1.254 | 1.158 | 2/14 |

### Reading the baseline

- On flat graphics BIC beats PNG (image-rs) and QOI. It is about 3 % larger
  than oxipng by total size, but about 32 % larger by geometric mean.
- On photographs BIC is the largest format measured. That is expected
  without spatial prediction: RLE and Zstandard find almost no exact repeats
  in camera noise.
- These numbers are the reference for phase 2 (predictors and color
  decorrelation). A change is kept only if it improves them.
