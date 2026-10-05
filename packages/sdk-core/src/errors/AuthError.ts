// File: packages/sdk-core/src/errors/AuthError.ts
//
// R-11/R-23: typed AC (auth-controller) error hierarchy. `AuthError` extends
// `SdkError` and is subtyped by HTTP status. Mapping is BY HTTP STATUS, read from
// the `{error:{code,message}}` envelope (`crates/ac-service/src/errors.rs`).
//
// LIVE STATUSES: AC emits 400 `INVALID_REQUEST` (registration format validation —
// bad email, short password, empty display name — and admin validation) and 409
// `CONFLICT` (registering an email that already exists in the org), alongside
// 401/403/404/429/500. Both land on their typed subtypes (`AuthBadRequestError` /
// `AuthConflictError`) and carry AC's fixed body message verbatim. (404 surfaces as
// the generic `AuthError` base for AC; the meeting-side 404 is `MeetingNotFoundError`.)
//
// Each subclass overrides `this.name` (per @code-reviewer relay) so a serialized
// error reports its concrete class, not the base name. `serverCode` is carried by
// the base (readonly) and emitted by `toJSON()` (non-secret).

import { SdkError, SdkErrorCode, readServerErrorEnvelope } from './SdkError.js';

/** Base for all AC HTTP failures. */
export class AuthError extends SdkError {
  constructor(message: string, status: number, serverCode?: string) {
    super(SdkErrorCode.Auth, message, {
      status,
      ...(serverCode !== undefined ? { serverCode } : {}),
    });
    this.name = 'AuthError';
    Object.setPrototypeOf(this, AuthError.prototype);
  }

  /**
   * Build the most specific `AuthError` subtype for an HTTP `status` + parsed
   * response `body`. Tolerates a missing/malformed envelope (generic message).
   * `retryAfterSeconds` (parsed from the `Retry-After` header) is applied only to
   * the 429 subtype.
   */
  static fromResponse(status: number, body: unknown, retryAfterSeconds?: number): AuthError {
    const env = readServerErrorEnvelope(body);
    const message = env.message ?? `Authentication request failed (HTTP ${status})`;
    const serverCode = env.code;
    switch (status) {
      case 400:
        return new AuthBadRequestError(message, serverCode);
      case 401:
        return new AuthUnauthorizedError(message, serverCode);
      case 403:
        return new AuthForbiddenError(message, serverCode);
      case 409:
        return new AuthConflictError(message, serverCode);
      case 429:
        return new AuthRateLimitError(message, serverCode, retryAfterSeconds);
      default:
        return new AuthError(message, status, serverCode);
    }
  }
}

/** 400 — malformed request (e.g. registration format validation). */
export class AuthBadRequestError extends AuthError {
  constructor(message: string, serverCode?: string) {
    super(message, 400, serverCode);
    this.name = 'AuthBadRequestError';
    Object.setPrototypeOf(this, AuthBadRequestError.prototype);
  }
}

/** 401 — invalid credentials / invalid token. */
export class AuthUnauthorizedError extends AuthError {
  constructor(message: string, serverCode?: string) {
    super(message, 401, serverCode);
    this.name = 'AuthUnauthorizedError';
    Object.setPrototypeOf(this, AuthUnauthorizedError.prototype);
  }
}

/** 403 — insufficient scope / forbidden. */
export class AuthForbiddenError extends AuthError {
  constructor(message: string, serverCode?: string) {
    super(message, 403, serverCode);
    this.name = 'AuthForbiddenError';
    Object.setPrototypeOf(this, AuthForbiddenError.prototype);
  }
}

/** 409 — conflict, e.g. registering an email that already exists in the org. */
export class AuthConflictError extends AuthError {
  constructor(message: string, serverCode?: string) {
    super(message, 409, serverCode);
    this.name = 'AuthConflictError';
    Object.setPrototypeOf(this, AuthConflictError.prototype);
  }
}

/** 429 — rate limited. Carries `retryAfterSeconds` when the server supplied a hint. */
export class AuthRateLimitError extends AuthError {
  /** Seconds to wait before retrying, from the `Retry-After` header; omitted if absent. */
  readonly retryAfterSeconds?: number;

  constructor(message: string, serverCode?: string, retryAfterSeconds?: number) {
    super(message, 429, serverCode);
    this.name = 'AuthRateLimitError';
    // exactOptionalPropertyTypes: omit rather than assign undefined.
    if (retryAfterSeconds !== undefined) {
      this.retryAfterSeconds = retryAfterSeconds;
    }
    Object.setPrototypeOf(this, AuthRateLimitError.prototype);
  }
}
