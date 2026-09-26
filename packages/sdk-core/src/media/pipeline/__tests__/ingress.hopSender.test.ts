// File: packages/sdk-core/src/media/pipeline/__tests__/ingress.hopSender.test.ts
//
// Ingress feeds the hop monitor the sender from the frame's OWN key id, so a
// slot refill (story 2 static fill) resets the slot's hop baseline instead of
// reading as a run of reorders. And a frame whose SFrame header cannot be read
// is STILL hop-observed, so it is counted once (as a reject) rather than twice
// (as a reject and as a relay gap).

import { beforeEach, describe, expect, it } from 'vitest';
import { InMemoryMetricsSink } from '@darktower/test-utils';
import { buildUnsignedFrame, finishFrame } from '../../frame/frameCodec.js';
import { ReplayWindow, TransmitKeyCache } from '../../frame/receivePath.js';
import { AES_256_KEY_BYTES } from '../../frame/sframe.js';
import { gateFor, kekHolderWith } from '../../__tests__/helpers.js';
import { MediaMetrics } from '../../setup/mediaMetrics.js';
import { FirstMediaObserver } from '../../setup/measurement.js';
import { RosterIdentityKeys } from '../../setup/rosterKeys.js';
import { HopSequenceMonitor } from '../hopSequenceMonitor.js';
import { IngressPipeline } from '../ingress.js';
import { buildTestFrame, makeIdentity, type TestIdentity } from './frameFixtures.js';

const KEK = new Uint8Array(AES_256_KEY_BYTES).fill(0x11);
const PLAINTEXT = new TextEncoder().encode('dark-tower refill audio');
const SLOT_ID = 0;
const SENDER_A = 258;
const SENDER_B = 4242;

const GAP = 'dt_client_media_downlink_gap_frames_total';
const REORDER = 'dt_client_media_downlink_reorder_total';
const ACCEPTED = 'dt_client_media_frames_accepted_total';

interface Harness {
  readonly roster: RosterIdentityKeys;
  readonly ingress: IngressPipeline;
  readonly hopMonitor: HopSequenceMonitor;
  count(name: string): number;
  dropReasons(): string[];
}

function makeHarness(): Harness {
  const sink = new InMemoryMetricsSink();
  const metrics = new MediaMetrics({ clientVersion: '0.0.0-test', orgId: 'demo' }, sink);
  const roster = new RosterIdentityKeys(8);
  const kekSource = kekHolderWith(KEK, 0);
  // ONE MONITOR PER TRANSPORT. These harnesses model a single transport, so one
  // monitor per harness — created here rather than shared at module scope,
  // which would carry hop high-water marks between tests.
  const hopMonitor = new HopSequenceMonitor([SLOT_ID]);
  const ingress = new IngressPipeline({
    metrics,
    roster,
    lanes: gateFor(SENDER_A, SENDER_B),
    keys: kekSource,
    cache: new TransmitKeyCache(8),
    replay: new ReplayWindow(8, 64),
    firstMedia: new FirstMediaObserver(metrics, () => 0),
  });
  return {
    roster,
    ingress,
    hopMonitor,
    count: (name) =>
      sink
        .getRecordedMetrics()
        .filter((m) => m.name === name)
        .reduce((sum, m) => sum + m.value, 0),
    dropReasons: () =>
      sink
        .getRecordedMetrics()
        .filter((m) => m.name === 'dt_client_media_frames_dropped_total')
        .map((m) => String(m.labels.reason)),
  };
}

/** A frame that DECODES (relay region readable) but whose SFrame header does not parse. */
function frameWithUnreadableKeyId(hopSequence: number): Uint8Array {
  const unsigned = buildUnsignedFrame({
    flags: { independentlyDecodable: true, discardable: false, keyBearing: false },
    streamSequence: 0,
    wrappedTransmitKey: null,
    extensions: [],
    streamId: SLOT_ID,
    hopSequence,
    // Shorter than the SFrame clear header (key id + tag): `parseSframe` rejects.
    payload: new Uint8Array(4),
  });
  return finishFrame(unsigned, new Uint8Array(64));
}

describe('ingress passes the key-id sender to the hop monitor', () => {
  let alice: TestIdentity;
  let bob: TestIdentity;

  beforeEach(async () => {
    alice = await makeIdentity(0x41);
    bob = await makeIdentity(0x42);
  });

  async function frameFrom(
    sender: number,
    signer: TestIdentity,
    hopSequence: number,
    streamSequence: number,
  ): Promise<Uint8Array> {
    return buildTestFrame({
      keyIdSenderId: sender,
      kek: KEK,
      signer,
      plaintext: PLAINTEXT,
      streamId: SLOT_ID,
      hopSequence,
      streamSequence,
    });
  }

  it('a slot refilled by a new sender whose counter starts at 0 counts no reorders and no gap', async () => {
    const h = makeHarness();
    await h.roster.upsert({ senderId: SENDER_A, identityPublicKey: alice.publicKey });
    await h.roster.upsert({ senderId: SENDER_B, identityPublicKey: bob.publicKey });

    // A's mark (21) is deliberately INSIDE the restart bound (64): the backward
    // jump to 0 alone reads as a REORDER, so only the key-id sender reaching the
    // monitor can explain zero reorders. From a mark of ~5000 the restart rule
    // would hide an ingress that passed no sender at all.
    await h.ingress.accept(await frameFrom(SENDER_A, alice, 20, 0), h.hopMonitor);
    await h.ingress.accept(await frameFrom(SENDER_A, alice, 21, 1), h.hopMonitor);
    // MC refilled the slot with B: same slot id, B's forwarder counts from 0.
    for (let hop = 0; hop < 3; hop += 1) {
      await h.ingress.accept(await frameFrom(SENDER_B, bob, hop, hop), h.hopMonitor);
    }

    // All five frames are genuine and accepted; the refill is not a loss event.
    expect(h.count(ACCEPTED)).toBe(5);
    expect(h.dropReasons()).toEqual([]);
    expect(h.count(REORDER)).toBe(0);
    expect(h.count(GAP)).toBe(0);
  });

  it('a refill by a sender whose counter is AHEAD of the old mark counts no gap', async () => {
    // MH's counter is per (publisher, egress stream): a sender that fed this
    // stream before resumes from where it stopped. Without the key-id sender
    // reaching the monitor, 100 -> 8000 would count 7899 missing frames.
    const h = makeHarness();
    await h.roster.upsert({ senderId: SENDER_A, identityPublicKey: alice.publicKey });
    await h.roster.upsert({ senderId: SENDER_B, identityPublicKey: bob.publicKey });

    await h.ingress.accept(await frameFrom(SENDER_A, alice, 100, 0), h.hopMonitor);
    await h.ingress.accept(await frameFrom(SENDER_B, bob, 8000, 0), h.hopMonitor);
    await h.ingress.accept(await frameFrom(SENDER_B, bob, 8001, 1), h.hopMonitor);

    expect(h.count(ACCEPTED)).toBe(3);
    expect(h.count(GAP)).toBe(0);
    expect(h.count(REORDER)).toBe(0);
  });

  it('an unreadable SFrame header is still hop-observed: rejected once, never also a gap', async () => {
    const h = makeHarness();
    await h.roster.upsert({ senderId: SENDER_A, identityPublicKey: alice.publicKey });

    await h.ingress.accept(await frameFrom(SENDER_A, alice, 1, 0), h.hopMonitor);
    await h.ingress.accept(frameWithUnreadableKeyId(2), h.hopMonitor);
    await h.ingress.accept(await frameFrom(SENDER_A, alice, 3, 1), h.hopMonitor);

    // Rejected with its own token, verbatim. This also proves the frame got
    // past `decodeFrame` to the key-id read, so the hop step was reachable.
    expect(h.dropReasons()).toEqual(['truncated']);
    expect(h.count(ACCEPTED)).toBe(2);
    // Had hop 2 not been observed, hop 3 would open a 1-frame gap.
    expect(h.count(GAP)).toBe(0);
    expect(h.count(REORDER)).toBe(0);
  });

  it('positive control: a genuinely missing hop number IS counted as a gap in this rig', async () => {
    // Without this, the two zero-gap assertions above would pass on a rig
    // whose gap counter never fires.
    const h = makeHarness();
    await h.roster.upsert({ senderId: SENDER_A, identityPublicKey: alice.publicKey });

    await h.ingress.accept(await frameFrom(SENDER_A, alice, 1, 0), h.hopMonitor);
    await h.ingress.accept(await frameFrom(SENDER_A, alice, 3, 1), h.hopMonitor);

    expect(h.count(ACCEPTED)).toBe(2);
    expect(h.count(GAP)).toBe(1);
  });
});
