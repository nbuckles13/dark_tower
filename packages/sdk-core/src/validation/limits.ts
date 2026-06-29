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

// ============================================================================
// Non-throwing truncators (cap-and-return; NEVER throw).
// Distinct from the validate-or-throw guards below: these silently bound a
// server-provided value so a too-long field can't crash a status report
// (@dry-reviewer / @code-reviewer D1). Keep the two semantics visibly separate.
// ============================================================================

// ANCHOR (DRY): the MH-status field byte-cap mirrors the MC handler's truncation
// bound — `MhConnectionStatus.{mh_url,failure_reason,failure_code}` are
// client-controlled/untrusted, and MC truncates each at 256 BYTES via
// `floor_char_boundary` before logging (proto/dark_tower/signaling/v1/signaling.proto
// comment on `mh_url`). The SDK caps them at the SAME 256-byte boundary when
// populating the outbound `MediaConnectionUpdate`, so nothing unbounded reaches the
// wire (R-23/R-60). BYTES, not chars — see `capUtf8Bytes`.
export const MAX_MH_URL_BYTES = 256;

const UTF8_ENCODER = /*@__PURE__*/ new TextEncoder();

/**
 * Truncate `value` to at most `maxBytes` UTF-8 bytes WITHOUT splitting a multi-byte
 * codepoint (mirrors Rust `str::floor_char_boundary`). Returns `value` unchanged
 * when it already fits. Unlike the `validate*` helpers this does NOT throw — a
 * too-long server-provided value is silently truncated, never a thrown error that
 * would crash the status report (security/DRY review: truncate, don't reject).
 *
 * Counts BYTES (`TextEncoder`), not `.length` — a single emoji is 1 `.length` unit
 * (or 2 with surrogates) but up to 4 UTF-8 bytes, and MC's bound is byte-based.
 */
export function capUtf8Bytes(value: string, maxBytes: number): string {
  // Fast path: most values are short ASCII well under the cap.
  if (value.length <= maxBytes && UTF8_ENCODER.encode(value).length <= maxBytes) {
    return value;
  }
  // Walk codepoints (for…of iterates by codepoint, never a lone surrogate),
  // accumulating UTF-8 byte length; stop BEFORE the cap would be exceeded.
  let bytes = 0;
  let out = '';
  for (const ch of value) {
    const chBytes = UTF8_ENCODER.encode(ch).length;
    if (bytes + chBytes > maxBytes) break;
    bytes += chBytes;
    out += ch;
  }
  return out;
}

/**
 * Cap an MH-status string field at {@link MAX_MH_URL_BYTES} (256) UTF-8 bytes on a
 * codepoint boundary. Applied to `mh_url` / `failure_reason` / `failure_code` when
 * the SDK builds an outbound `MhConnectionStatus` (R-60 / R-23).
 */
export function capMhUrl(value: string): string {
  return capUtf8Bytes(value, MAX_MH_URL_BYTES);
}

// ============================================================================
// Validate-or-throw input guards (R-31): reject over-long / malformed input
// BEFORE any network call by throwing `ValidationError`. Opposite semantics to
// the non-throwing truncators above — these REJECT, they do not truncate.
// ============================================================================

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
