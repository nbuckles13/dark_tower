// File: packages/web-app/src/lib/errorText.ts
//
// R-23: render a BOUNDED, non-secret string for a caught error. Typed SDK errors
// expose an SDK-authored `code` + bounded `message` (never a token / raw body);
// anything else collapses to a generic string. Never `String(err)` / `err.stack`.

import { MeetingUnauthorizedError, SdkError } from '@darktower/sdk-core';

/** A safe, bounded display string for any caught error. */
export function errorText(err: unknown): string {
  if (err instanceof SdkError) {
    return `${err.code}: ${err.message}`;
  }
  return 'Unexpected error';
}

/**
 * True when `err` proves the retained user token is no longer usable — a **401** from
 * any `userToken`-bearing call.
 *
 * This is the client's session-invalidation POLICY: which server responses prove a
 * credential is dead. Getting it wrong in either direction is user-visible.
 *
 * **401 only — deliberately NOT 403** (@paired-client, Gate 3). Every 403 these calls
 * can produce is an authorization decision on a **valid, live** token: external
 * participants not allowed, insufficient permissions to create, org meeting limit
 * exceeded. GC's contract states it directly — 401 = "Invalid or missing token",
 * 403 = "User not allowed to join" — and GC's auth middleware documents 401 only for
 * missing/invalid tokens. Treating 403 as credential death would silently sign out a
 * user who hit the org meeting limit, discard a perfectly good session, and drop them
 * on Sign in with no explanation (the error text is destroyed when the view unmounts),
 * where they would re-authenticate and hit the same limit — a loop with no cause shown.
 *
 * Credential-scoped rather than view-scoped (task #58 F-SEC-3): before the token path,
 * an invalid token self-healed because join re-authenticated every time. Now the
 * retained token is the sole join credential AND the auth nav is hidden once a session
 * exists, so without this the user holds a known-dead bearer token with no affordance
 * to replace it and no exit but a page reload. Any call that discovers the credential
 * is dead should drop it — but only a call that actually PROVES it dead.
 */
export function isSessionRejection(err: unknown): boolean {
  return err instanceof MeetingUnauthorizedError;
}
