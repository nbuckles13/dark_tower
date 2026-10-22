// File: packages/sdk-core/src/media/pipeline/__tests__/streamReader.test.ts
//
// The incremental uni-stream reader (story 3 R-21, C-B) — a deterministic
// corpus built from the v3 vectors, plus the allocation-before-length-check
// proof through the reader's allocator seam.
//
// CORPUS (there is no fast-check in the repo): from every VALID vector frame,
//   (a) truncation at EVERY byte offset, then EOF and then reset;
//   (b) `payload_length` in {0, MAX, MAX+1, u32::MAX};
//   (c) every single-bit flip over the header bytes the reader parses;
//   (d) a valid multi-frame stream split at every 1-byte boundary plus splits
//       from a fixed-seed PRNG (the seed is in the test name).
// Every case must end in a TYPED outcome — a whole frame, a `RejectReason`
// member, or "incomplete" at EOF/reset — never an untyped throw and never an
// allocation sized from an unbounded length.

import { describe, expect, it } from 'vitest';
import { createSeededRng } from '@darktower/test-utils';

import {
  FRAME_EXTENT_PEEK_BYTES,
  MAX_FRAME_BYTES,
  decodeFrame,
  peekFrameExtent,
} from '../../frame/frameCodec.js';
import { hexToBytes } from '../../frame/hex.js';
import {
  ALL_REJECT_REASONS,
  FrameRejectedError,
  type RejectReason,
} from '../../frame/rejectReason.js';
import { MAX_PAYLOAD_BYTES, WIRE_CONSTANTS } from '../../frame/wireConstants.js';
import { loadFrameVectors } from '../../frame/__tests__/frameVectors.js';
import { UniStreamFrameReader, type StreamFrameHeader } from '../streamReader.js';

type Outcome =
  | {
      readonly kind: 'frame';
      readonly bytes: Uint8Array;
      readonly header: StreamFrameHeader | undefined;
    }
  | { readonly kind: 'reject'; readonly reason: RejectReason }
  | { readonly kind: 'incomplete' };

interface Run {
  readonly started: number;
  readonly outcomes: Outcome[];
  readonly allocations: number[];
  readonly lastHeader: StreamFrameHeader | undefined;
}

/** Feed `chunks`, then end the stream. Returns every typed outcome, in order. */
function run(chunks: readonly Uint8Array[]): Run {
  const outcomes: Outcome[] = [];
  const allocations: number[] = [];
  let started = 0;
  const reader = new UniStreamFrameReader(
    {
      onFrameStart: () => {
        started += 1;
      },
      onFrame: (bytes, header) => outcomes.push({ kind: 'frame', bytes, header }),
      onPrefixReject: (reason) => outcomes.push({ kind: 'reject', reason }),
    },
    (n) => {
      allocations.push(n);
      return new Uint8Array(n);
    },
  );
  for (const chunk of chunks) reader.push(chunk);
  if (reader.end().partial) outcomes.push({ kind: 'incomplete' });
  return { started, outcomes, allocations, lastHeader: reader.lastHeader };
}

const vectors = loadFrameVectors();
/** Every vector frame the codec ACCEPTS whole — the corpus seeds. */
const VALID = vectors.vectors
  .map((row) => ({ name: row.name, bytes: hexToBytes(row.frame_hex) }))
  .filter(({ bytes }) => {
    try {
      decodeFrame(bytes);
      return true;
    } catch {
      return false;
    }
  });

/** Every outcome is typed; every started frame ended exactly once. */
function expectTyped(r: Run): void {
  for (const o of r.outcomes) {
    if (o.kind === 'reject') expect(ALL_REJECT_REASONS).toContain(o.reason);
  }
  expect(r.outcomes.length, 'each started frame ends in exactly one outcome').toBe(r.started);
  for (const n of r.allocations) expect(n).toBeLessThanOrEqual(MAX_FRAME_BYTES);
}

function withPayloadLength(frame: Uint8Array, value: number): Uint8Array {
  const out = Uint8Array.from(frame);
  new DataView(out.buffer).setUint32(2, value, false);
  return out;
}

describe('UniStreamFrameReader — whole frames only', () => {
  it('has a non-empty corpus seed set (positive control)', () => {
    expect(VALID.length).toBeGreaterThan(5);
  });

  it('delivers a whole frame byte-identical to its input, with exactly one allocation of its exact length', () => {
    const { bytes } = VALID[0] as { bytes: Uint8Array };
    const r = run([bytes]);
    expect(r.outcomes).toHaveLength(1);
    const [o] = r.outcomes;
    expect(o?.kind).toBe('frame');
    expect((o as { bytes: Uint8Array }).bytes).toEqual(bytes);
    // Allocator seam POSITIVE CONTROL: called once, with the frame's length.
    expect(r.allocations).toEqual([bytes.length]);
  });

  it('parses the header (relay slot, key-id sender, GoP) once the key id is buffered', () => {
    for (const { bytes } of VALID) {
      const decoded = decodeFrame(bytes);
      const r = run([bytes]);
      const header = (r.outcomes[0] as { header: StreamFrameHeader | undefined }).header;
      if (decoded.payload.length < WIRE_CONSTANTS.key_id_bytes + WIRE_CONSTANTS.aead_tag_bytes) {
        expect(header).toBeUndefined();
        continue;
      }
      expect(header?.relayStreamId).toBe(decoded.streamId);
      expect(header?.gopCounter).toBe(decoded.gopCounter);
    }
  });
});

describe('UniStreamFrameReader — the length bound comes BEFORE any allocation', () => {
  for (const value of [MAX_PAYLOAD_BYTES + 1, 0xffff_ffff]) {
    it(`payload_length ${value}: payload_length_exceeds_max, allocator never called`, () => {
      for (const { bytes } of VALID) {
        const r = run([withPayloadLength(bytes, value)]);
        expect(r.allocations).toEqual([]);
        expect(r.outcomes).toEqual([{ kind: 'reject', reason: 'payload_length_exceeds_max' }]);
      }
    });
  }

  it('payload_length 0: a typed outcome; any allocation is bounded', () => {
    for (const { bytes } of VALID) {
      const r = run([withPayloadLength(bytes, 0)]);
      expectTyped(r);
    }
  });

  it('payload_length MAX: exactly incomplete, with ONE allocation of the declared extent (<= MAX_FRAME_BYTES)', () => {
    for (const { bytes } of VALID) {
      const r = run([withPayloadLength(bytes, MAX_PAYLOAD_BYTES)]);
      expect(r.outcomes).toEqual([{ kind: 'incomplete' }]);
      const declared = bytes.length - decodeFrame(bytes).payloadLength + MAX_PAYLOAD_BYTES;
      expect(r.allocations).toEqual([declared]);
      expect(declared).toBeLessThanOrEqual(MAX_FRAME_BYTES);
    }
  });

  it('peekFrameExtent asks for more rather than throwing truncated on a short prefix', () => {
    const { bytes } = VALID[0] as { bytes: Uint8Array };
    for (let n = 0; n < FRAME_EXTENT_PEEK_BYTES; n += 1) {
      const extent = (() => {
        try {
          return peekFrameExtent(bytes.subarray(0, n));
        } catch (err) {
          return err;
        }
      })();
      if (extent instanceof FrameRejectedError) {
        expect(extent.rejectReason).not.toBe('truncated');
      } else if ('complete' in (extent as object)) {
        // ok
      } else {
        throw new Error('peekFrameExtent threw an untyped error');
      }
    }
  });
});

describe('UniStreamFrameReader — corpus (a): truncation at every offset', () => {
  it('every truncation of every valid frame ends typed, at EOF and at reset', () => {
    let cases = 0;
    for (const { bytes } of VALID) {
      for (let cut = 0; cut <= bytes.length; cut += 1) {
        const r = run([bytes.subarray(0, cut)]);
        expectTyped(r);
        if (cut === 0) {
          expect(r.outcomes).toEqual([]);
          expect(r.allocations).toEqual([]);
        } else if (cut === bytes.length) {
          expect(r.outcomes.map((o) => o.kind)).toEqual(['frame']);
          expect(r.allocations).toEqual([bytes.length]);
        } else {
          // A truncated VALID frame: exactly "incomplete", and either no
          // allocation yet (extent unknown) or exactly one of its exact length.
          expect(r.outcomes).toEqual([{ kind: 'incomplete' }]);
          expect([[], [bytes.length]]).toContainEqual(r.allocations);
        }
        cases += 1;
      }
    }
    expect(cases).toBeGreaterThan(100);
  });
});

describe('UniStreamFrameReader — corpus (c): single-bit flips over the parsed header', () => {
  it('every flip ends in a typed outcome, never an untyped throw', () => {
    let cases = 0;
    let rejected = 0;
    for (const { bytes } of VALID) {
      const span = Math.min(FRAME_EXTENT_PEEK_BYTES, bytes.length);
      for (let byte = 0; byte < span; byte += 1) {
        for (let bit = 0; bit < 8; bit += 1) {
          const flipped = Uint8Array.from(bytes);
          flipped[byte] = (flipped[byte] as number) ^ (1 << bit);
          let r: Run | undefined;
          expect(() => {
            r = run([flipped]);
          }).not.toThrow();
          expectTyped(r as Run);
          if ((r as Run).outcomes.some((o) => o.kind === 'reject')) rejected += 1;
          cases += 1;
        }
      }
    }
    expect(cases).toBeGreaterThan(1000);
    // Positive control: flips DO hit the prefix checks.
    expect(rejected).toBeGreaterThan(0);
  });
});

describe('UniStreamFrameReader — corpus (d): chunk splits of a valid multi-frame stream', () => {
  const frames = VALID.slice(0, 4).map((v) => v.bytes);
  const stream = new Uint8Array(frames.reduce((n, f) => n + f.length, 0));
  let at = 0;
  for (const f of frames) {
    stream.set(f, at);
    at += f.length;
  }

  function expectWhole(r: Run): void {
    expectTyped(r);
    expect(r.outcomes.map((o) => (o.kind === 'frame' ? Array.from(o.bytes) : o))).toEqual(
      frames.map((f) => Array.from(f)),
    );
  }

  it('every 1-byte split boundary reassembles the same frames', () => {
    for (let split = 1; split < stream.length; split += 1) {
      expectWhole(run([stream.subarray(0, split), stream.subarray(split)]));
    }
  });

  it('byte-at-a-time reassembles the same frames', () => {
    expectWhole(run(Array.from(stream, (b) => Uint8Array.of(b))));
  });

  const SEED = 0x5eed_c0de;
  it(`64 random multi-way splits (seed 0x${SEED.toString(16)}) reassemble the same frames`, () => {
    const rng = createSeededRng(SEED);
    for (let trial = 0; trial < 64; trial += 1) {
      const chunks: Uint8Array[] = [];
      let i = 0;
      while (i < stream.length) {
        const n = 1 + Math.floor(rng() * 97);
        chunks.push(stream.subarray(i, Math.min(stream.length, i + n)));
        i += n;
      }
      expectWhole(run(chunks));
    }
  });
});

describe('UniStreamFrameReader — lastHeader across truncation (reset recovery fields)', () => {
  it('is undefined before the key id is buffered and set from that point on', () => {
    const { bytes } = VALID.find(({ bytes }) => decodeFrame(bytes).payload.length >= 24) as {
      bytes: Uint8Array;
    };
    const decoded = decodeFrame(bytes);
    const keyIdEnd = decoded.offsets.payloadOffset + WIRE_CONSTANTS.key_id_bytes;
    expect(run([bytes.subarray(0, keyIdEnd - 1)]).lastHeader).toBeUndefined();
    const at = run([bytes.subarray(0, keyIdEnd)]).lastHeader;
    expect(at?.gopCounter).toBe(decoded.gopCounter);
    expect(at?.relayStreamId).toBe(decoded.streamId);
  });

  it('a prefix reject stops the reader: later bytes produce nothing', () => {
    const { bytes } = VALID[0] as { bytes: Uint8Array };
    const bad = Uint8Array.from(bytes);
    bad[0] = 2; // unknown version
    const r = run([bad, bytes]);
    expect(r.outcomes).toEqual([{ kind: 'reject', reason: 'unknown_version' }]);
    expect(r.started).toBe(1);
  });
});
