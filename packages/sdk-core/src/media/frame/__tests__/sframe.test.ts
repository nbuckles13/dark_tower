// File: packages/sdk-core/src/media/frame/__tests__/sframe.test.ts
//
// The AEAD layer's refusals and the detached-tag split, exercised directly.
// These are the paths the vectors reach only incidentally.

import { describe, expect, it } from 'vitest';

import { bytesToHex, hexToBytes } from '../hex.js';
import {
  AES_256_KEY_BYTES,
  aesGcmOpen,
  aesGcmSeal,
  openSframe,
  parseSframe,
  rejectWrongAesKeyLength,
  sealSframe,
  serializeSframe,
  unwrapTransmitKey,
  wrapNonce,
} from '../sframe.js';
import { FrameRejectedError } from '../rejectReason.js';
import { WIRE_CONSTANTS } from '../wireConstants.js';

const KEY = new Uint8Array(32).fill(0x20);
const NONCE = new Uint8Array(12).fill(0x01);
const KID = hexToBytes('0102030405060708');

describe('the AES-256-only refusal', () => {
  it('rejects every length except 32', () => {
    // WebCrypto ACCEPTS a 16-byte AES-GCM key — verified in Chrome and in Node —
    // so there is no platform backstop underneath this. Without the refusal the
    // vendored AES-128 contrast row could be driven through this module, which
    // would mean building a weak-key acceptance path into shipping crypto in
    // order to test that we do not have one.
    for (const len of [0, 15, 16, 24, 31, 33, 64]) {
      expect(() => rejectWrongAesKeyLength(len, 'test'), `${len}-byte key`).toThrow(
        /must be exactly 32 bytes/,
      );
    }
  });

  it('accepts exactly 32, or the check is vacuous', () => {
    expect(() => rejectWrongAesKeyLength(AES_256_KEY_BYTES, 'test')).not.toThrow();
  });

  it('applies on SEAL', async () => {
    await expect(
      aesGcmSeal({ key: new Uint8Array(16), nonce: NONCE, aad: new Uint8Array(0), plaintext: KEY }),
    ).rejects.toThrow(/must be exactly 32 bytes/);
  });

  it('applies on OPEN — the receive path, which is the one an attacker reaches', async () => {
    await expect(
      aesGcmOpen({
        key: new Uint8Array(16),
        nonce: NONCE,
        aad: new Uint8Array(0),
        ciphertextWithTag: new Uint8Array(32),
      }),
    ).rejects.toThrow(/must be exactly 32 bytes/);
  });

  it('applies on KEK UNWRAP', async () => {
    await expect(
      unwrapTransmitKey({
        kek: new Uint8Array(16),
        keyId: KID,
        wrappedKeyWithTag: new Uint8Array(48),
      }),
    ).rejects.toThrow(/must be exactly 32 bytes/);
  });
});

describe('AES-GCM round trip', () => {
  it('opens what it sealed', async () => {
    const aad = Uint8Array.of(1, 2, 3);
    const pt = new TextEncoder().encode('hello');
    const sealed = await aesGcmSeal({ key: KEY, nonce: NONCE, aad, plaintext: pt });
    expect(sealed.length).toBe(pt.length + WIRE_CONSTANTS.aead_tag_bytes);
    const opened = await aesGcmOpen({ key: KEY, nonce: NONCE, aad, ciphertextWithTag: sealed });
    expect(opened).not.toBeNull();
    expect(bytesToHex(opened!)).toBe(bytesToHex(pt));
  });

  it('returns null rather than throwing when the tag fails', async () => {
    // `null`, not an exception, because the SAME GCM failure means different
    // things at different call sites — `decrypt_failed` for the payload,
    // `unwrap_failed` for the KEK unwrap — with opposite remedies. The primitive
    // must not pick one.
    const sealed = await aesGcmSeal({
      key: KEY,
      nonce: NONCE,
      aad: new Uint8Array(0),
      plaintext: Uint8Array.of(9),
    });
    const opened = await aesGcmOpen({
      key: new Uint8Array(32).fill(0x21),
      nonce: NONCE,
      aad: new Uint8Array(0),
      ciphertextWithTag: sealed,
    });
    expect(opened).toBeNull();
  });

  it('returns null when the AAD differs', async () => {
    const sealed = await aesGcmSeal({
      key: KEY,
      nonce: NONCE,
      aad: Uint8Array.of(1),
      plaintext: Uint8Array.of(9),
    });
    expect(
      await aesGcmOpen({
        key: KEY,
        nonce: NONCE,
        aad: Uint8Array.of(2),
        ciphertextWithTag: sealed,
      }),
    ).toBeNull();
  });
});

describe('the SFrame object and its detached tag', () => {
  it('round-trips through seal, serialize, parse, open', async () => {
    const aad = Uint8Array.of(7, 7);
    const pt = new TextEncoder().encode('frame payload');
    const obj = await sealSframe({ key: KEY, nonce: NONCE, aad, plaintext: pt, keyId: KID });

    // The tag is DETACHED into the clear header — our layout is
    // `key_id(8) || tag(16) || ciphertext`, where RFC 9605's is
    // `config || KID || CTR` with a TRAILING tag.
    expect(obj.keyId.length).toBe(WIRE_CONSTANTS.key_id_bytes);
    expect(obj.tag.length).toBe(WIRE_CONSTANTS.aead_tag_bytes);
    expect(obj.ciphertext.length).toBe(pt.length);

    const wire = serializeSframe(obj);
    expect(wire.length).toBe(8 + 16 + pt.length);
    const parsed = parseSframe(wire);
    expect(bytesToHex(parsed.keyId)).toBe(bytesToHex(KID));
    expect(bytesToHex(parsed.tag)).toBe(bytesToHex(obj.tag));

    const opened = await openSframe({ key: KEY, nonce: NONCE, aad, object: parsed });
    expect(bytesToHex(opened!)).toBe(bytesToHex(pt));
  });

  it('rejects a key id of the wrong width on seal', async () => {
    await expect(
      sealSframe({
        key: KEY,
        nonce: NONCE,
        aad: new Uint8Array(0),
        plaintext: Uint8Array.of(1),
        keyId: new Uint8Array(7),
      }),
    ).rejects.toThrow(/8 bytes/);
  });

  it('rejects a payload too short to hold the clear header', () => {
    // A codec-layer `truncated`, not a crypto failure: there is no object here to
    // fail to authenticate.
    try {
      parseSframe(new Uint8Array(23));
      expect.unreachable('should have rejected');
    } catch (err) {
      expect(err).toBeInstanceOf(FrameRejectedError);
      expect((err as FrameRejectedError).rejectReason).toBe('truncated');
    }
  });

  it('accepts an empty ciphertext at the exact clear-header boundary', () => {
    const parsed = parseSframe(new Uint8Array(24));
    expect(parsed.ciphertext.length).toBe(0);
  });
});

describe('the KEK wrap', () => {
  it('derives the nonce as four zero bytes then the key id', () => {
    // ADR-0036 §4 pins this. Zero-PREFIX rather than zero-suffix so the codebase
    // carries ONE padding rule — SFrame's own nonce right-aligns a big-endian
    // value identically.
    expect(bytesToHex(wrapNonce(KID))).toBe('000000000102030405060708');
  });

  it('rejects a key id of the wrong width', () => {
    expect(() => wrapNonce(new Uint8Array(4))).toThrow(/8 bytes/);
    expect(
      unwrapTransmitKey({
        kek: KEY,
        keyId: new Uint8Array(4),
        wrappedKeyWithTag: new Uint8Array(48),
      }),
    ).rejects.toThrow(/8 bytes/);
  });

  it('unwraps a key bound to this key id, and refuses one bound to another', async () => {
    // The binding IS the AEAD: there is no bound-key-id field on the wire, so a
    // wrap bound elsewhere simply does not open under this frame's key id.
    const transmitKey = new Uint8Array(32).fill(0x44);
    const other = hexToBytes('0102030405060709');
    const wrapped = await aesGcmSeal({
      key: KEY,
      nonce: wrapNonce(KID),
      aad: KID,
      plaintext: transmitKey,
    });

    const ok = await unwrapTransmitKey({ kek: KEY, keyId: KID, wrappedKeyWithTag: wrapped });
    expect(bytesToHex(ok!)).toBe(bytesToHex(transmitKey));

    // Same bytes, different key id: nonce and AAD both change, so it fails.
    expect(
      await unwrapTransmitKey({ kek: KEY, keyId: other, wrappedKeyWithTag: wrapped }),
    ).toBeNull();
  });

  it('returns null under the wrong KEK — indistinguishable from a mis-bound wrap', async () => {
    // The property that forces `wrap_key_id_mismatch` to be decided by receiver
    // state rather than by anything observable here: both failures are one bit.
    const wrapped = await aesGcmSeal({
      key: KEY,
      nonce: wrapNonce(KID),
      aad: KID,
      plaintext: new Uint8Array(32).fill(0x44),
    });
    expect(
      await unwrapTransmitKey({
        kek: new Uint8Array(32).fill(0x99),
        keyId: KID,
        wrappedKeyWithTag: wrapped,
      }),
    ).toBeNull();
  });

  it('refuses an unwrapped key of the wrong width', async () => {
    // A KEK that opens but yields a wrong-width key is a protocol violation, not
    // a wrong key — refused rather than passed on to the key schedule.
    const wrapped = await aesGcmSeal({
      key: KEY,
      nonce: wrapNonce(KID),
      aad: KID,
      plaintext: new Uint8Array(16).fill(0x44),
    });
    await expect(
      unwrapTransmitKey({ kek: KEY, keyId: KID, wrappedKeyWithTag: wrapped }),
    ).rejects.toThrow(/expected 32/);
  });
});
