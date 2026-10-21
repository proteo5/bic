// Decodes every example .bic with the WebAssembly build and checks the
// pixels against the source images (hash of RGBA bytes). Times the decode.
import { readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';

const wasmPath = new URL('../target/wasm32-unknown-unknown/release/bic_wasm_spike.wasm', import.meta.url);
const { instance } = await WebAssembly.instantiate(readFileSync(wasmPath), {});
const x = instance.exports;
const expected = JSON.parse(readFileSync(new URL('./expected.json', import.meta.url), 'utf8'));
const bicDir = new URL('../../../site/out/bic/', import.meta.url);

function decode(bytes, maxAllocMib = 512) {
  const ptr = x.bic_alloc(bytes.length);
  new Uint8Array(x.memory.buffer, ptr, bytes.length).set(bytes);
  const status = x.bic_decode(ptr, bytes.length, maxAllocMib);
  x.bic_free(ptr, bytes.length);
  if (status !== 0) return { status };
  const out = new Uint8Array(x.memory.buffer, x.bic_out_ptr(), x.bic_out_len()).slice();
  const info = { status, width: x.bic_width(), height: x.bic_height(), rgba: out };
  x.bic_release();
  return info;
}

for (const e of expected) {
  const bytes = readFileSync(new URL(`${e.id}.bic`, bicDir));
  const first = decode(bytes);
  if (first.status !== 0) {
    console.log(`${e.id.padEnd(20)} ${e.kind.padEnd(6)} not decoded by the WASM build (status ${first.status})`);
    continue;
  }
  const hash = createHash('sha256').update(first.rgba).digest('hex');
  const ok = hash === e.sha256 && first.width === e.width && first.height === e.height;
  const runs = 30;
  const t0 = performance.now();
  for (let i = 0; i < runs; i++) decode(bytes);
  const ms = (performance.now() - t0) / runs;
  console.log(`${e.id.padEnd(20)} ${e.kind.padEnd(6)} ${ok ? 'EXACT' : 'MISMATCH'}  ${ms.toFixed(2)} ms/decode  ${first.width}x${first.height}`);
}
