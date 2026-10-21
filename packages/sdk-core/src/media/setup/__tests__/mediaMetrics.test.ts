// File: packages/sdk-core/src/media/setup/__tests__/mediaMetrics.test.ts
//
// THE LABEL SET IS THE POINT OF THIS FILE.
//
// ADR-0036 §11 bars a meeting, participant or stream dimension from every
// media-path metric, and NOTHING mechanical enforces that in TypeScript:
// `dt-guard`'s media-path deny is Rust-only, its metric-label scanner reads
// `crates/`, and `ts_pii.rs` scans only `console.*` / `logger.*` call sites. So
// the structural control is the constructors' SHAPE — two named strings (and,
// per kind, a closed media kind), never a label bag — and these assertions are
// what pin it.
//
// They are ALSO the drift guard between the two label bags in `mediaMetrics.ts`:
// the per-kind `mediaMetricLabels` (four keys) and `MediaSessionMetrics`'
// `#sessionLabels` literal (three keys). `dt-guard client-metrics-export` does
// not compare them; the exact-set assertions below do.

import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { describe, expect, it } from 'vitest';
import { InMemoryMetricsSink } from '@darktower/test-utils';

import { ALL_REJECT_REASONS, NON_DROPPING_REASON } from '../../frame/rejectReason.js';
import { loadFrameVectors, PENDING_P2_REJECT_TOKENS } from '../../frame/__tests__/frameVectors.js';
import {
  MEDIA_KEK_INSTALL_REFUSALS,
  MEDIA_KIND_LABELS,
  MEDIA_ROSTER_KEY_CHANGES,
  MEDIA_KEK_RETENTION_ANOMALIES,
  MEDIA_KEK_SOURCES,
  MEDIA_MUTE_ACTIONS,
  MEDIA_SEND_DROP_REASONS,
  MediaKindMetrics,
  MediaMetrics,
  MediaSessionMetrics,
  mediaMetricLabels,
  type MediaKindLabel,
} from '../mediaMetrics.js';

const IDENTITY = { clientVersion: '1.2.3', orgId: 'acme' };
const SESSION_KEYS = ['client_version', 'key_custody', 'org_id'];
const PER_KIND_KEYS = [...SESSION_KEYS, 'media_kind'].sort();
const KINDS: readonly MediaKindLabel[] = Object.values(MEDIA_KIND_LABELS);

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
  ['dt_client_media_receive_slots_rejected_total', (m) => m.receiveSlotsRejected(), null],
  ['dt_client_time_to_first_media_frame_ms', (m) => m.timeToFirstMediaFrameMs(42), null],
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
  it.each(KINDS)('produces exactly the four keys for %s', (kind) => {
    expect(mediaMetricLabels(IDENTITY, kind)).toEqual({
      client_version: '1.2.3',
      org_id: 'acme',
      key_custody: 'operator',
      media_kind: kind,
    });
  });

  it('carries key custody as a LABEL, never as an end-to-end boolean', () => {
    // ADR-0036 §4/§11: media is encrypted between clients; MH, transport and
    // storage cannot read it; MC can. No metric, log, span attribute or document
    // may carry an end-to-end or zero-trust boolean, because the default
    // deployment is neither.
    const labels = mediaMetricLabels(IDENTITY, MEDIA_KIND_LABELS.Audio);
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
    new MediaKindMetrics(ambient, MEDIA_KIND_LABELS.Video, sink).frameSent();
    const keys = new Set(sink.getRecordedMetrics().flatMap((m) => Object.keys(m.labels)));
    expect(keys.has('meeting_id_hash')).toBe(false);
    expect(keys.has('participant_id')).toBe(false);
  });
});

describe('the media_kind vocabulary matches its single source of truth', () => {
  it('equals the `media_kind` row of docs/observability/label-taxonomy.md', () => {
    const here = dirname(fileURLToPath(import.meta.url));
    const doc = readFileSync(
      resolve(here, '../../../../../../docs/observability/label-taxonomy.md'),
      'utf8',
    );
    const row = doc.split('\n').find((line) => line.startsWith('| `media_kind` |'));
    expect(row, 'no `| `media_kind` |` table row in label-taxonomy.md').toBeDefined();
    const valueCell = row!.split('|')[2] ?? '';
    const values = [...valueCell.matchAll(/`([^`]+)`/g)].map((m) => m[1]);
    expect(values.length, 'media_kind taxonomy row has no backticked values').toBeGreaterThan(0);
    expect(new Set(values)).toEqual(new Set(KINDS));
  });

  it('never uses the collector rewrite sentinel', () => {
    expect(KINDS).not.toContain('invalid');
  });
});

describe('every PER-KIND emission carries exactly the four-key set plus its discriminator', () => {
  for (const kind of KINDS) {
    it.each(PER_KIND_EMITTERS)(`%s (${kind})`, (name, emit, discriminator) => {
      const sink = new InMemoryMetricsSink();
      emit(new MediaKindMetrics(IDENTITY, kind, sink));
      const [record, ...rest] = sink.getRecordedMetrics();
      expect(rest).toEqual([]);
      expect(record?.name).toBe(name);
      const expected = discriminator ? [...PER_KIND_KEYS, discriminator].sort() : PER_KIND_KEYS;
      expect(Object.keys(record!.labels).sort()).toEqual(expected);
      expect(record!.labels.media_kind).toBe(kind);
      expect(record!.labels.key_custody).toBe('operator');
    });
  }

  it('resolves DISTINCT handles per kind: an audio emission never reads as video', () => {
    const sink = new InMemoryMetricsSink();
    const audio = new MediaKindMetrics(IDENTITY, MEDIA_KIND_LABELS.Audio, sink);
    const video = new MediaKindMetrics(IDENTITY, MEDIA_KIND_LABELS.Video, sink);
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
    expect(record!.labels.key_custody).toBe('operator');
  });
});

describe('the session container', () => {
  it('holds a kind-free session handle and an AUDIO handle only (no video yet)', () => {
    const metrics = new MediaMetrics(IDENTITY, undefined);
    expect(metrics.audio.kind).toBe('audio');
    expect(Object.keys(metrics.session.labels)).not.toContain('media_kind');
    // `in` sees accessors and inherited members, not only own enumerable keys,
    // so a `get video()` would fail here too.
    expect('video' in metrics).toBe(false);
    // Behavioural form: every per-kind handle reachable from the container
    // (own fields AND prototype getters) is the audio one. A video handle here
    // would zero-init a media_kind="video" series and pre-empt C-B's read-back.
    const reachable = [
      ...Object.getOwnPropertyNames(metrics),
      ...Object.getOwnPropertyNames(Object.getPrototypeOf(metrics)),
    ]
      .filter((k) => k !== 'constructor')
      .map((k) => (metrics as unknown as Record<string, unknown>)[k])
      .filter((v): v is MediaKindMetrics => v instanceof MediaKindMetrics);
    expect(reachable.length, 'no per-kind handle reachable: positive control').toBeGreaterThan(0);
    expect(reachable.map((h) => h.kind)).toEqual(reachable.map(() => 'audio'));
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
    // both directions against `proto/test-vectors/frame-v2.vectors.json`. There
    // is deliberately no second hand-written list here, in the test either.
    const file = loadFrameVectors();
    // Plus the tokens P2's vectors will add (all dropping); see
    // `PENDING_P2_REJECT_TOKENS` for why that list retires itself.
    const dropping = [
      ...file.reject_reasons.filter((r) => r.drops_frame).map((r) => r.token),
      ...PENDING_P2_REJECT_TOKENS,
    ];
    const sink = new InMemoryMetricsSink();
    const metrics = new MediaKindMetrics(IDENTITY, MEDIA_KIND_LABELS.Audio, sink);
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
    const metrics = new MediaKindMetrics(IDENTITY, MEDIA_KIND_LABELS.Audio, sink);
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
    const m = new MediaKindMetrics(IDENTITY, MEDIA_KIND_LABELS.Audio, sink);
    m.receiveSourceDeficit(0);
    m.receiveSourceDeficit(3);
    expect(sink.getRecordedMetrics().map((r) => r.value)).toEqual([0, 3]);
  });
});
