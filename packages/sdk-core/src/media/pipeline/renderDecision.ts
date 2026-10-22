// File: packages/sdk-core/src/media/pipeline/renderDecision.ts
//
// HOT PATH (evaluated per decoded frame). PURE: no state, no logging, no
// metrics.
//
// ---------------------------------------------------------------------------
// WHAT A VIDEO SLOT SHOWS (story 3 R-4, R-5)
// ---------------------------------------------------------------------------
//
// MC's slot state is authoritative for avatar versus video, and MC never names
// the live source: a live slot is attributed to the VERIFIED key-id sender of
// the frame it draws. This module maps MC's flat state token onto a render
// rule, in ONE exhaustive table keyed on `SlotStateToken` (the
// `signaling/slotStateMap.ts` pattern), so a new wire state is a compile error
// rather than a silent blank slot. The avatar reason IS the token.
//
// THE NEVER-RENDER RE-CHECK. `receivePolicy.ts` drops a self or
// server-muted sender's frames at ARRIVAL. A frame decoded BEFORE its sender was
// server-muted can still be held when the slot turns ACTIVE, so every draw
// candidate is re-checked against the same two predicates here, plus a sender
// with no roster participant mapping (security S2: never drawn because the mute
// check could not be evaluated). Part of the one composed policy
// (`receivePolicy.ts:RECEIVE_POLICY_OUTCOMES`).

import type { SlotStateToken } from '../../signaling/events.js';

/** The avatar reasons: exactly the avatar tokens of the wire state. */
export type AvatarSlotState = Extract<SlotStateToken, `avatar_${string}`>;

/** What one video slot shows. Emitted on transitions only, never per frame. */
export type VideoRenderState =
  /** Live video from the verified key-id sender of the drawn frame. */
  | { readonly kind: 'active'; readonly senderId: number }
  /** An avatar for the participant MC names, with MC's reason. */
  | {
      readonly kind: 'avatar';
      readonly participantSenderId: number | undefined;
      readonly reason: AvatarSlotState;
    }
  /** Fewer sources than slots: nothing to show. */
  | { readonly kind: 'fewer_sources' }
  /** Over MC's per-kind cap: permanently unavailable (R-1). */
  | { readonly kind: 'unavailable' }
  /** Fail closed: no assignment yet, or a state this SDK never renders media for. */
  | { readonly kind: 'none' };

/** How a slot treats decoded video under one MC state. */
export type RenderRule =
  /** ACTIVE: draw eligible decoded frames; until the first, keep showing what it showed. */
  | 'live'
  /** An avatar: switch at once, ignore trailing frames. */
  | 'avatar'
  | 'fewer_sources'
  | 'unavailable'
  /** Fail closed: no media is drawn. */
  | 'none';

/**
 * MC state token -> render rule. EXHAUSTIVE: a `Record` over the token union,
 * so a new token fails to compile until it is classified. Only `active` is
 * live; the never-emitted states fail closed.
 */
export const RENDER_RULES: Readonly<Record<SlotStateToken, RenderRule>> = {
  active: 'live',
  avatar_no_video: 'avatar',
  avatar_client_muted: 'avatar',
  avatar_server_muted: 'avatar',
  fewer_sources_than_slots: 'fewer_sources',
  unavailable: 'unavailable',
  unspecified: 'none',
  switch_pending: 'none',
  withheld_by_congestion: 'none',
  source_unreachable: 'none',
};

/** Whether `token` is an avatar token (narrows to {@link AvatarSlotState}). */
export function isAvatarState(token: SlotStateToken): token is AvatarSlotState {
  return RENDER_RULES[token] === 'avatar';
}

/**
 * The render state for a NON-live MC state. `undefined` for `active`, whose
 * state depends on what has been drawn (see `videoSlotPipeline.ts`).
 */
export function staticRenderState(
  token: SlotStateToken | undefined,
  participantSenderId: number | undefined,
): VideoRenderState | undefined {
  if (token === undefined) return { kind: 'none' };
  switch (RENDER_RULES[token]) {
    case 'live':
      return undefined;
    case 'avatar':
      return isAvatarState(token)
        ? { kind: 'avatar', participantSenderId, reason: token }
        : { kind: 'none' };
    case 'fewer_sources':
      return { kind: 'fewer_sources' };
    case 'unavailable':
      return { kind: 'unavailable' };
    case 'none':
      return { kind: 'none' };
  }
}

/** The sender predicates the never-render re-check reads. */
export interface RenderGuards {
  /** This client's own sender id. */
  readonly selfSenderId: number;
  /**
   * The verified sender's video server-mute state per the latest
   * `ParticipantMuteUpdate`; `unmapped` when no roster participant maps to it.
   */
  videoMuteState(senderId: number): 'muted' | 'unmuted' | 'unmapped';
}

/** What the ONE sender classifier says about a VERIFIED video sender. */
export type SenderClass = 'self' | 'muted' | 'unmapped' | 'ok';

/**
 * THE ONE sender classifier for video (DRY F5; security ruling), applied to the
 * VERIFIED key-id sender at two sites with a deliberate ASYMMETRY:
 *
 *   * ARRIVAL (`receivePolicy.ts`, after verify, before decrypt): `self` drops
 *     `self_sender`; `muted` drops `sender_server_muted` (A5's MH-failure
 *     signal); `unmapped` and `ok` PASS. `unmapped` must never become
 *     `sender_server_muted` — that would pollute A5 — and it is unreachable by
 *     construction (a sender whose frame verifies has a roster entry, mapped by
 *     the same `RosterKeyFeed.joined()` write).
 *   * RENDER (`mayRender`, every draw path incl. a re-attached sink): ONLY `ok`
 *     is drawn. Everything else fails closed (closed, counted post-accept),
 *     including `unmapped` — never rendered because the mute check could not
 *     be evaluated (security S2).
 */
export function classifySender(senderId: number, guards: RenderGuards): SenderClass {
  // Against this client's REAL id — never 0 or an absent id (security 8).
  if (senderId === guards.selfSenderId) return 'self';
  const mute = guards.videoMuteState(senderId);
  if (mute === 'muted') return 'muted';
  if (mute === 'unmapped') return 'unmapped';
  return 'ok';
}

/** Whether a decoded frame from verified `senderId` may be drawn: the RENDER mapping. */
export function mayRender(senderId: number, guards: RenderGuards): boolean {
  return classifySender(senderId, guards) === 'ok';
}

/** Whether two render states are the same (for transition-only emission). */
export function sameRenderState(a: VideoRenderState, b: VideoRenderState): boolean {
  if (a.kind !== b.kind) return false;
  if (a.kind === 'active' && b.kind === 'active') return a.senderId === b.senderId;
  if (a.kind === 'avatar' && b.kind === 'avatar') {
    return a.reason === b.reason && a.participantSenderId === b.participantSenderId;
  }
  return true;
}
