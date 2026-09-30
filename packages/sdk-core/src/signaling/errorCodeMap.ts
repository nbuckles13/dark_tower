// File: packages/sdk-core/src/signaling/errorCodeMap.ts
//
// R-18: the ONE oracle mapping the proto `ErrorCode` (wire enum) → the stable
// SDK-side `SignalingErrorCode`. Kept exhaustive over the proto enum so adding a
// new `ErrorCode` is a compile error here (the `Record<ErrorCode, …>` must list
// every member) AND forces the parametrized mapping test to update.
//
// Which codes CLOSE the signaling connection is decided here too, per join phase
// (`closesConnection`): while joining, `UNAUTHORIZED`/`FORBIDDEN` are auth-class
// and close with a typed reason (R-18); once joined, only `UNAUTHORIZED` does. A
// post-join `FORBIDDEN` is a REQUEST refusal — MC's one post-join use of it is the
// server-mute refusal, and MC keeps the connection open — so the client must not
// turn a refused mute into its own disconnect. Pure data; no transport/token
// concerns leak in.

import { ErrorCode } from '../proto/dark_tower/signaling/v1/signaling_pb.js';
import { SignalingErrorCode } from '../errors/SignalingError.js';

/**
 * Exhaustive proto `ErrorCode` → {@link SignalingErrorCode} table. Exhaustive by
 * construction (`Record<ErrorCode, …>`): a new proto `ErrorCode` won't compile
 * until mapped.
 */
const ERROR_CODE_TO_SIGNALING: Record<ErrorCode, SignalingErrorCode> = {
  [ErrorCode.UNSPECIFIED]: SignalingErrorCode.Unknown,
  [ErrorCode.INVALID_REQUEST]: SignalingErrorCode.InvalidRequest,
  [ErrorCode.UNAUTHORIZED]: SignalingErrorCode.Unauthorized,
  [ErrorCode.FORBIDDEN]: SignalingErrorCode.Forbidden,
  [ErrorCode.NOT_FOUND]: SignalingErrorCode.NotFound,
  [ErrorCode.CONFLICT]: SignalingErrorCode.Conflict,
  [ErrorCode.INTERNAL_ERROR]: SignalingErrorCode.InternalError,
  [ErrorCode.CAPACITY_EXCEEDED]: SignalingErrorCode.CapacityExceeded,
  [ErrorCode.STREAM_ERROR]: SignalingErrorCode.StreamError,
};

/** Stable, generic fallback message per signaling code (used when the server omits a message). */
const STATIC_MESSAGE: Record<SignalingErrorCode, string> = {
  [SignalingErrorCode.Unknown]: 'Signaling error',
  [SignalingErrorCode.InvalidRequest]: 'Invalid signaling request',
  [SignalingErrorCode.Unauthorized]: 'Signaling unauthorized',
  [SignalingErrorCode.Forbidden]: 'Signaling forbidden',
  [SignalingErrorCode.NotFound]: 'Not found',
  [SignalingErrorCode.Conflict]: 'Signaling conflict',
  [SignalingErrorCode.InternalError]: 'Internal server error',
  [SignalingErrorCode.CapacityExceeded]: 'Capacity exceeded',
  [SignalingErrorCode.StreamError]: 'Stream error',
  [SignalingErrorCode.Framing]: 'Signaling framing error',
  [SignalingErrorCode.Transport]: 'Signaling transport closed',
  [SignalingErrorCode.Timeout]: 'Join timed out before a JoinResponse was received',
  [SignalingErrorCode.ReceiveSlotsOverCap]:
    'The receive-slot count exceeds the cap the meeting controller allows',
};

/**
 * Map a proto `ErrorCode` to its stable {@link SignalingErrorCode}. Defensive on
 * an out-of-range numeric (a future/garbled wire value): collapses to `Unknown`.
 */
export function mapErrorCode(code: ErrorCode): SignalingErrorCode {
  return ERROR_CODE_TO_SIGNALING[code] ?? SignalingErrorCode.Unknown;
}

/** The stable, generic fallback message for a signaling code. */
export function staticMessageFor(code: SignalingErrorCode): string {
  return STATIC_MESSAGE[code];
}

/** When a signaling error arrives: before the join settled, or after. */
export type SignalingPhase = 'joining' | 'joined';

/** Codes that close the connection with a typed reason (R-18), per phase. */
const CLOSES_CONNECTION: Readonly<Record<SignalingPhase, ReadonlySet<SignalingErrorCode>>> = {
  joining: new Set([SignalingErrorCode.Unauthorized, SignalingErrorCode.Forbidden]),
  // Post-join FORBIDDEN is a request refusal (server-mute, story 2 R-8): surfaced
  // as a session `error`, connection kept. The server can still close it.
  joined: new Set([SignalingErrorCode.Unauthorized]),
};

/**
 * Whether an `ErrorMessage` carrying `code` in `phase` closes the connection with
 * `CloseReason.AuthFailed`. The ONE oracle; call sites never special-case a code.
 */
export function closesConnection(code: SignalingErrorCode, phase: SignalingPhase): boolean {
  return CLOSES_CONNECTION[phase].has(code);
}
