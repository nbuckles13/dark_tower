// R-14 / @security: prove the production bundle EXCLUDES the dev-only E2E-hook
// and cert-trust surfaces. Runs a real production `vite build` and scans every
// emitted artifact (JS + sourcemaps + html) for forbidden tokens. This is the
// forcing function that turns "tree-shaken" into a verified property.

import { execFileSync } from 'node:child_process';
import { readdirSync, readFileSync, rmSync, statSync } from 'node:fs';
import { resolve } from 'node:path';
import { afterAll, beforeAll, expect, test } from 'vitest';
import { TONE_RUNTIME_TOKENS } from '../../sdk-core/tests/toneMarkers.js';

const PKG_ROOT = resolve(__dirname, '..');
const DIST = resolve(PKG_ROOT, 'dist');

// Absent from the prod bundle: the E2E test bus + its gate, and the dev cert
// trust path + its gate. `__DEV_CERT_SHA256_HASHES__` resolves to `[]` in prod.
//
// ---------------------------------------------------------------------------
// THE RULE: a token qualifies iff it exists NOWHERE in production code
// ---------------------------------------------------------------------------
//
// Every entry must be a string that appears ONLY inside the `__E2E_HOOKS__`
// gate (or the dev cert-trust branch). Add one that also names a production
// concept and this file reds a CORRECT build — after which the natural "fix" is
// to weaken the assertion, which is the vacuity trap ADR-0028 warns about.
//
// The test is the token, NOT the category. An earlier draft of this comment said
// "bus event-type strings do not qualify", which is too broad and would have
// excluded a valid marker — see `mediaFrameCounts` below.
//
// Worked examples, because the mistake has already been made once here:
//   * `firstMediaFrame` — FAILS the test. Emitted by production sdk-core from
//     `media/lifecycle/AudioPipeline.ts` (the `FirstMediaObserver` callback).
//   * `muteState` — FAILS. A production getter on the same class.
//   * `joined` / `stateChange` / `mediaConnected` — FAIL. Session event names.
//     That is why those older bus events are deliberately absent here.
//   * `mediaFrameCounts` — PASSES, and is listed. It appears only inside the
//     `__E2E_HOOKS__` block in `lib/e2eBus.ts` and nowhere in production, so it
//     is a true marker for the sampler — the one prod-side surface the media
//     work adds — rather than a coincidence of naming.
//
// Both markers below are still load-bearing on their own: every bus emit and the
// sampler live inside one `if (__E2E_HOOKS__)` block whose window-attach marker
// `__darktower_test__` and whose gate `__E2E_HOOKS__` are listed, so their
// absence already proves the block was eliminated. `mediaFrameCounts` pins the
// sampler specifically, which is the piece that could plausibly be hoisted out.
//
// Story 2 additions (R-7, R-30), each checked against the same rule:
//   * `test_tone`, `createOscillator`, `createMediaStreamDestination` — the
//     test-tone mode token and synthesis, reachable only inside
//     `if (__DT_TEST_TONE__)` (the mode vocabulary is a TYPE, so no const
//     object carries the token into production).
//   * `createAnalyser`, `getFloatFrequencyData` — the layer-3 per-lane analysis,
//     reachable only inside `if (__E2E_HOOKS__)` in `lib/e2eAnalysis.ts`.
//   * `receiveLayers`, `receiveAnalysis` — bus event types that exist only in
//     the gated block. NOT `receiveVerification` (an SDK option key, present in
//     production as a property name), `captureSource` or `streamAssignments`
//     (SDK getter / event names): all three FAIL the rule.
//
// Story 2 task 15 additions (the `__DT_TEST_LEVERS__` per-context levers and the
// bus surface this task adds), each checked against the same rule:
//   * `__dt_test_levers__` — the init-script global, read only inside
//     `if (__DT_TEST_LEVERS__)` in `lib/testLevers.ts`.
//   * `blockHandlers`, `forceHostControls` — lever keys, named only in
//     `lib/testLevers.ts` and at `__DT_TEST_LEVERS__`-guarded use sites.
//   * `buildKnobs`, `telemetry_not_configured` — bus event type / bus flush
//     refusal, only inside `if (__E2E_HOOKS__)` in `lib/e2eBus.ts`.
//   NOT `participantMute` (a substring of the SDK's `participantMuteChanged`
//   event) or `kekGeneration` (a protobuf field name): both FAIL the rule.
const FORBIDDEN = [
  '__darktower_test__',
  'mediaFrameCounts',
  '__E2E_HOOKS__',
  'serverCertificateHashes',
  '__DEV_TRUST_FINGERPRINT__',
  '__DT_TEST_TONE__',
  'test_tone',
  'createOscillator',
  'createMediaStreamDestination',
  'createAnalyser',
  'getFloatFrequencyData',
  'receiveLayers',
  'receiveAnalysis',
  '__DT_TEST_LEVERS__',
  '__dt_test_levers__',
  'blockHandlers',
  'forceHostControls',
  'buildKnobs',
  'telemetry_not_configured',
] as const;

/**
 * Gate IDENTIFIERS are replaced by the define in EVERY build, so they are absent
 * from the positive-control build too and are exempt from it. Every other
 * marker MUST appear there, or its absence from production proves nothing (a
 * misspelled or minifier-renamed marker would pass forever).
 */
const GATE_IDENTIFIERS = new Set<string>([
  '__E2E_HOOKS__',
  '__DEV_TRUST_FINGERPRINT__',
  '__DT_TEST_TONE__',
  '__DT_TEST_LEVERS__',
]);

/** The positive-control build: dev mode with the test tone AND the test levers opted in. */
const CONTROL_DIST = resolve(PKG_ROOT, 'dist-e2e-control');

function walk(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir)) {
    const full = resolve(dir, entry);
    if (statSync(full).isDirectory()) out.push(...walk(full));
    else out.push(full);
  }
  return out;
}

/** The environment WITHOUT any test-define opt-in, whatever the caller's shell carries. */
function envWithoutTone(): NodeJS.ProcessEnv {
  const env = { ...process.env };
  delete env['DT_TEST_TONE'];
  delete env['DT_TEST_LEVERS'];
  return env;
}

beforeAll(() => {
  rmSync(DIST, { recursive: true, force: true });
  rmSync(CONTROL_DIST, { recursive: true, force: true });
  execFileSync('pnpm', ['exec', 'vite', 'build', '--mode', 'production'], {
    cwd: PKG_ROOT,
    stdio: 'inherit',
    env: envWithoutTone(),
  });
  execFileSync(
    'pnpm',
    ['exec', 'vite', 'build', '--mode', 'development', '--outDir', CONTROL_DIST],
    {
      cwd: PKG_ROOT,
      stdio: 'inherit',
      env: { ...envWithoutTone(), DT_TEST_TONE: '1', DT_TEST_LEVERS: '1' },
    },
  );
}, 360_000);

afterAll(() => {
  rmSync(CONTROL_DIST, { recursive: true, force: true });
});

test('FORBIDDEN carries every sdk-core tone marker, so this control proves each one', () => {
  // sdk-core's own scan relies on THIS file's positive-control build to show its
  // markers are real (`packages/sdk-core/tests/toneMarkers.ts`).
  for (const token of TONE_RUNTIME_TOKENS) expect(FORBIDDEN).toContain(token);
});

test('production dist emits at least one JS chunk', () => {
  const files = walk(DIST);
  expect(files.some((f) => f.endsWith('.js'))).toBe(true);
});

test.each(FORBIDDEN)('production bundle excludes forbidden token %s', (token) => {
  const files = walk(DIST);
  const offenders = files.filter((f) => readFileSync(f, 'utf8').includes(token));
  expect(offenders, `token "${token}" leaked into: ${offenders.join(', ')}`).toEqual([]);
});

test.each(FORBIDDEN.filter((t) => !GATE_IDENTIFIERS.has(t)))(
  'POSITIVE CONTROL: the gated build (dev + DT_TEST_TONE=1 + DT_TEST_LEVERS=1) CONTAINS %s',
  (token) => {
    const files = walk(CONTROL_DIST).filter((f) => f.endsWith('.js'));
    expect(files.length).toBeGreaterThan(0);
    expect(
      files.some((f) => readFileSync(f, 'utf8').includes(token)),
      `marker "${token}" is absent even where its gate is ON — it proves nothing about production`,
    ).toBe(true);
  },
);

test('a PRODUCTION build asked for the test tone FAILS at the gate, never coerces it off', () => {
  let failure: unknown;
  try {
    execFileSync(
      'pnpm',
      ['exec', 'vite', 'build', '--mode', 'production', '--outDir', `${CONTROL_DIST}-prod-tone`],
      {
        cwd: PKG_ROOT,
        stdio: 'pipe',
        env: { ...envWithoutTone(), DT_TEST_TONE: '1' },
      },
    );
  } catch (err) {
    failure = err;
  } finally {
    rmSync(`${CONTROL_DIST}-prod-tone`, { recursive: true, force: true });
  }
  expect(failure, 'the production build with DT_TEST_TONE=1 must not succeed').toBeDefined();
  const output = String((failure as { stderr?: unknown }).stderr ?? '');
  expect(output).toContain('DT_TEST_TONE=1 in a production build');
}, 180_000);

test('a PRODUCTION build asked for the test levers FAILS at the gate, never coerces them off', () => {
  let failure: unknown;
  try {
    execFileSync(
      'pnpm',
      ['exec', 'vite', 'build', '--mode', 'production', '--outDir', `${CONTROL_DIST}-prod-levers`],
      {
        cwd: PKG_ROOT,
        stdio: 'pipe',
        env: { ...envWithoutTone(), DT_TEST_LEVERS: '1' },
      },
    );
  } catch (err) {
    failure = err;
  } finally {
    rmSync(`${CONTROL_DIST}-prod-levers`, { recursive: true, force: true });
  }
  expect(failure, 'the production build with DT_TEST_LEVERS=1 must not succeed').toBeDefined();
  const output = String((failure as { stderr?: unknown }).stderr ?? '');
  expect(output).toContain('DT_TEST_LEVERS=1 in a production build');
}, 180_000);
