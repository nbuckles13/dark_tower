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

import type { StreamAssignmentEvent } from '@darktower/sdk-core';

/** The eight wire tokens, as the SDK maps them from `SlotState`. */
export type SlotStateToken = StreamAssignmentEvent['slotState'];

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

/**
 * User-facing text per wire state.
 *
 * Written as complete statements about the SOURCE rather than as UI adjectives:
 * an operator reading a screenshot needs "the sender is muted" to be
 * distinguishable from "the handler is holding this back", and "Muted" alone is
 * not.
 */
export const SLOT_STATE_TEXT: Record<SlotStateToken, string> = {
  // MC set no state. A protocol-level surprise, so it says so rather than
  // pretending to be a normal empty slot.
  unspecified: 'no state reported for this slot',
  active: 'receiving audio',
  // §5: a muted sender sends NOTHING. This text is rendered from the wire, never
  // from having noticed that frames stopped.
  source_muted: 'the sender is muted',
  // The only genuinely handler-observed state, and transient.
  withheld_congestion: 'held back by the handler (congestion)',
  fewer_sources: 'no one to fill this slot yet',
  zero_requested: 'no audio requested',
  // Structurally persistent, unlike the transient states above — this one does
  // not resolve by waiting, which is why it must not render as a spinner.
  source_unreachable: 'the sender cannot be reached',
  switch_pending: 'switching sources',
};

/**
 * Every wire token, DERIVED from the exhaustive table rather than hand-listed.
 *
 * `SLOT_STATE_TEXT` is `Record<SlotStateToken, string>`, so the compiler forces
 * it to cover the SDK's union. Deriving the list from its keys means a ninth §6
 * state added to the union automatically enters every test that iterates this —
 * whereas a hand-maintained array does NOT: TypeScript does not require an array
 * to be exhaustive over a union, so a missing entry compiles clean and silently
 * drops out of coverage while a length assertion still passes.
 */
export const WIRE_TOKENS = Object.keys(SLOT_STATE_TEXT) as readonly SlotStateToken[];

/** Text for a slot the controller has not assigned yet. */
const AWAITING_ASSIGNMENT_TEXT = 'waiting for the controller to fill this slot';

/** Render one slot state — wire token or the awaiting-assignment case. */
export function slotStateText(state: SlotStateAttr): string {
  return state === AWAITING_ASSIGNMENT ? AWAITING_ASSIGNMENT_TEXT : SLOT_STATE_TEXT[state];
}

/**
 * Whether this state means audio should be flowing.
 *
 * Used ONLY to decide presentation emphasis. It is deliberately not used to
 * decide anything about mute: the local mute indicator comes from the SDK's
 * client-mute state and nothing else.
 */
export function slotIsActive(state: SlotStateAttr): boolean {
  return state === 'active';
}
