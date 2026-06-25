// File: packages/sdk-core/src/errors/SdkError.ts
//
// R-23: base typed SDK error. Carries a stable `code` discriminant (const-object
// union, mirroring `transport/errors.ts` `TransportErrorCode` and the framing
// codec's `FramingErrorCode`), the originating HTTP `status` when applicable, and
// the server-authored `serverCode` (e.g. AC `INVALID_CREDENTIALS`, GC `NOT_FOUND`)
// so callers can branch on it WITHOUT string-matching message text.
//
// TOKEN REDACTION (R-23): `toJSON()` is the ONE serialization surface. It returns
// a FIXED allowlist of non-secret keys — never a token, password, Authorization
// header, or request body. The HTTP clients construct errors ONLY from
// `(status, parsed {error:{code,message}} envelope)`, which by construction carries
// no secret material, so no enumerable secret property is ever placed on an
// instance. `toJSON()` takes precedence for `JSON.stringify`, closing the leak even
// for accidental `JSON.stringify(err)`.
//
// PII-SAFETY of `message` (per @observability): `toJSON().message` is the one
// allowlist field whose VALUE is not structurally constrained. Its PII-safety is a
// TRANSITIVE guarantee — it rests on AC/GC keeping their server-authored reason
// strings generic (an UPSTREAM contract, enforced on the Rust side, not locally).
// The SDK never interpolates request-supplied secrets into `message`.
//
// No metric / trace / log emissions originate from this module — error-typing only.

/**
 * Stable discriminants for SDK errors. Kept as a const-object union so downstream
 * observability maps each cause to a label without coupling to message strings.
 */
export const SdkErrorCode = {
  /** AC (auth-controller) HTTP failure mapped from a `{error:{code,message}}` envelope. */
  Auth: 'AUTH',
  /** GC (global-controller) meeting HTTP failure mapped from the error envelope. */
  Meeting: 'MEETING',
  /** MC (meeting-controller) WebTransport signaling failure (proto `ErrorMessage`, framing, or transport close). */
  Signaling: 'SIGNALING',
  /** Client-side bounded-input / subdomain / meeting-code validation failure (pre-network). */
  Validation: 'VALIDATION',
  /** `fetch` rejected (DNS/TLS/offline) or the response body was not a usable envelope. */
  Network: 'NETWORK',
} as const;

export type SdkErrorCode = (typeof SdkErrorCode)[keyof typeof SdkErrorCode];

/**
 * The fixed, non-secret shape produced by {@link SdkError.toJSON}. This is the
 * ONLY serialization surface for any SDK error — see the file header for the
 * R-23 redaction guarantee.
 */
export interface RedactedSdkError {
  readonly name: string;
  readonly code: SdkErrorCode;
  readonly message: string;
  readonly status?: number;
  readonly serverCode?: string;
}

/** Construction options for {@link SdkError} (and subclasses). */
export interface SdkErrorOptions {
  /** HTTP status code, when the error originated from an HTTP response. */
  readonly status?: number;
  /** Server-authored error code from the `{error:{code,message}}` envelope. */
  readonly serverCode?: string;
}

/**
 * Base class for all typed SDK errors. Carries a stable {@link SdkErrorCode}
 * discriminant plus optional HTTP `status` / `serverCode` context.
 */
export class SdkError extends Error {
  readonly code: SdkErrorCode;
  /** HTTP status when this error came from an HTTP response; omitted otherwise. */
  readonly status?: number;
  /** Server-authored error code (non-secret) from the error envelope; omitted otherwise. */
  readonly serverCode?: string;

  constructor(code: SdkErrorCode, message: string, options: SdkErrorOptions = {}) {
    super(message);
    this.name = 'SdkError';
    this.code = code;
    // exactOptionalPropertyTypes: OMIT optional fields rather than assigning
    // `undefined` (per @code-reviewer relay).
    if (options.status !== undefined) {
      this.status = options.status;
    }
    if (options.serverCode !== undefined) {
      this.serverCode = options.serverCode;
    }
    // Restore prototype chain for `instanceof` across the ES2022 transpile
    // (matches the `TransportError` / `FramingError` idiom).
    Object.setPrototypeOf(this, SdkError.prototype);
  }

  /**
   * Serialize to a FIXED allowlist of non-secret keys. This is the R-23 redaction
   * boundary: no token / password / Authorization / request body is ever emitted.
   * Optional keys are omitted (not set to `undefined`) when absent.
   */
  toJSON(): RedactedSdkError {
    const json: { -readonly [K in keyof RedactedSdkError]: RedactedSdkError[K] } = {
      name: this.name,
      code: this.code,
      message: this.message,
    };
    if (this.status !== undefined) {
      json.status = this.status;
    }
    if (this.serverCode !== undefined) {
      json.serverCode = this.serverCode;
    }
    return json;
  }
}

/**
 * The `{ error: { code, message } }` envelope AC and GC return on failure
 * (`crates/ac-service/src/errors.rs`, `crates/gc-service/src/errors.rs`). The
 * fields are best-effort: a malformed / non-JSON body yields `undefined`s, which
 * the `fromResponse` factories tolerate.
 */
export interface ServerErrorEnvelope {
  readonly code?: string;
  readonly message?: string;
}

/**
 * Extract the inner `{code, message}` from a parsed response body, tolerating a
 * missing / malformed envelope. Never throws.
 */
export function readServerErrorEnvelope(body: unknown): ServerErrorEnvelope {
  if (typeof body === 'object' && body !== null && 'error' in body) {
    const inner = (body as { error: unknown }).error;
    if (typeof inner === 'object' && inner !== null) {
      const e = inner as { code?: unknown; message?: unknown };
      return {
        ...(typeof e.code === 'string' ? { code: e.code } : {}),
        ...(typeof e.message === 'string' ? { message: e.message } : {}),
      };
    }
  }
  return {};
}
