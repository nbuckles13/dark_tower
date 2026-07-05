// File: packages/sdk-svelte/src/stores/MeetingStore.svelte.ts
//
// R-28: the Svelte 5 reactive container backing the adapter's public stores
// `$meetingState` / `$participants` / `$mediaConnections` / `$lastError`.
//
// Public shape justification (R-28, @code-reviewer note 1): Svelte 5 `$state`
// cannot be exported as a bare re-assignable binding across a module boundary —
// assigning to an imported `$state` binding breaks reactivity, and a literal
// `$`-prefixed property name collides with Svelte's store auto-subscription
// sigil. So the requirement's `$meetingState`/… are realized as `$state` cells
// exposed through GETTERS: reading `store.participants` in a template tracks it
// reactively — that getter IS the `$participants` store (ADR-0028 §6).
//
// This module holds state + typed mutators only. Event→state wiring lives in
// `bindMeetingSession.ts` (the single subscription SPOT). No metric/log/trace
// emission originates here; roster accumulation is NEW (sdk-core keeps no live
// roster — it only emits events), not a duplication of any sdk-core surface.

import type {
  JoinedEvent,
  ParticipantJoinedEvent,
  ParticipantLeftEvent,
  RosterParticipant,
  SdkError,
} from '@darktower/sdk-core';
import { MeetingSessionState } from '@darktower/sdk-core';

/**
 * Reactive store mirroring the high-level `MeetingSession` state. Constructed
 * fresh per {@link bindMeetingSession} call; the mutators are driven by
 * {@link subscribeSession} from the SDK's typed events.
 */
export class MeetingStore {
  /** `$meetingState` — the current session lifecycle state. */
  #meetingState = $state<MeetingSessionState>(MeetingSessionState.Idle);
  /** `$participants` — live roster (seeded from `joined`, mutated on join/left). */
  #participants = $state<readonly RosterParticipant[]>([]);
  /** `$mediaConnections` — MH URLs whose handshake has succeeded (dedup'd). */
  #mediaConnections = $state<readonly string[]>([]);
  /** `$lastError` — the most recent terminal/post-join error (typed, redactable). */
  #lastError = $state<SdkError | undefined>(undefined);

  /** Reactive getter for `$meetingState`. */
  get meetingState(): MeetingSessionState {
    return this.#meetingState;
  }

  /** Reactive getter for `$participants` (readonly view). */
  get participants(): readonly RosterParticipant[] {
    return this.#participants;
  }

  /** Reactive getter for `$mediaConnections` (connected MH URLs, readonly view). */
  get mediaConnections(): readonly string[] {
    return this.#mediaConnections;
  }

  /** Reactive getter for `$lastError`. */
  get lastError(): SdkError | undefined {
    return this.#lastError;
  }

  /** Apply a state-machine transition (`stateChange` event). */
  setState(state: MeetingSessionState): void {
    this.#meetingState = state;
  }

  /** Seed the roster from the `joined` event's `existingParticipants`. */
  applyJoined(event: JoinedEvent): void {
    this.#participants = [...event.existingParticipants];
  }

  /** Append a participant (`participantJoined`); idempotent on participantId. */
  applyParticipantJoined(event: ParticipantJoinedEvent): void {
    const id = event.participant.participantId;
    if (this.#participants.some((p) => p.participantId === id)) return;
    this.#participants = [...this.#participants, event.participant];
  }

  /** Remove a participant by id (`participantLeft`). */
  applyParticipantLeft(event: ParticipantLeftEvent): void {
    this.#participants = this.#participants.filter((p) => p.participantId !== event.participantId);
  }

  /** Record a connected MH URL (`mediaConnected`); dedup'd. */
  applyMediaConnected(url: string): void {
    if (this.#mediaConnections.includes(url)) return;
    this.#mediaConnections = [...this.#mediaConnections, url];
  }

  /** Store the latest error (`error`). Held as the typed {@link SdkError}. */
  applyError(error: SdkError): void {
    this.#lastError = error;
  }
}
