// File: packages/sdk-core/src/media/pipeline/__tests__/ingress.verification.test.ts
//
// Story 2 R-30: the ingress feeds the injected receive-verification recorder at
// three points — layer 1 (key id parsed, pre-verify), layer 2 (verified AND
// decrypted), and every counted drop — keyed by (OBSERVED slot, OBSERVED
// sender). Real crypto, as in `ingress.attribution.test.ts`: a stubbed verifier
// would make the layer-2 assertion evidence of nothing.

import { beforeEach, describe, expect, it } from 'vitest';
import { InMemoryMetricsSink } from '@darktower/test-utils';

import { ReplayWindow, TransmitKeyCache } from '../../frame/receivePath.js';
import { AES_256_KEY_BYTES } from '../../frame/sframe.js';
import { gateFor, kekHolderWith } from '../../__tests__/helpers.js';
import { MediaMetrics } from '../../setup/mediaMetrics.js';
import { FirstMediaObserver } from '../../setup/measurement.js';
import { RosterIdentityKeys } from '../../setup/rosterKeys.js';
import { HopSequenceMonitor } from '../hopSequenceMonitor.js';
import { IngressPipeline } from '../ingress.js';
import { BoundedReceiveVerificationRecorder } from '../receiveVerification.js';
import { buildTestFrame, makeIdentity, type TestIdentity } from './frameFixtures.js';

const KEK = new Uint8Array(AES_256_KEY_BYTES).fill(0x11);
const PLAINTEXT = new TextEncoder().encode('tone');
const SENDER_A = 258;
const SENDER_B = 4242;
const UNROSTERED = 777;

function harness(withRecorder: boolean, assigned: number[] = [SENDER_A]) {
  const sink = new InMemoryMetricsSink();
  const metrics = new MediaMetrics({ clientVersion: 'v', orgId: 'o' }, sink);
  const roster = new RosterIdentityKeys(8);
  const recorder = withRecorder ? new BoundedReceiveVerificationRecorder() : undefined;
  const ingress = new IngressPipeline({
    metrics,
    roster,
    // By default only A is assigned: B is rostered but NOT assigned
    // (sender_not_assigned).
    lanes: gateFor(...assigned),
    keys: kekHolderWith(KEK, 0),
    cache: new TransmitKeyCache(8),
    replay: new ReplayWindow(8, 64),
    firstMedia: new FirstMediaObserver(metrics, () => 0),
    ...(recorder ? { verification: recorder } : {}),
  });
  // Slots 0 and 1 declared; stream 9 is not.
  const hop = new HopSequenceMonitor([0, 1]);
  return { ingress, roster, recorder, hop, sink };
}

describe('ingress → receive-verification recorder (R-30)', () => {
  let alice: TestIdentity;
  let bob: TestIdentity;
  let mallory: TestIdentity;

  beforeEach(async () => {
    alice = await makeIdentity(0x41);
    bob = await makeIdentity(0x42);
    mallory = await makeIdentity(0x43);
  });

  it('counts layer 1 and layer 2 under the OBSERVED slot and sender', async () => {
    const h = harness(true);
    await h.roster.upsert({ senderId: SENDER_A, identityPublicKey: alice.publicKey });
    for (const hopSequence of [1, 2]) {
      await h.ingress.accept(
        await buildTestFrame({
          keyIdSenderId: SENDER_A,
          kek: KEK,
          signer: alice,
          plaintext: PLAINTEXT,
          streamId: 1,
          hopSequence,
          streamSequence: hopSequence,
        }),
        h.hop,
      );
    }
    expect(h.recorder!.snapshot()).toEqual({
      layers: [{ slot: 1, senderId: SENDER_A, keyed: 2, verified: 2 }],
      drops: [],
      overflow: 0,
    });
  });

  it('MISROUTING surfaces under the wrong key: a frame on slot 0 naming B is counted as (0, B)', async () => {
    const h = harness(true);
    await h.roster.upsert({ senderId: SENDER_B, identityPublicKey: bob.publicKey });
    await h.ingress.accept(
      await buildTestFrame({
        keyIdSenderId: SENDER_B,
        kek: KEK,
        signer: bob,
        plaintext: PLAINTEXT,
        streamId: 0,
      }),
      h.hop,
    );
    const snap = h.recorder!.snapshot();
    // Layer 1 saw it; layer 2 did not (B is not assigned); the drop carries both.
    expect(snap.layers).toEqual([{ slot: 0, senderId: SENDER_B, keyed: 1, verified: 0 }]);
    expect(snap.drops).toEqual([
      { slot: 0, senderId: SENDER_B, reason: 'sender_not_assigned', count: 1 },
    ]);
  });

  it('a forged key id fails layer 2: keyed under the claimed sender, dropped, never verified', async () => {
    const h = harness(true);
    await h.roster.upsert({ senderId: SENDER_A, identityPublicKey: alice.publicKey });
    // Mallory signs a frame whose key id claims A.
    await h.ingress.accept(
      await buildTestFrame({
        keyIdSenderId: SENDER_A,
        kek: KEK,
        signer: mallory,
        plaintext: PLAINTEXT,
        streamId: 0,
      }),
      h.hop,
    );
    const snap = h.recorder!.snapshot();
    expect(snap.layers).toEqual([{ slot: 0, senderId: SENDER_A, keyed: 1, verified: 0 }]);
    // Pinned to the SIGNATURE token: a drop for any other reason (a KEK
    // mismatch, say) would not be evidence of layer 2 doing its job.
    expect(snap.drops).toEqual([
      { slot: 0, senderId: SENDER_A, reason: 'signature_invalid', count: 1 },
    ]);
  });

  it('MISROUTING that passes every gate stays visible: B on slot 0 is ACCEPTED yet counted as (0, B)', async () => {
    // A on slot 0, B on slot 1 — both assigned, so the per-sender lane gate
    // admits B's frame whatever slot it arrives on. Only the observed-slot key
    // shows that MH delivered B on A's slot.
    const h = harness(true, [SENDER_A, SENDER_B]);
    await h.roster.upsert({ senderId: SENDER_A, identityPublicKey: alice.publicKey });
    await h.roster.upsert({ senderId: SENDER_B, identityPublicKey: bob.publicKey });
    await h.ingress.accept(
      await buildTestFrame({
        keyIdSenderId: SENDER_B,
        kek: KEK,
        signer: bob,
        plaintext: PLAINTEXT,
        streamId: 0,
      }),
      h.hop,
    );
    expect(h.ingress.framesAccepted).toBe(1);
    expect(h.recorder!.snapshot()).toEqual({
      layers: [{ slot: 0, senderId: SENDER_B, keyed: 1, verified: 1 }],
      drops: [],
      overflow: 0,
    });
  });

  it('no_roster_entry and an undeclared stream land in their own buckets', async () => {
    const h = harness(true);
    await h.ingress.accept(
      await buildTestFrame({
        keyIdSenderId: UNROSTERED,
        kek: KEK,
        signer: mallory,
        plaintext: PLAINTEXT,
        streamId: 9,
      }),
      h.hop,
    );
    expect(h.recorder!.snapshot().drops).toEqual([
      { slot: 'undeclared', senderId: UNROSTERED, reason: 'no_roster_entry', count: 1 },
    ]);
  });

  it('a datagram that does not parse is an unparsed, UNATTRIBUTED drop', async () => {
    const h = harness(true);
    await h.ingress.accept(new Uint8Array([0xff, 0x00, 0x01]), h.hop);
    const snap = h.recorder!.snapshot();
    expect(snap.layers).toEqual([]);
    expect(snap.drops).toHaveLength(1);
    expect(snap.drops[0]).toMatchObject({ slot: 'unparsed', senderId: null, count: 1 });
  });

  it('OFF PATH: with no recorder injected (the production default) frames flow unchanged', async () => {
    const h = harness(false);
    expect(h.recorder).toBeUndefined();
    await h.roster.upsert({ senderId: SENDER_A, identityPublicKey: alice.publicKey });
    await h.ingress.accept(
      await buildTestFrame({
        keyIdSenderId: SENDER_A,
        kek: KEK,
        signer: alice,
        plaintext: PLAINTEXT,
        streamId: 0,
      }),
      h.hop,
    );
    expect(h.ingress.framesAccepted).toBe(1);
    expect(h.ingress.framesDropped).toBe(0);
  });
});
