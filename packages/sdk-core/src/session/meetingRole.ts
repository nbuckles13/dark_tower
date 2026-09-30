// File: packages/sdk-core/src/session/meetingRole.ts
//
// Whether THIS participant is the meeting's host, for the UI only (story 2
// R-8/R-11).
//
// ---------------------------------------------------------------------------
// A HINT, NEVER AN AUTHORITY
// ---------------------------------------------------------------------------
//
// GC stamps `role` into the meeting token (`crates/common/src/jwt.rs`
// `MeetingRole`, serde `rename_all = "lowercase"`, so exactly `"host"`), and
// nothing on the MC wire tells a client "you are host". The client reads that
// claim ONLY to decide whether to show the server-mute affordance. MC
// re-derives authority from the token it validated and from the authenticated
// connection on every request, so a wrong answer here shows or hides a button
// and grants nothing. The signature is NOT checked and does not need to be.
//
// ---------------------------------------------------------------------------
// ONLY A BOOLEAN SURVIVES
// ---------------------------------------------------------------------------
//
// The payload also carries `sub` and other claims. The decoded object is a local
// of this function and is dropped on return; only the boolean leaves. Nothing
// here logs, and a failure returns `false` with a bounded `reason` token for the
// caller to WARN with. No token, payload fragment or claim value is ever in it.

import { base64UrlToBytes } from '../encoding/base64url.js';
import { MAX_USER_TOKEN_LENGTH } from '../validation/limits.js';

/**
 * The serde spelling of `MeetingRole::Host` (`crates/common/src/jwt.rs`,
 * `rename_all = "lowercase"`). Compared exactly. Drift-tested against the Rust
 * enum (`__tests__/meetingRole.test.ts`), so a rename cannot silently hide host
 * controls.
 */
export const HOST_ROLE = 'host';

/** Why the role could not be read. Bounded; never carries token material. */
export type MeetingRoleReadFailure =
  'empty' | 'oversized' | 'not_a_jwt' | 'undecodable' | 'no_role_claim';

/** The result of reading the role: a boolean, or not-host with a bounded reason. */
export type MeetingRoleRead =
  | { readonly isHost: boolean }
  | { readonly isHost: false; readonly failure: MeetingRoleReadFailure };

/**
 * Read `role === "host"` from a meeting token's payload segment. Never throws.
 * The token must be the one GC just issued for this join.
 */
export function readIsHost(meetingToken: string): MeetingRoleRead {
  if (meetingToken.length === 0) return { isHost: false, failure: 'empty' };
  if (meetingToken.length > MAX_USER_TOKEN_LENGTH) return { isHost: false, failure: 'oversized' };
  const segments = meetingToken.split('.');
  if (segments.length !== 3) return { isHost: false, failure: 'not_a_jwt' };
  let role: unknown;
  try {
    const payload: unknown = JSON.parse(
      new TextDecoder('utf-8', { fatal: true }).decode(base64UrlToBytes(segments[1] ?? '')),
    );
    role =
      typeof payload === 'object' && payload !== null
        ? (payload as Record<string, unknown>)['role']
        : undefined;
  } catch {
    return { isHost: false, failure: 'undecodable' };
  }
  if (typeof role !== 'string') return { isHost: false, failure: 'no_role_claim' };
  return { isHost: role === HOST_ROLE };
}
