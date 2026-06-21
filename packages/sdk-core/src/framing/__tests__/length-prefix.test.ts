// File: packages/sdk-core/src/framing/__tests__/length-prefix.test.ts
//
// R-16 framing codec unit tests. The wire contract is anchored to MC's Rust
// implementation: crates/mc-service/src/webtransport/connection.rs
//   :33   MAX_MESSAGE_SIZE = 64 * 1024
//   :648  put_u32(len)  -> 4-byte big-endian length prefix
//   :607  reject len > MAX  ("Message too large")
//   :617  reject len == 0   ("Empty message")
//
// DEFERRED(R-41): property-based round-trip coverage via fast-check (random
// payloads / chunk splits) is deferred to the R-41 test-suite task — see
// ADR-0028 §182/184 and the matching docs/TODO.md entry. These example-based
// tests cover the contract surface explicitly in the meantime.

import { describe, expect, it } from 'vitest';
import {
  encodeFrame,
  FrameDecoder,
  FramingError,
  FramingErrorCode,
  MAX_MESSAGE_SIZE,
} from '../length-prefix.js';

function bytes(...vals: number[]): Uint8Array {
  return new Uint8Array(vals);
}

describe('encodeFrame / FrameDecoder round-trip', () => {
  it('test 1: encodes a payload and decodes it back (one frame, one chunk)', () => {
    const payload = bytes(1, 2, 3, 4, 5);
    const frame = encodeFrame(payload);

    const decoder = new FrameDecoder();
    const out = decoder.push(frame);

    expect(out).toHaveLength(1);
    expect(out[0]).toEqual(payload);
    expect(decoder.bufferedByteLength).toBe(0);
  });

  it('test 2: buffers a single frame split across 3 chunks (prefix + body splits)', () => {
    const payload = bytes(10, 20, 30, 40);
    const frame = encodeFrame(payload); // 4-byte prefix + 4-byte body = 8 bytes

    const decoder = new FrameDecoder();
    // Chunk A: 2 of the 4 prefix bytes -> nothing yet.
    expect(decoder.push(frame.slice(0, 2))).toEqual([]);
    // Chunk B: rest of prefix + 1 body byte -> still incomplete.
    expect(decoder.push(frame.slice(2, 5))).toEqual([]);
    // Chunk C: remaining 3 body bytes -> frame completes.
    const out = decoder.push(frame.slice(5));

    expect(out).toHaveLength(1);
    expect(out[0]).toEqual(payload);
    expect(decoder.bufferedByteLength).toBe(0);
  });

  it('test 3: returns multiple complete frames arriving in one chunk, in order', () => {
    const p1 = bytes(1, 1, 1);
    const p2 = bytes(2, 2);
    const p3 = bytes(3, 3, 3, 3);
    const combined = concatAll(encodeFrame(p1), encodeFrame(p2), encodeFrame(p3));

    const decoder = new FrameDecoder();
    const out = decoder.push(combined);

    expect(out).toHaveLength(3);
    expect(out[0]).toEqual(p1);
    expect(out[1]).toEqual(p2);
    expect(out[2]).toEqual(p3);
    expect(decoder.bufferedByteLength).toBe(0);
  });

  it('test 4: handles a chunk ending mid-frame after a complete frame', () => {
    const p1 = bytes(9, 9, 9);
    const p2 = bytes(7, 7, 7, 7, 7);
    const f1 = encodeFrame(p1);
    const f2 = encodeFrame(p2);

    // Chunk A = full f1 + first half of f2.
    const splitAt = 4 + 2; // f2 prefix + 2 body bytes
    const chunkA = concatAll(f1, f2.slice(0, splitAt));
    const chunkB = f2.slice(splitAt);

    const decoder = new FrameDecoder();
    const outA = decoder.push(chunkA);
    expect(outA).toHaveLength(1);
    expect(outA[0]).toEqual(p1);
    expect(decoder.bufferedByteLength).toBe(splitAt);

    const outB = decoder.push(chunkB);
    expect(outB).toHaveLength(1);
    expect(outB[0]).toEqual(p2);
    expect(decoder.bufferedByteLength).toBe(0);
  });
});

describe('encodeFrame rejection', () => {
  it('test 5: rejects a zero-length payload ("Empty message")', () => {
    expect(() => encodeFrame(new Uint8Array(0))).toThrow(FramingError);
    try {
      encodeFrame(new Uint8Array(0));
      expect.unreachable('should have thrown');
    } catch (e) {
      const err = e as FramingError;
      expect(err.code).toBe(FramingErrorCode.EmptyMessage);
      expect(err.length).toBe(0);
      expect(err.max).toBe(MAX_MESSAGE_SIZE);
    }
  });

  it('test 6: rejects a payload larger than MAX_MESSAGE_SIZE ("Message too large")', () => {
    const tooBig = new Uint8Array(MAX_MESSAGE_SIZE + 1);
    try {
      encodeFrame(tooBig);
      expect.unreachable('should have thrown');
    } catch (e) {
      const err = e as FramingError;
      expect(err.code).toBe(FramingErrorCode.MessageTooLarge);
      expect(err.length).toBe(MAX_MESSAGE_SIZE + 1);
      expect(err.max).toBe(MAX_MESSAGE_SIZE);
    }
    // Exactly MAX is allowed (boundary).
    expect(() => encodeFrame(new Uint8Array(MAX_MESSAGE_SIZE))).not.toThrow();
  });
});

describe('FrameDecoder rejection', () => {
  it('test 7: rejects an incoming frame whose prefix declares length 0', () => {
    const zeroPrefix = bytes(0, 0, 0, 0);
    const decoder = new FrameDecoder();
    try {
      decoder.push(zeroPrefix);
      expect.unreachable('should have thrown');
    } catch (e) {
      const err = e as FramingError;
      expect(err.code).toBe(FramingErrorCode.EmptyMessage);
      expect(err.length).toBe(0);
    }
  });

  it('test 8: rejects an oversize declared length BEFORE buffering the body', () => {
    // Prefix declares MAX+1, with NO body bytes following. The decoder must
    // reject on the prefix alone (no unbounded body buffering).
    const oversize = MAX_MESSAGE_SIZE + 1;
    const prefix = new Uint8Array(4);
    new DataView(prefix.buffer).setUint32(0, oversize, false);

    const decoder = new FrameDecoder();
    try {
      decoder.push(prefix);
      expect.unreachable('should have thrown');
    } catch (e) {
      const err = e as FramingError;
      expect(err.code).toBe(FramingErrorCode.MessageTooLarge);
      expect(err.length).toBe(oversize);
      expect(err.max).toBe(MAX_MESSAGE_SIZE);
    }
  });

  it('test 9: buffers a truncated prefix (<4 bytes) without emitting or throwing', () => {
    const decoder = new FrameDecoder();
    expect(decoder.push(bytes(0, 0))).toEqual([]);
    expect(decoder.push(bytes(0))).toEqual([]);
    expect(decoder.bufferedByteLength).toBe(3);
    // Completing the prefix (declares len 1) + body then yields the frame.
    const out = decoder.push(bytes(1, 0xab));
    expect(out).toHaveLength(1);
    expect(out[0]).toEqual(bytes(0xab));
  });
});

describe('wire-contract byte layout (matches MC put_u32)', () => {
  it('test 10: encodeFrame writes a big-endian u32 length prefix', () => {
    // 3-byte payload -> prefix bytes must be exactly [0x00, 0x00, 0x00, 0x03].
    const frame = encodeFrame(bytes(0xaa, 0xbb, 0xcc));
    expect(Array.from(frame.slice(0, 4))).toEqual([0x00, 0x00, 0x00, 0x03]);
    expect(Array.from(frame.slice(4))).toEqual([0xaa, 0xbb, 0xcc]);

    // A 258-byte payload exercises two prefix bytes (258 = 0x0102), confirming
    // big-endian ordering rather than little-endian ([0x02, 0x01, ...]).
    const f258 = encodeFrame(new Uint8Array(258));
    expect(Array.from(f258.slice(0, 4))).toEqual([0x00, 0x00, 0x01, 0x02]);
  });
});

function concatAll(...arrs: Uint8Array[]): Uint8Array {
  const total = arrs.reduce((n, a) => n + a.byteLength, 0);
  const out = new Uint8Array(total);
  let offset = 0;
  for (const a of arrs) {
    out.set(a, offset);
    offset += a.byteLength;
  }
  return out;
}
