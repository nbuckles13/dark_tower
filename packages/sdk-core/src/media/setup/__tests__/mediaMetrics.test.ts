// File: packages/sdk-core/src/media/setup/__tests__/mediaMetrics.test.ts
//
// THE LABEL SET IS THE POINT OF THIS FILE.
//
// ADR-0036 §11 bars a meeting, participant or stream dimension from every
// media-path metric, and NOTHING mechanical enforces that in TypeScript:
// `dt-guard`'s media-path deny is Rust-only, its metric-label scanner reads
// `crates/`, and `ts_pii.rs` scans only `console.*` / `logger.*` call sites. So
// the structural control is the constructors' SHAPE — two named strings (and,
// per kind, a wire media kind), never a label bag — and these assertions are
// what pin it.
//
// They are ALSO the drift guard between the two label bags in `mediaMetrics.ts`:
// the per-kind `mediaMetricLabels` (five keys) and `MediaSessionMetrics`'
// `#sessionLabels` literal (three keys). `dt-guard client-metrics-export` does
// not compare them; the exact-set assertions below do.

import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { describe, expect, it } from 'vitest';
import { InMemoryMetricsSink } from '@darktower/test-utils';

import { ALL_REJECT_REASONS, NON_DROPPING_REASON } from '../../frame/rejectReason.js';
import { loadFrameVectors } from '../../frame/__tests__/frameVectors.js';
import { CaptureFailure } from '../captureFailure.js';
import {
  MEDIA_CAPTURE_FAILURE_OUTCOMES,
  MEDIA_KEYFRAME_REQUEST_OUTCOMES,
  MEDIA_KEYFRAME_SOURCES,
  MEDIA_SEND_GROUP_STOP_OUTCOMES,
  MEDIA_KEYFRAME_UNATTRIBUTED_OUTCOMES,
  MEDIA_MH_CONTROL_CHANNEL_ENDINGS,
  MEDIA_KEK_INSTALL_REFUSALS,
  CONTENT_KIND_LABEL_VALUES,
  MEDIA_KIND_LABEL_PROJECTION,
  MEDIA_KIND_LABEL_VALUES,
  MEDIA_ROSTER_KEY_CHANGES,
  MEDIA_KEK_RETENTION_ANOMALIES,
  MEDIA_KEK_SOURCES,
  MEDIA_MUTE_ACTIONS,
  MEDIA_SEND_DROP_REASONS,
  MEDIA_RECOVERY_SOURCES,
  SLOT_UNFULFILLED_BUCKETS_MS,
  FIRST_RENDER_BUCKETS_MS,
  MediaKindMetrics,
  MediaMetrics,
  MediaSessionMetrics,
  mediaMetricLabels,
  projectMediaKind,
  publishedKindMetricLabels,
  type LabelledMediaKind,
} from '../mediaMetrics.js';
import { MediaKind } from '../../../proto/dark_tower/signaling/v1/signaling_pb.js';

const IDENTITY = { clientVersion: '1.2.3', orgId: 'acme' };
const SESSION_KEYS = ['client_version', 'key_custody', 'org_id'];
const PER_KIND_KEYS = [...SESSION_KEYS, 'media_kind', 'content_kind'].sort();
/** Every wire kind the projection labels (screen included: the handle is kind-generic). */
const KINDS: readonly LabelledMediaKind[] = Object.keys(MEDIA_KIND_LABEL_PROJECTION).map(
  (k) => Number(k) as LabelledMediaKind,
);

/** Every PER-KIND emitter, each with the one discriminator it may add (or none). */
const PER_KIND_EMITTERS: [string, (m: MediaKindMetrics) => void, string | null][] = [
  ['dt_client_media_frames_sent_total', (m) => m.frameSent(), null],
  [
    'dt_client_media_send_dropped_total',
    (m) => m.sendDropped(MEDIA_SEND_DROP_REASONS.EgressQueueOverflow),
    'reason',
  ],
  ['dt_client_media_send_queue_depth', (m) => m.sendQueueDepth(3), null],
  ['dt_client_media_frames_received_total', (m) => m.frameReceived(), null],
  ['dt_client_media_frames_dropped_total', (m) => m.frameDropped('signature_invalid'), 'reason'],
  ['dt_client_media_frames_accepted_total', (m) => m.frameAccepted(), null],
  [
    'dt_client_media_key_wrap_outcomes_total',
    (m) => m.wrapOutcome('kek_generation_not_held'),
    'outcome',
  ],
  ['dt_client_media_downlink_gap_frames_total', (m) => m.downlinkGapFrames(2), null],
  ['dt_client_media_downlink_reorder_total', (m) => m.downlinkReorder(), null],
  ['dt_client_media_undeclared_stream_id_total', (m) => m.undeclaredStreamId(), null],
  ['dt_client_media_decoder_errors_total', (m) => m.decoderError(), null],
  [
    'dt_client_media_mute_transitions_total',
    (m) => m.muteTransition(MEDIA_MUTE_ACTIONS.Mute),
    'action',
  ],
  ['dt_client_media_decode_queue_dropped_total', (m) => m.decodeQueueDropped(), null],
  ['dt_client_media_receive_source_deficit_total', (m) => m.receiveSourceDeficit(2), null],
  [
    'dt_client_media_capture_failures_total',
    (m) => m.captureFailure(MEDIA_CAPTURE_FAILURE_OUTCOMES.Ended),
    'outcome',
  ],
  [
    'dt_client_media_keyframe_requests_received_total',
    (m) => m.keyframeRequestReceived(MEDIA_KEYFRAME_REQUEST_OUTCOMES.NotSending),
    'outcome',
  ],
  ['dt_client_media_send_directive_kind_missing_total', (m) => m.sendDirectiveKindMissing(), null],
  [
    'dt_client_media_keyframes_produced_total',
    (m) => m.keyframeProduced(MEDIA_KEYFRAME_SOURCES.Abandonment),
    'source',
  ],
  ['dt_client_media_send_groups_abandoned_total', (m) => m.sendGroupAbandoned(), null],
  [
    'dt_client_media_send_groups_stopped_total',
    (m) => m.sendGroupStopped(MEDIA_SEND_GROUP_STOP_OUTCOMES.OpenStalled),
    'outcome',
  ],
  ['dt_client_media_encode_queue_dropped_total', (m) => m.encodeQueueDropped(), null],
  [
    'dt_client_media_keyframe_min_interval_clamped_total',
    (m) => m.keyframeMinIntervalClamped(),
    null,
  ],
  ['dt_client_media_encoder_errors_total', (m) => m.encoderError(), null],
  [
    'dt_client_media_send_directive_encoding_refused_total',
    (m) => m.sendDirectiveEncodingRefused(),
    null,
  ],
  ['dt_client_time_to_first_media_frame_ms', (m) => m.timeToFirstMediaFrameMs(42), null],
  // Story 3 C-B: the video receive series.
  [
    'dt_client_media_recovery_requests_sent_total',
    (m) => m.recoveryRequestSent(MEDIA_RECOVERY_SOURCES.GroupAbandoned),
    'source',
  ],
  ['dt_client_media_incoming_streams_refused_total', (m) => m.incomingStreamRefused(), null],
  [
    'dt_client_media_slot_unfulfilled_duration_ms',
    (m) => m.slotUnfulfilledDurationMs(120_000),
    null,
  ],
  ['dt_client_media_slots_unfulfilled', (m) => m.slotsUnfulfilled(2), null],
  ['dt_client_media_active_source_intervals_total', (m) => m.activeSourceIntervals(3), null],
  ['dt_client_media_time_to_first_render_ms', (m) => m.timeToFirstRenderMs(500), null],
  ['dt_client_media_slot_active_to_first_render_ms', (m) => m.slotActiveToFirstRenderMs(250), null],
];

/** Every SESSION-LEVEL (kind-free) emitter. */
const SESSION_EMITTERS: [string, (m: MediaSessionMetrics) => void, string | null][] = [
  [
    'dt_client_media_kek_updates_total',
    (m) => m.kekUpdate(MEDIA_KEK_SOURCES.JoinResponse),
    'source',
  ],
  ['dt_client_media_kek_retention_violations_total', (m) => m.kekRetentionViolation(), null],
  [
    'dt_client_media_kek_retention_anomalies_total',
    (m) => m.kekRetentionAnomaly(MEDIA_KEK_RETENTION_ANOMALIES.CeilingClamped),
    'outcome',
  ],
  [
    'dt_client_media_kek_install_refusals_total',
    (m) => m.kekInstallRefused(MEDIA_KEK_INSTALL_REFUSALS.OlderGeneration),
    'outcome',
  ],
  ['dt_client_media_kek_generations_retained_total', (m) => m.kekGenerationRetained(), null],
  [
    'dt_client_media_roster_key_rebinds_total',
    (m) => m.rosterKeyRebind(MEDIA_ROSTER_KEY_CHANGES.Downgrade),
    'outcome',
  ],
  ['dt_client_media_capture_source', (m) => m.captureSource('microphone'), 'mode'],
  [
    'dt_client_media_send_directive_header_version_refused_total',
    (m) => m.sendDirectiveHeaderVersionRefused(),
    null,
  ],
  [
    'dt_client_media_mh_control_channel_ended_total',
    (m) => m.mhControlChannelEnded(MEDIA_MH_CONTROL_CHANNEL_ENDINGS.Reset),
    'outcome',
  ],
  [
    'dt_client_media_keyframe_requests_unattributed_total',
    (m) => m.keyframeRequestUnattributed(MEDIA_KEYFRAME_REQUEST_OUTCOMES.Malformed),
    'outcome',
  ],
];

function emitEverything(metrics: MediaMetrics): void {
  for (const [, emit] of PER_KIND_EMITTERS) emit(metrics.audio);
  for (const [, emit] of SESSION_EMITTERS) emit(metrics.session);
}

describe('the emitter tables are complete (positive control)', () => {
  it('lists at least one emitter on each side', () => {
    expect(PER_KIND_EMITTERS.length, 'per-kind emitter table is EMPTY').toBeGreaterThan(0);
    expect(SESSION_EMITTERS.length, 'session emitter table is EMPTY').toBeGreaterThan(0);
  });

  it('covers every dt_client_ name in mediaMetrics.ts, each on exactly one side', () => {
    const here = dirname(fileURLToPath(import.meta.url));
    const source = readFileSync(resolve(here, '../mediaMetrics.ts'), 'utf8');
    const named = new Set(source.match(/'dt_client_[a-z0-9_]+'/g)?.map((n) => n.slice(1, -1)));
    expect(named.size, 'no dt_client_ literal found in mediaMetrics.ts').toBeGreaterThan(0);
    const perKind = PER_KIND_EMITTERS.map(([n]) => n);
    const session = SESSION_EMITTERS.map(([n]) => n);
    expect(perKind.filter((n) => session.includes(n))).toEqual([]);
    expect(new Set([...perKind, ...session])).toEqual(named);
  });
});

describe('mediaMetricLabels builds the PER-KIND set by ALLOW-LIST', () => {
  it.each(KINDS)('produces exactly the five keys for wire kind %s', (kind) => {
    expect(mediaMetricLabels(IDENTITY, kind)).toEqual({
      client_version: '1.2.3',
      org_id: 'acme',
      key_custody: 'operator',
      media_kind: MEDIA_KIND_LABEL_PROJECTION[kind].media_kind,
      content_kind: MEDIA_KIND_LABEL_PROJECTION[kind].content_kind,
    });
  });

  it('carries key custody as a LABEL, never as an end-to-end boolean', () => {
    // ADR-0036 §4/§11: media is encrypted between clients; MH, transport and
    // storage cannot read it; MC can. No metric, log, span attribute or document
    // may carry an end-to-end or zero-trust boolean, because the default
    // deployment is neither.
    const labels = mediaMetricLabels(IDENTITY, MediaKind.AUDIO);
    expect(labels.key_custody).toBe('operator');
    for (const key of Object.keys(labels)) {
      expect(key).not.toMatch(/end_to_end|e2e|zero_trust/i);
    }
  });

  it('cannot inherit an ambient key from the identity object', () => {
    // A caller holding the join label bag passes it where an identity is
    // expected: the allow-list reads two NAMED fields, so nothing else crosses.
    const ambient = { ...IDENTITY, meeting_id_hash: 'h', participant_id: 'p' };
    const sink = new InMemoryMetricsSink();
    const metrics = new MediaMetrics(ambient, sink);
    emitEverything(metrics);
    new MediaKindMetrics(ambient, MediaKind.VIDEO_CAMERA, sink).frameSent();
    const keys = new Set(sink.getRecordedMetrics().flatMap((m) => Object.keys(m.labels)));
    expect(keys.has('meeting_id_hash')).toBe(false);
    expect(keys.has('participant_id')).toBe(false);
  });
});

// The cross-artifact drift (this record vs the taxonomy projection table, MC's
// `KindLabels::from_proto` and the collector's exact rules) is `dt-guard`'s; the
// taxonomy is parsed THERE and nowhere in TypeScript. These are the runtime
// properties of the projection itself.
describe('the wire-kind label projection', () => {
  const GENERATED: readonly number[] = Object.values(MediaKind).filter(
    (v): v is number => typeof v === 'number',
  );

  it('labels EVERY generated wire kind except UNSPECIFIED — no more, no fewer', () => {
    expect(GENERATED.length, 'generated MediaKind enum has no numeric values').toBeGreaterThan(1);
    expect([...KINDS].sort()).toEqual(GENERATED.filter((v) => v !== MediaKind.UNSPECIFIED).sort());
  });

  it.each(KINDS)("projectMediaKind(%s) returns that kind's record entry", (kind) => {
    expect(projectMediaKind(kind)).toBe(MEDIA_KIND_LABEL_PROJECTION[kind]);
  });

  it('THROWS on UNSPECIFIED and on a number the enum does not name (open at runtime)', () => {
    expect(() => projectMediaKind(MediaKind.UNSPECIFIED)).toThrow(/has no metric labels/);
    expect(() => projectMediaKind(99 as MediaKind)).toThrow(/has no metric labels/);
    // Forced past the compile-time bar: the runtime throw is the second line.
    expect(
      () => new MediaKindMetrics(IDENTITY, MediaKind.UNSPECIFIED as LabelledMediaKind, undefined),
    ).toThrow();
  });

  it('a published kind projects through the one wire mapping (audio→AUDIO, video→VIDEO_CAMERA)', () => {
    expect(publishedKindMetricLabels('audio')).toBe(MEDIA_KIND_LABEL_PROJECTION[MediaKind.AUDIO]);
    expect(publishedKindMetricLabels('video')).toBe(
      MEDIA_KIND_LABEL_PROJECTION[MediaKind.VIDEO_CAMERA],
    );
  });

  it('the exported value sets are exactly the projection columns', () => {
    const all = Object.values(MEDIA_KIND_LABEL_PROJECTION);
    expect(new Set(MEDIA_KIND_LABEL_VALUES)).toEqual(new Set(all.map((l) => l.media_kind)));
    expect(new Set(CONTENT_KIND_LABEL_VALUES)).toEqual(new Set(all.map((l) => l.content_kind)));
    expect(MEDIA_KIND_LABEL_VALUES.length).toBeGreaterThan(0);
    expect(CONTENT_KIND_LABEL_VALUES.length).toBeGreaterThan(0);
  });

  it('never uses the collector rewrite sentinel', () => {
    expect([...MEDIA_KIND_LABEL_VALUES, ...CONTENT_KIND_LABEL_VALUES]).not.toContain('invalid');
  });
});

describe('every PER-KIND emission carries exactly the five-key set plus its discriminator', () => {
  for (const kind of KINDS) {
    it.each(PER_KIND_EMITTERS)(`%s (${kind})`, (name, emit, discriminator) => {
      const sink = new InMemoryMetricsSink();
      emit(new MediaKindMetrics(IDENTITY, kind, sink));
      const [record, ...rest] = sink.getRecordedMetrics();
      expect(rest).toEqual([]);
      expect(record?.name).toBe(name);
      const expected = discriminator ? [...PER_KIND_KEYS, discriminator].sort() : PER_KIND_KEYS;
      expect(Object.keys(record!.labels).sort()).toEqual(expected);
      // All or none: both kind labels, from the one projection entry.
      expect(record!.labels.media_kind).toBe(MEDIA_KIND_LABEL_PROJECTION[kind].media_kind);
      expect(record!.labels.content_kind).toBe(MEDIA_KIND_LABEL_PROJECTION[kind].content_kind);
      expect(record!.labels.key_custody).toBe('operator');
    });
  }

  it('resolves DISTINCT handles per kind: an audio emission never reads as video', () => {
    const sink = new InMemoryMetricsSink();
    const audio = new MediaKindMetrics(IDENTITY, MediaKind.AUDIO, sink);
    const video = new MediaKindMetrics(IDENTITY, MediaKind.VIDEO_CAMERA, sink);
    expect(audio.labels).not.toBe(video.labels);
    audio.frameReceived();
    audio.frameReceived();
    video.frameReceived();
    const byKind = sink
      .getRecordedMetrics()
      .map((r) => r.labels.media_kind)
      .reduce<Record<string, number>>(
        (acc, k) => ({ ...acc, [String(k)]: (acc[String(k)] ?? 0) + 1 }),
        {},
      );
    expect(byKind).toEqual({ audio: 2, video: 1 });
  });
});

describe('every SESSION-LEVEL emission is kind-free', () => {
  it.each(SESSION_EMITTERS)('%s', (name, emit, discriminator) => {
    const sink = new InMemoryMetricsSink();
    emit(new MediaSessionMetrics(IDENTITY, sink));
    const [record, ...rest] = sink.getRecordedMetrics();
    expect(rest).toEqual([]);
    expect(record?.name).toBe(name);
    const expected = discriminator ? [...SESSION_KEYS, discriminator].sort() : SESSION_KEYS;
    expect(Object.keys(record!.labels).sort()).toEqual(expected);
    expect(Object.keys(record!.labels)).not.toContain('media_kind');
    expect(Object.keys(record!.labels)).not.toContain('content_kind');
    expect(record!.labels.key_custody).toBe('operator');
  });
});

describe('the session container', () => {
  it('holds a kind-free session handle, an AUDIO and a VIDEO handle — and constructing emits NOTHING', () => {
    // Story 3 C-A2 adds the video handle. Construction must not zero-init any
    // series: a media_kind="video" series before video is DIRECTED would let
    // C-B's read-back pass vacuously.
    const recorded: unknown[] = [];
    const sink = {
      counter: (...a: unknown[]) => recorded.push(a),
      gauge: (...a: unknown[]) => recorded.push(a),
      histogram: (...a: unknown[]) => recorded.push(a),
    } as unknown as ConstructorParameters<typeof MediaMetrics>[1];
    const metrics = new MediaMetrics(IDENTITY, sink);
    expect(metrics.audio.kind).toBe(MediaKind.AUDIO);
    expect(metrics.video.kind).toBe(MediaKind.VIDEO_CAMERA);
    expect(metrics.audio.labels).toMatchObject({ media_kind: 'audio', content_kind: 'main' });
    expect(metrics.video.labels).toMatchObject({ media_kind: 'video', content_kind: 'main' });
    expect(Object.keys(metrics.session.labels)).not.toContain('media_kind');
    expect(Object.keys(metrics.session.labels)).not.toContain('content_kind');
    expect(recorded, 'constructing the handles emitted a series').toEqual([]);
  });
});

describe('every media emission carries nothing forbidden', () => {
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
    // both directions against `proto/test-vectors/frame-v3.vectors.json`. There
    // is deliberately no second hand-written list here, in the test either.
    const file = loadFrameVectors();
    const dropping = file.reject_reasons.filter((r) => r.drops_frame).map((r) => r.token);
    const sink = new InMemoryMetricsSink();
    const metrics = new MediaKindMetrics(IDENTITY, MediaKind.AUDIO, sink);
    for (const token of dropping) {
      metrics.frameDropped(token as never);
    }
    const emitted = sink.getRecordedMetrics().map((m) => String(m.labels.reason));
    expect(new Set(emitted)).toEqual(new Set(dropping));
    // Every token but one: the one left out is the non-dropping outcome.
    expect(dropping).toHaveLength(ALL_REJECT_REASONS.length - 1);
    expect(dropping).not.toContain(NON_DROPPING_REASON);
  });

  it('keeps the non-dropping outcome OFF the drop counter', () => {
    // Counting a played frame as a drop breaks
    // `received = accepted + sum(drops by reason)` in aggregate, silently, long
    // after the label set is frozen.
    const sink = new InMemoryMetricsSink();
    const metrics = new MediaKindMetrics(IDENTITY, MediaKind.AUDIO, sink);
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
      'gop_counter_exhausted',
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
    //
    // ---------------------------------------------------------------------
    // A ROSTER TEST PINS SPELLING. IT CANNOT PIN REACHABILITY.
    // ---------------------------------------------------------------------
    //
    // `not_connected` sat in this roster, in the catalog, in a runbook's
    // green-signal table and in a dashboard note — five sites asserting a live
    // fleet contract ("reads zero forever; alertable at > 0") — while NO
    // production code could increment it. Every one of those sites was green
    // throughout, because none of them asks whether the token is reachable. This
    // assertion would not have failed if the emitter had never been written, and
    // it will not fail if a future emitter is deleted.
    //
    // So a new token needs a test that drives the CONDITION and asserts the
    // counter moves. For this one that test is `counts not_connected, and faults
    // once, for a directed target it holds no transport to` in
    // `media/lifecycle/__tests__/audioPipeline.degradation.test.ts`, with the
    // per-lane variant in `media/pipeline/__tests__/egress.multiTarget.test.ts`.
    // Named in both directions so the pair is discoverable from either end.
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

describe('story-2 vocabularies', () => {
  it('keeps the story-2 outcome vocabularies pairwise DISJOINT', () => {
    // All ride `outcome`, a per-metric vocabulary with no cross-metric
    // aggregation contract. Disjoint values mean a mistaken `sum by(outcome)`
    // across them cannot silently merge, say, a refusal with a rebind.
    const sets = [
      Object.values(MEDIA_KEK_RETENTION_ANOMALIES),
      Object.values(MEDIA_KEK_INSTALL_REFUSALS),
      Object.values(MEDIA_ROSTER_KEY_CHANGES),
    ] as const;
    const all = sets.flat();
    expect(new Set(all).size).toBe(all.length);
  });

  it('carries both KEK arrival sources on the one counter', () => {
    const sink = new InMemoryMetricsSink();
    const m = new MediaSessionMetrics(IDENTITY, sink);
    m.kekUpdate(MEDIA_KEK_SOURCES.JoinResponse);
    m.kekUpdate(MEDIA_KEK_SOURCES.KekUpdate);
    expect(sink.getRecordedMetrics().map((r) => r.labels.source)).toEqual([
      'join_response',
      'kek_update',
    ]);
  });
});

describe('story 2 task 13 media metrics', () => {
  it('capture_source is a presence gauge: value 1, the active mode only', () => {
    const sink = new InMemoryMetricsSink();
    new MediaSessionMetrics(IDENTITY, sink).captureSource('microphone');
    const records = sink.getRecordedMetrics();
    expect(records).toHaveLength(1);
    expect(records[0]?.value).toBe(1);
    expect(records[0]?.labels.mode).toBe('microphone');
  });

  it('the deficit counter adds the COUNT, and 0 is a legal zero-initialisation', () => {
    const sink = new InMemoryMetricsSink();
    const m = new MediaKindMetrics(IDENTITY, MediaKind.AUDIO, sink);
    m.receiveSourceDeficit(0);
    m.receiveSourceDeficit(3);
    expect(sink.getRecordedMetrics().map((r) => r.value)).toEqual([0, 3]);
  });
});

describe('story 3 C-A1 vocabularies', () => {
  it('capture-failure outcomes are exactly the CaptureFailure vocabulary plus `ended`', () => {
    expect(new Set(Object.values(MEDIA_CAPTURE_FAILURE_OUTCOMES))).toEqual(
      new Set([...Object.values(CaptureFailure), 'ended']),
    );
  });

  it('keyframe-request outcomes are the six declared values', () => {
    expect(Object.values(MEDIA_KEYFRAME_REQUEST_OUTCOMES).sort()).toEqual(
      [
        'honoured',
        'honoured_counter_ahead',
        'malformed',
        'min_interval',
        'not_sending',
        'stale_counter',
      ].sort(),
    );
  });

  it('the unattributed outcomes are a subset of the keyframe outcomes', () => {
    for (const o of MEDIA_KEYFRAME_UNATTRIBUTED_OUTCOMES) {
      expect(Object.values(MEDIA_KEYFRAME_REQUEST_OUTCOMES)).toContain(o);
    }
  });

  it('no new vocabulary spells the collector sentinel `invalid`', () => {
    for (const v of [
      ...Object.values(MEDIA_CAPTURE_FAILURE_OUTCOMES),
      ...Object.values(MEDIA_KEYFRAME_REQUEST_OUTCOMES),
    ]) {
      expect(v).not.toBe('invalid');
    }
  });

  it('a zero-initialisation emits a 0-valued series with the kind-free label set', () => {
    const sink = new InMemoryMetricsSink();
    const metrics = new MediaMetrics(IDENTITY, sink);
    metrics.session.sendDirectiveHeaderVersionRefused(0);
    metrics.session.keyframeRequestUnattributed(MEDIA_KEYFRAME_REQUEST_OUTCOMES.NotSending, 0);
    const recorded = sink.getRecordedMetrics();
    expect(recorded.map((r) => r.value)).toEqual([0, 0]);
    expect(Object.keys(recorded[0]!.labels).sort()).toEqual(SESSION_KEYS);
  });
});

describe('video receive series (story 3 C-B)', () => {
  it('the unfulfilled histogram declares its boundaries from the SSoT const, with a bucket above 100 s', () => {
    const sink = new InMemoryMetricsSink();
    const m = new MediaKindMetrics(IDENTITY, MediaKind.VIDEO_CAMERA, sink);
    m.slotUnfulfilledDurationMs(5);
    const [obs] = sink.getRecordedMetrics();
    expect(obs?.boundaries).toEqual(SLOT_UNFULFILLED_BUCKETS_MS);
    expect(SLOT_UNFULFILLED_BUCKETS_MS.some((b) => b > 100_000)).toBe(true);
    // Sorted strictly ascending, as OTel requires.
    expect([...SLOT_UNFULFILLED_BUCKETS_MS].sort((a, b) => a - b)).toEqual(
      SLOT_UNFULFILLED_BUCKETS_MS,
    );
    expect(new Set(SLOT_UNFULFILLED_BUCKETS_MS).size).toBe(SLOT_UNFULFILLED_BUCKETS_MS.length);
  });

  it('both first-render histograms declare the first-render boundaries, past the 10 s maximum keyframe interval', () => {
    const sink = new InMemoryMetricsSink();
    const m = new MediaKindMetrics(IDENTITY, MediaKind.VIDEO_CAMERA, sink);
    m.timeToFirstRenderMs(1);
    m.slotActiveToFirstRenderMs(1);
    for (const obs of sink.getRecordedMetrics())
      expect(obs.boundaries).toEqual(FIRST_RENDER_BUCKETS_MS);
    expect(Math.max(...FIRST_RENDER_BUCKETS_MS)).toBeGreaterThan(10_000);
  });

  it('the recovery source vocabulary is closed: exactly the six reasons, no `invalid` sentinel', () => {
    expect(Object.values(MEDIA_RECOVERY_SOURCES).sort()).toEqual(
      [
        'decoder_error',
        'decrypt_failed',
        'group_abandoned',
        'local_drop',
        'server_mute_lifted',
        'verify_failed',
      ].sort(),
    );
  });
});
