// The three exclusions, on the shapes they were measured against in the real tree.
// Anchored on real call sites rather than synthetic equivalents: a future refactor
// breaks real sites, and these three are what stand between the guard and two
// repo-wide false positives.
import type { LoginParams } from './neg_transient_login_params.js';

// 1. INTERFACE MEMBER — real shape: events.ts `JoinOptions.credentials`.
//    A parameter type, not storage.
export interface JoinOptionsLike {
  readonly credentials: LoginParams;
}

// 2. MULTI-LINE FUNCTION PARAMETER — real shape: MeetingSession.ts:335
//    `options: JoinOptions,`. Lexically indistinguishable from a field declaration.
//    Without paren-depth tracking the guard red-lines MeetingSession.join's own
//    signature: the function this task exists to fix.
export async function joinSignaling(
  signaling: string,
  options: JoinOptionsLike,
): Promise<string> {
  return signaling + String(options);
}
