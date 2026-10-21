// File: packages/web-app/tests/_patternSpec.ts
//
// TEST-ONLY: the video test pattern's layout, DERIVED by parsing the one home
// of the spec, the assignment table in `e2e/README.md` §"Video test pattern".
// Shared by `patternDetector.test.ts` and `videoEvidence.test.ts`, so neither
// carries a hand transcription that could drift from the README. Never
// imported by `e2e/` (the detector must not be built from the same table its
// tests use). Not a test file: no `.test.ts` suffix, so vitest's
// `tests/**/*.test.ts` include does not collect it.
//
// The parse is anchored by literal grids in `patternDetector.test.ts` (e.g.
// sender 0x8001 frame 0 → indices 0, 1, 17, 23 white), so a parser bug cannot
// silently agree with a detector bug.

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

/** A README parse failure: the spec moved or changed shape. Never a verdict. */
export class PatternSpecExtractionError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'PatternSpecExtractionError';
  }
}

const README = fileURLToPath(new URL('../e2e/README.md', import.meta.url));
const SECTION = '### Video test pattern';

/** Parse the README text into the row-major layout and the guard colours. */
export function parsePatternSpec(readme: string): {
  readonly layout: readonly string[];
  readonly guardBits: Readonly<Record<string, number>>;
} {
  const start = readme.indexOf(SECTION);
  if (start === -1) throw new PatternSpecExtractionError(`README has no "${SECTION}" section`);
  const next = readme.indexOf('\n### ', start + SECTION.length);
  const section = readme.slice(start, next === -1 ? undefined : next);
  const rows: string[][] = [];
  for (const line of section.split('\n')) {
    const m = /^\s*\|\s*(\d+)\s*\|(.*)\|\s*$/.exec(line);
    if (m === null) continue;
    const cells = (m[2] ?? '').split('|').map((c) => c.trim());
    rows.push(cells);
  }
  if (rows.length !== 4 || rows.some((r) => r.length !== 6)) {
    throw new PatternSpecExtractionError(
      `README pattern assignment table is not 4x6 (found ${rows.length} row(s): ` +
        `${rows.map((r) => r.length).join(', ')} cells)`,
    );
  }
  const layout = rows.flat();
  if (!layout.every((n) => /^(G[0-3]|I(1[0-5]|[0-9])|C[0-3])$/.test(n))) {
    throw new PatternSpecExtractionError('README pattern table has an unknown cell name');
  }
  const guardBits: Record<string, number> = {};
  for (const m of section.matchAll(/\b(G[0-3]) \([^)]*\) = (white|black)/g)) {
    guardBits[m[1]!] = m[2] === 'white' ? 1 : 0;
  }
  if (Object.keys(guardBits).length !== 4) {
    throw new PatternSpecExtractionError(
      `README names ${Object.keys(guardBits).length} guard colour(s), expected 4 (G0..G3)`,
    );
  }
  return { layout, guardBits };
}

const parsed = parsePatternSpec(readFileSync(README, 'utf8'));

/** The row-major 6x4 layout, as the README states it. */
export const LAYOUT: readonly string[] = parsed.layout;
/** Guard colours, white = 1. */
export const GUARD_BITS: Readonly<Record<string, number>> = parsed.guardBits;

/** The 24 nominal lumas the spec draws for (senderId, captured frame n). */
export function specLumas(senderId: number, frame: number, white: number, black: number): number[] {
  const n = frame % 16;
  const gray = n ^ (n >> 1);
  return LAYOUT.map((name) => {
    let bit: number;
    if (name in GUARD_BITS) bit = GUARD_BITS[name]!;
    else if (name.startsWith('I')) bit = (senderId >> Number(name.slice(1))) & 1;
    else bit = (gray >> Number(name.slice(1))) & 1;
    return bit === 1 ? white : black;
  });
}

/**
 * Every module specifier a TS source pulls in: `import … from`, `export … from`,
 * side-effect `import '…'`, `require('…')`, and dynamic `import(…)` (reported as
 * `dynamic import`). The import-rule tests scan with this.
 */
export function importedSpecifiers(source: string): string[] {
  const out: string[] = [];
  const shapes = [
    /^\s*(?:import|export)\s+(?:type\s+)?[^;]*?from\s+['"]([^'"]+)['"]/gm,
    /^\s*import\s+['"]([^'"]+)['"]/gm,
    /\brequire\s*\(\s*['"]([^'"]+)['"]\s*\)/g,
  ];
  for (const re of shapes) for (const m of source.matchAll(re)) out.push(m[1] ?? '');
  if (/\bimport\s*\(/.test(source)) out.push('dynamic import');
  return out;
}
