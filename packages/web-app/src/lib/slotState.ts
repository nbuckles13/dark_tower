// File: packages/web-app/src/lib/slotState.ts
//
// The single place this demo turns an ADR-0036 §6 slot state into something a
// person reads.
//
// ---------------------------------------------------------------------------
// WHY THIS FILE EXISTS AT ALL
// ---------------------------------------------------------------------------
//
// §6: "Slot state is explicit on the wire. Absence of frames is not a signal."
// And the reason it says so: *withheld by congestion*, *fewer sources* and
// *source unreachable* are indistinguishable to a client — all present as no
// media — and render completely differently. "A naive client shows the same
// spinner for a bandwidth problem, an under-filled grid, and a participant it
// can never see."
//
// So the mapping is a table rather than an `if` ladder with a fallback, and the
// table is EXHAUSTIVE by type: `SlotStateToken` is keyed off the SDK's own union,
// so adding a wire state to `StreamAssignmentEvent['slotState']` is a COMPILE
// ERROR here rather than a slot that silently renders blank. A `default:` arm or
// an index signature would defeat that, which is why neither appears.
//
// The raw wire token is also what goes into the DOM's `data-slot-state`, so a
// test asserts on the protocol's vocabulary rather than on display copy that a
// copy edit can change.

import type {
  AvatarSlotState as SdkAvatarSlotState,
  PublishKind,
  SlotStateToken as SdkSlotStateToken,
  VideoRenderState,
} from '@darktower/sdk-core';

/** The wire tokens, as the SDK maps them from `SlotState` (flat vocabulary, story 3). */
export type SlotStateToken = SdkSlotStateToken;

/** The avatar reasons — DERIVED from the SDK (`Extract<…, \`avatar_${string}\`>`), never hand-listed. */
export type AvatarSlotState = SdkAvatarSlotState;

/**
 * The state of a slot the client declared but for which MC has sent no
 * assignment yet.
 *
 * **Deliberately not one of the wire tokens, and deliberately not folded into
 * `unspecified`.** They are different facts: `unspecified` is MC telling us it
 * has a slot whose state it did not classify, which is a protocol-level
 * surprise worth seeing; this one is "no `StreamAssignments` message has
 * arrived", which is the ordinary state between join and the first assignment.
 * Collapsing them would hide a real MC defect inside a normal startup state —
 * the same mistake as reading silence as mute.
 */
export const AWAITING_ASSIGNMENT = 'awaiting-assignment' as const;

/** What a slot row's `data-slot-state` can carry. */
export type SlotStateAttr = SlotStateToken | typeof AWAITING_ASSIGNMENT;

/** What a slot's copy may say about its source: the participant's NAME (already resolved and bounded) and the slot's kind. */
export interface SlotCopyContext {
  /** The participant the slot names, resolved via the roster (`participantState.ts`). */
  readonly name: string;
  /** The slot's kind, from the DECLARED slot (never a frame). */
  readonly kind: PublishKind;
}

/** Fail-closed copy for a state this client never renders media for (story 3 C-C1). */
function unexpected(token: SlotStateToken): string {
  return `unexpected state from the meeting controller (${token}); nothing is shown`;
}

/**
 * User-facing text per wire state — THE one copy table (story 3 C-C1 final
 * copy). EXHAUSTIVE by type: `Record<SlotStateToken, …>` keyed off the SDK's own
 * union, with no default arm and no index signature, so a new wire state is a
 * compile error here rather than a slot that silently renders blank.
 *
 * Every entry is a complete statement about the SOURCE, never a UI adjective:
 * an operator reading a screenshot needs "Bea turned their camera off" to be
 * distinguishable from "the host turned it off" and from "not sending video".
 * The avatar entries NAME the participant MC put there (resolved, bounded,
 * rendered as text only); the others ignore the context.
 */
export const SLOT_STATE_TEXT: Record<SlotStateToken, (ctx: SlotCopyContext) => string> = {
  // Live. Audio names its sender; live video names it from the VERIFIED
  // key-id sender of what is rendered (R-5), resolved by the caller.
  active: ({ name, kind }) => (kind === 'audio' ? `hearing ${name}` : `showing ${name}`),
  // Avatars for a NAMED participant, by reason (story 3 R-4). §5: a muted
  // sender sends NOTHING, and this text is rendered from the wire, never from
  // having noticed frames stop.
  avatar_no_video: ({ name }) => `${name} is not sending video`,
  avatar_client_muted: ({ name, kind }) =>
    kind === 'audio' ? `${name} muted their microphone` : `${name} turned their camera off`,
  avatar_server_muted: ({ name, kind }) =>
    kind === 'audio'
      ? `${name}'s microphone was turned off by the host`
      : `${name}'s camera was turned off by the host`,
  fewer_sources_than_slots: ({ kind }) =>
    kind === 'audio' ? 'no one to hear in this slot yet' : 'no one to show in this slot yet',
  // Over MC's per-kind cap: truncated, and permanent for the declaration (R-1).
  unavailable: ({ kind }) =>
    kind === 'audio'
      ? 'this meeting allows fewer audio slots'
      : 'this meeting allows fewer video slots',
  // MC set no state, or one this build does not know, and the retained but
  // never-emitted states: FAIL CLOSED, no media, and say it is unexpected.
  unspecified: () => unexpected('unspecified'),
  switch_pending: () => unexpected('switch_pending'),
  withheld_by_congestion: () => unexpected('withheld_by_congestion'),
  source_unreachable: () => unexpected('source_unreachable'),
};

/**
 * Every wire token, DERIVED from the exhaustive table rather than hand-listed.
 *
 * `SLOT_STATE_TEXT` is a `Record` over the SDK's union, so the compiler forces
 * it to cover every token. Deriving the list from its keys means a new §6 state
 * automatically enters every test that iterates this — a hand-maintained array
 * would NOT: TypeScript does not require an array to be exhaustive over a union.
 */
export const WIRE_TOKENS = Object.keys(SLOT_STATE_TEXT) as readonly SlotStateToken[];

/** Text for a slot the controller has not assigned yet. */
const AWAITING_ASSIGNMENT_TEXT = 'waiting for the meeting controller to fill this slot';

/** Render one slot state — wire token or the awaiting-assignment case. */
export function slotStateText(state: SlotStateAttr, ctx: SlotCopyContext): string {
  return state === AWAITING_ASSIGNMENT ? AWAITING_ASSIGNMENT_TEXT : SLOT_STATE_TEXT[state](ctx);
}

/**
 * Whether this state means media should be flowing (live). Every other state,
 * including the retained-never-emitted ones and `unspecified`, renders no media
 * (fail closed).
 *
 * Used ONLY to decide presentation emphasis. It is deliberately not used to
 * decide anything about mute: the local mute indicator comes from the SDK's
 * client-mute state and nothing else.
 */
export function slotIsActive(state: SlotStateAttr): boolean {
  return state === 'active';
}

/** What a video slot SHOWS: the DOM's `data-rendered` and the test bus's `rendered`. */
export type RenderedKind = 'video' | 'avatar' | 'empty';

/** SDK render kind -> what is shown. EXHAUSTIVE (a new render kind fails to compile). */
const RENDERED_KIND: Readonly<Record<VideoRenderState['kind'], RenderedKind>> = {
  active: 'video',
  avatar: 'avatar',
  fewer_sources: 'empty',
  unavailable: 'empty',
  none: 'empty',
};

/**
 * THE one mapping from the SDK's render state to what a video slot shows —
 * used by the tile (`VideoSlotTile.svelte`) and the test bus (`e2eBus.ts`), so
 * the DOM and the bus can never disagree. No state yet is `empty`.
 */
export function renderedKind(kind: VideoRenderState['kind'] | undefined): RenderedKind {
  return kind === undefined ? 'empty' : RENDERED_KIND[kind];
}
