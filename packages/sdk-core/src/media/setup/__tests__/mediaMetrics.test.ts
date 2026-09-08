// File: packages/sdk-core/src/media/setup/__tests__/mediaMetrics.test.ts
//
// THE LABEL SET IS THE POINT OF THIS FILE.
//
// ADR-0036 §11 bars a meeting, participant or stream dimension from every
// media-path metric, and NOTHING mechanical enforces that in TypeScript:
// `dt-guard`'s media-path deny is Rust-only, its metric-label scanner reads
// `crates/`, and `ts_pii.rs` scans only `console.*` / `logger.*` call sites. So
// the structural control is the constructor's SHAPE — two named strings, not a
// label bag — and these assertions are what pin it.

import { describe, expect, it } from 'vitest';
import { InMemoryMetricsSink } from '@darktower/test-utils';

import { ALL_REJECT_REASONS, NON_DROPPING_REASON } from '../../frame/rejectReason.js';
import { loadFrameVectors } from '../../frame/__tests__/frameVectors.js';
import {
  MEDIA_KEK_SOURCES,
  MEDIA_MUTE_ACTIONS,
  MEDIA_SEND_DROP_REASONS,
  MediaMetrics,
  mediaMetricLabels,
} from '../mediaMetrics.js';

const IDENTITY = { clientVersion: '1.2.3', orgId: 'acme' };

function emitEverything(metrics: MediaMetrics): void {
  metrics.frameSent();
  metrics.sendDropped(MEDIA_SEND_DROP_REASONS.EgressQueueOverflow);
  metrics.sendQueueDepth(3);
  metrics.frameReceived();
  metrics.frameDropped('signature_invalid');
  metrics.frameAccepted();
  metrics.wrapOutcome('kek_generation_not_held');
  metrics.wrapOutcome('wrap_key_id_mismatch');
  metrics.downlinkGapFrames(2);
  metrics.downlinkReorder();
  metrics.undeclaredStreamId();
  metrics.decoderError();
  metrics.muteTransition(MEDIA_MUTE_ACTIONS.Mute);
  metrics.kekUpdate(MEDIA_KEK_SOURCES.JoinResponse);
  metrics.timeToFirstMediaFrameMs(42);
}

describe('mediaMetricLabels builds the set by ALLOW-LIST', () => {
  it('produces exactly client_version, org_id and key_custody', () => {
    expect(mediaMetricLabels(IDENTITY)).toEqual({
      client_version: '1.2.3',
      org_id: 'acme',
      key_custody: 'operator',
    });
  });

  it('carries key custody as a LABEL, never as an end-to-end boolean', () => {
    // ADR-0036 §4/§11: media is encrypted between clients; MH, transport and
    // storage cannot read it; MC can. No metric, log, span attribute or document
    // may carry an end-to-end or zero-trust boolean, because the default
    // deployment is neither.
    const labels = mediaMetricLabels(IDENTITY);
    expect(labels.key_custody).toBe('operator');
    for (const key of Object.keys(labels)) {
      expect(key).not.toMatch(/end_to_end|e2e|zero_trust/i);
    }
  });
});

describe('every media emission carries the base set and nothing forbidden', () => {
  it('emits label KEYS that set-equal the base set plus at most one discriminator', () => {
    const sink = new InMemoryMetricsSink();
    emitEverything(new MediaMetrics(IDENTITY, sink));
    const records = sink.getRecordedMetrics();
    expect(records.length).toBe(15);

    for (const record of records) {
      const keys = new Set(Object.keys(record.labels));
      expect(keys.has('client_version')).toBe(true);
      expect(keys.has('org_id')).toBe(true);
      expect(record.labels.key_custody).toBe('operator');
      // At most ONE bounded discriminator beyond the base three.
      expect(keys.size).toBeLessThanOrEqual(4);
      const extra = [...keys].filter(
        (k) => !['client_version', 'org_id', 'key_custody'].includes(k),
      );
      for (const key of extra) {
        expect(['reason', 'outcome', 'action', 'source']).toContain(key);
      }
    }
  });

  it('never emits meeting_id_hash, a participant id, or a stream id', () => {
    // The negative half, and the one that matters. `reason` x `meeting_id_hash`
    // is an oracle over the receiver's key state: the reject vocabulary
    // faithfully mirrors where in the crypto stack a frame died, so joined to a
    // per-target dimension it tells an injecting party which layer rejected each
    // probe. And in a two-person meeting a per-meeting series is nearly
    // per-stream, which is the voice-activity trace.
    const sink = new InMemoryMetricsSink();
    emitEverything(new MediaMetrics(IDENTITY, sink));
    const keys = new Set(sink.getRecordedMetrics().flatMap((m) => Object.keys(m.labels)));
    for (const forbidden of [
      'meeting_id_hash',
      'meeting_id',
      'participant_id',
      'sender_id',
      'stream_id',
      'slot_id',
      'key_id',
      'kek_generation',
      'user_id',
      'email',
    ]) {
      expect(keys.has(forbidden), `${forbidden} must never label a media metric`).toBe(false);
    }
  });

  it('names every metric under the dt_client_ prefix', () => {
    const sink = new InMemoryMetricsSink();
    emitEverything(new MediaMetrics(IDENTITY, sink));
    for (const record of sink.getRecordedMetrics()) {
      expect(record.name).toMatch(/^dt_client_[a-z]([a-z0-9_]{0,52}[a-z0-9])?$/);
    }
  });

  it('is a no-op when no sink is configured', () => {
    // Telemetry is opt-in; an unconfigured SDK must not throw from the hot path.
    expect(() => emitEverything(new MediaMetrics(IDENTITY, undefined))).not.toThrow();
  });
});

describe('the drop counter carries the frozen reject vocabulary', () => {
  it('accepts every DROPPING token and no others', () => {
    // The tokens come from `rejectReason.ts`, which is set-equality pinned in
    // both directions against `proto/test-vectors/frame-v2.vectors.json`. There
    // is deliberately no second hand-written list here, in the test either.
    const file = loadFrameVectors();
    const dropping = file.reject_reasons.filter((r) => r.drops_frame).map((r) => r.token);
    const sink = new InMemoryMetricsSink();
    const metrics = new MediaMetrics(IDENTITY, sink);
    for (const token of dropping) {
      metrics.frameDropped(token as never);
    }
    const emitted = sink.getRecordedMetrics().map((m) => String(m.labels.reason));
    expect(new Set(emitted)).toEqual(new Set(dropping));
    // Fifteen of sixteen. The sixteenth is the non-dropping outcome.
    expect(dropping).toHaveLength(ALL_REJECT_REASONS.length - 1);
    expect(dropping).not.toContain(NON_DROPPING_REASON);
  });

  it('keeps the non-dropping outcome OFF the drop counter', () => {
    // Counting a played frame as a drop breaks
    // `received = accepted + sum(drops by reason)` in aggregate, silently, long
    // after the label set is frozen.
    const sink = new InMemoryMetricsSink();
    const metrics = new MediaMetrics(IDENTITY, sink);
    metrics.wrapOutcome(NON_DROPPING_REASON);
    const dropped = sink
      .getRecordedMetrics()
      .filter((m) => m.name === 'dt_client_media_frames_dropped_total');
    expect(dropped).toEqual([]);
    expect(sink.getRecordedMetrics().map((m) => m.name)).toEqual([
      'dt_client_media_key_wrap_outcomes_total',
    ]);
  });
});

describe('the send-drop vocabulary', () => {
  it('is bounded and has ONE home', () => {
    expect(Object.values(MEDIA_SEND_DROP_REASONS)).toEqual([
      'egress_queue_overflow',
      'transport_send_refused',
      'oversize_datagram',
      'connection_closed',
      'not_connected',
    ]);
  });

  it('mirrors the media handler spellings for the four SHARED conditions', () => {
    // Four of five deliberately match `MediaDropReason` in
    // `crates/mh-service/src/observability/metrics.rs` so `sum by(reason)`
    // compares across the two ends of one hop. `not_connected` is client-only:
    // MH never initiates, so there is no counterpart and the cross-end sum is
    // meaningful only for the shared four.
    //
    // THIS MIRROR HAS NO DRIFT GUARD, unlike the reject-reason mirror three
    // files away, which has a JSON SSoT and a both-directions set-equality test.
    // Nothing compares these five strings to anything.
    expect(MEDIA_SEND_DROP_REASONS.NotConnected).toBe('not_connected');
    expect(MEDIA_SEND_DROP_REASONS.EgressQueueOverflow).toBe('egress_queue_overflow');
    expect(MEDIA_SEND_DROP_REASONS.TransportSendRefused).toBe('transport_send_refused');
    expect(MEDIA_SEND_DROP_REASONS.OversizeDatagram).toBe('oversize_datagram');
    expect(MEDIA_SEND_DROP_REASONS.ConnectionClosed).toBe('connection_closed');
  });

  it('has no `muted` reason — mute is not a send drop', () => {
    // While muted nothing is encoded, so nothing enters the queue and nothing is
    // dropped. A `muted` reason would spike the send-drop rate on every mute.
    expect(Object.values(MEDIA_SEND_DROP_REASONS)).not.toContain('muted');
  });
});
