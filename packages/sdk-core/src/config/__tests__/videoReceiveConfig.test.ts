// File: packages/sdk-core/src/config/__tests__/videoReceiveConfig.test.ts
//
// The video RECEIVE bounds (story 3 C-B, R-21/R-23): exact floor boundaries,
// the reader memory ceiling, and the refusal messages naming the values
// involved, so the fix is obvious from the message alone.

import { describe, expect, it } from 'vitest';

import {
  ClientConfigError,
  DEFAULT_CLIENT_CONFIG,
  VIDEO_STREAM_CAP_HEADROOM,
  validateMediaConfig,
  videoReceiveConfigLogLine,
  type MediaConfig,
  type VideoReceiveConfig,
} from '../clientConfig.js';
import { MAX_FRAME_BYTES } from '../../media/frame/frameCodec.js';

function config(m: number, overrides: Partial<VideoReceiveConfig> = {}): MediaConfig {
  return {
    ...DEFAULT_CLIENT_CONFIG.media,
    receive: { ...DEFAULT_CLIENT_CONFIG.media.receive, videoSlots: m },
    videoReceive: { ...DEFAULT_CLIENT_CONFIG.media.videoReceive, ...overrides },
  };
}

function refusal(media: MediaConfig): ClientConfigError {
  try {
    validateMediaConfig(media);
  } catch (err) {
    if (err instanceof ClientConfigError) return err;
    throw err;
  }
  throw new Error('expected a refusal');
}

describe('the video receive defaults', () => {
  it('validate at M = 0, at dev-web M = 3, and at M = 7 (the most the default cap admits)', () => {
    for (const m of [0, 3, 7]) expect(() => validateMediaConfig(config(m))).not.toThrow();
  });

  it('refuse at M = 8 under the default cap, naming the cap, M and the floor', () => {
    const err = refusal(config(8));
    expect(err.configKey).toBe('media.videoReceive.maxIncomingStreams');
    expect(err.message).toContain('maxIncomingStreams (16)');
    expect(err.message).toContain('M = 8');
    expect(err.message).toContain(`= ${2 * 8 + VIDEO_STREAM_CAP_HEADROOM}`);
  });
});

describe('the incoming-stream floor 2 x M + c', () => {
  it('cap = 2M + c passes; 2M + c - 1 refuses', () => {
    const m = 3;
    const floor = 2 * m + VIDEO_STREAM_CAP_HEADROOM;
    expect(() => validateMediaConfig(config(m, { maxIncomingStreams: floor }))).not.toThrow();
    expect(refusal(config(m, { maxIncomingStreams: floor - 1 })).configKey).toBe(
      'media.videoReceive.maxIncomingStreams',
    );
  });
});

describe('the reader memory ceiling: cap x MAX_FRAME_BYTES', () => {
  it('product == ceiling passes; ceiling one byte lower refuses, naming both values and the product', () => {
    const cap = 10;
    const product = cap * MAX_FRAME_BYTES;
    expect(() =>
      validateMediaConfig(
        config(1, { maxIncomingStreams: cap, readerMemoryCeilingBytes: product }),
      ),
    ).not.toThrow();
    const err = refusal(
      config(1, { maxIncomingStreams: cap, readerMemoryCeilingBytes: product - 1 }),
    );
    expect(err.configKey).toBe('media.videoReceive.readerMemoryCeilingBytes');
    expect(err.message).toContain(`maxIncomingStreams (${cap})`);
    expect(err.message).toContain(`${product} bytes`);
    expect(err.message).toContain(`(${product - 1})`);
  });
});

describe('the jitter and decode bounds', () => {
  it('jitterMaxBytes == MAX_FRAME_BYTES passes; one byte less refuses', () => {
    expect(() => validateMediaConfig(config(1, { jitterMaxBytes: MAX_FRAME_BYTES }))).not.toThrow();
    expect(refusal(config(1, { jitterMaxBytes: MAX_FRAME_BYTES - 1 })).configKey).toBe(
      'media.videoReceive.jitterMaxBytes',
    );
  });

  it('decode bounds must admit the production profile', () => {
    const probe = DEFAULT_CLIENT_CONFIG.media.video.capabilityProbe;
    expect(() =>
      validateMediaConfig(
        config(1, {
          decodeMaxWidth: probe.width,
          decodeMaxHeight: probe.height,
          decodeMaxFrameRateHz: probe.frameRateHz,
        }),
      ),
    ).not.toThrow();
    for (const [key, o] of [
      ['decodeMaxWidth', { decodeMaxWidth: probe.width - 1 }],
      ['decodeMaxHeight', { decodeMaxHeight: probe.height - 1 }],
      ['decodeMaxFrameRateHz', { decodeMaxFrameRateHz: probe.frameRateHz - 1 }],
    ] as const) {
      const err = refusal(config(1, o));
      expect(err.configKey).toBe(`media.videoReceive.${key}`);
      expect(err.message).toContain(`media.videoReceive.${key}`);
    }
  });

  it.each([
    'maxIncomingStreams',
    'readerMemoryCeilingBytes',
    'jitterMaxFrames',
    'jitterMaxBytes',
    'jitterMaxHoldMs',
    'decoderQueueTarget',
    'decodeMaxWidth',
    'decodeMaxHeight',
    'decodeMaxFrameRateHz',
  ] as const)('%s must be a positive integer', (key) => {
    expect(refusal(config(1, { [key]: 0 })).configKey).toBe(`media.videoReceive.${key}`);
  });
});

describe('videoReceiveConfigLogLine', () => {
  it('is numbers and fixed tokens only, with provenance', () => {
    const line = videoReceiveConfigLogLine(config(3), 'configured');
    expect(line).toMatch(/^media receive config \(configured\): /);
    expect(line).toContain('videoSlots=3');
    expect(line).toContain(`maxFrameBytes=${MAX_FRAME_BYTES} (compiled)`);
    expect(line).toMatch(/^[\w ():;=.-]+$/);
  });
});
