// File: packages/sdk-core/src/media/lifecycle/__tests__/audioPipeline.deviceAndCounts.test.ts
//
// Two surfaces an embedder reads and this pipeline must not lie about:
//
//   * `setCaptureDevice()` — a microphone picker whose selection silently did
//     nothing after `start()` would be a masked failure wearing a UI disguise,
//     which is the exact failure mode this story exists to eliminate. So the
//     swap is asserted to be REAL: the old device is released, the new one is
//     started, and captured frames keep flowing into the same encoder.
//   * `frameCounts` — the monotone counters an embedder SAMPLES. They are the
//     evidence behind the browser suite's structural mute assertion ("egress
//     stays flat while muted"), so a counter that advanced while muted, or that
//     counted a refused send, would make that assertion decorative.
//
// No wall-clock assertion appears here, and none may (ADR-0036 §10).

import { describe, expect, it } from 'vitest';
import {
  FakeAudioCodecs,
  FakeCaptureSource,
  InMemoryMetricsSink,
  MockWebTransport,
  RecordingPlaybackSink,
  makeAudioData,
} from '@darktower/test-utils';

import { DEFAULT_CLIENT_CONFIG } from '../../../config/clientConfig.js';
import { AES_256_KEY_BYTES } from '../../frame/sframe.js';
import { MeetingIdentity } from '../../setup/identity.js';
import { JoinResponseKekSource } from '../../setup/kekSource.js';
import { MediaMetrics } from '../../setup/mediaMetrics.js';
import { RosterIdentityKeys } from '../../setup/rosterKeys.js';
import type { DatagramSender } from '../../pipeline/egress.js';
import { AudioPipeline, type AudioPipelineOptions } from '../AudioPipeline.js';

const KEK = new Uint8Array(AES_256_KEY_BYTES).fill(0x11);
const SENDER_ID = 258;
const MH_URL = 'https://mh.example/media';

/** Every capture the factory handed out, in order, with the id it was asked for. */
interface CaptureRecord {
  readonly deviceId: string | undefined;
  readonly capture: FakeCaptureSource;
}

interface Rig {
  readonly pipeline: AudioPipeline;
  readonly captures: CaptureRecord[];
  readonly sink: InMemoryMetricsSink;
  readonly faults: string[];
  /** Fail the NEXT capture acquisition, as a device in use or revoked would. */
  failNextAcquire(): void;
  /** Feed one synthetic frame from the newest capture. */
  emit(byte: number): boolean;
  /** Replay everything sent back into the pipeline's own ingress, as MH does. */
  loopback(): Promise<void>;
  /** Deliver an arbitrary datagram inbound, bypassing the send path. */
  inject(bytes: Uint8Array): Promise<void>;
  count(name: string): number;
}

async function makeRig(options: { readonly startDeviceId?: string } = {}): Promise<Rig> {
  const sink = new InMemoryMetricsSink();
  const metrics = new MediaMetrics({ clientVersion: '0.0.0-test', orgId: 'demo' }, sink);
  const kekSource = new JoinResponseKekSource();
  kekSource.set(KEK, 0);
  const roster = new RosterIdentityKeys(8);
  const identity = await MeetingIdentity.create();
  await roster.upsert({ senderId: SENDER_ID, identityPublicKey: identity.publicKey! });

  const codecs = new FakeAudioCodecs();
  const playback = new RecordingPlaybackSink();
  const transport = new MockWebTransport();
  transport.simulateReady();
  const writer = transport.datagrams.writable.getWriter();
  const sender: DatagramSender = {
    async send(bytes: Uint8Array): Promise<void> {
      await writer.ready;
      await writer.write(bytes);
    },
    get maxDatagramSize(): number | undefined {
      return transport.datagrams.maxDatagramSize;
    },
    isOpen: true,
  };

  const captures: CaptureRecord[] = [];
  const faults: string[] = [];
  let failNext = false;

  const pipelineOptions: AudioPipelineOptions = {
    config: DEFAULT_CLIENT_CONFIG.media,
    metrics,
    kekSource,
    roster,
    senderId: SENDER_ID,
    kekGeneration: 0,
    identity,
    declaredSlotIds: [0],
    senderFor: () => sender,
    readableFor: () => transport.datagrams.readable,
    reportMute: () => {},
    captureFactory: (async (opts: { deviceId: string | undefined }) => {
      if (failNext) {
        failNext = false;
        throw new Error('device unavailable');
      }
      const capture = new FakeCaptureSource();
      captures.push({ deviceId: opts.deviceId, capture });
      return capture;
    }) as never,
    encoderFactory: codecs.encoderFactory as never,
    decoderFactory: codecs.decoderFactory as never,
    playbackFactory: playback.factory as never,
    clock: () => 0,
    setInterval: () => 0 as unknown as ReturnType<typeof setInterval>,
    clearInterval: () => {},
  };

  const pipeline = new AudioPipeline(pipelineOptions);
  pipeline.on('fault', (fault) => faults.push(fault.stage));
  await pipeline.start(
    options.startDeviceId !== undefined ? { deviceId: options.startDeviceId } : {},
  );
  pipeline.setSendDirective({ streamNumber: 1, bitrateBps: 32_000, targets: [MH_URL] });

  return {
    pipeline,
    captures,
    sink,
    faults,
    failNextAcquire: () => {
      failNext = true;
    },
    emit(byte: number): boolean {
      const newest = captures[captures.length - 1];
      if (!newest) return false;
      return newest.capture.emit(makeAudioData({ samples: Float32Array.of(byte, byte, byte) }));
    },
    async loopback(): Promise<void> {
      const sent = [...transport.getOutboundDatagrams()];
      transport.clearInspector();
      for (const bytes of sent) transport.simulateIncomingDatagram(bytes);
      await settle();
    },
    async inject(bytes: Uint8Array): Promise<void> {
      transport.simulateIncomingDatagram(bytes);
      await settle();
    },
    count: (name) =>
      sink
        .getRecordedMetrics()
        .filter((m) => m.name === name)
        .reduce((sum, m) => sum + m.value, 0),
  };
}

/**
 * Yield until the pipeline's async work has settled. A SCHEDULING yield, not a
 * timing assertion — `submit()` awaits real WebCrypto, which resolves on the
 * macrotask queue. Nothing here asserts a duration, and nothing may.
 */
async function settle(): Promise<void> {
  for (let i = 0; i < 25; i += 1) {
    await new Promise<void>((resolve) => {
      setTimeout(resolve, 0);
    });
  }
}

describe('AudioPipeline.setCaptureDevice', () => {
  it('records the choice before start(), and start() applies it', async () => {
    const rig = await makeRig({ startDeviceId: 'mic-a' });
    expect(rig.pipeline.captureDeviceId).toBe('mic-a');
    expect(rig.captures).toHaveLength(1);
    expect(rig.captures[0]?.deviceId).toBe('mic-a');
    await rig.pipeline.stop();
  });

  it('swaps the live device: releases the old one, starts the new one, keeps capturing', async () => {
    const rig = await makeRig({ startDeviceId: 'mic-a' });
    const first = rig.captures[0]!.capture;

    await rig.pipeline.setCaptureDevice('mic-b');

    // The swap is REAL on all three axes a picker could fake.
    expect(rig.pipeline.captureDeviceId).toBe('mic-b');
    expect(rig.captures).toHaveLength(2);
    expect(rig.captures[1]?.deviceId).toBe('mic-b');
    // The old device is RELEASED, not merely dropped: a leaked track keeps the
    // microphone hot and the recording indicator lit.
    expect(first.stopCount).toBe(1);
    // And the new one is actually running — `emit` returns false unless the
    // capture's `onFrame` is wired, so this fails if the swap left a dead pipe.
    expect(rig.captures[1]?.capture.started).toBe(true);
    expect(rig.emit(7)).toBe(true);

    await rig.pipeline.stop();
  });

  it('is a no-op when the device does not change (no stop, no re-acquire)', async () => {
    const rig = await makeRig({ startDeviceId: 'mic-a' });
    await rig.pipeline.setCaptureDevice('mic-a');
    expect(rig.captures).toHaveLength(1);
    expect(rig.captures[0]?.capture.stopCount).toBe(0);
    await rig.pipeline.stop();
  });

  it('teardown after a swap stops the NEW capture, not only the original', async () => {
    // The regression this pins: `TeardownRegistry` has no unregister, so a
    // disposer closing over the FIRST capture local would leave the swapped-in
    // microphone live after `stop()` — a lit indicator after the meeting ended.
    const rig = await makeRig({ startDeviceId: 'mic-a' });
    await rig.pipeline.setCaptureDevice('mic-b');
    const swapped = rig.captures[1]!.capture;

    await rig.pipeline.stop();

    expect(swapped.stopCount).toBe(1);
  });

  it('reports a swap failure loudly: rejects AND raises a capture fault', async () => {
    const rig = await makeRig({ startDeviceId: 'mic-a' });
    rig.failNextAcquire();

    await expect(rig.pipeline.setCaptureDevice('mic-gone')).rejects.toThrow();

    // Both channels, because either alone leaves a hole: a caller that ignores
    // the rejection still sees the fault, and a caller that awaits gets an
    // error rather than silently believing a dead pipeline is live.
    expect(rig.faults).toContain('capture');
    await rig.pipeline.stop();
  });

  it('a device ended after a swap reports through the same fault path', async () => {
    // A second, subtly different ended-handler on the swap path is how a
    // swapped device ends up failing silently, so the handler is shared.
    const rig = await makeRig({ startDeviceId: 'mic-a' });
    await rig.pipeline.setCaptureDevice('mic-b');
    rig.captures[1]!.capture.endTrack();
    expect(rig.faults).toContain('capture');
  });
});

describe('AudioPipeline.frameCounts', () => {
  it('is zero before any media moves', async () => {
    const rig = await makeRig();
    expect(rig.pipeline.frameCounts).toEqual({
      framesSent: 0,
      framesReceived: 0,
      framesAccepted: 0,
      framesDropped: 0,
      lastDropReason: undefined,
    });
    await rig.pipeline.stop();
  });

  it('advances on send, and agrees with the metrics counter it mirrors', async () => {
    const rig = await makeRig();
    rig.emit(1);
    rig.emit(2);
    await settle();

    // The invariant the browser suite's mute assertion rests on: the sampled
    // integer and `dt_client_media_frames_sent_total` are one fact with two
    // readers. If a future edit separates the increment from the metric call,
    // this is what fails.
    expect(rig.pipeline.frameCounts.framesSent).toBe(2);
    expect(rig.count('dt_client_media_frames_sent_total')).toBe(2);

    await rig.pipeline.stop();
  });

  it('framesSent stays FLAT while muted and resumes on unmute', async () => {
    const rig = await makeRig();
    rig.emit(1);
    await settle();
    // Positive control: without this the flat-while-muted assertion below would
    // pass just as well on a pipeline that never sent anything at all.
    const before = rig.pipeline.frameCounts.framesSent;
    expect(before).toBeGreaterThan(0);

    rig.pipeline.setAudioMuted(true);
    rig.emit(2);
    rig.emit(3);
    await settle();
    expect(rig.pipeline.frameCounts.framesSent).toBe(before);

    rig.pipeline.setAudioMuted(false);
    rig.emit(4);
    await settle();
    expect(rig.pipeline.frameCounts.framesSent).toBeGreaterThan(before);

    await rig.pipeline.stop();
  });

  it('framesAccepted advances only for frames that complete the receive path', async () => {
    const rig = await makeRig();
    rig.emit(1);
    await settle();
    expect(rig.pipeline.frameCounts.framesAccepted).toBe(0);

    await rig.loopback();

    expect(rig.pipeline.frameCounts.framesAccepted).toBe(1);
    expect(rig.count('dt_client_media_frames_accepted_total')).toBe(1);

    await rig.pipeline.stop();
  });

  it('reports the COMPLETE receive identity, so a reject is distinguishable from silence', async () => {
    // THE PROPERTY THAT WAS MISSING, and it cost a live misdiagnosis: with only
    // `accepted` visible, "nothing is arriving" and "arriving and being
    // rejected" are the same reading — and they have opposite remediations (a
    // relay that stopped forwarding vs a client that cannot attribute the
    // sender). `received` and `dropped` are what separate them.
    const rig = await makeRig();

    // State 1: nothing has arrived. received === 0 is the discriminator.
    rig.emit(1);
    await settle();
    expect(rig.pipeline.frameCounts.framesReceived).toBe(0);
    expect(rig.pipeline.frameCounts.lastDropReason).toBeUndefined();

    // State 2: arriving and ACCEPTED.
    await rig.loopback();
    expect(rig.pipeline.frameCounts.framesReceived).toBe(1);
    expect(rig.pipeline.frameCounts.framesAccepted).toBe(1);
    expect(rig.pipeline.frameCounts.framesDropped).toBe(0);

    // State 3: arriving and REJECTED. `received` moves, `accepted` does not,
    // and the reason names which check failed — the state the loopback hit at
    // Gate 2, when it read as "MH is not forwarding".
    await rig.inject(Uint8Array.of(0xff, 0xff, 0xff, 0xff));
    const counts = rig.pipeline.frameCounts;
    expect(counts.framesReceived).toBe(2);
    expect(counts.framesAccepted).toBe(1);
    expect(counts.framesDropped).toBe(1);
    expect(counts.lastDropReason).toBeDefined();

    // The identity holds, which is what makes the three states readable at all.
    expect(counts.framesReceived).toBe(counts.framesAccepted + counts.framesDropped);

    await rig.pipeline.stop();
  });

  it('framesAccepted does not advance for a datagram the receive path rejects', async () => {
    const rig = await makeRig();
    // Establish the counter CAN move, so a zero below means "rejected", not
    // "the receive path is dead" — the vacuity control for this assertion.
    rig.emit(1);
    await settle();
    await rig.loopback();
    const acceptedAfterGoodFrame = rig.pipeline.frameCounts.framesAccepted;
    expect(acceptedAfterGoodFrame).toBe(1);

    // A datagram that is not a frame at all. It is RECEIVED but never accepted,
    // so `received = accepted + sum(drops by reason)` must hold with the
    // accepted term unmoved and a drop counted instead.
    await rig.inject(Uint8Array.of(0xff, 0xff, 0xff, 0xff));

    expect(rig.pipeline.frameCounts.framesAccepted).toBe(acceptedAfterGoodFrame);
    expect(rig.count('dt_client_media_frames_received_total')).toBe(2);
    expect(rig.count('dt_client_media_frames_dropped_total')).toBe(1);

    await rig.pipeline.stop();
  });
});
