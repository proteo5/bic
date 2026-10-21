// Service Worker for the spike, built with esbuild (see the README).
// Uses the JavaScript runtime from packages/bic-runtime.
import { decodeBic } from '../../packages/bic-runtime/src/index.js';

async function report(info) {
  const clients = await self.clients.matchAll({ type: 'window' });
  for (const client of clients) client.postMessage({ source: 'sw', ...info });
}

async function serveBic(request) {
  const response = await fetch(request);
  const bytes = new Uint8Array(await response.arrayBuffer());
  const decoded = decodeBic(bytes);
  if (decoded.kind === 'file') {
    // A PNG or an original JPEG/WebP is served as is; the browser decodes it.
    await report({ url: request.url, network: bytes.length, served: decoded.bytes.length, mode: 'original' });
    return new Response(decoded.bytes, { headers: { 'Content-Type': decoded.mime } });
  }
  const canvas = new OffscreenCanvas(decoded.width, decoded.height);
  const imageData = new ImageData(new Uint8ClampedArray(decoded.rgba.buffer), decoded.width, decoded.height);
  canvas.getContext('2d').putImageData(imageData, 0, 0);
  const png = await canvas.convertToBlob({ type: 'image/png' });
  await report({ url: request.url, network: bytes.length, served: png.size, mode: 'decoded' });
  return new Response(png, { headers: { 'Content-Type': 'image/png' } });
}

self.addEventListener('install', () => self.skipWaiting());
self.addEventListener('activate', (event) => event.waitUntil(self.clients.claim()));

self.addEventListener('fetch', (event) => {
  const url = new URL(event.request.url);
  if (url.origin !== self.location.origin || !url.pathname.endsWith('.bic')) return;
  event.respondWith(
    serveBic(event.request).catch((error) => {
      report({ url: event.request.url, error: String(error) });
      return Response.error();
    })
  );
});
