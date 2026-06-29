// File: packages/sdk-core/src/errors/index.ts
//
// INTERNAL re-export hub for the typed SDK error hierarchy. This is NOT the public
// package barrel (that is `src/index.ts`, a closed `exports` map) — it is a
// convenience surface for `src/http/` and tests. Public re-exports are curated in
// `src/index.ts`.

export { SdkError, SdkErrorCode, readServerErrorEnvelope } from './SdkError.js';
export type { RedactedSdkError, SdkErrorOptions, ServerErrorEnvelope } from './SdkError.js';

export {
  AuthError,
  AuthBadRequestError,
  AuthUnauthorizedError,
  AuthForbiddenError,
  AuthConflictError,
  AuthRateLimitError,
} from './AuthError.js';

export {
  MeetingError,
  MeetingBadRequestError,
  MeetingUnauthorizedError,
  MeetingForbiddenError,
  MeetingNotFoundError,
  MeetingConflictError,
} from './MeetingError.js';

export { NetworkError, NetworkErrorReason } from './NetworkError.js';

export { ValidationError } from './ValidationError.js';

export { MediaConnectionError, MediaConnectionErrorCode } from './MediaConnectionError.js';
export type {
  MediaConnectionErrorOptions,
  RedactedMediaConnectionError,
} from './MediaConnectionError.js';
