// File: packages/sdk-core/src/media/frame/__tests__/frameCodec.v3.test.ts
//
// HEADER v3, HAND-BUILT FROM THE ADR-0036 §2 TABLE (task C-v3).
//
// ---------------------------------------------------------------------------
// WHAT THIS FILE IS NOT
// ---------------------------------------------------------------------------
//
// THIS IS NOT EVIDENCE THAT THE v3 CODEC IS CORRECT. Every expectation below is
// the same author's reading of the same ADR table the codec was written from.
// The hand-assembled frame (`assembleV3`) is laid out byte by byte with explicit
// offsets, without calling any codec helper, so it does catch the encoder and
// the decoder DISAGREEING with each other, or with the offsets written down
// here. It cannot catch a MISREADING of the table: a misread offset would be
// misread identically in both places, and every test here would stay green.
//
// The gate that catches a misreading is the cross-language v3 vectors
// (`vectors.conformance.test.ts`), produced by the Rust generator written
// independently from the same table (R-22). Keeping the two derivations
// separate is what makes that gate meaningful. Do not "fix" a red conformance row by editing this file to agree
// with the codec; fix whichever side misreads the ADR.
//
// The ADR literals (offsets 0/1/2/6/10/14, block sizes 50/2/6/64) are written
// out here ON PURPOSE, as numbers. Deriving them from the codec's constants
// would make this file a second copy of the codec's arithmetic rather than a
// check against the table.

import { describe, expect, it } from 'vitest';

import { bytesToHex, concatBytes } from '../hex.js';
import {
  PROTOCOL_VERSION,
  SUPPORTED_HEADER_VERSIONS,
  buildPublisherRegion,
  buildUnsignedFrame,
  decodeFrame,
  finishFrame,
  isSupportedHeaderVersion,
  writeHopSequence,
  type EncodeFrameInput,
  type FrameExtension,
} from '../frameCodec.js';
import {
  exportPublicKey,
  importSigningKeyFromSeed,
  signFrame,
  verifyFrameSignature,
} from '../ed25519.js';
import { FrameRejectedError, type RejectReason } from '../rejectReason.js';
import { HEADER_VERSION, MAX_PAYLOAD_BYTES, WIRE_CONSTANTS } from '../wireConstants.js';

// --- the ADR-0036 §2 v3 table, as literals ----------------------------------

const V3 = 3;
const OFF_VERSION = 0;
const OFF_FLAGS = 1;
const OFF_PAYLOAD_LENGTH = 2;
const OFF_STREAM_SEQUENCE = 6;
const OFF_GOP_COUNTER = 10;
const FIXED_PREFIX = 14;
const WRAPPED_KEY_BLOCK = 50;
const EXT_LENGTH_FIELD = 2;
const RELAY_REGION = 6;
const SIGNATURE = 64;
const U32_MAX = 0xffff_ffff;
const U16_MAX = 0xffff;

const F_INDEP = 0x01;
const F_DISCARD = 0x02;
const F_KEY = 0x04;

/** Deterministic, never random (no `getRandomValues` anywhere in this file). */
const SEED = new Uint8Array(32).fill(0x42);
const WRAP_MATERIAL = Uint8Array.from({ length: 48 }, (_, i) => (i * 7 + 3) & 0xff);
const PAYLOAD = Uint8Array.from({ length: 24 }, (_, i) => (i * 13 + 5) & 0xff);
const FAKE_SIG = new Uint8Array(SIGNATURE).fill(0xab);
/** Salience 50: the one registry entry, at a legal value. */
const EXT_SALIENCE = Uint8Array.of(0x01, 0x01, 50);

interface V3Spec {
  readonly flags: number;
  readonly payloadLength?: number;
  readonly streamSequence: number;
  readonly gopCounter: number;
  readonly kekGeneration?: number;
  readonly ext?: Uint8Array;
  readonly streamId: number;
  readonly hopSequence: number;
  readonly payload: Uint8Array;
  readonly signature: Uint8Array;
}

/**
 * Lay out a v3 frame BYTE BY BYTE from the table. No codec helper is called.
 * The wrapped-key block is present iff the spec's flags carry key-bearing, which
 * is also how illegal flag combinations are built for the decoder.
 */
function assembleV3(spec: V3Spec): Uint8Array {
  const keyBearing = (spec.flags & F_KEY) !== 0;
  const ext = spec.ext ?? new Uint8Array(0);
  const total =
    FIXED_PREFIX +
    (keyBearing ? WRAPPED_KEY_BLOCK : 0) +
    EXT_LENGTH_FIELD +
    ext.length +
    RELAY_REGION +
    spec.payload.length +
    SIGNATURE;
  const out = new Uint8Array(total);
  const dv = new DataView(out.buffer);
  out[OFF_VERSION] = V3;
  out[OFF_FLAGS] = spec.flags;
  dv.setUint32(OFF_PAYLOAD_LENGTH, spec.payloadLength ?? spec.payload.length, false);
  dv.setUint32(OFF_STREAM_SEQUENCE, spec.streamSequence, false);
  dv.setUint32(OFF_GOP_COUNTER, spec.gopCounter, false);
  let at = FIXED_PREFIX;
  if (keyBearing) {
    dv.setUint16(at, spec.kekGeneration ?? 0, false);
    out.set(WRAP_MATERIAL, at + 2);
    at += WRAPPED_KEY_BLOCK;
  }
  dv.setUint16(at, ext.length, false);
  at += EXT_LENGTH_FIELD;
  out.set(ext, at);
  at += ext.length;
  dv.setUint16(at, spec.streamId, false);
  dv.setUint32(at + 2, spec.hopSequence, false);
  at += RELAY_REGION;
  out.set(spec.payload, at);
  at += spec.payload.length;
  out.set(spec.signature, at);
  return out;
}

/** Hand-computed region boundaries for a spec, from the table. */
function layoutOf(spec: V3Spec) {
  const keyBearing = (spec.flags & F_KEY) !== 0;
  const pubLen =
    FIXED_PREFIX +
    (keyBearing ? WRAPPED_KEY_BLOCK : 0) +
    EXT_LENGTH_FIELD +
    (spec.ext?.length ?? 0);
  const payloadStart = pubLen + RELAY_REGION;
  const sigOffset = payloadStart + spec.payload.length;
  return { pubLen, payloadStart, sigOffset, total: sigOffset + SIGNATURE };
}

function flagsOf(bits: number) {
  return {
    independentlyDecodable: (bits & F_INDEP) !== 0,
    discardable: (bits & F_DISCARD) !== 0,
    keyBearing: (bits & F_KEY) !== 0,
  };
}

function extensionsOf(ext: Uint8Array | undefined): FrameExtension[] {
  return ext && ext.length > 0
    ? [{ extType: ext[0] as number, value: Uint8Array.from(ext.subarray(2)) }]
    : [];
}

/** The codec input for the same spec. */
function inputOf(spec: V3Spec): EncodeFrameInput {
  return {
    headerVersion: V3,
    flags: flagsOf(spec.flags),
    streamSequence: spec.streamSequence,
    gopCounter: spec.gopCounter,
    wrappedTransmitKey:
      (spec.flags & F_KEY) !== 0
        ? {
            kekGeneration: spec.kekGeneration ?? 0,
            wrappedKeyWithTag: Uint8Array.from(WRAP_MATERIAL),
          }
        : null,
    extensions: extensionsOf(spec.ext),
    streamId: spec.streamId,
    hopSequence: spec.hopSequence,
    payload: spec.payload,
  };
}

function baseSpec(over: Partial<V3Spec> = {}): V3Spec {
  return {
    flags: F_INDEP | F_KEY,
    streamSequence: 0x0102_0304,
    gopCounter: 0x0a0b_0c0d,
    kekGeneration: 0x0708,
    ext: EXT_SALIENCE,
    streamId: 0x0506,
    hopSequence: 0x1112_1314,
    payload: PAYLOAD,
    signature: FAKE_SIG,
    ...over,
  };
}

/** The typed verdict, asserting the throw IS one. Never "it threw". */
function verdict(fn: () => unknown): RejectReason | 'accepted' {
  try {
    fn();
  } catch (err) {
    expect(err).toBeInstanceOf(FrameRejectedError);
    return (err as FrameRejectedError).rejectReason;
  }
  return 'accepted';
}

function rejectAt(fn: () => unknown): { reason: RejectReason; at: number | undefined } {
  try {
    fn();
  } catch (err) {
    expect(err).toBeInstanceOf(FrameRejectedError);
    const e = err as FrameRejectedError;
    return { reason: e.rejectReason, at: e.detail.at };
  }
  throw new Error('expected a reject');
}

async function signedV3(spec: V3Spec) {
  const signingKey = await importSigningKeyFromSeed(SEED);
  const publicKey = await exportPublicKey(signingKey);
  const unsigned = buildUnsignedFrame(inputOf(spec));
  const frame = finishFrame(unsigned, await signFrame(signingKey, unsigned.signedRange));
  return { frame, publicKey };
}

async function verifies(frame: Uint8Array, publicKey: Uint8Array): Promise<boolean> {
  const d = decodeFrame(frame);
  return verifyFrameSignature(publicKey, d.signature, d.signedRange);
}

// ---------------------------------------------------------------------------

describe('v3 is the only version: the SSoT and the table agree', () => {
  it('pins the generated constants to the table literally', () => {
    expect(HEADER_VERSION).toBe(V3);
    expect(PROTOCOL_VERSION).toBe(V3);
    expect(WIRE_CONSTANTS.gop_counter_field_bytes).toBe(4);
    expect(WIRE_CONSTANTS.publisher_fixed_prefix_bytes).toBe(FIXED_PREFIX);
    // MAX_HEADER_BYTES = 14 + 50 + 2 + MAX_EXT_BYTES + 6 (ADR-0036 §2 frame extent).
    expect(WIRE_CONSTANTS.max_header_bytes).toBe(
      FIXED_PREFIX +
        WRAPPED_KEY_BLOCK +
        EXT_LENGTH_FIELD +
        WIRE_CONSTANTS.max_ext_bytes +
        RELAY_REGION,
    );
    // The 14-byte prefix: a plain frame's publisher region is prefix + ext_length.
    const spec = baseSpec({ flags: F_INDEP, ext: new Uint8Array(0) });
    expect(decodeFrame(assembleV3(spec)).offsets.publisherRegionLen).toBe(14 + 2);
  });

  it('speaks exactly one version, for encode, decode, send and the join declaration', () => {
    expect(SUPPORTED_HEADER_VERSIONS).toEqual([V3]);
    expect(isSupportedHeaderVersion(V3)).toBe(true);
    for (const v of [0, 1, 2, 4, 0xff]) {
      expect(isSupportedHeaderVersion(v), `version ${v}`).toBe(false);
    }
  });
});

describe('hand-assembled frame vs the codec', () => {
  const cases: [string, Partial<V3Spec>][] = [
    ['key-bearing, with extensions', {}],
    ['key-bearing, no extensions', { ext: new Uint8Array(0) }],
    ['not key-bearing, with extensions', { flags: F_INDEP }],
    ['not key-bearing, no extensions', { flags: F_INDEP, ext: new Uint8Array(0) }],
  ];
  it.each(cases)('%s: encoder output is byte-equal to the table layout', (_name, over) => {
    const spec = baseSpec(over);
    expect(bytesToHex(finishFrame(buildUnsignedFrame(inputOf(spec)), FAKE_SIG))).toBe(
      bytesToHex(assembleV3(spec)),
    );
  });

  it.each(cases)('%s: decoder reads every field at the table offset', (_name, over) => {
    const spec = baseSpec(over);
    const d = decodeFrame(assembleV3(spec));
    const lay = layoutOf(spec);
    expect(d.version).toBe(3);
    expect(d.flags).toEqual(flagsOf(spec.flags));
    expect(d.payloadLength).toBe(PAYLOAD.length);
    expect(d.streamSequence).toBe(spec.streamSequence);
    expect(d.gopCounter).toBe(spec.gopCounter);
    if ((spec.flags & F_KEY) !== 0) {
      expect(d.wrappedTransmitKey?.kekGeneration).toBe(spec.kekGeneration);
      expect(bytesToHex(d.wrappedTransmitKey!.wrappedKeyWithTag)).toBe(bytesToHex(WRAP_MATERIAL));
    } else {
      expect(d.wrappedTransmitKey).toBeNull();
    }
    expect(d.extensions.map((e) => [e.extType, bytesToHex(e.value)])).toEqual(
      extensionsOf(spec.ext).map((e) => [e.extType, bytesToHex(e.value)]),
    );
    expect(d.streamId).toBe(spec.streamId);
    expect(d.hopSequence).toBe(spec.hopSequence);
    expect(bytesToHex(d.payload)).toBe(bytesToHex(PAYLOAD));
    expect(bytesToHex(d.signature)).toBe(bytesToHex(FAKE_SIG));
    expect(d.offsets).toEqual({
      publisherRegionLen: lay.pubLen,
      relayRegionOffset: lay.pubLen,
      payloadOffset: lay.payloadStart,
      signatureOffset: lay.sigOffset,
    });
  });
});

describe('flags (ADR-0036 §2 flags table, decode steps 4-5)', () => {
  it('decodes every legal combination and rejects key-bearing + discardable', () => {
    for (let bits = 0; bits < 8; bits += 1) {
      const frame = assembleV3(baseSpec({ flags: bits }));
      const illegal = (bits & (F_KEY | F_DISCARD)) === (F_KEY | F_DISCARD);
      if (illegal) {
        expect(
          verdict(() => decodeFrame(frame)),
          `flags ${bits}`,
        ).toBe('key_bearing_discardable');
      } else {
        expect(decodeFrame(frame).flags, `flags ${bits}`).toEqual(flagsOf(bits));
      }
    }
  });

  it('rejects each undefined bit, 0x08 through 0x80, one at a time', () => {
    for (let bit = 3; bit < 8; bit += 1) {
      const frame = assembleV3(baseSpec({ flags: F_INDEP | (1 << bit) }));
      expect(
        verdict(() => decodeFrame(frame)),
        `bit ${bit}`,
      ).toBe('reserved_flag_bit_set');
    }
  });

  it('reports the broader reserved-bit reject first when both apply (0xFE)', () => {
    const frame = assembleV3(baseSpec({ flags: F_KEY }));
    frame[OFF_FLAGS] = 0xfe;
    expect(verdict(() => decodeFrame(frame))).toBe('reserved_flag_bit_set');
  });

  it('key-bearing + discardable is terminal on the bare prefix, before the payload bound', () => {
    const prefix = new Uint8Array(FIXED_PREFIX);
    prefix[OFF_VERSION] = 3;
    prefix[OFF_FLAGS] = F_KEY | F_DISCARD;
    new DataView(prefix.buffer).setUint32(OFF_PAYLOAD_LENGTH, MAX_PAYLOAD_BYTES + 1, false);
    expect(verdict(() => decodeFrame(prefix))).toBe('key_bearing_discardable');
  });

  it('the ENCODER refuses key-bearing + discardable', () => {
    expect(verdict(() => buildUnsignedFrame(inputOf(baseSpec({ flags: F_KEY | F_DISCARD }))))).toBe(
      'key_bearing_discardable',
    );
  });
});

describe('version (decode steps 1-3)', () => {
  it('empty and [3] are truncated, any other one byte is terminal (v2 included)', () => {
    expect(rejectAt(() => decodeFrame(new Uint8Array(0)))).toEqual({
      reason: 'truncated',
      at: 0,
    });
    expect(verdict(() => decodeFrame(Uint8Array.of(V3)))).toBe('truncated');
    for (const v of [0, 1, 2, 4, 7, 0xff]) {
      expect(
        verdict(() => decodeFrame(Uint8Array.of(v))),
        `version ${v}`,
      ).toBe('unknown_version');
    }
  });

  it('a whole, well-formed frame relabelled as v2 is refused, not read as v2', () => {
    const frame = assembleV3(baseSpec());
    frame[OFF_VERSION] = 2;
    expect(rejectAt(() => decodeFrame(frame))).toEqual({ reason: 'unknown_version', at: 0 });
  });
});

describe('truncation at every offset (decode steps 1-13), against a hand-written oracle', () => {
  // Each row: cut lengths [from, to) -> reason and the `detail.at` the ADR step
  // implies. Written from the table, never computed by the codec.
  interface Row {
    readonly from: number;
    readonly to: number;
    readonly reason: RejectReason;
    readonly at: number;
  }

  function sweep(spec: V3Spec, oracle: readonly Row[]): number {
    const frame = assembleV3(spec);
    let checked = 0;
    for (let len = 0; len < frame.length; len += 1) {
      const row = oracle.find((r) => len >= r.from && len < r.to);
      expect(row, `no oracle row for length ${len}`).toBeDefined();
      expect(
        rejectAt(() => decodeFrame(frame.subarray(0, len))),
        `cut at ${len}`,
      ).toEqual({
        reason: row!.reason,
        at: row!.at,
      });
      checked += 1;
    }
    expect(decodeFrame(frame).payloadLength).toBe(PAYLOAD.length);
    return checked;
  }

  it('key-bearing frame with an extension region: 163 cut points', () => {
    // 14 prefix | 50 wrap -> 64 | ext_len -> 66 | 3 TLV -> 69 | relay -> 75 |
    // payload 24 -> 99 | sig 64 -> 163.
    const oracle: Row[] = [
      { from: 0, to: 1, reason: 'truncated', at: 0 }, // step 1
      { from: 1, to: 14, reason: 'truncated', at: 0 }, // step 3
      { from: 14, to: 64, reason: 'truncated', at: 14 }, // step 9
      { from: 64, to: 66, reason: 'truncated', at: 64 }, // step 10
      { from: 66, to: 69, reason: 'truncated', at: 66 }, // step 11
      { from: 69, to: 75, reason: 'truncated', at: 69 }, // step 12
      { from: 75, to: 139, reason: 'truncated', at: 75 }, // step 13: remaining < 64
      { from: 139, to: 163, reason: 'payload_length_exceeds_available', at: 2 }, // step 13
    ];
    expect(sweep(baseSpec(), oracle)).toBe(163);
  });

  it('plain frame (no key, no extensions): 110 cut points', () => {
    // 14 prefix | ext_len -> 16 | relay -> 22 | payload 24 -> 46 | sig -> 110.
    const oracle: Row[] = [
      { from: 0, to: 1, reason: 'truncated', at: 0 },
      { from: 1, to: 14, reason: 'truncated', at: 0 },
      { from: 14, to: 16, reason: 'truncated', at: 14 },
      { from: 16, to: 22, reason: 'truncated', at: 16 },
      { from: 22, to: 86, reason: 'truncated', at: 22 },
      { from: 86, to: 110, reason: 'payload_length_exceeds_available', at: 2 },
    ];
    expect(sweep(baseSpec({ flags: F_INDEP, ext: new Uint8Array(0) }), oracle)).toBe(110);
  });

  it('a byte after the signature is trailing_bytes (step 14)', () => {
    const frame = concatBytes(assembleV3(baseSpec()), Uint8Array.of(0));
    expect(verdict(() => decodeFrame(frame))).toBe('trailing_bytes');
  });
});

describe('payload_length (step 6), before anything is sized from it', () => {
  it('accepts 0 and MAX_PAYLOAD_BYTES', () => {
    expect(decodeFrame(assembleV3(baseSpec({ payload: new Uint8Array(0) }))).payload.length).toBe(
      0,
    );
    const max = new Uint8Array(MAX_PAYLOAD_BYTES);
    expect(decodeFrame(assembleV3(baseSpec({ payload: max }))).payloadLength).toBe(
      MAX_PAYLOAD_BYTES,
    );
  });

  it('rejects MAX+1 on a buffer far too short to hold it, so the bound ran first', () => {
    const frame = assembleV3(baseSpec({ payloadLength: MAX_PAYLOAD_BYTES + 1 }));
    expect(frame.length).toBeLessThan(200);
    expect(verdict(() => decodeFrame(frame))).toBe('payload_length_exceeds_max');
  });

  it('the v3 encoder refuses MAX+1 before building anything', () => {
    expect(verdict(() => buildPublisherRegion(inputOf(baseSpec()), MAX_PAYLOAD_BYTES + 1))).toBe(
      'payload_length_exceeds_max',
    );
  });
});

describe('gop_counter (step 8): every u32 is accepted and round-trips', () => {
  it.each([0, 1, U32_MAX])('gop_counter %d', (gop) => {
    const spec = baseSpec({ gopCounter: gop });
    const encoded = finishFrame(buildUnsignedFrame(inputOf(spec)), FAKE_SIG);
    expect(bytesToHex(encoded)).toBe(bytesToHex(assembleV3(spec)));
    expect(decodeFrame(encoded).gopCounter).toBe(gop);
  });
});

describe('encoder input ranges (v3): refused, never silently wrapped', () => {
  const bad = [2 ** 32, -1, Number.NaN, 1.5];
  const badU16 = [U16_MAX + 1, -1, Number.NaN, 0.5];

  it.each(bad)('stream_sequence / gop_counter / hop_sequence refuse %d', (v) => {
    expect(() => buildUnsignedFrame(inputOf(baseSpec({ streamSequence: v })))).toThrow(RangeError);
    expect(() => buildUnsignedFrame(inputOf(baseSpec({ gopCounter: v })))).toThrow(RangeError);
    expect(() => buildUnsignedFrame(inputOf(baseSpec({ hopSequence: v })))).toThrow(RangeError);
    const unsigned = buildUnsignedFrame(inputOf(baseSpec()));
    expect(() => writeHopSequence(unsigned.bytes, unsigned.relayRegionOffset, v)).toThrow(
      RangeError,
    );
  });

  it.each(badU16)('stream_id / kek_generation refuse %d', (v) => {
    expect(() => buildUnsignedFrame(inputOf(baseSpec({ streamId: v })))).toThrow(RangeError);
    expect(() => buildUnsignedFrame(inputOf(baseSpec({ kekGeneration: v })))).toThrow(RangeError);
  });

  it('accepts every field at its maximum', () => {
    const spec = baseSpec({
      streamSequence: U32_MAX,
      gopCounter: U32_MAX,
      hopSequence: U32_MAX,
      streamId: U16_MAX,
      kekGeneration: U16_MAX,
    });
    expect(bytesToHex(finishFrame(buildUnsignedFrame(inputOf(spec)), FAKE_SIG))).toBe(
      bytesToHex(assembleV3(spec)),
    );
  });

  it('refuses what the decoder would reject in the extension region and the flag/wrap pairing', () => {
    const unknownType: FrameExtension[] = [{ extType: 2, value: Uint8Array.of(1) }];
    const outOfSet: FrameExtension[] = [{ extType: 1, value: Uint8Array.of(101) }];
    const tooLarge: FrameExtension[] = [{ extType: 1, value: Uint8Array.of(1, 2) }];
    expect(
      verdict(() => buildUnsignedFrame({ ...inputOf(baseSpec()), extensions: unknownType })),
    ).toBe('extensions_malformed');
    expect(
      verdict(() => buildUnsignedFrame({ ...inputOf(baseSpec()), extensions: outOfSet })),
    ).toBe('extensions_malformed');
    expect(
      verdict(() => buildUnsignedFrame({ ...inputOf(baseSpec()), extensions: tooLarge })),
    ).toBe('extensions_too_large');
    expect(() => buildUnsignedFrame({ ...inputOf(baseSpec()), wrappedTransmitKey: null })).toThrow(
      RangeError,
    );
  });

  it('refuses a header version it does not speak', () => {
    for (const v of [0, 1, 2, 4]) {
      expect(verdict(() => buildUnsignedFrame({ ...inputOf(baseSpec()), headerVersion: v }))).toBe(
        'unknown_version',
      );
    }
  });
});

describe('authenticated ranges: slices of the received buffer, relay region excluded', () => {
  it('aeadAad is a VIEW on the input: same buffer, offset 0, the publisher region length', () => {
    for (const over of [{}, { flags: F_INDEP, ext: new Uint8Array(0) }]) {
      const spec = baseSpec(over);
      const data = assembleV3(spec);
      const d = decodeFrame(data);
      expect(d.aeadAad.buffer).toBe(data.buffer);
      expect(d.aeadAad.byteOffset).toBe(0);
      expect(d.aeadAad.length).toBe(layoutOf(spec).pubLen);
      // Aliasing, not a copy: mutating the input moves the view. The gop
      // counter's first byte, at offset 10.
      const before = d.aeadAad[OFF_GOP_COUNTER];
      data[OFF_GOP_COUNTER] = (before as number) ^ 0xff;
      expect(d.aeadAad[OFF_GOP_COUNTER]).toBe((before as number) ^ 0xff);
    }
  });

  it('signedRange is exactly publisher region ‖ payload (hand offsets), and not the naive span', () => {
    const spec = baseSpec();
    const data = assembleV3(spec);
    const lay = layoutOf(spec);
    const d = decodeFrame(data);
    const expected = concatBytes(
      data.subarray(0, lay.pubLen),
      data.subarray(lay.payloadStart, lay.sigOffset),
    );
    expect(bytesToHex(d.signedRange)).toBe(bytesToHex(expected));
    // The near-miss ADR-0036 §2 names: sign everything up to the signature.
    const naive = data.subarray(0, lay.sigOffset);
    expect(bytesToHex(d.signedRange)).not.toBe(bytesToHex(naive));
    expect(d.signedRange.length).toBe(naive.length - RELAY_REGION);
  });

  it('rewriting the relay stream_id (0 and 65535) and hop changes neither range; the signature holds', async () => {
    const { frame, publicKey } = await signedV3(baseSpec());
    const original = decodeFrame(frame);
    const relay = original.offsets.relayRegionOffset;
    for (const streamId of [0, U16_MAX]) {
      const rewritten = Uint8Array.from(frame);
      const dv = new DataView(rewritten.buffer);
      dv.setUint16(relay, streamId, false);
      dv.setUint32(relay + 2, U32_MAX, false);
      const d = decodeFrame(rewritten);
      expect(d.streamId).toBe(streamId);
      expect(bytesToHex(d.aeadAad)).toBe(bytesToHex(original.aeadAad));
      expect(bytesToHex(d.signedRange)).toBe(bytesToHex(original.signedRange));
      expect(await verifies(rewritten, publicKey)).toBe(true);
    }
  });

  it('flipping ANY publisher or payload byte breaks the frame; flipping a relay byte does not', async () => {
    const spec = baseSpec();
    const { frame, publicKey } = await signedV3(spec);
    const original = decodeFrame(frame);
    const lay = layoutOf(spec);
    expect(await verifies(frame, publicKey)).toBe(true);

    // The four gop_counter bytes, specifically: they must be authenticated.
    for (let i = OFF_GOP_COUNTER; i < OFF_GOP_COUNTER + 4; i += 1) {
      const t = Uint8Array.from(frame);
      t[i] = (t[i] as number) ^ 0x01;
      const d = decodeFrame(t);
      expect(bytesToHex(d.aeadAad), `gop byte ${i}`).not.toBe(bytesToHex(original.aeadAad));
      expect(bytesToHex(d.signedRange), `gop byte ${i}`).not.toBe(bytesToHex(original.signedRange));
      expect(await verifies(t, publicKey), `gop byte ${i}`).toBe(false);
    }

    // Every authenticated byte: either the decode rejects (typed), or the
    // ranges move and the signature fails. WHICH bytes reject is written down
    // from the table for this spec (xor 0x01 on each), never read off a run, so
    // a decoder change that turned most flips into structural rejects (and so
    // checked almost no signatures) goes red here:
    //   0       version 3 -> 2: unknown_version (no other version decodes)
    //   2-5     payload_length 24 -> over the maximum / over what is available
    //   64-65   ext_length 3 -> 259 (too large) / 2 (TLV cut short: malformed)
    //   66-67   TLV type 1 -> 0 (unknown) / value_len 1 -> 0 (registry mismatch)
    // Everything else — flags (still legal), seq, gop, kek_generation, the 48
    // wrap bytes, the salience value (50 -> 51) and all 24 payload bytes —
    // decodes and must FAIL verification.
    const EXPECTED_REJECTS = [0, 2, 3, 4, 5, 64, 65, 66, 67];
    const authenticated = [
      ...Array.from({ length: lay.pubLen }, (_, i) => i),
      ...Array.from({ length: lay.sigOffset - lay.payloadStart }, (_, i) => lay.payloadStart + i),
    ];
    const rejected: number[] = [];
    let verifyFailed = 0;
    for (const i of authenticated) {
      const t = Uint8Array.from(frame);
      t[i] = (t[i] as number) ^ 0x01;
      let d;
      try {
        d = decodeFrame(t);
      } catch (err) {
        expect(err, `byte ${i}`).toBeInstanceOf(FrameRejectedError);
        rejected.push(i);
        continue;
      }
      expect(bytesToHex(d.signedRange), `byte ${i}`).not.toBe(bytesToHex(original.signedRange));
      expect(await verifies(t, publicKey), `byte ${i}`).toBe(false);
      verifyFailed += 1;
    }
    expect(rejected).toEqual(EXPECTED_REJECTS);
    // flags 1 + seq 4 + gop 4 + kek 2 + wrap 48 + salience value 1 + payload 24.
    expect(verifyFailed).toBe(84);
    expect(verifyFailed + rejected.length).toBe(authenticated.length);

    for (let i = lay.pubLen; i < lay.payloadStart; i += 1) {
      const t = Uint8Array.from(frame);
      t[i] = (t[i] as number) ^ 0x01;
      const d = decodeFrame(t);
      expect(bytesToHex(d.aeadAad), `relay byte ${i}`).toBe(bytesToHex(original.aeadAad));
      expect(bytesToHex(d.signedRange), `relay byte ${i}`).toBe(bytesToHex(original.signedRange));
      expect(await verifies(t, publicKey), `relay byte ${i}`).toBe(true);
    }
  });

  it('the encoder takes its ranges from the completed buffer', () => {
    const spec = baseSpec();
    const unsigned = buildUnsignedFrame(inputOf(spec));
    const lay = layoutOf(spec);
    expect(unsigned.aeadAad.buffer).toBe(unsigned.bytes.buffer);
    expect(bytesToHex(unsigned.aeadAad)).toBe(bytesToHex(assembleV3(spec).subarray(0, lay.pubLen)));
    expect(unsigned.relayRegionOffset).toBe(lay.pubLen);
  });
});

describe('extension region on the v3 DECODE side (steps 10-11), hand-assembled', () => {
  function withExtLength(spec: V3Spec, extLength: number, at: number): Uint8Array {
    // Cut right after ext_length: the region it declares is NOT present, so a
    // too-large verdict proves the limit ran before availability.
    const frame = assembleV3(spec).subarray(0, at + EXT_LENGTH_FIELD);
    new DataView(frame.buffer, frame.byteOffset).setUint16(at, extLength, false);
    return frame;
  }

  it('ext_length = MAX_EXT_BYTES + 1 is extensions_too_large before availability (key-bearing, at 64)', () => {
    const frame = withExtLength(baseSpec(), 4, FIXED_PREFIX + WRAPPED_KEY_BLOCK);
    expect(rejectAt(() => decodeFrame(frame))).toEqual({ reason: 'extensions_too_large', at: 64 });
  });

  it('ext_length = MAX_EXT_BYTES + 1 is extensions_too_large before availability (plain, at 14)', () => {
    const frame = withExtLength(baseSpec({ flags: F_INDEP }), 4, FIXED_PREFIX);
    expect(rejectAt(() => decodeFrame(frame))).toEqual({ reason: 'extensions_too_large', at: 14 });
  });

  it('an unknown TLV type and an out-of-set salience are extensions_malformed', () => {
    expect(
      verdict(() => decodeFrame(assembleV3(baseSpec({ ext: Uint8Array.of(0x02, 0x01, 50) })))),
    ).toBe('extensions_malformed');
    expect(
      verdict(() => decodeFrame(assembleV3(baseSpec({ ext: Uint8Array.of(0x01, 0x01, 101) })))),
    ).toBe('extensions_malformed');
  });
});
