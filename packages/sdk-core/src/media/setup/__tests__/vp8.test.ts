// File: packages/sdk-core/src/media/setup/__tests__/vp8.test.ts
//
// Story 3 R-8: `probeVp8Support` fails LOUDLY (typed) unless BOTH encode and
// decode are supported. Independent of `packages/web-app/e2e/vp8Probe.ts` by
// design.

import { describe, expect, it } from 'vitest';

import {
  VideoCodecUnsupportedError,
  buildVp8EncoderConfig,
  probeVp8Support,
  type Vp8ProbeApi,
} from '../vp8.js';

const PROFILE = { width: 640, height: 360, frameRateHz: 30, bitrateBps: 800_000 };

function api(encoder: boolean | 'throw', decoder: boolean | 'throw'): Vp8ProbeApi {
  const answer = (v: boolean | 'throw') => async () => {
    if (v === 'throw') throw new Error('platform says no in its own words');
    return { supported: v };
  };
  return {
    VideoEncoder: { isConfigSupported: answer(encoder) } as never,
    VideoDecoder: { isConfigSupported: answer(decoder) } as never,
  };
}

describe('probeVp8Support', () => {
  it('passes only when the encoder AND the decoder both say yes', async () => {
    await expect(probeVp8Support(PROFILE, api(true, true))).resolves.toBeUndefined();
  });

  it.each([
    ['encoder', api(false, true)],
    ['encoder', api('throw', true)],
    ['decoder', api(true, false)],
    ['decoder', api(true, 'throw')],
  ] as const)('fails LOUDLY and typed at the %s stage', async (stage, platform) => {
    const err = await probeVp8Support(PROFILE, platform).catch((e: unknown) => e);
    expect(err).toBeInstanceOf(VideoCodecUnsupportedError);
    expect((err as VideoCodecUnsupportedError).stage).toBe(stage);
    // Bounded SDK text, never the platform's own message.
    expect((err as Error).message).not.toMatch(/its own words/);
  });

  it('fails as `platform` when WebCodecs is absent (the node tier itself)', async () => {
    const err = await probeVp8Support(PROFILE, {}).catch((e: unknown) => e);
    expect((err as VideoCodecUnsupportedError).stage).toBe('platform');
    const fromGlobal = await probeVp8Support(PROFILE).catch((e: unknown) => e);
    expect(fromGlobal).toBeInstanceOf(VideoCodecUnsupportedError);
  });

  it('the encoder config is VP8 at the profile with NO keyframe-interval member', () => {
    const config = buildVp8EncoderConfig(PROFILE);
    expect(config).toEqual({
      codec: 'vp8',
      width: 640,
      height: 360,
      framerate: 30,
      bitrate: 800_000,
      latencyMode: 'realtime',
    });
  });
});

describe('buildVp8DecoderConfig (story 3 C-B: every receive slot is configured from its bounds)', () => {
  it('is VP8 at the given coded size, latency-optimised, and carries nothing from a bitstream', async () => {
    const { buildVp8DecoderConfig } = await import('../vp8.js');
    expect(buildVp8DecoderConfig({ width: 1280, height: 720 })).toEqual({
      codec: 'vp8',
      codedWidth: 1280,
      codedHeight: 720,
      optimizeForLatency: true,
    });
  });
});

describe('vp8KeyframeDimensions (RFC 6386 §9.1, bounds-checked)', () => {
  const key = (w: number, h: number) =>
    Uint8Array.of(
      0x00,
      0,
      0,
      0x9d,
      0x01,
      0x2a,
      w & 0xff,
      (w >> 8) & 0x3f,
      h & 0xff,
      (h >> 8) & 0x3f,
      1,
      2,
    );

  it('reads the declared width and height of a key frame (scale bits ignored)', async () => {
    const { vp8KeyframeDimensions } = await import('../vp8.js');
    expect(vp8KeyframeDimensions(key(640, 360))).toEqual({ ok: true, width: 640, height: 360 });
    const scaled = key(1280, 720);
    scaled[7] = (scaled[7] as number) | 0xc0;
    expect(vp8KeyframeDimensions(scaled)).toEqual({ ok: true, width: 1280, height: 720 });
  });

  it('is a typed {ok: false} — never a throw or an out-of-bounds read — for short, delta or bad-start-code input', async () => {
    const { vp8KeyframeDimensions } = await import('../vp8.js');
    const full = key(640, 360);
    for (let n = 0; n < 10; n += 1)
      expect(vp8KeyframeDimensions(full.subarray(0, n))).toEqual({ ok: false });
    const delta = Uint8Array.from(full);
    delta[0] = 0x01;
    expect(vp8KeyframeDimensions(delta)).toEqual({ ok: false });
    const badStart = Uint8Array.from(full);
    badStart[4] = 0x00;
    expect(vp8KeyframeDimensions(badStart)).toEqual({ ok: false });
  });
});
