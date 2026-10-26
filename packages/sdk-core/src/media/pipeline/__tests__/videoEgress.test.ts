// File: packages/sdk-core/src/media/pipeline/__tests__/videoEgress.test.ts
//
// Story 3 C-A2 (R-12, R-14, R-20, amendment 4): stream-per-group video egress
// against `MockWebTransport` uni streams. No wall-clock sleeps: transport
// stalls are explicit gates, STOP_SENDING is injected, and `settle()` only
// drains microtasks/zero-delay macrotasks.

import { describe, expect, it } from 'vitest';
import { InMemoryMetricsSink, MockWebTransport } from '@darktower/test-utils';

import { HEADER_VERSION } from '../../frame/wireConstants.js';
import { decodeFrame } from '../../frame/frameCodec.js';
import { unpackKeyId } from '../../frame/keyId.js';
import { parseSframe, AES_256_KEY_BYTES } from '../../frame/sframe.js';
import { MeetingIdentity } from '../../setup/identity.js';
import { kekHolderWith } from '../../__tests__/helpers.js';
import { MediaKindMetrics } from '../../setup/mediaMetrics.js';
import { MediaKind } from '../../../proto/dark_tower/signaling/v1/signaling_pb.js';
import { TransmitKeyManager } from '../../lifecycle/transmitKeys.js';
import { uplinkRelayStreamId } from '../sendId.js';
import { VideoEgress, type GroupStreamOpener } from '../videoEgress.js';

const KEK = new Uint8Array(AES_256_KEY_BYTES).fill(0x22);
const SENDER_ID = 258;
const A = 'https://mh-0.example:4433';
const B = 'https://mh-1.example:4433';
const VIDEO_STREAM = 1;

async function settle(): Promise<void> {
  for (let i = 0; i < 30; i += 1) {
    await new Promise<void>((resolve) => {
      setTimeout(resolve, 0);
    });
  }
}

interface RigOptions {
  readonly urls?: readonly string[];
  readonly maxBacklogFrames?: number;
  readonly maxBacklogBytes?: number;
  readonly keyBearingFrames?: number;
  readonly opener?: (mocks: Map<string, MockWebTransport>) => GroupStreamOpener;
  readonly transmitKeys?: TransmitKeyManager;
}

async function makeRig(options: RigOptions = {}) {
  const urls = options.urls ?? [A, B];
  const sink = new InMemoryMetricsSink();
  const metrics = new MediaKindMetrics(
    { clientVersion: 'v', orgId: 'o' },
    MediaKind.VIDEO_CAMERA,
    sink,
  );
  const mocks = new Map<string, MockWebTransport>();
  for (const url of urls) mocks.set(url, new MockWebTransport());
  const transmitKeys = options.transmitKeys ?? new TransmitKeyManager(SENDER_ID);
  let selfAbandoned = 0;
  let keyChunks = 0;
  let notConnected = 0;
  const failures: unknown[] = [];
  const streamId = uplinkRelayStreamId({ streamNumber: VIDEO_STREAM });
  if (streamId === undefined) throw new Error('fixture stream id');
  const identity = await MeetingIdentity.create();
  const egress = new VideoEgress({
    metrics,
    transmitKeys,
    kek: kekHolderWith(KEK, 0),
    identity,
    streamNumber: VIDEO_STREAM,
    streamId,
    keyBearingFrames: options.keyBearingFrames ?? 3,
    maxBacklogFrames: options.maxBacklogFrames ?? 4,
    maxBacklogBytes: options.maxBacklogBytes ?? 1_000_000,
    openStream: options.opener?.(mocks) ?? ((url) => mocks.get(url)?.createUnidirectionalStream()),
    onSelfAbandoned: () => {
      selfAbandoned += 1;
    },
    onKeyChunk: () => {
      keyChunks += 1;
    },
    onTargetNotConnected: () => {
      notConnected += 1;
    },
    onBuildFailed: (err) => failures.push(err),
  });
  let ts = 0;
  const send = (key: boolean, epoch = egress.epoch): Promise<void> => {
    ts += 33_333;
    return egress.submit(
      { data: Uint8Array.of(key ? 0x9d : 0x11, 1, 2, 3), timestampUs: ts, key },
      epoch,
    );
  };
  const count = (name: string, labels: Record<string, string> = {}): number =>
    sink
      .getRecordedMetrics()
      .filter((m) => m.name === name && Object.entries(labels).every(([k, v]) => m.labels[k] === v))
      .reduce((n, m) => n + m.value, 0);
  /** Frames written on handler `url`'s outbound uni streams, per stream. */
  const streamsOf = (url: string) => mocks.get(url)!.getOutboundUniStreams();
  return {
    egress,
    identity,
    mocks,
    transmitKeys,
    sink,
    send,
    count,
    streamsOf,
    selfAbandoned: () => selfAbandoned,
    keyChunks: () => keyChunks,
    notConnected: () => notConnected,
    failures,
  };
}

describe('VideoEgress — groups (one uni stream per group per target)', () => {
  it('seals ONCE and writes identical bytes to every target; the first frame of a group is the keyframe', async () => {
    const r = await makeRig();
    r.egress.setTargets({ targets: [A, B], headerVersion: HEADER_VERSION });
    await r.send(true);
    await r.send(false);
    await r.send(false);
    await settle();
    for (const url of [A, B]) {
      const streams = r.streamsOf(url);
      expect(streams).toHaveLength(1);
      const frames = streams[0]!.chunks.map(decodeFrame);
      expect(frames.map((f) => f.flags.independentlyDecodable)).toEqual([true, false, false]);
      expect(frames.every((f) => f.flags.discardable === false)).toBe(true);
    }
    // Identical sealed bytes apart from the unauthenticated hop sequence.
    const a = r.streamsOf(A)[0]!.chunks.map(decodeFrame);
    const b = r.streamsOf(B)[0]!.chunks.map(decodeFrame);
    expect(a.map((f) => [f.streamSequence, f.gopCounter, [...f.signature].join()])).toEqual(
      b.map((f) => [f.streamSequence, f.gopCounter, [...f.signature].join()]),
    );
    // One stream sequence per FRAME, not per target.
    expect(a.map((f) => f.streamSequence)).toEqual([0, 1, 2]);
  });

  it('the uplink relay stream id is the directed send id (the one stamper)', async () => {
    const r = await makeRig({ urls: [A] });
    r.egress.setTargets({ targets: [A], headerVersion: HEADER_VERSION });
    await r.send(true);
    await settle();
    expect(decodeFrame(r.streamsOf(A)[0]!.chunks[0]!).streamId).toBe(VIDEO_STREAM);
  });

  it('a new key chunk FINISHES the previous group stream and opens a new one; the GoP counter advances per group', async () => {
    const r = await makeRig({ urls: [A] });
    r.egress.setTargets({ targets: [A], headerVersion: HEADER_VERSION });
    await r.send(true);
    await r.send(false);
    await r.send(true);
    await r.send(false);
    await settle();
    const streams = r.streamsOf(A);
    expect(streams.map((s) => s.ending)).toEqual(['closed', 'open']);
    expect(streams.map((s) => s.chunks.map((c) => decodeFrame(c).gopCounter))).toEqual([
      [0, 0],
      [1, 1],
    ]);
  });

  it('every group start ROTATES the session generation (R-14), and the key rides the first N frames after it', async () => {
    const r = await makeRig({ urls: [A], keyBearingFrames: 2 });
    r.egress.setTargets({ targets: [A], headerVersion: HEADER_VERSION });
    await r.send(true);
    for (let i = 0; i < 4; i += 1) await r.send(false);
    await r.send(true);
    await r.send(false);
    await r.send(false);
    await settle();
    const frames = r.streamsOf(A).flatMap((s) => s.chunks.map(decodeFrame));
    expect(frames.map((f) => f.flags.keyBearing)).toEqual([
      true,
      true,
      false,
      false,
      false,
      true,
      true,
      false,
    ]);
    // Never key-bearing AND discardable.
    expect(frames.some((f) => f.flags.keyBearing && f.flags.discardable)).toBe(false);
    const generations = frames.map((f) => unpackKeyId(parseSframe(f.payload).keyId).generation);
    expect(generations[0]).not.toBe(generations[5]);
  });

  it('a mid-group rotation (T or KEK) re-arms the wrap for N frames WITHOUT forcing a keyframe', async () => {
    const r = await makeRig({ urls: [A], keyBearingFrames: 2 });
    r.egress.setTargets({ targets: [A], headerVersion: HEADER_VERSION });
    await r.send(true);
    for (let i = 0; i < 3; i += 1) await r.send(false);
    r.transmitKeys.rotate(); // a T or KEK receipt
    for (let i = 0; i < 3; i += 1) await r.send(false);
    await settle();
    const frames = r.streamsOf(A).flatMap((s) => s.chunks.map(decodeFrame));
    expect(r.streamsOf(A)).toHaveLength(1);
    expect(frames.map((f) => f.flags.independentlyDecodable)).toEqual([
      true,
      false,
      false,
      false,
      false,
      false,
      false,
    ]);
    expect(frames.map((f) => f.flags.keyBearing)).toEqual([
      true,
      true,
      false,
      false,
      true,
      true,
      false,
    ]);
  });

  it('a delta frame with no group in progress anywhere builds nothing and consumes no nonce', async () => {
    const r = await makeRig({ urls: [A] });
    r.egress.setTargets({ targets: [A], headerVersion: HEADER_VERSION });
    await r.send(false);
    await settle();
    expect(r.streamsOf(A)).toHaveLength(0);
    expect(r.transmitKeys.nextStreamSequence(VIDEO_STREAM)).toBe(0);
  });

  it('every key chunk (solicited or not) is reported to the gate', async () => {
    const r = await makeRig({ urls: [A] });
    r.egress.setTargets({ targets: [A], headerVersion: HEADER_VERSION });
    await r.send(true);
    await r.send(false);
    await r.send(true);
    expect(r.keyChunks()).toBe(2);
  });

  it('GoP counter at u32::MAX (the owner returns null): the stream stops, never wraps, counted per lane', async () => {
    // The owner's own boundary is `transmitKeys.gopExhaustion.test.ts`; here
    // only what egress does with its `null`.
    let issued = 0;
    class NearlyExhausted extends TransmitKeyManager {
      override nextGopCounter(streamNumber: number): number | null {
        issued += 1;
        return issued === 1 ? super.nextGopCounter(streamNumber) : null;
      }
    }
    const r = await makeRig({ urls: [A, B], transmitKeys: new NearlyExhausted(SENDER_ID) });
    r.egress.setTargets({ targets: [A, B], headerVersion: HEADER_VERSION });
    await r.send(true);
    await r.send(false);
    await r.send(true); // exhausted here
    await r.send(false);
    await r.send(true);
    await settle();
    expect(r.streamsOf(A)).toHaveLength(1);
    expect(r.streamsOf(A)[0]!.chunks.map((c) => decodeFrame(c).gopCounter)).toEqual([0, 0]);
    expect(r.streamsOf(A)[0]!.ending).toBe('closed');
    // Three attempts after the stop, two active lanes each.
    expect(r.count('dt_client_media_send_dropped_total', { reason: 'gop_counter_exhausted' })).toBe(
      6,
    );
  });
});

describe('VideoEgress — per-target backlog and who repairs an abandonment', () => {
  it('a backlog cap on one target RESETS that target only (never finishes it); others and the chain never stall', async () => {
    const r = await makeRig({ urls: [A, B], maxBacklogFrames: 2 });
    r.egress.setTargets({ targets: [A, B], headerVersion: HEADER_VERSION });
    await r.send(true);
    await settle();
    r.mocks.get(A)!.pauseUniStreamWrites(0);
    // The chain must resolve while A is wedged (it awaits seal/sign only).
    for (let i = 0; i < 6; i += 1) await r.send(false);
    await settle();
    expect(r.streamsOf(A)[0]!.ending).toBe('aborted');
    expect(r.egress.laneState(A)).toBe('awaiting_group');
    expect(r.streamsOf(B)[0]!.chunks).toHaveLength(7);
    expect(r.count('dt_client_media_send_groups_abandoned_total')).toBe(1);
    expect(r.selfAbandoned()).toBe(1);
    expect(r.egress.targetsAwaitingGroup()).toBe(1);
  });

  it('R-35 local counters: bytesSent is the exact sum of bytes written to every target; groupsAbandoned counts the publisher abandonment', async () => {
    const r = await makeRig({ urls: [A, B], maxBacklogFrames: 2 });
    r.egress.setTargets({ targets: [A, B], headerVersion: HEADER_VERSION });
    expect(r.egress.bytesSent).toBe(0);
    expect(r.egress.groupsAbandoned).toBe(0);
    await r.send(true);
    await settle();
    r.mocks.get(A)!.pauseUniStreamWrites(0);
    for (let i = 0; i < 6; i += 1) await r.send(false);
    await settle();
    const written = [A, B]
      .flatMap((url) => r.streamsOf(url))
      .flatMap((stream) => stream.chunks)
      .reduce((n, chunk) => n + chunk.length, 0);
    const frames = [A, B]
      .flatMap((url) => r.streamsOf(url))
      .reduce((n, stream) => n + stream.chunks.length, 0);
    expect(written).toBeGreaterThan(0);
    expect(r.egress.bytesSent).toBe(written);
    expect(r.egress.framesSent).toBe(frames);
    expect(r.egress.groupsAbandoned).toBe(1);
    expect(r.count('dt_client_media_send_groups_abandoned_total')).toBe(1);
  });

  it('the byte cap trips like the frame cap', async () => {
    const r = await makeRig({ urls: [A], maxBacklogFrames: 1_000, maxBacklogBytes: 400 });
    r.egress.setTargets({ targets: [A], headerVersion: HEADER_VERSION });
    await r.send(true);
    await settle();
    r.mocks.get(A)!.pauseUniStreamWrites(0);
    for (let i = 0; i < 6; i += 1) await r.send(false);
    await settle();
    expect(r.streamsOf(A)[0]!.ending).toBe('aborted');
    expect(r.count('dt_client_media_send_groups_abandoned_total')).toBe(1);
  });

  it('after a publisher-decided abandonment, the NEXT key chunk starts a group on EVERY target', async () => {
    const r = await makeRig({ urls: [A, B], maxBacklogFrames: 2 });
    r.egress.setTargets({ targets: [A, B], headerVersion: HEADER_VERSION });
    await r.send(true);
    await settle();
    r.mocks.get(A)!.pauseUniStreamWrites(0);
    for (let i = 0; i < 4; i += 1) await r.send(false);
    await settle();
    expect(r.egress.laneState(A)).toBe('awaiting_group');
    // Deltas reach nothing on A until the next group.
    await r.send(false);
    await settle();
    expect(r.streamsOf(A)).toHaveLength(1);
    await r.send(true);
    await settle();
    expect(r.streamsOf(A)).toHaveLength(2);
    expect(r.streamsOf(B)).toHaveLength(2);
    expect(r.streamsOf(B)[0]!.ending).toBe('closed');
    expect(decodeFrame(r.streamsOf(A)[1]!.chunks[0]!).flags.independentlyDecodable).toBe(true);
    expect(r.egress.targetsAwaitingGroup()).toBe(0);
  });

  it('MH STOP_SENDING: no re-key, not counted as abandoned, not awaiting a group; the next group reaches it', async () => {
    const r = await makeRig({ urls: [A, B] });
    r.egress.setTargets({ targets: [A, B], headerVersion: HEADER_VERSION });
    await r.send(true);
    await settle();
    r.mocks.get(A)!.simulateOutboundUniStopSending(0, 7);
    await r.send(false);
    await r.send(false);
    await settle();
    expect(r.egress.laneState(A)).toBe('mh_stopped');
    expect(r.egress.targetsAwaitingGroup()).toBe(0);
    expect(r.selfAbandoned()).toBe(0);
    expect(r.count('dt_client_media_send_groups_abandoned_total')).toBe(0);
    // The SDK never chose an ending for the stopped stream.
    expect(r.streamsOf(A)[0]!.ending).toBe('open');
    // A group started for another reason (an MH request) reaches it.
    await r.send(true);
    await settle();
    expect(r.streamsOf(A)).toHaveLength(2);
    expect(r.egress.laneState(A)).toBe('in_group');
  });

  it('a hung open whose backlog overflows is MH’s (open-rate) decision: mh_stopped, no re-key, not counted', async () => {
    const r = await makeRig({
      urls: [A, B],
      maxBacklogFrames: 2,
      opener: (mocks) => (url) =>
        url === A
          ? new Promise<WritableStream<Uint8Array>>(() => {
              // Never settles: MAX_STREAMS starvation.
            })
          : mocks.get(url)?.createUnidirectionalStream(),
    });
    r.egress.setTargets({ targets: [A, B], headerVersion: HEADER_VERSION });
    await r.send(true);
    for (let i = 0; i < 4; i += 1) await r.send(false);
    await settle();
    expect(r.egress.laneState(A)).toBe('mh_stopped');
    expect(r.egress.targetsAwaitingGroup()).toBe(0);
    expect(r.selfAbandoned()).toBe(0);
    expect(r.count('dt_client_media_send_groups_abandoned_total')).toBe(0);
    // B was never stalled by A's open.
    expect(r.streamsOf(B)[0]!.chunks).toHaveLength(5);
  });

  it('a rejected open is MH’s too: mh_stopped, no re-key', async () => {
    const r = await makeRig({
      urls: [A],
      opener: () => () => Promise.reject(new Error('refused')),
    });
    r.egress.setTargets({ targets: [A], headerVersion: HEADER_VERSION });
    await r.send(true);
    await settle();
    expect(r.egress.laneState(A)).toBe('mh_stopped');
    expect(r.selfAbandoned()).toBe(0);
  });

  it('a target with no transport is not_connected (counted, faulted), never a self-key cause', async () => {
    const r = await makeRig({ urls: [A], opener: () => () => undefined });
    r.egress.setTargets({ targets: [A], headerVersion: HEADER_VERSION });
    await r.send(true);
    expect(r.notConnected()).toBe(1);
    expect(r.count('dt_client_media_send_dropped_total', { reason: 'not_connected' })).toBe(1);
    expect(r.egress.targetsAwaitingGroup()).toBe(0);
  });
});

describe('VideoEgress — hop sequence (what the transmitter actually SENT)', () => {
  it('is gapless per target across an abandonment and new groups; backlog drops consume no number', async () => {
    const r = await makeRig({ urls: [A], maxBacklogFrames: 2 });
    r.egress.setTargets({ targets: [A], headerVersion: HEADER_VERSION });
    await r.send(true);
    await r.send(false);
    await settle();
    r.mocks.get(A)!.pauseUniStreamWrites(0);
    for (let i = 0; i < 4; i += 1) await r.send(false);
    await settle();
    await r.send(true);
    await r.send(false);
    await settle();
    const hops = r.streamsOf(A).flatMap((s) => s.chunks.map((c) => decodeFrame(c).hopSequence));
    expect(hops).toEqual(hops.map((_, i) => i));
    expect(hops.length).toBeGreaterThanOrEqual(4);
  });

  it('is gapless on a lane that went mh_stopped and rejoined', async () => {
    const r = await makeRig({ urls: [A] });
    r.egress.setTargets({ targets: [A], headerVersion: HEADER_VERSION });
    await r.send(true);
    await r.send(false);
    await settle();
    r.mocks.get(A)!.simulateOutboundUniStopSending(0, 1);
    await r.send(false);
    await settle();
    await r.send(true);
    await r.send(false);
    await settle();
    const hops = r.streamsOf(A).flatMap((s) => s.chunks.map((c) => decodeFrame(c).hopSequence));
    expect(hops).toEqual([0, 1, 2, 3]);
  });

  it('a target removed and re-added resumes its hop count and awaits a group', async () => {
    const r = await makeRig({ urls: [A, B] });
    r.egress.setTargets({ targets: [A, B], headerVersion: HEADER_VERSION });
    await r.send(true);
    await settle();
    r.egress.setTargets({ targets: [B], headerVersion: HEADER_VERSION });
    await settle();
    expect(r.streamsOf(A)[0]!.ending).toBe('closed');
    r.egress.setTargets({ targets: [A, B], headerVersion: HEADER_VERSION });
    expect(r.egress.laneState(A)).toBe('awaiting_group');
    expect(r.egress.targetsAwaitingGroup()).toBe(1);
    await r.send(true);
    await settle();
    expect(decodeFrame(r.streamsOf(A)[1]!.chunks[0]!).hopSequence).toBe(1);
  });
});

describe('VideoEgress — mute / withdrawal epochs (security S5)', () => {
  it('output encoded before endGroups() is dropped: nothing leaves after a mute or withdrawal', async () => {
    const r = await makeRig({ urls: [A] });
    r.egress.setTargets({ targets: [A], headerVersion: HEADER_VERSION });
    await r.send(true);
    await settle();
    const encodedBeforeMute = r.egress.epoch;
    r.egress.endGroups();
    await r.send(false, encodedBeforeMute);
    await r.send(true, encodedBeforeMute);
    await settle();
    expect(r.streamsOf(A)).toHaveLength(1);
    expect(r.streamsOf(A)[0]!.chunks).toHaveLength(1);
    expect(r.streamsOf(A)[0]!.ending).toBe('closed');
    expect(r.egress.laneState(A)).toBe('awaiting_group');
  });

  it('an empty target set sends nothing and builds nothing', async () => {
    const r = await makeRig({ urls: [A] });
    r.egress.setTargets({ targets: [], headerVersion: HEADER_VERSION });
    await r.send(true);
    await settle();
    expect(r.streamsOf(A)).toHaveLength(0);
    expect(r.transmitKeys.currentGopCounter(VIDEO_STREAM)).toBeUndefined();
  });

  it('a build failure is loud: reported once, the stream stops, nothing unsigned is sent', async () => {
    const r = await makeRig({ urls: [A] });
    r.egress.setTargets({ targets: [A], headerVersion: HEADER_VERSION });
    r.identity.clear();
    await r.send(true);
    await r.send(true);
    await settle();
    expect(r.failures).toHaveLength(1);
    expect(r.streamsOf(A).flatMap((st) => st.chunks)).toEqual([]);
    expect(r.egress.hasActiveTarget).toBe(false);
  });
});

describe('VideoEgress — edges', () => {
  it('refuses a non-empty plan in a header version it cannot send (backstop)', async () => {
    const r = await makeRig({ urls: [A] });
    expect(() => r.egress.setTargets({ targets: [A], headerVersion: 99 })).toThrow(RangeError);
    expect(() => r.egress.setTargets({ targets: [], headerVersion: 99 })).not.toThrow();
  });

  it('after stop() nothing is accepted or written', async () => {
    const r = await makeRig({ urls: [A] });
    r.egress.setTargets({ targets: [A], headerVersion: HEADER_VERSION });
    await r.send(true);
    await settle();
    r.egress.stop();
    r.egress.setTargets({ targets: [A], headerVersion: HEADER_VERSION });
    await r.send(true, r.egress.epoch);
    await settle();
    expect(r.streamsOf(A)).toHaveLength(1);
    expect(r.streamsOf(A)[0]!.ending).toBe('closed');
    expect(r.egress.laneState(A)).toBeUndefined();
  });

  it('a group finished while its open is still pending is aborted when the open settles', async () => {
    let resolveOpen!: (s: WritableStream<Uint8Array>) => void;
    const opened: WritableStream<Uint8Array>[] = [];
    const r = await makeRig({
      urls: [A],
      opener: () => () =>
        new Promise<WritableStream<Uint8Array>>((resolve) => {
          resolveOpen = (s) => {
            opened.push(s);
            resolve(s);
          };
        }),
    });
    r.egress.setTargets({ targets: [A], headerVersion: HEADER_VERSION });
    await r.send(true);
    r.egress.endGroups();
    resolveOpen(await r.mocks.get(A)!.createUnidirectionalStream());
    await settle();
    expect(r.streamsOf(A)[0]!.ending).toBe('aborted');
    expect(r.streamsOf(A)[0]!.chunks).toEqual([]);
  });

  it('a group FINISHED mid-write closes after the in-flight write, never losing it', async () => {
    const r = await makeRig({ urls: [A] });
    r.egress.setTargets({ targets: [A], headerVersion: HEADER_VERSION });
    await r.send(true);
    await settle();
    r.mocks.get(A)!.pauseUniStreamWrites(0);
    await r.send(false);
    await settle();
    r.egress.endGroups();
    r.mocks.get(A)!.resumeUniStreamWrites(0);
    await settle();
    expect(r.streamsOf(A)[0]!.chunks).toHaveLength(2);
    expect(r.streamsOf(A)[0]!.ending).toBe('closed');
  });

  it('a removed target that MH had stopped stays inactive and is not counted', async () => {
    const r = await makeRig({ urls: [A, B] });
    r.egress.setTargets({ targets: [A, B], headerVersion: HEADER_VERSION });
    await r.send(true);
    await settle();
    r.mocks.get(A)!.simulateOutboundUniStopSending(0, 1);
    r.egress.setTargets({ targets: [B], headerVersion: HEADER_VERSION });
    await r.send(false);
    await settle();
    expect(r.egress.laneState(A)).toBeUndefined();
    expect(r.egress.targetsAwaitingGroup()).toBe(0);
  });

  it('an empty-url target is ignored', async () => {
    const r = await makeRig({ urls: [A] });
    r.egress.setTargets({ targets: ['', A], headerVersion: HEADER_VERSION });
    expect(r.egress.targetsAwaitingGroup()).toBe(1);
  });
});

describe('VideoEgress — Gate-3 fixes', () => {
  it('a keyframe LARGER than the byte cap is still delivered (the cap measures backlog behind it)', async () => {
    const r = await makeRig({ urls: [A, B], maxBacklogBytes: 64 });
    r.egress.setTargets({ targets: [A, B], headerVersion: HEADER_VERSION });
    await r.send(true);
    await settle();
    for (const url of [A, B]) {
      expect(r.streamsOf(url)[0]!.chunks).toHaveLength(1);
      expect(r.egress.laneState(url)).toBe('in_group');
    }
    expect(r.count('dt_client_media_send_groups_stopped_total')).toBe(0);
  });

  it('a group superseded with UNSENT backlog is RESET, never a truncated FIN', async () => {
    const r = await makeRig({ urls: [A], maxBacklogFrames: 10 });
    r.egress.setTargets({ targets: [A], headerVersion: HEADER_VERSION });
    await r.send(true);
    await settle();
    r.mocks.get(A)!.pauseUniStreamWrites(0);
    await r.send(false);
    await r.send(false); // queued behind the stalled write
    await settle();
    await r.send(true); // next group while backlog remains
    await settle();
    expect(r.streamsOf(A)[0]!.ending).toBe('aborted');
    expect(r.count('dt_client_media_send_groups_abandoned_total')).toBe(0);
  });

  it('a group superseded with NO backlog is FINISHED after its in-flight write', async () => {
    const r = await makeRig({ urls: [A] });
    r.egress.setTargets({ targets: [A], headerVersion: HEADER_VERSION });
    await r.send(true);
    await r.send(false);
    await settle();
    await r.send(true);
    await settle();
    expect(r.streamsOf(A)[0]!.ending).toBe('closed');
    expect(r.streamsOf(A)[0]!.chunks).toHaveLength(2);
  });

  it('the hop number is claimed when WRITTEN: a pending old-group write and the new keyframe never share one', async () => {
    const r = await makeRig({ urls: [A] });
    r.egress.setTargets({ targets: [A], headerVersion: HEADER_VERSION });
    await r.send(true);
    await settle();
    r.mocks.get(A)!.pauseUniStreamWrites(0);
    await r.send(false); // in flight on the old group, its write pending
    await settle();
    await r.send(true); // new group: open resolves, keyframe written
    await settle();
    r.mocks.get(A)!.resumeUniStreamWrites(0);
    await settle();
    const hops = r.streamsOf(A).flatMap((st) => st.chunks.map((c) => decodeFrame(c).hopSequence));
    expect([...hops].sort((x, y) => x - y)).toEqual([0, 1, 2]);
  });

  it.each([
    ['peer_stopped', 'stop'],
    ['open_failed', 'reject'],
    ['open_stalled', 'hang'],
  ] as const)('counts the transition into mh_stopped ONCE as %s', async (outcome, how) => {
    const r = await makeRig({
      urls: [A],
      maxBacklogFrames: 2,
      ...(how === 'reject'
        ? { opener: () => () => Promise.reject(new Error('refused')) }
        : how === 'hang'
          ? {
              opener: () => () =>
                new Promise<WritableStream<Uint8Array>>(() => {
                  // never settles
                }),
            }
          : {}),
    });
    r.egress.setTargets({ targets: [A], headerVersion: HEADER_VERSION });
    await r.send(true);
    await settle();
    if (how === 'stop') r.mocks.get(A)!.simulateOutboundUniStopSending(0, 1);
    for (let i = 0; i < 5; i += 1) await r.send(false);
    await settle();
    expect(r.egress.laneState(A)).toBe('mh_stopped');
    expect(r.count('dt_client_media_send_groups_stopped_total', { outcome })).toBe(1);
    expect(r.count('dt_client_media_send_groups_stopped_total')).toBe(1);
    expect(r.count('dt_client_media_send_groups_abandoned_total')).toBe(0);
  });
});
