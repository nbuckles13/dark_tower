// File: packages/test-utils/src/token-claims.ts
//
// Mirrors crates/common/src/jwt.rs:UserClaims (line ~261) and
// :MeetingTokenClaims (line ~356). Update both together if claim shape
// changes. JWT claim names follow JWT/JOSE conventions (snake_case for
// existing fields like `org_id`, `meeting_id`); R-53's camelCase migration
// applies to the AC HTTP API body, NOT to JWT claim names.

/**
 * User token claims — serialized into the JWT payload.
 *
 * `iat`/`exp` are **unix seconds** (NOT millis). The Rust side uses `i64`;
 * JS `number` is safe for unix-second timestamps until ~year 285,000.
 */
export interface UserClaims {
  /** Subject (user UUID). */
  sub: string;
  /** Organization ID the user belongs to. */
  org_id: string;
  /** User's email address. */
  email: string;
  /** User roles (e.g. `["user"]`, `["user","admin"]`). */
  roles: readonly string[];
  /** Issued-at timestamp (unix seconds). */
  iat: number;
  /** Expiration timestamp (unix seconds). */
  exp: number;
  /** Unique token identifier for revocation. */
  jti: string;
}

/** Authenticated participant type for meeting tokens. */
export type ParticipantType = 'member' | 'external';

/** Authenticated participant role for meeting tokens. */
export type MeetingRole = 'host' | 'participant';

/**
 * Meeting token claims — serialized into the JWT payload.
 *
 * `iat`/`exp` are **unix seconds** (see `UserClaims` for rationale).
 */
export interface MeetingClaims {
  /** Subject (participant UUID). */
  sub: string;
  /** Token type discriminator (must be `"meeting"`). */
  token_type: 'meeting';
  /** Meeting UUID. */
  meeting_id: string;
  /** Participant's home organization (omitted for same-org joins). */
  home_org_id?: string;
  /** Meeting's organization UUID. */
  meeting_org_id: string;
  /** Participant type. */
  participant_type: ParticipantType;
  /** Meeting role. */
  role: MeetingRole;
  /** Granted capabilities (e.g. `["video","audio","screen_share"]`). */
  capabilities: readonly string[];
  /** Issued-at timestamp (unix seconds). */
  iat: number;
  /** Expiration timestamp (unix seconds). */
  exp: number;
  /** Unique token identifier for revocation. */
  jti: string;
}
