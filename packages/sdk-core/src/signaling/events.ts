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
import type { MediaKindToken, OpenMediaKindToken } from '../media/mediaKind.js';

export { SignalingCodec } from './codecMap.js';

/** A participant in the meeting roster (minimal shape for this story). */
export interface RosterParticipant {
  readonly participantId: string;
  readonly name: string;
  /**
   * The participant's per-meeting sender id (wire `optional uint32`), or
   * `undefined` when MC published none. Never coerced to 0. It is what maps a
   * slot's `senderId` and `StreamAssignmentsEvent.unreachableSenderIds` onto a
   * roster entry — presentation only, never attribution.
   */
  readonly senderId?: number | undefined;
}

/**
 * Bound on an MC-relayed participant id before it enters a public payload (the
 * proto's 256-byte "truncate before logging" rule for `server_muted_by` and the
 * `UnmuteRequest` relay). Ids are ASCII (UUIDs) in practice; the bound is on
 * UTF-16 code units, which is never looser for ASCII.
 */
export const MAX_RELAYED_PARTICIPANT_ID_LENGTH = 256;

/**
 * Payload of `participantMuteChanged` (proto `ParticipantMuteUpdate`).
 *
 * SERVER mute only (ADR-0036 §5, story 2 R-11), PER KIND (story 3 R-15): MC emits
 * this when a participant's server mute changes, and replays it right after the
 * join response for every participant server-muted in EITHER kind, carrying both
 * kinds' flags. So a participant with no update is server-unmuted in both kinds
 * (known, not unknown). The proto's self-mute booleans are deliberately NOT
 * projected: MC sends them as informational snapshots taken at the server-mute
 * change, so they go stale on the next self-mute and are not a self-mute source
 * (the subscriber-visible signal is the slot's `avatar_client_muted`).
 */
export interface ParticipantMuteEvent {
  readonly participantId: string;
  /** Whether this participant's audio is server-muted (enforced at MH ingress). */
  readonly audioServerMuted: boolean;
  /** Whether this participant's video is server-muted (enforced at MH ingress). */
  readonly videoServerMuted: boolean;
  /**
   * Participant id of whoever applied the server mute, when muted and MC said.
   * MC's own value, bounded to {@link MAX_RELAYED_PARTICIPANT_ID_LENGTH}. Resolve it
   * against the roster to render; never render or log the raw string.
   */
  readonly serverMutedBy?: string | undefined;
}

/**
 * Payload of `unmuteRequested` — MC relayed a server-muted participant's request
 * to be unmuted to THIS client (only a host receives it). It NOTIFIES; it never
 * lifts the mute (R-10). `participantId` is MC-stamped from the requester's
 * authenticated connection, bounded like {@link ParticipantMuteEvent.serverMutedBy}.
 */
export interface UnmuteRequestedEvent {
  readonly participantId: string;
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
  /**
   * MC's per-kind receive-slot caps (`JoinResponse.max_audio_receive_slots` /
   * `max_video_receive_slots`, story 3 R-1), passed through verbatim:
   * `undefined` when absent (an MC that does not send them — the caps are then
   * UNKNOWN), and a present `0` kept as `0` (legal: a video cap of 0 is the
   * video-off lever, R-26). Never coerced. Informational only: MC truncates, so
   * a client never shrinks its declaration from these.
   */
  readonly maxAudioReceiveSlots?: number | undefined;
  readonly maxVideoReceiveSlots?: number | undefined;
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
   * Frame header versions this client can produce (ADR-0036 §2). MC directs
   * one meeting-wide version, and a join not declaring the current one is
   * refused (`ParticipantCapabilities.supported_header_versions`).
   *
   * Omit it to declare the SDK's own set (`SUPPORTED_HEADER_VERSIONS` in
   * `media/frame/frameCodec.ts`); a value given here replaces that default.
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

/**
 * What a participant publishes (story 3 R-2): a closed vocabulary mapped onto
 * `MEDIA_KIND_AUDIO` / `MEDIA_KIND_VIDEO_CAMERA` at the wire boundary. Screen
 * share is not a kind this story serves and is unrepresentable here.
 */
export type PublishKind = MediaKindToken;

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
  /** What this slot can decode. */
  readonly mediaKind: MediaKindToken;
}

/** `EncodingParameters` as received on a send directive (story 3). */
export interface SendStreamEncoding {
  /** The public codec, or `null` for `CODEC_UNSPECIFIED` / an unknown value. */
  readonly codec: SignalingCodec | null;
  readonly width: number;
  readonly height: number;
  readonly frameRate: number;
  readonly maxBitrateBps: number;
  /** `optional` on the wire: `undefined` when absent, a present 0 stays 0. */
  readonly minKeyframeIntervalMs: number | undefined;
}

/** One stream MC directs this client to produce (ADR-0036 §5). */
export interface SendStreamDirective {
  /** The publisher's stream index. 8-bit semantics: it is the key id's stream field. */
  readonly streamNumber: number;
  /** Media kind. `unknown` is a kind this client never publishes (fail closed: never sent). */
  readonly mediaKind: OpenMediaKindToken;
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
   * The directed `EncodingParameters` as received, or `undefined` when MC sent
   * none. Carried RAW (0 = absent for the plain scalars): the video send path
   * validates it (`session/mediaWiring.ts:validateVideoEncoding`) and refuses a
   * video stream missing any field — there is no client default for any video
   * encoding value. Audio reads only `maxBitrateBps` above.
   */
  readonly encoding: SendStreamEncoding | undefined;
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

/**
 * A slot's state, in tokens IDENTICAL to MC's `slot_state_label()`
 * (`crates/mc-service/src/media_signaling/assignments.rs`) and the
 * observability labels, so the client's and MC's distributions compare
 * directly. A FLAT vocabulary (story 3 wire freeze, ADR-0036 §6): one value per
 * thing a receiver renders.
 *
 *   * `active` — live. Audio names the sender; VIDEO DOES NOT (the verified
 *     key-id sender names it, R-5).
 *   * `avatar_no_video` / `avatar_client_muted` / `avatar_server_muted` — an
 *     avatar for the named participant, with the reason (server wins over
 *     client).
 *   * `fewer_sources_than_slots`, `unavailable` (over MC's per-kind cap,
 *     truncated) — sourceless.
 *   * `switch_pending`, `withheld_by_congestion`, `source_unreachable` —
 *     retained on the wire but NEVER EMITTED; a consumer renders them FAIL
 *     CLOSED (no media from the slot).
 *   * `unspecified` — the wire's zero value or ANY number this SDK does not
 *     name (the enum is open). Fail closed, never guessed.
 *
 * The one mapping site is `signaling/slotStateMap.ts`. Nothing but the wire's
 * `ACTIVE` ever maps to `active`.
 */
export type SlotStateToken =
  | 'unspecified'
  | 'active'
  | 'avatar_no_video'
  | 'avatar_client_muted'
  | 'avatar_server_muted'
  | 'fewer_sources_than_slots'
  | 'unavailable'
  | 'switch_pending'
  | 'withheld_by_congestion'
  | 'source_unreachable';

/** A slot's media kind, from the DECLARED slot (never a frame). `unknown` renders no media. */
export type SlotMediaKind = OpenMediaKindToken;

/** What MC has placed in one of this client's declared slots (ADR-0036 §6). */
export interface StreamAssignmentEvent {
  readonly slotId: number;
  /** The slot's media kind as MC echoed it (`StreamAssignment.media_kind`). */
  readonly mediaKind: SlotMediaKind;
  /**
   * The participant the slot shows, or `undefined` when MC names none: every
   * sourceless state, and a LIVE VIDEO slot (MC never names the live video
   * source — story 3 R-5; the frame's verified key-id sender does).
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
   * Why the slot looks the way it does — see {@link SlotStateToken}.
   *
   * EXPLICIT ON THE WIRE because absence of frames is not a signal (ADR-0036 §6).
   */
  readonly slotState: SlotStateToken;
}

/** The subscriber's current slot assignments. */
export interface StreamAssignmentsEvent {
  readonly assignments: readonly StreamAssignmentEvent[];
  /**
   * Roster participants THIS subscriber cannot reach (ADR-0036 §9).
   *
   * They share no connected media handler with this subscriber, and there is no
   * cross-handler forwarding. Per-subscriber, because reachability is relative to
   * the subscriber's own connected set.
   *
   * ---------------------------------------------------------------------------
   * A ROSTER MARK, NOT A SLOT STATE, AND NOT INFERRED FROM SILENCE
   * ---------------------------------------------------------------------------
   *
   * An unreachable participant consumes NO slot, so it never appears as an empty
   * or `source_unreachable` slot — `signaling.proto` reserves that state for the
   * pinned case, which static fill cannot produce. The wire carries this because
   * §6's rule is that absence of frames is not a signal: "withheld by
   * congestion", "fewer sources" and "cannot be reached" all present as no audio
   * and must render differently.
   *
   * REPLACE, NEVER MERGE, exactly as with `assignments`: each message is MC's
   * complete current view for this subscriber, and a peer that becomes reachable
   * simply stops appearing here. Merging would latch a transient into a
   * permanent "can't hear X" badge on a meeting whose audio is fine — the failure
   * that costs an incident, because it is indistinguishable to the user from the
   * real fault.
   *
   * WHAT THIS DELIBERATELY DOES NOT DISCLOSE: it is one bit per roster entry,
   * relative to this subscriber. It carries no handler identity, no handler count
   * and no other subscriber's partition, and a client MUST NOT infer co-location
   * AMONG the unreachable set — being absent from my reachable set says nothing
   * about whether two unreachable peers share a handler with each other.
   * Grouping them is the step that turns a reachability bit into a topology map.
   */
  readonly unreachableSenderIds: readonly number[];
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
  /**
   * What this participant publishes (`JoinRequest.publish_kinds`, story 3 R-2).
   * FIXED FOR THE MEMBERSHIP: MC refuses a reconnect presenting a different
   * set. Copied and frozen by the client; never re-read from the caller.
   */
  readonly publishKinds: readonly PublishKind[];
  /**
   * The INITIAL receive declaration (`JoinRequest.receive_capability`), in slot
   * id order (`session/mediaSelection.ts:receiveDeclaration`).
   */
  readonly receiveSlots: readonly ReceiveSlotDeclaration[];
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
