// File: packages/sdk-core/tests/bundle-content.test.ts
//
// R-14 build-artifact contract test (component tier — runs a real production
// `vite build`). Canonical path enforced by the dt-guard
// `crates/dt-guard/src/ts_dev_trust.rs` (state 2 -> 3). DO NOT rename without
// updating that guard's expected path.
//
// Asserts the production EXECUTABLE bundle does NOT contain the dev-only
// WebTransport cert-trust path. Forbidden tokens (per @semantic-guard Gate-2):
//   - `serverCertificateHashes` — the dev trust option
//   - `__DEV_TRUST_FINGERPRINT__` — the build-time flag (must be substituted
//      away, not left as a live identifier)
// A clean executable bundle proves the `if (__DEV_TRUST_FINGERPRINT__) { ... }`
// branch was statically eliminated / tree-shaken, not merely runtime-guarded.
//
// SCOPE (executable artifacts only — `.mjs`/`.cjs`): the scan deliberately
// EXCLUDES `.d.ts` declaration files and `.map` sourcemaps. Rationale: R-14 is
// a runtime/executable-code requirement — the trust path must not be able to
// RUN in prod. `.d.ts` files are compile-time types, not shipped/executed code;
// `ServerCertificateHash` / `serverCertificateHashes` legitimately appear there
// as the dev-facing API surface (a caller in dev needs the type). A type name
// in a declaration file cannot enable any runtime trust behavior. The dev
// fingerprint VALUE is still never present anywhere (it is runtime config).
// This scope was surfaced to @security at Gate-2.

import { execFileSync } from 'node:child_process';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { beforeAll, describe, expect, it } from 'vitest';

const PKG_ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const DIST = join(PKG_ROOT, 'dist');

// Both the browser trust-option key AND the build-time gate flag must be
// absent from the prod build (the gate must be substituted away, not left
// live). NOTE: the prod build here injects NO concrete dev fingerprint VALUE
// (the value only ever flows from runtime config into `connect`, never into the
// build). If a future test ever builds with a concrete fingerprint fixture,
// add that literal to this list so it cannot slip through.
const FORBIDDEN_TOKENS = ['serverCertificateHashes', '__DEV_TRUST_FINGERPRINT__'];

function listFiles(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) {
      out.push(...listFiles(full));
    } else {
      out.push(full);
    }
  }
  return out;
}

describe('R-14: production bundle excludes the dev-trust path', () => {
  beforeAll(() => {
    // Real production build — `mode=production` makes Vite substitute
    // `__DEV_TRUST_FINGERPRINT__` -> false and tree-shake the dead branch.
    // Use `pnpm exec` (not `npx`): it resolves the workspace-local, lockfile-
    // pinned `vite` deterministically and never reaches the registry, matching
    // the repo's `--frozen-lockfile` posture (R-34).
    execFileSync('pnpm', ['exec', 'vite', 'build', '--mode', 'production'], {
      cwd: PKG_ROOT,
      stdio: 'inherit',
    });
  });

  it('emits the primary ESM + CJS artifacts', () => {
    const files = listFiles(DIST).map((f) => f.replace(DIST + '/', ''));
    expect(files).toContain('index.mjs');
    expect(files).toContain('index.cjs');
    expect(files).toContain('index.d.ts');
  });

  it('contains none of the dev-trust tokens in the executable bundle (.mjs/.cjs)', () => {
    const executableFiles = listFiles(DIST).filter((f) => f.endsWith('.mjs') || f.endsWith('.cjs'));
    expect(executableFiles.length, 'expected at least one executable artifact').toBeGreaterThan(0);
    expect(scanForTokens(executableFiles)).toEqual([]);
  });

  it('contains none of the dev-trust tokens in ANY dist artifact (matches dt-guard dist scan)', () => {
    // Stricter, belt-and-suspenders check aligned with the R-14 dt-guard's
    // state-4 dist scan (`ts_dev_trust.rs`), which walks every file under
    // dist/. With the renamed `devCertificateHashes` API field and
    // `sourcemapExcludeSources` in prod, NO dist file (executable, `.d.ts`, or
    // `.map`) should carry either token.
    expect(scanForTokens(listFiles(DIST))).toEqual([]);
  });
});

function scanForTokens(files: readonly string[]): string[] {
  const offenders: string[] = [];
  for (const file of files) {
    const content = readFileSync(file, 'utf8');
    for (const token of FORBIDDEN_TOKENS) {
      if (content.includes(token)) {
        offenders.push(`${file.replace(DIST + '/', '')}: ${token}`);
      }
    }
  }
  return offenders;
}
