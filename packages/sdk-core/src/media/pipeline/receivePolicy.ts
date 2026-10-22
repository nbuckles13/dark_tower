// File: packages/sdk-core/src/media/pipeline/receivePolicy.ts
//
// HOT PATH. Per-frame code only: no logging, no metric names, no label
// construction.
//
// ---------------------------------------------------------------------------
// THE ONE RECEIVE POLICY, BY KIND (story 3 R-5)
// ---------------------------------------------------------------------------
//
// Every received frame of EITHER kind runs through `receiveByKind` — the audio
// datagram ingress (`ingress.ts`) and the video stream ingress
// (`videoIngress.ts`) call it, and neither carries a copy of its body. The
// KIND is the caller's: datagrams feed audio, group streams feed video. It is
// never read from the frame, so a video frame can never reach an audio lane or
// the reverse, by construction.
//
//   both kinds:  identity key from `key_id.sender_id` ONLY  -> `no_roster_entry`
//                verify (Ed25519, structural brand)          -> `signature_invalid`
//   audio:       sender assigned to a declared slot, BEFORE open
//                                                            -> `sender_not_assigned`
//                open (replay, unwrap, decrypt)
//   video:       verified sender is this client              -> `self_sender`
//                verified sender is video-server-muted NOW   -> `sender_server_muted`
//                open (replay per (KEK gen, sender, stream), unwrap, decrypt) —
//                in ANY slot state: the video decode-before-live exception
//
// The two video sender guards run on the VERIFIED key-id sender, after verify
// and before open (observability ruling O2, security concurred): a frame from a
// sender ALREADY server-muted when it ARRIVED is the precise receiver-side
// signal that MH's ingress drop failed (A5), and neither guard's frame is ever
// decrypted, decoded or rendered. A NEVER-RENDER re-check of the same two
// predicates runs on held decoded frames at render time
// (`renderDecision.ts`); together they are the one composed policy whose
// complete outcome set is {@link RECEIVE_POLICY_OUTCOMES}.
//
// Order is load-bearing, as in story 2: every gate runs AFTER verify (so a drop
// carries no unverified claim) and BEFORE `openVerifiedFrame` (so a gated frame
// neither decrypts, caches a transmit key from its wrap, nor advances the
// replay window).

import {
  openVerifiedFrame,
  verifyFrame,
  RECEIVE_PATH_REASONS,
  type OpenedFrame,
  type ReceiverKeys,
  type ReplayWindow,
  type TransmitKeyCache,
} from '../frame/receivePath.js';
import type { DecodedFrame } from '../frame/frameCodec.js';
import { assignmentReject, keyReject, type RejectReason } from '../frame/rejectReason.js';
import type { DecodeLane } from './receiveLanes.js';
import { classifySender, type RenderGuards } from './renderDecision.js';

/** Resolves a verification key from a frame's own `key_id.sender_id`. */
export interface IdentityKeyResolver {
  /** `undefined` when this client holds no usable key for `senderId`. */
  identityKeyFor(senderId: number): CryptoKey | undefined;
}

/**
 * A decoded frame and the sender its key id names, parsed ONCE by the caller
 * (who needs the sender earlier for the hop observation and the verification
 * recorder) so the two can never disagree. The sender is UNVERIFIED here.
 */
export interface KeyedFrame {
  readonly decoded: DecodedFrame;
  readonly senderId: number;
}

/** Receiver state the policy reads. Per kind; see `lifecycle/receiverState.ts`. */
export interface ReceiveContext {
  readonly roster: IdentityKeyResolver;
  readonly keys: ReceiverKeys;
  readonly cache: TransmitKeyCache;
  readonly replay: ReplayWindow;
}

/** The kind-specific gate. The kind is the CALLER's transport's, never the frame's. */
export type ReceiveGate =
  | {
      readonly kind: 'audio';
      /** The lane for an ASSIGNED sender, or `undefined` for any other. */
      laneFor(senderId: number): DecodeLane | undefined;
    }
  | {
      readonly kind: 'video';
      /**
       * The ONE sender classifier's inputs (`renderDecision.ts:classifySender`),
       * shared with the render-time re-check so the two can never disagree.
       */
      readonly guards: RenderGuards;
    };

/** An AUDIO frame the policy admitted, with the lane it was admitted to. */
export interface ReceivedAudioFrame {
  readonly opened: OpenedFrame;
  readonly lane: DecodeLane;
}

/** A VIDEO frame the policy admitted (its slot comes from the transport). */
export interface ReceivedVideoFrame {
  readonly opened: OpenedFrame;
}

/**
 * Every outcome of the ONE composed receive policy: the reject reasons
 * {@link receiveByKind} can throw, its accept, and the render-time outcomes of
 * `renderDecision.ts` (a decoded frame held but not drawn in a non-ACTIVE slot,
 * drawn in an ACTIVE slot, or refused by the never-render re-check). The
 * branch-table test's completeness assertion is against THIS const, so a new
 * branch on either side of the split fails without a row.
 */
export const RECEIVE_POLICY_OUTCOMES = [
  'no_roster_entry',
  'sender_not_assigned',
  'self_sender',
  'sender_server_muted',
  ...RECEIVE_PATH_REASONS,
  'accepted',
  'held',
  'drawn',
  'never_render',
] as const satisfies readonly (RejectReason | 'accepted' | 'held' | 'drawn' | 'never_render')[];

/** One of {@link RECEIVE_POLICY_OUTCOMES}. */
export type ReceivePolicyOutcome = (typeof RECEIVE_POLICY_OUTCOMES)[number];

/**
 * Run one keyed frame through the receive policy for `gate.kind`.
 *
 * @throws {FrameRejectedError} for every wire or policy reason — the caller
 * counts it verbatim. Anything else is a defect and propagates.
 */
export async function receiveByKind(
  keyed: KeyedFrame,
  gate: Extract<ReceiveGate, { kind: 'audio' }>,
  ctx: ReceiveContext,
): Promise<ReceivedAudioFrame>;
export async function receiveByKind(
  keyed: KeyedFrame,
  gate: Extract<ReceiveGate, { kind: 'video' }>,
  ctx: ReceiveContext,
): Promise<ReceivedVideoFrame>;
export async function receiveByKind(
  keyed: KeyedFrame,
  gate: ReceiveGate,
  ctx: ReceiveContext,
): Promise<ReceivedAudioFrame | ReceivedVideoFrame>;
export async function receiveByKind(
  keyed: KeyedFrame,
  gate: ReceiveGate,
  ctx: ReceiveContext,
): Promise<ReceivedAudioFrame | ReceivedVideoFrame> {
  const { decoded, senderId } = keyed;
  const identityKey = ctx.roster.identityKeyFor(senderId);
  if (!identityKey) {
    // FAIL CLOSED. `undefined` covers unknown participant, an EMPTY published
    // key and an unusable key — all three mean this frame cannot be verified.
    // It is never "no key, so skip verification".
    throw keyReject('no_roster_entry', "no usable identity key for the frame's key-id sender");
  }

  // VERIFY BEFORE DECRYPT, structurally: `openVerifiedFrame` is unreachable
  // without the brand `verifyFrame` applies.
  const verified = await verifyFrame(decoded, identityKey);

  if (gate.kind === 'audio') {
    // THE SLOT-EDGE GATE — after verify, before open (story 2).
    const lane = gate.laneFor(senderId);
    if (!lane) {
      throw assignmentReject(
        'sender_not_assigned',
        "the frame's sender is not assigned to any of this client's receive slots",
      );
    }
    return { opened: await openVerifiedFrame(verified, ctx.cache, ctx.keys, ctx.replay), lane };
  }
  // The ONE sender classifier, ARRIVAL mapping (see `classifySender`): self and
  // muted drop; `unmapped` passes here and is refused at render.
  const sender = classifySender(senderId, gate.guards);
  if (sender === 'self') {
    throw assignmentReject('self_sender', "the frame's verified sender is this client");
  }
  if (sender === 'muted') {
    throw assignmentReject(
      'sender_server_muted',
      "the frame's verified sender is server-muted for video",
    );
  }
  return { opened: await openVerifiedFrame(verified, ctx.cache, ctx.keys, ctx.replay) };
}
