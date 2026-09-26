// File: packages/test-utils/src/__tests__/FakeAudioCodecs.test.ts
//
// The codec double's own contract. The SDK runs one decoder PER SENDER, and its
// R-5 isolation tests are only meaningful if this double keeps each decoder's
// callbacks, records and failure strictly to itself. A double that routed every
// decoder's output through the last-created callback — or could fail only the
// last one — would make those tests pass vacuously. This file is what stops that
// regressing silently.

import { describe, expect, it } from 'vitest';

import { FakeAudioCodecs } from '../media/index.js';

type Decoder = { decode(frame: { data: Uint8Array; timestampUs: number }): void; close(): void };

async function twoDecoders() {
  const codecs = new FakeAudioCodecs();
  const outputs: [number[], number[]] = [[], []];
  const errors: [number, number] = [0, 0];
  const make = (i: 0 | 1) =>
    codecs.decoderFactory({
      sampleRateHz: 48_000,
      channels: 1,
      onOutput: (data: never) => {
        (data as unknown as { close(): void }).close();
        outputs[i].push(1);
      },
      onError: () => (errors[i] += 1),
    }) as Promise<Decoder>;
  const first = await make(0);
  const second = await make(1);
  return { codecs, first, second, outputs, errors };
}

const frame = (b: number) => ({ data: Uint8Array.of(b), timestampUs: 0 });

describe('FakeAudioCodecs keeps every decoder instance to itself', () => {
  it("invokes ONLY the decoding instance's onOutput, and records per instance", async () => {
    const { codecs, first, outputs } = await twoDecoders();
    first.decode(frame(7));
    expect(outputs).toEqual([[1], []]);
    expect(codecs.decoders[0]!.decoded.map((f) => f.data[0])).toEqual([7]);
    expect(codecs.decoders[1]!.decoded).toEqual([]);
    // The aggregate still sees everything.
    expect(codecs.decoded).toHaveLength(1);
  });

  it("fires ONLY the failed instance's onError", async () => {
    const { codecs, errors } = await twoDecoders();
    codecs.decoders[0]!.fail();
    expect(errors).toEqual([1, 0]);
  });

  it('counts a close once per instance, however often it is called', async () => {
    const { codecs, first } = await twoDecoders();
    first.close();
    first.close();
    expect(codecs.decoderClosed).toBe(1);
    expect(codecs.decoders[0]!.closed).toBe(true);
    expect(codecs.decoders[1]!.closed).toBe(false);
  });

  it('makes failNextDecoderCreation one-shot', async () => {
    const codecs = new FakeAudioCodecs();
    codecs.failNextDecoderCreation = new Error('unsupported');
    const opts = { sampleRateHz: 48_000, channels: 1, onOutput: () => {}, onError: () => {} };
    await expect(codecs.decoderFactory(opts)).rejects.toThrow('unsupported');
    await expect(codecs.decoderFactory(opts)).resolves.toBeDefined();
    expect(codecs.decoders).toHaveLength(1);
  });

  it('aims failDecoder() at the NEWEST OPEN decoder, skipping closed ones', async () => {
    const { codecs, second, errors } = await twoDecoders();
    second.close();
    codecs.failDecoder();
    expect(errors).toEqual([1, 0]);
  });
});
