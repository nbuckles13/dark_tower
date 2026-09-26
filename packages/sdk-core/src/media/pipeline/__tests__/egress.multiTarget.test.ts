// File: packages/sdk-core/src/media/pipeline/__tests__/egress.multiTarget.test.ts
//
// MULTI-TARGET SEND (ADR-0036 §9): one sealed frame, N transports.
//
// A sender's targets are exactly the handlers owning one of its outgoing edges,
// so a sender whose edges span handlers sends to all of them. The canonical case
// needs it: A is connected to both handlers, B only to the first, C only to the
// second, so A's audio must reach both or one of B/C never hears A.
//
// ---------------------------------------------------------------------------
// WHAT THESE TESTS ARE GUARDING, AND WHY BYTE EQUALITY IS NOT ENOUGH ALONE
// ---------------------------------------------------------------------------
//
// The dangerous refactor here is "one `EgressPipeline` per target", which this
// file's subject deliberately does NOT do. Because `submit()` allocates the
// stream sequence itself, N pipelines means N sequence allocations for one frame
// at best and N `TransmitKeyManager`s at worst — and a second manager for one
// sender is AES-GCM nonce reuse, which leaks the authentication subkey and
// permits forgery rather than merely exposing two frames (ADR-0036 §2).
//
// So there are three assertions, not one. `identical bytes` pins seal-once
// directly; `one sequence per frame` catches a second allocation; and the
// `(keyId, streamSequence)` uniqueness test states the crypto property in its
// own terms, so it survives someone "fixing" the byte-equality assertion when a
// legitimately per-target field starts differing. The redundancy is the point.

import { describe, expect, it } from 'vitest';
import { InMemoryMetricsSink } from '@darktower/test-utils';

import { DEFAULT_CLIENT_CONFIG } from '../../../config/clientConfig.js';
import { MeetingIdentity } from '../../setup/identity.js';
import { kekHolderWith } from '../../__tests__/helpers.js';
import { MediaMetrics } from '../../setup/mediaMetrics.js';
import { AES_256_KEY_BYTES } from '../../frame/sframe.js';
import { decodeFrame } from '../../frame/frameCodec.js';
import { parseSframe } from '../../frame/sframe.js';
import { TransmitKeyManager } from '../../lifecycle/transmitKeys.js';
import { EgressPipeline, type DatagramSender } from '../egress.js';

const KEK = new Uint8Array(AES_256_KEY_BYTES).fill(0x11);
const SENDER_ID = 258;
const FIRST = 'https://mh-0.example:4433';
const SECOND = 'https://mh-1.example:4433';
const THIRD = 'https://mh-2.example:4433';

async function settle(): Promise<void> {
  for (let i = 0; i < 25; i += 1) {
    await new Promise<void>((resolve) => {
      setTimeout(resolve, 0);
    });
  }
}

/** A sender that records every datagram it accepted. */
class RecordingSender implements DatagramSender {
  readonly sent: Uint8Array[] = [];
  isOpen = true;
  maxDatagramSize: number | undefined = undefined;
  #stalled = false;

  /** Block further sends, as a congested or hung transport would. */
  stall(): void {
    this.#stalled = true;
  }

  async send(bytes: Uint8Array): Promise<void> {
    if (this.#stalled) {
      await new Promise<void>(() => {
        // Never resolves: this lane is wedged for the life of the test.
      });
    }
    // Copied: the caller owns the buffer and stamps it in place.
    this.sent.push(new Uint8Array(bytes));
  }
}

interface Rig {
  readonly egress: EgressPipeline;
  readonly senders: Map<string, RecordingSender>;
  readonly transmitKeys: TransmitKeyManager;
  readonly sink: InMemoryMetricsSink;
  submit(byte: number): Promise<void>;
  notConnectedFaults(): number;
  reasons(): string[];
}

async function makeRig(urls: readonly string[]): Promise<Rig> {
  const sink = new InMemoryMetricsSink();
  const metrics = new MediaMetrics({ clientVersion: 'v', orgId: 'o' }, sink);
  const kekSource = kekHolderWith(KEK, 0);
  const identity = await MeetingIdentity.create();
  const senders = new Map<string, RecordingSender>();
  for (const url of urls) senders.set(url, new RecordingSender());
  let notConnected = 0;

  // ONE manager, as production has: its identity IS the nonce-uniqueness
  // boundary (see `transmitKeys.ts::nextStreamSequence`).
  const transmitKeys = new TransmitKeyManager(SENDER_ID);
  const egress = new EgressPipeline({
    metrics,
    transmitKeys,
    kek: kekSource,
    identity,
    maxQueueFrames: DEFAULT_CLIENT_CONFIG.media.egress.maxQueueFrames,
    streamNumber: 1,
    streamId: 0,
    senderFor: (url) => senders.get(url),
    onTargetNotConnected: () => {
      notConnected += 1;
    },
  });

  return {
    egress,
    senders,
    transmitKeys,
    sink,
    submit: (byte) => egress.submit({ data: Uint8Array.of(byte, byte, byte), timestampUs: 0 }),
    notConnectedFaults: () => notConnected,
    reasons: () =>
      sink
        .getRecordedMetrics()
        .filter((m) => m.name === 'dt_client_media_send_dropped_total')
        .map((m) => String(m.labels.reason)),
  };
}

/** The hop-sequence field: 4 bytes big-endian, after the 2-byte stream id. */
function hopSequenceOf(frame: Uint8Array): number {
  return decodeFrame(frame).hopSequence;
}

/** The packed key id AS IT APPEARS ON THE WIRE, hex-encoded for comparison. */
function keyIdOf(frame: Uint8Array): string {
  return Buffer.from(parseSframe(decodeFrame(frame).payload).keyId).toString('hex');
}

describe('multi-target send', () => {
  it('delivers EVERY frame to EVERY target', async () => {
    const rig = await makeRig([FIRST, SECOND]);
    rig.egress.setTargets([FIRST, SECOND]);

    await rig.submit(1);
    await rig.submit(2);
    await settle();

    expect(rig.senders.get(FIRST)!.sent).toHaveLength(2);
    expect(rig.senders.get(SECOND)!.sent).toHaveLength(2);
    // Datagrams, not captured frames: two frames to two handlers is four sends.
    expect(rig.egress.framesSent).toBe(4);
  });

  it('seals ONCE: the bytes on each target differ only in the hop sequence', async () => {
    // S14, pinned directly. The publisher region, the SFrame payload and the
    // signature are all identical across targets, because they were computed once
    // above the fan-out. Only the relay region's hop sequence — unauthenticated,
    // per (connection, stream) — is stamped per lane.
    const rig = await makeRig([FIRST, SECOND]);
    rig.egress.setTargets([FIRST, SECOND]);

    await rig.submit(7);
    await settle();

    const a = rig.senders.get(FIRST)!.sent[0]!;
    const b = rig.senders.get(SECOND)!.sent[0]!;
    expect(a).toHaveLength(b.length);

    const differing: number[] = [];
    for (let i = 0; i < a.length; i += 1) {
      if (a[i] !== b[i]) differing.push(i);
    }
    // The first frame on each lane is hop 0, so even that field matches here —
    // which is the strongest form of the claim: byte-for-byte identical.
    expect(differing).toEqual([]);
    expect(keyIdOf(a)).toEqual(keyIdOf(b));
  });

  it('seals ONCE even when the lanes are at DIFFERENT hop numbers', async () => {
    // The test above compares two frames that are both at hop 0, so it cannot
    // tell "identical because sealed once" from "identical because the only
    // per-lane field happened to match". This one forces the hop numbers apart
    // and then requires every OTHER byte to be equal — which is the actual claim:
    // the per-lane mutation is the hop sequence and nothing else.
    const rig = await makeRig([FIRST, SECOND]);

    // FIRST alone carries two frames, so its counter reaches 2 while SECOND's is
    // still at 0. Then both carry the SAME captured frame.
    rig.egress.setTargets([FIRST]);
    await rig.submit(1);
    await rig.submit(2);
    await settle();
    rig.egress.setTargets([FIRST, SECOND]);
    await rig.submit(3);
    await settle();

    const a = rig.senders.get(FIRST)!.sent[2]!;
    const b = rig.senders.get(SECOND)!.sent[0]!;
    expect(hopSequenceOf(a)).toBe(2);
    expect(hopSequenceOf(b)).toBe(0);

    const differing: number[] = [];
    for (let i = 0; i < a.length; i += 1) {
      if (a[i] !== b[i]) differing.push(i);
    }
    // POSITIVE CONTROL: the comparison can see a difference at all. Without this,
    // an all-equal result would be indistinguishable from a comparison that never
    // ran over anything.
    expect(differing.length).toBeGreaterThan(0);
    // And every differing byte lies inside ONE 4-byte window — the hop-sequence
    // field. Asserted as a contiguous window rather than against a hard-coded
    // offset, so the frame layout can move without this test lying.
    const span = differing[differing.length - 1]! - differing[0]! + 1;
    expect(span).toBeLessThanOrEqual(4);
    // The publisher region, payload and signature are untouched by the fan-out.
    expect(keyIdOf(a)).toEqual(keyIdOf(b));
    expect(Buffer.from(decodeFrame(a).payload).toString('hex')).toEqual(
      Buffer.from(decodeFrame(b).payload).toString('hex'),
    );
  });

  it('consumes exactly ONE stream sequence per frame regardless of target count', async () => {
    // A second sequence allocation per frame is the mild form of the forbidden
    // "one pipeline per target" refactor; a second TransmitKeyManager is the
    // catastrophic form (ADR-0036 §2: a nonce repeat under one key leaks the
    // authentication subkey and permits forgery). Both show up here.
    const three = await makeRig([FIRST, SECOND, THIRD]);
    three.egress.setTargets([FIRST, SECOND, THIRD]);
    await three.submit(1);
    await three.submit(2);
    await three.submit(3);
    await settle();
    // Sequences 0,1,2 consumed — so the next is 3, whatever the target count.
    expect(three.transmitKeys.nextStreamSequence(1)).toBe(3);

    const one = await makeRig([FIRST]);
    one.egress.setTargets([FIRST]);
    await one.submit(1);
    await one.submit(2);
    await one.submit(3);
    await settle();
    expect(one.transmitKeys.nextStreamSequence(1)).toBe(3);
  });

  it('never emits one (keyId, streamSequence) pair for two different payloads', async () => {
    // THE NONCE-REUSE TRIPWIRE. Stated in its own terms rather than as a
    // consequence of byte equality, so relaxing that assertion cannot silently
    // take this property with it. A duplicate pair here means two plaintexts were
    // sealed under one (key, nonce).
    const rig = await makeRig([FIRST, SECOND]);
    rig.egress.setTargets([FIRST, SECOND]);
    for (let byte = 1; byte <= 5; byte += 1) await rig.submit(byte);
    await settle();

    const perPayload = new Map<string, Set<string>>();
    for (const sender of rig.senders.values()) {
      for (const frame of sender.sent) {
        const decoded = decodeFrame(frame);
        const pair = `${keyIdOf(frame)}:${decoded.streamSequence}`;
        const payload = Buffer.from(decoded.payload).toString('hex');
        const seen = perPayload.get(pair) ?? new Set<string>();
        seen.add(payload);
        perPayload.set(pair, seen);
      }
    }
    for (const [pair, payloads] of perPayload) {
      expect(
        payloads.size,
        `NONCE REUSE: (keyId, streamSequence) ${pair} sealed ${payloads.size} different ` +
          `payloads. Under AES-GCM a repeated (key, nonce) leaks the authentication subkey ` +
          `and permits forgery (ADR-0036 §2) — this is not a counting bug.`,
      ).toBe(1);
    }
    // Positive control: the pairs were actually collected.
    expect(perPayload.size).toBe(5);
  });

  it('gives each target its OWN hop sequence, counting only what that lane sent', async () => {
    // `hop_sequence` is per (connection, media stream) in the frame format. A
    // shared counter would make each handler see every other number missing and
    // read ~50% uplink loss.
    const rig = await makeRig([FIRST, SECOND]);
    rig.egress.setTargets([FIRST, SECOND]);
    for (let byte = 1; byte <= 3; byte += 1) await rig.submit(byte);
    await settle();

    expect(rig.senders.get(FIRST)!.sent.map(hopSequenceOf)).toEqual([0, 1, 2]);
    expect(rig.senders.get(SECOND)!.sent.map(hopSequenceOf)).toEqual([0, 1, 2]);
  });

  it('keeps delivering to the healthy target when another lane is wedged', async () => {
    // Partial send: "some peers hear me, some do not" must not become "nobody
    // hears me". Lanes drain independently, so a hung transport cannot block one
    // that is fine.
    const rig = await makeRig([FIRST, SECOND]);
    rig.egress.setTargets([FIRST, SECOND]);
    rig.senders.get(SECOND)!.stall();

    for (let byte = 1; byte <= 4; byte += 1) await rig.submit(byte);
    await settle();

    expect(rig.senders.get(FIRST)!.sent.map(hopSequenceOf)).toEqual([0, 1, 2, 3]);
    // The wedged lane accepted the first datagram and never returned from it.
    expect(rig.senders.get(SECOND)!.sent).toHaveLength(0);
  });

  it('counts connection_closed on the closed lane only, and keeps the other sending', async () => {
    const rig = await makeRig([FIRST, SECOND]);
    rig.egress.setTargets([FIRST, SECOND]);
    rig.senders.get(SECOND)!.isOpen = false;

    await rig.submit(1);
    await settle();

    expect(rig.senders.get(FIRST)!.sent).toHaveLength(1);
    expect(rig.reasons()).toEqual(['connection_closed']);
  });

  it('retains a lane hop counter across a target leaving and returning', async () => {
    // The transport did not go anywhere, and `hop_sequence` is per CONNECTION. A
    // restart from 0 on the same transport reads as a publisher restart to
    // anything tracking that hop.
    const rig = await makeRig([FIRST, SECOND]);
    rig.egress.setTargets([FIRST, SECOND]);
    await rig.submit(1);
    await settle();

    // SECOND's edge moves away, then comes back.
    rig.egress.setTargets([FIRST]);
    await rig.submit(2);
    await settle();
    rig.egress.setTargets([FIRST, SECOND]);
    await rig.submit(3);
    await settle();

    expect(rig.senders.get(FIRST)!.sent.map(hopSequenceOf)).toEqual([0, 1, 2]);
    // Not [0, 0]: the second lane resumed on its retained counter.
    expect(rig.senders.get(SECOND)!.sent.map(hopSequenceOf)).toEqual([0, 1]);
  });

  it('builds NOTHING when the target set is empty (§5 send nothing)', async () => {
    const rig = await makeRig([FIRST]);
    rig.egress.setTargets([FIRST]);
    await rig.submit(1);
    await settle();
    expect(rig.egress.framesSent).toBe(1);

    rig.egress.setTargets([]);
    await rig.submit(2);
    await rig.submit(3);
    await settle();

    // No send, and no drop counted either: nothing was produced, so "count the
    // frames we did not send" would be inventing a failure.
    expect(rig.egress.framesSent).toBe(1);
    expect(rig.reasons()).toEqual([]);
    // And no sequence was consumed for the frames that were never built.
    expect(rig.transmitKeys.nextStreamSequence(1)).toBe(1);
  });

  it('counts not_connected per frame and faults once for a target with no transport', async () => {
    // The lane is created because MC directed it, but `senderFor` resolves
    // nothing. Under §9 that is a server-side condition, and it must be loud
    // rather than queued-and-aged as `egress_queue_overflow`.
    const rig = await makeRig([FIRST]);
    rig.egress.setTargets([FIRST, SECOND]);

    await rig.submit(1);
    await rig.submit(2);
    await settle();

    expect(rig.senders.get(FIRST)!.sent).toHaveLength(2);
    expect(rig.reasons()).toEqual(['not_connected', 'not_connected']);
    // The fault callback is invoked per frame; the LIFECYCLE layer dedupes it by
    // (stage, message), which is asserted in the AudioPipeline suite.
    expect(rig.notConnectedFaults()).toBe(2);
  });

  it('does not depend on target ORDER', async () => {
    // Nothing may assume a particular or sorted-first handler — not the rule, not
    // a test, not the client. The same two targets in the opposite order must
    // produce the same delivery.
    const forward = await makeRig([FIRST, SECOND]);
    forward.egress.setTargets([FIRST, SECOND]);
    await forward.submit(9);
    await settle();

    const reverse = await makeRig([FIRST, SECOND]);
    reverse.egress.setTargets([SECOND, FIRST]);
    await reverse.submit(9);
    await settle();

    for (const url of [FIRST, SECOND]) {
      expect(forward.senders.get(url)!.sent).toHaveLength(1);
      expect(reverse.senders.get(url)!.sent).toHaveLength(1);
      expect(hopSequenceOf(reverse.senders.get(url)!.sent[0]!)).toBe(0);
    }
  });
});
