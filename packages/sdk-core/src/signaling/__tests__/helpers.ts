// File: packages/sdk-core/src/signaling/__tests__/helpers.ts
//
// Test-local framing/encode helpers (R-41). These build framed `ServerMessage`
// bytes at RUNTIME via the generated protobuf-es types + the reused length-prefix
// codec — NO committed hex fixtures — and decode captured outbound frames back
// into `ClientMessage`s. They live in the sdk-core test tree (NOT test-utils) per
// the cross-boundary plan.

import { create, fromBinary, toBinary } from '@bufbuild/protobuf';

import { FrameDecoder, encodeFrame } from '../../framing/length-prefix.js';
import { HEADER_VERSION } from '../../media/frame/wireConstants.js';
import {
  ClientMessageSchema,
  Codec,
  ErrorCode,
  ErrorMessageSchema,
  JoinResponseSchema,
  LeaveReason,
  EncodingParametersSchema,
  MediaKind,
  MediaServerInfoSchema,
  MeetingKekUpdateSchema,
  ParticipantMuteUpdateSchema,
  UnmuteRequestSchema,
  ParticipantJoinedSchema,
  ParticipantLeftSchema,
  ParticipantSchema,
  SendDirectiveSchema,
  SendStreamSchema,
  SendTargetSchema,
  ServerMessageSchema,
  SlotState,
  StreamAssignmentSchema,
  StreamAssignmentsSchema,
  StreamPublishedSchema,
} from '../../proto/dark_tower/signaling/v1/signaling_pb.js';
import type { ClientMessage } from '../../proto/dark_tower/signaling/v1/signaling_pb.js';

export { ErrorCode, LeaveReason };

/** Wrap any `ServerMessage.message` oneof into a framed (4-byte BE prefixed) payload. */
export function frameServerMessage(message: Parameters<typeof buildServerMessage>[0]): Uint8Array {
  const sm = buildServerMessage(message);
  return encodeFrame(toBinary(ServerMessageSchema, sm));
}

function buildServerMessage(
  message:
    | { case: 'joinResponse'; value: ReturnType<typeof buildJoinResponse> }
    | { case: 'participantJoined'; value: ReturnType<typeof buildParticipantJoined> }
    | { case: 'participantLeft'; value: ReturnType<typeof buildParticipantLeft> }
    | { case: 'error'; value: ReturnType<typeof buildErrorMessage> }
    | { case: 'streamPublished'; value: ReturnType<typeof buildStreamPublished> }
    | { case: 'sendDirective'; value: ReturnType<typeof buildSendDirective> }
    | { case: 'streamAssignments'; value: ReturnType<typeof buildStreamAssignments> }
    | { case: 'meetingKekUpdate'; value: ReturnType<typeof buildMeetingKekUpdate> }
    | { case: 'participantMuteUpdate'; value: ReturnType<typeof buildParticipantMuteUpdate> }
    | { case: 'unmuteRequest'; value: ReturnType<typeof buildUnmuteRequestRelay> },
) {
  return create(ServerMessageSchema, { message });
}

export interface JoinResponseInit {
  participantId?: string;
  senderId?: number;
  kekGeneration?: number;
  participants?: {
    participantId: string;
    name: string;
    senderId?: number;
    identityPublicKey?: Uint8Array;
    /** `Participant.*_client_muted` (`optional bool`); omitted = absent on the wire. */
    audioClientMuted?: boolean;
    videoClientMuted?: boolean;
  }[];
  mediaServers?: string[];
  correlationId?: string;
  bindingToken?: string;
  // Present so a test can construct the leak scenario: the meeting KEK is on the
  // wire but must never be projected into `JoinedEvent` (events.ts guarantee).
  meetingKek?: Uint8Array;
  /** `JoinResponse.max_audio_receive_slots`; omitted = absent on the wire. */
  maxAudioReceiveSlots?: number;
  /** `JoinResponse.max_video_receive_slots`; omitted = absent on the wire. */
  maxVideoReceiveSlots?: number;
}

export function buildJoinResponse(init: JoinResponseInit = {}) {
  return create(JoinResponseSchema, {
    participantId: init.participantId ?? 'participant-self',
    senderId: init.senderId,
    existingParticipants: (init.participants ?? []).map((p) =>
      create(ParticipantSchema, {
        participantId: p.participantId,
        name: p.name,
        ...(p.senderId !== undefined ? { senderId: p.senderId } : {}),
        ...(p.identityPublicKey !== undefined ? { identityPublicKey: p.identityPublicKey } : {}),
        ...clientMuteInit(p),
      }),
    ),
    kekGeneration: init.kekGeneration ?? 0,
    mediaServers: (init.mediaServers ?? []).map((url) =>
      create(MediaServerInfoSchema, { mediaHandlerUrl: url }),
    ),
    correlationId: init.correlationId ?? '',
    bindingToken: init.bindingToken ?? '',
    meetingKek: init.meetingKek ?? new Uint8Array(),
    ...(init.maxAudioReceiveSlots !== undefined
      ? { maxAudioReceiveSlots: init.maxAudioReceiveSlots }
      : {}),
    ...(init.maxVideoReceiveSlots !== undefined
      ? { maxVideoReceiveSlots: init.maxVideoReceiveSlots }
      : {}),
  });
}

/** The wire's client-mute pair; an omitted kind stays ABSENT (never `false`). */
interface ClientMuteInit {
  audioClientMuted?: boolean;
  videoClientMuted?: boolean;
}

function clientMuteInit(init: ClientMuteInit): ClientMuteInit {
  return {
    ...(init.audioClientMuted !== undefined ? { audioClientMuted: init.audioClientMuted } : {}),
    ...(init.videoClientMuted !== undefined ? { videoClientMuted: init.videoClientMuted } : {}),
  };
}

export function buildParticipantJoined(
  participantId: string,
  name: string,
  clientMute: ClientMuteInit = {},
) {
  return create(ParticipantJoinedSchema, {
    participant: create(ParticipantSchema, { participantId, name, ...clientMuteInit(clientMute) }),
  });
}

export function buildParticipantLeft(participantId: string, reason: LeaveReason) {
  return create(ParticipantLeftSchema, { participantId, reason });
}

export function buildErrorMessage(code: ErrorCode, message = '') {
  return create(ErrorMessageSchema, { code, message });
}

export function buildStreamPublished(participantId: string) {
  return create(StreamPublishedSchema, { participantId });
}

/** Frame a `JoinResponse` ServerMessage. */
export function framedJoinResponse(init: JoinResponseInit = {}): Uint8Array {
  return frameServerMessage({ case: 'joinResponse', value: buildJoinResponse(init) });
}

/** Frame a `ParticipantJoined` ServerMessage. */
export function framedParticipantJoined(
  participantId: string,
  name: string,
  clientMute: ClientMuteInit = {},
): Uint8Array {
  return frameServerMessage({
    case: 'participantJoined',
    value: buildParticipantJoined(participantId, name, clientMute),
  });
}

/** Frame a `ParticipantJoined` ServerMessage whose `participant` field is unset. */
export function framedParticipantJoinedEmpty(): Uint8Array {
  return frameServerMessage({
    case: 'participantJoined',
    value: create(ParticipantJoinedSchema, {}),
  });
}

/** Frame a `ParticipantLeft` ServerMessage. */
export function framedParticipantLeft(participantId: string, reason: LeaveReason): Uint8Array {
  return frameServerMessage({
    case: 'participantLeft',
    value: buildParticipantLeft(participantId, reason),
  });
}

/** Frame an `ErrorMessage` ServerMessage. */
export function framedError(code: ErrorCode, message = ''): Uint8Array {
  return frameServerMessage({ case: 'error', value: buildErrorMessage(code, message) });
}

/** Frame a `StreamPublished` ServerMessage (an unhandled variant for this story). */
export function framedStreamPublished(participantId = 'p'): Uint8Array {
  return frameServerMessage({
    case: 'streamPublished',
    value: buildStreamPublished(participantId),
  });
}

/** Concatenate byte chunks into one buffer. */
export function concatBytes(chunks: readonly Uint8Array[]): Uint8Array {
  const total = chunks.reduce((n, c) => n + c.byteLength, 0);
  const out = new Uint8Array(total);
  let offset = 0;
  for (const c of chunks) {
    out.set(c, offset);
    offset += c.byteLength;
  }
  return out;
}

/**
 * Decode every outbound `ClientMessage` written to a captured bidi stream by
 * stripping the 4-byte BE length prefix (via the same `FrameDecoder`) and running
 * `fromBinary`.
 */
export function decodeOutboundClientMessages(chunks: readonly Uint8Array[]): ClientMessage[] {
  const decoder = new FrameDecoder();
  const out: ClientMessage[] = [];
  for (const chunk of chunks) {
    for (const frame of decoder.push(chunk)) {
      out.push(fromBinary(ClientMessageSchema, frame));
    }
  }
  return out;
}

/** Poll `predicate` across microtasks/timers until true (or throw on timeout). */
export async function waitFor(predicate: () => boolean, tries = 100): Promise<void> {
  for (let i = 0; i < tries; i++) {
    if (predicate()) return;
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
  throw new Error('waitFor: predicate did not become true in time');
}

// --- ADR-0036 §4/§5/§6 media signaling (story task 19) ---

/** One stream of a fixture `SendDirective`. */
export interface SendStreamInit {
  streamNumber?: number;
  maxBitrateBps?: number;
  targets?: string[];
  mediaKind?: MediaKind;
  /** Video encoding fields (story 3 C-A2); absent = unset on the wire. */
  video?: {
    codec?: Codec;
    width?: number;
    height?: number;
    frameRate?: number;
    minKeyframeIntervalMs?: number;
  };
}

function buildSendStream(init: SendStreamInit) {
  return create(SendStreamSchema, {
    streamNumber: init.streamNumber ?? 1,
    mediaKind: init.mediaKind ?? MediaKind.AUDIO,
    encoding: create(EncodingParametersSchema, {
      maxBitrateBps: init.maxBitrateBps ?? 0,
      ...(init.video
        ? {
            codec: init.video.codec ?? Codec.UNSPECIFIED,
            width: init.video.width ?? 0,
            height: init.video.height ?? 0,
            frameRate: init.video.frameRate ?? 0,
            ...(init.video.minKeyframeIntervalMs !== undefined
              ? { minKeyframeIntervalMs: init.video.minKeyframeIntervalMs }
              : {}),
          }
        : {}),
    }),
    targets: (init.targets ?? []).map((url) => create(SendTargetSchema, { mediaHandlerUrl: url })),
  });
}

/**
 * Build a `SendDirective` naming one stream (default audio) and its targets, or
 * the explicit `streams` list when given.
 */
export function buildSendDirective(
  init: SendStreamInit & { headerVersion?: number; streams?: SendStreamInit[] },
) {
  return create(SendDirectiveSchema, {
    // MC derives it from the current version; so does this fixture.
    headerVersion: init.headerVersion ?? HEADER_VERSION,
    streams: (init.streams ?? [init]).map(buildSendStream),
  });
}

/**
 * A `SendDirective` with NO streams — what MC sends a publisher nobody holds
 * (a solo participant, or one whose last holder left).
 */
export function framedSendDirectiveWithoutStreams(): Uint8Array {
  return frameServerMessage({
    case: 'sendDirective',
    value: create(SendDirectiveSchema, { headerVersion: HEADER_VERSION, streams: [] }),
  });
}

/** Build a `StreamAssignments` placing (or explaining) one slot. */
export function buildStreamAssignments(init: {
  slotId?: number;
  senderId?: number;
  mediaHandlerUrl?: string;
  slotState?: SlotState;
  /** The slot's kind; default audio. */
  mediaKind?: MediaKind;
  /** Extra slots, for the multi-handler cases (ADR-0036 §9). */
  extraSlots?: {
    slotId: number;
    senderId?: number;
    mediaHandlerUrl: string;
    mediaKind?: MediaKind;
    slotState?: SlotState;
  }[];
  /** Roster participants this subscriber shares no connected handler with. */
  unreachableSenderIds?: number[];
}) {
  return create(StreamAssignmentsSchema, {
    assignments: [
      create(StreamAssignmentSchema, {
        slotId: init.slotId ?? 0,
        ...(init.senderId !== undefined ? { senderId: init.senderId } : {}),
        mediaKind: init.mediaKind ?? MediaKind.AUDIO,
        mediaHandlerUrl: init.mediaHandlerUrl ?? '',
        slotState: init.slotState ?? SlotState.ACTIVE,
      }),
      ...(init.extraSlots ?? []).map((slot) =>
        create(StreamAssignmentSchema, {
          slotId: slot.slotId,
          ...(slot.senderId !== undefined ? { senderId: slot.senderId } : {}),
          mediaKind: slot.mediaKind ?? MediaKind.AUDIO,
          mediaHandlerUrl: slot.mediaHandlerUrl,
          slotState: slot.slotState ?? SlotState.ACTIVE,
        }),
      ),
    ],
    ...(init.unreachableSenderIds !== undefined
      ? { unreachableSenderIds: init.unreachableSenderIds }
      : {}),
  });
}

/** Build a `MeetingKekUpdate`. Unused this story; the SCRUB still runs. */
export function buildMeetingKekUpdate(kek: Uint8Array, generation: number) {
  return create(MeetingKekUpdateSchema, { meetingKek: kek, kekGeneration: generation });
}

export function framedSendDirective(init: Parameters<typeof buildSendDirective>[0]): Uint8Array {
  return frameServerMessage({ case: 'sendDirective', value: buildSendDirective(init) });
}

export function framedStreamAssignments(
  init: Parameters<typeof buildStreamAssignments>[0],
): Uint8Array {
  return frameServerMessage({ case: 'streamAssignments', value: buildStreamAssignments(init) });
}

export function framedMeetingKekUpdate(kek: Uint8Array, generation: number): Uint8Array {
  return frameServerMessage({
    case: 'meetingKekUpdate',
    value: buildMeetingKekUpdate(kek, generation),
  });
}

export { MediaKind, SlotState };

export function buildParticipantMuteUpdate(
  init: ClientMuteInit & {
    participantId: string;
    audioServerMuted: boolean;
    videoServerMuted?: boolean;
    serverMutedBy?: string;
  },
) {
  return create(ParticipantMuteUpdateSchema, {
    participantId: init.participantId,
    ...clientMuteInit(init),
    audioServerMuted: init.audioServerMuted,
    videoServerMuted: init.videoServerMuted ?? false,
    serverMutedBy: init.serverMutedBy ?? '',
  });
}

export function framedParticipantMuteUpdate(
  init: Parameters<typeof buildParticipantMuteUpdate>[0],
): Uint8Array {
  return frameServerMessage({
    case: 'participantMuteUpdate',
    value: buildParticipantMuteUpdate(init),
  });
}

/** MC's relay of a participant's unmute request to the host (`participant_id` MC-stamped). */
export function buildUnmuteRequestRelay(participantId: string) {
  return create(UnmuteRequestSchema, { requestAudio: true, participantId });
}

export function framedUnmuteRequestRelay(participantId: string): Uint8Array {
  return frameServerMessage({
    case: 'unmuteRequest',
    value: buildUnmuteRequestRelay(participantId),
  });
}
