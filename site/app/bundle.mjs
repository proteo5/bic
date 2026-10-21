// Bundles the page's script with the published runtime into out/assets/app.js,
// and measures the decoder alone (minified and gzipped) for the page.
import { build } from 'esbuild';
import { gzipSync } from 'node:zlib';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const out = join(here, '..', 'out');
mkdirSync(join(out, 'assets'), { recursive: true });

await build({
  entryPoints: [join(here, 'main.js')],
  bundle: true,
  minify: true,
  format: 'iife',
  target: 'es2020',
  outfile: join(out, 'assets', 'app.js'),
  logLevel: 'warning',
});

// The decoder on its own: what a page that only decodes .bic files would load.
const decoderOnly = await build({
  stdin: {
    contents: "export { decodeBic } from '@proteo5/bic-runtime';",
    resolveDir: here,
    loader: 'js',
  },
  bundle: true,
  minify: true,
  format: 'esm',
  target: 'es2020',
  write: false,
  logLevel: 'warning',
});
const code = decoderOnly.outputFiles[0].contents;
writeFileSync(join(out, 'size.json'), JSON.stringify({
  decoderMinBytes: code.length,
  decoderGzipBytes: gzipSync(code, { level: 9 }).length,
}, null, 2));
console.log(`bundled out/assets/app.js; decoder alone: ${code.length} bytes minified`);
void readFileSync;
