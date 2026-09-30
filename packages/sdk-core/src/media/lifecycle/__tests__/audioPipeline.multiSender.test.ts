// File: packages/sdk-core/src/media/lifecycle/__tests__/audioPipeline.multiSender.test.ts
//
// THE N-SENDER RECEIVE PATH, END TO END OVER `MockWebTransport` (story 2 R-5,
// R-13, R-14, R-17, R-18; ADR-0036 §4, §9).
//
// Real crypto throughout: frames are built with real Ed25519 signatures, real
// AES-GCM wraps and real SFrame sealing, and the S7 leaver scenario takes its
// frames from a REAL sending pipeline so it can tell whether the R-13 rotation
// actually happened. Nothing in the receive path is faked except the codecs,
// the playback output and the transports.
//
// EACH SENDER'S AUDIO IS ITS OWN BYTE PATTERN, so isolation is asserted as
// "every decoder saw exactly one sender's frames, and all of them" — a mixer
// that fed everything to one decoder would pass a bare count.
//
// Determinism: no wall clock. The KEK holder runs on a fake scheduler; frame
// processing is awaited with a bounded drain (`waitFor`) on the receive-path
// identity itself, never on a duration.

import { describe, expect, it } from 'vitest';
import {
  FakeAudioCodecs,
  FakeCaptureSource,
  InMemoryMetricsSink,
  MockWebTransport,
  RecordingPlaybackSink,
  makeAudioData,
} from '@darktower/test-utils';

import {
  DEFAULT_CLIENT_CONFIG,
  deriveKekRetention,
  transmitRewrapLatencyMs,
} from '../../../config/clientConfig.js';
import { kekObserver, rosterInvalidationListener } from '../../../session/mediaWiring.js';
import { NOMINAL_DEBOUNCE_SECONDS, waitFor } from '../../__tests__/helpers.js';
import { decodeFrame } from '../../frame/frameCodec.js';
import { unpackKeyId } from '../../frame/keyId.js';
import { MEETING_KEK_BYTES, parseSframe } from '../../frame/sframe.js';
import type { DatagramSender } from '../../pipeline/egress.js';
import {
  buildTestFrame,
  makeIdentity,
  type TestIdentity,
} from '../../pipeline/__tests__/frameFixtures.js';
import { MeetingIdentity } from '../../setup/identity.js';
import { MeetingKekHolder } from '../../setup/kekSource.js';
import { MediaMetrics } from '../../setup/mediaMetrics.js';
import { RosterIdentityKeys } from '../../setup/rosterKeys.js';
import { AudioPipeline, type ReceiveAssignment } from '../AudioPipeline.js';

const T = transmitRewrapLatencyMs(DEFAULT_CLIENT_CONFIG.media);
const RETENTION_MS = deriveKekRetention(NOMINAL_DEBOUNCE_SECONDS, T).retentionMs;
const HANDLER_A = 'https://mh-a.example:4433';
const HANDLER_B = 'https://mh-b.example:4433';
const RECEIVER_SENDER_ID = 900;

const kekOf = (fill: number) => new Uint8Array(MEETING_KEK_BYTES).fill(fill);
const KEK = [kekOf(0x10), kekOf(0x11), kekOf(0x12), kekOf(0x13)] as const;

/** A remote participant: a sender id, a real identity, and its audio byte. */
interface Peer {
  readonly id: number;
  readonly identity: TestIdentity;
  /** Every byte of this peer's audio. Identifies it at the decoder. */
  readonly byte: number;
  readonly slot: number;
  readonly handler: string;
  seq: number;
}

async function makePeer(id: number, byte: number, slot: number, handler: string): Promise<Peer> {
  return { id, identity: await makeIdentity(byte), byte, slot, handler, seq: 0 };
}

/** A fake scheduler, so KEK retention runs on test time. */
function fakeScheduler() {
  let now = 10_000;
  let next = 1;
  const timers = new Map<number, { at: number; fn: () => void }>();
  return {
    clock: () => now,
    setTimeout: (fn: () => void, ms: number) => {
      const id = next++;
      timers.set(id, { at: now + ms, fn });
      return id as unknown as ReturnType<typeof setTimeout>;
    },
    clearTimeout: (h: ReturnType<typeof setTimeout>) => void timers.delete(h as unknown as number),
    advance(ms: number) {
      now += ms;
      for (const [id, t] of [...timers]) {
        if (t.at <= now) {
          timers.delete(id);
          t.fn();
        }
      }
    },
  };
}

/** One receiving client: a real pipeline, two handler transports, a fake scheduler. */
async function makeReceiver(opts: { declared?: number[] } = {}) {
  const sink = new InMemoryMetricsSink();
  const metrics = new MediaMetrics({ clientVersion: '0.0.0-test', orgId: 'demo' }, sink);
  const sched = fakeScheduler();
  const warnings: string[] = [];
  const holder = new MeetingKekHolder({
    rewrapLatencyMs: T,
    observer: kekObserver(metrics, (m) => warnings.push(m)),
    clock: sched.clock,
    setTimeout: sched.setTimeout,
    clearTimeout: sched.clearTimeout,
  });
  const roster = new RosterIdentityKeys(32);
  const codecs = new FakeAudioCodecs();
  const playback = new RecordingPlaybackSink();
  const transports = new Map<string, MockWebTransport>();
  for (const url of [HANDLER_A, HANDLER_B]) {
    const t = new MockWebTransport();
    t.simulateReady();
    transports.set(url, t);
  }
  const identity = await MeetingIdentity.create();
  const ref: { pipeline?: AudioPipeline } = {};
  const intervals: { fn: () => void; ms: number }[] = [];
  // The SAME wiring the session installs, from the SAME module.
  roster.setTransmitKeyInvalidationListener(
    rosterInvalidationListener(
      () => metrics,
      () => ref.pipeline,
    ),
  );

  const pipeline = new AudioPipeline({
    config: DEFAULT_CLIENT_CONFIG.media,
    metrics,
    kekSource: holder,
    roster,
    senderId: RECEIVER_SENDER_ID,
    identity,
    declaredSlotIds: opts.declared ?? [0, 1, 2],
    captureSource: 'microphone',
    metricExportIntervalMs: 10_000,
    senderFor: () => undefined,
    readableFor: (url) => transports.get(url)?.datagrams.readable,
    reportMute: () => {},
    captureFactory: async () => new FakeCaptureSource() as never,
    encoderFactory: codecs.encoderFactory as never,
    decoderFactory: codecs.decoderFactory as never,
    playbackFactory: playback.factory as never,
    clock: sched.clock,
    // Recorded, never run on wall-clock time: `tickExport()` fires the export-
    // interval timer (the per-interval signals) on demand.
    setInterval: (fn, ms) => {
      intervals.push({ fn, ms });
      return (intervals.length - 1) as unknown as ReturnType<typeof setInterval>;
    },
    clearInterval: () => {},
  });
  ref.pipeline = pipeline;
  await pipeline.start();

  let delivered = 0;
  const hops = new Map<string, number>();
  const metric = (name: string, match: Record<string, string> = {}) =>
    sink
      .getRecordedMetrics()
      .filter((m) => m.name === name)
      .filter((m) => Object.entries(match).every(([k, v]) => m.labels[k] === v))
      .reduce((sum, m) => sum + m.value, 0);

  const rig = {
    pipeline,
    holder,
    roster,
    codecs,
    playback,
    sink,
    sched,
    warnings,
    metric,
    drops: (reason: string) => metric('dt_client_media_frames_dropped_total', { reason }),
    /** Install a KEK exactly as MC's join response or push would deliver it. */
    install(
      gen: number,
      source: 'join_response' | 'kek_update' = 'kek_update',
      w = NOMINAL_DEBOUNCE_SECONDS,
    ) {
      return holder.install(KEK[gen]!, gen, w, source);
    },
    async admit(...peers: Peer[]) {
      for (const p of peers)
        await roster.upsert({ senderId: p.id, identityPublicKey: p.identity.publicKey });
    },
    /** Apply MC's slot assignments, then let the new lanes' decoders come up. */
    async assign(...peers: Peer[]) {
      const assignments: ReceiveAssignment[] = peers.map((p) => ({
        slotId: p.slot,
        senderId: p.id,
        mediaHandlerUrl: p.handler,
        active: true,
      }));
      pipeline.setReceiveAssignments(assignments);
      await waitFor(
        () =>
          codecs.decoders.filter((d) => !d.closed).length >= new Set(peers.map((p) => p.id)).size,
      );
    },
    /** Deliver raw bytes on `handler`, as MH forwarding would. */
    deliver(handler: string, bytes: Uint8Array) {
      delivered += 1;
      transports.get(handler)!.simulateIncomingDatagram(bytes);
    },
    /** Wait until every delivered datagram is accounted for on the identity. */
    async drain() {
      await waitFor(() => {
        const c = pipeline.frameCounts;
        return c.framesReceived === delivered && c.framesAccepted + c.framesDropped === delivered;
      });
    },
    /** The receive-path identity: received = accepted + sum(drops by reason). */
    identityHolds(): boolean {
      const c = pipeline.frameCounts;
      return (
        c.framesReceived === delivered &&
        c.framesReceived === c.framesAccepted + c.framesDropped &&
        metric('dt_client_media_frames_received_total') ===
          metric('dt_client_media_frames_accepted_total') +
            metric('dt_client_media_frames_dropped_total')
      );
    },
    /** Audio bytes each open-or-closed decoder saw, by the sender byte they carry. */
    decodedBytes(): number[][] {
      return codecs.decoders.map((d) => d.decoded.map((f) => f.data[0]!));
    },
    decodedFor(peer: Peer): number {
      return codecs.decoded.filter((f) => f.data[0] === peer.byte).length;
    },
    /** The sequence numbers of `peer`'s decoded frames, in decode order. */
    decodedSeqFor(peer: Peer): number[] {
      return codecs.decoded.filter((f) => f.data[0] === peer.byte).map((f) => f.data[1]!);
    },
    /** Fire the export-interval timer once (the capture gauge + deficit tick). */
    tickExport() {
      for (const t of intervals) if (t.ms === 10_000) t.fn();
    },
    deficit: () => metric('dt_client_media_receive_source_deficit_total'),
    nextHop(handler: string): number {
      const h = (hops.get(handler) ?? 0) + 1;
      hops.set(handler, h);
      return h;
    },
  };
  return rig;
}
type Receiver = Awaited<ReturnType<typeof makeReceiver>>;

/** Build a frame from `peer` with real crypto. Defaults: KEK gen 0, transmit gen 0. */
async function frameFrom(
  r: Receiver,
  peer: Peer,
  over: {
    kekGen?: number;
    transmitGen?: number;
    signer?: TestIdentity;
    seq?: number;
    /**
     * `false` omits the wrap: the frame opens ONLY from a transmit key the
     * receiver already has cached. That is what makes a cache purge observable —
     * a key-bearing frame would re-unwrap and play whether or not the cache was
     * purged.
     */
    keyBearing?: boolean;
  } = {},
): Promise<Uint8Array> {
  const kekGen = over.kekGen ?? 0;
  const seq = over.seq ?? peer.seq++;
  return buildTestFrame({
    keyIdSenderId: peer.id,
    generation: over.transmitGen ?? 0,
    streamSequence: seq,
    kek: KEK[kekGen]!,
    kekGeneration: kekGen,
    signer: over.signer ?? peer.identity,
    // byte 0 = the SENDER, byte 1 = the sequence: a decoder's record then shows
    // both whose audio it got AND in what order.
    plaintext: Uint8Array.of(peer.byte, seq & 0xff, peer.byte, peer.byte),
    streamId: peer.slot,
    hopSequence: r.nextHop(peer.handler),
    ...(over.keyBearing === false ? { keyBearing: false } : {}),
  });
}

async function send(r: Receiver, peer: Peer, over: Parameters<typeof frameFrom>[2] = {}) {
  const bytes = await frameFrom(r, peer, over);
  r.deliver(peer.handler, bytes);
  return bytes;
}

/** Three senders across TWO handlers, admitted, assigned, KEK gen 0 installed. */
async function threeSenderMeeting(declared?: number[]) {
  const r = await makeReceiver(declared ? { declared } : {});
  const a = await makePeer(11, 0xa1, 0, HANDLER_A);
  const b = await makePeer(22, 0xb2, 1, HANDLER_A);
  const c = await makePeer(33, 0xc3, 2, HANDLER_B);
  r.install(0, 'join_response');
  await r.admit(a, b, c);
  await r.assign(a, b, c);
  return { r, a, b, c };
}

// ===========================================================================

describe('N senders decode concurrently, each on its own decoder (R-5)', () => {
  it('decodes three interleaved senders across two handlers, disjoint and complete', async () => {
    const { r, a, b, c } = await threeSenderMeeting();
    const PER_SENDER = 5;
    for (let i = 0; i < PER_SENDER; i += 1) {
      await send(r, a);
      await send(r, c);
      await send(r, b);
    }
    await r.drain();

    expect(r.pipeline.frameCounts.framesDropped).toBe(0);
    expect(r.identityHolds()).toBe(true);
    // One decoder per sender, each holding ONLY its sender's audio, and all of it.
    const perDecoder = r.decodedBytes().filter((d) => d.length > 0);
    expect(perDecoder).toHaveLength(3);
    for (const seen of perDecoder) expect(new Set(seen).size).toBe(1);
    expect(perDecoder.map((d) => d[0]).sort()).toEqual([a.byte, b.byte, c.byte].sort());
    for (const peer of [a, b, c]) expect(r.decodedFor(peer)).toBe(PER_SENDER);
    // Each sender reaches playback on its OWN lane, mixed by the one sink.
    const lanes = r.playback.lanes.filter((l) => l.played.length > 0);
    expect(lanes).toHaveLength(3);
    for (const lane of lanes) {
      expect(new Set(lane.played.map((p) => p[0])).size).toBe(1);
      expect(lane.played).toHaveLength(PER_SENDER);
    }
  });
});

describe('one bad sender is isolated; the others are unaffected (R-5)', () => {
  /** Misbehave as `bad`, returning how many of its frames the receiver ACCEPTED. */
  type Bad = (r: Receiver, bad: Peer) => Promise<number>;
  const rows: [string, string | null, Bad][] = [
    [
      'signature failure',
      'signature_invalid',
      async (r, bad) => {
        // Authored by someone else under the bad sender's key id.
        const intruder = await makeIdentity(0x5e);
        await send(r, bad, { signer: intruder });
        return 0;
      },
    ],
    [
      'replay',
      'replay_detected',
      async (r, bad) => {
        const bytes = await send(r, bad);
        r.deliver(bad.handler, bytes);
        return 1;
      },
    ],
    [
      'missing roster key',
      'no_roster_entry',
      async (r, bad) => {
        r.roster.remove(bad.id);
        await send(r, bad);
        return 0;
      },
    ],
    [
      'decoder fault',
      null,
      async (r, bad) => {
        // Both frames verify and open, so both are ACCEPTED; the second lands
        // while this sender's decoder is being replaced.
        let accepted = 0;
        await send(r, bad);
        accepted += 1;
        await r.drain();
        const record = r.codecs.decoders.find((d) => d.decoded.some((f) => f.data[0] === bad.byte));
        record!.fail();
        await send(r, bad);
        accepted += 1;
        return accepted;
      },
    ],
  ];

  it.each(rows)('%s', async (_label, token, misbehave) => {
    const { r, a, b, c } = await threeSenderMeeting();
    const ROUNDS = 3;
    let goodSent = 0;
    let acceptedForB = 0;
    for (let i = 0; i < ROUNDS; i += 1) {
      await send(r, a);
      await send(r, c);
      goodSent += 1;
      if (i === 1) acceptedForB = await misbehave(r, b);
    }
    await r.drain();
    expect(r.identityHolds()).toBe(true);

    // The good senders' counts are what they would be with the bad one absent —
    // taken from what was SENT, never a hand-typed number.
    expect(r.decodedFor(a)).toBe(goodSent);
    expect(r.decodedFor(c)).toBe(goodSent);

    if (token !== null) {
      expect(r.drops(token)).toBe(1);
      expect(r.pipeline.frameCounts.lastDropReason).toBe(token);
    } else {
      // The decoder-fault row: counted as a decoder error, never as a frame drop.
      expect(r.metric('dt_client_media_decoder_errors_total')).toBe(1);
      expect(r.pipeline.frameCounts.framesDropped).toBe(0);
      // Only the bad sender's decoder closed; the good ones stayed open.
      const closed = r.codecs.decoders.filter((d) => d.closed);
      expect(closed).toHaveLength(1);
      expect(closed[0]!.decoded.every((f) => f.data[0] === b.byte)).toBe(true);
      // SECOND-SEGMENT identity for the faulted lane, and it is an INEQUALITY
      // ONLY HERE: a frame in flight in a decoder that is being closed is
      // legitimately lost, so decoded + evicted + pending may fall short of what
      // was accepted for that sender. Do not "fix" this into an equality.
      // The pre-fault frame DID play: without this lower bound a lane that never
      // decoded anything would satisfy the inequality below vacuously.
      expect(r.decodedFor(b)).toBeGreaterThanOrEqual(1);
      expect(
        r.decodedFor(b) +
          r.metric('dt_client_media_decode_queue_dropped_total') +
          r.pipeline.decodePendingFor(b.id),
      ).toBeLessThanOrEqual(acceptedForB);
    }
    // The fault never reached anyone else's playback lane.
    for (const lane of r.playback.lanes) {
      const senders = new Set(lane.played.map((p) => p[0]));
      expect(senders.size).toBeLessThanOrEqual(1);
    }
  });

  it('keeps the SECOND-SEGMENT identity exact per lane when nothing faults', async () => {
    const { r, a, b, c } = await threeSenderMeeting();
    for (let i = 0; i < 4; i += 1) for (const p of [a, b, c]) await send(r, p);
    await r.drain();
    await waitFor(() => [a, b, c].every((p) => r.pipeline.decodePendingFor(p.id) === 0));
    for (const p of [a, b, c]) {
      expect(
        r.decodedFor(p) +
          r.metric('dt_client_media_decode_queue_dropped_total') +
          r.pipeline.decodePendingFor(p.id),
      ).toBe(4);
    }
  });
});

describe('the slot-edge gate: a verified frame from an UNASSIGNED sender is not decoded', () => {
  it('drops it as sender_not_assigned, opens no decoder, and the SAME bytes play once assigned', async () => {
    // A fourth DECLARED slot, empty at first, so D can later be assigned to a
    // slot of its own without displacing anyone.
    const { r, a, b, c } = await threeSenderMeeting([0, 1, 2, 3]);
    // D is on the roster with a valid key (§9 puts everyone there) but MC has
    // assigned it to none of this client's slots.
    const d = await makePeer(44, 0xd4, 3, HANDLER_A);
    await r.admit(d);
    const decodersBefore = r.codecs.decoders.length;
    const bytes = await frameFrom(r, d);
    r.deliver(d.handler, bytes);
    await r.drain();

    expect(r.drops('sender_not_assigned')).toBe(1);
    expect(r.pipeline.frameCounts.framesAccepted).toBe(0);
    expect(r.codecs.decoders).toHaveLength(decodersBefore);
    expect(r.decodedFor(d)).toBe(0);

    // POSITIVE CONTROL with the IDENTICAL datagram: it now plays. This is what
    // proves the gate ran BEFORE the frame was opened — had it advanced the
    // replay window or cached the wrap, the redelivery would be replay_detected.
    await r.assign(a, b, c, d);
    r.deliver(d.handler, bytes);
    await r.drain();
    expect(r.decodedFor(d)).toBe(1);
    expect(r.drops('replay_detected')).toBe(0);
    expect(r.identityHolds()).toBe(true);
  });

  it("a re-map closes ONLY the departing sender's decoder, and a kept sender keeps its replay state", async () => {
    const { r, a, b, c } = await threeSenderMeeting();
    const aFrame = await send(r, a);
    await send(r, b);
    await r.drain();
    const aDecoder = r.codecs.decoders.find((dec) =>
      dec.decoded.some((f) => f.data[0] === a.byte),
    )!;
    const bDecoder = r.codecs.decoders.find((dec) =>
      dec.decoded.some((f) => f.data[0] === b.byte),
    )!;

    // B leaves the assignment set; A and C are unchanged.
    await r.assign(a, c);
    expect(bDecoder.closed).toBe(true);
    expect(aDecoder.closed).toBe(false);

    // An unchanged slot never resets crypto or replay state: A's old frame is
    // still a replay after the re-map.
    r.deliver(a.handler, aFrame);
    await r.drain();
    expect(r.drops('replay_detected')).toBe(1);
  });
});

describe('KEK rotation on the receive side (R-14)', () => {
  it('plays in-flight PREVIOUS-generation frames across the switch with no gap', async () => {
    const { r, a, b } = await threeSenderMeeting();
    await send(r, a);
    await send(r, b);
    await r.drain();

    expect(r.install(1)).toBe('installed_retaining');
    // Frames in flight when the rotation landed: new key ids (not cached), still
    // wrapped under generation 0 — only retention can open them.
    await send(r, a, { transmitGen: 1, kekGen: 0 });
    await send(r, b, { transmitGen: 1, kekGen: 0 });
    // And the senders' post-rotation frames.
    await send(r, a, { transmitGen: 2, kekGen: 1 });
    await send(r, b, { transmitGen: 2, kekGen: 1 });
    await r.drain();

    expect(r.pipeline.frameCounts.framesDropped).toBe(0);
    expect(r.decodedFor(a)).toBe(3);
    expect(r.decodedFor(b)).toBe(3);
    // Each decoder holds ONE sender's audio...
    for (const seen of r.decodedBytes().filter((s) => s.length > 0)) {
      expect(new Set(seen).size).toBe(1);
    }
    // ...and IN SEND ORDER straight across the switch: the pre-rotation frame,
    // the in-flight previous-generation frame, then the post-rotation frame. The
    // sequence rides in the plaintext, so a reordering or a gap is visible here.
    expect(r.decodedSeqFor(a)).toEqual([0, 1, 2]);
    expect(r.decodedSeqFor(b)).toEqual([0, 1, 2]);
    expect(r.metric('dt_client_media_kek_generations_retained_total')).toBe(1);
    expect(r.identityHolds()).toBe(true);
  });

  it('opens the previous generation just inside the window, and drops it at the window', async () => {
    const { r, a } = await threeSenderMeeting();
    r.install(1);
    r.sched.advance(RETENTION_MS - 1);
    await send(r, a, { transmitGen: 1, kekGen: 0 });
    await r.drain();
    expect(r.decodedFor(a)).toBe(1);

    r.sched.advance(1);
    await send(r, a, { transmitGen: 2, kekGen: 0 });
    await r.drain();
    expect(r.drops('kek_generation_stale')).toBe(1);
    expect(r.decodedFor(a)).toBe(1);
  });

  it('after a SECOND rotation: older than retained is stale, newer than held is no_kek', async () => {
    const { r, a } = await threeSenderMeeting();
    r.install(1);
    r.install(2);
    await send(r, a, { transmitGen: 5, kekGen: 0 });
    await send(r, a, { transmitGen: 6, kekGen: 3 });
    await send(r, a, { transmitGen: 7, kekGen: 1 });
    await r.drain();
    expect(r.drops('kek_generation_stale')).toBe(1);
    expect(r.drops('no_kek_for_generation')).toBe(1);
    // Generation 1 is the retained previous, so it still opens.
    expect(r.decodedFor(a)).toBe(1);
    expect(r.identityHolds()).toBe(true);
  });

  it('a JOINER after the rotation opens current frames, and drops a pre-rotation one as stale', async () => {
    const r = await makeReceiver();
    const a = await makePeer(11, 0xa1, 0, HANDLER_A);
    // Joined after the rotation: MC's join response carries generation 1 only.
    r.install(1, 'join_response');
    await r.admit(a);
    await r.assign(a);
    // The in-flight pre-rotation frame reaches the joiner first — the realistic
    // order — but the ORDER NO LONGER DECIDES THE TOKEN: replay state is scoped
    // by KEK generation, and a frame under a generation this receiver never held
    // is refused BEFORE any replay state is consulted (story 2 R-16). So it is
    // `kek_generation_stale` whichever transmit generation arrived first.
    await send(r, a, { transmitGen: 2, kekGen: 0 });
    await send(r, a, { transmitGen: 3, kekGen: 1 });
    await r.drain();
    expect(r.decodedFor(a)).toBe(1);
    expect(r.drops('kek_generation_stale')).toBe(1);
  });
});

// ---------------------------------------------------------------------------
// Story 2 R-16 under (A'): receiver state scoped by (kek_generation, sender_id),
// through the REAL holder and the pipeline's one generation-dropped listener.
// Case ids are the devloop plan's ("sdk-core test cases").
// ---------------------------------------------------------------------------

describe('scoped receiver state, end to end (story 2 R-16, C-3)', () => {
  it('D1 — a reissued sender id is heard: new identity, same id, new generation, transmit gen 0', async () => {
    // Red against: the shipped unscoped high-water — the departed holder's
    // transmit generation 40 deafened the new holder forever (S-1).
    const { r, a } = await threeSenderMeeting();
    await send(r, a, { transmitGen: 40, kekGen: 0 });
    await r.drain();
    expect(r.decodedFor(a)).toBe(1);

    // The departed holder's ParticipantLeft, then MC's epoch reset (a new
    // generation), then the new holder's ParticipantJoined on the SAME id.
    r.roster.remove(a.id);
    r.install(1);
    const reissued = await makePeer(a.id, 0xe5, a.slot, a.handler);
    await r.admit(reissued);
    await send(r, reissued, { transmitGen: 0, kekGen: 1 });
    await send(r, reissued, { transmitGen: 0, kekGen: 1 });
    await r.drain();

    expect(r.decodedFor(reissued)).toBe(2);
    expect(r.drops('replay_detected')).toBe(0);
    // A first binding after a removal, not an MC-defect rebind.
    expect(r.metric('dt_client_media_roster_key_rebinds_total', { outcome: 'rebind' })).toBe(0);
    expect(r.identityHolds()).toBe(true);
  });

  it('C3 — at expiry, the scope s replay state AND its transmit keys go together, before replay is consulted', async () => {
    // Red against: discarding later, or on a separate timer, or only one of
    // the two. One observation after the holder's timer fires: a REPLAY of an
    // accepted frame is `kek_generation_stale` (refused before any replay state
    // — not `replay_detected`), and a wrapless frame under the same key id finds
    // no transmit key (`no_transmit_key`).
    const { r, a } = await threeSenderMeeting();
    const accepted = await send(r, a, { transmitGen: 3, kekGen: 0 });
    await r.drain();
    r.install(1);
    r.sched.advance(RETENTION_MS - 1);
    // C2, never earlier: still retained, the replay is caught by g's own bucket.
    r.deliver(a.handler, accepted);
    await r.drain();
    expect(r.drops('replay_detected')).toBe(1);

    r.sched.advance(1);
    r.deliver(a.handler, accepted);
    await send(r, a, { transmitGen: 3, kekGen: 0, keyBearing: false });
    await r.drain();
    expect(r.drops('kek_generation_stale')).toBe(1);
    expect(r.drops('no_transmit_key')).toBe(1);
    expect(r.drops('replay_detected')).toBe(1);
    expect(r.identityHolds()).toBe(true);
  });

  it('C6 — retention, not LRU, decides cross-generation admission', async () => {
    // Red against: scope lifetime driven by an LRU over generations, or a
    // context budget shared across scopes. Flooding the sender's NEW scope far
    // past its per-scope budget leaves its RETAINED scope intact — the replay is
    // still caught there — and only the holder's expiry ends it.
    const { r, a } = await threeSenderMeeting();
    const accepted = await send(r, a, { transmitGen: 3, kekGen: 0 });
    await r.drain();
    r.install(1);
    const budget = DEFAULT_CLIENT_CONFIG.media.receiverState.maxReplayContextsPerSender;
    for (let t = 4; t < 4 + budget + 8; t += 1) await send(r, a, { transmitGen: t, kekGen: 1 });
    await r.drain();

    r.deliver(a.handler, accepted);
    await r.drain();
    expect(r.drops('replay_detected')).toBe(1);

    r.sched.advance(RETENTION_MS);
    r.deliver(a.handler, accepted);
    await r.drain();
    expect(r.drops('kek_generation_stale')).toBe(1);
  });

  it('B6 — the E-1 refusal is COUNTED as wrap_generation_conflict, and is not a drop', async () => {
    // Red against: the value missing from `ReportableWrapOutcome`, or the refusal
    // counted as a drop (which would break received = accepted + sum(drops)).
    const { r, a } = await threeSenderMeeting();
    r.install(1);
    // Cached under the retained generation 0...
    await send(r, a, { transmitGen: 5, kekGen: 0 });
    await r.drain();
    const dropsBefore = r.pipeline.frameCounts.framesDropped;
    // ...then the SAME transmit key re-wrapped under the held current 1.
    await send(r, a, { transmitGen: 5, kekGen: 1 });
    await r.drain();

    expect(
      r.metric('dt_client_media_key_wrap_outcomes_total', { outcome: 'wrap_generation_conflict' }),
    ).toBe(1);
    expect(r.pipeline.frameCounts.framesDropped).toBe(dropsBefore);
    expect(r.decodedFor(a)).toBe(2);
    expect(r.identityHolds()).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// S7 and R-13, from a REAL sending pipeline
// ---------------------------------------------------------------------------

/** A real sending client: its own pipeline, KEK holder, identity and transport. */
async function makeSender(senderId: number) {
  const holder = new MeetingKekHolder({ rewrapLatencyMs: T });
  holder.install(KEK[0]!, 0, NOMINAL_DEBOUNCE_SECONDS, 'join_response');
  const identity = await MeetingIdentity.create();
  const capture = new FakeCaptureSource();
  const codecs = new FakeAudioCodecs();
  const transport = new MockWebTransport();
  transport.simulateReady();
  const writer = transport.datagrams.writable.getWriter();
  const datagrams: DatagramSender = {
    async send(bytes: Uint8Array): Promise<void> {
      await writer.ready;
      await writer.write(bytes);
    },
    get maxDatagramSize(): number | undefined {
      return transport.datagrams.maxDatagramSize;
    },
    isOpen: true,
  };
  const pipeline = new AudioPipeline({
    config: DEFAULT_CLIENT_CONFIG.media,
    metrics: new MediaMetrics({ clientVersion: 'v', orgId: 'o' }, new InMemoryMetricsSink()),
    kekSource: holder,
    roster: new RosterIdentityKeys(8),
    senderId,
    identity,
    declaredSlotIds: [0],
    captureSource: 'microphone',
    metricExportIntervalMs: 10_000,
    senderFor: () => datagrams,
    readableFor: () => undefined,
    reportMute: () => {},
    captureFactory: async () => capture as never,
    encoderFactory: codecs.encoderFactory as never,
    decoderFactory: codecs.decoderFactory as never,
    playbackFactory: new RecordingPlaybackSink().factory as never,
    clock: () => 0,
    setInterval: () => 0 as unknown as ReturnType<typeof setInterval>,
    clearInterval: () => {},
  });
  await pipeline.start();
  pipeline.setSendDirective({ streamNumber: 1, bitrateBps: 32_000, targets: [HANDLER_A] });
  return {
    senderId,
    holder,
    publicKey: identity.publicKey!,
    /** Capture one frame and wait for the datagram it produces. */
    async speak(byte: number): Promise<Uint8Array> {
      const before = transport.getOutboundDatagrams().length;
      capture.emit(makeAudioData({ samples: Float32Array.of(byte, byte, byte) }));
      await waitFor(() => transport.getOutboundDatagrams().length === before + 1);
      return transport.getOutboundDatagrams()[before]!;
    },
  };
}

function inspect(datagram: Uint8Array) {
  const decoded = decodeFrame(datagram);
  return {
    transmitGeneration: unpackKeyId(parseSframe(decoded.payload).keyId).generation,
    wrapKekGeneration: decoded.wrappedTransmitKey?.kekGeneration,
  };
}

describe('R-13: a sender rotates its transmit keys on receipt of a new KEK', () => {
  it('bumps the transmit generation and wraps the next frame under the NEW KEK', async () => {
    const s = await makeSender(77);
    const before = inspect(await s.speak(0x0b));
    expect(s.holder.install(KEK[1]!, 1, NOMINAL_DEBOUNCE_SECONDS, 'kek_update')).toBe(
      'installed_retaining',
    );
    const after = inspect(await s.speak(0x0b));
    expect(after.transmitGeneration).toBeGreaterThan(before.transmitGeneration);
    expect(before.wrapKekGeneration).toBe(0);
    expect(after.wrapKekGeneration).toBe(1);
  });

  it('does NOT rotate on a redelivery of the SAME generation (no reconnect rotation storm)', async () => {
    const s = await makeSender(78);
    const before = inspect(await s.speak(0x0c));
    expect(s.holder.install(KEK[0]!, 0, NOMINAL_DEBOUNCE_SECONDS, 'kek_update')).toBe(
      'already_held',
    );
    const after = inspect(await s.speak(0x0c));
    expect(after.transmitGeneration).toBe(before.transmitGeneration);
  });
});

describe('S7: a leaver holding only the old KEK cannot open post-rotation frames', () => {
  it("drops the sender's post-rotation frame as no_kek_for_generation and decodes nothing new", async () => {
    // A departed browser tears down, so this is expressible only here. The
    // frames come from the REAL egress after the KEK update — a hand-built frame
    // with a chosen generation could not tell whether R-13 actually rotated.
    const s = await makeSender(55);
    const speaker: Peer = {
      id: s.senderId,
      identity: { seed: new Uint8Array(0), signingKey: undefined as never, publicKey: s.publicKey },
      byte: 0x0d,
      slot: 0,
      handler: HANDLER_A,
      seq: 0,
    };
    const leaver = await makeReceiver();
    const member = await makeReceiver();
    for (const rx of [leaver, member]) {
      rx.install(0, 'join_response');
      await rx.admit(speaker);
      await rx.assign(speaker);
    }

    const beforeRotation = await s.speak(0x0d);
    // MC rotates on the leave: the member and the sender receive generation 1;
    // the leaver, having left, never does.
    member.install(1);
    s.holder.install(KEK[1]!, 1, NOMINAL_DEBOUNCE_SECONDS, 'kek_update');
    const afterRotation = await s.speak(0x0d);

    for (const rx of [leaver, member]) {
      rx.deliver(HANDLER_A, beforeRotation);
      rx.deliver(HANDLER_A, afterRotation);
      await rx.drain();
    }

    // The leaver heard the pre-rotation frame and NOTHING after it.
    expect(leaver.decodedFor(speaker)).toBe(1);
    expect(leaver.drops('no_kek_for_generation')).toBe(1);
    // A current member hears both.
    expect(member.decodedFor(speaker)).toBe(2);
    expect(member.pipeline.frameCounts.framesDropped).toBe(0);
    expect(member.metric('dt_client_media_kek_updates_total', { source: 'kek_update' })).toBe(1);
  });
});

describe('roster rebind purges transmit keys and keeps replay state (R-18)', () => {
  // HOW A PURGE IS MADE OBSERVABLE. A key-bearing frame carries its own wrap, so
  // a receiver whose cache was purged simply re-unwraps it and the frame plays
  // either way — a test sending only key-bearing frames cannot tell a purge
  // happened. A `keyBearing: false` frame under an already-cached key id opens
  // ONLY from the cache: it plays if the key survived, and drops as
  // `no_transmit_key` if it was purged. Every purge assertion below uses one.

  it('purges and counts on a rebind, and rejects a rebind-BACK replay', async () => {
    const { r, a } = await threeSenderMeeting();
    const original = await send(r, a);
    await r.drain();
    expect(r.decodedFor(a)).toBe(1);

    // MC rebinds A's live sender id to a DIFFERENT key.
    const other = await makeIdentity(0x6f);
    await r.roster.upsert({ senderId: a.id, identityPublicKey: other.publicKey });
    expect(r.metric('dt_client_media_roster_key_rebinds_total', { outcome: 'rebind' })).toBe(1);
    // A frame signed by A's ORIGINAL key no longer verifies.
    await send(r, a);
    await r.drain();
    expect(r.drops('signature_invalid')).toBe(1);

    // Rebind BACK to the original key, then replay a frame the window accepted
    // before. Replay state survived both rebinds, so it is rejected.
    await r.roster.upsert({ senderId: a.id, identityPublicKey: a.identity.publicKey });
    expect(r.metric('dt_client_media_roster_key_rebinds_total', { outcome: 'rebind' })).toBe(2);
    r.deliver(a.handler, original);
    await r.drain();
    expect(r.drops('replay_detected')).toBe(1);

    // THE PURGE ITSELF: a fresh, validly signed, NON-key-bearing frame under the
    // very key id A's cached transmit key belonged to. It verifies (A is bound to
    // its original key again) and passes the gate, but the unwrapped key was
    // purged, so there is nothing to open it with.
    await send(r, a, { keyBearing: false });
    await r.drain();
    expect(r.drops('no_transmit_key')).toBe(1);
    expect(r.decodedFor(a)).toBe(1);
  });

  it('purges on a FORGET too: remove, then re-add with the SAME key (a first binding)', async () => {
    // The rebind check is blind across a forget — a re-add after removal is a
    // first binding — so forgetting must purge on its own. This is the case that
    // catches a refactor to "purge only on rebind".
    const { r, a } = await threeSenderMeeting();
    await send(r, a);
    await r.drain();
    expect(r.decodedFor(a)).toBe(1);

    r.roster.remove(a.id);
    await r.roster.upsert({ senderId: a.id, identityPublicKey: a.identity.publicKey });
    // A first binding, not a rebind: nothing counted.
    expect(r.metric('dt_client_media_roster_key_rebinds_total')).toBe(0);

    await send(r, a, { keyBearing: false });
    await r.drain();
    expect(r.drops('no_transmit_key')).toBe(1);
    expect(r.decodedFor(a)).toBe(1);
  });

  it('counts a DOWNGRADE apart from a rebind, and still purges', async () => {
    const { r, a } = await threeSenderMeeting();
    await send(r, a);
    await r.drain();
    // MC publishes an EMPTY key for A — a documented occurrence, not a rebind.
    await r.roster.upsert({ senderId: a.id, identityPublicKey: new Uint8Array(0) });
    expect(r.metric('dt_client_media_roster_key_rebinds_total', { outcome: 'downgrade' })).toBe(1);
    expect(r.metric('dt_client_media_roster_key_rebinds_total', { outcome: 'rebind' })).toBe(0);
    // Keyless now, so A's next frame fails closed before verify. This is the
    // FAIL-CLOSED half; it cannot observe the purge, because the cache is never
    // consulted for a frame that stops at `no_roster_entry`.
    await send(r, a, { keyBearing: false });
    await r.drain();
    expect(r.drops('no_roster_entry')).toBe(1);

    // THE PURGE, isolated. Re-bind A to its ORIGINAL key: keyless -> key is a
    // FIRST binding, which counts nothing and purges nothing. So a probe that
    // now fails as `no_transmit_key` can only be explained by the downgrade's
    // own purge — which is what catches a downgrade-specific skip in the wiring.
    await r.roster.upsert({ senderId: a.id, identityPublicKey: a.identity.publicKey });
    expect(r.metric('dt_client_media_roster_key_rebinds_total', { outcome: 'rebind' })).toBe(0);
    await send(r, a, { keyBearing: false });
    await r.drain();
    expect(r.drops('no_transmit_key')).toBe(1);
    expect(r.decodedFor(a)).toBe(1);
  });

  it('does NOT count or purge for equal bytes, or for a first binding of a keyless entry', async () => {
    const { r, a } = await threeSenderMeeting();
    // Cache A's transmit key first, so there is something a purge would remove.
    await send(r, a);
    await r.drain();

    await r.roster.upsert({
      senderId: a.id,
      identityPublicKey: Uint8Array.from(a.identity.publicKey),
    });
    const keyless = await makePeer(66, 0xe6, 0, HANDLER_A);
    await r.roster.upsert({ senderId: keyless.id, identityPublicKey: new Uint8Array(0) });
    await r.roster.upsert({ senderId: keyless.id, identityPublicKey: keyless.identity.publicKey });
    expect(r.metric('dt_client_media_roster_key_rebinds_total')).toBe(0);

    // PROOF the cache survived: a non-key-bearing frame can open ONLY from it.
    await send(r, a, { keyBearing: false });
    await r.drain();
    expect(r.drops('no_transmit_key')).toBe(0);
    expect(r.decodedFor(a)).toBe(2);
  });
});

describe('peer-key validation fails closed (R-17)', () => {
  it.each([
    ['empty', new Uint8Array(0)],
    ['31 bytes', new Uint8Array(31).fill(1)],
    ['33 bytes', new Uint8Array(33).fill(1)],
  ])('drops a peer with a %s key as no_roster_entry', async (_label, key) => {
    const r = await makeReceiver();
    const p = await makePeer(12, 0xa2, 0, HANDLER_A);
    r.install(0, 'join_response');
    await r.roster.upsert({ senderId: p.id, identityPublicKey: key });
    await r.assign(p);
    await send(r, p);
    await r.drain();
    expect(r.drops('no_roster_entry')).toBe(1);
    expect(r.decodedFor(p)).toBe(0);
  });

  it('never accepts a frame under a 32-byte key that is not a valid public key', async () => {
    // WHY THERE IS NO "UNIMPORTABLE 32-BYTE KEY -> no_roster_entry" ROW: on this
    // platform EVERY 32-byte value imports as a raw Ed25519 key — including a
    // point not on the curve (verified: Node 22's WebCrypto). So that branch of
    // `RosterIdentityKeys.upsert` cannot be produced honestly here, and faking
    // it would test the fake. What CAN be asserted is the property that branch
    // exists for: such a key never lets a frame through. On a platform whose
    // import rejects it, the drop is `no_roster_entry`; here it is
    // `signature_invalid`. Either way nothing is accepted.
    const r = await makeReceiver();
    const p = await makePeer(13, 0xa3, 0, HANDLER_A);
    const notOnCurve = new Uint8Array(32);
    notOnCurve[0] = 2;
    r.install(0, 'join_response');
    await r.roster.upsert({ senderId: p.id, identityPublicKey: notOnCurve });
    await r.assign(p);
    await send(r, p);
    await r.drain();
    expect(r.pipeline.frameCounts.framesAccepted).toBe(0);
    expect(r.drops('no_roster_entry') + r.drops('signature_invalid')).toBe(1);
  });
});

describe('every media metric stays aggregate across senders', () => {
  it('carries no sender, meeting or stream label, and key_custody=operator, after a full scenario', async () => {
    const { r, a, b, c } = await threeSenderMeeting();
    for (const p of [a, b, c]) await send(r, p);
    r.install(1);
    r.install(0, 'kek_update');
    r.install(2, 'kek_update', 0);
    await send(r, a, { transmitGen: 9, kekGen: 3 });
    const d = await makePeer(44, 0xd4, 0, HANDLER_A);
    await r.admit(d);
    await send(r, d);
    await r.roster.upsert({
      senderId: b.id,
      identityPublicKey: (await makeIdentity(0x7a)).publicKey,
    });
    await r.drain();

    const recorded = r.sink
      .getRecordedMetrics()
      .filter((m) => m.name.startsWith('dt_client_media_'));
    // Non-vacuous: the scenario must actually have recorded the metrics under test.
    const names = new Set(recorded.map((m) => m.name));
    expect(names.size).toBeGreaterThanOrEqual(8);
    for (const required of [
      'dt_client_media_kek_updates_total',
      'dt_client_media_kek_generations_retained_total',
      'dt_client_media_kek_install_refusals_total',
      'dt_client_media_kek_retention_anomalies_total',
      'dt_client_media_roster_key_rebinds_total',
      'dt_client_media_frames_dropped_total',
    ]) {
      expect(names, `the scenario never recorded ${required}`).toContain(required);
    }

    const ALLOWED = new Set([
      'client_version',
      'org_id',
      'key_custody',
      'reason',
      'outcome',
      'action',
      'source',
      // Story 2 task 13: `dt_client_media_capture_source{mode}` — build-time,
      // identity-free, datapoint-only (GC MEDIA_DATAPOINT_EXTRA / keep_keys).
      'mode',
    ]);
    const FORBIDDEN = /sender|meeting|stream|participant|slot/;
    for (const m of recorded) {
      for (const key of Object.keys(m.labels)) {
        expect(ALLOWED.has(key), `${m.name} carries label ${key}`).toBe(true);
        expect(FORBIDDEN.test(key), `${m.name} carries identity label ${key}`).toBe(false);
      }
      expect(m.labels.key_custody, m.name).toBe('operator');
    }
    expect(r.metric('dt_client_media_kek_updates_total', { source: 'kek_update' })).toBe(3);
  });
});

describe('R-13 holds from the first captured frame, not from when start() resolves', () => {
  it('rotates on a KEK demotion that lands WHILE capture is starting', async () => {
    // The capture callback is registered inside `capture.start` and can mint
    // before it resolves. A subscription made only after start resolved would
    // miss a demotion in that window, leaving the sender on a transmit key a
    // leaver holds until the next timer or unmute rotation.
    const holder = new MeetingKekHolder({ rewrapLatencyMs: T });
    holder.install(KEK[0]!, 0, NOMINAL_DEBOUNCE_SECONDS, 'join_response');
    const codecs = new FakeAudioCodecs();
    const capture = {
      async start(): Promise<void> {
        // MC's rotation push arrives mid-start.
        holder.install(KEK[1]!, 1, NOMINAL_DEBOUNCE_SECONDS, 'kek_update');
      },
      stop(): void {},
    };
    const pipeline = new AudioPipeline({
      config: DEFAULT_CLIENT_CONFIG.media,
      metrics: new MediaMetrics({ clientVersion: 'v', orgId: 'o' }, new InMemoryMetricsSink()),
      kekSource: holder,
      roster: new RosterIdentityKeys(8),
      senderId: 79,
      identity: await MeetingIdentity.create(),
      declaredSlotIds: [0],
      captureSource: 'microphone',
      metricExportIntervalMs: 10_000,
      senderFor: () => undefined,
      readableFor: () => undefined,
      reportMute: () => {},
      captureFactory: async () => capture as never,
      encoderFactory: codecs.encoderFactory as never,
      decoderFactory: codecs.decoderFactory as never,
      playbackFactory: new RecordingPlaybackSink().factory as never,
      clock: () => 0,
      setInterval: () => 0 as unknown as ReturnType<typeof setInterval>,
      clearInterval: () => {},
    });
    expect(pipeline.transmitGeneration).toBe(0n);
    await pipeline.start();
    expect(pipeline.transmitGeneration).toBe(1n);
    await pipeline.stop();
  });
});

describe('receive-source deficit against REAL decode activity (story 2 R-28)', () => {
  it('decoding between ticks keeps the deficit at 0; a sender that goes silent then counts', async () => {
    const { r, a } = await threeSenderMeeting();
    // Only A remains assigned (and active) from here on.
    r.pipeline.setReceiveAssignments([
      { slotId: a.slot, senderId: a.id, mediaHandlerUrl: a.handler, active: true },
    ]);
    r.tickExport(); // grace: A's assignment is new relative to the start sample
    for (let tick = 0; tick < 3; tick += 1) {
      await send(r, a);
      await r.drain();
      r.tickExport();
    }
    // A decoded in every interval: the lane-activity wiring really feeds the
    // decision (a regression to "no activity" would read 3 here).
    expect(r.deficit()).toBe(0);
    // Now A falls silent while MC still says it is active.
    r.tickExport();
    expect(r.deficit()).toBe(1);
  });
});
