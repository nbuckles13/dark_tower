// File: packages/sdk-core/src/transport/errors.ts
//
// Typed transport errors with stable discriminants, mirroring the framing
// codec's `FramingError` shape. The `code` is the stable contract for later
// observability instrumentation (R-24/25/26): a `dt_client_*` metric label or
// structured-log field maps directly off `code`, with no string-matching of
// message text and no refactor of the throw site — a pure add.
//
// No metric / trace / log emissions originate from this module (those are
// deferred by design); this is error-typing only.

/**
 * Stable discriminants for transport failures. Kept as a const-object union so
 * downstream observability maps each cause to a label without coupling to error
 * message strings.
 */
export const TransportErrorCode = {
  /** Platform lacks the WebTransport API (unsupported browser / Node w/o polyfill). */
  Unavailable: 'WEBTRANSPORT_UNAVAILABLE',
} as const;

export type TransportErrorCode = (typeof TransportErrorCode)[keyof typeof TransportErrorCode];

/**
 * Typed error for transport-layer failures. Carries a stable {@link
 * TransportErrorCode} discriminant so callers / instrumentation have structured
 * context (not stringly-typed).
 */
export class TransportError extends Error {
  readonly code: TransportErrorCode;

  constructor(code: TransportErrorCode, message: string) {
    super(message);
    this.name = 'TransportError';
    this.code = code;
    // Restore prototype chain for `instanceof` across the ES2022 transpile.
    Object.setPrototypeOf(this, TransportError.prototype);
  }
}
