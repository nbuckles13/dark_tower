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
import { TONE_RUNTIME_TOKENS } from './toneMarkers.js';

const PKG_ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const REPO_ROOT = join(PKG_ROOT, '..', '..');
const DIST = join(PKG_ROOT, 'dist');

// Both the browser trust-option key AND the build-time gate flag must be
// absent from the prod build (the gate must be substituted away, not left
// live). NOTE: the prod build here injects NO concrete dev fingerprint VALUE
// (the value only ever flows from runtime config into `connect`, never into the
// build). If a future test ever builds with a concrete fingerprint fixture,
// add that literal to this list so it cannot slip through.
// The frame-vector SSoT must never reach the shipped bundle. It is 94 KB, it
// carries a `_non_production` banner, and every one of its rows holds `kek_hex`,
// `transmit_key_hex` and `identity_private_seed_hex`. Production reads those
// numbers through the RENDERED `wireConstants.ts` — ~15 integers, no `node:fs`,
// no fixtures — so an appearance here would mean someone imported the JSON from
// `src/`, and `vite.config.ts` externalises only `/^@opentelemetry\//`, so
// anything imported from `src/` IS bundled.
//
// FORBID THE FIXTURE VALUES, NOT THE FILE PATH OR ITS FIELD NAMES, and derive the
// values FROM the SSoT rather than copying them. Two lessons compressed into one
// list:
//
//   * Assert on the thing, never on a name for the thing. An earlier version
//     banned `frame-v2.vectors` (the path) and `identity_private_seed_hex` (a
//     field name), both of which `vite-plugin-dts` carries into `.d.ts` via
//     JSDoc — and g12 positively REQUIRES the path string in `frameCodec.ts` /
//     `sframe.ts`, so a path-token ban put two correct controls in unsatisfiable
//     conflict. The values below are data; they cannot appear in prose.
//   * A hand-copied SUBSET of the thing is still a name for it (@dry-reviewer).
//     An earlier list carried three of the four distinct key-material values by
//     hand and silently omitted the fourth — incomplete on the day it landed,
//     and it would go fully stale and green if the generator's fixture pattern
//     ever changed. Deriving from `key_material_fields` across every row is
//     exhaustive BY CONSTRUCTION and cannot drift from the SSoT.
function forbiddenFixtureValues(): string[] {
  const vectorsPath = join(REPO_ROOT, 'proto/test-vectors/frame-v2.vectors.json');
  const ssot = JSON.parse(readFileSync(vectorsPath, 'utf8')) as {
    key_material_fields: string[];
    vectors: { crypto: Record<string, string> }[];
  };
  const values = new Set<string>();
  for (const row of ssot.vectors) {
    for (const field of ssot.key_material_fields) {
      const v = row.crypto[field];
      if (typeof v === 'string' && v.length > 0) values.add(v);
    }
  }
  if (values.size === 0) {
    throw new Error(
      'no key-material fixture values found in the SSoT — the bundle scan would forbid nothing',
    );
  }
  return [...values];
}

const FORBIDDEN_TOKENS = [
  'serverCertificateHashes',
  '__DEV_TRUST_FINGERPRINT__',
  ...forbiddenFixtureValues(),
];

// Story 2 R-7 (@security A1): the library build's `__DT_TEST_TONE__` is a hard
// `false`, so the published sdk-core's RUNTIME carries neither the gate nor the
// tone synthesis nor the `test_tone` mode token. Each marker is a string or DOM
// method name the minifier cannot rename, and none appears in production code
// outside that gate. The markers are `TONE_RUNTIME_TOKENS` (`./toneMarkers.ts`);
// the app bundle's scan asserts its FORBIDDEN list is a superset of them and its
// positive-control build proves each one real.
//
// Scanned in EVERY dist artifact except type declarations (`.d.ts`, `.d.mts`,
// `.d.cts` and their maps). The declarations legitimately carry the public
// `MediaCaptureSourceMode = 'microphone' | 'test_tone'` type and the doc comments
// naming the gate; a type is not code, and forbidding it would forbid the SDK
// from typing its own metric label. What must never ship is the synthesis.

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

  it('carries no test-tone synthesis, gate or mode token outside type declarations', () => {
    // DENY-list of declaration files, not an allow-list of runtime ones
    // (@security): a future chunk extension is scanned by default.
    const isDeclaration = (f: string): boolean => /\.d\.[cm]?ts(\.map)?$/.test(f);
    const scanned = listFiles(DIST).filter((f) => !isDeclaration(f));
    // VACUITY CONTROL, with its own message: the scan must have read at least one
    // runtime file of EACH module format, or "no marker found" proves nothing.
    expect(
      scanned.some((f) => f.endsWith('.mjs')) && scanned.some((f) => f.endsWith('.cjs')),
      'SCAN VACUOUS: no .mjs and .cjs runtime artifact was scanned for the tone markers',
    ).toBe(true);
    expect(scanForTokens(scanned, TONE_RUNTIME_TOKENS)).toEqual([]);
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

function scanForTokens(
  files: readonly string[],
  tokens: readonly string[] = FORBIDDEN_TOKENS,
): string[] {
  const offenders: string[] = [];
  for (const file of files) {
    const content = readFileSync(file, 'utf8');
    for (const token of tokens) {
      if (content.includes(token)) {
        offenders.push(`${file.replace(DIST + '/', '')}: ${token}`);
      }
    }
  }
  return offenders;
}
