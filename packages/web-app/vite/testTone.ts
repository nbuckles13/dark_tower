// File: packages/web-app/vite/testTone.ts
//
// NODE-ONLY build helper (runs at Vite config time, never shipped). Story 2
// R-7: resolve the `__DT_TEST_TONE__` build-time define.
//
// ONE PREDICATE FEEDS BOTH THE DEFINE AND THE PRODUCTION REFUSAL. A second parse
// (say, the define accepting `true` while the refusal checks `=== '1'`) would let
// a value slip past the refusal and ship a production tone bundle with no error.
//
// The rules (@security S1, @operations O-1):
//   * OPT-IN, NEVER DEFAULT-ON. Unset (or empty) means no tone, in every mode —
//     unlike `__E2E_HOOKS__`, which is on in every dev build.
//   * The only accepted opt-in spelling is `1`. Any other value THROWS: a typo
//     such as `true` silently building the microphone path would read as "the
//     tone is on" to whoever set it.
//   * `1` in PRODUCTION mode THROWS. Never coerced to `false`: a production
//     build asked for a test tone is a pipeline defect, and failing the build is
//     the only signal that cannot be ignored.
//   * Never a runtime setting: the value is a literal in the bundle.
//
// Enable: `DT_TEST_TONE=1 scripts/dev-web.sh` (or `DT_TEST_TONE=1 pnpm dev`).

/** The environment variable that opts a dev build into the test tone. */
export const TEST_TONE_ENV = 'DT_TEST_TONE';

/**
 * Whether this build carries the test tone. THROWS on a malformed opt-in and on
 * an opt-in in production mode.
 */
export function resolveTestTone(mode: string, raw: string | undefined): boolean {
  if (raw === undefined || raw === '') return false;
  if (raw !== '1') {
    throw new Error(
      `${TEST_TONE_ENV} must be unset or exactly "1", got ${JSON.stringify(raw.slice(0, 32))}. ` +
        `Refusing to guess whether the test-tone build (story 2 R-7) was intended.`,
    );
  }
  if (mode === 'production') {
    throw new Error(
      `${TEST_TONE_ENV}=1 in a production build. The test tone replaces every participant's ` +
        `microphone and must never reach a production bundle; unset ${TEST_TONE_ENV} to build ` +
        `for production.`,
    );
  }
  return true;
}
