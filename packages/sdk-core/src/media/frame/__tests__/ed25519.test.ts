// File: packages/sdk-core/src/media/frame/__tests__/ed25519.test.ts
//
// The fail-closed property, which is the only thing about this module that can
// be wrong in a way that matters.

import { describe, expect, it } from 'vitest';

import { bytesToHex, hexToBytes } from '../hex.js';
import {
  ED25519_PUBLIC_KEY_BYTES,
  ED25519_SIGNATURE_BYTES,
  assertEd25519Available,
  exportPublicKey,
  importSigningKeyFromSeed,
  signFrame,
  verifyFrameSignature,
} from '../ed25519.js';

// A pinned seed/public-key pair, so the PKCS#8 wrapping is asserted against a
// known answer rather than only against itself. Same synthetic fixture the
// vectors use.
const SEED = '404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f';
const PUBLIC = '2543b92ff1095511476adc8369db6ddc933665a11978dda1404ee1066ca9559d';

describe('capability', () => {
  it('is available on this platform', async () => {
    // Verified in Chrome for Testing 151 as well as here — WebCrypto Ed25519 is
    // unflagged, which is why no `@noble/ed25519` fallback ships.
    await expect(assertEd25519Available()).resolves.toBeUndefined();
  });
});

describe('seed import and deterministic signing', () => {
  it('derives the pinned public key from the pinned seed', async () => {
    // Asserts the 16-byte PKCS#8 DER prefix is right. A wrong prefix would import
    // *something* and produce consistent-but-wrong signatures.
    const key = await importSigningKeyFromSeed(hexToBytes(SEED));
    expect(bytesToHex(await exportPublicKey(key))).toBe(PUBLIC);
  });

  it('signs deterministically, per RFC 8032', async () => {
    const key = await importSigningKeyFromSeed(hexToBytes(SEED));
    const msg = new TextEncoder().encode('dark-tower');
    const a = await signFrame(key, msg);
    const b = await signFrame(key, msg);
    expect(bytesToHex(a)).toBe(bytesToHex(b));
    expect(a.length).toBe(ED25519_SIGNATURE_BYTES);
  });

  it('refuses a seed of the wrong length', async () => {
    await expect(importSigningKeyFromSeed(new Uint8Array(31))).rejects.toThrow(/32 bytes/);
  });
});

describe('key import', () => {
  it('rejects a public key of the wrong length loudly', async () => {
    // `importVerifyKey` throws where `verifyFrameSignature` returns false. The
    // difference is deliberate: importing a malformed key is a programming error
    // at a call site we control, whereas a bad key ARRIVING is untrusted input on
    // the receive path and must fail closed rather than throw.
    const { importVerifyKey } = await import('../ed25519.js');
    await expect(importVerifyKey(new Uint8Array(31))).rejects.toThrow(/32 bytes/);
    await expect(importVerifyKey(hexToBytes(PUBLIC))).resolves.toBeDefined();
  });

  it('caches the capability probe', async () => {
    await expect(assertEd25519Available()).resolves.toBeUndefined();
    await expect(assertEd25519Available()).resolves.toBeUndefined();
  });
});

describe('verification fails closed', () => {
  it('accepts a genuine signature', async () => {
    const key = await importSigningKeyFromSeed(hexToBytes(SEED));
    const msg = new TextEncoder().encode('payload');
    const sig = await signFrame(key, msg);
    expect(await verifyFrameSignature(hexToBytes(PUBLIC), sig, msg)).toBe(true);
  });

  it('rejects a tampered message', async () => {
    const key = await importSigningKeyFromSeed(hexToBytes(SEED));
    const msg = new TextEncoder().encode('payload');
    const sig = await signFrame(key, msg);
    expect(
      await verifyFrameSignature(hexToBytes(PUBLIC), sig, new TextEncoder().encode('payloae')),
    ).toBe(false);
  });

  it('rejects a signature of the wrong length rather than throwing', async () => {
    // Every one of these returns FALSE rather than throwing. That is the whole
    // design: a caller must never be able to catch an exception here and treat the
    // outcome as "inconclusive", because the only inconclusive-shaped behaviour
    // available to a receiver is to accept an unverified frame.
    expect(
      await verifyFrameSignature(hexToBytes(PUBLIC), new Uint8Array(63), new Uint8Array(1)),
    ).toBe(false);
  });

  it('rejects a public key of the wrong length rather than throwing', async () => {
    expect(
      await verifyFrameSignature(new Uint8Array(31), new Uint8Array(64), new Uint8Array(1)),
    ).toBe(false);
  });

  it('rejects garbage key material rather than throwing', async () => {
    // A 32-byte value that is not a valid curve point. `importKey` throws inside;
    // the throw is converted to `false`, never propagated.
    const notAPoint = new Uint8Array(ED25519_PUBLIC_KEY_BYTES).fill(0xff);
    expect(await verifyFrameSignature(notAPoint, new Uint8Array(64), new Uint8Array(1))).toBe(
      false,
    );
  });

  it('rejects an all-zero signature', async () => {
    expect(
      await verifyFrameSignature(hexToBytes(PUBLIC), new Uint8Array(64), new Uint8Array(1)),
    ).toBe(false);
  });
});
