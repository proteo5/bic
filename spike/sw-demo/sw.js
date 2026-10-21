// Spike: a Service Worker that answers requests for .bic files with a PNG.
// The network carries the small .bic file; the PNG is produced in the browser.

const WASM_URL = new URL('bic.wasm', self.location.href).href;
let wasmReady = null;

function loadDecoder() {
  if (!wasmReady) {
    wasmReady = WebAssembly.instantiateStreaming(fetch(WASM_URL), {}).then((r) => r.instance.exports);
  }
  return wasmReady;
}

// Container header (FORMAT.md): magic, version, storage, flags, width, height,
// plain_len, payload_len. Flag bit 0 means an ICC profile follows the header.
function readContainer(bytes) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const storage = bytes[5];
  const flags = view.getUint16(6, true);
  let offset = 32;
  if (flags & 1) offset += 4 + view.getUint32(32, true);
  return { storage, payload: bytes.subarray(offset) };
}

// Image type of an embedded original, from its signature.
function sniffType(bytes) {
  if (bytes[0] === 0x89 && bytes[1] === 0x50) return 'image/png';
  if (bytes[0] === 0xff && bytes[1] === 0xd8) return 'image/jpeg';
  if (bytes[0] === 0x52 && bytes[8] === 0x57) return 'image/webp';
  return 'application/octet-stream';
}

async function decodeToPng(bytes) {
  const x = await loadDecoder();
  const ptr = x.bic_alloc(bytes.length);
  new Uint8Array(x.memory.buffer, ptr, bytes.length).set(bytes);
  const status = x.bic_decode(ptr, bytes.length, 512);
  x.bic_free(ptr, bytes.length);
  if (status !== 0) throw new Error('bic_decode status ' + status);
  const width = x.bic_width();
  const height = x.bic_height();
  const rgba = new Uint8ClampedArray(x.memory.buffer, x.bic_out_ptr(), x.bic_out_len()).slice();
  x.bic_release();
  const canvas = new OffscreenCanvas(width, height);
  canvas.getContext('2d').putImageData(new ImageData(rgba, width, height), 0, 0);
  return canvas.convertToBlob({ type: 'image/png' });
}

async function report(event, info) {
  const clients = await self.clients.matchAll({ type: 'window' });
  for (const client of clients) client.postMessage({ source: 'sw', ...info });
}

async function serveBic(request, event) {
  const response = await fetch(request);
  const bytes = new Uint8Array(await response.arrayBuffer());
  const { storage, payload } = readContainer(bytes);
  if (storage === 2) {
    // Embedded original (JPEG, PNG, WebP): the browser decodes it, no WASM needed.
    await report(event, { url: request.url, network: bytes.length, served: payload.length, mode: 'original' });
    return new Response(payload, { headers: { 'Content-Type': sniffType(payload) } });
  }
  const png = await decodeToPng(bytes);
  await report(event, { url: request.url, network: bytes.length, served: png.size, mode: 'decoded' });
  return new Response(png, { headers: { 'Content-Type': 'image/png' } });
}

self.addEventListener('install', () => self.skipWaiting());
self.addEventListener('activate', (event) => event.waitUntil(self.clients.claim()));

self.addEventListener('fetch', (event) => {
  const url = new URL(event.request.url);
  if (url.origin !== self.location.origin || !url.pathname.endsWith('.bic')) return;
  event.respondWith(
    serveBic(event.request, event).catch((error) => {
      // Let the page's onerror handler fall back to the PNG.
      report(event, { url: event.request.url, error: String(error) });
      return Response.error();
    })
  );
});
