// File: packages/sdk-core/src/errors/ValidationError.ts
//
// R-31: client-side input-validation failure, raised BEFORE any network call. It is
// domain-neutral — NOT an `AuthError` and NOT a `MeetingError` — so a meeting-domain
// call that fails local validation rejects with `ValidationError`, never leaking an
// auth-domain type into the meeting client's error surface (code-reviewer F1). Carries
// `code: SdkErrorCode.Validation` (the discriminant designed for exactly this case)
// and HTTP-equivalent `status: 400` (the failure a server would have returned for the
// same bad input), but no `serverCode` — nothing came from a server.

import { SdkError, SdkErrorCode } from './SdkError.js';

/** Pre-network input-validation failure (R-31). Domain-neutral; `code: VALIDATION`. */
export class ValidationError extends SdkError {
  /**
   * The input field that failed, as a stable non-secret discriminant
   * (e.g. `displayName`, `email`, `password`, `subdomain`, `meetingCode`). Carries
   * the field name only — never the rejected value.
   */
  readonly field: string;

  constructor(field: string, message: string) {
    super(SdkErrorCode.Validation, message, { status: 400 });
    this.name = 'ValidationError';
    this.field = field;
    Object.setPrototypeOf(this, ValidationError.prototype);
  }
}
