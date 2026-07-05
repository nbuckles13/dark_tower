// File: packages/web-app/src/lib/types.ts
//
// Shared demo view types. Credentials live in memory only (R-23) — never
// persisted to storage, never logged.

export type AuthMode = 'login' | 'register';

/** Result of a successful sign-up / sign-in, carried in memory across views. */
export interface AuthResult {
  readonly subdomain: string;
  readonly email: string;
  readonly password: string;
  readonly displayName?: string;
  readonly mode: AuthMode;
  /** User access token (in memory only; used by the create-meeting view). */
  readonly userToken: string;
}

/** The four demo views (R-29). */
export type View = 'signup' | 'signin' | 'create' | 'join';
