// Demo page script. It decodes the page's .bic files with the published
// @proteo5/bic-runtime, in the browser, and checks every result against the
// source pixels. Nothing here runs on a server.

import { decodeBic, BicError } from '@proteo5/bic-runtime';

const data = JSON.parse(document.getElementById('bic-data').textContent);
const ui = data.ui;

/** Fills {name} placeholders of a translated string. */
function fill(template, values) {
  return template.replace(/\{(\w+)\}/g, (_, key) => values[key]);
}

// ---- Theme ----------------------------------------------------------------

const THEME_KEY = 'bic-theme';

function systemTheme() {
  return matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

function currentTheme() {
  return document.documentElement.dataset.theme || systemTheme();
}

function setTheme(mode) {
  document.documentElement.dataset.theme = mode;
  try {
    localStorage.setItem(THEME_KEY, mode);
  } catch {
    // Private browsing: the choice applies to this page only.
  }
  const button = document.getElementById('theme-toggle');
  if (button) {
    const next = mode === 'dark' ? ui.themeLight : ui.themeDark;
    button.setAttribute('aria-label', next);
    button.title = next;
  }
}

function initTheme() {
  let saved = null;
  try {
    saved = localStorage.getItem(THEME_KEY);
  } catch {
    // No storage: use the system theme.
  }
  setTheme(saved === 'dark' || saved === 'light' ? saved : systemTheme());
  document.getElementById('theme-toggle')?.addEventListener('click', () => {
    setTheme(currentTheme() === 'dark' ? 'light' : 'dark');
  });
}

// ---- Decoding -------------------------------------------------------------

async function sha256Hex(bytes) {
  const digest = await crypto.subtle.digest('SHA-256', bytes);
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('');
}

function drawPixels(width, height, rgba) {
  const canvas = document.createElement('canvas');
  canvas.width = width;
  canvas.height = height;
  const pixels = new ImageData(new Uint8ClampedArray(rgba.buffer, rgba.byteOffset, rgba.byteLength), width, height);
  canvas.getContext('2d').putImageData(pixels, 0, 0);
  return canvas;
}

/** Decodes a .bic file and returns what to show, how long it took, and a checksum. */
async function decodeFile(bytes) {
  const start = performance.now();
  const decoded = decodeBic(bytes);
  const ms = performance.now() - start;
  if (decoded.kind === 'pixels') {
    return {
      ms,
      view: drawPixels(decoded.width, decoded.height, decoded.rgba),
      hash: await sha256Hex(decoded.rgba),
      stored: null,
      width: decoded.width,
      height: decoded.height,
      rgba: decoded.rgba,
    };
  }
  const url = URL.createObjectURL(new Blob([decoded.bytes], { type: decoded.mime }));
  const img = new Image(decoded.width, decoded.height);
  img.src = url;
  img.alt = '';
  return {
    ms,
    view: img,
    hash: await sha256Hex(decoded.bytes),
    stored: decoded.mime,
    width: decoded.width,
    height: decoded.height,
  };
}

// ---- Examples -------------------------------------------------------------

function setStatus(element, text, state) {
  element.textContent = text;
  element.dataset.state = state;
}

async function showExample(card, example) {
  const status = card.querySelector('.status');
  try {
    const response = await fetch(example.bic);
    if (!response.ok) throw new Error(`${response.status}`);
    const result = await decodeFile(new Uint8Array(await response.arrayBuffer()));
    card.querySelector('.view').replaceChildren(result.view);
    const identical = result.hash === example.sha256;
    const how = result.stored
      ? fill(ui.storedAs, { mime: result.stored })
      : fill(ui.decodedIn, { ms: result.ms.toFixed(1) });
    setStatus(status, `${how} · ${identical ? ui.identical : ui.mismatch}`, identical ? 'ok' : 'bad');
  } catch (error) {
    setStatus(status, `${ui.failed}: ${error.message}`, 'bad');
  }
}

function initExamples() {
  const cards = document.querySelectorAll('[data-example]');
  for (const card of cards) {
    const example = data.examples.find((e) => e.id === card.dataset.example);
    if (example) showExample(card, example);
  }
}

// ---- Drop a file ----------------------------------------------------------

function initDropZone() {
  const zone = document.getElementById('drop');
  const input = document.getElementById('drop-input');
  const result = document.getElementById('drop-result');
  if (!zone || !input || !result) return;

  async function handle(file) {
    result.hidden = false;
    result.replaceChildren();
    const status = document.createElement('p');
    status.className = 'status';
    result.append(status);
    try {
      const shown = await decodeFile(new Uint8Array(await file.arrayBuffer()));
      const info = document.createElement('p');
      info.className = 'meta';
      info.textContent = `${file.name} · ${shown.width} × ${shown.height} · ${
        shown.stored ? fill(ui.storedAs, { mime: shown.stored }) : fill(ui.decodedIn, { ms: shown.ms.toFixed(1) })
      }`;
      result.append(info, shown.view);
      setStatus(status, ui.dropDone, 'ok');
      if (!shown.stored) {
        const link = document.createElement('a');
        link.className = 'button';
        link.textContent = ui.downloadPng;
        link.download = file.name.replace(/\.bic$/i, '') + '.png';
        shown.view.toBlob((blob) => {
          link.href = URL.createObjectURL(blob);
        }, 'image/png');
        result.append(link);
      }
    } catch (error) {
      const message = error instanceof BicError ? ui.notBic : ui.failed;
      setStatus(status, `${message}: ${error.message}`, 'bad');
    }
  }

  input.addEventListener('change', () => input.files[0] && handle(input.files[0]));
  zone.addEventListener('dragover', (event) => {
    event.preventDefault();
    zone.classList.add('over');
  });
  zone.addEventListener('dragleave', () => zone.classList.remove('over'));
  zone.addEventListener('drop', (event) => {
    event.preventDefault();
    zone.classList.remove('over');
    if (event.dataTransfer.files[0]) handle(event.dataTransfer.files[0]);
  });
}

// ---- Start ----------------------------------------------------------------

initTheme();
initExamples();
initDropZone();
document.getElementById('runtime-version')?.replaceChildren(data.runtimeVersion);
