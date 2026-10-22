// File: packages/sdk-core/src/media/lifecycle/receiverState.ts
//
// THE ONE lifecycle binding for a kind's receive-side key state — its
// `TransmitKeyCache` and `ReplayWindow` — shared by the audio pipeline and the
// video receiver (story 3 C-B, DRY P5). Each kind keeps its OWN instances
// (per-kind budgets: video key churn can never evict audio keys, and the key-id
// stream ids are disjoint), but how those instances live and die is written
// once, here:
//
//   * C-3 (story 2 R-16): a KEK generation leaving retention takes its replay
//     scope and its transmit keys with it, TOGETHER, in ONE synchronous
//     listener. Both are DERIVED from the KEK holder, which is authoritative for
//     which generations are live, so they cannot disagree with it or with each
//     other. There is no other timer.
//   * Teardown clears both (ADR-0028 §5's explicit cleanup).
//
// NOT RE-ENTRANT, STRUCTURALLY (@security). The listener body is two
// synchronous map deletions and nothing else: no await, no emit, no metric, no
// call back into an ingress — so although it fires from INSIDE frame
// processing (the holder's lazy expiry runs on the receive path's own KEK
// lookup), it cannot re-enter `openVerifiedFrame`. Registered exactly ONCE per
// kind, by this function. Keep it that way: anything async or anything that
// emits would make the discard interleave with the frame whose lookup
// triggered it.
//
// The rebind PURGE (story 2 R-18: keys only, replay retained) is not here: it
// is driven by the roster, through `session/mediaWiring.ts`, for every kind.

import type { ReplayWindow, TransmitKeyCache } from '../frame/receivePath.js';

/** A kind's receive-side key state. */
export interface ReceiverKeyState {
  readonly cache: TransmitKeyCache;
  readonly replay: ReplayWindow;
}

/**
 * The teardown-name prefix per kind. A closed union rather than `string`: the
 * name is interpolated into a teardown-failure fault (an event-once dedupe
 * key), so it must be a static literal — now by type, not by comment.
 */
export type ReceiverStatePrefix = '' | 'video-';

/** The KEK holder's generation-dropped notification. */
export interface GenerationDroppedSource {
  /** Fires synchronously when a generation leaves the holder. Returns the unsubscribe. */
  onGenerationDropped(listener: (generation: number) => void): () => void;
}

/**
 * Bind `state` to the KEK holder and to its owner's teardown. Call ONCE per
 * kind, BEFORE any frame can reach that kind's ingress, so no scope is created
 * that the listener does not see. `prefix` keeps the two kinds' teardown names
 * distinct (static literals only: a teardown failure interpolates the name).
 */
export function bindReceiverState(
  state: ReceiverKeyState,
  kek: GenerationDroppedSource,
  register: (name: string, dispose: () => void) => void,
  prefix: ReceiverStatePrefix,
): void {
  register(`${prefix}receiver-state`, () => {
    state.cache.clear();
    state.replay.clear();
  });
  const unsubscribe = kek.onGenerationDropped((generation) => {
    state.replay.discardGeneration(generation);
    state.cache.discardGeneration(generation);
  });
  register(`${prefix}kek-generation-dropped-listener`, unsubscribe);
}
