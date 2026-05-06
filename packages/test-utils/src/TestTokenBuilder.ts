// File: packages/test-utils/src/TestTokenBuilder.ts
//
// Builds claim objects matching the wire shape of AC- and MC-issued tokens.
// No cryptography here — pair with `@darktower/test-utils/test-only/signer`'s
// `TestTokenSigner` to produce signed JWTs. Test-only.

import type { MeetingClaims, MeetingRole, ParticipantType, UserClaims } from './token-claims.js';

const DEFAULT_TTL_SECONDS = 3600;
const SAMPLE_USER_UUID = '00000000-0000-4000-8000-000000000001';
const SAMPLE_ORG_UUID = '00000000-0000-4000-8000-00000000000a';
const SAMPLE_MEETING_UUID = '00000000-0000-4000-8000-00000000000b';
const SAMPLE_PARTICIPANT_UUID = '00000000-0000-4000-8000-00000000000c';
const SAMPLE_EMAIL = 'user-test-001@example.test';

/**
 * Optional overrides for `userClaims`. Anything omitted falls back to
 * synthetic defaults (no real PII).
 */
export interface UserClaimsOverrides {
  sub?: string;
  org_id?: string;
  email?: string;
  roles?: readonly string[];
  iat?: number;
  exp?: number;
  ttlSeconds?: number;
  jti?: string;
}

/**
 * Optional overrides for `meetingClaims`. Anything omitted falls back to
 * synthetic defaults.
 */
export interface MeetingClaimsOverrides {
  sub?: string;
  meeting_id?: string;
  home_org_id?: string;
  meeting_org_id?: string;
  participant_type?: ParticipantType;
  role?: MeetingRole;
  capabilities?: readonly string[];
  iat?: number;
  exp?: number;
  ttlSeconds?: number;
  jti?: string;
}

/**
 * Stateless builder for unsigned claim payloads. Pair with
 * `TestTokenSigner.sign(claims)` (sub-path) to produce signed JWTs.
 *
 * @example
 * const claims = TestTokenBuilder.userClaims({ email: 'tester@example.test' });
 * const jwt = await signer.sign(claims);
 */
export const TestTokenBuilder = {
  /**
   * Build a user-token claim object matching `crates/common/src/jwt.rs:UserClaims`
   * wire shape. Defaults produce a 1-hour valid token with synthetic
   * identifiers — no real PII.
   */
  userClaims(overrides: UserClaimsOverrides = {}): UserClaims {
    const now = nowSeconds();
    const ttl = overrides.ttlSeconds ?? DEFAULT_TTL_SECONDS;
    return {
      sub: overrides.sub ?? SAMPLE_USER_UUID,
      org_id: overrides.org_id ?? SAMPLE_ORG_UUID,
      email: overrides.email ?? SAMPLE_EMAIL,
      roles: overrides.roles ?? ['user'],
      iat: overrides.iat ?? now,
      exp: overrides.exp ?? now + ttl,
      jti: overrides.jti ?? SAMPLE_PARTICIPANT_UUID,
    };
  },

  /**
   * Build a meeting-token claim object matching
   * `crates/common/src/jwt.rs:MeetingTokenClaims` wire shape. Defaults
   * produce a 15-minute participant token. `home_org_id` is omitted unless
   * explicitly supplied (matches Rust `Option<String>` skip-if-none).
   */
  meetingClaims(overrides: MeetingClaimsOverrides = {}): MeetingClaims {
    const now = nowSeconds();
    const ttl = overrides.ttlSeconds ?? 900;
    const result: MeetingClaims = {
      sub: overrides.sub ?? SAMPLE_PARTICIPANT_UUID,
      token_type: 'meeting',
      meeting_id: overrides.meeting_id ?? SAMPLE_MEETING_UUID,
      meeting_org_id: overrides.meeting_org_id ?? SAMPLE_ORG_UUID,
      participant_type: overrides.participant_type ?? 'member',
      role: overrides.role ?? 'participant',
      capabilities: overrides.capabilities ?? ['video', 'audio'],
      iat: overrides.iat ?? now,
      exp: overrides.exp ?? now + ttl,
      jti: overrides.jti ?? SAMPLE_USER_UUID,
    };
    if (overrides.home_org_id !== undefined) {
      result.home_org_id = overrides.home_org_id;
    }
    return result;
  },
};

function nowSeconds(): number {
  return Math.floor(Date.now() / 1000);
}
