// File: packages/sdk-core/src/errors/SignalingError.ts
//
// R-18/R-23: typed MC (meeting-controller) signaling error. `SignalingError`
// extends `SdkError`, so it inherits the ONE redaction surface (`toJSON`'s fixed
// allowlist) — the meeting/join JWT and any token NEVER appear on the error. It
// is sited here alongside `AuthError`/`MeetingError`/`NetworkError` so the whole
// `errors/` hierarchy shares the same `code` discriminant + `toJSON` redaction
// idiom (siting decision, Gate-1 approved).
//
// Two code domains live on the error:
//   - `code = SdkErrorCode.Signaling` — the coarse hierarchy discriminant.
//   - `signalingCode` — the stable, bounded SDK-side classification mapped from
//     the proto `ErrorCode` (errorCodeMap.ts), plus the local `Framing` /
//     `Transport` causes that have no proto equivalent. This is what callers
//     branch on.
// The proto `ErrorCode` NAME (e.g. "UNAUTHORIZED") is carried in the inherited
// `serverCode` (non-secret, bounded).
//
// REDACTION (R-23): `toJSON` extends the base allowlist with ONLY `signalingCode`
// (bounded SDK enum) and the numeric `closeCode` (a WebTransport/application close
// code — a small integer, non-PII). The free-form server `reason` string is NEVER
// carried as a label/field, and the proto `ErrorMessage.details` map is dropped
// entirely upstream (never reaches this class). The underlying cause (a
// `FramingError` / transport reject) is preserved ONLY on the non-enumerable
// standard `Error.cause` for local debugging — it is NOT emitted by `toJSON`.

import { SdkError, SdkErrorCode } from './SdkError.js';
import type { RedactedSdkError } from './SdkError.js';

/**
 * Stable, bounded SDK-side signaling error classification. The first nine map
 * 1:1 from the proto `ErrorCode` (see `signaling/errorCodeMap.ts`); `Framing` and
 * `Transport` cover local failures with no proto equivalent. Kept as a
 * const-object union (mirroring `SdkErrorCode` / `FramingErrorCode`) so
 * observability maps each cause to a label without coupling to message strings.
 */
export const SignalingErrorCode = {
  /** Proto `UNKNOWN` — unclassified server error. */
  Unknown: 'UNKNOWN',
  /** Proto `INVALID_REQUEST` — malformed/invalid signaling request. */
  InvalidRequest: 'INVALID_REQUEST',
  /** Proto `UNAUTHORIZED` — bad/expired join token. Auth-class: closes the connection. */
  Unauthorized: 'UNAUTHORIZED',
  /** Proto `FORBIDDEN` — not permitted. Auth-class: closes the connection. */
  Forbidden: 'FORBIDDEN',
  /** Proto `NOT_FOUND` — meeting/participant not found. */
  NotFound: 'NOT_FOUND',
  /** Proto `CONFLICT` — conflicting state. */
  Conflict: 'CONFLICT',
  /** Proto `INTERNAL_ERROR` — server-side internal error. */
  InternalError: 'INTERNAL_ERROR',
  /** Proto `CAPACITY_EXCEEDED` — meeting/server at capacity. */
  CapacityExceeded: 'CAPACITY_EXCEEDED',
  /** Proto `STREAM_ERROR` — stream-level error. */
  StreamError: 'STREAM_ERROR',
  /** LOCAL: length-prefix framing failure (oversize / empty / decode) — wraps `FramingError`. */
  Framing: 'FRAMING',
  /** LOCAL: WebTransport connection failed / closed before or during signaling. */
  Transport: 'TRANSPORT',
  /** LOCAL: the join deadline elapsed without a `JoinResponse` (stalled/wedged MC). */
  Timeout: 'TIMEOUT',
} as const;

export type SignalingErrorCode = (typeof SignalingErrorCode)[keyof typeof SignalingErrorCode];

/** Construction options for {@link SignalingError}. */
export interface SignalingErrorOptions {
  /**
   * Numeric WebTransport/application close code observed on a transport-close /
   * terminal settle. Kept NUMERIC (never a pre-stringified reason) so callers
   * (R-22 `MeetingSession`) can derive the bounded `close_reason` metric label
   * via `normalizeCloseReason(closeCode)`.
   */
  readonly closeCode?: number;
  /** Server-authored error code NAME from the proto `ErrorCode` enum (non-secret). */
  readonly serverCode?: string;
  /** Underlying cause (e.g. `FramingError`, transport reject). Retained on `Error.cause` only. */
  readonly cause?: unknown;
}

/**
 * The fixed, non-secret shape produced by {@link SignalingError.toJSON} — the base
 * allowlist plus the bounded `signalingCode` and the numeric `closeCode`.
 */
export interface RedactedSignalingError extends RedactedSdkError {
  readonly signalingCode: SignalingErrorCode;
  readonly closeCode?: number;
}

/** Typed MC signaling failure. See file header for the R-23 redaction guarantee. */
export class SignalingError extends SdkError {
  /** Stable, bounded SDK-side classification callers branch on. */
  readonly signalingCode: SignalingErrorCode;
  /** Numeric close code when this error came from a transport close; omitted otherwise. */
  readonly closeCode?: number;

  constructor(
    signalingCode: SignalingErrorCode,
    message: string,
    options: SignalingErrorOptions = {},
  ) {
    super(SdkErrorCode.Signaling, message, {
      ...(options.serverCode !== undefined ? { serverCode: options.serverCode } : {}),
    });
    this.name = 'SignalingError';
    this.signalingCode = signalingCode;
    // exactOptionalPropertyTypes: omit rather than assign undefined.
    if (options.closeCode !== undefined) {
      this.closeCode = options.closeCode;
    }
    // Preserve the raw cause on the non-enumerable standard `Error.cause` ONLY
    // (NOT emitted by toJSON) — mirrors `NetworkError`. Keeps the underlying
    // FramingError/transport reject for local debugging without crossing the
    // redaction line.
    if (options.cause !== undefined) {
      Object.defineProperty(this, 'cause', {
        value: options.cause,
        enumerable: false,
        writable: true,
        configurable: true,
      });
    }
    Object.setPrototypeOf(this, SignalingError.prototype);
  }

  /**
   * Serialize to a FIXED allowlist: the base non-secret keys plus `signalingCode`
   * and the numeric `closeCode`. No token / server `reason` string / `details`
   * map / cause is ever emitted (R-23).
   */
  override toJSON(): RedactedSignalingError {
    return {
      ...super.toJSON(),
      signalingCode: this.signalingCode,
      ...(this.closeCode !== undefined ? { closeCode: this.closeCode } : {}),
    };
  }
}
