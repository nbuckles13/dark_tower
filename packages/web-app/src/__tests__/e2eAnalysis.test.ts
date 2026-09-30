// File: packages/web-app/src/__tests__/e2eAnalysis.test.ts
//
// Story 2 R-30: the layer-3 record the tone detector (task 14) consumes, pinned
// deterministically with fake Web Audio nodes — no real audio, no timing.
//
// The band check reads the SDK's tone constants from SOURCE. That is a drift
// guard in a unit test, not detector code: the instrumentation itself does not
// import them, which is what keeps analysis independent of synthesis.

import { afterEach, describe, expect, it, vi } from 'vitest';
import type * as SdkCore from '@darktower/sdk-core';

import {
  TEST_TONE_BAND_END_HZ,
  TEST_TONE_BAND_START_HZ,
} from '../../../sdk-core/src/media/setup/testTone.js';

interface FakeContext {
  sampleRate: number;
  currentTime: number;
}

const created: { fftSize: number; smoothingTimeConstant: number }[] = [];
let timeDomain: (buf: Float32Array) => void = (buf) => buf.fill(0);
let lanesWithoutOutput = false;

function fakeAnalyser(context: FakeContext) {
  const node = {
    context,
    fftSize: 2048,
    smoothingTimeConstant: 0.8,
    minDecibels: -100,
    maxDecibels: -30,
    get frequencyBinCount() {
      return node.fftSize / 2;
    },
    getFloatFrequencyData(buf: Float32Array) {
      // Bin i carries -i dB: lets the test identify exactly which bins were kept.
      for (let i = 0; i < buf.length; i += 1) buf[i] = -i;
    },
    getFloatTimeDomainData(buf: Float32Array) {
      timeDomain(buf);
    },
  };
  created.push(node);
  return node;
}

function fakeOutput(context: FakeContext) {
  // The SAME live context object the analyser reads, so time moves for both.
  const live = Object.assign(context, { createAnalyser: () => fakeAnalyser(context) });
  return {
    context: live,
    connect: vi.fn(),
    disconnect: vi.fn(),
  };
}

const ctx: FakeContext = { sampleRate: 48_000, currentTime: 0 };

vi.mock('@darktower/sdk-core', async (importOriginal) => {
  const actual = await importOriginal<typeof SdkCore>();
  return {
    ...actual,
    createAudioContextPlaybackSink: async () => ({
      openLane: () => ({
        enqueue: () => {},
        close: () => {},
        ...(lanesWithoutOutput ? {} : { output: fakeOutput(ctx) }),
      }),
      close: () => {},
    }),
  };
});

const { createE2EInstrumentation, E2E_ANALYSIS_MAX_BINS, E2E_ANALYSER_FFT_SIZE } =
  await import('../lib/e2eAnalysis.js');

afterEach(() => {
  created.length = 0;
  ctx.sampleRate = 48_000;
  ctx.currentTime = 0;
  lanesWithoutOutput = false;
  timeDomain = (buf) => buf.fill(0);
});

async function openLane(senderId = 7) {
  const inst = createE2EInstrumentation()!;
  const sink = await inst.playbackFactory({ sampleRateHz: ctx.sampleRate, channels: 1 });
  const lane = sink.openLane(senderId);
  return { inst, lane };
}

describe('e2e layer-3 analysis', () => {
  it('configures the analyser explicitly (fftSize, smoothing 0)', async () => {
    await openLane();
    expect(created[0]).toMatchObject({ fftSize: E2E_ANALYSER_FFT_SIZE, smoothingTimeConstant: 0 });
  });

  it('bandStartHz is the frequency of spectrumDb[0], and the record respects the bin bound', async () => {
    const { inst } = await openLane();
    const [record] = inst.analysis();
    const binHz = 48_000 / E2E_ANALYSER_FFT_SIZE;
    // The fake puts -i dB in bin i, so spectrumDb[0] names its own bin.
    const firstBin = -record!.spectrumDb[0]!;
    expect(record!.bandStartHz).toBeCloseTo(firstBin * binHz, 6);
    expect(record!.spectrumDb.length).toBeLessThanOrEqual(E2E_ANALYSIS_MAX_BINS);
    expect(record!.bandEndHz).toBeCloseTo(
      record!.bandStartHz + (record!.spectrumDb.length - 1) * binHz,
      6,
    );
  });

  it.each([[44_100], [48_000]])(
    'at %i Hz the EMITTED band covers every SDK tone with 3 bins of margin',
    async (rate) => {
      ctx.sampleRate = rate;
      const { inst } = await openLane();
      const [record] = inst.analysis();
      const binHz = rate / E2E_ANALYSER_FFT_SIZE;
      expect(record!.bandStartHz).toBeLessThanOrEqual(TEST_TONE_BAND_START_HZ - 3 * binHz);
      expect(record!.bandEndHz).toBeGreaterThanOrEqual(TEST_TONE_BAND_END_HZ + 3 * binHz);
    },
  );

  it('REFUSES a rate at which the band would not fit the bin bound, rather than clipping it', async () => {
    ctx.sampleRate = 16_000;
    await expect(openLane()).rejects.toThrow(/bins/);
  });

  it('levelDbfs: a full-scale sine is about -3 dBFS; silence is -Infinity', async () => {
    const { inst } = await openLane();
    expect(inst.analysis()[0]!.levelDbfs).toBe(-Infinity);
    timeDomain = (buf) => {
      for (let i = 0; i < buf.length; i += 1) buf[i] = Math.sin((2 * Math.PI * i) / 64);
    };
    expect(inst.analysis()[0]!.levelDbfs).toBeCloseTo(-3.0, 1);
  });

  it('ready is false until a full FFT window of DECODED audio has played into the lane', async () => {
    const { inst, lane } = await openLane();
    const window = E2E_ANALYSER_FFT_SIZE / ctx.sampleRate;
    // Open, silent, and plenty of time passed: still not ready.
    ctx.currentTime = 10;
    expect(inst.analysis()[0]!.ready).toBe(false);
    lane.enqueue({} as AudioData);
    ctx.currentTime = 10 + window / 2;
    expect(inst.analysis()[0]!.ready).toBe(false);
    ctx.currentTime = 10 + window;
    expect(inst.analysis()[0]!.ready).toBe(true);
  });

  it('closing a lane removes it from the analysis', async () => {
    const { inst, lane } = await openLane();
    expect(inst.analysis()).toHaveLength(1);
    lane.close();
    expect(inst.analysis()).toEqual([]);
  });

  it('a lane with no output node is a BROKEN HARNESS, not an unanalysed sender', async () => {
    lanesWithoutOutput = true;
    await expect(openLane()).rejects.toThrow(/output node/);
  });
});
