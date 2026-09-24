// File: packages/sdk-core/src/media/lifecycle/__tests__/audioPipeline.degradation.test.ts
//
// The paths that only fire when something has gone wrong. Every one of them is
// LOUD: ADR-0036 §6 makes slot state explicit on the wire precisely because
// absence of frames is not a signal, and a media path that absorbs its own
// failures silently is the every-signal-green-no-audio case this story exists to
// localise.

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
import type { DatagramSender } from '../../pipeline/egress.js';
import { buildTestFrame, makeIdentity } from '../../pipeline/__tests__/frameFixtures.js';
import { AudioPipeline, type AudioPipelineOptions } from '../AudioPipeline.js';

const KEK = new Uint8Array(AES_256_KEY_BYTES).fill(0x11);
const SENDER_ID = 258;
const MH_URL = 'https://mh.example/media';

async function settle(): Promise<void> {
  for (let i = 0; i < 25; i += 1) {
    await new Promise<void>((resolve) => {
      setTimeout(resolve, 0);
    });
  }
}

interface Options {
  sender?: DatagramSender | undefined;
  readable?: ReadableStream<Uint8Array> | undefined;
  targets?: string[];
  /** Handler URLs from `StreamAssignments`; `null` means none were delivered. */
  receiveUrls?: string[] | null;
  /** Handler URLs delivered BEFORE `start()` — MC can answer the declaration first. */
  receiveUrlsBeforeStart?: string[];
  config?: MediaConfig;
  kekProvisioned?: boolean;
}

async function makePipeline(opts: Options = {}): Promise<{
  pipeline: AudioPipeline;
  capture: FakeCaptureSource;
  sink: InMemoryMetricsSink;
  faults: string[];
  emit(): boolean;
  count(name: string): number;
}> {
  const sink = new InMemoryMetricsSink();
  const kekSource = new JoinResponseKekSource();
  if (opts.kekProvisioned !== false) kekSource.set(KEK, 0);
  const capture = new FakeCaptureSource();
  const codecs = new FakeAudioCodecs();
  const playback = new RecordingPlaybackSink();
  const identity = await MeetingIdentity.create();
  const roster = new RosterIdentityKeys(4);
  await roster.upsert({ senderId: SENDER_ID, identityPublicKey: identity.publicKey! });

  const options: AudioPipelineOptions = {
    config: opts.config ?? DEFAULT_CLIENT_CONFIG.media,
    metrics: new MediaMetrics({ clientVersion: 'v', orgId: 'o' }, sink),
    kekSource,
    roster,
    senderId: SENDER_ID,
    kekGeneration: 0,
    identity,
    declaredSlotIds: [0],
    senderFor: () => opts.sender,
    readableFor: () => opts.readable,
    reportMute: () => {},
    captureFactory: async () => capture as never,
    encoderFactory: codecs.encoderFactory as never,
    decoderFactory: codecs.decoderFactory as never,
    playbackFactory: playback.factory as never,
    clock: () => 0,
    setInterval: () => 0 as unknown as ReturnType<typeof setInterval>,
    clearInterval: () => {},
  };
  const pipeline = new AudioPipeline(options);
  const faults: string[] = [];
  pipeline.on('fault', (f) => faults.push(f.stage));
  if (opts.receiveUrlsBeforeStart) pipeline.setReceiveHandlers(opts.receiveUrlsBeforeStart);
  await pipeline.start();
  pipeline.setSendDirective({
    streamNumber: 1,
    bitrateBps: 32_000,
    targets: opts.targets ?? [MH_URL],
  });
  // By default, assignments name the handler only when the rig supplies a stream
  // for it: naming a handler with no connected transport is itself a fault.
  const receiveUrls =
    opts.receiveUrls !== undefined ? opts.receiveUrls : opts.readable ? [MH_URL] : null;
  if (receiveUrls !== null) pipeline.setReceiveHandlers(receiveUrls);
  return {
    pipeline,
    capture,
    sink,
    faults,
    emit: () => capture.emit(makeAudioData({ samples: Float32Array.of(1, 2, 3) })),
    count: (name) =>
      sink
        .getRecordedMetrics()
        .filter((m) => m.name === name)
        .reduce((sum, m) => sum + m.value, 0),
  };
}

describe('send-side degradation', () => {
  it('counts connection_closed when the transport closed underneath a send', async () => {
    // ROUTINE — a participant leaves every meeting, many times — and
    // deliberately not folded into `transport_send_refused`, whose fleet
    // contract is "reads zero forever". Folding them makes that counter an
    // unalertable mixture of "we have a bug" and "someone hung up".
    const rig = await makePipeline({
      sender: {
        send: async () => {},
        maxDatagramSize: undefined,
        isOpen: false,
      },
    });
    rig.emit();
    await settle();
    const reasons = rig.sink
      .getRecordedMetrics()
      .filter((m) => m.name === 'dt_client_media_send_dropped_total')
      .map((m) => String(m.labels.reason));
    expect(reasons).toEqual(['connection_closed']);
    expect(rig.count('dt_client_media_frames_sent_total')).toBe(0);
  });

  it('counts transport_send_refused when the transport rejects a datagram', async () => {
    const rig = await makePipeline({
      sender: {
        send: async () => {
          throw new Error('refused');
        },
        maxDatagramSize: undefined,
        isOpen: true,
      },
    });
    rig.emit();
    await settle();
    const reasons = rig.sink
      .getRecordedMetrics()
      .filter((m) => m.name === 'dt_client_media_send_dropped_total')
      .map((m) => String(m.labels.reason));
    expect(reasons).toEqual(['transport_send_refused']);
  });

  it('holds frames rather than counting a drop when no transport is attached', async () => {
    // A send was never ATTEMPTED, so nothing was refused. Frames age out through
    // the drop-oldest policy instead, which is a different condition with a
    // different remedy.
    const rig = await makePipeline({ sender: undefined });
    rig.emit();
    await settle();
    expect(rig.count('dt_client_media_send_dropped_total')).toBe(0);
    expect(rig.count('dt_client_media_frames_sent_total')).toBe(0);
  });

  it('sends nothing while the target set is EMPTY, and rotates on resume', async () => {
    // ADR-0036 §5: "A target set may be empty. That means send nothing." And
    // §4: resuming from empty rotates the transmit key.
    const sent: Uint8Array[] = [];
    const rig = await makePipeline({
      targets: [],
      sender: {
        send: async (bytes) => {
          sent.push(bytes);
        },
        maxDatagramSize: undefined,
        isOpen: true,
      },
    });
    rig.emit();
    await settle();
    expect(sent).toHaveLength(0);

    const before = rig.pipeline.transmitGeneration;
    rig.pipeline.setSendDirective({ streamNumber: 1, bitrateBps: 32_000, targets: [MH_URL] });
    expect(rig.pipeline.transmitGeneration).toBe(before + 1n);
    rig.emit();
    await settle();
    expect(sent.length).toBeGreaterThan(0);
  });

  it('surfaces a crypto fault when a frame cannot be built', async () => {
    // No KEK means no wrap, and a client cannot announce a key it cannot wrap.
    // Sending an unwrapped or unannounced frame is not an available degradation.
    const rig = await makePipeline({ kekProvisioned: false });
    rig.emit();
    await settle();
    expect(rig.faults).toEqual(['crypto']);
  });
});

describe('receive-side degradation', () => {
  it('surfaces the inbound datagram stream ENDING while the session is live', async () => {
    // Absence of frames is not a signal (ADR-0036 §6). A reader that quietly
    // stopped would be indistinguishable from a muted far end.
    const transport = new MockWebTransport();
    transport.simulateReady();
    const rig = await makePipeline({ readable: transport.datagrams.readable });
    transport.simulateDatagramReadableEnd();
    await settle();
    expect(rig.faults).toEqual(['transport']);
  });

  /** A datagram the ingress counts as received (and then rejects as malformed). */
  const PROBE = Uint8Array.of(0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0);
  const RECEIVED = 'dt_client_media_frames_received_total';

  it('starts the read loop ONCE across repeated StreamAssignments', async () => {
    // MC re-emits `StreamAssignments` on EVERY structural change in the meeting,
    // and a `ReadableStream` admits only one reader — so a second `getReader()`
    // would throw from inside an assignment handler, presenting as "media
    // stopped after someone joined". A re-emit is a no-op.
    const transport = new MockWebTransport();
    transport.simulateReady();
    const rig = await makePipeline({ readable: transport.datagrams.readable });
    rig.pipeline.setReceiveHandlers([MH_URL]);
    rig.pipeline.setReceiveHandlers([MH_URL, '']);
    // Directives no longer touch the read loop at all.
    rig.pipeline.setSendDirective({ streamNumber: 1, bitrateBps: 32_000, targets: [MH_URL] });
    await settle();
    expect(rig.faults).toEqual([]);
    // And the one loop still works.
    transport.simulateIncomingDatagram(PROBE);
    await settle();
    expect(rig.count(RECEIVED)).toBe(1);
  });

  it('RECEIVES with an EMPTY send target set — the participant nobody holds', async () => {
    // Static fill, N=1, three participants: C's slot holds A, nobody's holds C.
    // MC directs C to send nothing (§5), and C must still hear A. Keyed off the
    // send target, this read loop never started and C was deaf behind an
    // ACTIVE slot.
    const transport = new MockWebTransport();
    transport.simulateReady();
    const rig = await makePipeline({
      readable: transport.datagrams.readable,
      targets: [],
      receiveUrls: [MH_URL],
    });
    transport.simulateIncomingDatagram(PROBE);
    await settle();
    expect(rig.count(RECEIVED)).toBe(1);
    expect(rig.faults).toEqual([]);
  });

  it('opens nothing while every slot is unassigned, and starts once a slot fills', async () => {
    // A solo participant: every slot is `fewer_sources` with an EMPTY url, which
    // the proto defines as "no source assigned" — legitimate, not an error.
    const transport = new MockWebTransport();
    transport.simulateReady();
    const rig = await makePipeline({
      readable: transport.datagrams.readable,
      targets: [],
      receiveUrls: [''],
    });
    transport.simulateIncomingDatagram(PROBE);
    await settle();
    // STRUCTURE, not a counter: no reader was ever taken on the stream. A zero
    // count alone would only show the probe was not read YET — the stream
    // buffers it (see the exact count below).
    expect(transport.datagrams.readable.locked).toBe(false);
    expect(rig.count(RECEIVED)).toBe(0);
    expect(rig.faults).toEqual([]);

    // Positive control on the same rig: a peer joins and fills the slot.
    rig.pipeline.setReceiveHandlers([MH_URL]);
    transport.simulateIncomingDatagram(PROBE);
    await settle();
    expect(transport.datagrams.readable.locked).toBe(true);
    // Exactly two: the probe the stream BUFFERED while no reader existed, then
    // the new one. Buffered by the transport's queue, not read and discarded.
    expect(rig.count(RECEIVED)).toBe(2);
  });

  it('holds assignments that arrive BEFORE start() and opens the loop at start', async () => {
    // MC answers the declaration, which `startMedia` sends before `start()`.
    // Frames must not be read and discarded before the ingress exists, and the
    // assignment must not be lost either.
    const transport = new MockWebTransport();
    transport.simulateReady();
    const rig = await makePipeline({
      readable: transport.datagrams.readable,
      receiveUrlsBeforeStart: [MH_URL],
      receiveUrls: null,
    });
    transport.simulateIncomingDatagram(PROBE);
    await settle();
    expect(rig.count(RECEIVED)).toBe(1);
    expect(rig.faults).toEqual([]);
  });

  it('keeps a running loop when a later snapshot empties every slot', async () => {
    // The last co-handler peer leaves: slots go back to `fewer_sources`. The loop
    // stays, so a later refill needs no reconnect and no second reader.
    const transport = new MockWebTransport();
    transport.simulateReady();
    const rig = await makePipeline({ readable: transport.datagrams.readable });
    expect(transport.datagrams.readable.locked).toBe(true);

    // The snapshot empties. The loop must still be READING — probed now, while
    // empty, not after the refill (a loop torn down on empty and reopened on
    // refill would pass that version of this test).
    rig.pipeline.setReceiveHandlers(['']);
    transport.simulateIncomingDatagram(PROBE);
    await settle();
    expect(transport.datagrams.readable.locked).toBe(true);
    expect(rig.count(RECEIVED)).toBe(1);

    // And a refill neither reopens nor faults.
    rig.pipeline.setReceiveHandlers([MH_URL]);
    transport.simulateIncomingDatagram(PROBE);
    await settle();
    expect(rig.count(RECEIVED)).toBe(2);
    expect(rig.faults).toEqual([]);
  });

  it('faults, and opens nothing, when assignments name more than one handler', async () => {
    // MC places each participant on ONE handler this story. Two urls mean MC
    // broke its own scoping; the client says so rather than absorbing it.
    const transport = new MockWebTransport();
    transport.simulateReady();
    const rig = await makePipeline({
      readable: transport.datagrams.readable,
      receiveUrls: [MH_URL, 'https://mh-other.example:4433'],
    });
    transport.simulateIncomingDatagram(PROBE);
    await settle();
    expect(rig.faults).toEqual(['transport']);
    expect(rig.count(RECEIVED)).toBe(0);
  });

  it('faults when assignments name a handler this client is not connected to', async () => {
    // `readableFor` is a selector over connected transports; it cannot dial.
    // A url with no connected transport is surfaced, never silently skipped.
    const rig = await makePipeline({ readable: undefined, receiveUrls: [MH_URL] });
    await settle();
    expect(rig.faults).toEqual(['transport']);
  });

  it('faults, and keeps the running loop, when assignments move to a different handler', async () => {
    const transport = new MockWebTransport();
    transport.simulateReady();
    const rig = await makePipeline({ readable: transport.datagrams.readable });
    rig.pipeline.setReceiveHandlers(['https://mh-other.example:4433']);
    await settle();
    expect(rig.faults).toEqual(['transport']);
    // The original loop was not torn down to follow it.
    transport.simulateIncomingDatagram(PROBE);
    await settle();
    expect(rig.count(RECEIVED)).toBe(1);
  });

  it('a second, DIFFERENT transport fault is not silenced by the first; a repeat is', async () => {
    // Faults dedupe on (stage, message). A stage-only key let "not connected"
    // hide a later "moved mid-session" — the one that names an MC placement
    // defect — for the rest of the session.
    const transport = new MockWebTransport();
    transport.simulateReady();
    const messages: string[] = [];
    const rig = await makePipeline({ readable: transport.datagrams.readable });
    rig.pipeline.on('fault', (f) => messages.push(f.message));

    // Two different handlers named at once, then the loop moved mid-session.
    rig.pipeline.setReceiveHandlers([MH_URL, 'https://mh-other.example:4433']);
    rig.pipeline.setReceiveHandlers(['https://mh-other.example:4433']);
    // The same condition again: bounded, so NOT reported twice.
    rig.pipeline.setReceiveHandlers(['https://mh-other.example:4433']);
    await settle();

    expect(rig.faults).toEqual(['transport', 'transport']);
    expect(new Set(messages).size).toBe(2);
  });

  it('passes the CONFIGURED hop-restart bound to the monitor, not a hardcoded default', async () => {
    // The config key is validated and the monitor takes the bound as an
    // argument, but nothing else proves `AudioPipeline` connects the two. Run
    // the same input under two bounds and require different answers: A at hop
    // 10 then hop 5 is 5 behind — a RESTART under a bound of 4, a REORDER under
    // the default of 64. A pipeline that hardcoded the default passes only the
    // second arm.
    const signer = await makeIdentity(0x41);
    const frame = (hopSequence: number, streamSequence: number): Promise<Uint8Array> =>
      buildTestFrame({
        keyIdSenderId: SENDER_ID,
        kek: KEK,
        signer,
        plaintext: Uint8Array.of(1, 2, 3),
        streamId: 0,
        hopSequence,
        streamSequence,
      });

    async function reordersUnder(bound: number): Promise<number> {
      const transport = new MockWebTransport();
      transport.simulateReady();
      const rig = await makePipeline({
        readable: transport.datagrams.readable,
        config: {
          ...DEFAULT_CLIENT_CONFIG.media,
          ingress: { ...DEFAULT_CLIENT_CONFIG.media.ingress, hopRestartBackwardJumpFrames: bound },
        },
      });
      transport.simulateIncomingDatagram(await frame(10, 0));
      transport.simulateIncomingDatagram(await frame(5, 1));
      await settle();
      // Vacuity guard: both frames reached the ingress, so the hop step ran.
      expect(rig.count(RECEIVED)).toBe(2);
      return rig.count('dt_client_media_downlink_reorder_total');
    }

    expect(DEFAULT_CLIENT_CONFIG.media.ingress.hopRestartBackwardJumpFrames).toBeGreaterThan(5);
    expect(await reordersUnder(4)).toBe(0);
    expect(
      await reordersUnder(DEFAULT_CLIENT_CONFIG.media.ingress.hopRestartBackwardJumpFrames),
    ).toBe(1);
  });

  it('counts a lost inbound datagram as never received at all', async () => {
    // The mock records the loss so the assertion can SAY which frames were
    // withheld; an omission would state nothing, and `received = accepted +
    // sum(drops)` needs the left-hand side to be exact.
    const transport = new MockWebTransport();
    transport.simulateReady();
    const rig = await makePipeline({ readable: transport.datagrams.readable });
    transport.simulateDatagramLoss(Uint8Array.of(2, 0, 0, 0, 0));
    await settle();
    expect(transport.getLostInboundDatagrams()).toHaveLength(1);
    expect(rig.count('dt_client_media_frames_received_total')).toBe(0);
  });

  it('drops a malformed datagram by reason without ending the read loop', async () => {
    const transport = new MockWebTransport();
    transport.simulateReady();
    const rig = await makePipeline({ readable: transport.datagrams.readable });
    transport.simulateIncomingDatagram(Uint8Array.of(0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0));
    transport.simulateIncomingDatagram(Uint8Array.of(0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0));
    await settle();
    // Both counted, and the loop survived the first: a receive path that threw
    // on wire input would let one crafted datagram end media for the session.
    expect(rig.count('dt_client_media_frames_received_total')).toBe(2);
    expect(rig.count('dt_client_media_frames_dropped_total')).toBe(2);
    expect(rig.faults).toEqual([]);
  });
});
