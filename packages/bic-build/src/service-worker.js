// The Service Worker that serves BIC images under their original URLs.
//
// A request for /assets/logo.png is answered with the decoded image when the
// build converted that file to /assets/logo.bic (listed in ROUTES). The page
// never changes: its <img> and CSS still name the PNG. Without a Service
// Worker the browser downloads the PNG as usual, so no visitor pays twice.
//
// ROUTES is injected at build time by the Vite plugin (esbuild `define`).

import { decodeBic } from '@proteo5/bic-runtime';

const ROUTES = __BIC_ROUTES__;
const CACHE = 'bic-decoded-v1';

self.addEventListener('install', () => self.skipWaiting());

self.addEventListener('activate', (event) => {
  event.waitUntil(
    (async () => {
      // Drop decoded images from earlier builds: their keys are no longer routes.
      const cache = await caches.open(CACHE);
      for (const request of await cache.keys()) {
        if (!Object.prototype.hasOwnProperty.call(ROUTES, new URL(request.url).pathname)) {
          await cache.delete(request);
        }
      }
      await self.clients.claim();
    })()
  );
});

self.addEventListener('fetch', (event) => {
  const url = new URL(event.request.url);
  if (url.origin !== self.location.origin) return;
  if (event.request.method !== 'GET') return;
  if (!Object.prototype.hasOwnProperty.call(ROUTES, url.pathname)) return;
  event.respondWith(serve(event.request, url.pathname, ROUTES[url.pathname]));
});

async function serve(request, key, bicPath) {
  try {
    const cache = await caches.open(CACHE);
    const cached = await cache.match(key);
    if (cached) return cached;

    const network = await fetch(bicPath);
    if (!network.ok) throw new Error(`${bicPath} returned ${network.status}`);
    const decoded = decodeBic(new Uint8Array(await network.arrayBuffer()));
    const response = await toResponse(decoded);
    await cache.put(key, response.clone());
    return response;
  } catch (error) {
    // Anything unexpected: show the original image instead of a broken one.
    return fetch(request);
  }
}

async function toResponse(decoded) {
  if (decoded.kind === 'file') {
    return new Response(decoded.bytes, { headers: { 'Content-Type': decoded.mime } });
  }
  const canvas = new OffscreenCanvas(decoded.width, decoded.height);
  const pixels = new ImageData(new Uint8ClampedArray(decoded.rgba.buffer), decoded.width, decoded.height);
  canvas.getContext('2d').putImageData(pixels, 0, 0);
  const png = await canvas.convertToBlob({ type: 'image/png' });
  return new Response(png, { headers: { 'Content-Type': 'image/png' } });
}
