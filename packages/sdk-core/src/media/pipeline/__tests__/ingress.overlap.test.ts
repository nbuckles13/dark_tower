// File: packages/sdk-core/src/media/pipeline/__tests__/ingress.overlap.test.ts
//
// story 4 C2 / R-16: a frame arriving via TWO handlers during a make-before-
// break overlap is decoded and played ONCE. The duplicate is still DROPPED —
// counted `duplicate_cross_transport`, never `replay_detected` — and the slot's
// continuity evidence fires once, for the first copy. Real crypto
// (`frameFixtures.ts`); one shared ingress with its ONE replay window, two
// transports (refs) each with their own hop monitor, exactly as production.

import { beforeAll, describe, expect, it } from 'vitest';
import { InMemoryMetricsSink } from '@darktower/test-utils';

import { ReplayWindow, TransmitKeyCache } from '../../frame/receivePath.js';
import { AES_256_KEY_BYTES } from '../../frame/sframe.js';
import { gateFor, kekHolderWith } from '../../__tests__/helpers.js';
import { MediaKindMetrics } from '../../setup/mediaMetrics.js';
import { MediaKind } from '../../../proto/dark_tower/signaling/v1/signaling_pb.js';
import { FirstMediaObserver } from '../../setup/measurement.js';
import { RosterIdentityKeys } from '../../setup/rosterKeys.js';
import { HopSequenceMonitor } from '../hopSequenceMonitor.js';
import { IngressPipeline } from '../ingress.js';
import { TransportRecency } from '../../lifecycle/transportRecency.js';
import { handlerRef } from '../../handlerSet.js';
import { DEFAULT_CLIENT_CONFIG } from '../../../config/clientConfig.js';
import { buildTestFrame, makeIdentity, type TestIdentity } from './frameFixtures.js';

const KEK = new Uint8Array(AES_256_KEY_BYTES).fill(0x11);
/** The membership bound production uses (`media.handlers.reconnectAbandonMs`). */
const WINDOW_MS = DEFAULT_CLIENT_CONFIG.media.handlers.reconnectAbandonMs;
const SLOT = 0;
const SENDER = 258;
const OLD = handlerRef(1);
const NEW = handlerRef(2);

let alice: TestIdentity;
beforeAll(async () => {
  alice = await makeIdentity(0x61);
});

async function rig() {
  const sink = new InMemoryMetricsSink();
  const metrics = new MediaKindMetrics({ clientVersion: 'v', orgId: 'o' }, MediaKind.AUDIO, sink);
  const roster = new RosterIdentityKeys(8);
  await roster.upsert({ senderId: SENDER, identityPublicKey: alice.publicKey });
  const gate = gateFor(SENDER);
  let now = 0;
  const recency = new TransportRecency(WINDOW_MS, () => now);
  const presented: [number, number, number][] = [];
  const ingress = new IngressPipeline({
    metrics,
    roster,
    lanes: gate,
    keys: kekHolderWith(KEK, 0),
    cache: new TransmitKeyCache(8),
    replay: new ReplayWindow(8, 64),
    firstMedia: new FirstMediaObserver(metrics, () => 0),
    declaredSlotIds: [SLOT],
    arrivalFor: (ref) => recency.arrival(ref),
    onPresented: (slotId, ref, senderId) => presented.push([slotId, ref, senderId]),
  });
  const monitors = new Map([
    [OLD, new HopSequenceMonitor([SLOT])],
    [NEW, new HopSequenceMonitor([SLOT])],
  ]);
  const via = (bytes: Uint8Array, ref: typeof OLD) =>
    ingress.accept(bytes, monitors.get(ref)!, ref);
  const drops = () =>
    sink
      .getRecordedMetrics()
      .filter((m) => m.name === 'dt_client_media_frames_dropped_total')
      .map((m) => String(m.labels.reason));
  return {
    via,
    gate,
    presented,
    drops,
    recency,
    advance: (ms: number) => {
      now += ms;
    },
  };
}

const frame = (streamSequence: number) =>
  buildTestFrame({
    keyIdSenderId: SENDER,
    kek: KEK,
    signer: alice,
    plaintext: new Uint8Array([7, streamSequence]),
    streamSequence,
    streamId: SLOT,
  });

describe('make-before-break overlap: one frame, two handlers', () => {
  it('is decoded ONCE and presented ONCE; the second copy is dropped as duplicate_cross_transport', async () => {
    const r = await rig();
    const bytes = await frame(5);
    await r.via(bytes, OLD);
    await r.via(bytes, NEW);
    expect(r.gate.decodedBy.get(SENDER)).toHaveLength(1);
    expect(r.presented).toEqual([[SLOT, OLD, SENDER]]);
    expect(r.drops()).toEqual(['duplicate_cross_transport']);
    // Positive arm: the NEXT frame via the new handler is decoded and presented.
    await r.via(await frame(6), NEW);
    expect(r.gate.decodedBy.get(SENDER)).toHaveLength(2);
    expect(r.presented.at(-1)).toEqual([SLOT, NEW, SENDER]);
  });

  it('a duplicate via the SAME handler is replay_detected (and still decoded once)', async () => {
    const r = await rig();
    const bytes = await frame(9);
    await r.via(bytes, OLD);
    await r.via(bytes, OLD);
    expect(r.gate.decodedBy.get(SENDER)).toHaveLength(1);
    expect(r.drops()).toEqual(['replay_detected']);
  });

  it('a duplicate whose first copy came via a handler removed LONG ago is replay_detected', async () => {
    const r = await rig();
    const bytes = await frame(11);
    await r.via(bytes, OLD);
    r.recency.forget(OLD);
    r.advance(WINDOW_MS + 1);
    // A LATER removal prunes OLD's timestamp: it must stay not-recent (fail closed).
    r.recency.arrival(handlerRef(3));
    r.recency.forget(handlerRef(3));
    await r.via(bytes, NEW);
    expect(r.drops()).toEqual(['replay_detected']);
    // ...while one removed JUST NOW still classifies as the overlap.
    const r2 = await rig();
    await r2.via(bytes, OLD);
    r2.recency.forget(OLD);
    await r2.via(bytes, NEW);
    expect(r2.drops()).toEqual(['duplicate_cross_transport']);
  });
});

describe('the membership bound edge (TransportRecency uses <=)', () => {
  it('removed EXACTLY the window ago is still the overlap; one ms later it is a replay', async () => {
    const at = await rig();
    const bytes = await frame(21);
    await at.via(bytes, OLD);
    at.recency.forget(OLD);
    at.advance(WINDOW_MS);
    await at.via(bytes, NEW);
    expect(at.drops()).toEqual(['duplicate_cross_transport']);
    const past = await rig();
    await past.via(bytes, OLD);
    past.recency.forget(OLD);
    past.advance(WINDOW_MS + 1);
    await past.via(bytes, NEW);
    expect(past.drops()).toEqual(['replay_detected']);
  });
});
