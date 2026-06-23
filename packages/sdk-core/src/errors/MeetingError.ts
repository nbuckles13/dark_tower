// File: packages/sdk-core/src/errors/MeetingError.ts
//
// R-12/R-23: typed GC (global-controller) meeting error hierarchy. `MeetingError`
// extends `SdkError`, subtyped by HTTP status, mapped from the
// `{error:{code,message}}` envelope (`crates/gc-service/src/errors.rs`).
//
// GC emits 400/401/403/404/409 (among others) for the meeting endpoints; R-12 asks
// for typed errors across that set. Each subclass overrides `this.name` (per
// @code-reviewer relay).

import { SdkError, SdkErrorCode, readServerErrorEnvelope } from './SdkError.js';

/** Base for all GC meeting HTTP failures. */
export class MeetingError extends SdkError {
  constructor(message: string, status: number, serverCode?: string) {
    super(SdkErrorCode.Meeting, message, {
      status,
      ...(serverCode !== undefined ? { serverCode } : {}),
    });
    this.name = 'MeetingError';
    Object.setPrototypeOf(this, MeetingError.prototype);
  }

  /**
   * Build the most specific `MeetingError` subtype for an HTTP `status` + parsed
   * response `body`. Tolerates a missing/malformed envelope (generic message).
   */
  static fromResponse(status: number, body: unknown): MeetingError {
    const env = readServerErrorEnvelope(body);
    const message = env.message ?? `Meeting request failed (HTTP ${status})`;
    const serverCode = env.code;
    switch (status) {
      case 400:
        return new MeetingBadRequestError(message, serverCode);
      case 401:
        return new MeetingUnauthorizedError(message, serverCode);
      case 403:
        return new MeetingForbiddenError(message, serverCode);
      case 404:
        return new MeetingNotFoundError(message, serverCode);
      case 409:
        return new MeetingConflictError(message, serverCode);
      default:
        return new MeetingError(message, status, serverCode);
    }
  }
}

/** 400 — invalid request body / parameters. */
export class MeetingBadRequestError extends MeetingError {
  constructor(message: string, serverCode?: string) {
    super(message, 400, serverCode);
    this.name = 'MeetingBadRequestError';
    Object.setPrototypeOf(this, MeetingBadRequestError.prototype);
  }
}

/** 401 — invalid / missing user token. */
export class MeetingUnauthorizedError extends MeetingError {
  constructor(message: string, serverCode?: string) {
    super(message, 401, serverCode);
    this.name = 'MeetingUnauthorizedError';
    Object.setPrototypeOf(this, MeetingUnauthorizedError.prototype);
  }
}

/** 403 — not permitted to join / create (e.g. external participant, role). */
export class MeetingForbiddenError extends MeetingError {
  constructor(message: string, serverCode?: string) {
    super(message, 403, serverCode);
    this.name = 'MeetingForbiddenError';
    Object.setPrototypeOf(this, MeetingForbiddenError.prototype);
  }
}

/** 404 — meeting code not found / meeting ended. */
export class MeetingNotFoundError extends MeetingError {
  constructor(message: string, serverCode?: string) {
    super(message, 404, serverCode);
    this.name = 'MeetingNotFoundError';
    Object.setPrototypeOf(this, MeetingNotFoundError.prototype);
  }
}

/** 409 — conflict (e.g. meeting code collision surfaced to the caller). */
export class MeetingConflictError extends MeetingError {
  constructor(message: string, serverCode?: string) {
    super(message, 409, serverCode);
    this.name = 'MeetingConflictError';
    Object.setPrototypeOf(this, MeetingConflictError.prototype);
  }
}
