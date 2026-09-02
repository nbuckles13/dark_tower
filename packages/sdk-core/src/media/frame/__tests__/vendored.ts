// File: packages/sdk-core/src/media/frame/__tests__/vendored.ts
//
// Loader for the vendored external sframe-wg SFrame test vectors and their
// manifest. Reads from the ONE vendored path — never an inlined copy, never a
// convenience subset checked in beside the tests.
//
// The manifest carries the row selector as DATA so that the Rust key-schedule
// gate and this TypeScript gate cannot select different subsets while both
// report green. A selector written twice, once per language, is two
// implementations plus an agreement, and agreements drift.

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

/** The vendored directory, relative to the repository root. */
export const VENDORED_DIR = 'proto/test-vectors/external/sframe-wg';

/** Per-ciphersuite expectations, declared in the manifest rather than in code. */
export interface SuiteExpectation {
  readonly cipher_suite: number;
  readonly name: string;
  readonly hash: 'SHA-256' | 'SHA-512';
  readonly prk_bytes: number;
  readonly key_bytes: number;
  readonly salt_bytes: number;
  readonly tag_bytes: number;
  readonly min_rows: number;
  /**
   * Whether this suite's row may be driven through the AEAD primitive.
   *
   * `false` for the AES-128 contrast suite: it exists to prove the key schedule
   * differs by hash, and running it through seal/open would build an AES-128
   * acceptance path into our crypto in order to test it.
   */
  readonly gates_aead: boolean;
  readonly role: 'anchor' | 'contrast';
}

export interface VendoredManifest {
  readonly source: {
    readonly repo: string;
    readonly commit: string;
    readonly path: string;
    readonly sha256: string;
    readonly bytes: number;
  };
  /**
   * The shared row selector, read by BOTH this harness and the Rust gate
   * (`crates/media-vector-gen/tests/external_sframe_gate.rs`). Flat at the top
   * level because that is the shape the Rust gate reads; the shape is a shared
   * contract, so it changes by agreement, never unilaterally.
   */
  readonly cipher_suites: readonly number[];
  readonly array: string;
  readonly expectations: readonly SuiteExpectation[];
}

/** One upstream `sframe` row, in upstream's own field spelling. */
export interface UpstreamSframeRow {
  readonly cipher_suite: number;
  readonly kid: number;
  readonly ctr: number;
  readonly base_key: string;
  readonly sframe_key_label: string;
  readonly sframe_salt_label: string;
  readonly sframe_secret: string;
  readonly sframe_key: string;
  readonly sframe_salt: string;
  readonly metadata: string;
  readonly nonce: string;
  readonly aad: string;
  readonly pt: string;
  readonly ct: string;
}

/**
 * Walk up to the repository root.
 *
 * Anchored on `pnpm-workspace.yaml` rather than a fixed `../../../../../..`
 * count, so moving this file one directory does not silently resolve to the
 * wrong tree.
 */
function repoRoot(): string {
  let dir = dirname(fileURLToPath(import.meta.url));
  for (let i = 0; i < 12; i += 1) {
    try {
      readFileSync(join(dir, 'pnpm-workspace.yaml'));
      return dir;
    } catch {
      const parent = dirname(dir);
      if (parent === dir) break;
      dir = parent;
    }
  }
  throw new Error('could not locate repository root (no pnpm-workspace.yaml above this file)');
}

function readVendored(file: string): string {
  const path = join(repoRoot(), VENDORED_DIR, file);
  try {
    return readFileSync(path, 'utf8');
  } catch {
    throw new Error(
      `vendored external SFrame vectors missing: ${path}\n` +
        `This gate does not skip when its input is absent — an absent anchor and a passing ` +
        `anchor must not look alike. Land the vendored subset (story task 8, protocol owns ` +
        `${VENDORED_DIR}) or fix the path.`,
    );
  }
}

export function loadManifest(): VendoredManifest {
  return JSON.parse(readVendored('manifest.json')) as VendoredManifest;
}

/** The verbatim upstream file, parsed. */
export function loadUpstream(): Record<string, unknown> {
  return JSON.parse(readVendored('test-vectors.json')) as Record<string, unknown>;
}

/**
 * Rows selected by the manifest, with the anti-vacuity conditions enforced.
 *
 * A filter that selects nothing and reports "no disagreements found" is green
 * in exactly the scenario the anchor exists to prevent, so emptiness is a
 * failure at three levels: an empty suite list, a suite that matches no rows,
 * and a suite that matches fewer rows than declared. The per-suite check is the
 * load-bearing one — "at least one row overall" would still pass if 0x0005
 * vanished upstream and only the 0x0004 contrast survived, which is precisely
 * the degradation this anchor exists to catch.
 */
export function selectRows(
  manifest: VendoredManifest,
  upstream: Record<string, unknown>,
): Map<number, UpstreamSframeRow[]> {
  const { array, cipher_suites: suites } = manifest;

  if (suites.length === 0) {
    throw new Error('manifest cipher_suites is empty — the gate would verify nothing');
  }

  const declared = new Set(manifest.expectations.map((e) => e.cipher_suite));
  for (const suite of suites) {
    if (!declared.has(suite)) {
      throw new Error(`selector names suite ${suite} with no matching expectations entry`);
    }
  }
  for (const expectation of manifest.expectations) {
    if (!suites.includes(expectation.cipher_suite)) {
      throw new Error(
        `expectations declare suite ${expectation.cipher_suite} that the selector does not select`,
      );
    }
  }

  const rows = upstream[array];
  if (!Array.isArray(rows)) {
    throw new Error(`upstream file has no array at key ${JSON.stringify(array)}`);
  }

  const bySuite = new Map<number, UpstreamSframeRow[]>();
  for (const suite of suites) {
    const matching = (rows as UpstreamSframeRow[]).filter((r) => r.cipher_suite === suite);
    const expectation = manifest.expectations.find((e) => e.cipher_suite === suite);
    const minRows = expectation?.min_rows ?? 1;
    if (matching.length < minRows) {
      throw new Error(
        `suite ${suite} (0x${suite.toString(16).padStart(4, '0')}) selected ${matching.length} ` +
          `row(s), manifest requires at least ${minRows}. Upstream may have moved the suite at ` +
          `the pinned commit — this is the degradation the anchor exists to catch, not a ` +
          `condition to relax.`,
      );
    }
    bySuite.set(suite, matching);
  }
  return bySuite;
}

export function expectationFor(manifest: VendoredManifest, suite: number): SuiteExpectation {
  const found = manifest.expectations.find((e) => e.cipher_suite === suite);
  if (!found) throw new Error(`no expectations entry for suite ${suite}`);
  return found;
}
