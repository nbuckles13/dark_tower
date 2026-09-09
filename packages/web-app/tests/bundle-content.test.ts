// R-14 / @security: prove the production bundle EXCLUDES the dev-only E2E-hook
// and cert-trust surfaces. Runs a real production `vite build` and scans every
// emitted artifact (JS + sourcemaps + html) for forbidden tokens. This is the
// forcing function that turns "tree-shaken" into a verified property.

import { execFileSync } from 'node:child_process';
import { readdirSync, readFileSync, rmSync, statSync } from 'node:fs';
import { resolve } from 'node:path';
import { beforeAll, expect, test } from 'vitest';

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
const FORBIDDEN = [
  '__darktower_test__',
  'mediaFrameCounts',
  '__E2E_HOOKS__',
  'serverCertificateHashes',
  '__DEV_TRUST_FINGERPRINT__',
] as const;

function walk(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir)) {
    const full = resolve(dir, entry);
    if (statSync(full).isDirectory()) out.push(...walk(full));
    else out.push(full);
  }
  return out;
}

beforeAll(() => {
  rmSync(DIST, { recursive: true, force: true });
  execFileSync('pnpm', ['exec', 'vite', 'build', '--mode', 'production'], {
    cwd: PKG_ROOT,
    stdio: 'inherit',
  });
}, 180_000);

test('production dist emits at least one JS chunk', () => {
  const files = walk(DIST);
  expect(files.some((f) => f.endsWith('.js'))).toBe(true);
});

test.each(FORBIDDEN)('production bundle excludes forbidden token %s', (token) => {
  const files = walk(DIST);
  const offenders = files.filter((f) => readFileSync(f, 'utf8').includes(token));
  expect(offenders, `token "${token}" leaked into: ${offenders.join(', ')}`).toEqual([]);
});
