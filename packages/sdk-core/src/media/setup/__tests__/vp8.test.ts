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
