// File: packages/sdk-core/src/errors/NetworkError.ts
//
// R-23/R-41: typed transport-failure error for the HTTP layer — distinct from the
// WebTransport `transport/errors.ts` `TransportError`. Raised when `fetch` itself
// rejects (DNS/TLS/offline) or when a response body is not a usable JSON envelope.
//
// SEMANTIC-GUARD (required, per @semantic-guard relay): `message` is the one
// `toJSON()` allowlist field whose value is not structurally constrained, so it MUST
// be a STATIC string. We NEVER interpolate the raw response body or the underlying
// fetch-reject detail (`cause.message`) into `message` — that could smuggle
// PII/secret material from an upstream into the redaction-safe surface. The raw
// cause is preserved ONLY on the standard non-enumerable `Error.cause` (not emitted
// by `toJSON()`), available for local debugging without crossing the redaction line.

import { SdkError, SdkErrorCode } from './SdkError.js';

/** Stable discriminants for HTTP-layer network failures. */
export const NetworkErrorReason = {
  /** `fetch` rejected before a response was received (DNS / TLS / offline / abort). */
  FetchFailed: 'FETCH_FAILED',
  /** A response arrived but its body was not valid / usable JSON. */
  MalformedResponse: 'MALFORMED_RESPONSE',
} as const;

export type NetworkErrorReason = (typeof NetworkErrorReason)[keyof typeof NetworkErrorReason];

// STATIC messages only — see the file header. Keyed by reason so the public
// message is fixed and audit-reviewable.
const STATIC_MESSAGE: Record<NetworkErrorReason, string> = {
  [NetworkErrorReason.FetchFailed]: 'Network request failed before a response was received',
  [NetworkErrorReason.MalformedResponse]: 'Response was not valid JSON',
};

/** HTTP-layer network failure. Always carries a STATIC `message`. */
export class NetworkError extends SdkError {
  /** The stable network-failure discriminant. */
  readonly reason: NetworkErrorReason;

  constructor(reason: NetworkErrorReason, cause?: unknown) {
    // STATIC message — never interpolate `cause` or response body.
    super(SdkErrorCode.Network, STATIC_MESSAGE[reason], {});
    this.name = 'NetworkError';
    this.reason = reason;
    // Preserve the raw cause on the non-enumerable `Error.cause` ONLY (not emitted
    // by toJSON). Set via defineProperty to keep it non-enumerable.
    if (cause !== undefined) {
      Object.defineProperty(this, 'cause', {
        value: cause,
        enumerable: false,
        writable: true,
        configurable: true,
      });
    }
    Object.setPrototypeOf(this, NetworkError.prototype);
  }
}
