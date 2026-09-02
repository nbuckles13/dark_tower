// File: packages/sdk-core/src/media/frame/__tests__/vendored.test.ts
//
// Self-test for the external anchor's ANTI-VACUITY conditions.
//
// `selectRows` is what stands between "the external gate verified our key
// schedule" and "the external gate iterated zero rows and reported success".
// Those two outcomes are indistinguishable from the outside, so each rejection
// path is driven here with a synthetic manifest.
//
// This exists for the same reason the reject paths in the anchor gate do: a
// check whose failure branch is never exercised is a check that passes on the
// day it stops working. Asserting only the happy path would leave every
// rejection below free to rot green under a later refactor.

import { describe, expect, it } from 'vitest';

import { selectRows, type VendoredManifest } from './vendored.js';

const UPSTREAM = {
  sframe: [
    { cipher_suite: 4, kid: 291 },
    { cipher_suite: 5, kid: 291 },
  ],
};

function manifest(overrides: Partial<VendoredManifest> = {}): VendoredManifest {
  return {
    source: { repo: 'r', commit: 'c', path: 'p', sha256: 's', bytes: 1 },
    array: 'sframe',
    cipher_suites: [4, 5],
    expectations: [
      {
        cipher_suite: 4,
        name: 'AES_128_GCM_SHA256_128',
        hash: 'SHA-256',
        prk_bytes: 32,
        key_bytes: 16,
        salt_bytes: 12,
        tag_bytes: 16,
        min_rows: 1,
        gates_aead: false,
        role: 'contrast',
      },
      {
        cipher_suite: 5,
        name: 'AES_256_GCM_SHA512_128',
        hash: 'SHA-512',
        prk_bytes: 64,
        key_bytes: 32,
        salt_bytes: 12,
        tag_bytes: 16,
        min_rows: 1,
        gates_aead: true,
        role: 'anchor',
      },
    ],
    ...overrides,
  };
}

describe('selectRows anti-vacuity', () => {
  it('accepts the real shape and returns rows per suite', () => {
    const selected = selectRows(manifest(), UPSTREAM);
    expect(selected.get(5)).toHaveLength(1);
    expect(selected.get(4)).toHaveLength(1);
  });

  it('rejects an empty selector rather than verifying nothing', () => {
    expect(() => selectRows(manifest({ cipher_suites: [], expectations: [] }), UPSTREAM)).toThrow(
      /cipher_suites is empty/,
    );
  });

  it('rejects a suite that matches no upstream rows', () => {
    // The degradation this anchor exists to catch: upstream moves or renames a
    // suite at the pinned commit and the gate silently narrows.
    expect(() => selectRows(manifest(), { sframe: [{ cipher_suite: 4, kid: 291 }] })).toThrow(
      /suite 5 \(0x0005\) selected 0 row\(s\)/,
    );
  });

  it('rejects PER-SUITE, not merely "at least one row overall"', () => {
    // The aggregate form would pass here: 0x0004 survives, so a global
    // "found some rows" check is satisfied while the suite we actually ship
    // has vanished. This is the specific weakness the per-suite form closes.
    const upstreamMissingPrimary = { sframe: [{ cipher_suite: 4, kid: 291 }] };
    const rows = upstreamMissingPrimary.sframe.length;
    expect(rows).toBeGreaterThan(0);
    expect(() => selectRows(manifest(), upstreamMissingPrimary)).toThrow(/suite 5/);
  });

  it('rejects a suite matching fewer rows than the manifest requires', () => {
    const m = manifest();
    const raised = m.expectations.map((e) =>
      e.cipher_suite === 5 ? { ...e, min_rows: 2 } : e,
    ) as VendoredManifest['expectations'];
    expect(() => selectRows(manifest({ expectations: raised }), UPSTREAM)).toThrow(
      /requires at least 2/,
    );
  });

  it('rejects a selector naming a suite with no expectations entry', () => {
    expect(() => selectRows(manifest({ cipher_suites: [4, 5, 6] }), UPSTREAM)).toThrow(
      /suite 6 with no matching expectations/,
    );
  });

  it('rejects expectations declaring a suite the selector does not select', () => {
    expect(() => selectRows(manifest({ cipher_suites: [5] }), UPSTREAM)).toThrow(
      /declare suite 4 that the selector does not select/,
    );
  });

  it('rejects an upstream file with no array at the selected key', () => {
    expect(() => selectRows(manifest({ array: 'nope' }), UPSTREAM)).toThrow(
      /no array at key "nope"/,
    );
  });
});
