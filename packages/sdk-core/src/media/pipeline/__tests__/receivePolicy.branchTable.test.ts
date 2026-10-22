// File: packages/sdk-core/src/media/pipeline/__tests__/receivePolicy.branchTable.test.ts
//
// THE ONE RECEIVE POLICY, EVERY BRANCH (story 3 R-5; security acceptance "a
// single receive policy, with every branch tested"). One table: each row drives
// ONE branch with REAL crypto and names the outcome it must produce. The
// completeness assertion compares the rows' outcomes with
// `RECEIVE_POLICY_OUTCOMES` — the CODE's declared outcome set — so a new branch
// on either side of the policy/render split fails here until it has a row.

import { beforeAll, describe, expect, it } from 'vitest';
import { FakeVideoDecoders, FakeVideoFrameRegistry } from '@darktower/test-utils';

import { DEFAULT_CLIENT_CONFIG } from '../../../config/clientConfig.js';
import { ReplayWindow, TransmitKeyCache } from '../../frame/receivePath.js';
import { decodeFrame } from '../../frame/frameCodec.js';
import { FrameRejectedError } from '../../frame/rejectReason.js';
import { AES_256_KEY_BYTES } from '../../frame/sframe.js';
import { gateFor, kekHolderWith } from '../../__tests__/helpers.js';
import { RosterIdentityKeys } from '../../setup/rosterKeys.js';
import type { VideoDecoderFactory } from '../../setup/seams.js';
import { UnfulfilledSlots } from '../../lifecycle/unfulfilledSlots.js';
import { FrameAccounting } from '../frameAccounting.js';
import {
  RECEIVE_POLICY_OUTCOMES,
  receiveByKind,
  type ReceiveContext,
  type ReceiveGate,
  type ReceivePolicyOutcome,
} from '../receivePolicy.js';
import { VideoSlotPipeline } from '../videoSlotPipeline.js';
import { buildTestFrame, makeIdentity, type TestIdentity } from './frameFixtures.js';
import { KEK, manualClock, videoFrame, videoMetrics } from './videoFixtures.js';

const SINK_ERR = (): void => {};
const A = 301;
const SELF = 9;
const MUTED = 302;
const UNROSTERED = 303;

let alice: TestIdentity;
let muted: TestIdentity;
let self: TestIdentity;
let mallory: TestIdentity;

beforeAll(async () => {
  alice = await makeIdentity(0x51);
  muted = await makeIdentity(0x52);
  self = await makeIdentity(0x53);
  mallory = await makeIdentity(0x54);
});

async function context(kekGeneration = 0): Promise<ReceiveContext> {
  const roster = new RosterIdentityKeys(8);
  await roster.upsert({ senderId: A, identityPublicKey: alice.publicKey });
  await roster.upsert({ senderId: MUTED, identityPublicKey: muted.publicKey });
  await roster.upsert({ senderId: SELF, identityPublicKey: self.publicKey });
  return {
    roster,
    keys: kekHolderWith(KEK, kekGeneration),
    cache: new TransmitKeyCache(8),
    replay: new ReplayWindow(8, 64),
  };
}

const VIDEO_GATE: Extract<ReceiveGate, { kind: 'video' }> = {
  kind: 'video',
  guards: {
    selfSenderId: SELF,
    videoMuteState: (s) => (s === MUTED ? 'muted' : 'unmuted'),
  },
};

async function outcomeOf(
  bytes: Uint8Array,
  gate: ReceiveGate,
  ctx: ReceiveContext,
): Promise<ReceivePolicyOutcome> {
  const decoded = decodeFrame(bytes);
  const { parseSframe } = await import('../../frame/sframe.js');
  const { unpackKeyId } = await import('../../frame/keyId.js');
  const senderId = Number(unpackKeyId(parseSframe(decoded.payload).keyId).senderId);
  try {
    await receiveByKind({ decoded, senderId }, gate, ctx);
    return 'accepted';
  } catch (err) {
    if (err instanceof FrameRejectedError) return err.rejectReason as ReceivePolicyOutcome;
    throw err;
  }
}

/** Render-side rows: a decoded frame against the slot's state and the never-render re-check. */
function renderOutcome(
  state: 'active' | 'avatar_client_muted',
  mutedAfterDecode: boolean,
): ReceivePolicyOutcome {
  const registry = new FakeVideoFrameRegistry();
  const decoders = new FakeVideoDecoders({ registry });
  const { metrics } = videoMetrics();
  const clock = manualClock();
  let isMuted = false;
  const drawn: unknown[] = [];
  const slot = new VideoSlotPipeline({
    slotId: 0,
    config: DEFAULT_CLIENT_CONFIG.media.videoReceive,
    decoderRestartBackoffMs: 1_000,
    decoderFactory: decoders.factory as unknown as VideoDecoderFactory,
    metrics,
    accounting: new FrameAccounting(metrics),
    guards: { selfSenderId: SELF, videoMuteState: () => (isMuted ? 'muted' : 'unmuted') },
    clock: clock.now,
    unfulfilled: new UnfulfilledSlots(metrics, clock.now),
    onRenderChanged: () => {},
    onLoss: () => {},
    onDecoded: () => {},
    onDrawn: () => {},
    onDecoderFault: () => {},
    onSinkError: SINK_ERR,
  });
  slot.setSink({ drawFrame: (f) => drawn.push(f) });
  slot.setMcState('avatar_client_muted', A);
  slot.push({
    senderId: A,
    gopCounter: 1,
    isKeyframe: true,
    data: new Uint8Array([0, 0, 0, 0x9d, 1, 0x2a, 0x80, 2, 0x68, 1]),
    url: 'u',
  });
  decoders.last!.emitOutput();
  if (mutedAfterDecode) isMuted = true;
  if (state === 'active') slot.setMcState('active', undefined);
  const outcome: ReceivePolicyOutcome =
    drawn.length > 0 ? 'drawn' : state === 'active' ? 'never_render' : 'held';
  slot.stop();
  expect(registry.notClosedExactlyOnce()).toEqual([]);
  return outcome;
}

interface Row {
  readonly name: string;
  readonly expected: ReceivePolicyOutcome;
  readonly run: () => Promise<ReceivePolicyOutcome>;
}

const frameOf = (
  signer: TestIdentity,
  sender: number,
  extra: Partial<Parameters<typeof buildTestFrame>[0]> = {},
) =>
  buildTestFrame({
    keyIdSenderId: sender,
    kek: KEK,
    signer,
    plaintext: new Uint8Array([1, 2, 3, 4]),
    stream: 1,
    ...extra,
  });

const ROWS: Row[] = [
  // --- AUDIO (unchanged from story 2) -------------------------------------
  {
    name: 'audio: an assigned rostered sender is accepted',
    expected: 'accepted',
    run: async () =>
      outcomeOf(
        await frameOf(alice, A, { stream: 0 }),
        { kind: 'audio', ...gateFor(A) },
        await context(),
      ),
  },
  {
    name: 'audio: an UNASSIGNED rostered sender is sender_not_assigned, before open',
    expected: 'sender_not_assigned',
    run: async () => {
      const ctx = await context();
      const out = await outcomeOf(
        await frameOf(alice, A, { stream: 0 }),
        { kind: 'audio', ...gateFor() },
        ctx,
      );
      // Before open: the frame's wrap was NOT cached.
      const { packKeyId } = await import('../../frame/keyId.js');
      const kid = packKeyId({ senderId: BigInt(A), stream: 0n, generation: 0n });
      expect(ctx.cache.has({ senderId: BigInt(A), stream: 0n, generation: 0n }, kid)).toBe(false);
      return out;
    },
  },
  // --- BOTH KINDS -----------------------------------------------------------
  {
    name: 'video: an unrostered sender is no_roster_entry',
    expected: 'no_roster_entry',
    run: async () => outcomeOf(await frameOf(mallory, UNROSTERED), VIDEO_GATE, await context()),
  },
  {
    name: "video: a frame signed by someone else under A's key id is signature_invalid",
    expected: 'signature_invalid',
    run: async () => outcomeOf(await frameOf(mallory, A), VIDEO_GATE, await context()),
  },
  // --- VIDEO ----------------------------------------------------------------
  {
    name: 'video: a rostered sender is accepted (decoded in any slot state)',
    expected: 'accepted',
    run: async () => outcomeOf(await frameOf(alice, A), VIDEO_GATE, await context()),
  },
  {
    name: "video: this client's own verified sender is self_sender",
    expected: 'self_sender',
    run: async () => outcomeOf(await frameOf(self, SELF), VIDEO_GATE, await context()),
  },
  {
    name: 'video: a sender server-muted at arrival is sender_server_muted',
    expected: 'sender_server_muted',
    run: async () => outcomeOf(await frameOf(muted, MUTED), VIDEO_GATE, await context()),
  },
  {
    name: 'video: the same frame twice is replay_detected',
    expected: 'replay_detected',
    run: async () => {
      const ctx = await context();
      const bytes = await frameOf(alice, A);
      expect(await outcomeOf(bytes, VIDEO_GATE, ctx)).toBe('accepted');
      return outcomeOf(bytes, VIDEO_GATE, ctx);
    },
  },
  {
    name: 'video: a wrap under an unheld NEWER KEK generation is no_kek_for_generation',
    expected: 'no_kek_for_generation',
    run: async () =>
      outcomeOf(await frameOf(alice, A, { kekGeneration: 5 }), VIDEO_GATE, await context()),
  },
  {
    name: 'video: a wrap under a generation OLDER than retention is kek_generation_stale',
    expected: 'kek_generation_stale',
    run: async () =>
      outcomeOf(await frameOf(alice, A, { kekGeneration: 1 }), VIDEO_GATE, await context(3)),
  },
  {
    name: 'video: a non-key-bearing frame for an unknown key id is no_transmit_key',
    expected: 'no_transmit_key',
    run: async () =>
      outcomeOf(await frameOf(alice, A, { keyBearing: false }), VIDEO_GATE, await context()),
  },
  {
    name: 'video: a wrap under a different KEK is unwrap_failed',
    expected: 'unwrap_failed',
    run: async () =>
      outcomeOf(
        await frameOf(alice, A, { kek: new Uint8Array(AES_256_KEY_BYTES).fill(0x77) }),
        VIDEO_GATE,
        await context(),
      ),
  },
  {
    name: 'video: a validly signed frame sealed under another transmit key is decrypt_failed',
    expected: 'decrypt_failed',
    run: async () =>
      outcomeOf(
        await frameOf(alice, A, { sealTransmitKey: new Uint8Array(AES_256_KEY_BYTES).fill(0x66) }),
        VIDEO_GATE,
        await context(),
      ),
  },
  {
    name: 'video: KIND FROM THE TRANSPORT — the same audio-stream bytes through the video gate are video',
    expected: 'accepted',
    run: async () => {
      // A key-id stream 0 frame (what an audio publisher would send) reaching the
      // VIDEO path is decided by the video gate (no slot-edge gate applies),
      // because the kind is the caller's transport, never the frame's.
      const bytes = await frameOf(alice, A, { stream: 0 });
      expect(await outcomeOf(bytes, { kind: 'audio', ...gateFor() }, await context())).toBe(
        'sender_not_assigned',
      );
      return outcomeOf(bytes, VIDEO_GATE, await context());
    },
  },
  // --- RENDER SIDE (`renderDecision.ts`) ----------------------------------
  {
    name: 'render: a decoded frame in a NON-ACTIVE slot is held, never drawn',
    expected: 'held',
    run: async () => renderOutcome('avatar_client_muted', false),
  },
  {
    name: 'render: a held frame is drawn when the slot turns ACTIVE',
    expected: 'drawn',
    run: async () => renderOutcome('active', false),
  },
  {
    name: 'render: a held frame whose sender was muted after decode is never rendered',
    expected: 'never_render',
    run: async () => renderOutcome('active', true),
  },
];

describe('receive policy branch table (story 3 R-5)', () => {
  it.each(ROWS.map((r) => [r.name, r] as const))('%s', async (_name, row) => {
    expect(await row.run()).toBe(row.expected);
  });

  it('the rows cover EXACTLY the declared policy outcome set (completeness)', () => {
    expect(new Set(ROWS.map((r) => r.expected))).toEqual(new Set(RECEIVE_POLICY_OUTCOMES));
  });

  it('video frames reach no audio lane: the video gate has no lane lookup at all', async () => {
    const ctx = await context();
    const received: object = await receiveByKind(
      {
        decoded: decodeFrame(
          await videoFrame({ sender: A, signer: alice, slot: 4, gop: 1, seq: 1, key: true }),
        ),
        senderId: A,
      },
      VIDEO_GATE,
      ctx,
    );
    expect('lane' in received).toBe(false);
  });
});
