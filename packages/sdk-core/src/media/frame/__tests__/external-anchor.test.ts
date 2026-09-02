// File: packages/sdk-core/src/media/frame/__tests__/external-anchor.test.ts
//
// The EXTERNAL ANCHOR gate (story task 8).
//
// Checks our RFC 9605 §4.4 key schedule and the AES-GCM primitive against the
// vendored sframe-wg test vectors at commit 025d568. This is a
// no-correlated-error gate: the expected values come from an IETF working-group
// corpus that neither this codebase nor its Rust reference generator authored,
// so an error shared between our two in-tree implementations cannot hide here.
//
// ---------------------------------------------------------------------------
// WHAT THIS GATE DOES AND DOES NOT COVER
// ---------------------------------------------------------------------------
//
// GATED by these vectors:
//   * HKDF-Extract and HKDF-Expand under SHA-512 (and SHA-256, via the
//     contrast suite), including the exact label bytes and the KID/ciphersuite
//     suffix that go into `info`.
//   * The PRK width and hash — 64 bytes under SHA-512, 32 under SHA-256.
//   * The nonce construction, `salt XOR big-endian(counter)`. RFC 9605's
//     counter is 64-bit and ours is the 32-bit stream sequence, but
//     zero-extending a big-endian 32-bit value into 12 bytes IS the big-endian
//     12-byte encoding of the same integer, so the construction is identical
//     and genuinely exercised (upstream's counter exceeds 16 bits).
//   * AES-256-GCM with a 128-bit tag.
//
// NOT gated by these vectors — and no comfort about them may be drawn from the
// anchor's rigour, however many derivations agree here:
//   * Our SFrame object's clear-header shape. Ours is `key_id(8) || tag(16)`;
//     RFC 9605's is `config || KID || CTR`. Different layouts entirely.
//   * Our AEAD associated data. Upstream's AAD is `sframe_header || metadata`
//     (asserted below so the difference is pinned, not assumed); ours is the
//     publisher region of the v2 frame. These must never be read across.
//   * Our Ed25519 signed range, our detached-tag split, our KEK unwrap, and
//     the entire v2 frame header.
//
// Those five are gated ONLY by the Rust reference generator against an
// independent TypeScript derivation. The external anchor reduces that surface
// by zero rows. The scorecard is therefore: several agreeing derivations on the
// part that was never at risk, and a two-way check on the part that is.

import { describe, expect, it } from 'vitest';

import { bigUintToBytesBE, bytesToHex, concatBytes, hexToBytes } from './hex.js';
import {
  SFRAME_KEY_LABEL,
  SFRAME_SALT_LABEL,
  aesGcmSeal,
  deriveSframeKeys,
  hkdfExpand,
  sframeInfo,
  sframeNonce,
  webCryptoHkdf,
  defaultHkdfSalt,
} from './sframe-key-schedule.js';
import {
  expectationFor,
  loadManifest,
  loadUpstream,
  selectRows,
  type UpstreamSframeRow,
} from './vendored.js';

const manifest = loadManifest();
const rowsBySuite = selectRows(manifest, loadUpstream());

describe('external sframe-wg anchor — manifest and selection', () => {
  it('selects a non-empty row set for every declared suite', () => {
    expect(manifest.cipher_suites.length).toBeGreaterThan(0);
    for (const suite of manifest.cipher_suites) {
      expect(rowsBySuite.get(suite)?.length ?? 0).toBeGreaterThanOrEqual(
        expectationFor(manifest, suite).min_rows,
      );
    }
  });

  it('pins the upstream commit as a full 40-character sha', () => {
    expect(manifest.source.commit).toMatch(/^[0-9a-f]{40}$/);
  });

  it('declares exactly one anchor suite and treats the other as contrast', () => {
    const roles = manifest.expectations.map((e) => e.role);
    expect(roles.filter((r) => r === 'anchor')).toHaveLength(1);
    // The contrast suite exists to make "PRK is 64 bytes under SHA-512"
    // falsifiable rather than self-referential. It is NOT the precedence-ladder
    // fallback: 0x0005 is present at the pinned commit, so we are on the
    // primary branch with nothing degraded.
    expect(roles).toContain('contrast');
  });
});

describe.each(manifest.cipher_suites)('ciphersuite 0x%s', (suite) => {
  const expectation = expectationFor(manifest, suite);
  const rows = rowsBySuite.get(suite) ?? [];

  it.each(rows.map((r, i) => [i, r] as const))(
    'row %i reproduces upstream key schedule',
    async (_i, row: UpstreamSframeRow) => {
      const baseKey = hexToBytes(row.base_key, 'base_key');
      // The KID is taken as an opaque 8-byte big-endian run. It is NOT routed
      // through our key-id packer: upstream's kid (0x123) decomposes under our
      // `sender_id(16) | stream(8) | generation(40)` layout to a sender_id of
      // 0, which our own layout treats as invalid. Routing it through our
      // packer would couple an external gate to our field semantics and break
      // it on a layout change that has nothing to do with the key schedule.
      const kid = bigUintToBytesBE(BigInt(row.kid), 8, 'kid');

      const derived = await deriveSframeKeys({
        hash: expectation.hash,
        baseKey,
        kid,
        cipherSuiteId: suite,
        keyBytes: expectation.key_bytes,
        saltBytes: expectation.salt_bytes,
      });

      // The labels are constructed from the RFC text, then compared with
      // upstream's. Because each label embeds the 8-byte KID, this comparison
      // independently validates our KID encoding: a wrong big-endian width or
      // byte order fails here rather than surfacing later as an opaque key
      // mismatch.
      expect(bytesToHex(sframeInfo(SFRAME_KEY_LABEL, kid, suite))).toBe(row.sframe_key_label);
      expect(bytesToHex(sframeInfo(SFRAME_SALT_LABEL, kid, suite))).toBe(row.sframe_salt_label);

      // PRK width and hash, asserted explicitly. WebCrypto's one-shot HKDF
      // never exposes the PRK, which is why the schedule is implemented as an
      // explicit extract-then-expand.
      expect(derived.secret.length).toBe(expectation.prk_bytes);
      expect(bytesToHex(derived.secret)).toBe(row.sframe_secret);

      expect(derived.key.length).toBe(expectation.key_bytes);
      expect(bytesToHex(derived.key)).toBe(row.sframe_key);
      expect(derived.salt.length).toBe(expectation.salt_bytes);
      expect(bytesToHex(derived.salt)).toBe(row.sframe_salt);

      expect(bytesToHex(sframeNonce(derived.salt, BigInt(row.ctr)))).toBe(row.nonce);
    },
  );

  it.each(rows.map((r, i) => [i, r] as const))(
    'row %i agrees with the platform HKDF',
    async (_i, row: UpstreamSframeRow) => {
      // Cross-check our explicit extract-then-expand against WebCrypto's
      // one-shot HKDF. Free, and it catches an error in our HMAC loop that
      // happened to be self-consistent.
      const baseKey = hexToBytes(row.base_key, 'base_key');
      const kid = bigUintToBytesBE(BigInt(row.kid), 8, 'kid');
      const info = sframeInfo(SFRAME_KEY_LABEL, kid, suite);
      const ours = await hkdfExpand(
        expectation.hash,
        hexToBytes(row.sframe_secret, 'sframe_secret'),
        info,
        expectation.key_bytes,
      );
      const platform = await webCryptoHkdf(
        expectation.hash,
        baseKey,
        defaultHkdfSalt(expectation.hash),
        info,
        expectation.key_bytes,
      );
      expect(bytesToHex(ours)).toBe(bytesToHex(platform));
      expect(bytesToHex(ours)).toBe(row.sframe_key);
    },
  );

  if (expectation.gates_aead) {
    it.each(rows.map((r, i) => [i, r] as const))(
      'row %i reproduces the AEAD ciphertext',
      async (_i, row: UpstreamSframeRow) => {
        const key = hexToBytes(row.sframe_key, 'sframe_key');
        const nonce = hexToBytes(row.nonce, 'nonce');
        const aad = hexToBytes(row.aad, 'aad');
        const metadata = hexToBytes(row.metadata, 'metadata');
        const pt = hexToBytes(row.pt, 'pt');
        const ct = hexToBytes(row.ct, 'ct');

        // Upstream's AAD is its own SFrame header followed by the metadata.
        // Asserted rather than assumed, so the difference from OUR AAD (the v2
        // publisher region) is pinned by a test and cannot be quietly conflated.
        const header = aad.subarray(0, aad.length - metadata.length);
        expect(bytesToHex(aad)).toBe(bytesToHex(concatBytes(header, metadata)));

        const sealed = await aesGcmSeal({
          key,
          nonce,
          aad,
          plaintext: pt,
          tagBytes: expectation.tag_bytes,
        });

        // Combined mode: `ciphertext || tag`. The detached-tag split ADR-0036
        // §2 requires is framing and is deliberately not exercised here.
        expect(sealed.length).toBe(pt.length + expectation.tag_bytes);
        expect(bytesToHex(concatBytes(header, sealed))).toBe(bytesToHex(ct));
      },
    );
  } else {
    it('stops at the key-schedule output and never reaches the AEAD', () => {
      // The AES-128 contrast suite terminates here BY DESIGN. Driving it
      // through seal/open would require an AES-128-capable code path, i.e.
      // building a weak-key acceptance path into our crypto in order to test
      // it. WebCrypto offers no backstop — `importKey('raw', <16 bytes>,
      // 'AES-GCM', ...)` succeeds without complaint — so the refusal has to be
      // ours, and asserting the 16 here documents WHY the row stops rather
      // than leaving it looking like an unfinished test.
      expect(expectation.key_bytes).toBe(16);
      expect(expectation.role).toBe('contrast');
    });

    it("and our AEAD refuses this suite's key length outright", async () => {
      // The other half of the same control: it is not enough that the contrast
      // row declines to call the AEAD — the AEAD must refuse if anyone ever
      // does. Asserted here, against this suite's actual key length, so the
      // refusal is tied to the concrete hazard rather than to a generic
      // "rejects short keys" case that could drift apart from it.
      await expect(
        aesGcmSeal({
          key: new Uint8Array(expectation.key_bytes),
          nonce: new Uint8Array(12),
          aad: new Uint8Array(0),
          plaintext: new Uint8Array(1),
          tagBytes: expectation.tag_bytes,
        }),
      ).rejects.toThrow(/must be 32 bytes/);
    });
  }
});

describe('external sframe-wg anchor — the SHA-512 claim is falsifiable', () => {
  it('the two suites derive PRKs of different widths from the same base key', () => {
    // Without a SHA-256 suite in the corpus, "the PRK is 64 bytes, not 32"
    // would be asserted only against itself. Two suites over one base key make
    // the hash choice observable.
    const widths = manifest.expectations.map((e) => e.prk_bytes);
    expect(new Set(widths).size).toBe(widths.length);
    expect(widths).toContain(64);
    expect(widths).toContain(32);
  });
});
