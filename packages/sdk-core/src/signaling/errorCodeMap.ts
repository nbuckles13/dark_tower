// File: packages/sdk-core/src/signaling/errorCodeMap.ts
//
// R-18: the ONE oracle mapping the proto `ErrorCode` (wire enum) → the stable
// SDK-side `SignalingErrorCode`. Kept exhaustive over the proto enum so adding a
// new `ErrorCode` is a compile error here (the `Record<ErrorCode, …>` must list
// every member) AND forces the parametrized mapping test to update.
//
// Auth-class (`UNAUTHORIZED`/`FORBIDDEN`) is identified here too: those close the
// signaling connection with a typed reason (per R-18). The classification is a
// pure data table — no transport / token concerns leak in.

import { ErrorCode } from '../proto/dark_tower/signaling/v1/signaling_pb.js';
import { SignalingErrorCode } from '../errors/SignalingError.js';

/**
 * Exhaustive proto `ErrorCode` → {@link SignalingErrorCode} table. Exhaustive by
 * construction (`Record<ErrorCode, …>`): a new proto `ErrorCode` won't compile
 * until mapped.
 */
const ERROR_CODE_TO_SIGNALING: Record<ErrorCode, SignalingErrorCode> = {
  [ErrorCode.UNKNOWN]: SignalingErrorCode.Unknown,
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

/** Auth-class signaling codes that close the connection with a typed reason (R-18). */
const AUTH_CLASS: ReadonlySet<SignalingErrorCode> = new Set([
  SignalingErrorCode.Unauthorized,
  SignalingErrorCode.Forbidden,
]);

/** Whether `code` is auth-class (closes the connection with `CloseReason.AuthFailed`). */
export function isAuthClass(code: SignalingErrorCode): boolean {
  return AUTH_CLASS.has(code);
}
