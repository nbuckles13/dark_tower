// File: packages/sdk-core/src/validation/limits.ts
//
// R-31: bounded client-side input validation, performed BEFORE any network call so
// over-long / malformed input never leaves the browser. Violations throw the
// domain-neutral `ValidationError` (`code: VALIDATION`, status 400) — NOT an
// `AuthError` / `MeetingError` — so a meeting-domain caller never sees an auth-domain
// error type leak out of local validation (code-reviewer F1).
//
// R-11 subdomain injection prevention: `validateSubdomain` runs BEFORE the
// subdomain is interpolated into the AC origin URL. The regex is an anchored,
// module-level `const` literal — never `new RegExp(userInput)`.

import { ValidationError } from '../errors/ValidationError.js';

/** Max display name length (R-31). */
export const MAX_DISPLAY_NAME_LENGTH = 64;
/** Max email length, RFC 5321 (R-31). */
export const MAX_EMAIL_LENGTH = 254;
/** Max password length (R-31). */
export const MAX_PASSWORD_LENGTH = 128;

// ANCHOR (DRY): the subdomain rule is the SOURCE-OF-TRUTH mirror of the Rust side.
// AC identifies orgs by subdomain (ADR-0020); the client-side guard MUST match the
// server's accepted shape exactly to prevent subdomain-injection at URL-interpolation
// time (security R-2 review). Authoritative anchor: R-11 in
//   docs/user-stories/2026-05-02-browser-client-join.md
// (DNS-label form: lowercase alphanumeric + internal hyphens, 1..=63 chars). Same
// anchoring convention as `length-prefix.ts` -> MC `MAX_MESSAGE_SIZE`.
const SUBDOMAIN_REGEX = /^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$/;

// ANCHOR (DRY): the meeting-code rule mirrors GC's generator, the source of truth:
//   crates/gc-service/src/handlers/meetings.rs
//     :54  const BASE62_CHARS = b"0..9A..Za..z"   -> base62 alphabet
//     :57  const MEETING_CODE_LENGTH = 12          -> exactly 12 chars
// Validated client-side before the GET so a malformed code never reaches the DB
// lookup (mirrors GC's own server-side pre-lookup validation, R-31).
const MEETING_CODE_REGEX = /^[0-9A-Za-z]{12}$/;

/**
 * Validate a display name (R-31): non-empty after trim, <= 64 chars.
 * @throws {ValidationError} if invalid.
 */
export function validateDisplayName(displayName: string): void {
  if (displayName.trim().length === 0) {
    throw new ValidationError('displayName', 'Display name is required'); // pii-safe: static field-label, not a PII value
  }
  if (displayName.length > MAX_DISPLAY_NAME_LENGTH) {
    throw new ValidationError(
      'displayName',
      `Display name must be at most ${MAX_DISPLAY_NAME_LENGTH} characters`, // pii-safe: static field-label + numeric bound, not a PII value
    );
  }
}

/**
 * Validate an email (R-31): non-empty, <= 254 chars (RFC 5321). Shape validation
 * (presence of `@` etc.) is intentionally deferred to AC — this is a bound, not a
 * format gate.
 * @throws {ValidationError} if invalid.
 */
export function validateEmail(email: string): void {
  if (email.length === 0) {
    throw new ValidationError('email', 'Email is required'); // pii-safe: static field-label, not a PII value
  }
  if (email.length > MAX_EMAIL_LENGTH) {
    throw new ValidationError('email', `Email must be at most ${MAX_EMAIL_LENGTH} characters`); // pii-safe: static field-label + numeric bound, not a PII value
  }
}

/**
 * Validate a password (R-31): non-empty, <= 128 chars. The VALUE is never logged
 * or placed on an error.
 * @throws {ValidationError} if invalid.
 */
export function validatePassword(password: string): void {
  if (password.length === 0) {
    throw new ValidationError('password', 'Password is required');
  }
  if (password.length > MAX_PASSWORD_LENGTH) {
    throw new ValidationError(
      'password',
      `Password must be at most ${MAX_PASSWORD_LENGTH} characters`,
    );
  }
}

/**
 * Validate an org subdomain against the anchored DNS-label regex BEFORE it is
 * interpolated into the AC origin URL (R-11 injection prevention).
 * @throws {ValidationError} if invalid.
 */
export function validateSubdomain(subdomain: string): void {
  if (!SUBDOMAIN_REGEX.test(subdomain)) {
    throw new ValidationError('subdomain', 'Invalid organization subdomain');
  }
}

/**
 * Validate a meeting code against GC's 12-char base62 contract BEFORE the request.
 * @throws {ValidationError} if invalid.
 */
export function validateMeetingCode(code: string): void {
  if (!MEETING_CODE_REGEX.test(code)) {
    throw new ValidationError('meetingCode', 'Invalid meeting code');
  }
}
