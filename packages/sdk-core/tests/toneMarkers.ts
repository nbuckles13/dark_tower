// File: packages/sdk-core/tests/toneMarkers.ts
//
// THE test-tone bundle markers (story 2 R-7) — one list, read by BOTH bundle
// scans: sdk-core's (`bundle-content.test.ts`, which forbids them in its runtime
// artifacts) and the web app's (`packages/web-app/tests/bundle-content.test.ts`,
// whose FORBIDDEN list must be a superset of this one and whose positive-control
// build proves each marker is real). An sdk-core marker is only evidence while
// the web app's control proves it, which is why the web app asserts the
// superset rather than restating the tokens.
//
// Each is a string or DOM method name the minifier cannot rename, present only
// inside the `__DT_TEST_TONE__` gate.
export const TONE_RUNTIME_TOKENS = [
  '__DT_TEST_TONE__',
  'createOscillator',
  'createMediaStreamDestination',
  'test_tone',
] as const;

/**
 * THE test-PATTERN bundle markers (story 3 R-27) — same contract as
 * {@link TONE_RUNTIME_TOKENS}: forbidden in sdk-core's runtime artifacts, and
 * the web app's FORBIDDEN list must be a superset whose positive-control build
 * proves each one real. Generator-unique literals only: `dt-test-pattern` is
 * the generator's marker (`media/setup/testPattern.ts:TEST_PATTERN_MARKER`, on
 * a live gated path), `test_pattern` the mode token. A platform API name such
 * as `OffscreenCanvas` is deliberately NOT a marker: a later legitimate render
 * path could use it.
 */
export const PATTERN_RUNTIME_TOKENS = [
  '__DT_TEST_PATTERN__',
  'test_pattern',
  'dt-test-pattern',
] as const;
