// File: packages/test-utils/src/__tests__/FakeVideoDecoders.test.ts
//
// The video decoder double's own contract (story 3 C-B). The SDK's R-6 tests
// (exactly M decoders; a switch builds none; a fault on one slot never reaches
// another) and its frame-ownership tests are only meaningful if this double is
// deterministic, keeps each decoder to itself, and counts every close.

import { describe, expect, it } from 'vitest';

import { FakeVideoDecoders, FakeVideoFrameRegistry } from '../media/video.js';

function build(d: FakeVideoDecoders) {
  const outputs: unknown[] = [];
  const errors: unknown[] = [];
  const seam = d.factory({
    config: { codec: 'vp8' },
    onOutput: (f: never) => outputs.push(f),
    onError: (e) => errors.push(e),
  });
  return { seam, outputs, errors };
}

describe('FakeVideoDecoders', () => {
  it('counts constructions, including a failed one', () => {
    const d = new FakeVideoDecoders();
    build(d);
    d.failNextConstruction = new Error('no');
    expect(() => build(d)).toThrow('no');
    build(d);
    expect(d.constructions).toBe(3);
    expect(d.decoders).toHaveLength(2);
  });

  it('outputs ONLY when the test says so, in submission order, carrying the chunk timestamp', () => {
    const d = new FakeVideoDecoders({ width: 320, height: 180 });
    const { seam, outputs } = build(d);
    seam.decode({ data: Uint8Array.of(1), timestampUs: 10, isKeyframe: true });
    seam.decode({ data: Uint8Array.of(2), timestampUs: 20, isKeyframe: false });
    expect(outputs).toHaveLength(0);
    expect(seam.decodeQueueSize).toBe(2);
    const first = d.last!.emitOutput();
    expect(first.timestamp).toBe(10);
    expect([first.codedWidth, first.codedHeight]).toEqual([320, 180]);
    expect(seam.decodeQueueSize).toBe(1);
    const custom = d.last!.emitOutput({ timestampUs: 99, width: 1, height: 2 });
    expect(custom.timestamp).toBe(99);
    expect(seam.decodeQueueSize).toBe(1); // an explicit timestamp does not consume a chunk
    d.last!.queueSizeOverride = 7;
    expect(seam.decodeQueueSize).toBe(7);
  });

  it('errors are per instance; a closed decoder refuses decode', () => {
    const d = new FakeVideoDecoders();
    const a = build(d);
    const b = build(d);
    d.decoders[0]!.emitError();
    expect(a.errors).toHaveLength(1);
    expect(b.errors).toHaveLength(0);
    a.seam.close();
    expect(d.decoders[0]!.closed).toBe(true);
    expect(() =>
      a.seam.decode({ data: Uint8Array.of(1), timestampUs: 1, isKeyframe: true }),
    ).toThrow();
  });
});

describe('FakeVideoDecoders.throwOnNextDecode', () => {
  it('throws synchronously once, then decodes normally (positive control)', () => {
    const d = new FakeVideoDecoders();
    const { seam } = build(d);
    d.last!.throwOnNextDecode = new Error('InvalidStateError');
    const chunk = { data: Uint8Array.of(1), timestampUs: 1, isKeyframe: true };
    expect(() => seam.decode(chunk)).toThrow('InvalidStateError');
    expect(() => seam.decode(chunk)).not.toThrow();
    expect(d.last!.decoded).toHaveLength(1);
  });
});

describe('FakeVideoFrameRegistry', () => {
  it('reports every frame not closed exactly once (positive and negative control)', () => {
    const r = new FakeVideoFrameRegistry();
    const once = r.make(1, 2, 3);
    const never = r.make(2, 2, 3);
    const twice = r.make(3, 2, 3);
    once.close();
    twice.close();
    twice.close();
    expect(r.notClosedExactlyOnce()).toEqual([never, twice]);
  });
});
