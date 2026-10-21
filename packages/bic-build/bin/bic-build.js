#!/usr/bin/env node
// bic-build <input-folder> <output-folder> [--min-saving 10] [--max-colors 4096] [--bic path]
import { optimizeDirectory, DEFAULT_POLICY } from '../src/index.js';

const args = process.argv.slice(2);
const flags = {};
const positional = [];
for (let i = 0; i < args.length; i++) {
  if (args[i].startsWith('--')) flags[args[i].slice(2)] = args[++i];
  else positional.push(args[i]);
}
if (positional.length !== 2) {
  console.error('usage: bic-build <input-folder> <output-folder> [--min-saving 10] [--max-colors 4096] [--bic path]');
  process.exit(2);
}

const policy = {
  minSaving: flags['min-saving'] !== undefined ? Number(flags['min-saving']) / 100 : DEFAULT_POLICY.minSaving,
  maxDistinctColors: flags['max-colors'] !== undefined ? Number(flags['max-colors']) : DEFAULT_POLICY.maxDistinctColors,
};

const { entries, totals } = await optimizeDirectory({
  input: positional[0],
  output: positional[1],
  bic: flags.bic,
  policy,
});

for (const e of entries) {
  const mark = e.use ? 'BIC ' : 'keep';
  console.log(`${mark} ${e.source.padEnd(40)} ${e.reason}`);
}
console.log(
  `\n${totals.converted} of ${totals.images} images converted; ` +
  `${totals.savedBytes} bytes saved against the source files (${totals.sourceBytesConverted} -> ${totals.bicBytesConverted}).`
);
