// The decision: should an image be stored as BIC?
//
// Pure function of a measurement (see `bic inspect --json`), so projects can
// read it, test it and change the thresholds without running the encoder.

/** Default thresholds. A project can override any of them. */
export const DEFAULT_POLICY = Object.freeze({
  // BIC must be at least this much smaller than the best lossless alternative.
  minSaving: 0.1,
  // Images with more distinct colors than this are treated as photographs and
  // never converted. Measured: BIC does not win on photographs.
  maxDistinctColors: 4096,
});

/**
 * @param {object} report  The JSON printed by `bic inspect --json`.
 * @param {object} [policy]  Overrides for DEFAULT_POLICY.
 * @returns {{ use: boolean, reason: string, saving: number|null,
 *            best: { name: string, bytes: number } | null }}
 *   saving is the fraction BIC saves against `best` (0.25 means 25 % smaller).
 */
export function decide(report, policy = {}) {
  const { minSaving, maxDistinctColors } = { ...DEFAULT_POLICY, ...policy };
  const sizes = report.sizes;
  const alternatives = [
    ['PNG', sizes.png],
    ['oxipng', sizes.oxipng],
    ['WebP lossless', sizes.webp_lossless],
  ];
  const [name, bytes] = alternatives.reduce((a, b) => (b[1] < a[1] ? b : a));
  const best = { name, bytes };

  if (report.bic_storage === 'original') {
    return {
      use: false,
      reason: 'the source is already a lossy format that BIC keeps as it is',
      saving: null,
      best: null,
    };
  }
  if (report.distinct_colors > maxDistinctColors) {
    return {
      use: false,
      reason: `${report.distinct_colors} colors, above the ${maxDistinctColors} limit for flat graphics`,
      saving: null,
      best,
    };
  }
  const saving = 1 - sizes.bic / bytes;
  if (saving < minSaving) {
    const pct = Math.abs(saving * 100).toFixed(1);
    const change = saving < 0 ? `${pct}% larger than` : `saves only ${pct}% against`;
    return {
      use: false,
      reason: `${change} ${name}, below the ${(minSaving * 100).toFixed(0)}% threshold`,
      saving,
      best,
    };
  }
  return {
    use: true,
    reason: `saves ${(saving * 100).toFixed(1)}% against ${name}`,
    saving,
    best,
  };
}
