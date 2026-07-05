// File: packages/sdk-svelte/src/stores/bindMeetingSession.ts
//
// R-28: the adapter's public helper. `bindMeetingSession(session)` wires a
// `MeetingSession`'s typed events into a fresh reactive {@link MeetingStore} and
// tears the subscription down on component unmount (the leak-prevention
// requirement). Split into a pure `subscribeSession` (the single event→state
// SPOT, testable with no component lifecycle) + the lifecycle-bound wrapper.

import { onDestroy } from 'svelte';
import type { MeetingSessionEventMap, MeetingSessionState } from '@darktower/sdk-core';
import { MeetingStore } from './MeetingStore.svelte.js';

/**
 * The minimal structural surface {@link bindMeetingSession} consumes. The
 * concrete `MeetingSession` satisfies this unchanged (it exposes a `state`
 * getter and the `TypedEventEmitter` `on`), so binding never couples to the
 * heavy facade — component tests inject a lightweight emitter mock instead.
 */
export interface BoundMeetingSession {
  /** The current lifecycle state (seeds the store before the first event). */
  readonly state: MeetingSessionState;
  /** Subscribe to a typed event; returns an unsubscribe closure. */
  on<K extends keyof MeetingSessionEventMap>(
    type: K,
    listener: (payload: MeetingSessionEventMap[K]) => void,
  ): () => void;
}

/**
 * Wire every `MeetingSession` event into `store`. PURE: no Svelte lifecycle.
 * Seeds `store` with the session's current `state`, then subscribes. Returns an
 * aggregate unsubscribe that removes ALL listeners (collected from each `.on()`
 * return closure — no hand-rolled `.off` registry).
 */
export function subscribeSession(store: MeetingStore, session: BoundMeetingSession): () => void {
  store.setState(session.state);
  const unsubscribes: Array<() => void> = [
    session.on('stateChange', (state) => store.setState(state)),
    session.on('joined', (event) => store.applyJoined(event)),
    session.on('participantJoined', (event) => store.applyParticipantJoined(event)),
    session.on('participantLeft', (event) => store.applyParticipantLeft(event)),
    session.on('mediaConnected', (url) => store.applyMediaConnected(url)),
    session.on('error', (error) => store.applyError(error)),
  ];
  return () => {
    for (const unsubscribe of unsubscribes) unsubscribe();
  };
}

/**
 * Bind `session` to a fresh {@link MeetingStore} and register cleanup via Svelte
 * `onDestroy`. MUST be called during component initialization (standard Svelte
 * lifecycle contract). A re-mount / re-bind creates a fresh store + subscription
 * owning its own teardown — no listeners survive unmount (verified by the
 * behavior-based unmount-leak test).
 *
 * @returns the reactive store; read `.meetingState` / `.participants` /
 *   `.mediaConnections` / `.lastError` in your template.
 */
export function bindMeetingSession(session: BoundMeetingSession): MeetingStore {
  const store = new MeetingStore();
  const dispose = subscribeSession(store, session);
  onDestroy(dispose);
  return store;
}
