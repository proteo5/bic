// BIC2 decoder in JavaScript. A line-by-line port of the Rust decoder in
// src/lib.rs (decode_native and its helpers), so both produce the same pixels.
// The conformance tests in test/ compare the two on every fixture.
//
// Zstandard comes from `fzstd`, a pure JavaScript decoder.

import { decompress as zstdDecompress } from 'fzstd';

const MAGIC = [0x42, 0x49, 0x43, 0x32]; // "BIC2"
const VERSION = 2;
const HEADER_LEN = 32;
const FLAG_ICC = 1;
const MAX_ICC = 16 << 20;
const DEFAULT_MAX_PIXELS = 100_000_000;
const MAX_BLOCK = 4096;
const RLE_BITS = [4, 8, 12, 16];
// Green is decoded first, so red and blue can be predicted from it.
const CHANNEL_ORDER = [1, 0, 2, 3];

const STORAGE_NATIVE = 0;
const STORAGE_PNG = 1;
const STORAGE_ORIGINAL = 2;

const TRANSFORM_DIRECT = 0;
const TRANSFORM_SUBTRACT_GREEN = 1;
const TRANSFORM_PALETTE = 2;

const PREDICTOR_NONE = 0;
const PREDICTOR_UP = 1;
const PREDICTOR_PER_ROW = 2;

// Row rules, numbered as in the format (FORMAT.md).
const RULE_NONE = 0;
const RULE_LEFT = 1;
const RULE_UP = 2;
const RULE_AVERAGE = 3;
const RULE_PAETH = 4;
const RULE_MED = 5;

const CHANNEL_CONSTANT = 0;
const CHANNEL_CODED = 1;

export class BicError extends Error {
  constructor(message) {
    super(message);
    this.name = 'BicError';
  }
}

function fail(message) {
  throw new BicError(message);
}

function clamp(v, lo, hi) {
  return v < lo ? lo : v > hi ? hi : v;
}

function u64(view, offset) {
  const lo = view.getUint32(offset, true);
  const hi = view.getUint32(offset + 4, true);
  if (hi >= 0x200000) fail('length exceeds 2^53');
  return hi * 0x100000000 + lo;
}

function checkedPixels(width, height, maxPixels) {
  const pixels = width * height;
  if (!Number.isSafeInteger(pixels) || pixels > maxPixels) fail('image exceeds pixel limit');
  return pixels;
}

/**
 * Splits a BIC2 file into its header fields and payload, checking the layout
 * the same way the Rust parser does.
 */
export function parseContainer(input, { maxPixels = DEFAULT_MAX_PIXELS } = {}) {
  const bytes = input instanceof Uint8Array ? input : new Uint8Array(input);
  if (bytes.length < HEADER_LEN || MAGIC.some((b, i) => bytes[i] !== b)) fail('not a BIC2 file');
  if (bytes[4] !== VERSION) fail('unsupported BIC version');
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const storage = bytes[5];
  const flags = view.getUint16(6, true);
  if (flags & ~FLAG_ICC) fail('unsupported BIC flags');
  const width = view.getUint32(8, true);
  const height = view.getUint32(12, true);
  const pixels = checkedPixels(width, height, maxPixels);
  const plainLen = u64(view, 16);
  const payloadLen = u64(view, 24);

  let at = HEADER_LEN;
  let icc = null;
  if (flags & FLAG_ICC) {
    if (bytes.length < at + 4) fail('truncated ICC profile');
    const len = view.getUint32(at, true);
    at += 4;
    if (len > MAX_ICC) fail('ICC profile too large');
    if (bytes.length < at + len) fail('truncated ICC profile');
    icc = bytes.subarray(at, at + len);
    at += len;
  }
  if (bytes.length - at !== payloadLen) fail('container payload length mismatch');
  return { storage, width, height, pixels, plainLen, icc, payload: bytes.subarray(at) };
}

/** Image type of an embedded file, from its signature. Null if unknown. */
export function sniffImageType(bytes) {
  if (bytes.length >= 8 && bytes[0] === 0x89 && bytes[1] === 0x50 && bytes[2] === 0x4e && bytes[3] === 0x47) {
    return 'image/png';
  }
  if (bytes.length >= 3 && bytes[0] === 0xff && bytes[1] === 0xd8 && bytes[2] === 0xff) return 'image/jpeg';
  if (bytes.length >= 12 && String.fromCharCode(...bytes.subarray(0, 4)) === 'RIFF' &&
      String.fromCharCode(...bytes.subarray(8, 12)) === 'WEBP') {
    return 'image/webp';
  }
  if (bytes.length >= 12 && String.fromCharCode(...bytes.subarray(4, 8)) === 'ftyp') {
    const brand = String.fromCharCode(...bytes.subarray(8, 12));
    if (['heic', 'heix', 'heim', 'heis', 'hevc', 'hevx', 'mif1', 'msf1', 'heif'].includes(brand)) {
      return 'image/heif';
    }
  }
  return null;
}

/** Quantizer of the near-lossless mode (FORMAT.md, "Prediction and quantization"). */
function quantizer(maxError) {
  const step = 2 * maxError + 1;
  return { near: maxError, step, range: Math.floor((255 + 2 * maxError) / step) + 1 };
}

function signed(q, symbol) {
  return symbol >= Math.floor((q.range + 1) / 2) ? symbol - q.range : symbol;
}

function reconstruct(q, prediction, symbol) {
  let r = prediction + signed(q, symbol) * q.step;
  if (r < -q.near) r += q.range * q.step;
  else if (r > 255 + q.near) r -= q.range * q.step;
  return clamp(r, 0, 255);
}

/** The prediction rule applied to the left, upper and upper-left neighbours. */
function predictDelta(rule, left, up, upLeft) {
  switch (rule) {
    case RULE_NONE:
      return 0;
    case RULE_LEFT:
      return left;
    case RULE_UP:
      return up;
    case RULE_AVERAGE:
      return (left + up) >> 1;
    case RULE_PAETH: {
      const pl = Math.abs(up - upLeft);
      const pu = Math.abs(left - upLeft);
      const pul = Math.abs(left + up - 2 * upLeft);
      if (pl <= pu && pl <= pul) return left;
      return pu <= pul ? up : upLeft;
    }
    case RULE_MED: {
      const lo = Math.min(left, up);
      const hi = Math.max(left, up);
      return clamp(left + up - upLeft, lo, hi);
    }
    default:
      fail('invalid row predictor');
  }
  return 0;
}

/**
 * Reconstructs row y in place. For each sample, the prediction comes from
 * samples already reconstructed (see FORMAT.md), with the same rules and edges
 * as the Rust decoder's `prediction`. Written without per-sample closures,
 * because this loop runs once per pixel.
 */
function reconstructRow(plane, base, width, y, rule, q) {
  const row = y * width;
  const lossless = q.near === 0;
  for (let x = 0; x < width; x++) {
    const i = row + x;
    const b = base === null ? 0 : base[i];
    let p;
    if (rule === RULE_NONE || i === 0) {
      p = clamp(b, 0, 255);
    } else if (y === 0) {
      // First row: left neighbour.
      const left = plane[i - 1] - (base === null ? 0 : base[i - 1]);
      p = clamp(b + left, 0, 255);
    } else if (x === 0) {
      // First column: upper neighbour.
      const up = plane[i - width] - (base === null ? 0 : base[i - width]);
      p = clamp(b + up, 0, 255);
    } else {
      const left = plane[i - 1] - (base === null ? 0 : base[i - 1]);
      const up = plane[i - width] - (base === null ? 0 : base[i - width]);
      const upLeft = plane[i - width - 1] - (base === null ? 0 : base[i - width - 1]);
      p = clamp(b + predictDelta(rule, left, up, upLeft), 0, 255);
    }
    const s = plane[i];
    if (lossless) {
      // With max_error = 0 the quantizer is the identity modulo 256, and every
      // symbol is a byte, so the range check cannot fail.
      plane[i] = (p + s) & 255;
    } else {
      if (s >= q.range) fail('symbol outside quantizer range');
      plane[i] = reconstruct(q, p, s);
    }
  }
}

function take(data, cursor, n) {
  if (cursor.at + n > data.length) fail('truncated stream');
  const out = data.subarray(cursor.at, cursor.at + n);
  cursor.at += n;
  return out;
}

function readU16(data, cursor) {
  const b = take(data, cursor, 2);
  return b[0] | (b[1] << 8);
}

function readU32(data, cursor) {
  const b = take(data, cursor, 4);
  return (b[0] | (b[1] << 8) | (b[2] << 16) | (b[3] << 24)) >>> 0;
}

/** Decodes one RLE block into out[n..n+len) and returns the new position. */
function decodeRle(data, len, bits, out, n) {
  if (data.length < 2) fail('truncated RLE header');
  const runs = data[0] | (data[1] << 8);
  if (data.length < 2 + runs) fail('truncated RLE values');
  let at = 2 + runs;
  let buf = 0;
  let have = 0;
  const target = n + len;
  const getCounter = () => {
    while (have < bits) {
      if (at >= data.length) fail('truncated RLE counter');
      buf |= data[at] << have;
      at += 1;
      have += 8;
    }
    const v = buf & ((1 << bits) - 1);
    buf >>>= bits;
    have -= bits;
    return v;
  };
  for (let k = 0; k < runs; k++) {
    const value = data[2 + k];
    const count = getCounter() + 1;
    if (n + count > target) fail('RLE overrun');
    out.fill(value, n, n + count);
    n += count;
  }
  if (n !== target) fail('invalid RLE length or padding');
  if (buf !== 0) fail('invalid RLE length or padding');
  for (let k = at; k < data.length; k++) {
    if (data[k] !== 0) fail('invalid RLE length or padding');
  }
  return n;
}

/** Reads a plane of `expected` bytes stored as blocks (FORMAT.md, "Blocks and modes"). */
function readPlane(data, cursor, expected) {
  const count = readU32(data, cursor);
  if (count > expected) fail('too many blocks');
  const out = new Uint8Array(expected);
  let n = 0;
  for (let k = 0; k < count; k++) {
    const len = readU16(data, cursor);
    if (!(len > 0 && len <= MAX_BLOCK && n + len <= expected)) fail('bad block length');
    const mode = take(data, cursor, 1)[0];
    const bytes = readU32(data, cursor);
    const payload = take(data, cursor, bytes);
    if (mode === 0) {
      if (payload.length !== len) fail('RAW length mismatch');
      out.set(payload, n);
      n += len;
    } else if (mode >= 1 && mode <= 4) {
      n = decodeRle(payload, len, RLE_BITS[mode - 1], out, n);
    } else {
      fail('unknown block mode');
    }
  }
  if (n !== expected) fail('plane length mismatch');
  return out;
}

/** One channel: constant, or coded symbols reconstructed row by row. */
function readChannel(data, cursor, pixels, base, width, predictor, q) {
  const kind = take(data, cursor, 1)[0];
  if (kind === CHANNEL_CONSTANT) {
    const value = take(data, cursor, 1)[0];
    return new Uint8Array(pixels).fill(value);
  }
  if (kind !== CHANNEL_CODED) fail(`invalid channel kind ${kind}`);

  const rows = width === 0 ? 0 : Math.floor(pixels / width);
  let rules;
  if (predictor === PREDICTOR_NONE) {
    rules = new Uint8Array(rows);
  } else if (predictor === PREDICTOR_UP) {
    rules = new Uint8Array(rows).fill(RULE_UP);
  } else {
    rules = readPlane(data, cursor, rows);
    for (const r of rules) if (r > RULE_MED) fail('invalid row predictor');
  }

  const plane = readPlane(data, cursor, pixels);
  for (let y = 0; y < rows; y++) reconstructRow(plane, base, width, y, rules[y], q);
  return plane;
}

/** The native stream (FORMAT.md, "Native stream") to RGBA bytes. */
function decodeNative(width, height, plain) {
  const pixels = width * height;
  const cursor = { at: 0 };
  const header = take(plain, cursor, 3);
  let transform;
  if (header[0] === TRANSFORM_DIRECT) transform = TRANSFORM_DIRECT;
  else if (header[0] === TRANSFORM_SUBTRACT_GREEN) transform = TRANSFORM_SUBTRACT_GREEN;
  else if (header[0] === TRANSFORM_PALETTE) transform = TRANSFORM_PALETTE;
  else fail(`invalid transform ${header[0]}`);
  let predictor;
  if (header[1] === PREDICTOR_NONE) predictor = PREDICTOR_NONE;
  else if (header[1] === PREDICTOR_UP) predictor = PREDICTOR_UP;
  else if (header[1] === PREDICTOR_PER_ROW) predictor = PREDICTOR_PER_ROW;
  else fail(`invalid predictor ${header[1]}`);
  const maxError = header[2];
  const q = quantizer(maxError);
  const rgba = new Uint8Array(pixels * 4);

  if (transform === TRANSFORM_PALETTE) {
    if (maxError !== 0) fail('palette encoding is lossless only');
    const count = take(plain, cursor, 1)[0] + 1;
    const colors = take(plain, cursor, count * 4);
    const indices = readChannel(plain, cursor, pixels, null, width, predictor, q);
    for (let i = 0; i < pixels; i++) {
      const index = indices[i];
      if (index >= count) fail('palette index out of range');
      rgba.set(colors.subarray(index * 4, index * 4 + 4), i * 4);
    }
  } else {
    let green = null;
    for (const c of CHANNEL_ORDER) {
      const base = transform === TRANSFORM_SUBTRACT_GREEN && (c === 0 || c === 2) ? green : null;
      const plane = readChannel(plain, cursor, pixels, base, width, predictor, q);
      for (let i = 0; i < pixels; i++) rgba[i * 4 + c] = plane[i];
      if (c === 1 && transform === TRANSFORM_SUBTRACT_GREEN) green = plane;
    }
  }
  if (cursor.at !== plain.length) fail('trailing native stream bytes');
  return rgba;
}

/**
 * Decodes a BIC2 file.
 *
 * Returns one of:
 *   { kind: 'pixels', width, height, rgba, icc }   for BIC's own coding
 *   { kind: 'file', mime, bytes, width, height, icc } for a PNG or an original
 *       JPEG/WebP/HEIF file stored inside the container. The browser decodes
 *       these, so no pixel work is done here.
 *
 * Options: maxPixels (default 100 million). Throws BicError on invalid input.
 */
export function decodeBic(input, options = {}) {
  const c = parseContainer(input, options);

  if (c.storage === STORAGE_PNG) {
    return { kind: 'file', mime: 'image/png', bytes: c.payload, width: c.width, height: c.height, icc: c.icc };
  }
  if (c.storage === STORAGE_ORIGINAL) {
    const mime = sniffImageType(c.payload);
    if (!mime) fail('embedded file is not PNG, JPEG, WebP or HEIF');
    return { kind: 'file', mime, bytes: c.payload, width: c.width, height: c.height, icc: c.icc };
  }
  if (c.storage !== STORAGE_NATIVE) fail(`unsupported storage type ${c.storage}`);

  // Same bound as the Rust decoder's memory check: 6 bytes per pixel of working
  // memory, plus the decompressed stream. Refuse declared sizes beyond that
  // before decompressing anything.
  const plainLen = c.plainLen;
  if (plainLen > 8 * c.pixels + 4096) fail('stream length does not fit the image size');

  // The output buffer is sized from the header, so a payload that inflates
  // further is cut off and the length check below rejects it.
  const buffer = new Uint8Array(plainLen);
  let plain;
  try {
    plain = zstdDecompress(c.payload, buffer);
  } catch (error) {
    fail(`invalid Zstandard data: ${error.message}`);
  }
  if (plain.length !== plainLen) fail('Zstd output length mismatch');
  const rgba = decodeNative(c.width, c.height, plain);
  return { kind: 'pixels', width: c.width, height: c.height, rgba, icc: c.icc };
}
