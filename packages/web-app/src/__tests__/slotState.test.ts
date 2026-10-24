// File: packages/web-app/src/__tests__/slotState.test.ts
//
// The §6 slot-state mapping. Two properties, both of which exist because
// ADR-0036 §6 names the failure directly: "A naive client shows the same spinner
// for a bandwidth problem, an under-filled grid, and a participant it can never
// see."
//
//   1. EXHAUSTIVE — every wire token has text. The type system already enforces
//      this (`Record<SlotStateToken, …>` keyed off the SDK's union, so a new
//      wire state is a compile error), and this file adds the runtime half: a
//      list of tokens that must stay in step with the union.
//   2. DISTINCT — no two states render the same string, which is the actual
//      requirement. Exhaustiveness alone is satisfied by mapping all of them to
//      "no audio", and that is precisely the defect.

import { expect, test } from 'vitest';
import {
  AWAITING_ASSIGNMENT,
  WIRE_TOKENS,
  slotIsActive,
  slotStateText,
  type SlotStateAttr,
} from '../lib/slotState.js';

const ALL_STATES: readonly SlotStateAttr[] = [...WIRE_TOKENS, AWAITING_ASSIGNMENT];

test('the wire vocabulary is the ten flat states (story 3 wire freeze; MC slot_state_label)', () => {
  // A TRIPWIRE, not the coverage mechanism. `WIRE_TOKENS` is derived from
  // `SLOT_STATE_TEXT`'s keys, and that table is `Record<SlotStateToken, string>`
  // — compiler-forced to cover the SDK union — so a ninth §6 state enters the
  // distinctness and non-empty tests below automatically. This assertion only
  // announces that it happened, so the change is noticed rather than absorbed.
  //
  // The earlier version of this file hand-listed the tokens and claimed the
  // length check caught an unlisted addition. It did not: TypeScript does not
  // require an array to be exhaustive over a union, so a ninth token added to
  // the union and to the table but not to the list compiled clean, dropped out
  // of every test below, and left the length check passing — the expectation
  // was written from belief rather than derived from the artifact.
  expect(WIRE_TOKENS).toHaveLength(10);
  expect(new Set(WIRE_TOKENS).size).toBe(10);
  // The reserved token is not in the vocabulary (the superseded legacy one is
  // pinned unrepresentable by sdk-core's `slotStateMap` tests).
  expect(WIRE_TOKENS).not.toContain('zero_requested' as never);
});

const CTX = { name: 'Bea', kind: 'video' } as const;
const text = (state: SlotStateAttr, kind: 'audio' | 'video' = 'video'): string =>
  slotStateText(state, { ...CTX, kind });

test('every state renders non-empty text, for both kinds', () => {
  for (const state of ALL_STATES) {
    for (const kind of ['audio', 'video'] as const) {
      expect(text(state, kind), `no text for "${state}" (${kind})`).not.toBe('');
    }
  }
});

test('no two states render the same text (per kind)', () => {
  // THE ASSERTION THAT MATTERS. Exhaustiveness is satisfied by mapping all of
  // them to "no video"; distinctness is what §6 actually asks for, because the
  // no-media states have completely different remedies.
  for (const kind of ['audio', 'video'] as const) {
    const texts = ALL_STATES.map((s) => text(s, kind));
    expect(new Set(texts).size).toBe(ALL_STATES.length);
  }
});

test('avatars NAME the participant and give the reason (story 3 R-4)', () => {
  expect(text('avatar_client_muted')).toBe('Bea turned their camera off');
  expect(text('avatar_server_muted')).toBe("Bea's camera was turned off by the host");
  expect(text('avatar_no_video')).toBe('Bea is not sending video');
  expect(text('avatar_client_muted', 'audio')).toBe('Bea muted their microphone');
  expect(text('unavailable')).toBe('this meeting allows fewer video slots');
});

test('the never-emitted states AND unspecified render fail-closed "unexpected" copy', () => {
  for (const state of [
    'unspecified',
    'switch_pending',
    'withheld_by_congestion',
    'source_unreachable',
  ] as const) {
    expect(text(state)).toContain('unexpected state');
    expect(slotIsActive(state)).toBe(false);
  }
});

test('awaiting-assignment is distinct from unspecified', () => {
  // Different facts: "MC has sent nothing yet" (ordinary, between join and the
  // first assignment) versus "MC sent a slot whose state it did not classify" (a
  // protocol-level surprise). Collapsing them hides a real defect inside a
  // normal startup state.
  expect(text(AWAITING_ASSIGNMENT)).not.toBe(text('unspecified'));
});

test('only the active state counts as active', () => {
  for (const state of ALL_STATES) {
    expect(slotIsActive(state), `"${state}" active?`).toBe(state === 'active');
  }
});
