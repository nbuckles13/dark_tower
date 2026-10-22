// File: packages/sdk-core/src/media/pipeline/__tests__/videoSlotPipeline.test.ts
//
// One video receive slot (story 3 R-4, R-5, R-6, R-13): pipeline reuse across a
// source switch (zero new decoders, no flush, re-based timeline), the render
// decision in both orders, the group gate, fault isolation, decode bounds,
// recovery-request dedupe, and VideoFrame ownership (closed exactly once).
//
// The decoder is the deterministic test double: it outputs ONLY when the test
// says so. Every deadline runs on a manual clock.

import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { FakeVideoDecoders, FakeVideoFrameRegistry } from '@darktower/test-utils';

import { DEFAULT_CLIENT_CONFIG, type VideoReceiveConfig } from '../../../config/clientConfig.js';
import type { SlotStateToken } from '../../../signaling/events.js';
import { FrameAccounting } from '../frameAccounting.js';
import type { RenderGuards, VideoRenderState } from '../renderDecision.js';
import type { VideoDecoderFactory, VideoSink } from '../../setup/seams.js';
import { VideoSlotPipeline, type SlotFrame, type SlotLossEvent } from '../videoSlotPipeline.js';
import { UnfulfilledSlots } from '../../lifecycle/unfulfilledSlots.js';
import {
  counterSum,
  dropsByReason,
  manualClock,
  videoMetrics,
  vp8Delta,
  vp8Key,
} from './videoFixtures.js';

const A = 101;
const B = 202;
const SELF = 7;
const URL = 'https://mh-0.example';
const SLOT = 3;

let registry: FakeVideoFrameRegistry;
let sinkErrors = 0;
const SINK_ERR = (): void => {
  sinkErrors += 1;
};

beforeEach(() => {
  registry = new FakeVideoFrameRegistry();
  sinkErrors = 0;
});

afterEach(() => {
  // EVERY VideoFrame created in the test was closed EXACTLY once (T6).
  expect(registry.notClosedExactlyOnce()).toEqual([]);
});

interface Harness {
  readonly slot: VideoSlotPipeline;
  readonly decoders: FakeVideoDecoders;
  readonly clock: ReturnType<typeof manualClock>;
  readonly losses: SlotLossEvent[];
  readonly renders: VideoRenderState[];
  readonly drawn: { senderTimestamp: number }[];
  readonly drawnFrames: unknown[];
  readonly unfulfilled: UnfulfilledSlots;
  readonly sink: ReturnType<typeof videoMetrics>['sink'];
  readonly accounting: FrameAccounting;
  readonly muted: Set<number>;
  readonly unmapped: Set<number>;
  readonly decodedSenders: number[];
  readonly activeToRender: (number | undefined)[];
  readonly faults: number[];
  stop(): void;
}

function harness(overrides: Partial<VideoReceiveConfig> = {}): Harness {
  const decoders = new FakeVideoDecoders({ registry });
  const clock = manualClock();
  const { metrics, sink } = videoMetrics();
  const accounting = new FrameAccounting(metrics);
  const unfulfilled = new UnfulfilledSlots(metrics, clock.now);
  const losses: SlotLossEvent[] = [];
  const renders: VideoRenderState[] = [];
  const drawnFrames: unknown[] = [];
  const muted = new Set<number>();
  const unmapped = new Set<number>();
  const decodedSenders: number[] = [];
  const activeToRender: (number | undefined)[] = [];
  const faults: number[] = [];
  const guards: RenderGuards = {
    selfSenderId: SELF,
    videoMuteState: (s) => (unmapped.has(s) ? 'unmapped' : muted.has(s) ? 'muted' : 'unmuted'),
  };
  const slot = new VideoSlotPipeline({
    slotId: SLOT,
    config: { ...DEFAULT_CLIENT_CONFIG.media.videoReceive, ...overrides },
    decoderRestartBackoffMs: 1_000,
    decoderFactory: decoders.factory as unknown as VideoDecoderFactory,
    metrics,
    accounting,
    guards,
    clock: clock.now,
    unfulfilled,
    onRenderChanged: (_id, state) => renders.push(state),
    onLoss: (e) => losses.push(e),
    onDecoded: (s) => decodedSenders.push(s),
    onDrawn: (ms) => activeToRender.push(ms),
    onDecoderFault: () => faults.push(1),
    onSinkError: SINK_ERR,
  });
  const sinkImpl: VideoSink = { drawFrame: (f) => drawnFrames.push(f) };
  slot.setSink(sinkImpl);
  return {
    slot,
    decoders,
    clock,
    losses,
    renders,
    drawn: [],
    drawnFrames,
    unfulfilled,
    sink,
    accounting,
    muted,
    unmapped,
    decodedSenders,
    activeToRender,
    faults,
    stop: () => {
      slot.stop();
      unfulfilled.stop();
    },
  };
}

function frame(
  senderId: number,
  gop: number,
  key: boolean,
  extra: Partial<SlotFrame> = {},
): SlotFrame {
  return {
    senderId,
    gopCounter: gop,
    isKeyframe: key,
    data: key ? vp8Key() : vp8Delta(),
    url: URL,
    ...extra,
  };
}

function setState(h: Harness, token: SlotStateToken | undefined, participant?: number): void {
  h.slot.setMcState(token, participant);
}

describe('exactly one decoder per slot, reused across a source switch (R-6)', () => {
  it('constructs ONE decoder at construction and ZERO on a source switch; no flush; old plays out first', () => {
    const h = harness({ decoderQueueTarget: 10 });
    expect(h.decoders.constructions).toBe(1);
    setState(h, 'active');
    // A: keyframe + 2 deltas queued (decoder held back so the buffer holds them).
    const dec = h.decoders.last!;
    dec.queueSizeOverride = 99;
    h.slot.push(frame(A, 5, true));
    h.slot.push(frame(A, 5, false));
    h.slot.push(frame(A, 5, false));
    // The switch: B's keyframe arrives while A's frames are still queued.
    h.slot.push(frame(B, 9, true));
    h.slot.push(frame(B, 9, false));
    expect(h.decoders.constructions).toBe(1);
    dec.queueSizeOverride = undefined;
    h.slot.push(frame(B, 9, false)); // pumps the buffer
    // Order into the ONE decoder: A's queued frames first (no flush), then B from its keyframe.
    expect(dec.decoded.map((c) => c.isKeyframe)).toEqual([true, false, false, true, false, false]);
    expect(h.decoders.constructions).toBe(1);
    // Drain outputs; drop them through render.
    for (let i = 0; i < dec.decoded.length; i += 1) dec.emitOutput();
    h.stop();
  });

  it('re-bases the new source onto the playout timeline: exact, monotonic, spacing preserved', () => {
    const h = harness({ decodeMaxFrameRateHz: 50, decoderQueueTarget: 10 }); // nominal 20 000 us
    setState(h, 'active');
    const dec = h.decoders.last!;
    h.clock.set(10_000);
    h.slot.push(frame(A, 1, true));
    h.clock.set(10_033);
    h.slot.push(frame(A, 1, false));
    // B's first frame arrives EARLIER on its own clock than A's would continue.
    h.clock.set(10_040);
    h.slot.push(frame(B, 4, true));
    h.clock.set(10_073);
    h.slot.push(frame(B, 4, false));
    const ts = dec.decoded.map((c) => c.timestampUs);
    expect(ts).toEqual([
      10_000_000, 10_033_000,
      // re-based: A's last + one nominal interval
      10_053_000,
      // B's own 33 ms spacing preserved
      10_086_000,
    ]);
    for (let i = 0; i < ts.length; i += 1) dec.emitOutput();
    h.stop();
  });

  it('trailing frames of the old source after the new keyframe are never fed (no A deltas on B reference)', () => {
    const h = harness();
    const dec = h.decoders.last!;
    h.slot.push(frame(A, 1, true));
    h.slot.push(frame(B, 2, true));
    h.slot.push(frame(A, 1, false)); // trailing A delta
    h.slot.push(frame(B, 2, false));
    expect(dec.decoded.map((c) => c.isKeyframe)).toEqual([true, true, false]);
    expect(counterSum(h.sink, 'dt_client_media_decode_queue_dropped_total')).toBe(1);
    for (let i = 0; i < 3; i += 1) dec.emitOutput();
    h.stop();
  });

  it('security F2: a delta-FLAGGED frame whose bitstream is a VP8 key frame is discarded, never decoded', () => {
    const h = harness();
    const dec = h.decoders.last!;
    h.slot.push(frame(A, 1, true));
    h.slot.push(frame(A, 1, false, { data: vp8Key(16383, 16383) }));
    expect(dec.decoded).toHaveLength(1);
    expect(counterSum(h.sink, 'dt_client_media_decode_queue_dropped_total')).toBe(1);
    dec.emitOutput();
    h.stop();
  });

  it("a new sender's delta frames before its keyframe are discarded, never decoded", () => {
    const h = harness();
    const dec = h.decoders.last!;
    h.slot.push(frame(A, 1, true));
    h.slot.push(frame(B, 2, false));
    expect(dec.decoded).toHaveLength(1);
    dec.emitOutput();
    h.stop();
  });
});

describe('render decision, both orders (R-5)', () => {
  it('decoded BEFORE the slot turns ACTIVE: held, not drawn; drawn AT the ACTIVE transition', () => {
    const h = harness();
    setState(h, 'avatar_client_muted', A);
    h.slot.push(frame(A, 1, true));
    h.decoders.last!.emitOutput();
    expect(h.drawnFrames).toHaveLength(0);
    expect(h.renders.at(-1)).toEqual({
      kind: 'avatar',
      participantSenderId: A,
      reason: 'avatar_client_muted',
    });
    setState(h, 'active');
    expect(h.drawnFrames).toHaveLength(1);
    expect(h.renders.at(-1)).toEqual({ kind: 'active', senderId: A });
    expect(h.slot.lastRenderedSender).toBe(A);
    h.stop();
  });

  it('ACTIVE before the first frame: keeps the previous render (no spinner) until the first decode', () => {
    const h = harness();
    setState(h, 'avatar_no_video', A);
    const before = h.renders.length;
    setState(h, 'active');
    expect(h.renders.length).toBe(before); // no transition emitted
    expect(h.slot.renderState.kind).toBe('avatar');
    expect(h.unfulfilled.isUnfulfilled(SLOT)).toBe(true);
    h.slot.push(frame(B, 1, true));
    h.decoders.last!.emitOutput();
    expect(h.slot.renderState).toEqual({ kind: 'active', senderId: B });
    expect(h.unfulfilled.isUnfulfilled(SLOT)).toBe(false);
    h.stop();
  });

  it('ACTIVE -> avatar switches AT ONCE and ignores trailing frames, also on a later ACTIVE', () => {
    const h = harness();
    setState(h, 'active');
    const dec = h.decoders.last!;
    h.slot.push(frame(A, 1, true));
    h.slot.push(frame(A, 1, false));
    dec.emitOutput();
    expect(h.drawnFrames).toHaveLength(1);
    setState(h, 'avatar_server_muted', A);
    expect(h.slot.renderState.kind).toBe('avatar');
    dec.emitOutput(); // trailing A frame
    expect(h.drawnFrames).toHaveLength(1);
    setState(h, 'active'); // trailing frame never shown
    expect(h.drawnFrames).toHaveLength(1);
    // A resume keyframe (a NEWER group) is eligible.
    h.slot.push(frame(A, 2, true));
    dec.emitOutput();
    expect(h.drawnFrames).toHaveLength(2);
    h.stop();
  });

  it('attribution is the INPUT chunk sender carried through the decoder, never the current source', () => {
    const h = harness();
    setState(h, 'active');
    const dec = h.decoders.last!;
    dec.queueSizeOverride = undefined;
    h.slot.push(frame(A, 1, true)); // submitted
    h.slot.push(frame(B, 2, true)); // submitted: B is now the "current source"
    dec.emitOutput(); // A's output arrives after the switch
    expect(h.renders.at(-1)).toEqual({ kind: 'active', senderId: A });
    dec.emitOutput();
    expect(h.renders.at(-1)).toEqual({ kind: 'active', senderId: B });
    h.stop();
  });

  it('an output matching no submitted chunk fails closed: closed, not drawn, counted', () => {
    const h = harness();
    setState(h, 'active');
    h.slot.push(frame(A, 1, true));
    h.decoders.last!.emitOutput({ timestampUs: 123 });
    expect(h.drawnFrames).toHaveLength(0);
    expect(counterSum(h.sink, 'dt_client_media_decode_queue_dropped_total')).toBe(1);
    h.decoders.last!.emitOutput();
    h.stop();
  });

  it('fail-closed states draw nothing and report none; unavailable and fewer-sources are distinct', () => {
    const h = harness();
    for (const [token, kind] of [
      ['unspecified', 'none'],
      ['switch_pending', 'none'],
      ['withheld_by_congestion', 'none'],
      ['source_unreachable', 'none'],
      ['unavailable', 'unavailable'],
      ['fewer_sources_than_slots', 'fewer_sources'],
    ] as const) {
      setState(h, token);
      expect(h.slot.renderState.kind).toBe(kind);
    }
    h.slot.push(frame(A, 1, true));
    h.decoders.last!.emitOutput();
    expect(h.drawnFrames).toHaveLength(0);
    h.stop();
  });
});

describe('never-render re-check on held frames', () => {
  it('a held frame from a sender server-muted AFTER it decoded is never drawn at the ACTIVE handoff', () => {
    const h = harness();
    setState(h, 'avatar_client_muted', A);
    h.slot.push(frame(A, 1, true));
    h.decoders.last!.emitOutput();
    h.muted.add(A);
    setState(h, 'active');
    expect(h.drawnFrames).toHaveLength(0);
    expect(counterSum(h.sink, 'dt_client_media_decode_queue_dropped_total')).toBe(1);
    h.stop();
  });

  it('an unmapped verified sender is never drawn (security S2)', () => {
    const h = harness();
    setState(h, 'active');
    h.unmapped.add(A);
    h.slot.push(frame(A, 1, true));
    h.decoders.last!.emitOutput();
    expect(h.drawnFrames).toHaveLength(0);
    h.stop();
  });

  it("this client's own sender is never drawn", () => {
    const h = harness();
    setState(h, 'active');
    h.slot.push(frame(SELF, 1, true));
    h.decoders.last!.emitOutput();
    expect(h.drawnFrames).toHaveLength(0);
    h.stop();
  });

  it('a throwing sink: frame still closed once; NOT rendered (stays unfulfilled, no latency); sink error raised', () => {
    const h = harness();
    h.slot.setSink({
      drawFrame: () => {
        throw new Error('sink blew up');
      },
    });
    setState(h, 'active');
    h.slot.push(frame(A, 1, true));
    h.decoders.last!.emitOutput();
    expect(h.slot.renderState).toEqual({ kind: 'active', senderId: A });
    expect(h.unfulfilled.isUnfulfilled(SLOT)).toBe(true);
    expect(h.activeToRender).toEqual([]);
    expect(h.slot.lastRenderedSender).toBeUndefined();
    expect(sinkErrors).toBe(1);
    h.stop();
  });

  it('no sink attached: a drawable frame is NOT rendered until a sink takes it', () => {
    const h = harness();
    h.slot.setSink(undefined);
    setState(h, 'active');
    h.slot.push(frame(A, 1, true));
    h.decoders.last!.emitOutput();
    expect(h.unfulfilled.isUnfulfilled(SLOT)).toBe(true);
    h.slot.setSink({ drawFrame: (f) => h.drawnFrames.push(f) });
    expect(h.drawnFrames).toHaveLength(1);
    expect(h.unfulfilled.isUnfulfilled(SLOT)).toBe(false);
    expect(h.slot.lastRenderedSender).toBe(A);
    h.stop();
  });

  it('security F1: re-attaching a sink never draws a held frame whose sender was muted since', () => {
    const h = harness();
    setState(h, 'active');
    h.slot.push(frame(A, 1, true));
    h.decoders.last!.emitOutput();
    expect(h.drawnFrames).toHaveLength(1);
    h.muted.add(A);
    h.slot.setSink({ drawFrame: (f) => h.drawnFrames.push(f) });
    expect(h.drawnFrames).toHaveLength(1);
    expect(counterSum(h.sink, 'dt_client_media_decode_queue_dropped_total')).toBe(1);
    h.stop();
  });
});

describe('decode bounds (R-6)', () => {
  it('decoded output above the configured geometry is dropped decode_over_bound', () => {
    const h = harness();
    setState(h, 'active');
    h.slot.push(frame(A, 1, true));
    h.decoders.last!.emitOutput({ width: 4096, height: 2160 });
    expect(dropsByReason(h.sink).get('decode_over_bound')).toBe(1);
    expect(h.drawnFrames).toHaveLength(0);
    h.stop();
  });

  it('an over-bound KEYFRAME header never reaches the decoder', () => {
    const h = harness();
    h.slot.push(frame(A, 1, true, { data: vp8Key(4096, 2160) }));
    expect(h.decoders.last!.decoded).toHaveLength(0);
    expect(dropsByReason(h.sink).get('decode_over_bound')).toBe(1);
    h.stop();
  });

  it('a short or malformed keyframe header is a typed discard, never an exception', () => {
    const h = harness();
    expect(() => h.slot.push(frame(A, 1, true, { data: Uint8Array.of(0, 1, 2) }))).not.toThrow();
    expect(() => h.slot.push(frame(A, 2, true, { data: vp8Delta() }))).not.toThrow();
    expect(h.decoders.last!.decoded).toHaveLength(0);
    expect(counterSum(h.sink, 'dt_client_media_decode_queue_dropped_total')).toBe(2);
    h.stop();
  });

  it('frame rate: the (fps+1)th output inside 1000 ms is over bound; at exactly 1000 ms it is not (T7)', () => {
    const h = harness({ decodeMaxFrameRateHz: 30 });
    setState(h, 'active');
    const dec = h.decoders.last!;
    h.slot.push(frame(A, 1, true));
    for (let i = 0; i < 31; i += 1) h.slot.push(frame(A, 1, false));
    const start = h.clock.now();
    for (let i = 0; i < 30; i += 1) dec.emitOutput();
    expect(dropsByReason(h.sink).get('decode_over_bound') ?? 0).toBe(0);
    h.clock.set(start + 999);
    dec.emitOutput();
    expect(dropsByReason(h.sink).get('decode_over_bound')).toBe(1);
    h.clock.set(start + 1_000);
    dec.emitOutput();
    expect(dropsByReason(h.sink).get('decode_over_bound')).toBe(1);
    h.stop();
  });
});

describe('fault isolation (R-6)', () => {
  it('a decoder error closes ONLY this slot decoder; replaced on the next keyframe after the backoff (T7)', () => {
    const h = harness();
    setState(h, 'active');
    const first = h.decoders.last!;
    h.slot.push(frame(A, 1, true));
    first.emitError();
    expect(first.closed).toBe(true);
    expect(h.faults).toHaveLength(1);
    expect(h.losses).toEqual([
      { url: URL, slotId: SLOT, senderId: A, gopCounter: 1, reason: 'decoder_error' },
    ]);
    // A keyframe INSIDE the backoff does not recreate.
    h.clock.advance(999);
    h.slot.push(frame(A, 2, true));
    expect(h.decoders.constructions).toBe(1);
    // At the backoff edge it recreates EXACTLY once and decodes the queued keyframe.
    h.clock.advance(1);
    h.slot.push(frame(A, 2, false));
    expect(h.decoders.constructions).toBe(2);
    expect(h.decoders.last!.decoded.map((c) => c.isKeyframe)).toEqual([true, false]);
    h.decoders.last!.emitOutput();
    h.decoders.last!.emitOutput();
    h.stop();
  });

  it('a decoder error loop at one (sender, counter) sends ONE recovery request', () => {
    const h = harness();
    h.slot.push(frame(A, 1, true));
    h.decoders.last!.emitError();
    h.clock.advance(1_000);
    h.slot.push(frame(A, 1, true)); // same pair again (trailing, discarded)
    expect(h.losses).toHaveLength(1);
    h.stop();
  });

  it('security F3: a SYNCHRONOUS decode() throw is this slot decoder fault, never an exception', () => {
    const h = harness();
    const first = h.decoders.last!;
    first.throwOnNextDecode = new Error('InvalidStateError');
    expect(() => h.slot.push(frame(A, 1, true))).not.toThrow();
    expect(counterSum(h.sink, 'dt_client_media_decoder_errors_total')).toBe(1);
    expect(h.losses.map((l) => l.reason)).toEqual(['decoder_error']);
    expect(first.closed).toBe(true);
    h.clock.advance(1_000);
    h.slot.push(frame(A, 2, true));
    expect(h.decoders.constructions).toBe(2);
    h.decoders.last!.emitOutput();
    h.stop();
  });

  it('a decoder that cannot be BUILT is a decoder fault, not an exception', () => {
    const decoders = new FakeVideoDecoders({ registry });
    decoders.failNextConstruction = new Error('no');
    const { metrics, sink } = videoMetrics();
    const clock = manualClock();
    const slot = new VideoSlotPipeline({
      slotId: 0,
      config: DEFAULT_CLIENT_CONFIG.media.videoReceive,
      decoderRestartBackoffMs: 1_000,
      decoderFactory: decoders.factory as unknown as VideoDecoderFactory,
      metrics,
      accounting: new FrameAccounting(metrics),
      guards: { selfSenderId: SELF, videoMuteState: () => 'unmuted' },
      clock: clock.now,
      unfulfilled: new UnfulfilledSlots(metrics, clock.now),
      onRenderChanged: () => {},
      onLoss: () => {},
      onDecoded: () => {},
      onDrawn: () => {},
      onDecoderFault: () => {},
      onSinkError: SINK_ERR,
    });
    expect(counterSum(sink, 'dt_client_media_decoder_errors_total')).toBe(1);
    clock.advance(1_000);
    slot.push(frame(A, 1, true));
    expect(decoders.constructions).toBe(2);
    decoders.last!.emitOutput();
    slot.stop();
  });
});

describe('recovery requests (R-13)', () => {
  it('a loss carries exactly the failing frame fields; repeats at the same pair send nothing', () => {
    const h = harness();
    h.slot.loss(A, 4, 'verify_failed', URL);
    h.slot.loss(A, 4, 'decrypt_failed', URL);
    expect(h.losses).toEqual([
      { url: URL, slotId: SLOT, senderId: A, gopCounter: 4, reason: 'verify_failed' },
    ]);
    h.stop();
  });

  it('security S1: an unverified huge counter does not suppress a later genuine lower-counter loss', () => {
    const h = harness();
    h.slot.loss(A, 0xffff_fff0, 'verify_failed', URL);
    h.slot.loss(A, 7, 'decrypt_failed', URL);
    expect(h.losses.map((l) => l.gopCounter)).toEqual([0xffff_fff0, 7]);
    h.stop();
  });

  it('A -> B -> A at a LOWER A counter still sends (one pair of state, by design)', () => {
    const h = harness();
    h.slot.loss(A, 9, 'verify_failed', URL);
    h.slot.loss(B, 3, 'verify_failed', URL);
    h.slot.loss(A, 2, 'verify_failed', URL);
    expect(h.losses.map((l) => [l.senderId, l.gopCounter])).toEqual([
      [A, 9],
      [B, 3],
      [A, 2],
    ]);
    h.stop();
  });

  it('jitter overflow: local_drop, the group stops feeding until a newer keyframe', () => {
    const h = harness({ jitterMaxFrames: 2 });
    const dec = h.decoders.last!;
    dec.queueSizeOverride = 99;
    h.slot.push(frame(A, 1, true));
    h.slot.push(frame(A, 1, false));
    h.slot.push(frame(A, 1, false)); // overflow
    h.slot.push(frame(A, 1, false)); // awaiting keyframe: discarded, no second request
    expect(h.losses).toEqual([
      { url: URL, slotId: SLOT, senderId: A, gopCounter: 1, reason: 'local_drop' },
    ]);
    dec.queueSizeOverride = undefined;
    h.slot.push(frame(A, 2, true));
    expect(dec.decoded.map((c) => c.isKeyframe)).toEqual([true, false, true]);
    for (let i = 0; i < 3; i += 1) dec.emitOutput();
    h.stop();
  });

  it('max hold: a frame held past jitterMaxHoldMs is a local_drop; at exactly the hold it is kept (T7)', () => {
    const h = harness({ jitterMaxHoldMs: 500 });
    const dec = h.decoders.last!;
    dec.queueSizeOverride = 99;
    h.slot.push(frame(A, 1, true));
    h.clock.advance(500);
    dec.queueSizeOverride = undefined;
    h.slot.push(frame(A, 1, false));
    expect(dec.decoded).toHaveLength(2);
    expect(h.losses).toHaveLength(0);
    dec.queueSizeOverride = 99;
    h.slot.push(frame(A, 1, false));
    h.clock.advance(501);
    dec.queueSizeOverride = undefined;
    h.slot.push(frame(A, 1, false)); // awaiting after the expiry
    expect(h.losses).toEqual([
      { url: URL, slotId: SLOT, senderId: A, gopCounter: 1, reason: 'local_drop' },
    ]);
    // The expired frame was never fed late, and nothing after it was either.
    expect(dec.decoded).toHaveLength(2);
    for (let i = 0; i < 2; i += 1) dec.emitOutput();
    h.stop();
  });

  it('a slot whose NEW source sends nothing sends nothing (MH recovers it, R-10/R-12)', () => {
    const h = harness();
    setState(h, 'active');
    h.slot.push(frame(A, 1, true));
    h.decoders.last!.emitOutput();
    // MC moves the slot to a new source (via avatar, then live again); nothing arrives.
    setState(h, 'avatar_no_video', B);
    setState(h, 'active');
    for (let i = 0; i < 10; i += 1) h.clock.advance(10_000);
    expect(h.losses).toEqual([]);
    h.stop();
  });

  it('no timer and no retry: advancing the clock alone sends nothing more', () => {
    const h = harness();
    h.slot.loss(A, 1, 'verify_failed', URL);
    h.clock.advance(60_000);
    expect(h.losses).toHaveLength(1);
    h.stop();
  });

  it('a loss while ACTIVE enters unfulfilled; the next drawn frame exits it', () => {
    const h = harness();
    setState(h, 'active');
    h.slot.push(frame(A, 1, true));
    h.decoders.last!.emitOutput();
    expect(h.unfulfilled.isUnfulfilled(SLOT)).toBe(false);
    h.slot.loss(A, 1, 'verify_failed', URL);
    expect(h.unfulfilled.isUnfulfilled(SLOT)).toBe(true);
    h.slot.push(frame(A, 2, true));
    h.decoders.last!.emitOutput();
    expect(h.unfulfilled.isUnfulfilled(SLOT)).toBe(false);
    h.stop();
  });
});

describe('server_mute_lifted repair (observability O2, security conditions)', () => {
  it('a muted drop then the unmute edge: exactly one request with that frame header', () => {
    const h = harness();
    h.slot.recordMutedDrop(A, 6, URL);
    h.slot.serverMuteLifted(A);
    h.slot.serverMuteLifted(A); // repeated edge: nothing
    expect(h.losses).toEqual([
      { url: URL, slotId: SLOT, senderId: A, gopCounter: 6, reason: 'server_mute_lifted' },
    ]);
    h.stop();
  });

  it('unmute with no recorded drop sends nothing', () => {
    const h = harness();
    h.slot.serverMuteLifted(A);
    expect(h.losses).toHaveLength(0);
    h.stop();
  });

  it('a newer group from that sender already admitted sends nothing', () => {
    const h = harness();
    h.slot.recordMutedDrop(A, 6, URL);
    h.slot.push(frame(A, 7, true));
    h.slot.serverMuteLifted(A);
    expect(h.losses).toHaveLength(0);
    h.decoders.last!.emitOutput();
    h.stop();
  });

  it('a source change between the drop and the unmute sends nothing', () => {
    const h = harness();
    h.slot.recordMutedDrop(A, 6, URL);
    h.slot.push(frame(B, 1, true));
    h.slot.serverMuteLifted(A);
    expect(h.losses).toHaveLength(0);
    h.decoders.last!.emitOutput();
    h.stop();
  });
});

describe('teardown', () => {
  it('stop closes the held frame and late outputs from the closed decoder', () => {
    const h = harness();
    setState(h, 'avatar_client_muted', A);
    h.slot.push(frame(A, 1, true));
    h.slot.push(frame(A, 1, false));
    const dec = h.decoders.last!;
    dec.emitOutput(); // held
    h.stop();
    expect(dec.closed).toBe(true);
    dec.emitOutput(); // late output after stop: closed, ignored
  });
});
