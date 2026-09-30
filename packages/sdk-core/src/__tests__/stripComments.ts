// File: packages/sdk-core/src/__tests__/stripComments.ts
//
// The ONE comment stripper for the SDK's source-scan tests (extracted on third
// use, story 2 task 13: `serverMessageSinkScan.test.ts`, `media/__tests__/
// hotPathLayout.test.ts`, `joinLabelSpread.test.ts`).

/**
 * Blank out comments while PRESERVING LINE NUMBERS, so a file DOCUMENTING a rule
 * does not trip it.
 *
 * A block comment is replaced by its own newlines rather than deleted, so a
 * reported `file:line` points at the real source. A scan that names the wrong
 * line sends its reader to innocent code and gets dismissed as a false positive
 * — which is how a true finding is lost. `://` is left alone so URLs survive.
 */
export function stripComments(source: string): string {
  return source
    .replace(/\/\*[\s\S]*?\*\//g, (block) => block.replace(/[^\n]/g, ' '))
    .replace(/(^|[^:])\/\/.*$/gm, '$1');
}
