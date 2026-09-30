// File: packages/web-app/vite/testDefines.ts
//
// NODE-ONLY build helper (runs at Vite config time, never shipped). Resolves the
// OPT-IN TEST build defines:
//
//   * `__DT_TEST_TONE__`   (`DT_TEST_TONE=1`)   — story 2 R-7: the capture source
//     is a per-participant tone instead of the microphone.
//   * `__DT_TEST_LEVERS__` (`DT_TEST_LEVERS=1`) — story 2 task 15: per-browsing-
//     context test levers (handler blocking for S10a, per-context N for S2,
//     forced host controls for the non-host refusal) read from ONE init-script
//     global at ONE site, `src/lib/testLevers.ts`.
//
// ONE PREDICATE FEEDS EVERY DEFINE AND EVERY PRODUCTION REFUSAL. A second parse
// (say, a define accepting `true` while its refusal checks `=== '1'`) would let a
// value slip past the refusal and ship a production test bundle with no error.
//
// The rules (@security S1/L1, @operations O-1), identical for every define:
//   * OPT-IN, NEVER DEFAULT-ON. Unset (or empty) means off, in every mode —
//     unlike `__E2E_HOOKS__`, which is on in every dev build.
//   * The only accepted opt-in spelling is `1`. Any other value THROWS: a typo
//     such as `true` silently building without the lever would read as "on".
//   * `1` in PRODUCTION mode THROWS. Never coerced to `false`: a production
//     build asked for a test define is a pipeline defect, and failing the build
//     is the only signal that cannot be ignored.
//   * Never a runtime setting: the value is a literal in the bundle.

/** The environment variable that opts a dev build into the test tone. */
export const TEST_TONE_ENV = 'DT_TEST_TONE';

/** The environment variable that opts a dev build into the per-context test levers. */
export const TEST_LEVERS_ENV = 'DT_TEST_LEVERS';

/**
 * Whether this build carries the test define named by `envName`. THROWS on a
 * malformed opt-in and on an opt-in in production mode.
 */
export function resolveOptInTestDefine(
  envName: string,
  mode: string,
  raw: string | undefined,
): boolean {
  if (raw === undefined || raw === '') return false;
  if (raw !== '1') {
    throw new Error(
      `${envName} must be unset or exactly "1", got ${JSON.stringify(raw.slice(0, 32))}. ` +
        `Refusing to guess whether the ${envName} test build was intended.`,
    );
  }
  if (mode === 'production') {
    throw new Error(
      `${envName}=1 in a production build. Test-only build defines change what the client ` +
        `captures or connects to and must never reach a production bundle; unset ${envName} ` +
        `to build for production.`,
    );
  }
  return true;
}
