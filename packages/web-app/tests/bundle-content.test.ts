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
const FORBIDDEN = [
  '__darktower_test__',
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
