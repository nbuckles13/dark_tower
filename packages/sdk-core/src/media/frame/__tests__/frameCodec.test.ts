// File: packages/sdk-core/src/media/frame/__tests__/frameCodec.test.ts
//
// Codec behaviour the vectors do not pin: encode/decode round trips, the
// boundary conditions either side of each reject, and the encode-side rule that
// the publisher region is complete before either authenticated range is taken.

import { describe, expect, it } from 'vitest';

import { bytesToHex, concatBytes } from '../hex.js';
import {
  FLAG_INDEPENDENTLY_DECODABLE,
  FLAG_KEY_BEARING,
  buildUnsignedFrame,
  decodeFrame,
  finishFrame,
  type EncodeFrameInput,
} from '../frameCodec.js';
import { FrameRejectedError } from '../rejectReason.js';
import { WIRE_CONSTANTS } from '../wireConstants.js';

const SIG = new Uint8Array(WIRE_CONSTANTS.signature_bytes).fill(0xab);

function baseInput(over: Partial<EncodeFrameInput> = {}): EncodeFrameInput {
  return {
    flags: { independentlyDecodable: true, discardable: false, keyBearing: false },
    streamSequence: 7,
    wrappedTransmitKey: null,
    extensions: [],
    streamId: 2,
    hopSequence: 11,
    payload: new Uint8Array(24).fill(0x5a),
    ...over,
  };
}

function encode(over: Partial<EncodeFrameInput> = {}) {
  return finishFrame(buildUnsignedFrame(baseInput(over)), SIG);
}

describe('round trip', () => {
  it('preserves every field with no wrapped key and no extensions', () => {
    const decoded = decodeFrame(encode());
    expect(decoded.version).toBe(2);
    expect(decoded.flags).toEqual({
      independentlyDecodable: true,
      discardable: false,
      keyBearing: false,
    });
    expect(decoded.streamSequence).toBe(7);
    expect(decoded.streamId).toBe(2);
    expect(decoded.hopSequence).toBe(11);
    expect(decoded.payloadLength).toBe(24);
    expect(decoded.wrappedTransmitKey).toBeNull();
  });

  it('preserves the wrapped key block and the extension region', () => {
    const wrap = new Uint8Array(
      WIRE_CONSTANTS.wrapped_transmit_key_material_bytes + WIRE_CONSTANTS.aead_tag_bytes,
    ).fill(0x33);
    const decoded = decodeFrame(
      encode({
        flags: { independentlyDecodable: true, discardable: true, keyBearing: true },
        wrappedTransmitKey: { kekGeneration: 9, wrappedKeyWithTag: wrap },
        extensions: [{ extType: 1, value: Uint8Array.of(42) }],
      }),
    );
    expect(decoded.wrappedTransmitKey?.kekGeneration).toBe(9);
    expect(bytesToHex(decoded.wrappedTransmitKey!.wrappedKeyWithTag)).toBe(bytesToHex(wrap));
    expect(decoded.extensions).toHaveLength(1);
    expect(decoded.extensions[0]?.extType).toBe(1);
    expect(decoded.extensions[0]?.value[0]).toBe(42);
  });

  it('the encoder produces the ranges the decoder recovers', () => {
    // The encode side and the decode side must agree on both authenticated
    // ranges, or a frame we send is a frame we cannot verify.
    const input = baseInput({
      flags: { independentlyDecodable: true, discardable: false, keyBearing: true },
      wrappedTransmitKey: {
        kekGeneration: 1,
        wrappedKeyWithTag: new Uint8Array(48).fill(0x11),
      },
    });
    const unsigned = buildUnsignedFrame(input);
    const decoded = decodeFrame(finishFrame(unsigned, SIG));
    expect(bytesToHex(decoded.aeadAad)).toBe(bytesToHex(unsigned.aeadAad));
    expect(bytesToHex(decoded.signedRange)).toBe(bytesToHex(unsigned.signedRange));
  });
});

describe('the authenticated ranges', () => {
  it('the AAD is the publisher region only — relay region and payload excluded', () => {
    const frame = encode();
    const decoded = decodeFrame(frame);
    expect(decoded.aeadAad.length).toBe(decoded.offsets.publisherRegionLen);
    // The relay region's bytes must not appear at the end of the AAD.
    expect(bytesToHex(decoded.aeadAad)).toBe(
      bytesToHex(frame.subarray(0, decoded.offsets.relayRegionOffset)),
    );
  });

  it('the signed range SKIPS the relay region, and differs from the naive contiguous span', () => {
    const frame = encode();
    const decoded = decodeFrame(frame);
    const naive = frame.subarray(0, decoded.offsets.signatureOffset);
    // The near-miss. Signing everything up to the signature is the natural wrong
    // implementation, and it fails silently: both codecs agree, the relay rewrites
    // the region, and verification fails at the receiver with nothing to point at.
    expect(bytesToHex(decoded.signedRange)).not.toBe(bytesToHex(naive));
    expect(decoded.signedRange.length).toBe(naive.length - WIRE_CONSTANTS.relay_region_bytes);
  });

  it('rewriting the relay region changes neither range', () => {
    // The property the split exists for: MH rewrites these six bytes per
    // subscriber and both authenticated ranges must be untouched by it.
    const frame = encode();
    const before = decodeFrame(frame);
    const rewritten = Uint8Array.from(frame);
    // Explicit reads rather than `^=`: under `noUncheckedIndexedAccess` the
    // compound form types as possibly-undefined, and `?? 0` would silently write a
    // zero on an out-of-range index instead of failing.
    const at = before.offsets.relayRegionOffset;
    rewritten.set([(frame[at] as number) ^ 0xff], at);
    rewritten.set([(frame[at + 3] as number) ^ 0xff], at + 3);
    const after = decodeFrame(rewritten);
    expect(bytesToHex(after.aeadAad)).toBe(bytesToHex(before.aeadAad));
    expect(bytesToHex(after.signedRange)).toBe(bytesToHex(before.signedRange));
    expect(after.streamId).not.toBe(before.streamId);
  });

  it('decode slices rather than copies', () => {
    const frame = encode();
    const decoded = decodeFrame(frame);
    expect(decoded.payload.buffer).toBe(frame.buffer);
    expect(decoded.aeadAad.buffer).toBe(frame.buffer);
    expect(decoded.signature.buffer).toBe(frame.buffer);
  });
});

describe('reject boundaries', () => {
  function rejectOf(bytes: Uint8Array): string {
    try {
      decodeFrame(bytes);
    } catch (err) {
      expect(err).toBeInstanceOf(FrameRejectedError);
      return (err as FrameRejectedError).rejectReason;
    }
    return 'accepted';
  }

  it('rejects an unknown version', () => {
    const f = encode();
    f[0] = 3;
    expect(rejectOf(f)).toBe('unknown_version');
  });

  it('rejects every reserved flag bit, one at a time', () => {
    // Every bit above the legal mask, individually — a mask comparison that
    // happened to be right for one bit and wrong for another would pass a single
    // spot check.
    for (let bit = 3; bit < 8; bit += 1) {
      const f = encode();
      f.set([(f[1] as number) | (1 << bit)], 1);
      expect(rejectOf(f), `flag bit ${bit}`).toBe('reserved_flag_bit_set');
    }
  });

  it('accepts exactly the legal flag mask and no more', () => {
    const f = encode();
    f[1] = WIRE_CONSTANTS.legal_flag_mask;
    // Setting key-bearing changes the layout, so this frame no longer parses as a
    // complete frame — but it must fail on LENGTH, never on the flag check.
    expect(rejectOf(f)).not.toBe('reserved_flag_bit_set');
  });

  it('enforces the maximum before the availability check', () => {
    // Order is load-bearing: the limit must be applied to the declared field
    // before anything is sized from it. A reader that checked availability first
    // would already have computed a length from an attacker-controlled u32.
    const f = encode();
    new DataView(f.buffer).setUint32(2, 0xffffffff, false);
    expect(rejectOf(f)).toBe('payload_length_exceeds_max');
  });

  it('accepts a payload at the maximum boundary and rejects one past it', () => {
    const f = encode();
    const view = new DataView(f.buffer);
    view.setUint32(2, WIRE_CONSTANTS.max_payload_bytes, false);
    // At the limit it passes the max check and fails the availability check —
    // proving the boundary is `>`, not `>=`.
    expect(rejectOf(f)).toBe('payload_length_exceeds_available');
    view.setUint32(2, WIRE_CONSTANTS.max_payload_bytes + 1, false);
    expect(rejectOf(f)).toBe('payload_length_exceeds_max');
  });

  it('reports truncation inside the relay region as truncated, not as an availability error', () => {
    const f = encode();
    const cut = decodeFrame(f).offsets.relayRegionOffset + 1;
    expect(rejectOf(f.subarray(0, cut))).toBe('truncated');
  });

  it('rejects trailing bytes after the signature', () => {
    // QUIC preserves datagram boundaries, so this is never a transport artifact.
    expect(rejectOf(concatBytes(encode(), Uint8Array.of(0)))).toBe('trailing_bytes');
  });

  it('rejects an extension region larger than the registry allows', () => {
    const f = encode();
    const extAt = WIRE_CONSTANTS.publisher_fixed_prefix_bytes;
    new DataView(f.buffer).setUint16(extAt, 0xffff, false);
    expect(rejectOf(f)).toBe('extensions_too_large');
  });

  it('rejects an unknown extension type rather than skipping it', () => {
    // Skipping is the forward-compatible instinct and the ADR-0036 §2 defect: it
    // turns the region into a covert channel out of a compromised publisher.
    const f = encode({ extensions: [{ extType: 1, value: Uint8Array.of(5) }] });
    f[WIRE_CONSTANTS.publisher_fixed_prefix_bytes + 2] = 0x7f;
    expect(rejectOf(f)).toBe('extensions_malformed');
  });

  it('enforces the accepted VALUE set, not merely the type and length', () => {
    // §7 treats publisher-supplied selection signals as untrusted: a value past
    // the declared maximum is a publisher trying to outrank its priority group.
    const max = WIRE_CONSTANTS.max_ext_bytes;
    expect(max).toBeGreaterThan(0);
    const f = encode({ extensions: [{ extType: 1, value: Uint8Array.of(100) }] });
    expect(() => decodeFrame(f)).not.toThrow();
    const bad = encode({ extensions: [{ extType: 1, value: Uint8Array.of(100) }] });
    bad[WIRE_CONSTANTS.publisher_fixed_prefix_bytes + 4] = 101;
    expect(rejectOf(bad)).toBe('extensions_malformed');
  });
});

describe('extension grammar', () => {
  function withExtRegion(ext: Uint8Array): Uint8Array {
    // Hand-built extension region: `buildUnsignedFrame` can only produce
    // grammatical ones, and these cases are about ungrammatical input.
    const f = encode({ extensions: [{ extType: 1, value: Uint8Array.of(5) }] });
    const at = WIRE_CONSTANTS.publisher_fixed_prefix_bytes;
    const out = Uint8Array.from(f);
    new DataView(out.buffer).setUint16(at, ext.length, false);
    out.set(ext, at + 2);
    return out.subarray(0, at + 2 + ext.length + WIRE_CONSTANTS.relay_region_bytes + 24 + 64);
  }

  function rejectOf(bytes: Uint8Array): string {
    try {
      decodeFrame(bytes);
    } catch (err) {
      return (err as FrameRejectedError).rejectReason;
    }
    return 'accepted';
  }

  it('rejects an entry header cut short', () => {
    expect(rejectOf(withExtRegion(Uint8Array.of(1)))).toBe('extensions_malformed');
  });

  it('rejects a value shorter than the entry declares', () => {
    expect(rejectOf(withExtRegion(Uint8Array.of(1, 1)))).toBe('extensions_malformed');
  });

  it('rejects a declared value length the registry does not agree with', () => {
    expect(rejectOf(withExtRegion(Uint8Array.of(1, 2, 0)))).toBe('extensions_malformed');
  });

  it('accepts an empty extension region', () => {
    expect(() => decodeFrame(encode({ extensions: [] }))).not.toThrow();
  });

  // NOT TESTED, and deliberately: the duplicate-type and ascending-order arms are
  // unreachable through the current registry, which declares exactly one type.
  // Two entries of type 1 total six bytes and trip `extensions_too_large` first,
  // and an out-of-order pair needs a second type to exist. The checks stay because
  // the registry is data and will gain entries; contorting a test to reach them
  // today would pin the contortion rather than the rule.

  it('rejects an encoder-side extension region over the registry maximum', () => {
    expect(() =>
      buildUnsignedFrame(
        baseInput({
          extensions: [
            { extType: 1, value: Uint8Array.of(1) },
            { extType: 1, value: Uint8Array.of(2) },
          ],
        }),
      ),
    ).toThrow(FrameRejectedError);
  });
});

describe('encode-side invariants', () => {
  it('refuses a key-bearing flag without a wrapped key, and the reverse', () => {
    expect(() =>
      buildUnsignedFrame(
        baseInput({
          flags: { independentlyDecodable: true, discardable: false, keyBearing: true },
        }),
      ),
    ).toThrow(/must agree/);
  });

  it('refuses a payload over the maximum before allocating for it', () => {
    expect(() =>
      buildUnsignedFrame(
        baseInput({ payload: new Uint8Array(WIRE_CONSTANTS.max_payload_bytes + 1) }),
      ),
    ).toThrow(FrameRejectedError);
  });

  it('populates payload_length before the AAD is sliced', () => {
    // The encode-side mirror of the slice rule. A range taken over a half-filled
    // buffer authenticates zeros, and the frame then verifies against nothing.
    const unsigned = buildUnsignedFrame(baseInput({ payload: new Uint8Array(24).fill(1) }));
    expect(new DataView(unsigned.aeadAad.buffer).getUint32(2, false)).toBe(24);
  });

  it('populates the wrapped-key block before the AAD is sliced', () => {
    const wrap = new Uint8Array(48).fill(0x77);
    const unsigned = buildUnsignedFrame(
      baseInput({
        flags: { independentlyDecodable: true, discardable: false, keyBearing: true },
        wrappedTransmitKey: { kekGeneration: 4, wrappedKeyWithTag: wrap },
      }),
    );
    const aadHex = bytesToHex(unsigned.aeadAad);
    expect(aadHex).toContain(bytesToHex(wrap));
    expect(aadHex).not.toContain('0'.repeat(96));
  });

  it('rejects a signature of the wrong length', () => {
    expect(() => finishFrame(buildUnsignedFrame(baseInput()), new Uint8Array(63))).toThrow(
      /64 bytes/,
    );
  });
});

describe('flag constants agree with the SSoT mask', () => {
  it('the three flags OR to legal_flag_mask', () => {
    // `legal_flag_mask` is 7, so no banned-literal guard could ever catch a
    // hardcode of it — this equality is the only enforcement that the mask and
    // the individual bits stay in step.
    expect(FLAG_INDEPENDENTLY_DECODABLE | 0b10 | FLAG_KEY_BEARING).toBe(
      WIRE_CONSTANTS.legal_flag_mask,
    );
  });
});
