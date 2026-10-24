// File: packages/sdk-core/src/media/lifecycle/__tests__/videoReceiver.test.ts
//
// The video receive path end to end at the node tier (story 3 C-B): MockWebTransport
// incoming uni streams -> the per-handler acceptor -> the reader -> the ONE
// policy -> M slot pipelines -> sinks. Real crypto and the REAL receiver wiring
// (KEK holder, generation-drop listener, rebind purge); only VP8 is faked.
//
// Integration tests the task names: exactly M pipelines whatever the sender
// count, a fault on one slot never stalls another, a video LEAVER cannot open
// post-rotation video, a replayed earlier WHOLE group is rejected and never
// re-rendered, the rebind purge drops keys and keeps replay state, and the
// unfulfilled gauge returns to 0 on every teardown path.

import { afterEach, beforeAll, beforeEach, describe, expect, it } from 'vitest';
import { FakeVideoDecoders, FakeVideoFrameRegistry, MockWebTransport } from '@darktower/test-utils';

import { DEFAULT_CLIENT_CONFIG, type MediaConfig } from '../../../config/clientConfig.js';
import type { SlotStateToken } from '../../../signaling/events.js';
import { AES_256_KEY_BYTES } from '../../frame/sframe.js';
import {
  emptyKekHolder,
  kekHolderWith,
  NOMINAL_DEBOUNCE_SECONDS,
} from '../../__tests__/helpers.js';
import type { MhRecoveryRequest } from '../../events.js';
import type { VideoRenderState } from '../../pipeline/renderDecision.js';
import { BoundedReceiveVerificationRecorder } from '../../pipeline/receiveVerification.js';
import { MEDIA_RECOVERY_SOURCES } from '../../setup/mediaMetrics.js';
import { RosterIdentityKeys } from '../../setup/rosterKeys.js';
import type { MeetingKekHolder } from '../../setup/kekSource.js';
import type { VideoDecoderFactory } from '../../setup/seams.js';
import type { MediaFault } from '../AudioPipeline.js';
import { VideoReceiver } from '../VideoReceiver.js';
import { InMemoryMetricsSink } from '@darktower/test-utils';
import { ReplayWindow, TransmitKeyCache } from '../../frame/receivePath.js';
import { gateFor } from '../../__tests__/helpers.js';
import { FirstMediaObserver } from '../../setup/measurement.js';
import { MEDIA_KIND_LABELS, MediaKindMetrics } from '../../setup/mediaMetrics.js';
import { HopSequenceMonitor } from '../../pipeline/hopSequenceMonitor.js';
import { IngressPipeline } from '../../pipeline/ingress.js';
import { buildTestFrame } from '../../pipeline/__tests__/frameFixtures.js';
import { makeIdentity, type TestIdentity } from '../../pipeline/__tests__/frameFixtures.js';
import {
  KEK,
  counterSum,
  dropsByReason,
  expectIdentity,
  flush,
  manualClock,
  videoFrame,
  videoMetrics,
} from '../../pipeline/__tests__/videoFixtures.js';

const URL = 'https://mh-0.example';
const SELF = 5;
const SENDERS = [601, 602, 603, 604];
const VIDEO_SLOTS = [3, 4];

let ids: TestIdentity[];
let registry: FakeVideoFrameRegistry;

beforeAll(async () => {
  ids = await Promise.all(SENDERS.map((_, i) => makeIdentity(0x70 + i)));
});

beforeEach(() => {
  registry = new FakeVideoFrameRegistry();
});

afterEach(() => {
  expect(registry.notClosedExactlyOnce()).toEqual([]);
});

interface Options {
  readonly config?: MediaConfig;
  readonly kek?: MeetingKekHolder;
  readonly slots?: readonly number[];
  /** Every recovery-request write fails. */
  readonly sendFails?: boolean;
}

async function harness(opts: Options = {}) {
  const { metrics, sink } = videoMetrics();
  const clock = manualClock();
  const mock = new MockWebTransport();
  const decoders = new FakeVideoDecoders({ registry });
  const roster = new RosterIdentityKeys(16);
  for (let i = 0; i < SENDERS.length; i += 1) {
    await roster.upsert({
      senderId: SENDERS[i] as number,
      identityPublicKey: (ids[i] as TestIdentity).publicKey,
    });
  }
  const recovery: { url: string; request: MhRecoveryRequest }[] = [];
  const renders: { slotId: number; state: VideoRenderState }[] = [];
  const faults: MediaFault[] = [];
  const logs: string[] = [];
  const muted = new Set<number>();
  const unmapped = new Set<number>();
  let tick: (() => void) | undefined;
  const recorder = new BoundedReceiveVerificationRecorder();
  const receiver = new VideoReceiver({
    config: opts.config ?? DEFAULT_CLIENT_CONFIG.media,
    configSource: 'default',
    metrics,
    kekSource: opts.kek ?? kekHolderWith(KEK, 0),
    roster,
    selfSenderId: SELF,
    videoMuteState: (s) => (unmapped.has(s) ? 'unmapped' : muted.has(s) ? 'muted' : 'unmuted'),
    declaredSlotIds: opts.slots ?? VIDEO_SLOTS,
    incomingStreamsFor: (url) => (url === URL ? mock.incomingUnidirectionalStreams : undefined),
    sendRecoveryRequest: async (url, request) => {
      if (opts.sendFails) throw new Error('control stream gone');
      recovery.push({ url, request });
    },
    decoderFactory: decoders.factory as unknown as VideoDecoderFactory,
    metricExportIntervalMs: 10_000,
    fault: (f) => faults.push(f),
    onRenderChanged: (slotId, state) => renders.push({ slotId, state }),
    verification: recorder,
    info: (line) => logs.push(line),
    clock: clock.now,
    setInterval: (fn) => {
      tick = fn;
      return 0 as unknown as ReturnType<typeof setInterval>;
    },
    clearInterval: () => {
      tick = undefined;
    },
  });
  const drawn = new Map<number, number>();
  const assign = (
    states: Record<number, SlotStateToken>,
    participants: Record<number, number> = {},
  ) =>
    receiver.setAssignments(
      Object.entries(states).map(([slot, slotState]) => ({
        slotId: Number(slot),
        slotState,
        senderId: participants[Number(slot)],
        mediaHandlerUrl: URL,
      })),
    );
  /** Deliver frames on ONE new group stream (FIN), and wait until processed. */
  const deliver = async (frames: readonly Uint8Array[]): Promise<void> => {
    const handle = mock.simulateIncomingUniStream();
    for (const f of frames) handle.enqueue(f);
    handle.close();
    await flush();
    await receiver.settled();
  };
  const drain = (): void => {
    for (const d of decoders.decoders)
      while (!d.closed && d.delivered < d.decoded.length) d.emitOutput();
  };
  const sinkFor = (slotId: number) => ({
    drawFrame: () => drawn.set(slotId, (drawn.get(slotId) ?? 0) + 1),
  });
  return {
    receiver,
    roster,
    sink,
    clock,
    mock,
    decoders,
    recovery,
    renders,
    faults,
    logs,
    muted,
    unmapped,
    drawn,
    recorder,
    assign,
    deliver,
    drain,
    sinkFor,
    tick: () => tick?.(),
  };
}

const key = (sender: number, slot: number, gop: number, seq: number, extra = {}) =>
  videoFrame({
    sender,
    signer: ids[SENDERS.indexOf(sender)] as TestIdentity,
    slot,
    gop,
    seq,
    key: true,
    ...extra,
  });
const delta = (sender: number, slot: number, gop: number, seq: number, extra = {}) =>
  videoFrame({
    sender,
    signer: ids[SENDERS.indexOf(sender)] as TestIdentity,
    slot,
    gop,
    seq,
    key: false,
    ...extra,
  });

const [A, B, C, D] = SENDERS as [number, number, number, number];

describe('VideoReceiver — start', () => {
  it('logs the receive config ONCE and zero-initialises its series', async () => {
    const h = await harness();
    h.receiver.start();
    h.receiver.start();
    expect(h.logs).toHaveLength(1);
    expect(h.logs[0]).toContain('maxIncomingStreams=16');
    for (const reason of Object.values(MEDIA_RECOVERY_SOURCES)) {
      expect(
        h.sink
          .getRecordedMetrics()
          .some(
            (m) =>
              m.name === 'dt_client_media_recovery_requests_sent_total' &&
              m.labels['source'] === reason &&
              m.value === 0,
          ),
      ).toBe(true);
    }
    await h.receiver.stop();
  });

  it('builds EXACTLY M decoders whatever the sender count; a source switch builds none', async () => {
    const h = await harness();
    h.receiver.start();
    expect(h.decoders.constructions).toBe(VIDEO_SLOTS.length);
    h.assign({ 3: 'active', 4: 'active' });
    // Four senders through two slots, switching sources on both.
    await h.deliver([await key(A, 3, 1, 1)]);
    await h.deliver([await key(B, 4, 1, 1)]);
    await h.deliver([await key(C, 3, 1, 1)]);
    await h.deliver([await key(D, 4, 1, 1)]);
    expect(h.decoders.constructions).toBe(VIDEO_SLOTS.length);
    expect(counterSum(h.sink, 'dt_client_media_frames_accepted_total')).toBe(4);
    expectIdentity(h.sink, h.receiver.frameCounts);
    h.drain();
    await h.receiver.stop();
  });
});

describe('VideoReceiver — render, sinks and evidence', () => {
  it('draws a live slot through its sink, attributed to the verified sender, and pairs it', async () => {
    const h = await harness();
    h.receiver.start();
    h.receiver.setVideoSink(3, h.sinkFor(3));
    h.assign({ 3: 'active', 4: 'avatar_no_video' }, { 4: B });
    await h.deliver([await key(A, 3, 1, 1)]);
    h.drain();
    expect(h.drawn.get(3)).toBe(1);
    expect(h.renders.find((r) => r.slotId === 3)?.state).toEqual({ kind: 'active', senderId: A });
    expect(h.receiver.liveSenders()).toEqual(new Map([[3, A]]));
    // R-32 evidence: counts, closed tokens, booleans + the two allowed ids.
    const evidence = h.receiver.evidence();
    expect(evidence).toEqual([
      {
        slotId: 3,
        mcState: 'active',
        render: 'active',
        lastRenderedSender: A,
        unfulfilled: false,
        framesDecoded: 1,
      },
      {
        slotId: 4,
        mcState: 'avatar_no_video',
        render: 'avatar',
        lastRenderedSender: undefined,
        unfulfilled: false,
        framesDecoded: 0,
      },
    ]);
    for (const row of evidence) {
      expect(Object.keys(row).sort()).toEqual(
        [
          'framesDecoded',
          'lastRenderedSender',
          'mcState',
          'render',
          'slotId',
          'unfulfilled',
        ].sort(),
      );
      for (const value of Object.values(row)) {
        expect(['number', 'string', 'boolean', 'undefined']).toContain(typeof value);
      }
    }
    await h.receiver.stop();
  });

  it('first-render latencies: media start -> first render once; ACTIVE -> first render per entry', async () => {
    const h = await harness();
    h.receiver.start();
    h.receiver.setVideoSink(3, h.sinkFor(3));
    h.clock.advance(300);
    h.assign({ 3: 'active' });
    h.clock.advance(200);
    await h.deliver([await key(A, 3, 1, 1)]);
    h.drain();
    const obs = (name: string) =>
      h.sink
        .getRecordedMetrics()
        .filter((m) => m.name === name)
        .map((m) => m.value);
    expect(obs('dt_client_media_time_to_first_render_ms')).toEqual([500]);
    expect(obs('dt_client_media_slot_active_to_first_render_ms')).toEqual([200]);
    await h.receiver.stop();
  });

  it('decode activity: the tick counts DISTINCT verified senders that decoded this interval', async () => {
    const h = await harness();
    h.receiver.start();
    h.assign({ 3: 'avatar_client_muted', 4: 'avatar_client_muted' }, { 3: A, 4: B });
    await h.deliver([await key(A, 3, 1, 1), await delta(A, 3, 1, 2, { keyBearing: true })]);
    await h.deliver([await key(B, 4, 1, 1)]);
    h.drain();
    h.tick();
    h.tick(); // nothing decoded in the second interval
    const values = h.sink
      .getRecordedMetrics()
      .filter((m) => m.name === 'dt_client_media_active_source_intervals_total')
      .map((m) => m.value);
    expect(values).toEqual([0, 2]);
    await h.receiver.stop();
  });
});

describe('VideoReceiver — stream cap and isolation', () => {
  it('refuses a stream over the concurrency cap: cancelled unread, counted, never queued', async () => {
    const config: MediaConfig = {
      ...DEFAULT_CLIENT_CONFIG.media,
      receive: { ...DEFAULT_CLIENT_CONFIG.media.receive, videoSlots: 1 },
      videoReceive: { ...DEFAULT_CLIENT_CONFIG.media.videoReceive, maxIncomingStreams: 4 },
    };
    const h = await harness({ config, slots: [3] });
    h.receiver.start();
    h.assign({ 3: 'active' });
    const open = [0, 1, 2, 3].map(() => h.mock.simulateIncomingUniStream());
    const refused = h.mock.simulateIncomingUniStream();
    await flush();
    expect(counterSum(h.sink, 'dt_client_media_incoming_streams_refused_total')).toBe(1);
    const info = h.mock.getInboundUniStreams();
    expect(info[refused.index]?.cancelled).toBe(true);
    expect(info.filter((s) => s.cancelled)).toHaveLength(1);
    for (const s of open) s.close();
    await flush();
    await h.receiver.settled();
    await h.receiver.stop();
  });

  it('a decoder fault on one slot never stalls the other slot', async () => {
    const h = await harness();
    h.receiver.start();
    h.assign({ 3: 'active', 4: 'active' });
    h.receiver.setVideoSink(3, h.sinkFor(3));
    h.receiver.setVideoSink(4, h.sinkFor(4));
    await h.deliver([await key(A, 3, 1, 1)]);
    await h.deliver([await key(B, 4, 1, 1)]);
    const [d3, d4] = h.decoders.decoders;
    d3?.emitError();
    d4?.emitOutput();
    await h.deliver([await delta(B, 4, 1, 2, { keyBearing: true })]);
    d4?.emitOutput();
    expect(h.drawn.get(4)).toBe(2);
    expect(h.drawn.get(3)).toBeUndefined();
    expect(h.faults.map((f) => f.stage)).toEqual(['decoder']);
    expect(h.recovery.map((r) => r.request)).toEqual([{ slotId: 3, senderId: A, gopCounter: 1 }]);
    await h.receiver.stop();
  });
});

describe('VideoReceiver — keys and replay (R-14), through the real wiring', () => {
  it('a video LEAVER cannot open post-rotation video (positive control: pre-rotation accepted)', async () => {
    // The leaver holds generation 0 only; the meeting rotated to 1 after it left.
    const h = await harness({ kek: kekHolderWith(KEK, 0) });
    h.receiver.start();
    h.assign({ 3: 'active' });
    await h.deliver([await key(A, 3, 1, 1)]);
    expect(counterSum(h.sink, 'dt_client_media_frames_accepted_total')).toBe(1);
    const kek1 = new Uint8Array(AES_256_KEY_BYTES).fill(0x22);
    await h.deliver([
      await key(A, 3, 2, 2, {
        kek: kek1,
        kekGeneration: 1,
        generation: 1,
        transmitKey: new Uint8Array(AES_256_KEY_BYTES).fill(0x33),
      }),
    ]);
    expect(dropsByReason(h.sink).get('no_kek_for_generation')).toBe(1);
    expect(counterSum(h.sink, 'dt_client_media_frames_accepted_total')).toBe(1);
    expectIdentity(h.sink, h.receiver.frameCounts);
    h.drain();
    await h.receiver.stop();
  });

  it('a generation leaving retention takes the video scope with it (generation-drop listener)', async () => {
    const clock = manualClock(0);
    const timers: (() => void)[] = [];
    const holder = emptyKekHolder({
      clock: clock.now,
      setTimeout: (fn) => {
        timers.push(fn);
        return 0 as unknown as ReturnType<typeof setTimeout>;
      },
      clearTimeout: () => {},
    });
    holder.install(KEK, 0, NOMINAL_DEBOUNCE_SECONDS, 'join_response');
    const h = await harness({ kek: holder });
    h.receiver.start();
    h.assign({ 3: 'active' });
    const g0 = await key(A, 3, 1, 1);
    await h.deliver([g0]);
    holder.install(
      new Uint8Array(AES_256_KEY_BYTES).fill(0x22),
      1,
      NOMINAL_DEBOUNCE_SECONDS,
      'kek_update',
    );
    clock.advance(10 * 60_000);
    for (const fire of timers.splice(0)) fire();
    expect(holder.kekForGeneration(0)).toBeUndefined();
    // The listener discarded the video cache's generation-0 key WITH its replay
    // scope: a wrapless frame of that group now finds no key at all (had the
    // entry survived, it would have opened against a dropped generation's scope).
    await h.deliver([await delta(A, 3, 1, 2)]);
    expect(dropsByReason(h.sink).get('no_transmit_key')).toBe(1);
    expect(counterSum(h.sink, 'dt_client_media_frames_accepted_total')).toBe(1);
    h.drain();
    await h.receiver.stop();
  });

  it('a replayed earlier WHOLE group is rejected (high-water) and never re-rendered', async () => {
    const h = await harness();
    h.receiver.start();
    h.receiver.setVideoSink(3, h.sinkFor(3));
    h.assign({ 3: 'active' });
    const tk0 = new Uint8Array(AES_256_KEY_BYTES).fill(0x41);
    const tk1 = new Uint8Array(AES_256_KEY_BYTES).fill(0x42);
    const group0 = [
      await key(A, 3, 1, 1, { generation: 0, transmitKey: tk0 }),
      await delta(A, 3, 1, 2, { generation: 0, transmitKey: tk0 }),
      await delta(A, 3, 1, 3, { generation: 0, transmitKey: tk0 }),
    ];
    const group1 = [
      await key(A, 3, 2, 4, { generation: 1, transmitKey: tk1 }),
      await delta(A, 3, 2, 5, { generation: 1, transmitKey: tk1 }),
    ];
    await h.deliver(group0);
    h.drain();
    await h.deliver(group1);
    h.drain();
    const drawnBefore = h.drawn.get(3);
    expect(drawnBefore).toBe(5);
    await h.deliver(group0); // the captured earlier group, byte for byte
    h.drain();
    expect(dropsByReason(h.sink).get('replay_detected')).toBe(3);
    expect(h.drawn.get(3)).toBe(drawnBefore);
    expect(h.recovery).toEqual([]); // replays are expected drops: no request
    expectIdentity(h.sink, h.receiver.frameCounts);
    await h.receiver.stop();
  });

  it('the rebind purge drops video keys (keys only); replay state is retained', async () => {
    const h = await harness();
    h.receiver.start();
    h.assign({ 3: 'active' });
    const k1 = await key(A, 3, 1, 1);
    await h.deliver([k1, await delta(A, 3, 1, 2)]);
    h.receiver.purgeTransmitKeys(A);
    await h.deliver([await delta(A, 3, 1, 3)]);
    expect(dropsByReason(h.sink).get('no_transmit_key')).toBe(1);
    await h.deliver([k1]); // replay of a frame seen BEFORE the purge
    expect(dropsByReason(h.sink).get('replay_detected')).toBe(1);
    h.drain();
    await h.receiver.stop();
  });

  it('self frames and frames from a sender muted at arrival are never rendered and are counted', async () => {
    const h = await harness();
    h.receiver.start();
    h.receiver.setVideoSink(3, h.sinkFor(3));
    h.assign({ 3: 'active' });
    h.muted.add(A);
    await h.deliver([await key(A, 3, 1, 1)]);
    expect(dropsByReason(h.sink).get('sender_server_muted')).toBe(1);
    expect(h.drawn.get(3)).toBeUndefined();
    // The unmute edge re-requests the dropped frame once.
    h.muted.delete(A);
    h.receiver.videoServerMuteLifted(A);
    await flush();
    expect(h.recovery.map((r) => r.request)).toEqual([{ slotId: 3, senderId: A, gopCounter: 1 }]);
    expect(
      counterSum(
        h.sink,
        'dt_client_media_recovery_requests_sent_total',
        (l) => l['source'] === 'server_mute_lifted',
      ),
    ).toBe(1);
    expectIdentity(h.sink, h.receiver.frameCounts);
    await h.receiver.stop();
  });
});

describe('VideoReceiver — the unfulfilled series returns to 0 on every path', () => {
  const gauge = (h: Awaited<ReturnType<typeof harness>>) =>
    h.sink
      .getRecordedMetrics()
      .filter((m) => m.name === 'dt_client_media_slots_unfulfilled')
      .map((m) => m.value);
  const observations = (h: Awaited<ReturnType<typeof harness>>) =>
    h.sink
      .getRecordedMetrics()
      .filter((m) => m.name === 'dt_client_media_slot_unfulfilled_duration_ms')
      .map((m) => m.value);

  it('exit by rendered frame (delivered to the attached sink)', async () => {
    const h = await harness();
    h.receiver.start();
    h.receiver.setVideoSink(3, h.sinkFor(3));
    h.assign({ 3: 'active' });
    h.clock.advance(70);
    await h.deliver([await key(A, 3, 1, 1)]);
    h.drain();
    expect(gauge(h).at(-1)).toBe(0);
    expect(observations(h)).toEqual([70]);
    await h.receiver.stop();
  });

  it('exit by MC leaving ACTIVE', async () => {
    const h = await harness();
    h.receiver.start();
    h.assign({ 3: 'active' });
    h.clock.advance(30);
    h.assign({ 3: 'avatar_client_muted' }, { 3: A });
    expect(gauge(h)).toEqual([1, 0]);
    expect(observations(h)).toEqual([30]);
    await h.receiver.stop();
  });

  it('exit by the slot leaving the snapshot', async () => {
    const h = await harness();
    h.receiver.start();
    h.assign({ 3: 'active' });
    h.clock.advance(40);
    h.assign({ 4: 'avatar_no_video' }, { 4: B });
    expect(observations(h)).toEqual([40]);
    expect(gauge(h).at(-1)).toBe(0);
    await h.receiver.stop();
  });

  it('exit by teardown (leave / disconnect / dispose all stop the receiver): every slot observed, gauge 0', async () => {
    const h = await harness();
    h.receiver.start();
    h.assign({ 3: 'active', 4: 'active' });
    h.clock.advance(150_000);
    expect(gauge(h).at(-1)).toBe(2);
    await h.receiver.stop();
    expect(observations(h)).toEqual([150_000, 150_000]);
    expect(gauge(h).at(-1)).toBe(0);
    // The timer is stopped with the receiver.
    h.tick();
    expect(gauge(h).at(-1)).toBe(0);
  });

  it('stop records every exit and the gauge 0 SYNCHRONOUSLY, before its promise settles', async () => {
    const h = await harness();
    h.receiver.start();
    h.assign({ 3: 'active' });
    h.clock.advance(5);
    const stopping = h.receiver.stop();
    expect(observations(h)).toEqual([5]);
    expect(gauge(h).at(-1)).toBe(0);
    await stopping;
  });

  it('the export tick re-asserts the current count', async () => {
    const h = await harness();
    h.receiver.start();
    h.assign({ 3: 'active' });
    h.tick();
    expect(gauge(h)).toEqual([1, 1]);
    await h.receiver.stop();
  });
});

describe('VideoReceiver — lifecycle edges', () => {
  it('assignments that arrive BEFORE start are applied at start', async () => {
    const h = await harness();
    h.assign({ 3: 'avatar_no_video' }, { 3: A });
    expect(h.renders).toEqual([]);
    h.receiver.start();
    expect(h.renders).toEqual([
      { slotId: 3, state: { kind: 'avatar', participantSenderId: A, reason: 'avatar_no_video' } },
    ]);
    await h.receiver.stop();
  });

  it('a sink attached to a live slot draws its held frame at once', async () => {
    const h = await harness();
    h.receiver.start();
    h.assign({ 3: 'active' });
    await h.deliver([await key(A, 3, 1, 1)]);
    h.drain();
    h.receiver.setVideoSink(3, h.sinkFor(3));
    expect(h.drawn.get(3)).toBe(1);
    // ACTIVE -> ACTIVE re-emit changes nothing.
    h.assign({ 3: 'active' });
    expect(h.drawn.get(3)).toBe(1);
    await h.receiver.stop();
  });

  it('a failed recovery write is a transport fault (event-once), not counted, not retried', async () => {
    const h = await harness({ sendFails: true });
    h.receiver.start();
    h.assign({ 3: 'active', 4: 'active' });
    const mallory = await makeIdentity(0x7f);
    await h.deliver([
      await videoFrame({ sender: A, signer: mallory, slot: 3, gop: 1, seq: 1, key: true }),
    ]);
    await h.deliver([
      await videoFrame({ sender: B, signer: mallory, slot: 4, gop: 1, seq: 1, key: true }),
    ]);
    await flush();
    expect(h.faults.map((f) => f.message)).toEqual([
      'a video recovery request could not be sent to the media handler',
    ]);
    expect(
      h.sink
        .getRecordedMetrics()
        .filter((m) => m.name === 'dt_client_media_recovery_requests_sent_total')
        .every((m) => m.value === 0),
    ).toBe(true);
    await h.receiver.stop();
  });

  it('the local unfulfilled snapshot has time-in-state per declared slot', async () => {
    const h = await harness();
    h.receiver.start();
    h.assign({ 3: 'active' });
    h.clock.advance(25);
    expect(h.receiver.unfulfilledSnapshot()).toEqual([
      { slotId: 3, unfulfilled: true, sinceMs: 25 },
      { slotId: 4, unfulfilled: false, sinceMs: 0 },
    ]);
    await h.receiver.stop();
  });

  it('after stop nothing more happens: assignments, mute edges and ticks are inert', async () => {
    const h = await harness();
    h.receiver.start();
    await h.receiver.stop();
    await h.receiver.stop();
    h.assign({ 3: 'active' });
    h.receiver.videoServerMuteLifted(A);
    h.receiver.start();
    expect(h.renders).toEqual([]);
    expect(h.decoders.constructions).toBe(2);
  });
});

describe('VideoReceiver — Gate 3 coverage (self, over-bound identity, audio isolation, sink fault)', () => {
  it('a SELF-signed frame on an ACTIVE slot: counted self_sender, never drawn, no request, identity holds', async () => {
    const h = await harness();
    const self = await makeIdentity(0x7e);
    await h.roster.upsert({ senderId: SELF, identityPublicKey: self.publicKey });
    h.receiver.start();
    h.receiver.setVideoSink(3, h.sinkFor(3));
    h.assign({ 3: 'active' });
    await h.deliver([
      await videoFrame({ sender: SELF, signer: self, slot: 3, gop: 1, seq: 1, key: true }),
    ]);
    expect(dropsByReason(h.sink).get('self_sender')).toBe(1);
    expect(
      h.sink
        .getRecordedMetrics()
        .every(
          (m) =>
            m.name !== 'dt_client_media_frames_dropped_total' || m.labels['media_kind'] === 'video',
        ),
    ).toBe(true);
    expect(h.drawn.get(3)).toBeUndefined();
    expect(h.recovery).toEqual([]);
    expectIdentity(h.sink, h.receiver.frameCounts);
    await h.receiver.stop();
  });

  it('decode_over_bound on BOTH paths (key-frame header and decoded output): identity holds with the stated exclusion', async () => {
    const h = await harness();
    h.receiver.start();
    h.receiver.setVideoSink(3, h.sinkFor(3));
    h.assign({ 3: 'active' });
    // Pre-decode: an over-bound key-frame header never reaches the decoder.
    await h.deliver([await key(A, 3, 1, 1, { width: 4096, height: 2160 })]);
    // Post-decode: a decoded frame above the bounds.
    await h.deliver([await key(A, 3, 2, 2)]);
    h.decoders.decoders[0]!.emitOutput({ width: 4096, height: 2160 });
    expect(dropsByReason(h.sink).get('decode_over_bound')).toBe(2);
    expect(counterSum(h.sink, 'dt_client_media_frames_accepted_total')).toBe(2);
    expectIdentity(h.sink, h.receiver.frameCounts);
    expect(h.drawn.get(3)).toBeUndefined();
    await h.receiver.stop();
  });

  it('a video decoder fault never stalls AUDIO: the audio ingress keeps accepting, its identity holds', async () => {
    const h = await harness();
    h.receiver.start();
    h.assign({ 3: 'active' });
    const { metrics: audioMetrics, sink: audioSink } = (() => {
      const m = videoMetricsAudio();
      return m;
    })();
    const kek = kekHolderWith(KEK, 0);
    const audio = new IngressPipeline({
      metrics: audioMetrics,
      roster: h.roster,
      lanes: gateFor(A),
      keys: kek,
      cache: new TransmitKeyCache(8),
      replay: new ReplayWindow(8, 64),
      firstMedia: new FirstMediaObserver(audioMetrics, h.clock.now),
    });
    const hop = new HopSequenceMonitor([0]);
    const audioFrame = (seq: number) =>
      buildTestFrame({
        keyIdSenderId: A,
        stream: 0,
        streamSequence: seq,
        kek: KEK,
        signer: ids[0] as TestIdentity,
        plaintext: new Uint8Array([1, 2, 3]),
        streamId: 0,
        hopSequence: seq,
      });
    await audio.accept(await audioFrame(1), hop);
    await h.deliver([await key(A, 3, 1, 1)]);
    h.decoders.decoders[0]!.emitError(); // the video slot's decoder dies
    await audio.accept(await audioFrame(2), hop);
    await audio.accept(await audioFrame(2), hop); // a replay: one counted drop
    expect(audio.framesAccepted).toBe(2);
    expect(h.faults.map((f) => f.stage)).toEqual(['decoder']);
    // The audio identity at its (datagram) parse boundary: no exclusions.
    expectIdentity(audioSink, audio, []);
    await h.receiver.stop();
  });

  it('a throwing sink raises ONE sink fault (fixed text) and keeps the slot unfulfilled', async () => {
    const h = await harness();
    h.receiver.start();
    h.receiver.setVideoSink(3, {
      drawFrame: () => {
        throw new Error('embedder bug');
      },
    });
    h.assign({ 3: 'active' });
    await h.deliver([await key(A, 3, 1, 1), await delta(A, 3, 1, 2, { keyBearing: true })]);
    h.drain();
    expect(h.faults).toEqual([
      {
        stage: 'playback',
        message: "the application's video sink failed to draw a frame; that slot is not rendering",
        fatal: false,
      },
    ]);
    expect(h.receiver.evidence()[0]?.unfulfilled).toBe(true);
    await h.receiver.stop();
  });

  it('an UNMAPPED verified sender passes arrival (never sender_server_muted) and is never drawn', async () => {
    const h = await harness();
    h.receiver.start();
    h.receiver.setVideoSink(3, h.sinkFor(3));
    h.assign({ 3: 'active' });
    h.unmapped.add(A);
    await h.deliver([await key(A, 3, 1, 1)]);
    h.drain();
    expect(dropsByReason(h.sink).get('sender_server_muted')).toBeUndefined();
    expect(counterSum(h.sink, 'dt_client_media_frames_accepted_total')).toBe(1);
    expect(h.drawn.get(3)).toBeUndefined();
    await h.receiver.stop();
  });
});

function videoMetricsAudio() {
  const sink = new InMemoryMetricsSink();
  const metrics = new MediaKindMetrics(
    { clientVersion: 'v', orgId: 'o' },
    MEDIA_KIND_LABELS.Audio,
    sink,
  );
  return { metrics, sink };
}

describe('VideoReceiver — R-35 local statistics (sampled; never exported)', () => {
  it('per declared slot: exact packets, bytes, drops by reason, decoded, keyframes; undeclared ids are not tallied', async () => {
    const h = await harness();
    h.receiver.start();
    h.assign({ 3: 'active', 4: 'active' });
    const k = await key(A, 3, 1, 1);
    const d = await delta(A, 3, 1, 2);
    await h.deliver([k, d]);
    // The SAME keyframe again: a replay, dropped and attributed to slot 3.
    await h.deliver([k]);
    // A stream naming an UNDECLARED slot: counted by the kind totals only.
    await h.deliver([await key(B, 9, 1, 1)]);
    h.drain();
    const rows = h.receiver.localStats();
    expect(rows.map((r) => r.slotId)).toEqual([3, 4]);
    const three = rows[0]!;
    expect(three).toMatchObject({
      kind: 'video',
      slotId: 3,
      packets: 3,
      bytes: k.length * 2 + d.length,
      framesDecoded: 2,
      queueDepth: 0,
      localDrops: 0,
      keyframesSeen: 1,
      unfulfilledMs: 0,
    });
    expect(Object.entries(three.drops).filter(([, n]) => n > 0)).toEqual([['replay_detected', 1]]);
    const four = rows[1]!;
    expect(four).toMatchObject({ packets: 0, bytes: 0, framesDecoded: 0, keyframesSeen: 0 });
    expect(Object.values(four.drops).every((n) => n === 0)).toBe(true);
    // A FRESH object per read: mutating one snapshot cannot reach the next.
    (three.drops as Record<string, number>)['replay_detected'] = 99;
    expect(h.receiver.localStats()[0]!.drops.replay_detected).toBe(1);
    await h.receiver.stop();
  });

  it('carries the unfulfilled time-in-state from the existing local snapshot', async () => {
    const h = await harness();
    h.receiver.start();
    h.assign({ 3: 'active' });
    h.clock.advance(1_250);
    expect(h.receiver.localStats().find((r) => r.slotId === 3)?.unfulfilledMs).toBe(1_250);
    await h.receiver.stop();
  });
});
