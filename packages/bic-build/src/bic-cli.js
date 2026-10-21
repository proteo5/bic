// Runs the `bic` command-line tool. The binary is looked up in this order:
// the explicit option, the BIC_BIN environment variable, the copy installed
// with this package, and finally `bic` on the PATH.

import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { vendoredBinary } from './install-binary.js';

const run = promisify(execFile);
const MAX_BUFFER = 64 << 20;

export function bicBinary(override) {
  return override || process.env.BIC_BIN || vendoredBinary() || 'bic';
}

/** Measurements for one image, as `bic inspect --json` reports them. */
export async function inspect(file, binary) {
  let stdout;
  try {
    ({ stdout } = await run(binary, ['inspect', file, '--json'], { maxBuffer: MAX_BUFFER }));
  } catch (error) {
    if (/unrecognized subcommand 'inspect'/.test(String(error.stderr))) {
      throw new Error(`${binary} is older than the bic-build package: it has no "inspect" command. ` +
        'Install a newer bic release, or set BIC_BIN to a current build.');
    }
    throw error;
  }
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
