// File: packages/sdk-core/src/media/frame/__tests__/keyId.test.ts
//
// The KID packer's refusals. Every one of these is a nonce-collision hazard, not
// a cosmetic range check: two senders sharing a key id share a (key, nonce) pair,
// and under AES-GCM a repeated pair leaks the authentication subkey.

import { describe, expect, it } from 'vitest';

import { bytesToHex, hexToBytes } from '../hex.js';
import {
  GENERATION_EXCLUSIVE_BOUND,
  KeyIdRangeError,
  SENDER_ID_EXCLUSIVE_BOUND,
  STREAM_EXCLUSIVE_BOUND,
  keyIdCacheKey,
  packKeyId,
  unpackKeyId,
} from '../keyId.js';

describe('packKeyId rejects every out-of-range field', () => {
  it('rejects sender_id above 65535 rather than truncating', () => {
    // MC range-checks this to 65535, and the client checks it AGAIN. A client that
    // trusted MC's check would truncate silently on a compromised or buggy
    // controller — and 65536 masks to 0, colliding with every other truncation.
    expect(() =>
      packKeyId({ senderId: SENDER_ID_EXCLUSIVE_BOUND, stream: 1n, generation: 1n }),
    ).toThrow(KeyIdRangeError);
    expect(() => packKeyId({ senderId: 65536n, stream: 1n, generation: 1n })).toThrow(
      /does not fit in 16 bits/,
    );
  });

  it('rejects sender_id 0 as reserved-invalid', () => {
    // Mirrors `crates/media-vector-gen/src/kid.rs::pack`. A shared zero is N
    // colliding key ids — the same hazard as truncation by a different route.
    expect(() => packKeyId({ senderId: 0n, stream: 1n, generation: 1n })).toThrow(
      /reserved-invalid/,
    );
  });

  it('rejects stream above 255', () => {
    expect(() =>
      packKeyId({ senderId: 1n, stream: STREAM_EXCLUSIVE_BOUND, generation: 1n }),
    ).toThrow(/does not fit in 8 bits/);
  });

  it('rejects generation at 2^40', () => {
    // Out of range for generation means beyond 2^40-1. The ceiling is
    // UNREACHABLE in practice rather than unchecked — at one rotation per video
    // group it cannot be driven to wrap within any meeting lifetime — but the
    // packer still refuses rather than overflowing into the stream field.
    expect(() =>
      packKeyId({ senderId: 1n, stream: 1n, generation: GENERATION_EXCLUSIVE_BOUND }),
    ).toThrow(/does not fit in 40 bits/);
  });

  it('rejects negative components', () => {
    expect(() => packKeyId({ senderId: -1n, stream: 0n, generation: 0n })).toThrow(/negative/);
  });

  it('names the offending field so a caller can act on it', () => {
    try {
      packKeyId({ senderId: 1n, stream: 999n, generation: 0n });
      expect.unreachable('should have thrown');
    } catch (err) {
      expect((err as KeyIdRangeError).field).toBe('stream');
    }
  });
});

describe('packKeyId round-trips at the top of every field', () => {
  it('survives encode -> decode with every field at its maximum', () => {
    const parts = {
      senderId: SENDER_ID_EXCLUSIVE_BOUND - 1n,
      stream: STREAM_EXCLUSIVE_BOUND - 1n,
      generation: GENERATION_EXCLUSIVE_BOUND - 1n,
    };
    const packed = packKeyId(parts);
    expect(bytesToHex(packed)).toBe('ffffffffffffffff');
    expect(unpackKeyId(packed)).toEqual(parts);
  });

  it('places each field at the right bit offset', () => {
    // Distinguishable values per field, so a shift error cannot pass by symmetry.
    const packed = packKeyId({ senderId: 0x0102n, stream: 0x03n, generation: 0x0405060708n });
    expect(bytesToHex(packed)).toBe('0102030405060708');
    const parts = unpackKeyId(packed);
    expect(parts.senderId).toBe(0x0102n);
    expect(parts.stream).toBe(0x03n);
    expect(parts.generation).toBe(0x0405060708n);
  });

  it('handles a 40-bit generation that a 32-bit shift would mangle', () => {
    // The reason this module is bigint-only: JS bitwise operators coerce to 32
    // bits, so `1 << 40` is 256. A number-typed implementation cannot represent
    // this value's placement at all.
    const generation = (1n << 39n) + 1n;
    const parts = unpackKeyId(packKeyId({ senderId: 1n, stream: 0n, generation }));
    expect(parts.generation).toBe(generation);
  });
});

describe('unpackKeyId is for lookup only', () => {
  it('accepts sender_id 0 on receive, because rejection is a pack-time rule', () => {
    // A frame claiming sender 0 must reach the roster lookup and be dropped as
    // `no_roster_entry`, which is more informative than a parse failure. This is
    // also what lets the external sframe-wg vectors, whose kid decomposes to
    // sender 0, be read without routing them through our packer.
    const parts = unpackKeyId(hexToBytes('0000000000000123'));
    expect(parts.senderId).toBe(0n);
  });

  it('rejects a key id that is not 8 bytes', () => {
    expect(() => unpackKeyId(new Uint8Array(7))).toThrow(/8 bytes/);
  });
});

describe('keyIdCacheKey derives from the received bytes', () => {
  it('distinguishes key ids that decompose to the same fields', () => {
    // Derived from bytes rather than from decoded fields, so two distinct wire
    // key ids can never collide in a cache however they decompose.
    expect(keyIdCacheKey(hexToBytes('0102030405060708'))).not.toBe(
      keyIdCacheKey(hexToBytes('0102030405060709')),
    );
  });
});
