// File: packages/web-app/e2e/videoPlan.ts
//
// Story 3 C-C1: PURE helpers mapping the oracle's per-kind expectation onto the
// slot ids a receiver declared. No Playwright, no `./env`, so the node tier
// (`tests/videoPlan.test.ts`) self-tests it.
//
// Slot ids are the SDK's declaration order (`session/slotPlan.ts:
// receiveDeclaration`, `ReceiveSlotsDiagnostics`): audio `0..N-1`, then video
// `N..N+M-1`, over the DECLARED counts — truncated slots keep their ids and
// read `unavailable` (R-1).

import type { SlotExpectation } from './slotOracle.js';

/** A receiver's declared per-kind counts. */
export interface DeclaredSlots {
  readonly audioReceiveSlots: number;
  readonly videoReceiveSlots: number;
}

/** The declared VIDEO slot ids, in declaration order. */
export function videoSlotIds(declared: DeclaredSlots): number[] {
  return Array.from(
    { length: declared.videoReceiveSlots },
    (_, i) => declared.audioReceiveSlots + i,
  );
}

/** One video slot id with the oracle's expectation for it. */
export interface VideoSlotPlan {
  readonly slotId: number;
  readonly expectation: SlotExpectation;
}

/**
 * Zip the oracle's VIDEO expectations (declaration order, truncation included)
 * with the declared video slot ids. THROWS on a length mismatch: the oracle and
 * the declaration disagreeing is a harness error, never something to pad.
 */
export function videoSlotPlans(
  declared: DeclaredSlots,
  expected: readonly SlotExpectation[],
): VideoSlotPlan[] {
  const ids = videoSlotIds(declared);
  if (ids.length !== expected.length) {
    throw new Error(
      `HARNESS: the receiver declared ${ids.length} video slots but the oracle expects ` +
        `${expected.length}`,
    );
  }
  return ids.map((slotId, i) => ({ slotId, expectation: expected[i]! }));
}
