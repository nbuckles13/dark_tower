// File: packages/sdk-core/src/config/__tests__/kekRetention.test.ts
//
// The retention rule's ONE computation, `deriveKekRetention`, and the config
// relationships it depends on. The rule's authority is the
// `JoinResponse.kek_rotation_debounce_seconds` field comment in
// `signaling.proto`; these tests pin that the client applies it, not restate it
// as a second authority.
//
// Expected windows are READ BACK from the derivation where a value is compared,
// never written as `W / 2` here: a restated formula is a second encoding that
// goes stale with the rule. The two bounds are asserted as bounds (ordering and
// relationships), not as the numbers they happen to hold today.

import { describe, expect, it } from 'vitest';

import {
  ClientConfigError,
  DEFAULT_CLIENT_CONFIG,
  KEK_RETENTION_CEILING_MS,
  KEK_RETENTION_FLOOR_MS,
  deriveKekRetention,
  sendBufferedLatencyMs,
  transmitRewrapLatencyMs,
  validateMediaConfig,
  type MediaConfig,
} from '../clientConfig.js';

const media = DEFAULT_CLIENT_CONFIG.media;
const T = transmitRewrapLatencyMs(media);

describe('deriveKekRetention — min(W/2, ceiling), floored on a zero or absent W', () => {
  it('substitutes the FLOOR on a zero W and reports it, never hard-failing', () => {
    // A non-optional proto3 scalar: zero and absent are one observable. An MC
    // one version older does not send the field — the supported rollback.
    expect(deriveKekRetention(0, T)).toEqual({
      retentionMs: KEK_RETENTION_FLOOR_MS,
      outcome: 'floor_substituted',
    });
  });

  it('treats a non-finite or negative W as absent, not as a window', () => {
    for (const w of [Number.NaN, -1, Number.NEGATIVE_INFINITY]) {
      expect(deriveKekRetention(w, T).outcome).toBe('floor_substituted');
    }
  });

  it('CLAMPS to the ceiling when W/2 would exceed it, and says so distinctly', () => {
    // Floor-substituted and ceiling-clamped have different owners and opposite
    // expectedness; the proto requires they never share one count.
    const huge = deriveKekRetention(86_400, T);
    expect(huge).toEqual({ retentionMs: KEK_RETENTION_CEILING_MS, outcome: 'ceiling_clamped' });
    expect(huge.outcome).not.toBe(deriveKekRetention(0, T).outcome);
  });

  it("is nominal at MC's default W, and never clamps it", () => {
    const atDefault = deriveKekRetention(60, T);
    expect(atDefault.outcome).toBe('nominal');
    expect(atDefault.retentionMs).toBeLessThanOrEqual(KEK_RETENTION_CEILING_MS);
  });

  it('keeps retention strictly shorter than W at every W — structurally, by derivation', () => {
    // The property the derivation exists to make true without any cross-service
    // check: both bounds only ever pull retention DOWN from W/2. Swept across the
    // range including the clamp boundary; the floor applies only when there is
    // no W to compare against.
    for (const w of [1, 2, 5, 10, 30, 59, 60, 61, 120, 3_600, 86_400]) {
      const { retentionMs } = deriveKekRetention(w, T);
      expect(retentionMs, `W=${w}s`).toBeLessThan(w * 1_000);
    }
  });

  it('REPORTS a window at or below T but does NOT raise it', () => {
    // Raising it could push retention to or past W, breaking the structural
    // property above to paper over an MC misconfiguration. So the fault is named
    // and the remedy stays in MC.
    const tiny = deriveKekRetention(0.2, 1_000);
    expect(tiny.outcome).toBe('below_rewrap_latency');
    expect(tiny.retentionMs).toBeLessThan(1_000);
    expect(tiny.retentionMs).toBeLessThan(0.2 * 1_000);
  });

  it('treats exactly-T as below: the window must EXCEED the re-wrap latency', () => {
    const exact = deriveKekRetention(2, deriveKekRetention(2, 0).retentionMs);
    expect(exact.outcome).toBe('below_rewrap_latency');
  });
});

describe('the bounds relate to each other and to T', () => {
  it('orders FLOOR <= CEILING', () => {
    expect(KEK_RETENTION_FLOOR_MS).toBeLessThanOrEqual(KEK_RETENTION_CEILING_MS);
  });

  it('puts the FLOOR above the default T, so a floor-substituted client never drops re-wrapping peers', () => {
    expect(KEK_RETENTION_FLOOR_MS).toBeGreaterThan(T);
  });

  it('derives T from the send-side queues plus one encoded frame, never from a literal', () => {
    // T is not a knob: it is what can still leave under the old KEK after the
    // synchronous rotation on receipt — the queued frames, plus one.
    expect(T).toBe(sendBufferedLatencyMs(media) + media.audio.frameDurationMs);
    expect(T).toBeGreaterThan(0);
  });
});

describe('validateMediaConfig', () => {
  function withEgress(overrides: Partial<MediaConfig['egress']>): MediaConfig {
    return { ...media, egress: { ...media.egress, ...overrides } };
  }

  it('REJECTS queues deep enough that T reaches the retention floor, naming the relationship', () => {
    // Deep enough that the queues alone exceed the floor. The UA-age backstop is
    // raised too, so the ONLY relationship broken is the retention one.
    const frames = Math.ceil(KEK_RETENTION_FLOOR_MS / media.audio.frameDurationMs) + 1;
    const cfg = withEgress({ maxQueueFrames: frames, transportOutgoingMaxAgeMs: frames * 1_000 });
    expect(() => validateMediaConfig(cfg)).toThrow(ClientConfigError);
    expect(() => validateMediaConfig(cfg)).toThrow(/retention floor/);
    expect(() => validateMediaConfig(cfg)).toThrow(/re-wrap latency/);
  });

  it('accepts the defaults', () => {
    expect(() => validateMediaConfig(media)).not.toThrow();
  });

  it.each(['maxDecodeLanes', 'decoderRestartBackoffMs', 'decoderPendingFrames'] as const)(
    'requires ingress.%s to be a positive integer',
    (key) => {
      const cfg: MediaConfig = { ...media, ingress: { ...media.ingress, [key]: 0 } };
      expect(() => validateMediaConfig(cfg)).toThrow(`media.ingress.${key}`);
    },
  );
});
