// File: packages/sdk-core/src/media/pipeline/__tests__/videoIngress.test.ts
//
// The video stream ingress (story 3 C-B): slot binding from the first frame,
// the parse-boundary accounting identity under EVERY drop path, and the
// recovery-request mapping with fields from the FAILING frame's header. Real
// crypto throughout; only the VP8 codec is faked.

import { afterEach, beforeAll, beforeEach, describe, expect, it } from 'vitest';
import { FakeVideoDecoders, FakeVideoFrameRegistry } from '@darktower/test-utils';

import { DEFAULT_CLIENT_CONFIG } from '../../../config/clientConfig.js';
import { ReplayWindow, TransmitKeyCache } from '../../frame/receivePath.js';
import { ALL_REJECT_REASONS, type RejectReason } from '../../frame/rejectReason.js';
import { kekHolderWith } from '../../__tests__/helpers.js';
import { FirstMediaObserver } from '../../setup/measurement.js';
import { MEDIA_RECOVERY_SOURCES } from '../../setup/mediaMetrics.js';
import { RosterIdentityKeys } from '../../setup/rosterKeys.js';
import type { VideoDecoderFactory } from '../../setup/seams.js';
import { UnfulfilledSlots } from '../../lifecycle/unfulfilledSlots.js';
import { FrameAccounting } from '../frameAccounting.js';
import { BoundedReceiveVerificationRecorder } from '../receiveVerification.js';
import { RECOVERY_REASON_FOR, VideoIngress } from '../videoIngress.js';
import { VideoSlotPipeline, type SlotLossEvent } from '../videoSlotPipeline.js';
import { makeIdentity, type TestIdentity } from './frameFixtures.js';
import { buildUnsignedFrame, finishFrame } from '../../frame/frameCodec.js';
import { hexToBytes } from '../../frame/hex.js';
import { HEADER_VERSION } from '../../frame/wireConstants.js';
import { loadFrameVectors } from '../../frame/__tests__/frameVectors.js';
import {
  KEK,
  controlledStream,
  counterSum,
  dropsByReason,
  expectIdentity,
  manualClock,
  videoFrame,
  videoMetrics,
} from './videoFixtures.js';

const SINK_ERR = (): void => {};
const A = 401;
const B = 402;
const SELF = 9;
const MUTED = 403;
const URL = 'https://mh-0.example';
const AUDIO_SLOT = 0;
const VIDEO_SLOTS = [2, 3];

let alice: TestIdentity;
let bob: TestIdentity;
let mallory: TestIdentity;
let mutedId: TestIdentity;
let registry: FakeVideoFrameRegistry;

beforeAll(async () => {
  alice = await makeIdentity(0x61);
  bob = await makeIdentity(0x62);
  mallory = await makeIdentity(0x63);
  mutedId = await makeIdentity(0x64);
});

beforeEach(() => {
  registry = new FakeVideoFrameRegistry();
});

afterEach(() => {
  expect(registry.notClosedExactlyOnce()).toEqual([]);
});

async function harness() {
  const { metrics, sink } = videoMetrics();
  const roster = new RosterIdentityKeys(8);
  await roster.upsert({ senderId: A, identityPublicKey: alice.publicKey });
  await roster.upsert({ senderId: B, identityPublicKey: bob.publicKey });
  await roster.upsert({ senderId: MUTED, identityPublicKey: mutedId.publicKey });
  const clock = manualClock();
  const decoders = new FakeVideoDecoders({ registry });
  const unfulfilled = new UnfulfilledSlots(metrics, clock.now);
  const losses: SlotLossEvent[] = [];
  const recorder = new BoundedReceiveVerificationRecorder();
  const accounting = new FrameAccounting(metrics, recorder);
  const slots = new Map<number, VideoSlotPipeline>();
  for (const slotId of VIDEO_SLOTS) {
    slots.set(
      slotId,
      new VideoSlotPipeline({
        slotId,
        config: DEFAULT_CLIENT_CONFIG.media.videoReceive,
        decoderRestartBackoffMs: 1_000,
        decoderFactory: decoders.factory as unknown as VideoDecoderFactory,
        metrics,
        accounting,
        guards: { selfSenderId: SELF, videoMuteState: () => 'unmuted' },
        clock: clock.now,
        unfulfilled,
        onRenderChanged: () => {},
        onLoss: (e) => losses.push(e),
        onDecoded: () => {},
        onDrawn: () => {},
        onDecoderFault: () => {},
        onSinkError: SINK_ERR,
      }),
    );
  }
  const allocations: number[] = [];
  const ingress = new VideoIngress({
    metrics,
    accounting,
    ctx: {
      roster,
      keys: kekHolderWith(KEK, 0),
      cache: new TransmitKeyCache(8),
      replay: new ReplayWindow(8, 64),
    },
    gate: {
      kind: 'video',
      guards: { selfSenderId: SELF, videoMuteState: (s) => (s === MUTED ? 'muted' : 'unmuted') },
    },
    slotFor: (id) => slots.get(id),
    firstMedia: new FirstMediaObserver(metrics, clock.now),
    isStopped: () => false,
    verification: recorder,
    alloc: (n) => {
      allocations.push(n);
      return new Uint8Array(n);
    },
  });
  const stop = (): void => {
    for (const slot of slots.values()) slot.stop();
  };
  /** Drain every decoder's pending outputs, so held frames are closed at stop. */
  const drain = (): void => {
    for (const d of decoders.decoders) while (d.delivered < d.decoded.length) d.emitOutput();
  };
  return { ingress, sink, accounting, losses, decoders, slots, recorder, allocations, stop, drain };
}

type H = Awaited<ReturnType<typeof harness>>;

/**
 * Send `frames` on one stream as ONE chunk (a reset discards whatever the
 * reader has not yet read, exactly like RESET_STREAM), then end it: FIN, reset,
 * or leave it open (for a stream the reader aborts itself).
 */
async function stream(h: H, frames: readonly Uint8Array[], end: 'fin' | 'reset' | 'open' = 'fin') {
  const s = controlledStream();
  const read = h.ingress.readStream(s.readable, URL);
  const total = frames.reduce((n, f) => n + f.length, 0);
  const chunk = new Uint8Array(total);
  let at = 0;
  for (const f of frames) {
    chunk.set(f, at);
    at += f.length;
  }
  s.enqueue(chunk);
  if (end === 'fin') s.close();
  else if (end === 'reset') s.reset();
  await read;
  return s;
}

function done(h: H): void {
  expectIdentity(h.sink, h.accounting);
  h.drain();
  h.stop();
}

describe('video ingress — accept path and slot binding', () => {
  it('a keyframe + delta on a declared video slot are accepted and reach that slot decoder', async () => {
    const h = await harness();
    await stream(h, [
      await videoFrame({ sender: A, signer: alice, slot: 2, gop: 1, seq: 1, key: true }),
      await videoFrame({
        sender: A,
        signer: alice,
        slot: 2,
        gop: 1,
        seq: 2,
        key: false,
        keyBearing: true,
      }),
    ]);
    expect(counterSum(h.sink, 'dt_client_media_frames_accepted_total')).toBe(2);
    expect(h.decoders.decoders[0]?.decoded).toHaveLength(2); // slot 2's decoder
    expect(h.decoders.decoders[1]?.decoded).toHaveLength(0); // slot 3 untouched
    // Layer-2 evidence keyed (slot, VERIFIED sender).
    expect(h.recorder.snapshot().layers).toEqual([{ slot: 2, senderId: A, keyed: 2, verified: 2 }]);
    done(h);
  });

  it('an undeclared slot id: stream_slot_mismatch + undeclared diagnostic + the stream is aborted', async () => {
    const h = await harness();
    const s = await stream(
      h,
      [
        await videoFrame({ sender: A, signer: alice, slot: 9, gop: 1, seq: 1, key: true }),
        await videoFrame({
          sender: A,
          signer: alice,
          slot: 9,
          gop: 1,
          seq: 2,
          key: false,
          keyBearing: true,
        }),
      ],
      'open',
    );
    // The offending frame AND the frame already read behind it: every received
    // frame ends in exactly one outcome.
    expect(dropsByReason(h.sink).get('stream_slot_mismatch')).toBe(2);
    expect(counterSum(h.sink, 'dt_client_media_undeclared_stream_id_total')).toBe(1);
    expect(s.cancelled()).toBe(true);
    done(h);
  });

  it('an AUDIO slot id on a group stream is treated as undeclared (kind from transport)', async () => {
    const h = await harness();
    await stream(h, [
      await videoFrame({ sender: A, signer: alice, slot: AUDIO_SLOT, gop: 1, seq: 1, key: true }),
    ]);
    expect(dropsByReason(h.sink).get('stream_slot_mismatch')).toBe(1);
    expect(counterSum(h.sink, 'dt_client_media_frames_accepted_total')).toBe(0);
    done(h);
  });

  it('a later frame naming a different slot aborts the stream as stream_slot_mismatch', async () => {
    const h = await harness();
    const s = await stream(
      h,
      [
        await videoFrame({ sender: A, signer: alice, slot: 2, gop: 1, seq: 1, key: true }),
        await videoFrame({
          sender: A,
          signer: alice,
          slot: 3,
          gop: 1,
          seq: 2,
          key: false,
          keyBearing: true,
        }),
      ],
      'open',
    );
    expect(dropsByReason(h.sink).get('stream_slot_mismatch')).toBe(1);
    expect(counterSum(h.sink, 'dt_client_media_frames_accepted_total')).toBe(1);
    expect(s.cancelled()).toBe(true);
    done(h);
  });

  it('a prefix reject (unknown version) aborts the stream, counted once', async () => {
    const h = await harness();
    const good = await videoFrame({ sender: A, signer: alice, slot: 2, gop: 1, seq: 1, key: true });
    const bad = Uint8Array.from(good);
    bad[0] = 2;
    const s = await stream(h, [bad, good], 'open');
    expect(dropsByReason(h.sink).get('unknown_version')).toBe(1);
    expect(s.cancelled()).toBe(true);
    expect(h.losses).toHaveLength(0);
    done(h);
  });

  it('payload_length above MAX is refused from the header before any allocation', async () => {
    const h = await harness();
    const f = Uint8Array.from(
      await videoFrame({ sender: A, signer: alice, slot: 2, gop: 1, seq: 1, key: true }),
    );
    new DataView(f.buffer).setUint32(2, 0xffff_ffff, false);
    await stream(h, [f]);
    expect(h.allocations).toEqual([]);
    expect(dropsByReason(h.sink).get('payload_length_exceeds_max')).toBe(1);
    done(h);
  });
});

describe('video ingress — recovery requests from the FAILING frame header', () => {
  it('a bad signature mid-group: verify_failed with that frame sender and counter', async () => {
    const h = await harness();
    await stream(h, [
      await videoFrame({ sender: A, signer: alice, slot: 2, gop: 3, seq: 1, key: true }),
      await videoFrame({
        sender: A,
        signer: mallory,
        slot: 2,
        gop: 3,
        seq: 2,
        key: false,
        keyBearing: true,
      }),
    ]);
    expect(dropsByReason(h.sink).get('signature_invalid')).toBe(1);
    expect(h.losses).toEqual([
      { url: URL, slotId: 2, senderId: A, gopCounter: 3, reason: 'verify_failed' },
    ]);
    done(h);
  });

  it('just after a switch: the request names the NEW (failing) sender, not the last verified one', async () => {
    const h = await harness();
    await stream(h, [
      await videoFrame({ sender: A, signer: alice, slot: 2, gop: 3, seq: 1, key: true }),
    ]);
    // B's keyframe arrives on the next group stream and fails verification.
    await stream(h, [
      await videoFrame({ sender: B, signer: mallory, slot: 2, gop: 11, seq: 1, key: true }),
    ]);
    expect(h.losses).toEqual([
      { url: URL, slotId: 2, senderId: B, gopCounter: 11, reason: 'verify_failed' },
    ]);
    done(h);
  });

  it('roster lag on a new source: no_roster_entry requests verify_failed', async () => {
    const h = await harness();
    await stream(h, [
      await videoFrame({ sender: 499, signer: mallory, slot: 3, gop: 2, seq: 1, key: true }),
    ]);
    expect(dropsByReason(h.sink).get('no_roster_entry')).toBe(1);
    expect(h.losses).toEqual([
      { url: URL, slotId: 3, senderId: 499, gopCounter: 2, reason: 'verify_failed' },
    ]);
    done(h);
  });

  it('a broken group sends ONE request (later failures at the same pair send nothing)', async () => {
    const h = await harness();
    const frames = [
      await videoFrame({ sender: A, signer: alice, slot: 2, gop: 3, seq: 1, key: true }),
    ];
    for (let seq = 2; seq < 6; seq += 1) {
      frames.push(
        await videoFrame({
          sender: A,
          signer: mallory,
          slot: 2,
          gop: 3,
          seq,
          key: false,
          keyBearing: true,
        }),
      );
    }
    await stream(h, frames);
    expect(h.losses).toHaveLength(1);
    done(h);
  });

  it('replay, self and a muted sender send no request; a muted drop is recorded for the unmute edge', async () => {
    const h = await harness();
    const key = await videoFrame({ sender: A, signer: alice, slot: 2, gop: 1, seq: 1, key: true });
    await stream(h, [key, key]);
    await stream(h, [
      await videoFrame({ sender: MUTED, signer: mutedId, slot: 3, gop: 4, seq: 1, key: true }),
    ]);
    expect(dropsByReason(h.sink).get('replay_detected')).toBe(1);
    expect(dropsByReason(h.sink).get('sender_server_muted')).toBe(1);
    expect(h.losses).toHaveLength(0);
    h.slots.get(3)?.serverMuteLifted(MUTED);
    expect(h.losses).toEqual([
      { url: URL, slotId: 3, senderId: MUTED, gopCounter: 4, reason: 'server_mute_lifted' },
    ]);
    done(h);
  });

  it('a reset mid-frame AFTER the key id: stream_partial_reset + one group_abandoned request', async () => {
    const h = await harness();
    const key = await videoFrame({ sender: A, signer: alice, slot: 2, gop: 8, seq: 1, key: true });
    const next = await videoFrame({
      sender: A,
      signer: alice,
      slot: 2,
      gop: 8,
      seq: 2,
      key: false,
      keyBearing: true,
    });
    await stream(h, [key, next.subarray(0, next.length - 10)], 'reset');
    expect(dropsByReason(h.sink).get('stream_partial_reset')).toBe(1);
    expect(h.losses).toEqual([
      { url: URL, slotId: 2, senderId: A, gopCounter: 8, reason: 'group_abandoned' },
    ]);
    done(h);
  });

  it('a reset before any header is parsed sends nothing (MH recovers its own abandonment)', async () => {
    const h = await harness();
    const key = await videoFrame({ sender: A, signer: alice, slot: 2, gop: 8, seq: 1, key: true });
    await stream(h, [key.subarray(0, 10)], 'reset');
    expect(dropsByReason(h.sink).get('stream_partial_reset')).toBe(1);
    expect(h.losses).toHaveLength(0);
    done(h);
  });

  it('a FIN mid-frame is stream_partial_reset with NO request', async () => {
    const h = await harness();
    const key = await videoFrame({ sender: A, signer: alice, slot: 2, gop: 8, seq: 1, key: true });
    await stream(h, [key.subarray(0, key.length - 3)], 'fin');
    expect(dropsByReason(h.sink).get('stream_partial_reset')).toBe(1);
    expect(h.losses).toHaveLength(0);
    done(h);
  });

  it('a reset at a frame boundary after a whole frame: group_abandoned, no partial drop', async () => {
    const h = await harness();
    await stream(
      h,
      [await videoFrame({ sender: A, signer: alice, slot: 2, gop: 8, seq: 1, key: true })],
      'reset',
    );
    expect(dropsByReason(h.sink).get('stream_partial_reset')).toBeUndefined();
    expect(h.losses.map((l) => l.reason)).toEqual(['group_abandoned']);
    done(h);
  });
});

describe('video ingress — whole-frame rejects, key-id failures, teardown', () => {
  it('a whole-frame codec reject with a readable header: verify_failed with that header fields', async () => {
    const h = await harness();
    const row = loadFrameVectors().vectors.find((v) => v.name === 'ext_salience_out_of_range');
    const bytes = hexToBytes(row?.frame_hex ?? '');
    const relay = row?.offsets.relay_region_offset ?? 0;
    new DataView(bytes.buffer).setUint16(relay, 2, false); // MH's slot rewrite (unsigned)
    await stream(h, [bytes]);
    expect(dropsByReason(h.sink).get('extensions_malformed')).toBe(1);
    expect(h.losses).toEqual([
      {
        url: URL,
        slotId: 2,
        senderId: row?.decoded.key_id_decomposed.sender_id,
        gopCounter: row?.decoded.gop_counter,
        reason: 'verify_failed',
      },
    ]);
    done(h);
  });

  it('a decodable frame whose payload holds no key id: dropped, NO request (no sender, security R3)', async () => {
    const h = await harness();
    const unsigned = buildUnsignedFrame({
      headerVersion: HEADER_VERSION,
      flags: { independentlyDecodable: true, discardable: false, keyBearing: false },
      streamSequence: 1,
      gopCounter: 1,
      wrappedTransmitKey: null,
      extensions: [],
      streamId: 2,
      hopSequence: 1,
      payload: Uint8Array.of(1, 2, 3, 4),
    });
    await stream(h, [finishFrame(unsigned, new Uint8Array(64))]);
    expect(dropsByReason(h.sink).get('truncated')).toBe(1);
    expect(h.losses).toHaveLength(0);
    done(h);
  });

  it('cancelAll cancels every stream being read; settled resolves', async () => {
    const h = await harness();
    const s = controlledStream();
    const read = h.ingress.readStream(s.readable, URL);
    expect(h.ingress.openStreams).toBe(1);
    h.ingress.cancelAll();
    await read;
    await h.ingress.settled();
    expect(s.cancelled()).toBe(true);
    expect(h.ingress.openStreams).toBe(0);
    done(h);
  });
});

describe('the recovery mapping is a decision for EVERY reject reason (T2)', () => {
  it('RECOVERY_REASON_FOR has an entry for every RejectReason, each a recovery reason or null', () => {
    const recovery = new Set<string>(Object.values(MEDIA_RECOVERY_SOURCES));
    expect(new Set(Object.keys(RECOVERY_REASON_FOR))).toEqual(new Set(ALL_REJECT_REASONS));
    for (const reason of ALL_REJECT_REASONS) {
      const mapped = RECOVERY_REASON_FOR[reason as RejectReason];
      if (mapped !== null) expect(recovery).toContain(mapped);
    }
  });

  it('pins the security table: which reasons request and which do not', () => {
    const requests = ALL_REJECT_REASONS.filter((r) => RECOVERY_REASON_FOR[r] !== null).sort();
    expect(requests).toEqual(
      [
        'decrypt_failed',
        'extensions_malformed',
        'no_kek_for_generation',
        'no_roster_entry',
        'no_transmit_key',
        'payload_length_exceeds_available',
        'signature_invalid',
        'trailing_bytes',
        'truncated',
        'unwrap_failed',
      ].sort(),
    );
  });
});
