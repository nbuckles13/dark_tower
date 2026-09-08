// File: packages/sdk-core/src/media/pipeline/__tests__/ingress.attribution.test.ts
//
// ATTRIBUTION, FAIL-CLOSED ROSTER HANDLING, REASON INDIVIDUALITY, AND THE
// RECEIVE-PATH ACCOUNTING IDENTITY.
//
// ---------------------------------------------------------------------------
// REAL CRYPTO, AND WHAT THAT BUYS
// ---------------------------------------------------------------------------
//
// Every frame here is built by `frameFixtures.ts` with the production Ed25519
// signer, KEK wrap, RFC 9605 key schedule and SFrame seal, and is verified and
// opened by the production receive path. No verifier is stubbed. That matters
// because the loopback this story ships is SELF-CONSISTENT under a swapped
// associated data, an inverted nonce, a truncated key id, or a signature over
// the wrong range — so a faked verifier would make these assertions evidence of
// nothing. Crypto CORRECTNESS is proven elsewhere, by the externally anchored
// vectors; what is proven here is that the pipeline routes each outcome to the
// right place.

import { beforeEach, describe, expect, it } from 'vitest';
import { InMemoryMetricsSink } from '@darktower/test-utils';

import { ReplayWindow, TransmitKeyCache } from '../../frame/receivePath.js';
import { AES_256_KEY_BYTES } from '../../frame/sframe.js';
import { JoinResponseKekSource } from '../../setup/kekSource.js';
import { MediaMetrics } from '../../setup/mediaMetrics.js';
import { FirstMediaObserver } from '../../setup/measurement.js';
import { RosterIdentityKeys } from '../../setup/rosterKeys.js';
import { HopSequenceMonitor } from '../hopSequenceMonitor.js';
import { IngressPipeline, type AcceptedFrame } from '../ingress.js';
import { buildTestFrame, makeIdentity, type TestIdentity } from './frameFixtures.js';

const KEK = new Uint8Array(AES_256_KEY_BYTES).fill(0x11);
const PLAINTEXT = new TextEncoder().encode('dark-tower loopback audio');
const SLOT_ID = 0;

/** Participant A: the sender the key id names in every frame below. */
const SENDER_A = 258;
/** Participant B: the sender the slot ASSIGNMENT names. Never the attribution. */
const SENDER_B = 4242;

interface Harness {
  readonly sink: InMemoryMetricsSink;
  readonly metrics: MediaMetrics;
  readonly roster: RosterIdentityKeys;
  readonly ingress: IngressPipeline;
  readonly accepted: AcceptedFrame[];
  /** Every `reason` value the drop counter recorded, in order. */
  dropReasons(): string[];
  count(name: string): number;
}

function makeHarness(): Harness {
  const sink = new InMemoryMetricsSink();
  const metrics = new MediaMetrics({ clientVersion: '0.0.0-test', orgId: 'demo' }, sink);
  const roster = new RosterIdentityKeys(8);
  const kekSource = new JoinResponseKekSource();
  kekSource.set(KEK, 0);
  const accepted: AcceptedFrame[] = [];
  const ingress = new IngressPipeline({
    metrics,
    roster,
    keys: kekSource,
    cache: new TransmitKeyCache(8),
    replay: new ReplayWindow(8, 64),
    hopMonitor: new HopSequenceMonitor([SLOT_ID]),
    firstMedia: new FirstMediaObserver(metrics, () => 0),
    onAccepted: (frame) => accepted.push(frame),
  });
  return {
    sink,
    metrics,
    roster,
    ingress,
    accepted,
    dropReasons: () =>
      sink
        .getRecordedMetrics()
        .filter((m) => m.name === 'dt_client_media_frames_dropped_total')
        .map((m) => String(m.labels.reason)),
    count: (name) =>
      sink
        .getRecordedMetrics()
        .filter((m) => m.name === name)
        .reduce((sum, m) => sum + m.value, 0),
  };
}

describe('ingress attribution comes from the key id, never from the slot assignment', () => {
  let alice: TestIdentity;
  let bob: TestIdentity;

  beforeEach(async () => {
    alice = await makeIdentity(0x41);
    bob = await makeIdentity(0x42);
  });

  it('attributes a frame to the sender its OWN key id names, not to the slot mapping', async () => {
    const h = makeHarness();
    // The roster holds BOTH participants, so the wrong lookup would succeed —
    // which is what makes this a real test rather than one that passes because
    // only one key was available.
    await h.roster.upsert({ senderId: SENDER_A, identityPublicKey: alice.publicKey });
    await h.roster.upsert({ senderId: SENDER_B, identityPublicKey: bob.publicKey });

    // The slot's assignment names participant B. The pipeline is never told —
    // deliberately: `IngressPipeline` has no parameter that could carry it, so
    // "attribute by assignment" is not expressible, not merely not done.
    const frame = await buildTestFrame({
      keyIdSenderId: SENDER_A,
      kek: KEK,
      signer: alice,
      plaintext: PLAINTEXT,
      streamId: SLOT_ID,
    });
    await h.ingress.accept(frame);

    expect(h.accepted).toHaveLength(1);
    expect(h.accepted[0]?.senderId).toBe(SENDER_A);
    expect(h.accepted[0]?.senderId).not.toBe(SENDER_B);
    expect(new TextDecoder().decode(h.accepted[0]?.plaintext)).toBe(
      new TextDecoder().decode(PLAINTEXT),
    );
    expect(h.dropReasons()).toEqual([]);
  });

  it('rejects at VERIFY a frame whose key id names A but which B signed', async () => {
    const h = makeHarness();
    await h.roster.upsert({ senderId: SENDER_A, identityPublicKey: alice.publicKey });
    await h.roster.upsert({ senderId: SENDER_B, identityPublicKey: bob.publicKey });

    // The insider-forgery shape: every member holds the meeting KEK, so every
    // member can unwrap every other member's transmit key and seal a frame under
    // another sender's key id. This frame WOULD decrypt if verification were
    // skipped — the payload is sealed under the key id's own schedule — which is
    // precisely what makes the signature layer load-bearing rather than
    // decorative.
    const forged = await buildTestFrame({
      keyIdSenderId: SENDER_A,
      kek: KEK,
      signer: bob,
      plaintext: PLAINTEXT,
      streamId: SLOT_ID,
    });
    await h.ingress.accept(forged);

    // Dropped at VERIFY, before any decrypt.
    expect(h.dropReasons()).toEqual(['signature_invalid']);
    // And never accepted, and never attributed to the mapping's participant.
    expect(h.accepted).toEqual([]);
    expect(h.count('dt_client_media_frames_accepted_total')).toBe(0);
  });
});

describe('the roster fails closed on an absent identity key', () => {
  it('DROPS AND COUNTS frames from a participant whose published key is EMPTY', async () => {
    // THE REQUIRED TEST. MC admits a joiner with an empty
    // `identity_public_key` and publishes an EMPTY key (length 0, never a
    // zero-filled placeholder). Rejecting empty at admission was considered and
    // reversed, and the protection moved here — nothing in MC will catch a
    // violation. The dangerous implementation is not "forgot to check"; it is
    // reading empty as "no key, so skip verification", which fails open and is
    // the natural reading. Present-key coverage does not discharge this.
    const h = makeHarness();
    const alice = await makeIdentity(0x41);
    await h.roster.upsert({ senderId: SENDER_A, identityPublicKey: new Uint8Array(0) });

    const frame = await buildTestFrame({
      keyIdSenderId: SENDER_A,
      kek: KEK,
      signer: alice,
      plaintext: PLAINTEXT,
      streamId: SLOT_ID,
    });
    await h.ingress.accept(frame);

    expect(h.dropReasons()).toEqual(['no_roster_entry']);
    expect(h.count('dt_client_media_frames_dropped_total')).toBe(1);
    // NEVER skipped and NEVER rendered.
    expect(h.accepted).toEqual([]);
    expect(h.count('dt_client_media_frames_accepted_total')).toBe(0);
  });

  it('drops a frame from a participant not on the roster at all', async () => {
    const h = makeHarness();
    const alice = await makeIdentity(0x41);
    const frame = await buildTestFrame({
      keyIdSenderId: SENDER_A,
      kek: KEK,
      signer: alice,
      plaintext: PLAINTEXT,
      streamId: SLOT_ID,
    });
    await h.ingress.accept(frame);
    expect(h.dropReasons()).toEqual(['no_roster_entry']);
  });
});

describe('the unauthenticated relay region cannot reach attribution or state', () => {
  it('attributes by key id and creates no hop state for an undeclared stream_id', async () => {
    const h = makeHarness();
    const alice = await makeIdentity(0x41);
    await h.roster.upsert({ senderId: SENDER_A, identityPublicKey: alice.publicKey });

    // A media handler writes `stream_id` freely and nobody signs it. An
    // unbounded 16-bit map key is 65 536 attacker-chosen state entries per
    // connection, created by the relay at datagram rate — so the bound is on
    // STATE CREATION, not on rendering.
    const frame = await buildTestFrame({
      keyIdSenderId: SENDER_A,
      kek: KEK,
      signer: alice,
      plaintext: PLAINTEXT,
      streamId: 0xbeef,
      hopSequence: 999_999,
    });
    await h.ingress.accept(frame);

    // The frame still verifies and still attributes to its OWN key id.
    expect(h.accepted[0]?.senderId).toBe(SENDER_A);
    // The anomaly is observable...
    expect(h.count('dt_client_media_undeclared_stream_id_total')).toBe(1);
    // ...and produced no gap or reorder signal, because no state was created.
    expect(h.count('dt_client_media_downlink_gap_frames_total')).toBe(0);
    expect(h.count('dt_client_media_downlink_reorder_total')).toBe(0);
  });
});

describe('receive-path accounting identity', () => {
  it('holds: received = accepted + sum(drops by reason)', async () => {
    const h = makeHarness();
    const alice = await makeIdentity(0x41);
    const bob = await makeIdentity(0x42);
    await h.roster.upsert({ senderId: SENDER_A, identityPublicKey: alice.publicKey });

    const good = await buildTestFrame({
      keyIdSenderId: SENDER_A,
      kek: KEK,
      signer: alice,
      plaintext: PLAINTEXT,
      streamId: SLOT_ID,
      streamSequence: 1,
    });
    const forged = await buildTestFrame({
      keyIdSenderId: SENDER_A,
      kek: KEK,
      signer: bob,
      plaintext: PLAINTEXT,
      streamId: SLOT_ID,
      streamSequence: 2,
    });
    const unknownSender = await buildTestFrame({
      keyIdSenderId: 999,
      kek: KEK,
      signer: alice,
      plaintext: PLAINTEXT,
      streamId: SLOT_ID,
      streamSequence: 3,
    });
    const garbage = Uint8Array.of(0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00);

    for (const frame of [good, forged, unknownSender, garbage]) {
      await h.ingress.accept(frame);
    }

    const received = h.count('dt_client_media_frames_received_total');
    const accepted = h.count('dt_client_media_frames_accepted_total');
    const dropped = h.count('dt_client_media_frames_dropped_total');
    expect(received).toBe(4);
    expect(accepted + dropped).toBe(received);
    // And the three drop reasons are individually visible, never collapsed.
    expect(new Set(h.dropReasons())).toEqual(
      new Set(['signature_invalid', 'no_roster_entry', 'unknown_version']),
    );
  });

  it('replays are rejected and counted, and the window advances only on verified frames', async () => {
    const h = makeHarness();
    const alice = await makeIdentity(0x41);
    await h.roster.upsert({ senderId: SENDER_A, identityPublicKey: alice.publicKey });
    const frame = await buildTestFrame({
      keyIdSenderId: SENDER_A,
      kek: KEK,
      signer: alice,
      plaintext: PLAINTEXT,
      streamId: SLOT_ID,
      streamSequence: 7,
    });
    await h.ingress.accept(frame);
    // Byte-identical replay: a valid signature and a valid authentication tag —
    // it WAS legitimately produced — so no cryptographic check can reject it.
    // Only the receiver's window can.
    await h.ingress.accept(frame);

    expect(h.dropReasons()).toEqual(['replay_detected']);
    expect(h.count('dt_client_media_frames_accepted_total')).toBe(1);
  });
});

describe('media metric labels', () => {
  it('carry client_version, org_id and key_custody — and never a meeting dimension', async () => {
    const h = makeHarness();
    const alice = await makeIdentity(0x41);
    await h.roster.upsert({ senderId: SENDER_A, identityPublicKey: alice.publicKey });
    await h.ingress.accept(
      await buildTestFrame({
        keyIdSenderId: SENDER_A,
        kek: KEK,
        signer: alice,
        plaintext: PLAINTEXT,
        streamId: SLOT_ID,
      }),
    );

    const media = h.sink.getRecordedMetrics().filter((m) => m.name.startsWith('dt_client_media_'));
    expect(media.length).toBeGreaterThan(0);
    for (const record of media) {
      const keys = new Set(Object.keys(record.labels));
      // The base set is always exactly these three; a metric may add its own
      // bounded discriminator (`reason`, `outcome`, `action`, `source`).
      expect(keys.has('client_version')).toBe(true);
      expect(keys.has('org_id')).toBe(true);
      expect(record.labels.key_custody).toBe('operator');
      // The negative half, and the one that matters: `meeting_id_hash` on a
      // media metric is a per-stream voice-activity trace in a two-person
      // meeting, and joined to the reject vocabulary it is an oracle over the
      // receiver's key state.
      expect(keys.has('meeting_id_hash')).toBe(false);
    }
  });
});

describe('non-dropping wrap outcomes are counted separately from drops', () => {
  it('plays a frame off the cache when the carried KEK generation is not held', async () => {
    // THE KEK-ROTATION-LAG SIGNAL. The sender re-wrapped under a generation
    // whose push has not landed yet. Dropping a PLAYABLE frame because a key we
    // do not need is unavailable would turn a benign race into an audio gap —
    // so the frame plays, and the condition is counted on its own counter rather
    // than folded into `absent`, whose semantics are "nothing to do".
    const h = makeHarness();
    const alice = await makeIdentity(0x41);
    await h.roster.upsert({ senderId: SENDER_A, identityPublicKey: alice.publicKey });

    // First frame: KEK generation 0, which the receiver holds. Caches the key.
    await h.ingress.accept(
      await buildTestFrame({
        keyIdSenderId: SENDER_A,
        kek: KEK,
        kekGeneration: 0,
        signer: alice,
        plaintext: PLAINTEXT,
        streamId: SLOT_ID,
        streamSequence: 1,
      }),
    );
    // Second frame: SAME key id and SAME transmit key, but the sender has
    // re-wrapped under a NEW meeting KEK and announces its generation. The
    // receiver does not hold that generation yet — the push has not landed —
    // and the wrap block therefore differs from the one it cached, so the
    // cached-wrap skip does not apply and the unwrap is actually attempted.
    await h.ingress.accept(
      await buildTestFrame({
        keyIdSenderId: SENDER_A,
        kek: new Uint8Array(AES_256_KEY_BYTES).fill(0x99),
        kekGeneration: 9,
        signer: alice,
        plaintext: PLAINTEXT,
        streamId: SLOT_ID,
        streamSequence: 2,
      }),
    );

    const outcomes = h.sink
      .getRecordedMetrics()
      .filter((m) => m.name === 'dt_client_media_key_wrap_outcomes_total')
      .map((m) => String(m.labels.outcome));
    expect(outcomes).toEqual(['kek_generation_not_held']);
    // ACCEPTED, not dropped — and the identity still holds.
    expect(h.count('dt_client_media_frames_accepted_total')).toBe(2);
    expect(h.count('dt_client_media_frames_dropped_total')).toBe(0);
    expect(h.count('dt_client_media_frames_received_total')).toBe(2);
  });

  it('drops with no_kek_for_generation when nothing usable is cached', async () => {
    // Same condition, opposite receiver state: no cached transmit key, so the
    // frame cannot be opened at all. The split is by RECEIVER STATE, which is
    // exactly why the two live on different counters.
    const h = makeHarness();
    const alice = await makeIdentity(0x41);
    await h.roster.upsert({ senderId: SENDER_A, identityPublicKey: alice.publicKey });
    await h.ingress.accept(
      await buildTestFrame({
        keyIdSenderId: SENDER_A,
        kek: new Uint8Array(AES_256_KEY_BYTES).fill(0x99),
        kekGeneration: 9,
        signer: alice,
        plaintext: PLAINTEXT,
        streamId: SLOT_ID,
      }),
    );
    expect(h.dropReasons()).toEqual(['no_kek_for_generation']);
    expect(h.count('dt_client_media_frames_accepted_total')).toBe(0);
  });

  it('drops no_transmit_key for a frame with neither key nor wrap', async () => {
    // A protocol violation, not a third key reason and not a decode reject:
    // under §4's cadence audio carries the key on EVERY frame, so the only paths
    // here are defects.
    const h = makeHarness();
    const alice = await makeIdentity(0x41);
    await h.roster.upsert({ senderId: SENDER_A, identityPublicKey: alice.publicKey });
    await h.ingress.accept(
      await buildTestFrame({
        keyIdSenderId: SENDER_A,
        kek: KEK,
        signer: alice,
        plaintext: PLAINTEXT,
        streamId: SLOT_ID,
        keyBearing: false,
      }),
    );
    expect(h.dropReasons()).toEqual(['no_transmit_key']);
  });
});
