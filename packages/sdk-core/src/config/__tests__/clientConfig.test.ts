// File: packages/sdk-core/src/config/__tests__/clientConfig.test.ts
//
// The two RELATIONSHIP checks are the load-bearing ones here. A range check
// catches a typo; these two catch a configuration that is individually plausible
// and jointly wrong, and whose failure mode is silence rather than an error.

import { describe, expect, it } from 'vitest';

import {
  ClientConfigError,
  DEFAULT_CLIENT_CONFIG,
  DEFAULT_METRIC_EXPORT_INTERVAL_MS,
  DEFAULT_RECEIVE_AUDIO_SLOTS,
  MIN_AUDIO_ROTATION_PERIOD_MS,
  parseReceiveSlots,
  validateMediaConfig,
  type MediaConfig,
} from '../clientConfig.js';
import { DEFAULT_HOP_RESTART_BACKWARD_JUMP_FRAMES } from '../../media/pipeline/hopSequenceMonitor.js';

function withEgress(overrides: Partial<MediaConfig['egress']>): MediaConfig {
  return {
    ...DEFAULT_CLIENT_CONFIG.media,
    egress: { ...DEFAULT_CLIENT_CONFIG.media.egress, ...overrides },
  };
}

describe('the shipped defaults', () => {
  it('validate', () => {
    expect(() => validateMediaConfig(DEFAULT_CLIENT_CONFIG.media)).not.toThrow();
  });

  it('encode the ADR-0036 §3 cost assumptions', () => {
    const audio = DEFAULT_CLIENT_CONFIG.media.audio;
    expect(audio.sampleRateHz).toBe(48_000);
    expect(audio.channels).toBe(1);
    expect(audio.frameDurationMs).toBe(20);
    // 32 kbps keeps §3's arithmetic (~80 B Opus frame, ~174 B on the wire,
    // ~70 kbps) true. A 48 kbps default would silently make those figures 1.5x
    // optimistic while nothing failed.
    expect(audio.defaultBitrateBps).toBe(32_000);
    expect(audio.opusComplexity).toBe(10);
    expect(audio.opusApplication).toBe('voip');
    expect(audio.opusSignal).toBe('voice');
    // OFF, and not merely as a default: §5 makes mute an out-of-band signal
    // precisely so absence of frames is never itself a signal, and DTX
    // reintroduces exactly that inference.
    expect(audio.opusUseDtx).toBe(false);
  });

  it('express the egress bound in FRAMES, with the implied latency stated', () => {
    const { egress, audio } = DEFAULT_CLIENT_CONFIG.media;
    // ADR-0036 §1 requires this unit explicitly. N frames x 20 ms is the
    // added-latency ceiling an operator can reason about; bytes are not.
    expect(egress.maxQueueFrames * audio.frameDurationMs).toBe(200);
    expect(egress.transportOutgoingHighWaterMarkFrames * audio.frameDurationMs).toBe(40);
  });

  it('set the client metric export cadence from one named constant', () => {
    expect(DEFAULT_METRIC_EXPORT_INTERVAL_MS).toBe(10_000);
    expect(DEFAULT_CLIENT_CONFIG.telemetry.metricExportIntervalMs).toBe(
      DEFAULT_METRIC_EXPORT_INTERVAL_MS,
    );
  });
});

describe('validation throws rather than clamping', () => {
  it('rejects an application bound that does not exceed the transport ceiling', () => {
    // THE FAILURE THIS PREVENTS IS SILENT. With the pair inverted, the transport
    // queue fills first, the user agent discards silently, and every counter we
    // own reads zero — a control that applies but never fires.
    expect(() =>
      validateMediaConfig(
        withEgress({ maxQueueFrames: 2, transportOutgoingHighWaterMarkFrames: 2 }),
      ),
    ).toThrow(ClientConfigError);
  });

  it('rejects a UA age bound that competes with the application queue', () => {
    // 240 ms is exactly what both queues can hold; at or below that the user
    // agent's silent age-discard becomes a ROUTINE drop path rather than a
    // backstop, and it is invisible to both ends of the hop.
    expect(() => validateMediaConfig(withEgress({ transportOutgoingMaxAgeMs: 240 }))).toThrow(
      ClientConfigError,
    );
    expect(() => validateMediaConfig(withEgress({ transportOutgoingMaxAgeMs: 241 }))).not.toThrow();
  });

  it('names the offending key on the error', () => {
    try {
      validateMediaConfig(withEgress({ maxQueueFrames: 1 }));
      expect.unreachable('expected a ClientConfigError');
    } catch (err) {
      expect(err).toBeInstanceOf(ClientConfigError);
      expect((err as ClientConfigError).configKey).toBe('media.egress.maxQueueFrames');
    }
  });

  it('rejects a non-numeric or out-of-band bitrate rather than substituting one', () => {
    const bad = {
      ...DEFAULT_CLIENT_CONFIG.media,
      audio: { ...DEFAULT_CLIENT_CONFIG.media.audio, defaultBitrateBps: Number.NaN },
    };
    expect(() => validateMediaConfig(bad)).toThrow(ClientConfigError);
  });

  it('rejects a rotation period below the transmit-key retirement floor', () => {
    // A transmit key superseded by a rotation is overwritten at the NEXT
    // rotation, and the window that must not close early is one HKDF-extract —
    // the single `crypto.subtle.sign('HMAC', ...)` that reads the key after an
    // `await importKey`. Below the floor, a periodic rotation could close it
    // while a frame is still reading, sealing that frame under zeros while its
    // wrapped block announces the real key.
    //
    // This validates the PERIODIC half of that deferral only. `rotate()` is also
    // called on resume-from-empty and on unmute, and that interval is
    // caller-controlled — see `TransmitKeyManager.rotate`.
    const below = {
      ...DEFAULT_CLIENT_CONFIG.media,
      keys: { audioRotationPeriodMs: MIN_AUDIO_ROTATION_PERIOD_MS - 1 },
    };
    expect(() => validateMediaConfig(below)).toThrow(ClientConfigError);
    try {
      validateMediaConfig(below);
      expect.unreachable('expected a ClientConfigError');
    } catch (err) {
      expect((err as ClientConfigError).configKey).toBe('media.keys.audioRotationPeriodMs');
    }

    const at = {
      ...DEFAULT_CLIENT_CONFIG.media,
      keys: { audioRotationPeriodMs: MIN_AUDIO_ROTATION_PERIOD_MS },
    };
    expect(() => validateMediaConfig(at)).not.toThrow();
  });

  it('keeps the floor far below any plausible operational setting', () => {
    // A floor that rejected a real configuration would be a worse defect than
    // the one it prevents. ADR-0036 §4 puts audio rotation "every T" with T on
    // the order of a minute, and the shipped default is 60 s.
    expect(MIN_AUDIO_ROTATION_PERIOD_MS).toBeLessThan(
      DEFAULT_CLIENT_CONFIG.media.keys.audioRotationPeriodMs,
    );
  });

  it('rejects an out-of-range Opus complexity', () => {
    const bad = {
      ...DEFAULT_CLIENT_CONFIG.media,
      audio: { ...DEFAULT_CLIENT_CONFIG.media.audio, opusComplexity: 11 },
    };
    expect(() => validateMediaConfig(bad)).toThrow(ClientConfigError);
  });
});

describe('the downlink hop-restart bound', () => {
  function withIngress(overrides: Partial<MediaConfig['ingress']>): MediaConfig {
    return {
      ...DEFAULT_CLIENT_CONFIG.media,
      ingress: { ...DEFAULT_CLIENT_CONFIG.media.ingress, ...overrides },
    };
  }

  function configKeyOf(config: MediaConfig): string | undefined {
    try {
      validateMediaConfig(config);
      return undefined;
    } catch (err) {
      expect(err).toBeInstanceOf(ClientConfigError);
      return (err as ClientConfigError).configKey;
    }
  }

  it('ships the monitor-owned default, not a second literal', () => {
    // SSoT: the default is defined beside the class that uses it and imported
    // here, the same `config -> media` direction as the receive-path bounds.
    expect(DEFAULT_CLIENT_CONFIG.media.ingress.hopRestartBackwardJumpFrames).toBe(
      DEFAULT_HOP_RESTART_BACKWARD_JUMP_FRAMES,
    );
  });

  it('rejects a non-positive or non-integer bound, naming the key', () => {
    for (const bad of [0, -1, 1.5, Number.NaN]) {
      expect(configKeyOf(withIngress({ hopRestartBackwardJumpFrames: bad }))).toBe(
        'media.ingress.hopRestartBackwardJumpFrames',
      );
    }
  });

  it('rejects a bound at or above the serial-arithmetic half-space', () => {
    // At 2^31 a backward jump can no longer be told from a forward one, so the
    // restart rule would be meaningless. Rejected at config, not at first frame.
    expect(configKeyOf(withIngress({ hopRestartBackwardJumpFrames: 2 ** 31 }))).toBe(
      'media.ingress.hopRestartBackwardJumpFrames',
    );
    expect(configKeyOf(withIngress({ hopRestartBackwardJumpFrames: 2 ** 31 - 1 }))).toBeUndefined();
  });
});

// ---------------------------------------------------------------------------
// Story 2 R-1 / R-23: the receive-slot count N
// ---------------------------------------------------------------------------

describe('parseReceiveSlots', () => {
  it('ABSENT yields the SDK default, marked as the default', () => {
    expect(parseReceiveSlots(undefined)).toEqual({
      count: DEFAULT_RECEIVE_AUDIO_SLOTS,
      source: 'default',
    });
  });

  it.each([
    ['1', 1],
    ['3', 3],
    ['32', 32],
  ])('accepts %j', (raw, count) => {
    expect(parseReceiveSlots(raw)).toEqual({ count, source: 'configured' });
  });

  it.each([
    [''],
    ['0'],
    ['03'],
    ['-1'],
    ['+3'],
    ['3.0'],
    ['1.5'],
    [' 3'],
    ['3 '],
    ['abc'],
    ['3e2'],
    ['0x3'],
    ['99999999999999999999'],
  ])('REJECTS %j loudly — never falls back to a default', (raw) => {
    expect(() => parseReceiveSlots(raw)).toThrow(ClientConfigError);
  });
});

describe('validateMediaConfig — receive slots', () => {
  const media = DEFAULT_CLIENT_CONFIG.media;

  it('accepts N up to maxDecodeLanes', () => {
    expect(() =>
      validateMediaConfig({
        ...media,
        receive: { audioSlots: media.ingress.maxDecodeLanes },
      }),
    ).not.toThrow();
  });

  it('rejects N above maxDecodeLanes (the client-side ceiling)', () => {
    expect(() =>
      validateMediaConfig({
        ...media,
        receive: { audioSlots: media.ingress.maxDecodeLanes + 1 },
      }),
    ).toThrow(ClientConfigError);
  });

  it('rejects a non-positive N', () => {
    expect(() => validateMediaConfig({ ...media, receive: { audioSlots: 0 } })).toThrow(
      ClientConfigError,
    );
  });

  it('the default maxDecodeLanes covers the demo topology N', () => {
    // `scripts/dev-web.sh` DEMO_RECEIVE_SLOTS=3.
    expect(media.ingress.maxDecodeLanes).toBeGreaterThanOrEqual(3);
  });
});
