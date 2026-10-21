// Runs the `bic` command-line tool. The binary is found through the BIC_BIN
// environment variable, or as `bic` on the PATH.

import { execFile } from 'node:child_process';
import { promisify } from 'node:util';

const run = promisify(execFile);
const MAX_BUFFER = 64 << 20;

export function bicBinary(override) {
  return override || process.env.BIC_BIN || 'bic';
}

/** Measurements for one image, as `bic inspect --json` reports them. */
export async function inspect(file, binary) {
  const { stdout } = await run(binary, ['inspect', file, '--json'], { maxBuffer: MAX_BUFFER });
  return JSON.parse(stdout);
}

/** Encodes `input` to `output` with default settings, then checks it. */
export async function encode(input, output, binary) {
  await run(binary, ['encode', input, output], { maxBuffer: MAX_BUFFER });
  // Lossless files must decode to the source pixels; checking here costs one
  // decode and guards against a broken encoder build.
  const { stdout } = await run(binary, ['verify', input, output], { maxBuffer: MAX_BUFFER });
  if (!stdout.includes('verified')) throw new Error(`verification failed for ${input}`);
}
