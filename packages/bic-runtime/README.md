# @proteo5/bic-runtime

Decodes BIC images in the browser or in Node.js. A JavaScript port of the BIC2
decoder, verified against the Rust decoder on every fixture. Zstandard is
decoded with `fzstd`. The minified bundle is about 12.5 KiB (5.7 KiB gzip).

```js
import { decodeBic } from '@proteo5/bic-runtime';

const decoded = decodeBic(new Uint8Array(await (await fetch('chart.bic')).arrayBuffer()));
if (decoded.kind === 'pixels') {
  // decoded.width, decoded.height, decoded.rgba (8-bit RGBA, row-major)
} else {
  // decoded.mime and decoded.bytes: a PNG or an original JPEG/WebP to show as is
}
```

Malformed input throws `BicError`. The format is described in `FORMAT.md` in
the BIC repository: https://github.com/proteo5/bic

Documentation, live examples and the runtime API reference:
https://proteo5.github.io/bic/docs/ (Español: https://proteo5.github.io/bic/es/docs/)
