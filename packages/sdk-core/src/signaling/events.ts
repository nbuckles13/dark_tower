// File: packages/sdk-core/src/signaling/events.ts
//
// R-16/R-17: the PUBLIC, plain-typed surface of the signaling layer. These types
// are deliberately decoupled from the generated protobuf-es types: the generated
// `*_pb.ts` are gitignored and not part of the published API, so leaking them into
// the public `.d.ts` would break consumers on a clean checkout. SignalingClient
// maps the wire types onto these plain shapes at the dispatch boundary.
//
// `userId` is `bigint` (proto `uint64` → protobuf-es `bigint`, R-17). The roster
// is intentionally minimal for this story (participantId + name); stream metadata
// is out of scope until layout/subscription lands.

import { LeaveReason } from '../proto/dark_tower/signaling/v1/signaling_pb.js';

/** A participant in the meeting roster (minimal shape for this story). */
export interface RosterParticipant {
  readonly participantId: string;
  readonly name: string;
}

/**
 * Payload of the `joined` event (proto `JoinResponse`). `userId` is a `bigint`
 * (proto `uint64`). `mediaServers` are the MH WebTransport URLs taken from each
 * `MediaServerInfo.mediaHandlerUrl`. `correlationId`/`bindingToken` are stored for
 * future reconnection (storage only this story).
 */
export interface JoinedEvent {
  readonly participantId: string;
  readonly userId: bigint;
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
  readonly videoCodecs?: readonly string[];
  readonly audioCodecs?: readonly string[];
  readonly supportsSimulcast?: boolean;
  readonly maxVideoStreams?: number;
}

/** Parameters for {@link SignalingClient.join}. */
export interface SignalingJoinParams {
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
