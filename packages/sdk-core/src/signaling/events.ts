// File: packages/sdk-core/src/signaling/events.ts
//
// R-16/R-17: the PUBLIC, plain-typed surface of the signaling layer. These types
// are deliberately decoupled from the generated protobuf-es types: the generated
// `*_pb.ts` are gitignored and not part of the published API, so leaking them into
// the public `.d.ts` would break consumers on a clean checkout. SignalingClient
// maps the wire types onto these plain shapes at the dispatch boundary.
//
// `senderId` is `number | undefined` (proto `optional uint32`, ADR-0036 §2).
// The roster is intentionally minimal for this story (participantId + name);
// per-slot stream metadata arrives with the receive-capability model (§6).

import { LeaveReason } from '../proto/dark_tower/signaling/v1/signaling_pb.js';

import type { SignalingCodec } from './codecMap.js';

export { SignalingCodec } from './codecMap.js';

/** A participant in the meeting roster (minimal shape for this story). */
export interface RosterParticipant {
  readonly participantId: string;
  readonly name: string;
}

/**
 * Payload of the `joined` event (proto `JoinResponse`).
 *
 * `senderId` is the joiner's per-meeting numeric sender id (ADR-0036 §2): a
 * `number` carrying 16-bit semantics, valid 1..=65535, and `undefined` when MC
 * has not assigned one — the state this story ships. **Zero is never valid and
 * `undefined` MUST NOT be coerced to 0** (no `?? 0`): identical sender ids
 * collide on the SFrame key id and therefore on the derived AES-GCM wrap nonce
 * under one meeting KEK. It replaces the pre-ADR-0036 `userId`, which was a
 * hardcoded-zero `uint64` durable user id; a per-meeting id is deliberate,
 * because a durable one would make participants linkable across meetings.
 *
 * `mediaServers` are the MH WebTransport URLs taken from each
 * `MediaServerInfo.mediaHandlerUrl`. `correlationId`/`bindingToken` are stored for
 * future reconnection (storage only this story).
 *
 * Deliberately carries NO key material: the meeting KEK is on the wire
 * (`JoinResponse.meetingKek`) but is not projected here, because this object's
 * natural use is `console.log`.
 */
export interface JoinedEvent {
  readonly participantId: string;
  readonly senderId?: number | undefined;
  readonly existingParticipants: readonly RosterParticipant[];
  readonly mediaServers: readonly string[];
  readonly correlationId: string;
  readonly bindingToken: string;
}

/** Payload of the `participantJoined` event (proto `ParticipantJoined`). */
export interface ParticipantJoinedEvent {
  readonly participant: RosterParticipant;
}

/**
 * Bounded reason a participant left, mapped from the proto `LeaveReason` enum so
 * the public surface carries no generated enum. Unknown/unmapped values collapse
 * to `unknown`.
 */
export const ParticipantLeaveReason = {
  Voluntary: 'voluntary',
  Kicked: 'kicked',
  ConnectionLost: 'connection_lost',
  MeetingEnded: 'meeting_ended',
  Timeout: 'timeout',
  Unknown: 'unknown',
} as const;

export type ParticipantLeaveReason =
  (typeof ParticipantLeaveReason)[keyof typeof ParticipantLeaveReason];

/** Payload of the `participantLeft` event (proto `ParticipantLeft`). */
export interface ParticipantLeftEvent {
  readonly participantId: string;
  readonly reason: ParticipantLeaveReason;
}

/** Map the proto `LeaveReason` enum to the bounded public {@link ParticipantLeaveReason}. */
export function mapLeaveReason(reason: LeaveReason): ParticipantLeaveReason {
  switch (reason) {
    case LeaveReason.VOLUNTARY:
      return ParticipantLeaveReason.Voluntary;
    case LeaveReason.KICKED:
      return ParticipantLeaveReason.Kicked;
    case LeaveReason.CONNECTION_LOST:
      return ParticipantLeaveReason.ConnectionLost;
    case LeaveReason.MEETING_ENDED:
      return ParticipantLeaveReason.MeetingEnded;
    case LeaveReason.TIMEOUT:
      return ParticipantLeaveReason.Timeout;
    default:
      return ParticipantLeaveReason.Unknown;
  }
}

/**
 * Participant capabilities advertised in the `JoinRequest` (proto
 * `ParticipantCapabilities`). All fields optional; SignalingClient applies
 * proto3 defaults (empty arrays / false / 0) when omitted.
 */
export interface SignalingCapabilities {
  /**
   * Codecs this client can encode and decode, in the stable public vocabulary
   * ({@link SignalingCodec}). One list, not a video/audio pair: media kind is
   * a property of the codec's own identity, so encoding it a second time
   * positionally would let the two representations disagree.
   */
  readonly supportedCodecs?: readonly SignalingCodec[];

  /**
   * Frame header versions this client can produce and consume (ADR-0036 §2).
   * An upper bound on what the client can do — never a lower bound on what the
   * meeting accepts: MC selects from a server-side allowlist with a minimum
   * floor and rejects a client that declares nothing at or above it.
   */
  readonly supportedHeaderVersions?: readonly number[];
}

/** Parameters for {@link SignalingClient.join}. */

// ---------------------------------------------------------------------------
// ADR-0036 §4/§5/§6 media signaling
// ---------------------------------------------------------------------------

/**
 * Write side of the roster's identity-key feed.
 *
 * Declared STRUCTURALLY so signaling does not depend on the media pipeline's
 * concrete resolver. `RosterIdentityKeys` satisfies it.
 *
 * Identity public keys are PUBLIC material, so unlike the KEK they could ride an
 * event — but they are threaded through a sink anyway, for two reasons: it keeps
 * the public `RosterParticipant` surface unchanged (and therefore keeps key bytes
 * out of the web app's e2e-bus projection), and it makes the roster feed a
 * one-way write rather than something a consumer can read back and re-derive
 * attribution from.
 */
export interface RosterKeySink {
  /** Add or replace one participant's verification material. */
  upsert(entry: {
    readonly senderId: number;
    readonly identityPublicKey: Uint8Array;
  }): Promise<void>;
  /** Forget a participant. */
  remove(senderId: number): void;
}

/** One receive slot this client declares it can render (ADR-0036 §6). */
export interface ReceiveSlotDeclaration {
  /**
   * Subscriber-chosen slot id, echoed back in `StreamAssignment.slot_id`.
   *
   * SCOPED TO THIS SUBSCRIBER'S CONNECTION, not globally unique. 16-bit value
   * space, the same as the frame's relay-region `stream_id`, which is what lets
   * a receiver validate an arriving `stream_id` against its own declared slots.
   */
  readonly slotId: number;
  /** What this slot can decode. Audio only this story. */
  readonly mediaKind: 'audio';
}

/** One stream MC directs this client to produce (ADR-0036 §5). */
export interface SendStreamDirective {
  /** The publisher's stream index. 8-bit semantics: it is the key id's stream field. */
  readonly streamNumber: number;
  /** Media kind. Audio only this story; a video stream is ignored by this client. */
  readonly mediaKind: 'audio' | 'video' | 'other';
  /**
   * Directed bitrate ceiling, or `undefined` when MC did not set it.
   *
   * THE WIRE SSoT for the encoder's bitrate. The SDK applies its configured
   * default only when this is absent, and never a local ceiling on top: MC
   * validates the value at config load against a code-owned band and refuses to
   * start without satisfying it, so a client-side mirror would only hard-fail
   * fielded clients the first time an operator legitimately moved the ceiling.
   */
  readonly maxBitrateBps: number | undefined;
  /**
   * Media-handler URLs to send to.
   *
   * EMPTY MEANS SEND NOTHING (ADR-0036 §5). Resuming from empty rotates the
   * transmit key.
   */
  readonly targets: readonly string[];
}

/** What MC directs this client to produce, and where. */
export interface SendDirectiveEvent {
  readonly streams: readonly SendStreamDirective[];
  /** The meeting-wide frame header version every stream must use (ADR-0036 §2). */
  readonly headerVersion: number;
}

/** What MC has placed in one of this client's declared slots (ADR-0036 §6). */
export interface StreamAssignmentEvent {
  readonly slotId: number;
  /**
   * The source participant, or `undefined` when no source is assigned.
   *
   * ATTRIBUTION DOES NOT COME FROM HERE. A receiver attributes an arriving frame
   * from that frame's own `key_id.sender_id` and the signature verifying against
   * the roster key for it; a compromised MC can mis-map this field and the media
   * path must not care. This is presentation state, not identity.
   */
  readonly senderId: number | undefined;
  /** Which handler to receive this slot's media from. Empty when unassigned. */
  readonly mediaHandlerUrl: string;
  /**
   * Why the slot looks the way it does.
   *
   * EXPLICIT ON THE WIRE because absence of frames is not a signal (ADR-0036 §6):
   * "withheld by congestion", "fewer sources than slots" and "source unreachable"
   * are indistinguishable to a client — all present as no media — and render
   * completely differently.
   */
  readonly slotState:
    | 'unspecified'
    | 'active'
    | 'source_muted'
    | 'withheld_congestion'
    | 'fewer_sources'
    | 'zero_requested'
    | 'source_unreachable'
    | 'switch_pending';
}

/** The subscriber's current slot assignments. */
export interface StreamAssignmentsEvent {
  readonly assignments: readonly StreamAssignmentEvent[];
}

export interface SignalingJoinParams {
  /**
   * The client's raw 32-byte Ed25519 identity signing public key (ADR-0036 §4).
   *
   * Generated per meeting and never persisted: a key reused across meetings
   * makes a participant linkable by public key regardless of display name, which
   * matters most for the guests who have the least identity assurance to begin
   * with. MC publishes it on the roster AS RECEIVED — no attestation check this
   * story — so a signature verifying against it proves only that every frame came
   * from the same keyholder, never who that keyholder is.
   *
   * Omitted when the caller is not starting media. MC then publishes an EMPTY
   * key, and every consumer must fail closed on it.
   */
  readonly identityPublicKey?: Uint8Array;
  /** MC WebTransport endpoint (`JoinMeetingResponse.mcAssignment.webtransportEndpoint`). */
  readonly webtransportEndpoint: string;
  /** Meeting id to join. */
  readonly meetingId: string;
  /** Meeting JWT from GC. Held in memory only; never logged or placed on errors (R-23). */
  readonly joinToken: string;
  /** Display name for this participant. */
  readonly participantName: string;
  /** Optional advertised capabilities. */
  readonly capabilities?: SignalingCapabilities;
}

/**
 * Plain, per-MH connection-outcome report — the boundary INPUT to
 * {@link SignalingClient.sendMediaConnectionUpdate}. PRODUCED by `MediaTransport`
 * (media layer) and CONSUMED by `SignalingClient` (signaling layer); sited HERE in
 * the foundational signaling layer (beside {@link SignalingJoinParams}) so the
 * dependency edge runs media→signaling, never the inversion (@dry-reviewer #4 /
 * @code-reviewer D). SignalingClient maps it onto the generated proto
 * `MhConnectionStatus` internally — no `*_pb` type crosses this boundary.
 *
 * `failureReason`/`failureCode` are SDK-AUTHORED bounded classifications (never a
 * raw transport-error message), present only when `state === 'failed'` (R-23).
 * `mhUrl`/`failureReason`/`failureCode` are capped ≤256 UTF-8 bytes by SignalingClient.
 */
export interface MhConnectionStatusReport {
  /** The MH URL this status pertains to (server-designated). */
  readonly mhUrl: string;
  /** Terminal per-MH outcome. */
  readonly state: 'connected' | 'failed';
  /** Bounded SDK failure reason (static string); set only when `state === 'failed'`. */
  readonly failureReason?: string;
  /** Bounded SDK failure code (enum-as-string); set only when `state === 'failed'`. */
  readonly failureCode?: string;
  /** Epoch ms the outcome was observed (injectable clock for deterministic tests). */
  readonly observedAtMs: number;
}
