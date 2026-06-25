// File: packages/sdk-core/src/signaling/__tests__/helpers.ts
//
// Test-local framing/encode helpers (R-41). These build framed `ServerMessage`
// bytes at RUNTIME via the generated protobuf-es types + the reused length-prefix
// codec — NO committed hex fixtures — and decode captured outbound frames back
// into `ClientMessage`s. They live in the sdk-core test tree (NOT test-utils) per
// the cross-boundary plan.

import { create, fromBinary, toBinary } from '@bufbuild/protobuf';

import { FrameDecoder, encodeFrame } from '../../framing/length-prefix.js';
import {
  ClientMessageSchema,
  ErrorCode,
  ErrorMessageSchema,
  JoinResponseSchema,
  LeaveReason,
  MediaServerInfoSchema,
  ParticipantJoinedSchema,
  ParticipantLeftSchema,
  ParticipantSchema,
  ServerMessageSchema,
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
    | { case: 'streamPublished'; value: ReturnType<typeof buildStreamPublished> },
) {
  return create(ServerMessageSchema, { message });
}

export interface JoinResponseInit {
  participantId?: string;
  userId?: bigint;
  participants?: { participantId: string; name: string }[];
  mediaServers?: string[];
  correlationId?: string;
  bindingToken?: string;
}

export function buildJoinResponse(init: JoinResponseInit = {}) {
  return create(JoinResponseSchema, {
    participantId: init.participantId ?? 'participant-self',
    userId: init.userId ?? 0n,
    existingParticipants: (init.participants ?? []).map((p) =>
      create(ParticipantSchema, { participantId: p.participantId, name: p.name }),
    ),
    mediaServers: (init.mediaServers ?? []).map((url) =>
      create(MediaServerInfoSchema, { mediaHandlerUrl: url }),
    ),
    correlationId: init.correlationId ?? '',
    bindingToken: init.bindingToken ?? '',
  });
}

export function buildParticipantJoined(participantId: string, name: string) {
  return create(ParticipantJoinedSchema, {
    participant: create(ParticipantSchema, { participantId, name }),
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
export function framedParticipantJoined(participantId: string, name: string): Uint8Array {
  return frameServerMessage({
    case: 'participantJoined',
    value: buildParticipantJoined(participantId, name),
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
