// File: packages/sdk-core/src/errors/MediaConnectionError.ts
//
// R-20/R-23: typed MH (media-handler) connect failure raised during the
// active/active media handshake. Extends `SdkError`, so it inherits the ONE
// redaction surface (`toJSON`'s fixed allowlist) — the meeting JWT and any token
// NEVER appear on the error. Sited alongside `SignalingError` so the whole
// `errors/` hierarchy shares the same `code` discriminant + `toJSON` redaction idiom.
//
// Two code domains (mirrors SignalingError):
//   - `code = SdkErrorCode.Media` — the coarse hierarchy discriminant.
//   - `mediaCode` — the stable, bounded SDK-side classification callers branch on,
//     and the source of the bounded `failure_code` reported in `MhConnectionStatus`.
//
// REDACTION (R-23): `toJSON` extends the base allowlist with ONLY `mediaCode`
// (bounded SDK enum) and the capped, non-secret `mhUrl` (the server-designated MH
// URL from `JoinResponse.media_servers`, ≤256 bytes). The underlying cause (a
// transport reject / connect timeout) is preserved ONLY on the non-enumerable
// standard `Error.cause` for local debugging — NEVER emitted by `toJSON`, and the
// raw cause text is NEVER stringified into the message or any field (so a transport
// error that echoes written bytes — which include the JWT envelope — cannot leak).

import { SdkError, SdkErrorCode } from './SdkError.js';
import type { RedactedSdkError } from './SdkError.js';

/**
 * Stable, bounded SDK-side media-connect classification. Const-object union
 * (mirroring `SignalingErrorCode` / `SdkErrorCode`) so observability maps each
 * cause to a label without coupling to message strings (ADR-0011). These values
 * are also what populate the bounded `MhConnectionStatus.failure_code` on the wire.
 */
export const MediaConnectionErrorCode = {
  /** A single MH transport failed to connect (ready rejected / stream open / write). */
  Transport: 'TRANSPORT',
  /** A single MH did not become ready within the connect deadline. */
  ConnectTimeout: 'CONNECT_TIMEOUT',
  /** EVERY MH failed — the aggregate `connectAll` rejection. */
  AllFailed: 'ALL_FAILED',
} as const;

export type MediaConnectionErrorCode =
  (typeof MediaConnectionErrorCode)[keyof typeof MediaConnectionErrorCode];

/** Construction options for {@link MediaConnectionError}. */
export interface MediaConnectionErrorOptions {
  /**
   * The failing MH URL (server-designated, from `JoinResponse.media_servers`).
   * Capped to ≤256 bytes by the caller. Omitted for the `ALL_FAILED` aggregate.
   */
  readonly mhUrl?: string;
  /** Underlying cause (transport reject / timeout). Retained on `Error.cause` ONLY. */
  readonly cause?: unknown;
}

/**
 * The fixed, non-secret shape produced by {@link MediaConnectionError.toJSON} — the
 * base allowlist plus the bounded `mediaCode` and the capped non-secret `mhUrl`.
 */
export interface RedactedMediaConnectionError extends RedactedSdkError {
  readonly mediaCode: MediaConnectionErrorCode;
  readonly mhUrl?: string;
}

/** Typed MH media-connect failure. See file header for the R-23 redaction guarantee. */
export class MediaConnectionError extends SdkError {
  /** Stable, bounded SDK-side classification callers branch on. */
  readonly mediaCode: MediaConnectionErrorCode;
  /** The failing MH URL (capped, non-secret); omitted for the `ALL_FAILED` aggregate. */
  readonly mhUrl?: string;

  constructor(
    mediaCode: MediaConnectionErrorCode,
    message: string,
    options: MediaConnectionErrorOptions = {},
  ) {
    super(SdkErrorCode.Media, message);
    this.name = 'MediaConnectionError';
    this.mediaCode = mediaCode;
    // exactOptionalPropertyTypes: OMIT rather than assign `undefined`.
    if (options.mhUrl !== undefined) {
      this.mhUrl = options.mhUrl;
    }
    // Preserve the raw cause on the non-enumerable standard `Error.cause` ONLY (NOT
    // emitted by toJSON) — mirrors `SignalingError`. Keeps the transport reject for
    // local debugging without crossing the redaction line.
    if (options.cause !== undefined) {
      Object.defineProperty(this, 'cause', {
        value: options.cause,
        enumerable: false,
        writable: true,
        configurable: true,
      });
    }
    // Restore prototype chain for `instanceof` across the ES2022 transpile.
    Object.setPrototypeOf(this, MediaConnectionError.prototype);
  }

  /**
   * Serialize to a FIXED allowlist: the base non-secret keys plus `mediaCode` and
   * the capped `mhUrl`. No token / raw cause is ever emitted (R-23).
   */
  override toJSON(): RedactedMediaConnectionError {
    return {
      ...super.toJSON(),
      mediaCode: this.mediaCode,
      ...(this.mhUrl !== undefined ? { mhUrl: this.mhUrl } : {}),
    };
  }
}
