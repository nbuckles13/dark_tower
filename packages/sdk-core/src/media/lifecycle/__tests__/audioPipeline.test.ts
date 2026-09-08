// File: packages/sdk-core/src/media/lifecycle/__tests__/audioPipeline.test.ts
//
// LIFECYCLE: loopback wiring, client mute, the bounded egress queue, teardown,
// and degradation.
//
// ---------------------------------------------------------------------------
// WHAT THE LOOPBACK HERE DOES AND DOES NOT PROVE
// ---------------------------------------------------------------------------
//
// The round-trip case below feeds this pipeline's OWN egress back into its own
// ingress. That proves WIRING — capture -> mute gate -> encode -> frame -> seal
// -> sign -> queue -> transport -> decode -> verify -> replay -> unwrap ->
// decrypt -> audio decode -> playback. It proves NOTHING about cryptographic
// correctness: a single client encrypting and decrypting with its own keys is
// self-consistent under a swapped associated data, an inverted nonce, a
// truncated key id, or a signature over the wrong range, and every one of those
// would pass this test green.
//
// The crypto is proven by the externally anchored vectors in the codec task's
// conformance suite, which this file neither weakens nor routes around. Only the
// Opus MEDIA CODEC is faked here; the frame crypto is the production primitives.

import { describe, expect, it } from 'vitest';
import {
  FakeAudioCodecs,
  FakeCaptureSource,
  InMemoryMetricsSink,
  MockWebTransport,
  RecordingPlaybackSink,
  makeAudioData,
} from '@darktower/test-utils';

import { DEFAULT_CLIENT_CONFIG, type MediaConfig } from '../../../config/clientConfig.js';
import { AES_256_KEY_BYTES } from '../../frame/sframe.js';
import { MeetingIdentity } from '../../setup/identity.js';
import { JoinResponseKekSource } from '../../setup/kekSource.js';
import { MediaMetrics } from '../../setup/mediaMetrics.js';
import { RosterIdentityKeys } from '../../setup/rosterKeys.js';
import {
  EgressIdentityReleasedError,
  EgressPipeline,
  type DatagramSender,
} from '../../pipeline/egress.js';
import { TransmitKeyManager } from '../transmitKeys.js';
import { AudioPipeline, type AudioPipelineOptions } from '../AudioPipeline.js';

const KEK = new Uint8Array(AES_256_KEY_BYTES).fill(0x11);
const SENDER_ID = 258;
const MH_URL = 'https://mh.example/media';

interface Rig {
  readonly pipeline: AudioPipeline;
  readonly capture: FakeCaptureSource;
  readonly codecs: FakeAudioCodecs;
  readonly playback: RecordingPlaybackSink;
  readonly transport: MockWebTransport;
  readonly sink: InMemoryMetricsSink;
  readonly muteReports: boolean[];
  readonly identity: MeetingIdentity;
  /**
   * Submit one encoded frame straight to the egress pipeline.
   *
   * Needed because `EgressPipeline.submit` short-circuits on `#stopped`, so a
   * frame can only reach the signing step after release if the pipeline itself
   * was not stopped — which is exactly the teardown race being asserted, and is
   * not reachable through the capture callback.
   */
  submitRaw(): Promise<void>;
  count(name: string): number;
  /** Feed one synthetic captured frame. */
  emit(byte: number): boolean;
  /**
   * Model MH: take every datagram the pipeline sent and deliver it back on the
   * inbound stream.
   *
   * The pipeline is NOT told these are its own frames — the loopback is MC's
   * assignment and MH's forwarding, and there is no self-trust branch anywhere.
   */
  loopback(): Promise<void>;
}

async function makeRig(overrides: Partial<MediaConfig> = {}): Promise<Rig> {
  const sink = new InMemoryMetricsSink();
  const metrics = new MediaMetrics({ clientVersion: '0.0.0-test', orgId: 'demo' }, sink);
  const kekSource = new JoinResponseKekSource();
  kekSource.set(KEK, 0);
  const roster = new RosterIdentityKeys(8);
  const identity = await MeetingIdentity.create();
  await roster.upsert({ senderId: SENDER_ID, identityPublicKey: identity.publicKey! });

  const capture = new FakeCaptureSource();
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
  const muteReports: boolean[] = [];

  const config: MediaConfig = { ...DEFAULT_CLIENT_CONFIG.media, ...overrides };
  const options: AudioPipelineOptions = {
    config,
    metrics,
    kekSource,
    roster,
    senderId: SENDER_ID,
    kekGeneration: 0,
    identity,
    declaredSlotIds: [0],
    senderFor: () => sender,
    readableFor: () => transport.datagrams.readable,
    reportMute: (muted) => muteReports.push(muted),
    captureFactory: async () => capture as never,
    encoderFactory: codecs.encoderFactory as never,
    decoderFactory: codecs.decoderFactory as never,
    playbackFactory: playback.factory as never,
    clock: () => 0,
    // Rotation is driven explicitly; a wall-clock interval in a unit test is a
    // flake waiting to happen and this suite asserts no durations at all.
    setInterval: () => 0 as unknown as ReturnType<typeof setInterval>,
    clearInterval: () => {},
  };
  const pipeline = new AudioPipeline(options);
  await pipeline.start();
  pipeline.setSendDirective({ streamNumber: 1, bitrateBps: 32_000, targets: [MH_URL] });

  const egress = new EgressPipeline({
    metrics,
    transmitKeys: new TransmitKeyManager(SENDER_ID),
    kek: { source: kekSource, generation: 0 },
    identity,
    maxQueueFrames: config.egress.maxQueueFrames,
    streamNumber: 1,
    streamId: 0,
  });

  const rig: Rig = {
    pipeline,
    identity,
    submitRaw: () => egress.submit({ data: Uint8Array.of(1, 2, 3), timestampUs: 0 }),
    capture,
    codecs,
    playback,
    transport,
    sink,
    muteReports,
    count: (name) =>
      sink
        .getRecordedMetrics()
        .filter((m) => m.name === name)
        .reduce((sum, m) => sum + m.value, 0),
    emit(byte: number): boolean {
      return capture.emit(makeAudioData({ samples: Float32Array.of(byte, byte, byte) }));
    },
    async loopback(): Promise<void> {
      const sent = [...transport.getOutboundDatagrams()];
      transport.clearInspector();
      for (const bytes of sent) transport.simulateIncomingDatagram(bytes);
      await settle();
    },
  };
  return rig;
}

/**
 * Yield until the pipeline's async work has settled.
 *
 * `submit()` awaits real WebCrypto — the KEK wrap, the HKDF schedule, the SFrame
 * seal and the Ed25519 signature — which resolve on the macrotask queue after
 * libuv thread-pool work, so draining microtasks alone is not enough.
 *
 * This is a SCHEDULING YIELD, not a timing assertion: no test here asserts that
 * anything completes within a duration, and none may. ADR-0036 §10 forbids
 * gating on wall-clock latency, and a threshold on a shared machine is a
 * permanent flake that ADR-0028 would then require be deleted rather than
 * quarantined.
 */
async function settle(): Promise<void> {
  for (let i = 0; i < 25; i += 1) {
    await new Promise<void>((resolve) => {
      setTimeout(resolve, 0);
    });
  }
}

describe('client mute is enforced at capture', () => {
  it('stops egress within one frame and resumes with no signalling round trip', async () => {
    const rig = await makeRig();

    rig.emit(1);
    await settle();
    const beforeMute = rig.transport.getOutboundDatagrams().length;
    expect(beforeMute).toBe(1);

    // Mute BETWEEN frame N and frame N+1. Asserted by FRAME INDEX, never by
    // elapsed time: a wall-clock assertion here would be a flake, and this suite
    // asserts no durations at all.
    rig.pipeline.setAudioMuted(true);
    rig.emit(2);
    rig.emit(3);
    await settle();
    expect(
      rig.transport.getOutboundDatagrams().length,
      'no encoded audio may leave the device while muted',
    ).toBe(beforeMute);
    // Nothing was ENCODED either — the gate is at capture, not at the queue.
    expect(rig.codecs.encoded).toHaveLength(1);
    // And nothing was counted as a send drop: while muted nothing is produced,
    // so nothing is dropped. Counting suppressed frames would spike the
    // send-drop rate on every mute.
    expect(rig.count('dt_client_media_send_dropped_total')).toBe(0);

    // Unmute resumes on the very next captured frame. The mute report to MC is
    // fire-and-forget and is NOT awaited: ADR-0036 §5 requires client mute to
    // hold without the server honouring it, so resumption cannot depend on it.
    rig.pipeline.setAudioMuted(false);
    rig.emit(4);
    await settle();
    expect(rig.transport.getOutboundDatagrams().length).toBe(beforeMute + 1);

    expect(rig.count('dt_client_media_mute_transitions_total')).toBe(2);
    expect(rig.muteReports).toEqual([true, false]);
  });

  it('counts a transition only on a real state change', async () => {
    const rig = await makeRig();
    rig.pipeline.setAudioMuted(true);
    rig.pipeline.setAudioMuted(true);
    expect(rig.count('dt_client_media_mute_transitions_total')).toBe(1);
    expect(rig.muteReports).toEqual([true]);
  });

  it('rotates the transmit key on unmute, and never resets the generation', async () => {
    const rig = await makeRig();
    const start = rig.pipeline.transmitGeneration;
    rig.pipeline.setAudioMuted(true);
    expect(rig.pipeline.transmitGeneration).toBe(start);
    rig.pipeline.setAudioMuted(false);
    // Resuming from silence is a resume-from-empty for the stream. The bump is
    // UNCONDITIONAL and only ever upward: a sequence reset reachable without a
    // generation advance is a (key, nonce) repeat under AES-GCM.
    expect(rig.pipeline.transmitGeneration).toBe(start + 1n);
  });
});

describe('loopback wiring', () => {
  it('round-trips captured audio through frame, crypto, transport and playback', async () => {
    // WIRING ONLY. A single client encrypting and decrypting with its own keys
    // is self-consistent under a swapped associated data, an inverted nonce, a
    // truncated key id, or a signature over the wrong range — every one of those
    // passes this test green. Crypto correctness is the externally anchored
    // vectors' job, not this test's.
    const rig = await makeRig();
    rig.emit(5);
    await settle();
    expect(rig.transport.getOutboundDatagrams()).toHaveLength(1);
    expect(rig.count('dt_client_media_frames_sent_total')).toBe(1);

    await rig.loopback();

    expect(rig.count('dt_client_media_frames_received_total')).toBe(1);
    expect(rig.count('dt_client_media_frames_dropped_total')).toBe(0);
    expect(rig.count('dt_client_media_frames_accepted_total')).toBe(1);
    // Identity encoding, so the exact bytes must survive the whole path.
    expect(rig.playback.played).toHaveLength(1);
    expect([...(rig.playback.played[0] ?? [])]).toEqual([5, 5, 5]);
    // Every `AudioData` handed to playback was released.
    expect(rig.playback.leaked).toBe(0);
    // First-media is OBSERVED, never gated: the assertion is that a value
    // exists, and deliberately not that it is below any threshold.
    expect(
      rig.sink
        .getRecordedMetrics()
        .filter((m) => m.name === 'dt_client_time_to_first_media_frame_ms').length,
    ).toBe(1);
  });
});

describe('the bounded egress queue drops oldest and counts it', () => {
  it('evicts the oldest frame under back-pressure and counts egress_queue_overflow', async () => {
    const rig = await makeRig({
      // 3 frames, against a transport high-water mark of 2. The application
      // bound MUST exceed the transport ceiling — `validateMediaConfig` throws
      // otherwise — because an inverted pair moves every drop into the user
      // agent's queue, where neither this client nor MH can count it.
      egress: { ...DEFAULT_CLIENT_CONFIG.media.egress, maxQueueFrames: 3 },
    });
    // Stall the transport so the queue is the only place frames can accumulate
    // and the eviction is the SDK's own decision, not the user agent's.
    rig.transport.pauseDatagramWrites();

    for (let i = 0; i < 5; i += 1) rig.emit(i);
    await settle();

    // Capacity 3, five frames offered with the transport stalled: at least one
    // eviction, each counted.
    expect(rig.count('dt_client_media_send_dropped_total')).toBeGreaterThan(0);
    const reasons = new Set(
      rig.sink
        .getRecordedMetrics()
        .filter((m) => m.name === 'dt_client_media_send_dropped_total')
        .map((m) => String(m.labels.reason)),
    );
    expect(reasons).toEqual(new Set(['egress_queue_overflow']));
    // The depth gauge tracks the bound rather than growing without limit.
    const depths = rig.sink
      .getRecordedMetrics()
      .filter((m) => m.name === 'dt_client_media_send_queue_depth')
      .map((m) => m.value);
    expect(Math.max(...depths)).toBeLessThanOrEqual(3);

    rig.transport.resumeDatagramWrites();
  });

  it('refuses an oversize datagram under its own reason rather than as a transport fault', async () => {
    const rig = await makeRig();
    // A frame is always larger than this. `transport_send_refused`'s fleet
    // contract is "reads zero forever", so a configuration condition must not
    // land there and poison an alertable invariant.
    rig.transport.setMaxDatagramSize(4);
    rig.emit(7);
    await settle();
    const reasons = rig.sink
      .getRecordedMetrics()
      .filter((m) => m.name === 'dt_client_media_send_dropped_total')
      .map((m) => String(m.labels.reason));
    expect(reasons).toEqual(['oversize_datagram']);
    expect(rig.count('dt_client_media_frames_sent_total')).toBe(0);
  });
});

describe('teardown', () => {
  it('stops the microphone even when a later setup step throws', async () => {
    // A leaked capture keeps the microphone hot and the browser's recording
    // indicator lit after the meeting ends. That is a user-visible privacy
    // failure, so teardown must reach a PARTIALLY-CONSTRUCTED pipeline.
    const capture = new FakeCaptureSource();
    const identity = await MeetingIdentity.create();
    const kekSource = new JoinResponseKekSource();
    kekSource.set(KEK, 0);
    const sink = new InMemoryMetricsSink();
    const pipeline = new AudioPipeline({
      config: DEFAULT_CLIENT_CONFIG.media,
      metrics: new MediaMetrics({ clientVersion: 'v', orgId: 'o' }, sink),
      kekSource,
      roster: new RosterIdentityKeys(4),
      senderId: SENDER_ID,
      kekGeneration: 0,
      identity,
      declaredSlotIds: [0],
      senderFor: () => undefined,
      readableFor: () => undefined,
      reportMute: () => {},
      captureFactory: async () => capture as never,
      // Fails AFTER capture has been acquired and registered.
      playbackFactory: async () => {
        throw new Error('audio context is suspended');
      },
      encoderFactory: async () => {
        throw new Error('unreachable');
      },
      decoderFactory: async () => {
        throw new Error('unreachable');
      },
      clock: () => 0,
    });

    await expect(pipeline.start()).rejects.toThrow('audio context is suspended');
    await pipeline.stop();
    expect(capture.stopCount, 'the microphone must stop even on a partial start').toBe(1);
  });

  it('is idempotent and releases every resource once', async () => {
    const rig = await makeRig();
    await rig.pipeline.stop();
    await rig.pipeline.stop();
    expect(rig.capture.stopCount).toBe(1);
    expect(rig.codecs.encoderClosed).toBe(1);
    expect(rig.codecs.decoderClosed).toBe(1);
    expect(rig.playback.closeCount).toBe(1);
  });
});

describe('teardown revokes the signing capability, it does not merely stop using it', () => {
  it('refuses to sign once the meeting identity has been released', async () => {
    // THE PROPERTY THE GUARD WAS POINTING AT. Before this, `EgressPipeline`
    // held its own `CryptoKey` copied out of the identity at construction, so
    // `MeetingIdentity.clear()` released the holder's reference and the pipeline
    // kept an independent one — the capability outlived the object owning it,
    // which is the half of ADR-0028 §5's "clear all references" that a copy
    // silently defeats.
    //
    // Passing the HOLDER and reading `.signer` at the point of use is what makes
    // teardown actually revoke it. Asserted on behaviour rather than on the
    // absence of a field, because a field name is not a lifetime.
    const rig = await makeRig();
    rig.emit(1);
    await settle();
    expect(rig.transport.getOutboundDatagrams()).toHaveLength(1);

    rig.identity.clear();

    // A frame that reaches the signing step after release fails loudly. It is
    // not signed with a stale key, and nothing is emitted.
    await expect(rig.submitRaw()).rejects.toBeInstanceOf(EgressIdentityReleasedError);
    expect(rig.transport.getOutboundDatagrams()).toHaveLength(1);
  });
});

describe('degradation is loud', () => {
  it('surfaces a capture end (device unplugged or access revoked) as a fatal fault', async () => {
    const rig = await makeRig();
    const faults: string[] = [];
    rig.pipeline.on('fault', (f) => faults.push(f.stage));
    rig.capture.endTrack();
    expect(faults).toEqual(['capture']);
  });

  it('counts a decoder error rather than logging one per frame', async () => {
    const rig = await makeRig();
    const faults: string[] = [];
    rig.pipeline.on('fault', (f) => faults.push(f.stage));
    rig.codecs.failDecoder();
    rig.codecs.failDecoder();
    // The counter moves per callback; the FAULT EVENT is once per stage, so a
    // decoder erroring repeatedly cannot become per-frame telemetry.
    expect(rig.count('dt_client_media_decoder_errors_total')).toBe(2);
    expect(faults).toEqual(['decoder']);
  });
});
