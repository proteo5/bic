// Decodes the demo page's .bic files with the JavaScript runtime, checks the
// pixels against the source images, and reports the time per decode.
import { readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { decodeBic } from '../src/index.js';

const root = new URL('../../../', import.meta.url);
const expected = JSON.parse(readFileSync(new URL('spike/wasm-decoder/test/expected.json', root), 'utf8'));

for (const e of expected) {
  const bytes = readFileSync(new URL(`site/out/bic/${e.id}.bic`, root));
  const first = decodeBic(bytes);
  if (first.kind !== 'pixels') {
    console.log(`${e.id.padEnd(20)} stored as ${first.mime} (served as is, ${first.bytes.length} bytes)`);
    continue;
  }
  const hash = createHash('sha256').update(first.rgba).digest('hex');
  const runs = 20;
  const t0 = performance.now();
  for (let i = 0; i < runs; i++) decodeBic(bytes);
  const ms = (performance.now() - t0) / runs;
  console.log(`${e.id.padEnd(20)} ${hash === e.sha256 ? 'EXACT' : 'MISMATCH'}  ${ms.toFixed(2)} ms/decode  ${first.width}x${first.height}`);
}
void fileURLToPath;
