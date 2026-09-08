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
    config: DEFAULT_CLIENT_CONFIG.media,
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
  await pipeline.start();
  pipeline.setSendDirective({
    streamNumber: 1,
    bitrateBps: 32_000,
    targets: opts.targets ?? [MH_URL],
  });
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

  it('starts the read loop ONCE across repeated send directives', async () => {
    // MC re-issues a directive whenever meeting state changes, and a
    // `ReadableStream` admits only one reader — so a second `getReader()` throws
    // from inside a directive handler, where it would present as "media stopped
    // after someone joined" rather than as what it is.
    const transport = new MockWebTransport();
    transport.simulateReady();
    const rig = await makePipeline({ readable: transport.datagrams.readable });
    rig.pipeline.setSendDirective({ streamNumber: 1, bitrateBps: 32_000, targets: [MH_URL] });
    rig.pipeline.setSendDirective({ streamNumber: 1, bitrateBps: 32_000, targets: [MH_URL] });
    await settle();
    expect(rig.faults).toEqual([]);
    // And the one loop still works.
    transport.simulateIncomingDatagram(Uint8Array.of(0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0));
    await settle();
    expect(rig.count('dt_client_media_frames_received_total')).toBe(1);
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
