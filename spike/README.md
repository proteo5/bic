# Browser spike: showing `.bic` files in `<img>`

This folder holds the measurements behind the plan for a front-end module that
serves `.bic` images in browsers. Nothing here is published or part of the
crate. The folders are:

- `wasm-decoder/`: the BIC decoder compiled to WebAssembly, with raw C-ABI exports.
- `sw-demo/`: a Service Worker that turns `.bic` requests into PNG responses, and two pages that use it.
- `sw-test/`: a headless Chrome test (playwright-core) that measures network bytes and checks pixels.
- `js-size/`: the size of a pure JavaScript zstd decoder (`fzstd`), for comparison.

## Results (Chrome, headless, 2026-10-21)

### WebAssembly decoder

| Item | Value |
| --- | ---: |
| `.wasm`, uncompressed | 312 KiB |
| `.wasm`, gzip (measured here, not served) | 123 KiB |
| Flat images decoded exactly (5 of 5) | yes, 0.5 to 11 ms each in Node (V8) |
| JPEG and HEIF sources | not in the WASM build; the browser decodes JPEG and the file is served as is |

zstd-sys (C) does not build for `wasm32-unknown-unknown`, so the browser build
uses `ruzstd` (pure Rust, decoding only). Encoding is native only.

### Pure JavaScript alternative

`fzstd`, a zstd decoder in JavaScript, is 6.6 KiB gzip unminified. A BIC decoder
in JavaScript would add a few KiB, so the runtime would be about 10 to 12 KiB,
against 123 KiB for WebAssembly. Not implemented yet.

### Where the Service Worker runs

Image bytes for three examples (bar chart, interface screenshot, photograph):

| Page | First visit | Second visit (SW active) |
| --- | ---: | ---: |
| `<img src="x.bic" onerror="…png">` | **1 474 KB** | 692 KB |
| Deferred source, chosen by a script at the end of `<body>` | **781 KB** | 692 KB |

- The `onerror` version downloads the whole `.bic` before it falls back to the PNG, so the first visit pays for both. The deferred version downloads nothing that it does not use.
- Second visit, pixels: the bar chart hashes equal to the source RGBA on both pages.
- Per image on the second visit: the bar chart goes from 122 KB (PNG) to 48 KB (BIC), the interface from 154 KB to 140 KB, and the photograph stays at 504 KB because the JPEG is served as is.
- The WebAssembly file is fetched once by the Service Worker and is not in these totals.

## Limits of this spike

- Tested only in Chromium. Firefox and Safari were not tested. Their handling of `onerror` on a failed decode, of Service Worker claims and of the first visit may differ.
- The built-in browser pane in the desktop app does not allow Service Worker registration, so the tests use headless Chrome.
- The byte counts come from the browser's resource timing and from the Service Worker's own count. They are not measured on a network.

## Reproduce

```bash
cd spike/wasm-decoder && cargo build --release --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/bic_wasm_spike.wasm ../sw-demo/bic.wasm
# copy the example .bic and PNG/JPEG files into sw-demo/assets (see sw-test/run.mjs)
cd ../sw-demo && python -m http.server 8124
cd ../sw-test && npm install && node run.mjs
```
