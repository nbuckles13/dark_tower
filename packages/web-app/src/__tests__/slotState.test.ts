// File: packages/web-app/src/__tests__/slotState.test.ts
//
// The §6 slot-state mapping. Two properties, both of which exist because
// ADR-0036 §6 names the failure directly: "A naive client shows the same spinner
// for a bandwidth problem, an under-filled grid, and a participant it can never
// see."
//
//   1. EXHAUSTIVE — every wire token has text. The type system already enforces
//      this (`Record<SlotStateToken, string>` keyed off the SDK's union, so a new
//      wire state is a compile error), and this file adds the runtime half: a
//      list of tokens that must stay in step with the union.
//   2. DISTINCT — no two states render the same string, which is the actual
//      requirement. Exhaustiveness alone is satisfied by mapping all eight to
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

test('the wire vocabulary is the eight states ADR-0036 §6 enumerates', () => {
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
  // of every test below, and left `toHaveLength(8)` passing — the expectation
  // was written from belief rather than derived from the artifact.
  expect(WIRE_TOKENS).toHaveLength(8);
  expect(new Set(WIRE_TOKENS).size).toBe(8);
});

test('every state renders non-empty text', () => {
  for (const state of ALL_STATES) {
    expect(slotStateText(state), `no text for "${state}"`).not.toBe('');
  }
});

test('no two states render the same text', () => {
  // THE ASSERTION THAT MATTERS. Exhaustiveness is satisfied by mapping all nine
  // to "no audio"; distinctness is what §6 actually asks for, because the three
  // no-media states have completely different remedies.
  const texts = ALL_STATES.map(slotStateText);
  expect(new Set(texts).size).toBe(ALL_STATES.length);
});

test('the three indistinguishable no-media states read differently from each other', () => {
  // Named explicitly rather than left to the distinctness sweep above: these are
  // the exact three §6 calls out, and a reader of this file should see them.
  const congestion = slotStateText('withheld_congestion');
  const fewer = slotStateText('fewer_sources');
  const unreachable = slotStateText('source_unreachable');
  expect(new Set([congestion, fewer, unreachable]).size).toBe(3);
});

test('awaiting-assignment is distinct from unspecified', () => {
  // Different facts: "MC has sent nothing yet" (ordinary, between join and the
  // first assignment) versus "MC sent a slot whose state it did not classify" (a
  // protocol-level surprise). Collapsing them hides a real defect inside a
  // normal startup state.
  expect(slotStateText(AWAITING_ASSIGNMENT)).not.toBe(slotStateText('unspecified'));
});

test('only the active state counts as active', () => {
  for (const state of ALL_STATES) {
    expect(slotIsActive(state), `"${state}" active?`).toBe(state === 'active');
  }
});
