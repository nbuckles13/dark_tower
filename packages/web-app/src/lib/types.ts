// File: packages/web-app/src/lib/types.ts
//
// Shared demo view types. The session lives in memory only (R-23) — never persisted
// to storage, never logged.

/**
 * The authenticated session, carried in memory across views.
 *
 * Named for what it *is* (retained state) rather than for the moment that produced
 * it. The predecessor type was `AuthResult`, and that naming is part of why a raw
 * password sat in a long-lived object without anyone blinking: a type named after a
 * transaction does not invite the question "how long does this live?".
 *
 * **`userToken` is a bearer credential**, which is what makes the in-memory-only
 * property load-bearing rather than decorative. Adding a `password` — or any
 * credential-shaped field — to this type is a Layer 3 failure:
 * `dt-guard ts-no-retained-credentials` flags credential fields on retained types,
 * and this type is retained in `App.svelte`.
 *
 * Field set is deliberately minimal (task #58):
 * - `password` removed — the defect. Held past the token exchange that made it
 *   unnecessary, and re-transmitted at join time.
 * - `mode` removed — join is token-based now, so the register-vs-login distinction
 *   has no reader and the AC 409 class it caused is structurally impossible.
 * - `email` removed — its only consumer was the join view's credential assembly,
 *   which no longer exists. Retaining unused identity in a long-lived object is the
 *   same minimisation mistake one field over.
 */
export interface AuthSession {
  readonly subdomain: string;
  /** Roster label. Absent for sign-in: AC's token response carries no identity fields. */
  readonly displayName?: string;
  /** User access token (in memory only; the sole credential the demo presents). */
  readonly userToken: string;
}

/** The four demo views (R-29). */
export type View = 'signup' | 'signin' | 'create' | 'join';
